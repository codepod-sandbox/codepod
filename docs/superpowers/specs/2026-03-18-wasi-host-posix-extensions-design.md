# WASI Host POSIX Extensions

Extend codepod's WASI P1 host (`wasi-host.ts`) to implement currently-stubbed syscalls, so more Rust crates compile and run on `wasm32-wasip1` without modification.

## Goal

Make the "compile it and it works" path viable for Rust crates. Today, crates that call `std::thread::sleep()`, use `dup2` semantics, or query clock resolution hit `ENOSYS` stubs and fail at runtime. Implementing these syscalls in the WASI host means any Rust binary targeting `wasm32-wasip1` benefits automatically — no per-crate patching.

## Background

Rust on `wasm32-wasip1` chains: **Rust std -> wasi-libc (statically linked) -> wasi_snapshot_preview1 imports (our WasiHost)**. The host already implements ~30 of ~45 WASI P1 syscalls. The remaining stubs are either safe no-ops or return `ENOSYS`. This work promotes four `ENOSYS` stubs to real implementations.

## Scope

Four syscalls, priority ordered:

### 1. `poll_oneoff` (high priority)

**Why:** `std::thread::sleep()` compiles on `wasm32-wasip1` but calls `poll_oneoff` with a clock subscription. Currently returns `ENOSYS`, causing sleep to fail. Many crates use `sleep` for retry loops, rate limiting, or timeouts.

**WASI spec:** Takes an array of subscriptions, blocks until one fires, writes events back.

**Subscription types:**
- `EVENTTYPE_CLOCK` (type 0) — timer. Has `clock_id`, `timeout` (absolute or relative nanos), `precision`, `flags` (bit 0 = absolute).
- `EVENTTYPE_FD_READ` (type 1) — fd becomes readable.
- `EVENTTYPE_FD_WRITE` (type 2) — fd becomes writable.

**Validation:** If `nsubscriptions == 0`, return `EINVAL` immediately.

**Implementation:**

Clock subscriptions: compute the earliest deadline across all clock subscriptions. Convert relative timeouts to absolute by adding current time. If any deadline is in the past, return immediately with those events fired. Otherwise, compute the wait duration as `Math.min(clockDeadline - now, this.deadlineMs - Date.now())` to ensure we never sleep past the sandbox deadline. Return a `Promise` that resolves after the delay (JSPI suspends the WASM stack, same pattern as `fdReadPipe`). On wake, call `checkDeadline()` — if the sandbox deadline triggered the wake, it throws `WasiExitError(124)` as usual. Otherwise, write the fired clock events.

FD subscriptions: for pipe-backed fds, check readability/writability synchronously via new `hasData()`/`hasCapacity()` getters on the async pipe types. For VFS-backed fds, they're always ready (in-memory). Return immediately with ready events.

Edge case: if no fds are ready and there's no clock subscription, return `EINVAL`. This is a sandbox-specific choice — the WASI spec says to block indefinitely, but in a single-threaded sandbox with no external wake source, blocking forever is a guaranteed hang. `EINVAL` signals to the caller that the poll cannot make progress.

**Subscription memory layout** (48 bytes per subscription):
```
offset  0: u64 userdata
offset  8: u8  type (0=clock, 1=fd_read, 2=fd_write)
offset  9: 7 bytes padding (alignment to 16)
offset 16: union (32 bytes) {
  clock: {
    offset 16: u32 clock_id
    offset 20: u32 padding (alignment for u64)
    offset 24: u64 timeout (nanoseconds)
    offset 32: u64 precision (nanoseconds)
    offset 40: u16 flags (bit 0 = ABSTIME)
    offset 42: 6 bytes padding
  }
  fd_read / fd_write: {
    offset 16: u32 fd
    offset 20: 28 bytes padding
  }
}
```

