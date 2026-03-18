# WASI Host POSIX Extensions — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement four currently-stubbed WASI P1 syscalls (`poll_oneoff`, `fd_renumber`, `clock_res_get`, `path_link`) so more Rust crates run on codepod's `wasm32-wasip1` sandbox without modification. Also migrate the shell's `sleep` builtin from the custom `host_sleep` import to `std::thread::sleep()`, which routes through `poll_oneoff` — eliminating a redundant code path and proving the syscall works end-to-end.

**Architecture:** TypeScript WASI host changes (`wasi-host.ts` + supporting modules), plus a small Rust change to the shell's sleep builtin. The shell-exec binary gets rebuilt to use `poll_oneoff` instead of the custom `host_sleep` import.

**Tech Stack:** TypeScript (Deno runtime), Rust (wasm32-wasip1), WASI Preview 1 spec.

**Spec:** `docs/superpowers/specs/2026-03-18-wasi-host-posix-extensions-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `packages/orchestrator/src/wasi/types.ts` | Modify | Add WASI event type and subscription flag constants |
| `packages/orchestrator/src/vfs/pipe.ts` | Modify | Add `hasData()`/`hasCapacity()` getters to async pipe interfaces |
| `packages/orchestrator/src/vfs/fd-table.ts` | Modify | Add `renumber()` method |
| `packages/orchestrator/src/wasi/wasi-host.ts` | Modify | Implement 4 syscall methods, wire into imports |
| `packages/shell-exec/src/builtins.rs` | Modify | Replace `host.sleep(ms)` with `std::thread::sleep()` |
| `packages/shell-exec/src/host.rs` | Modify | Remove `sleep()` from `HostInterface` trait and FFI |
| `packages/orchestrator/src/host-imports/kernel-imports.ts` | Modify | Remove `host_sleep` |
| `packages/orchestrator/src/shell/shell-instance.ts` | Modify | Remove `host_sleep` from WASM imports |
| `packages/orchestrator/src/__tests__/wasi-syscalls.test.ts` | Create | Integration tests |

---

### Task 1: Add WASI constants to `types.ts`

**Files:**
- Modify: `packages/orchestrator/src/wasi/types.ts`

- [ ] **Step 1: Add event type, subscription flag, and event flag constants**

At the end of `packages/orchestrator/src/wasi/types.ts`, after the existing constants, add:

```typescript
// Event types (for poll_oneoff subscriptions and events)
export const WASI_EVENTTYPE_CLOCK = 0;
export const WASI_EVENTTYPE_FD_READ = 1;
export const WASI_EVENTTYPE_FD_WRITE = 2;

// Subscription clock flags
export const WASI_SUBCLOCKFLAGS_SUBSCRIPTION_CLOCK_ABSTIME = 1;

// Event read/write flags
export const WASI_EVENTRWFLAGS_FD_READWRITE_HANGUP = 1;
```

- [ ] **Step 2: Verify no type errors**

Run: `deno check packages/orchestrator/src/wasi/types.ts`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add packages/orchestrator/src/wasi/types.ts
git commit -m "feat(wasi): add poll_oneoff event type and flag constants"
```

---

### Task 2: Add pipe readiness getters

**Files:**
- Modify: `packages/orchestrator/src/vfs/pipe.ts`

The `poll_oneoff` implementation needs to check whether a pipe has data (readable) or capacity (writable) synchronously without suspending. The async pipe interfaces don't expose this today.

- [ ] **Step 1: Add `hasData` getter to `AsyncPipeReadEnd` interface**

In `packages/orchestrator/src/vfs/pipe.ts`, add to the `AsyncPipeReadEnd` interface (after line 111 `readonly closed: boolean;`):

```typescript
  /** Whether the pipe has buffered data available for a non-blocking read. */
  readonly hasData: boolean;
```

- [ ] **Step 2: Add `hasCapacity` getter to `AsyncPipeWriteEnd` interface**

In `packages/orchestrator/src/vfs/pipe.ts`, add to the `AsyncPipeWriteEnd` interface (after line 122 `readonly closed: boolean;`):

```typescript
  /** Whether the pipe has space for a non-blocking write. */
  readonly hasCapacity: boolean;
```

- [ ] **Step 3: Implement `hasData` on the readEnd object**

In the `readEnd` object returned by `createAsyncPipe()` (around line 208), add after the `closed` getter:

```typescript
    get hasData() {
      return shared.totalBytes > 0 || shared.writeClosed;
    },
```

