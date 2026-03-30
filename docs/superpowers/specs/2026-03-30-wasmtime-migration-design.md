# Wasmtime Migration Design

**Date:** 2026-03-30
**Status:** Draft

## Summary

Replace Deno as the backend WASM runtime with wasmtime — a production-ready, native Rust runtime that supports WASM threads (wasi-threads) natively, eliminating the need for JSPI or asyncify on the server side. Deno remains available via a build-time `--engine` flag for development and debugging. Browser execution is unchanged (JSPI on Chrome/Deno, asyncify fallback on Safari).

## Background

The current backend (MCP server, Python SDK server) embeds Deno via `deno compile`, which produces a binary containing the full Deno VM (~50MB) alongside the bundled TypeScript orchestrator. This has several downsides:

- Deno VM overhead for what is effectively a WASM-hosting task
- JSPI required for async host imports — a browser-focused proposal not ideal for server use
- Large binary size
- Deno as a hidden runtime dependency

`packages/sdk-server-wasmtime` already has ~60% of the wasmtime backend complete: RPC transport, VFS layer, WASM engine with wasmtime, process kernel with fd tables and pipes, and network fetch. What remains is wiring the dispatcher methods.

## Architecture

### Current

```
Python SDK  →  Deno subprocess  →  packages/sdk-server (TS)  →  @codepod/sandbox (JS)
MCP server  →  deno compile binary  →  packages/mcp-server (TS)  →  @codepod/sandbox (JS)
Browser     →  @codepod/sandbox (JS)  →  WebAssembly (JSPI)
```

### Target

```
Python SDK  →  codepod-server (Rust/wasmtime)    [default]
           →  Deno subprocess                     [--engine deno]
MCP server  →  codepod-mcp-rust (Rust/wasmtime)  [default]
           →  deno compile binary                 [--engine deno]
Browser     →  @codepod/sandbox (JS, JSPI or asyncify)  [unchanged]
```

The wasmtime path eliminates the JS VM for backend use entirely. WASM runs natively via wasmtime; async host calls use real OS threads (wasi-threads) rather than JSPI suspension or asyncify stack-save.

## Components

### 1. Complete `sdk-server-wasmtime` Dispatcher

**Already done:** RPC transport (newline-delimited JSON-RPC 2.0 over stdio), VFS (MemVfs with COW), WASM engine (wasmtime + WASI preview1), process kernel (fd tables, pipes, waitpid), network fetch (reqwest), host function bindings for ~40 codepod-namespace imports.

**Remaining:** Implement the 23 dispatcher method handlers in `src/dispatcher.rs`. Also split the crate into `lib.rs` + `main.rs` so `mcp-server-rust` can link against the library without duplicating code. Each method maps directly to an existing Rust layer:

| RPC Method | Implementation |
|---|---|
| `create` | Init MemVfs, load `codepod-shell-exec.wasm` via WasmEngine, start _start |
| `run` | Call `__run_command`, stream stdout/stderr as JSON-RPC notifications |
| `files.read` | MemVfs::read_file |
| `files.write` | MemVfs::write_file |
| `files.list` | MemVfs::readdir |
| `files.mkdir` | MemVfs::mkdir |
| `files.rm` | MemVfs::rm |
| `files.stat` | MemVfs::stat |
| `sandbox.fork` | COW clone of MemVfs + fresh WasmEngine instance |
| `sandbox.create` | New top-level sandbox (same as create) |
| `sandbox.destroy` | Drop sandbox, release resources |
| `sandbox.list` | Return active sandbox IDs |
| `snapshot.create` | MemVfs::snapshot() |
| `snapshot.restore` | MemVfs::restore(id) |
| `state.export` | Serialize VFS to bytes (existing serializer) |
| `state.import` | Deserialize bytes into VFS |
| `env.set` | Write env var into WASM env table |
| `env.get` | Read env var from WASM env table |
| `history.get` | Return command history from ShellState |
| `history.clear` | Clear command history |
| `mount` | Add virtual read-only overlay to MemVfs |
| `offload` | Serialize VFS + notify Python callback |
| `rehydrate` | Deserialize VFS from Python callback |

The WASM binary (`codepod-shell-exec.wasm`) is the same `wasm32-wasip1` binary — no separate build target. wasmtime runs it directly with native async via Tokio tasks (one task per spawned child process), no JSPI or asyncify needed.

**Streaming output:** The `run` dispatcher sends JSON-RPC notifications (`{"jsonrpc":"2.0","method":"output","params":{"request_id":N,"stream":"stdout","data":"..."}}`) while the WASM runs, matching the existing protocol the Python SDK and TS server already speak.

### 2. New `packages/mcp-server-rust/` Crate

Rather than adding MCP protocol handling to `sdk-server-wasmtime`, create a thin `mcp-server-rust` crate that:

- Links `sdk_server_wasmtime` as a library dependency
- Implements the MCP protocol layer (JSON-RPC over stdio, tool registration)
- Exposes the same tools as the current TS MCP server: `create_sandbox`, `destroy_sandbox`, `list_sandboxes`, `run_command`, `read_file`, `write_file`, `list_directory`, `snapshot`, `restore`, `export_state`, `import_state`
- Uses the `rmcp` crate for MCP protocol scaffolding, or hand-rolls the thin layer (MCP tool calls are just JSON-RPC method dispatches)