**Event memory layout** (32 bytes per event):
```
offset  0: u64 userdata (copied from subscription)
offset  8: u16 error (0 = success, or WASI errno)
offset 10: u8  type (echoed from subscription)
offset 11: 5 bytes padding
offset 16: u64 nbytes (for fd events: bytes available; 0 for clock)
offset 24: u16 flags (for fd events: bit 0 = EVENTRWFLAGS_FD_READWRITE_HANGUP)
offset 26: 6 bytes padding
```

**Return type:** `number | Promise<number>` — sync if all events are immediately ready, async (JSPI) if waiting on a clock deadline.

**Tests:**
- `thread::sleep` equivalent: single clock subscription, verify it waits approximately the right duration
- Zero-timeout poll: clock subscription with timeout=0 returns immediately
- FD readiness: fd_read subscription on a pipe with data returns immediately
- Multiple subscriptions: clock + fd_read, verify correct events returned
- Absolute vs relative clock flags
- `nsubscriptions == 0` returns `EINVAL`
- Deadline interaction: `sleep(10)` with 1-second sandbox deadline exits with code 124
- Multiple clock subscriptions with different timeouts: earliest fires first
- fd_read on closed/EOF pipe: returns immediately with hangup flag

### 2. `fd_renumber` (medium priority)

**Why:** This is the WASI equivalent of `dup2(src, dst)`. The `FdTable` already has `dup()`, and the kernel imports already expose `host_dup2`. But the WASI-level syscall is stubbed. Some crates and wasi-libc internal code may use this directly.

**WASI spec:** `fd_renumber(from: fd, to: fd)` — atomically replaces fd `to` with fd `from`. `from` is consumed (as if closed). If `to` was open, it's closed first.

**Implementation:**
1. If `from` or `to` is an ioFd (stdio): return `EBADF` (can't renumber managed I/O fds)
2. If `from` is not open in fdTable and not in dirFds: return `EBADF`
3. If `to` is open in fdTable: close it (flush writes via existing `close()`)
4. If `to` is in dirFds: remove it
5. Move `from`'s entry to `to` in fdTable (or dirFds if `from` was a dirFd), remove `from`
6. If `toFd >= fdTable.nextFd`, update `nextFd = toFd + 1` to prevent future allocation collisions

Note on shared buffers: `FdTable.dup()` shares the buffer reference between original and duplicate. `renumber` moves the entry (same buffer reference), which preserves correct POSIX semantics where renumbered fds share the same underlying file description. This is intentional.

Note: FdTable currently doesn't support assigning a specific fd number. We'll add a `renumber(fromFd: number, toFd: number): void` method that moves the entry and updates `nextFd` if needed.

**Tests:**
- Renumber fd 5 to fd 10: read from fd 10 succeeds, fd 5 is gone
- Renumber to an open fd: old fd is closed/flushed first
- Renumber stdio fds: returns EBADF
- Renumber nonexistent fd: returns EBADF
- Renumber a dirFd: verify directory access works via new fd number
- Renumber where source was dup'd: verify shared buffer not corrupted

### 3. `clock_res_get` (low priority)

**Why:** Returns clock resolution. Some crates query this to decide timing precision. Currently `ENOSYS`.

**WASI spec:** `clock_res_get(clock_id: u32, resolution_ptr: u32) -> errno`. Writes a u64 timestamp (nanoseconds) representing the clock's resolution.

**Implementation:**
- `CLOCK_REALTIME` (0): return 1_000_000 (1ms — `Date.now()` precision)
- `CLOCK_MONOTONIC` (1): return 1_000_000 (1ms — same backing)
- `CLOCK_PROCESS_CPUTIME_ID` (2): return `EINVAL` (not supported)
- `CLOCK_THREAD_CPUTIME_ID` (3): return `EINVAL` (not supported)

**Tests:**
- Realtime returns 1ms resolution
- Monotonic returns 1ms resolution
- CPU clocks return EINVAL

### 4. `path_link` (low priority)

**Why:** Hard links. Infrequently used but some build tools and crate tests call this. Currently `ENOSYS`.

**WASI spec:** `path_link(old_fd, old_flags, old_path, new_fd, new_path) -> errno`. Creates a hard link from old_path to new_path.

**Implementation:** Return `ENOTSUP`. Hard links require inode-level refcounting that the VFS doesn't have (it's tree-based, not inode-table-based). This is an honest "not supported" rather than "not implemented" — it's a deliberate design limitation. Crates that check for hard link support will handle `ENOTSUP` gracefully.