Note: returns `true` when `writeClosed` even if no data, because a read would return 0 (EOF) immediately — the fd is "ready" in poll terms.

- [ ] **Step 4: Implement `hasCapacity` on the writeEnd object**

In the `writeEnd` object returned by `createAsyncPipe()` (around line 257), add after the `closed` getter:

```typescript
    get hasCapacity() {
      return shared.totalBytes < shared.capacity || shared.readClosed;
    },
```

Note: returns `true` when `readClosed` because a write would return -1 (EPIPE) immediately — the fd is "ready" in poll terms.

- [ ] **Step 5: Verify no type errors**

Run: `deno check packages/orchestrator/src/vfs/pipe.ts`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add packages/orchestrator/src/vfs/pipe.ts
git commit -m "feat(pipe): add hasData/hasCapacity getters for poll_oneoff readiness checks"
```

---

### Task 3: Add `renumber()` to FdTable

**Files:**
- Modify: `packages/orchestrator/src/vfs/fd-table.ts`

- [ ] **Step 1: Add the `renumber` method**

In `packages/orchestrator/src/vfs/fd-table.ts`, add after the `dup()` method (after line 190):

```typescript
  /** Move an fd entry from one number to another. Closes toFd if open. */
  renumber(fromFd: number, toFd: number): void {
    const entry = this.entries.get(fromFd);
    if (entry === undefined) {
      throw new Error(`EBADF: bad file descriptor ${fromFd}`);
    }

    // Close target fd if it's open (flushes writes)
    if (this.entries.has(toFd)) {
      this.close(toFd);
    }

    // Move entry
    this.entries.set(toFd, entry);
    this.entries.delete(fromFd);

    // Prevent future open() from reusing toFd
    if (toFd >= this.nextFd) {
      this.nextFd = toFd + 1;
    }
  }
```

- [ ] **Step 2: Verify no type errors**

Run: `deno check packages/orchestrator/src/vfs/fd-table.ts`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add packages/orchestrator/src/vfs/fd-table.ts
git commit -m "feat(fd-table): add renumber() method for fd_renumber syscall"
```

---

### Task 4: Implement all four WASI syscalls

**Files:**
- Modify: `packages/orchestrator/src/wasi/wasi-host.ts`

Implements `clock_res_get`, `path_link`, `fd_renumber`, and `poll_oneoff` in one task. All four replace existing `this.stub` entries in `getImports()`.

- [ ] **Step 1: Add all needed imports from types.ts**

In `packages/orchestrator/src/wasi/wasi-host.ts`, update the import from `'./types.js'` to include:

```typescript
  WASI_ENOTSUP,
  WASI_CLOCK_REALTIME,
  WASI_CLOCK_MONOTONIC,
  WASI_EVENTTYPE_CLOCK,
  WASI_EVENTTYPE_FD_READ,
  WASI_EVENTTYPE_FD_WRITE,
  WASI_SUBCLOCKFLAGS_SUBSCRIPTION_CLOCK_ABSTIME,
  WASI_EVENTRWFLAGS_FD_READWRITE_HANGUP,
```

- [ ] **Step 2: Implement `clockResGet`**

Add near the existing `clockTimeGet` method:

```typescript
  private clockResGet(clockId: number, resPtr: number): number {
    const view = this.getView();
    switch (clockId) {
      case WASI_CLOCK_REALTIME:
      case WASI_CLOCK_MONOTONIC:
        // Date.now() precision is 1ms = 1,000,000 nanoseconds
        view.setBigUint64(resPtr, BigInt(1_000_000), true);
        return WASI_ESUCCESS;
      default:
        return WASI_EINVAL;
    }
  }
```

- [ ] **Step 3: Implement `pathLink`**

```typescript
  private pathLink(): number {
    return WASI_ENOTSUP;
  }
```

- [ ] **Step 4: Implement `fdRenumber`**