**Asset bundling:**
- `codepod-shell-exec.wasm` — embedded via `include_bytes!` (1.4MB, acceptable)
- Coreutils WASMs (80+ files, ~30MB total) — distributed alongside the binary in a `wasm/` directory, not embedded (too large)
- Python packages — distributed alongside as before
- Build script copies the `wasm/` directory next to the compiled binary

### 3. Python SDK Update (`_rpc.py`)

Changes to `RpcClient`:

```python
class RpcClient:
    def __init__(self, engine='auto', ...):
        if engine == 'auto':
            binary = self._find_binary('codepod-server') or self._find_deno()
        elif engine == 'wasmtime':
            binary = self._find_binary('codepod-server')
        elif engine == 'deno':
            binary = self._find_deno()
        ...
```

Binary discovery order for `auto`:
1. `codepod-server` adjacent to the Python wheel (installed via platform wheel)
2. `codepod-server` on PATH
3. Deno fallback (existing behavior)

The `codepod-server` Rust binary is distributed as a platform-specific extra in the wheel, built by `maturin` or a simple `build.py` that invokes `cargo build --release`.

Expose `engine` in the public API:
```python
import codepod
sb = codepod.Sandbox(engine='wasmtime')  # explicit
sb = codepod.Sandbox()                   # auto-detect (wasmtime preferred)
```

### 4. Build Script Flags

```bash
# Existing scripts gain an --engine flag:
scripts/build-mcp.sh [--engine wasmtime|deno]     # default: wasmtime
scripts/build-sdk-server.sh [--engine wasmtime|deno]  # new script, default: wasmtime
```

**`--engine wasmtime`** (default):
- `cargo build -p mcp-server-rust --release` → `dist/codepod-mcp`
- `cargo build -p sdk-server-wasmtime --release` → `dist/codepod-server`
- Copy `wasm/` assets alongside binary

**`--engine deno`**:
- Existing `deno compile` path (unchanged behavior)

## Testing Strategy

Tests must cover both engines and catch regressions independently.

### Unit Tests

- **Rust (`cargo test`):** `sdk-server-wasmtime` and `mcp-server-rust` unit tests. Already started; expand to cover all dispatcher methods with mocked WASM and real VFS.
- **Deno (`deno test`):** Existing orchestrator test suite. Unchanged.

### Integration Tests (Engine-Parameterized)

A shared integration test suite that spawns the server binary and drives it via JSON-RPC. Lives in `packages/integration-tests/` (Deno-based for easy JSON-RPC scripting):

```typescript
// Parameterized by SERVER_BINARY env var
const server = spawnServer(Deno.env.get('SERVER_BINARY') ?? 'dist/codepod-server');
// Run same scenarios against both engines
```

Test scenarios cover: create/run/destroy lifecycle, file I/O, pipelines, snapshots, forking, env vars, streaming output, error handling.

### Browser Tests

Existing Playwright suite (Chromium + WebKit) unchanged — browser path is not affected by this migration.

### CI Matrix

```yaml
jobs:
  test-deno:      # existing: deno test + browser playwright
  test-wasmtime:  # new: cargo test + integration tests with SERVER_BINARY=dist/codepod-server
  test-deno-compat:  # new: integration tests with SERVER_BINARY=dist/codepod-server-deno
  test-browser:   # existing: playwright chromium + webkit
```

The `test-deno-compat` job builds the Deno binary and runs the same integration scenarios, ensuring the `--engine deno` path stays working.

## Scope Boundaries

**In scope:**
- Complete wasmtime dispatcher (all 23 methods)
- New `mcp-server-rust` crate
- Python SDK `engine` parameter + binary discovery
- Build script `--engine` flag
- Integration test suite (engine-parameterized)
- CI matrix update
- Docs update (TypeScript SDK guide, Python SDK guide, CLAUDE.md)

**Out of scope:**
- Wasmer / WASIX (non-standard, not needed)
- Node.js / Bun as engine targets (no JSPI in Bun; Node 25 not widely deployed)
- Asyncify for the wasmtime binary (wasmtime uses threads natively)
- Persistence backends for wasmtime (export/import covers the essential case; auto-persistence is a follow-on)

## File Layout (after migration)

```
packages/
  sdk-server-wasmtime/     # Rust, library + binary crates
    src/
      lib.rs               # pub exports for mcp-server-rust linkage
      main.rs              # codepod-server binary (JSON-RPC stdio)
      dispatcher.rs        # RPC method handlers (new)
      wasm/                # wasmtime engine, process kernel, network
      vfs/                 # MemVfs
  mcp-server-rust/         # Rust, new crate
    src/
      main.rs              # codepod-mcp binary
      tools.rs             # MCP tool definitions
  sdk-server/              # TS, kept for --engine deno path
  mcp-server/              # TS, kept for --engine deno path
  integration-tests/       # new: engine-parameterized test suite
    tests/
      lifecycle.test.ts
      files.test.ts
      pipelines.test.ts
      snapshots.test.ts
scripts/
  build-mcp.sh             # gains --engine flag
  build-sdk-server.sh      # new script
```