**Rationale for not implementing:** Adding hard links would require restructuring the VFS from a tree of `{name -> content}` to an inode table with a separate directory layer. That's a large refactor with minimal benefit — hard links are rarely needed in sandbox contexts.

**Tests:**
- Returns ENOTSUP (not ENOSYS — signals deliberate unsupported vs unimplemented)

## Changes

### `packages/orchestrator/src/wasi/wasi-host.ts`
- Replace `poll_oneoff: this.stub` with `poll_oneoff: this.pollOneoff`
- Replace `fd_renumber: this.stub` with `fd_renumber: this.fdRenumber`
- Replace `clock_res_get: this.stub` with `clock_res_get: this.clockResGet`
- Replace `path_link: this.stub` with `path_link: this.pathLink`
- Add `pollOneoff()` method (~80 lines): parse subscriptions, handle clock/fd types, return sync or Promise
- Add `fdRenumber()` method (~25 lines): validate fds, close target, move entry, handle dirFds
- Add `clockResGet()` method (~10 lines): switch on clock_id, write resolution
- Add `pathLink()` method (~3 lines): return `ENOTSUP`

### `packages/orchestrator/src/vfs/fd-table.ts`
- Add `renumber(fromFd: number, toFd: number): void` method: moves entry, updates `nextFd` if needed

### `packages/orchestrator/src/vfs/pipe.ts`
- Add `hasData(): boolean` getter to `AsyncPipeReadEnd` (checks `shared.totalBytes > 0`)
- Add `hasCapacity(): boolean` getter to `AsyncPipeWriteEnd` (checks `shared.totalBytes < shared.capacity`)
- These enable synchronous readiness checks for `poll_oneoff` fd subscriptions

### `packages/orchestrator/src/wasi/types.ts`
- Add event type constants: `WASI_EVENTTYPE_CLOCK = 0`, `WASI_EVENTTYPE_FD_READ = 1`, `WASI_EVENTTYPE_FD_WRITE = 2`
- Add clock subscription flag: `WASI_SUBCLOCKFLAGS_SUBSCRIPTION_CLOCK_ABSTIME = 1`
- Add event flag: `WASI_EVENTRWFLAGS_FD_READWRITE_HANGUP = 1`
- Import `WASI_ENOTSUP` (already defined as 58) where needed

### New: `packages/orchestrator/src/__tests__/wasi-syscalls.test.ts`
- Tests via `sandbox.run()` using shell commands:
  - `poll_oneoff`/sleep: `sleep 0.01` completes without error; timing sanity check
  - `fd_renumber`: shell redirection patterns that exercise dup2
  - `path_link`: `ln source dest` (without -s) returns error
- Unit-level tests directly calling WasiHost methods with mock memory for `clock_res_get` and edge cases where shell-level testing is impractical

## What this does NOT include

- **mmap emulation** — deferred to a future phase. The `memmap2` crate stubs at compile-time (`cfg(target_os)`), not at link-time, so even providing the symbol wouldn't help without forking memmap2.
- **Thread support** — `std::thread::spawn()` returns `Err` at runtime on wasip1. This is correct behavior; `wasm32-wasip1-threads` is a separate target.
- **Networking** — socket syscalls remain `ENOSYS`. Codepod has its own networking via host imports, not WASI sockets.
- **Process spawning** — `std::process::Command` returns `Err`. Process spawning goes through codepod's orchestrator, not WASI.

## Estimated size

~170 lines of implementation, ~150 lines of tests. `poll_oneoff` is the bulk of the work.