```typescript
  private fdRenumber(fromFd: number, toFd: number): number {
    // Cannot renumber stdio/custom I/O fds
    if (this.ioFds.has(fromFd) || this.ioFds.has(toFd)) {
      return WASI_EBADF;
    }

    // Handle dirFd sources
    const fromDirPath = this.dirFds.get(fromFd);
    if (fromDirPath !== undefined) {
      if (this.dirFds.has(toFd)) {
        this.dirFds.delete(toFd);
      }
      if (this.fdTable.isOpen(toFd)) {
        try { this.fdTable.close(toFd); } catch { /* ignore */ }
      }
      this.dirFds.set(toFd, fromDirPath);
      this.dirFds.delete(fromFd);
      return WASI_ESUCCESS;
    }

    // Handle regular fd sources
    if (!this.fdTable.isOpen(fromFd)) {
      return WASI_EBADF;
    }

    try {
      if (this.dirFds.has(toFd)) {
        this.dirFds.delete(toFd);
      }
      this.fdTable.renumber(fromFd, toFd);
      return WASI_ESUCCESS;
    } catch (err) {
      return fdErrorToWasi(err);
    }
  }
```

- [ ] **Step 5: Implement `pollOneoff` and `writePollEvents`**

```typescript
  private pollOneoff(
    inPtr: number,
    outPtr: number,
    nsubscriptions: number,
    neventsPtr: number,
  ): number | Promise<number> {
    this.checkDeadline();

    if (nsubscriptions === 0) {
      return WASI_EINVAL;
    }

    const view = this.getView();
    const events: Array<{
      userdata: bigint;
      error: number;
      type: number;
      nbytes: bigint;
      flags: number;
    }> = [];

    let earliestClockDeadlineMs = Infinity;
    let hasClockSub = false;
    const clockSubs: Array<{ userdata: bigint; deadlineMs: number }> = [];

    // Parse all subscriptions (48 bytes each)
    for (let i = 0; i < nsubscriptions; i++) {
      const base = inPtr + i * 48;
      const userdata = view.getBigUint64(base, true);
      const type = view.getUint8(base + 8);

      if (type === WASI_EVENTTYPE_CLOCK) {
        hasClockSub = true;
        const timeout = view.getBigUint64(base + 24, true);
        const flags = view.getUint16(base + 40, true);
        const isAbsolute = (flags & WASI_SUBCLOCKFLAGS_SUBSCRIPTION_CLOCK_ABSTIME) !== 0;

        let deadlineMs: number;
        if (isAbsolute) {
          deadlineMs = Number(timeout / BigInt(1_000_000));
        } else {
          deadlineMs = Date.now() + Number(timeout / BigInt(1_000_000));
        }

        clockSubs.push({ userdata, deadlineMs });
        if (deadlineMs < earliestClockDeadlineMs) {
          earliestClockDeadlineMs = deadlineMs;
        }
      } else if (type === WASI_EVENTTYPE_FD_READ || type === WASI_EVENTTYPE_FD_WRITE) {
        const fd = view.getUint32(base + 16, true);
        const target = this.ioFds.get(fd);

        let ready = false;
        let hangup = false;
        let nbytes = BigInt(0);

        if (target) {
          if (type === WASI_EVENTTYPE_FD_READ && target.type === 'pipe_read') {
            ready = target.pipe.hasData;
            hangup = target.pipe.closed;
          } else if (type === WASI_EVENTTYPE_FD_WRITE && target.type === 'pipe_write') {
            ready = target.pipe.hasCapacity;
            hangup = target.pipe.closed;
          } else if (target.type === 'static') {
            ready = true;
            nbytes = BigInt(target.data.byteLength - target.offset);
          } else if (target.type === 'null') {
            ready = true;
          } else if (target.type === 'buffer') {
            ready = type === WASI_EVENTTYPE_FD_WRITE;
          }
        } else if (this.fdTable.isOpen(fd)) {
          ready = true; // VFS-backed fds are always ready
        } else {
          events.push({ userdata, error: WASI_EBADF, type, nbytes: BigInt(0), flags: 0 });
          continue;
        }

        if (ready) {
          events.push({
            userdata,
            error: WASI_ESUCCESS,
            type,
            nbytes,
            flags: hangup ? WASI_EVENTRWFLAGS_FD_READWRITE_HANGUP : 0,
          });
        }
      }
    }

    // If any fd events are ready, return immediately
    if (events.length > 0) {
      return this.writePollEvents(outPtr, neventsPtr, events);
    }

    // Wait for earliest clock subscription
    if (hasClockSub) {
      const now = Date.now();

      for (const sub of clockSubs) {
        if (sub.deadlineMs <= now) {
          events.push({
            userdata: sub.userdata,
            error: WASI_ESUCCESS,
            type: WASI_EVENTTYPE_CLOCK,
            nbytes: BigInt(0),
            flags: 0,
          });
        }
      }

      if (events.length > 0) {
        return this.writePollEvents(outPtr, neventsPtr, events);
      }

      // Clamp to sandbox deadline
      const waitMs = Math.max(0, Math.min(
        earliestClockDeadlineMs - now,
        this.deadlineMs - now,
      ));

      return new Promise<number>((resolve) => {
        setTimeout(() => {
          this.checkDeadline();
          const afterWait = Date.now();
          for (const sub of clockSubs) {
            if (sub.deadlineMs <= afterWait) {
              events.push({
                userdata: sub.userdata,
                error: WASI_ESUCCESS,
                type: WASI_EVENTTYPE_CLOCK,
                nbytes: BigInt(0),
                flags: 0,
              });
            }
          }
          if (events.length === 0) {
            events.push({
              userdata: clockSubs[0].userdata,
              error: WASI_ESUCCESS,
              type: WASI_EVENTTYPE_CLOCK,
              nbytes: BigInt(0),
              flags: 0,
            });
          }
          resolve(this.writePollEvents(outPtr, neventsPtr, events));
        }, waitMs);
      });
    }

    return WASI_EINVAL;
  }

  /** Write poll events to WASM memory and return ESUCCESS. */
  private writePollEvents(
    outPtr: number,
    neventsPtr: number,
    events: Array<{
      userdata: bigint;
      error: number;
      type: number;
      nbytes: bigint;
      flags: number;
    }>,
  ): number {
    const view = this.getView();
    for (let i = 0; i < events.length; i++) {
      const base = outPtr + i * 32;
      const ev = events[i];
      view.setBigUint64(base, ev.userdata, true);
      view.setUint16(base + 8, ev.error, true);
      view.setUint8(base + 10, ev.type);
      view.setUint8(base + 11, 0);
      view.setUint32(base + 12, 0, true);
      view.setBigUint64(base + 16, ev.nbytes, true);
      view.setUint16(base + 24, ev.flags, true);
      view.setUint16(base + 26, 0, true);
      view.setUint32(base + 28, 0, true);
    }
    view.setUint32(neventsPtr, events.length, true);
    return WASI_ESUCCESS;
  }
```

- [ ] **Step 6: Wire all four into `getImports()`**

Replace these four stubs:

```typescript
        fd_renumber: this.stub.bind(this),
        path_link: this.stub.bind(this),
        poll_oneoff: this.stub.bind(this),
        clock_res_get: this.stub.bind(this),
```

with:

```typescript
        fd_renumber: this.fdRenumber.bind(this),
        path_link: this.pathLink.bind(this),
        poll_oneoff: this.pollOneoff.bind(this),
        clock_res_get: this.clockResGet.bind(this),
```

- [ ] **Step 7: Verify no type errors**

Run: `deno check packages/orchestrator/src/wasi/wasi-host.ts`
Expected: No errors.

- [ ] **Step 8: Commit**

```bash
git add packages/orchestrator/src/wasi/wasi-host.ts
git commit -m "feat(wasi): implement poll_oneoff, fd_renumber, clock_res_get, path_link"
```

---

### Task 5: Migrate shell sleep from `host_sleep` to `std::thread::sleep`

**Files:**
- Modify: `packages/shell-exec/src/builtins.rs`
- Modify: `packages/shell-exec/src/host.rs`
- Modify: `packages/orchestrator/src/host-imports/kernel-imports.ts`
- Modify: `packages/orchestrator/src/shell/shell-instance.ts`

Currently the shell's `sleep` builtin calls `host.sleep(ms)` which is a custom FFI import (`host_sleep`). This bypasses `poll_oneoff` entirely. We replace it with `std::thread::sleep()`, which goes through wasi-libc → `poll_oneoff`. This:
1. Eliminates a redundant code path
2. Makes `sleep` the integration test for `poll_oneoff`
3. Means any Rust binary using `std::thread::sleep()` now works

- [ ] **Step 1: Update `builtins.rs` to use `std::thread::sleep`**

In `packages/shell-exec/src/builtins.rs`, replace the `builtin_sleep` function (around line 1984):

```rust
fn builtin_sleep(host: &dyn HostInterface, args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        shell_eprintln!("sleep: missing operand");
        return BuiltinResult::Result(1);
    }
    let secs: f64 = args[0].parse().unwrap_or(0.0);
    let ms = (secs * 1000.0) as u32;
    if ms > 0 {
        let _ = host.sleep(ms);
    }
    BuiltinResult::Result(0)
}
```

with:

```rust
fn builtin_sleep(_host: &dyn HostInterface, args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        shell_eprintln!("sleep: missing operand");
        return BuiltinResult::Result(1);
    }
    let secs: f64 = args[0].parse().unwrap_or(0.0);
    let ms = (secs * 1000.0) as u64;
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    BuiltinResult::Result(0)
}
```

- [ ] **Step 2: Remove `sleep` from `HostInterface` trait in `host.rs`**

In `packages/shell-exec/src/host.rs`, remove the `sleep` method from the `HostInterface` trait definition and its implementation on `WasmHost`. Also remove the `host_sleep` extern declaration.

Remove from the trait:
```rust
    /// Sleep for the given number of milliseconds. JSPI-suspending on wasm32.
    fn sleep(&self, ms: u32) -> Result<(), HostError>;
```

Remove the `host_sleep` extern:
```rust
    fn host_sleep(ms: u32);
```

Remove the implementation:
```rust
    fn sleep(&self, ms: u32) -> Result<(), HostError> {
        unsafe { host_sleep(ms) };
        Ok(())
    }
```

Also remove the `sleep` method from the `TestHost` impl if it exists (search for `fn sleep` in the test support code).

- [ ] **Step 3: Remove `host_sleep` from kernel-imports.ts**

In `packages/orchestrator/src/host-imports/kernel-imports.ts`, remove the `host_sleep` function (around line 182-186):

```typescript
    // host_sleep(ms) -> void
    // Async — suspends WASM for ms milliseconds.
    async host_sleep(ms: number): Promise<void> {
      await new Promise<void>(resolve => setTimeout(resolve, ms));
    },
```

- [ ] **Step 4: Remove `host_sleep` from shell-instance.ts WASM imports**

In `packages/orchestrator/src/shell/shell-instance.ts`, find and remove the two places where `host_sleep` is wired into WASM imports. Search for `host_sleep` — there are two occurrences (around lines 178-180 and 893-895):

```typescript
      // host_sleep: cooperative sleep
      codepodImports.host_sleep = new WebAssembly.Suspending(
        kernelImports.host_sleep as (ms: number) => Promise<void>,
      );
```

Remove both occurrences.

- [ ] **Step 5: Verify Rust compiles**

Run: `cargo check --target wasm32-wasip1 -p codepod-shell-exec`
Expected: Compiles. `std::thread::sleep` is available on wasm32-wasip1 (goes through `poll_oneoff`).

- [ ] **Step 6: Verify TypeScript compiles**

Run: `deno check packages/orchestrator/src/shell/shell-instance.ts`
Expected: No errors.

- [ ] **Step 7: Commit**

```bash
git add packages/shell-exec/src/builtins.rs packages/shell-exec/src/host.rs packages/orchestrator/src/host-imports/kernel-imports.ts packages/orchestrator/src/shell/shell-instance.ts
git commit -m "refactor: migrate shell sleep from host_sleep to std::thread::sleep (poll_oneoff)"
```

---

### Task 6: Rebuild shell-exec WASM binary

**Files:**
- Rebuild: `packages/orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm`

The shell-exec binary must be rebuilt so it uses `poll_oneoff` instead of `host_sleep`.

- [ ] **Step 1: Build**

Run: `cargo build --target wasm32-wasip1 --release -p codepod-shell-exec`
Expected: Compiles successfully.

- [ ] **Step 2: Copy to fixtures**

Run the project's copy script or manually:
```bash
cp target/wasm32-wasip1/release/codepod-shell-exec.wasm packages/orchestrator/src/platform/__tests__/fixtures/
```

(Check if `scripts/copy-wasm.sh` handles this — use it if so.)

- [ ] **Step 3: Verify poll_oneoff is now imported**

Run: `wasm-tools dump packages/orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm 2>/dev/null | grep poll_oneoff`
Expected: Shows `poll_oneoff` in the import section.

- [ ] **Step 4: Verify host_sleep is no longer imported**

Run: `wasm-tools dump packages/orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm 2>/dev/null | grep host_sleep`
Expected: No output (host_sleep is gone).

- [ ] **Step 5: Commit**

```bash
git add packages/orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm
git commit -m "build: rebuild shell-exec with poll_oneoff instead of host_sleep"
```

---

### Task 7: Integration tests

**Files:**
- Create: `packages/orchestrator/src/__tests__/wasi-syscalls.test.ts`

Now that the shell's `sleep` uses `std::thread::sleep()` → `poll_oneoff`, we can test `poll_oneoff` directly through `sleep` commands. No separate test fixture needed.

- [ ] **Step 1: Create test file**

Create `packages/orchestrator/src/__tests__/wasi-syscalls.test.ts`:

```typescript
/**
 * Integration tests for WASI syscall implementations:
 * poll_oneoff, fd_renumber, clock_res_get, path_link.
 *
 * poll_oneoff is tested via the shell's `sleep` builtin, which now uses
 * std::thread::sleep() → wasi-libc → poll_oneoff (not the old host_sleep).
 */
import { describe, it, afterEach } from '@std/testing/bdd';
import { expect } from '@std/expect';
import { resolve } from 'node:path';
import { Sandbox } from '../sandbox.js';
import { NodeAdapter } from '../platform/node-adapter.js';

const WASM_DIR = resolve(import.meta.dirname, '../platform/__tests__/fixtures');

describe('WASI syscalls', { sanitizeResources: false, sanitizeOps: false }, () => {
  let sandbox: Sandbox;

  afterEach(() => {
    sandbox?.destroy();
  });

  // ---- poll_oneoff (via sleep → std::thread::sleep → poll_oneoff) ----

  describe('poll_oneoff', () => {
    it('sleep completes without error', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      const result = await sandbox.run('sleep 0.01');
      expect(result.exitCode).toBe(0);
    });

    it('sleep 0 completes immediately', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      const result = await sandbox.run('sleep 0');
      expect(result.exitCode).toBe(0);
    });

    it('sleep respects approximate duration', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      const start = Date.now();
      const result = await sandbox.run('sleep 0.05');
      const elapsed = Date.now() - start;
      expect(result.exitCode).toBe(0);
      // Should have waited at least ~30ms (allowing for timer imprecision)
      expect(elapsed).toBeGreaterThanOrEqual(30);
    });

    it('sleep works in a pipeline', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      const result = await sandbox.run('sleep 0.01 && echo done');
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toBe('done');
    });
  });

  // ---- fd_renumber (via shell redirection which uses dup2) ----

  describe('fd_renumber', () => {
    it('output redirection works', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      const result = await sandbox.run('echo hello > /tmp/out.txt && cat /tmp/out.txt');
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toBe('hello');
    });

    it('append redirection works', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      const result = await sandbox.run(
        'echo first > /tmp/out.txt && echo second >> /tmp/out.txt && cat /tmp/out.txt'
      );
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toBe('first\nsecond');
    });
  });

  // ---- path_link ----

  describe('path_link', () => {
    it('hard link (ln without -s) fails gracefully', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      sandbox.writeFile('/tmp/source.txt', new TextEncoder().encode('hello'));
      const result = await sandbox.run('ln /tmp/source.txt /tmp/link.txt');
      expect(result.exitCode).not.toBe(0);
    });

    it('symlink (ln -s) still works', async () => {
      sandbox = await Sandbox.create({ wasmDir: WASM_DIR, adapter: new NodeAdapter() });
      sandbox.writeFile('/tmp/source.txt', new TextEncoder().encode('hello'));
      const result = await sandbox.run(
        'ln -s /tmp/source.txt /tmp/link.txt && cat /tmp/link.txt'
      );
      expect(result.exitCode).toBe(0);
      expect(result.stdout.trim()).toBe('hello');
    });
  });
});
```

- [ ] **Step 2: Run the tests**

Run: `deno test -A --no-check packages/orchestrator/src/__tests__/wasi-syscalls.test.ts`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add packages/orchestrator/src/__tests__/wasi-syscalls.test.ts
git commit -m "test(wasi): add integration tests for poll_oneoff, fd_renumber, path_link"
```

---

### Task 8: Run full test suite

Verify nothing is broken by the `host_sleep` removal and new syscalls.

- [ ] **Step 1: Source dev environment**

Run: `source scripts/dev-init.sh`

- [ ] **Step 2: Run all tests**

Run: `deno test -A --no-check packages/orchestrator/src/__tests__/*.test.ts packages/sdk-server/src/*.test.ts`
Expected: All tests pass (existing + new). The existing sleep-related tests should still work since `sleep` still works — it just goes through `poll_oneoff` now.

- [ ] **Step 3: Final commit if any fixups needed**

If any tests required fixes, commit:

```bash
git add -u
git commit -m "fix(wasi): address test failures from syscall implementations"
```
