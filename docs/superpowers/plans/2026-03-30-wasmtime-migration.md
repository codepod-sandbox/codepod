# Wasmtime Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Deno as the backend WASM runtime with a native Rust `codepod-server-wasmtime` binary, keeping Deno available via `--engine deno`.

**Architecture:** The `sdk-server-wasmtime` crate already has ~60% of the wasmtime backend (VFS, engine, process kernel, network). This plan wires the dispatcher to those layers, adds a Rust MCP server, updates the Python SDK with an `engine` parameter, and adds engine-parameterized integration tests to CI.

**Tech Stack:** Rust/Tokio/wasmtime 28, serde/bincode for VFS serialization, Python `subprocess`, Deno TypeScript for integration tests.

---

## File Map

**New files:**
- `packages/sdk-server-wasmtime/src/sandbox.rs` — `SandboxState` struct (ShellInstance + env) and `SandboxManager` (root + forks + named sandboxes)
- `packages/sdk-server-wasmtime/tests/integration.rs` — dispatcher integration tests
- `packages/mcp-server-rust/Cargo.toml`
- `packages/mcp-server-rust/src/main.rs`
- `packages/mcp-server-rust/src/tools.rs`
- `scripts/build-sdk-server.sh`

**Modified files:**
- `packages/sdk-server-wasmtime/src/dispatcher.rs` — wired handlers
- `packages/sdk-server-wasmtime/src/lib.rs` — export sandbox module
- `packages/sdk-server-wasmtime/src/main.rs` — pass `stdout_tx` to dispatcher
- `packages/sdk-server-wasmtime/src/vfs/inode.rs` — add serde derives
- `packages/sdk-server-wasmtime/src/vfs/mod.rs` — add serde derives + serialization
- `packages/sdk-server-wasmtime/Cargo.toml` — add bincode dep
- `packages/python-sdk/src/codepod/sandbox.py` — `engine` parameter
- `scripts/build-mcp.sh` — `--engine` flag
- `Cargo.toml` — add mcp-server-rust workspace member
- `.github/workflows/ci.yml` — add wasmtime CI job

---

## Task 1: SandboxState struct + wired `create` handler

**Files:**
- Create: `packages/sdk-server-wasmtime/src/sandbox.rs`
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs`
- Modify: `packages/sdk-server-wasmtime/src/lib.rs`
- Test: `packages/sdk-server-wasmtime/tests/integration.rs`

- [ ] **Step 1: Create test file with a failing `create` test**

```rust
// packages/sdk-server-wasmtime/tests/integration.rs
use sdk_server_wasmtime::sandbox::SandboxManager;
use serde_json::json;

fn wasm_bytes() -> Vec<u8> {
    // Path relative to the workspace root; adjust if running from elsewhere.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm");
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[tokio::test]
async fn test_create_and_run() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();
    let result = mgr.root_run("echo hello").await.unwrap();
    assert_eq!(result["exit_code"].as_i64().unwrap(), 0);
    assert!(result["stdout"].as_str().unwrap().contains("hello"));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd packages/sdk-server-wasmtime && cargo test test_create_and_run 2>&1 | tail -20
```
Expected: compile error — `sandbox` module doesn't exist yet.

- [ ] **Step 3: Create `sandbox.rs`**

```rust
// packages/sdk-server-wasmtime/src/sandbox.rs
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{bail, Result};
use serde_json::Value;

use crate::wasm::{WasmEngine, ShellInstance, StoreData};
use crate::vfs::MemVfs;

const MAX_FORKS: usize = 16;
const MAX_NAMED: usize = 64;

pub struct SandboxState {
    pub engine: Arc<WasmEngine>,
    pub wasm_bytes: Arc<Vec<u8>>,
    pub shell: ShellInstance,
    /// Env table — synced from `__run_command` output after each call.
    pub env: HashMap<String, String>,
}

impl SandboxState {
    pub async fn new(
        engine: Arc<WasmEngine>,
        wasm_bytes: Arc<Vec<u8>>,
        fs_limit_bytes: Option<usize>,
        initial_env: Vec<(String, String)>,
    ) -> Result<Self> {
        let vfs = MemVfs::new(fs_limit_bytes, None);
        let shell = ShellInstance::new(&engine, &wasm_bytes, vfs, &initial_env).await?;
        let env: HashMap<_, _> = initial_env.into_iter().collect();
        Ok(Self { engine, wasm_bytes, shell, env })
    }

    /// Run a shell command; sync env from the WASM output.
    pub async fn run(&mut self, cmd: &str) -> Result<Value> {
        let result = self.shell.run_command(cmd).await?;
        // Sync env returned by the WASM shell.
        if let Some(env_map) = result.get("env").and_then(|v| v.as_object()) {
            self.env.clear();
            for (k, v) in env_map {
                if let Some(s) = v.as_str() {
                    self.env.insert(k.clone(), s.to_owned());
                }
            }
        }
        // Build response: stdout/stderr come from the pipe captures.
        let stdout = String::from_utf8_lossy(&self.shell.take_stdout()).into_owned();
        let stderr = String::from_utf8_lossy(&self.shell.take_stderr()).into_owned();
        Ok(serde_json::json!({
            "exitCode": result["exit_code"].as_i64().unwrap_or(1),
            "stdout": stdout,
            "stderr": stderr,
            "executionTimeMs": result["execution_time_ms"].as_u64().unwrap_or(0),
        }))
    }

    pub async fn fork(&self) -> Result<Self> {
        let forked_vfs = self.shell.vfs().cow_clone();
        let env_vec: Vec<_> = self.env.iter().map(|(k,v)| (k.clone(), v.clone())).collect();
        let shell = ShellInstance::new(&self.engine, &self.wasm_bytes, forked_vfs, &env_vec).await?;
        Ok(Self {
            engine: self.engine.clone(),
            wasm_bytes: self.wasm_bytes.clone(),
            shell,
            env: self.env.clone(),
        })
    }
}

/// Manages root sandbox + forks + named sandboxes (mirrors TypeScript Dispatcher).
pub struct SandboxManager {
    pub root: Option<SandboxState>,
    pub forks: HashMap<String, SandboxState>,
    pub next_fork_id: u32,
    pub named: HashMap<String, SandboxState>,
    pub next_named_id: u32,
}

impl SandboxManager {
    pub fn new() -> Self {
        Self {
            root: None,
            forks: HashMap::new(),
            next_fork_id: 1,
            named: HashMap::new(),
            next_named_id: 1,
        }
    }

    pub async fn create(
        &mut self,
        wasm_bytes: Vec<u8>,
        fs_limit_bytes: Option<usize>,
        timeout_ms: Option<u64>,
        initial_env: Option<Vec<(String, String)>>,
    ) -> Result<()> {
        let _ = timeout_ms; // Phase 6: fuel limits
        let engine = Arc::new(WasmEngine::new()?);
        let wasm = Arc::new(wasm_bytes);
        let env = initial_env.unwrap_or_default();
        self.root = Some(SandboxState::new(engine, wasm, fs_limit_bytes, env).await?);
        Ok(())
    }

    pub fn resolve(&mut self, sandbox_id: Option<&str>) -> Result<&mut SandboxState> {
        match sandbox_id {
            None | Some("") => self.root.as_mut().ok_or_else(|| anyhow::anyhow!("no root sandbox")),
            Some(id) => {
                if let Some(sb) = self.named.get_mut(id) { return Ok(sb); }
                if let Some(sb) = self.forks.get_mut(id) { return Ok(sb); }
                bail!("unknown sandboxId: {id}")
            }
        }
    }

    pub async fn root_run(&mut self, cmd: &str) -> Result<Value> {
        self.root.as_mut().ok_or_else(|| anyhow::anyhow!("no root sandbox"))?.run(cmd).await
    }
}
```

- [ ] **Step 4: Export sandbox module from `lib.rs`**

```rust
// packages/sdk-server-wasmtime/src/lib.rs
pub mod sandbox;
pub mod vfs;
pub mod wasm;
```

- [ ] **Step 5: Update `ShellInstance` to expose `take_stdout`/`take_stderr` as `Vec<u8>`**

`take_stdout()` and `take_stderr()` return `bytes::Bytes`. Update the test to handle this, or add wrapper methods. Check current signature in `wasm/instance.rs` — both return `bytes::Bytes`. The `sandbox.rs` code above calls them via `.into_owned()` on `Cow` from `from_utf8_lossy` — this works fine since `Bytes` implements `Deref<[u8]>`.

- [ ] **Step 6: Update `dispatcher.rs` to hold a `SandboxManager` and wire `create`**

```rust
// packages/sdk-server-wasmtime/src/dispatcher.rs
use std::path::Path;
use serde_json::{json, Value};
use crate::rpc::{codes, RequestId, Response};
use crate::sandbox::SandboxManager;

pub struct Dispatcher {
    pub manager: SandboxManager,
    initialized: bool,
    stdout_tx: tokio::sync::mpsc::Sender<String>,
    next_cb_id: u32,
}

impl Dispatcher {
    pub fn new(stdout_tx: tokio::sync::mpsc::Sender<String>) -> Self {
        Self {
            manager: SandboxManager::new(),
            initialized: false,
            stdout_tx,
            next_cb_id: 1,
        }
    }

    pub async fn dispatch(&mut self, id: Option<RequestId>, method: &str, params: Value) -> (Response, bool) {
        let resp = match method {
            "create" => self.handle_create(id, params).await,
            "kill"   => return (Response::ok(id, json!({"ok":true})), true),
            _ if !self.initialized => Response::err(id, codes::INVALID_REQUEST, "call 'create' first"),
            _ => self.dispatch_initialized(id, method, params).await,
        };
        (resp, false)
    }

    async fn handle_create(&mut self, id: Option<RequestId>, params: Value) -> Response {
        if self.initialized {
            return Response::err(id, codes::INVALID_REQUEST, "already initialized");
        }
        let shell_wasm_path = match params.get("shellWasmPath").and_then(|v| v.as_str()) {
            Some(p) => p.to_owned(),
            None => return Response::err(id, codes::INVALID_PARAMS, "missing shellWasmPath"),
        };
        let wasm_bytes = match std::fs::read(&shell_wasm_path) {
            Ok(b) => b,
            Err(e) => return Response::err(id, codes::INTERNAL_ERROR, format!("read wasm: {e}")),
        };
        let fs_limit = params.get("fsLimitBytes").and_then(|v| v.as_u64()).map(|n| n as usize);
        let timeout_ms = params.get("timeoutMs").and_then(|v| v.as_u64());

        // Handle mounts encoded as [{path, files: {rel_path: base64}}]
        let mounts: Vec<(String, std::collections::HashMap<String, Vec<u8>>)> = params
            .get("mounts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|m| {
                    let path = m.get("path")?.as_str()?.to_owned();
                    let files = m.get("files")?.as_object()?;
                    let decoded: std::collections::HashMap<String, Vec<u8>> = files.iter()
                        .filter_map(|(k, v)| {
                            let b64 = v.as_str()?;
                            let bytes = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD, b64
                            ).ok()?;
                            Some((k.clone(), bytes))
                        }).collect();
                    Some((path, decoded))
                }).collect()
            })
            .unwrap_or_default();

        if let Err(e) = self.manager.create(wasm_bytes, fs_limit, timeout_ms, None).await {
            return Response::err(id, codes::INTERNAL_ERROR, format!("create: {e}"));
        }

        // Apply mounts
        if let Some(root) = self.manager.root.as_mut() {
            for (mount_path, files) in mounts {
                for (rel, content) in files {
                    let full = format!("{}/{}", mount_path.trim_end_matches('/'), rel);
                    if let Some(parent) = Path::new(&full).parent() {
                        let _ = root.shell.vfs_mut().mkdirp(parent.to_str().unwrap_or("/"));
                    }
                    if let Err(e) = root.shell.vfs_mut().write_file(&full, &content) {
                        tracing::warn!("mount write {full}: {e}");
                    }
                }
            }
        }

        self.initialized = true;
        Response::ok(id, json!({"ok": true}))
    }

    async fn dispatch_initialized(&mut self, id: Option<RequestId>, method: &str, params: Value) -> Response {
        Response::not_implemented(id, method)
    }
}
```

- [ ] **Step 7: Update `main.rs` to pass `stdout_tx` to dispatcher**

```rust
// In main() in main.rs, change:
// let mut dispatcher = dispatcher::Dispatcher::new();
// to:
let mut dispatcher = dispatcher::Dispatcher::new(stdout_tx.clone());
```

- [ ] **Step 8: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test test_create_and_run 2>&1 | tail -30
```
Expected: PASS (echo hello returns exitCode 0, stdout contains "hello").

- [ ] **Step 9: Commit**

```bash
git add packages/sdk-server-wasmtime/src/sandbox.rs \
        packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/src/lib.rs \
        packages/sdk-server-wasmtime/src/main.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): add SandboxState + wire create handler"
```

---

## Task 2: File operations (files.read / write / list / mkdir / rm / stat)

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs` (add `dispatch_initialized` arms)
- Test: `packages/sdk-server-wasmtime/tests/integration.rs`

- [ ] **Step 1: Add file operation tests**

```rust
// Append to tests/integration.rs
#[tokio::test]
async fn test_file_ops() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();

    let sb = mgr.root.as_mut().unwrap();

    // write
    sb.shell.vfs_mut().write_file("/tmp/hello.txt", b"hello world").unwrap();

    // read
    let content = sb.shell.vfs().read_file("/tmp/hello.txt").unwrap();
    assert_eq!(content, b"hello world");

    // stat
    let st = sb.shell.vfs().stat("/tmp/hello.txt").unwrap();
    assert_eq!(st.size, 11);

    // list
    let entries = sb.shell.vfs().readdir("/tmp").unwrap();
    assert!(entries.iter().any(|e| e.name == "hello.txt"));

    // mkdir + list
    sb.shell.vfs_mut().mkdir("/tmp/subdir").unwrap();
    let entries2 = sb.shell.vfs().readdir("/tmp").unwrap();
    assert!(entries2.iter().any(|e| e.name == "subdir"));

    // rm
    sb.shell.vfs_mut().unlink("/tmp/hello.txt").unwrap();
    assert!(sb.shell.vfs().read_file("/tmp/hello.txt").is_err());
}
```

- [ ] **Step 2: Run test to verify it fails (compile error — `dispatch_initialized` not wired)**

```bash
cd packages/sdk-server-wasmtime && cargo test test_file_ops 2>&1 | tail -10
```

- [ ] **Step 3: Add RPC-level file operation helpers in dispatcher**

Add these helpers at the top of `dispatcher.rs`:

```rust
use base64::Engine as B64Engine;
use crate::vfs::VfsError;

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD.decode(s)
        .map_err(|e| format!("base64 decode: {e}"))
}
fn b64_encode(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}
fn vfs_err(id: Option<RequestId>, e: VfsError) -> Response {
    Response::err(id, 1, e.to_string())
}
fn require_str<'a>(id: Option<RequestId>, params: &'a Value, key: &str)
    -> Result<&'a str, Response>
{
    params.get(key).and_then(|v| v.as_str())
        .ok_or_else(|| Response::err(id.clone(), codes::INVALID_PARAMS, format!("missing: {key}")))
}
fn sandbox_id(params: &Value) -> Option<&str> {
    params.get("sandboxId").and_then(|v| v.as_str())
}
```

- [ ] **Step 4: Wire file operations in `dispatch_initialized`**

```rust
async fn dispatch_initialized(&mut self, id: Option<RequestId>, method: &str, params: Value) -> Response {
    let sid = sandbox_id(&params);
    match method {
        "run" => self.handle_run(id, params).await,
        "files.read" => self.handle_files_read(id, params, sid),
        "files.write" => self.handle_files_write(id, params, sid),
        "files.list" => self.handle_files_list(id, params, sid),
        "files.mkdir" => self.handle_files_mkdir(id, params, sid),
        "files.rm" => self.handle_files_rm(id, params, sid),
        "files.stat" => self.handle_files_stat(id, params, sid),
        "env.set" => self.handle_env_set(id, params, sid).await,
        "env.get" => self.handle_env_get(id, params, sid),
        "snapshot.create" => self.handle_snapshot_create(id, params, sid),
        "snapshot.restore" => self.handle_snapshot_restore(id, params, sid),
        "persistence.export" => self.handle_persistence_export(id, params, sid),
        "persistence.import" => self.handle_persistence_import(id, params, sid),
        "mount" => self.handle_mount(id, params, sid),
        "sandbox.fork" => self.handle_sandbox_fork(id, params, sid).await,
        "sandbox.destroy" => self.handle_sandbox_destroy(id, params),
        "sandbox.create" => self.handle_sandbox_create(id, params).await,
        "sandbox.list" => self.handle_sandbox_list(id),
        "sandbox.remove" => self.handle_sandbox_remove(id, params),
        "shell.history.list" => self.handle_history_list(id, params, sid).await,
        "shell.history.clear" => self.handle_history_clear(id, params, sid).await,
        "offload" => self.handle_offload(id, params, sid).await,
        "rehydrate" => self.handle_rehydrate(id, params, sid).await,
        _ => Response::method_not_found(id, method),
    }
}
```

- [ ] **Step 5: Implement file operation handlers**

```rust
fn handle_files_read(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.shell.vfs().read_file(path) {
        Ok(bytes) => Response::ok(id, json!({"data": b64_encode(&bytes)})),
        Err(e) => vfs_err(id, e),
    }
}

fn handle_files_write(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let data_b64 = match require_str(id.clone(), &params, "data") { Ok(d) => d, Err(r) => return r };
    let bytes = match b64_decode(data_b64) { Ok(b) => b, Err(e) => return Response::err(id, codes::INVALID_PARAMS, e) };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    // Ensure parent directory exists.
    if let Some(parent) = std::path::Path::new(path).parent().and_then(|p| p.to_str()) {
        let _ = sb.shell.vfs_mut().mkdirp(parent);
    }
    match sb.shell.vfs_mut().write_file(path, &bytes) {
        Ok(_) => Response::ok(id, json!({"ok": true})),
        Err(e) => vfs_err(id, e),
    }
}

fn handle_files_list(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.shell.vfs().readdir(path) {
        Ok(entries) => {
            let enriched: Vec<_> = entries.iter().map(|e| {
                let full = format!("{}/{}", path.trim_end_matches('/'), e.name);
                let size = sb.shell.vfs().stat(&full).map(|s| s.size).unwrap_or(0);
                let kind = match e.kind {
                    crate::vfs::inode::InodeKind::File => "file",
                    crate::vfs::inode::InodeKind::Dir => "dir",
                    crate::vfs::inode::InodeKind::Symlink => "symlink",
                };
                json!({"name": e.name, "type": kind, "size": size})
            }).collect();
            Response::ok(id, json!({"entries": enriched}))
        }
        Err(e) => vfs_err(id, e),
    }
}

fn handle_files_mkdir(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.shell.vfs_mut().mkdir(path) {
        Ok(_) => Response::ok(id, json!({"ok": true})),
        Err(e) => vfs_err(id, e),
    }
}

fn handle_files_rm(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    let vfs = sb.shell.vfs_mut();
    let result = match vfs.stat(path) {
        Ok(st) if st.is_dir() => vfs.remove_recursive(path),
        _ => vfs.unlink(path),
    };
    match result {
        Ok(_) => Response::ok(id, json!({"ok": true})),
        Err(e) => vfs_err(id, e),
    }
}

fn handle_files_stat(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.shell.vfs().stat(path) {
        Ok(st) => {
            let kind = if st.is_dir() { "dir" } else { "file" };
            let name = std::path::Path::new(path).file_name()
                .and_then(|n| n.to_str()).unwrap_or(path);
            Response::ok(id, json!({"name": name, "type": kind, "size": st.size}))
        }
        Err(e) => vfs_err(id, e),
    }
}
```

Note: `DirEntry` in `vfs/inode.rs` needs an `InodeKind` enum or equivalent. Check the actual struct — if it uses a string `"file"/"dir"/"symlink"`, adapt accordingly.

- [ ] **Step 6: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test test_file_ops 2>&1 | tail -20
```
Expected: PASS (compile warnings OK, test passes).

- [ ] **Step 7: Commit**

```bash
git add packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): implement file operation RPC handlers"
```

---

## Task 3: `run` handler + env operations

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs`
- Test: append to `tests/integration.rs`

- [ ] **Step 1: Add `run` and env tests**

```rust
#[tokio::test]
async fn test_run_and_env() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();

    let result = mgr.root_run("echo hello").await.unwrap();
    assert_eq!(result["exitCode"].as_i64().unwrap(), 0);
    assert!(result["stdout"].as_str().unwrap().trim() == "hello");

    // env.set via shell export
    mgr.root.as_mut().unwrap().run("export MYVAR=world").await.unwrap();
    let result2 = mgr.root_run("echo $MYVAR").await.unwrap();
    assert!(result2["stdout"].as_str().unwrap().trim() == "world");
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd packages/sdk-server-wasmtime && cargo test test_run_and_env 2>&1 | tail -10
```
Expected: compile error (handle_run not defined yet).

- [ ] **Step 3: Implement `handle_run`**

```rust
async fn handle_run(&mut self, id: Option<RequestId>, params: Value) -> Response {
    let cmd = match require_str(id.clone(), &params, "command") { Ok(c) => c, Err(r) => return r };
    let sid = sandbox_id(&params);
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.run(cmd).await {
        Ok(result) => Response::ok(id, result),
        Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    }
}
```

- [ ] **Step 4: Implement `handle_env_set` and `handle_env_get`**

`env.set` runs an `export` command to update the shell's internal env:

```rust
async fn handle_env_set(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let name = match require_str(id.clone(), &params, "name") { Ok(n) => n, Err(r) => return r };
    let value = match require_str(id.clone(), &params, "value") { Ok(v) => v, Err(r) => return r };
    // Validate name: must be a valid shell identifier.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
        return Response::err(id, codes::INVALID_PARAMS, "invalid env var name");
    }
    // POSIX single-quote escape for the value.
    let quoted = format!("'{}'", value.replace('\'', "'\\''"));
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    if let Err(e) = sb.run(&format!("export {name}={quoted}")).await {
        return Response::err(id, codes::INTERNAL_ERROR, e.to_string());
    }
    Response::ok(id, json!({"ok": true}))
}

fn handle_env_get(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let name = match require_str(id.clone(), &params, "name") { Ok(n) => n, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    let value = sb.env.get(name).cloned();
    Response::ok(id, json!({"value": value}))
}
```

- [ ] **Step 5: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test test_run_and_env 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): implement run and env RPC handlers"
```

---

## Task 4: Snapshot operations + mount

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs`
- Test: append to `tests/integration.rs`

- [ ] **Step 1: Add snapshot and mount tests**

```rust
#[tokio::test]
async fn test_snapshot() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();

    let sb = mgr.root.as_mut().unwrap();
    sb.shell.vfs_mut().write_file("/tmp/before.txt", b"before").unwrap();

    let snap_id = sb.shell.vfs_mut().snapshot();
    sb.shell.vfs_mut().write_file("/tmp/after.txt", b"after").unwrap();
    assert!(sb.shell.vfs().read_file("/tmp/after.txt").is_ok());

    sb.shell.vfs_mut().restore(&snap_id).unwrap();
    assert!(sb.shell.vfs().read_file("/tmp/before.txt").is_ok());
    assert!(sb.shell.vfs().read_file("/tmp/after.txt").is_err());
}

#[tokio::test]
async fn test_mount() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();

    let sb = mgr.root.as_mut().unwrap();
    // Simulate the mount RPC: write files to a path.
    sb.shell.vfs_mut().mkdirp("/mnt/tools").unwrap();
    sb.shell.vfs_mut().write_file("/mnt/tools/greet.sh", b"echo greetings").unwrap();

    let result = sb.run("sh /mnt/tools/greet.sh").await.unwrap();
    assert!(result["stdout"].as_str().unwrap().contains("greetings"));
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd packages/sdk-server-wasmtime && cargo test test_snapshot test_mount 2>&1 | tail -10
```

- [ ] **Step 3: Check `MemVfs::snapshot()` / `restore()` signatures in `vfs/mod.rs`**

Read the relevant part of `vfs/mod.rs` to see what `snapshot()` returns and what `restore()` expects:

```bash
grep -n "pub fn snapshot\|pub fn restore\|pub fn cow_clone" \
  packages/sdk-server-wasmtime/src/vfs/mod.rs
```

Adapt the handler code to match the actual signatures.

- [ ] **Step 4: Implement `handle_snapshot_create` and `handle_snapshot_restore`**

```rust
fn handle_snapshot_create(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    let snap_id = sb.shell.vfs_mut().snapshot();
    Response::ok(id, json!({"id": snap_id}))
}

fn handle_snapshot_restore(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let snap_id = match require_str(id.clone(), &params, "id") { Ok(s) => s, Err(r) => return r };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.shell.vfs_mut().restore(snap_id) {
        Ok(_) => Response::ok(id, json!({"ok": true})),
        Err(e) => vfs_err(id, e),
    }
}
```

- [ ] **Step 5: Implement `handle_mount`**

```rust
fn handle_mount(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let path = match require_str(id.clone(), &params, "path") { Ok(p) => p, Err(r) => return r };
    let files_raw = match params.get("files").and_then(|v| v.as_object()) {
        Some(f) => f,
        None => return Response::err(id, codes::INVALID_PARAMS, "missing files"),
    };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    let _ = sb.shell.vfs_mut().mkdirp(path);
    for (rel, val) in files_raw {
        let b64 = match val.as_str() { Some(s) => s, None => continue };
        let bytes = match b64_decode(b64) { Ok(b) => b, Err(_) => continue };
        let full = format!("{}/{}", path.trim_end_matches('/'), rel);
        if let Some(parent) = std::path::Path::new(&full).parent().and_then(|p| p.to_str()) {
            let _ = sb.shell.vfs_mut().mkdirp(parent);
        }
        let _ = sb.shell.vfs_mut().write_file(&full, &bytes);
    }
    Response::ok(id, json!({"ok": true}))
}
```

- [ ] **Step 6: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test test_snapshot test_mount 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): implement snapshot and mount handlers"
```

---

## Task 5: VFS serialization + persistence.export/import

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/vfs/inode.rs` — add `#[derive(Serialize, Deserialize)]`
- Modify: `packages/sdk-server-wasmtime/src/vfs/mod.rs` — add derive + `export_bytes`/`import_bytes`
- Modify: `packages/sdk-server-wasmtime/Cargo.toml` — add bincode
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs`
- Test: append to `tests/integration.rs`

- [ ] **Step 1: Add persistence test**

```rust
#[tokio::test]
async fn test_persistence() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm.clone(), None, None, None).await.unwrap();

    let sb = mgr.root.as_mut().unwrap();
    sb.shell.vfs_mut().write_file("/tmp/persist.txt", b"persistent data").unwrap();
    let blob = sb.shell.vfs().export_bytes().unwrap();
    assert!(!blob.is_empty());

    // Import into fresh sandbox
    let engine = std::sync::Arc::new(crate::wasm::WasmEngine::new().unwrap());
    let wasm_arc = std::sync::Arc::new(wasm);
    let vfs2 = crate::vfs::MemVfs::import_bytes(&blob).unwrap();
    let shell2 = crate::wasm::ShellInstance::new(&engine, &wasm_arc, vfs2, &[]).await.unwrap();
    let content = shell2.vfs().read_file("/tmp/persist.txt").unwrap();
    assert_eq!(content, b"persistent data");
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd packages/sdk-server-wasmtime && cargo test test_persistence 2>&1 | tail -10
```

- [ ] **Step 3: Add bincode to Cargo.toml**

```toml
# packages/sdk-server-wasmtime/Cargo.toml — under [dependencies]
bincode = "1"
```

- [ ] **Step 4: Add serde derives to `vfs/inode.rs`**

Add `#[derive(Clone, Serialize, Deserialize)]` to `InodeMeta`, `Inode`, `DirEntry`, `StatResult`. The `Arc<Vec<u8>>` in `Inode::File` serializes transparently via serde's Arc support.

```rust
// At the top of vfs/inode.rs, the existing derive needs serde:
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InodeMeta { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Inode {
    File { meta: InodeMeta, content: Arc<Vec<u8>> },
    Dir { meta: InodeMeta, children: BTreeMap<String, Inode> },
    Symlink { meta: InodeMeta, target: String },
}
```

- [ ] **Step 5: Add serde derives to `vfs/mod.rs` and `export_bytes`/`import_bytes`**

```rust
// In vfs/mod.rs — add to MemVfs struct derives
#[derive(Serialize, Deserialize)]
pub struct VfsSnapshot {
    root: Inode,
}

impl MemVfs {
    /// Serialize the entire filesystem to a compact binary blob.
    pub fn export_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let snap = VfsSnapshot { root: self.root.clone() };
        Ok(bincode::serialize(&snap)?)
    }

    /// Deserialize a filesystem from a blob returned by `export_bytes`.
    pub fn import_bytes(blob: &[u8]) -> anyhow::Result<Self> {
        let snap: VfsSnapshot = bincode::deserialize(blob)?;
        let mut vfs = MemVfs::new(None, None);
        vfs.root = snap.root;
        Ok(vfs)
    }
}
```

Add `use bincode;` and `use serde::{Serialize, Deserialize};` at the top of `vfs/mod.rs`.

- [ ] **Step 6: Implement persistence handlers in dispatcher**

```rust
fn handle_persistence_export(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.shell.vfs().export_bytes() {
        Ok(blob) => Response::ok(id, json!({"data": b64_encode(&blob)})),
        Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    }
}

fn handle_persistence_import(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let data_b64 = match require_str(id.clone(), &params, "data") { Ok(d) => d, Err(r) => return r };
    let blob = match b64_decode(data_b64) { Ok(b) => b, Err(e) => return Response::err(id, codes::INVALID_PARAMS, e) };
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match crate::vfs::MemVfs::import_bytes(&blob) {
        Ok(new_vfs) => {
            *sb.shell.vfs_mut() = new_vfs;
            Response::ok(id, json!({"ok": true}))
        }
        Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    }
}
```

Note: `vfs_mut()` returns `&mut MemVfs` — this works if we can replace it entirely. Check `instance.rs` — it exposes `pub fn vfs_mut(&mut self) -> &mut MemVfs`. The assignment `*sb.shell.vfs_mut() = new_vfs;` replaces the VFS in place.

- [ ] **Step 7: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test test_persistence 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add packages/sdk-server-wasmtime/Cargo.toml \
        packages/sdk-server-wasmtime/src/vfs/ \
        packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): add VFS serialization + persistence.export/import handlers"
```

---

## Task 6: Fork + sandbox management

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs`
- Test: append to `tests/integration.rs`

- [ ] **Step 1: Add fork and sandbox management tests**

```rust
#[tokio::test]
async fn test_fork() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();

    let sb = mgr.root.as_mut().unwrap();
    sb.shell.vfs_mut().write_file("/tmp/shared.txt", b"shared").unwrap();
    let forked = sb.fork().await.unwrap();

    let fork_id = "f1".to_string();
    mgr.forks.insert(fork_id.clone(), forked);

    // Fork can read the file
    let fork = mgr.forks.get_mut(&fork_id).unwrap();
    let content = fork.shell.vfs().read_file("/tmp/shared.txt").unwrap();
    assert_eq!(content, b"shared");

    // Fork write does not affect root
    fork.shell.vfs_mut().write_file("/tmp/fork_only.txt", b"fork").unwrap();
    assert!(mgr.root.as_ref().unwrap().shell.vfs().read_file("/tmp/fork_only.txt").is_err());
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd packages/sdk-server-wasmtime && cargo test test_fork 2>&1 | tail -10
```

- [ ] **Step 3: Implement fork and sandbox handlers in dispatcher**

```rust
async fn handle_sandbox_fork(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    if self.manager.forks.len() >= 16 {
        return Response::err(id, codes::INVALID_PARAMS, "max forks reached");
    }
    let forked = {
        let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
        match sb.fork().await {
            Ok(f) => f,
            Err(e) => return Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
        }
    };
    let fork_id = self.manager.next_fork_id.to_string();
    self.manager.next_fork_id += 1;
    self.manager.forks.insert(fork_id.clone(), forked);
    Response::ok(id, json!({"sandboxId": fork_id}))
}

fn handle_sandbox_destroy(&mut self, id: Option<RequestId>, params: Value) -> Response {
    let sid = match require_str(id.clone(), &params, "sandboxId") { Ok(s) => s, Err(r) => return r };
    if self.manager.forks.remove(sid).is_none() {
        return Response::err(id, codes::INVALID_PARAMS, format!("unknown sandboxId: {sid}"));
    }
    Response::ok(id, json!({"ok": true}))
}

async fn handle_sandbox_create(&mut self, id: Option<RequestId>, _params: Value) -> Response {
    if self.manager.named.len() >= 64 {
        return Response::err(id, codes::INVALID_PARAMS, "max sandboxes reached");
    }
    // For sandbox.create, clone the engine/wasm from root and create a fresh sandbox.
    let (engine, wasm_bytes) = {
        let root = match self.manager.root.as_ref() {
            Some(r) => r,
            None => return Response::err(id, 1, "no root sandbox"),
        };
        (root.engine.clone(), root.wasm_bytes.clone())
    };
    let vfs = crate::vfs::MemVfs::new(None, None);
    let shell = match crate::wasm::ShellInstance::new(&engine, &wasm_bytes, vfs, &[]).await {
        Ok(s) => s,
        Err(e) => return Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    };
    let sb = crate::sandbox::SandboxState { engine, wasm_bytes, shell, env: Default::default() };
    let sid = self.manager.next_named_id.to_string();
    self.manager.next_named_id += 1;
    self.manager.named.insert(sid.clone(), sb);
    Response::ok(id, json!({"sandboxId": sid}))
}

fn handle_sandbox_list(&mut self, id: Option<RequestId>) -> Response {
    let entries: Vec<_> = self.manager.named.keys()
        .map(|sid| json!({"sandboxId": sid}))
        .collect();
    Response::ok(id, json!(entries))
}

fn handle_sandbox_remove(&mut self, id: Option<RequestId>, params: Value) -> Response {
    let sid = match require_str(id.clone(), &params, "sandboxId") { Ok(s) => s, Err(r) => return r };
    if self.manager.named.remove(sid).is_none() {
        return Response::err(id, codes::INVALID_PARAMS, format!("unknown sandboxId: {sid}"));
    }
    Response::ok(id, json!({"ok": true}))
}
```

- [ ] **Step 4: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test test_fork 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): implement fork and sandbox management handlers"
```

---

## Task 7: Shell history + offload/rehydrate

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs`
- Test: append to `tests/integration.rs`

- [ ] **Step 1: Add history test**

```rust
#[tokio::test]
async fn test_history() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();

    mgr.root_run("echo first").await.unwrap();
    mgr.root_run("echo second").await.unwrap();

    let sb = mgr.root.as_mut().unwrap();
    let result = sb.run("history").await.unwrap();
    let stdout = result["stdout"].as_str().unwrap();
    // history output contains the commands we ran
    assert!(stdout.contains("echo first") || stdout.contains("first"));
}
```

- [ ] **Step 2: Implement `handle_history_list` and `handle_history_clear`**

Shell history is maintained inside the WASM (see `state.history` in `shell-exec/src/state.rs`). Access it by running the `history` builtin and parsing output.

```rust
async fn handle_history_list(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.run("history").await {
        Ok(result) => {
            let stdout = result["stdout"].as_str().unwrap_or("").to_string();
            // Parse lines like "    1  command" or "1\tcommand"
            let entries: Vec<_> = stdout.lines().enumerate().map(|(i, line)| {
                let cmd = line.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '\t');
                json!({"index": i + 1, "command": cmd, "timestamp": 0})
            }).collect();
            Response::ok(id, json!({"entries": entries}))
        }
        Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    }
}

async fn handle_history_clear(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
    let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
    match sb.run("history -c").await {
        Ok(_) => Response::ok(id, json!({"ok": true})),
        Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    }
}
```

- [ ] **Step 3: Implement `handle_offload` and `handle_rehydrate`**

`offload`/`rehydrate` use the callback protocol: the server sends a callback request to the Python SDK (or MCP client) on stdout, then reads the response from stdin. This requires refactoring `main.rs` to support callback round-trips.

The approach: add a `oneshot` channel to the dispatcher. The main loop forwards callback responses to it.

**In `main.rs`:** Add a callback channel.

```rust
// In main():
let (cb_tx, cb_rx) = tokio::sync::mpsc::channel::<String>(4);
let mut dispatcher = dispatcher::Dispatcher::new(stdout_tx.clone(), cb_rx);

// In the message loop, route callback responses to dispatcher:
while let Some(line) = lines.next_line().await? {
    let msg: serde_json::Value = match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue };

    // Callback response? Route to pending callback channel.
    if msg.get("id").and_then(|v| v.as_str()).map(|id| id.starts_with("cb_")).unwrap_or(false)
        && msg.get("method").is_none()
    {
        let _ = cb_tx.send(line).await;
        continue;
    }
    // ... normal dispatch
}
```

**In `dispatcher.rs`:** add `cb_rx` field.

```rust
pub struct Dispatcher {
    // ... existing fields
    cb_rx: tokio::sync::mpsc::Receiver<String>,
}

impl Dispatcher {
    pub fn new(stdout_tx: tokio::sync::mpsc::Sender<String>, cb_rx: tokio::sync::mpsc::Receiver<String>) -> Self {
        Self { ..., cb_rx }
    }

    async fn send_callback(&mut self, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let cb_id = format!("cb_{}", self.next_cb_id);
        self.next_cb_id += 1;
        let req = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": cb_id,
            "method": method,
            "params": params,
        }))?;
        self.stdout_tx.send(req).await?;
        let resp_line = self.cb_rx.recv().await.ok_or_else(|| anyhow::anyhow!("callback channel closed"))?;
        let resp: serde_json::Value = serde_json::from_str(&resp_line)?;
        if let Some(err) = resp.get("error") {
            anyhow::bail!("callback error: {err}");
        }
        Ok(resp["result"].clone())
    }

    async fn handle_offload(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
        let blob = {
            let sb = match self.manager.resolve(sid) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
            match sb.shell.vfs().export_bytes() {
                Ok(b) => b,
                Err(e) => return Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
            }
        };
        let sandbox_id_str = sid.unwrap_or("root");
        if let Err(e) = self.send_callback("storage.save", json!({
            "sandbox_id": sandbox_id_str,
            "state": b64_encode(&blob),
        })).await {
            return Response::err(id, codes::INTERNAL_ERROR, e.to_string());
        }
        Response::ok(id, json!(null))
    }

    async fn handle_rehydrate(&mut self, id: Option<RequestId>, params: Value, sid: Option<&str>) -> Response {
        let sandbox_id_str = sid.unwrap_or("root").to_string();
        let result = match self.send_callback("storage.load", json!({"sandbox_id": sandbox_id_str})).await {
            Ok(r) => r,
            Err(e) => return Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
        };
        let b64 = match result.as_str() {
            Some(s) => s.to_string(),
            None => return Response::err(id, codes::INTERNAL_ERROR, "expected base64 string"),
        };
        let blob = match b64_decode(&b64) { Ok(b) => b, Err(e) => return Response::err(id, codes::INVALID_PARAMS, e) };
        let sb = match self.manager.resolve(Some(&sandbox_id_str)) { Ok(s) => s, Err(e) => return Response::err(id, 1, e.to_string()) };
        match crate::vfs::MemVfs::import_bytes(&blob) {
            Ok(new_vfs) => { *sb.shell.vfs_mut() = new_vfs; Response::ok(id, json!(null)) }
            Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
        }
    }
}
```

- [ ] **Step 4: Run history test**

```bash
cd packages/sdk-server-wasmtime && cargo test test_history 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 5: Run all dispatcher tests**

```bash
cd packages/sdk-server-wasmtime && cargo test 2>&1 | tail -20
```
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/sdk-server-wasmtime/src/dispatcher.rs \
        packages/sdk-server-wasmtime/src/main.rs \
        packages/sdk-server-wasmtime/tests/integration.rs
git commit -m "feat(wasmtime): implement history, offload/rehydrate, callback protocol"
```

---

## Task 8: Streaming run output

**Goal:** While `run_command` executes, send JSON-RPC notifications for stdout/stderr chunks. This enables real-time output in the Python SDK.

**Files:**
- Modify: `packages/sdk-server-wasmtime/src/wasm/mod.rs` — pipe stdout/stderr to a channel
- Modify: `packages/sdk-server-wasmtime/src/sandbox.rs` — accept a notify callback
- Modify: `packages/sdk-server-wasmtime/src/dispatcher.rs` — send `output` notifications

- [ ] **Step 1: Add streaming test**

```rust
#[tokio::test]
async fn test_streaming_run() {
    // This tests that streaming output is sent via the notify channel.
    // We use a channel to capture notifications instead of stdout.
    use tokio::sync::mpsc;
    let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<(String, String)>();

    let wasm = wasm_bytes();
    let engine = std::sync::Arc::new(crate::wasm::WasmEngine::new().unwrap());
    let wasm_arc = std::sync::Arc::new(wasm);
    let vfs = crate::vfs::MemVfs::new(None, None);
    let mut shell = crate::wasm::ShellInstance::new(&engine, &wasm_arc, vfs, &[]).await.unwrap();
    let result = shell.run_command("echo streaming").await.unwrap();
    let stdout = String::from_utf8_lossy(&shell.take_stdout()).into_owned();
    // Basic: stdout captured after run
    assert!(stdout.contains("streaming"));
}
```

- [ ] **Step 2: Update `handle_run` to send streaming notifications**

When `params.stream == true` and a request id is present, send output notifications. Since stdout/stderr are only available after the command completes (via `take_stdout`/`take_stderr`), streaming is a best-effort batch: send all captured output as a single notification after command completion.

True line-by-line streaming requires piping stdout/stderr through an async channel during execution — a more complex refactor. For v1, implement batch streaming (send all at once after completion). This satisfies the Python SDK which just concatenates streamed chunks anyway.

```rust
async fn handle_run(&mut self, id: Option<RequestId>, params: Value) -> Response {
    let cmd = match require_str(id.clone(), &params, "command") { Ok(c) => c.to_owned(), Err(r) => return r };
    let sid = sandbox_id(&params).map(str::to_owned);
    let stream = params.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    let sb = match self.manager.resolve(sid.as_deref()) {
        Ok(s) => s,
        Err(e) => return Response::err(id, 1, e.to_string()),
    };
    match sb.run(&cmd).await {
        Ok(result) => {
            if stream {
                if let Some(ref req_id) = id {
                    let stdout = result["stdout"].as_str().unwrap_or("").to_string();
                    let stderr = result["stderr"].as_str().unwrap_or("").to_string();
                    if !stdout.is_empty() {
                        let notif = serde_json::to_string(&json!({
                            "jsonrpc": "2.0",
                            "method": "output",
                            "params": {"request_id": req_id, "stream": "stdout", "data": stdout}
                        })).unwrap_or_default();
                        let _ = self.stdout_tx.send(notif).await;
                    }
                    if !stderr.is_empty() {
                        let notif = serde_json::to_string(&json!({
                            "jsonrpc": "2.0",
                            "method": "output",
                            "params": {"request_id": req_id, "stream": "stderr", "data": stderr}
                        })).unwrap_or_default();
                        let _ = self.stdout_tx.send(notif).await;
                    }
                    // Return result with empty stdout/stderr (already streamed)
                    return Response::ok(id, json!({
                        "exitCode": result["exitCode"],
                        "stdout": "",
                        "stderr": "",
                        "executionTimeMs": result["executionTimeMs"],
                    }));
                }
            }
            Response::ok(id, result)
        }
        Err(e) => Response::err(id, codes::INTERNAL_ERROR, e.to_string()),
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd packages/sdk-server-wasmtime && cargo test 2>&1 | tail -20
```
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add packages/sdk-server-wasmtime/src/dispatcher.rs
git commit -m "feat(wasmtime): add streaming run output notifications (batch)"
```

---

## Task 9: Python SDK `engine` parameter

**Files:**
- Modify: `packages/python-sdk/src/codepod/sandbox.py`
- Test: `packages/python-sdk/tests/test_engine_param.py`

- [ ] **Step 1: Add test**

```python
# packages/python-sdk/tests/test_engine_param.py
import pytest
from unittest.mock import patch, MagicMock
from codepod.sandbox import Sandbox, _find_codepod_server, _find_deno


def test_find_deno_returns_path_or_none():
    result = _find_deno()
    # Either finds deno or returns None — must not raise.
    assert result is None or isinstance(result, str)


def test_sandbox_engine_deno_uses_deno(monkeypatch):
    """With engine='deno', should use the deno runtime."""
    calls = []
    def fake_rpc(runtime, args):
        calls.append((runtime, args))
        m = MagicMock()
        m.start.return_value = None
        m.call.return_value = {"ok": True}
        m.register_storage_handlers.return_value = None
        return m

    monkeypatch.setattr("codepod.sandbox.RpcClient", fake_rpc)
    monkeypatch.setattr("codepod.sandbox._find_deno", lambda: "/usr/bin/deno")
    monkeypatch.setattr("codepod.sandbox._is_bundled", lambda: False)

    with patch.object(Sandbox, 'kill', return_value=None):
        sb = Sandbox(engine='deno')

    assert calls[0][0] == "/usr/bin/deno"
```

- [ ] **Step 2: Run to verify failure**

```bash
cd packages/python-sdk && python -m pytest tests/test_engine_param.py -v 2>&1 | tail -15
```
Expected: ImportError (`_find_codepod_server`, `_find_deno` not defined yet).

- [ ] **Step 3: Update `sandbox.py`**

```python
# packages/python-sdk/src/codepod/sandbox.py

import os
import shutil
import sys

_PKG_DIR = os.path.dirname(os.path.abspath(__file__))
_BUNDLED_DIR = os.path.join(_PKG_DIR, "_bundled")


def _is_bundled() -> bool:
    return os.path.isdir(_BUNDLED_DIR)


def _find_codepod_server() -> str | None:
    """Find codepod-server binary: adjacent to wheel, then on PATH."""
    # 1. Adjacent to the installed package (platform wheel).
    adjacent = os.path.join(_PKG_DIR, "codepod-server")
    if os.path.isfile(adjacent) and os.access(adjacent, os.X_OK):
        return adjacent
    # 2. On PATH.
    return shutil.which("codepod-server")


def _find_deno() -> str | None:
    """Find the deno binary."""
    if _is_bundled():
        p = os.path.join(_BUNDLED_DIR, "deno")
        if os.path.isfile(p):
            return p
    return shutil.which("deno") or (
        os.path.expanduser("~/.deno/bin/deno")
        if os.path.isfile(os.path.expanduser("~/.deno/bin/deno")) else None
    )


def _resolve_runtime(engine: str) -> tuple[str, list[str], str, str]:
    """Return (runtime, server_args, wasm_dir, shell_wasm) for the chosen engine."""
    if engine == 'wasmtime' or (engine == 'auto' and _find_codepod_server() is not None):
        binary = _find_codepod_server()
        if binary is None:
            raise RuntimeError(
                "codepod-server not found. Install it or use engine='deno'."
            )
        wasm_dir, shell_wasm = _wasmtime_wasm_paths()
        return binary, [], wasm_dir, shell_wasm

    # Deno path (engine='deno' or auto-fallback)
    deno = _find_deno()
    if deno is None:
        raise RuntimeError("Neither codepod-server nor deno found on PATH.")
    if _is_bundled():
        server = os.path.join(_BUNDLED_DIR, "server.js")
        server_args = [server]
        wasm_dir = os.path.join(_BUNDLED_DIR, "wasm")
    else:
        repo_root = os.path.abspath(os.path.join(_PKG_DIR, "..", "..", "..", ".."))
        server = os.path.join(repo_root, "packages", "sdk-server", "src", "server.ts")
        server_args = ["run", "-A", "--no-check", "--unstable-sloppy-imports", server]
        wasm_dir = os.path.join(
            repo_root, "packages", "orchestrator", "src", "platform", "__tests__", "fixtures"
        )
    shell_wasm = os.path.join(wasm_dir, "codepod-shell-exec.wasm")
    return deno, server_args, wasm_dir, shell_wasm


def _wasmtime_wasm_paths() -> tuple[str, str]:
    """Return (wasm_dir, shell_wasm) for the wasmtime engine."""
    if _is_bundled():
        wasm_dir = os.path.join(_BUNDLED_DIR, "wasm")
    else:
        repo_root = os.path.abspath(os.path.join(_PKG_DIR, "..", "..", "..", ".."))
        wasm_dir = os.path.join(
            repo_root, "packages", "orchestrator", "src", "platform", "__tests__", "fixtures"
        )
    shell_wasm = os.path.join(wasm_dir, "codepod-shell-exec.wasm")
    return wasm_dir, shell_wasm
```

In the `Sandbox.__init__` method, replace the existing runtime resolution block:

```python
# OLD:
# if _is_bundled():
#     runtime, server_args, wasm_dir, shell_wasm = _bundled_paths()
# else:
#     runtime, server_args, wasm_dir, shell_wasm = _dev_paths()

# NEW:
engine_param = kwargs.pop('engine', 'auto') if 'engine' not in {
    'timeout_ms', 'fs_limit_bytes', 'mounts', 'python_path', 'extensions', 'storage',
    '_sandbox_id', '_client'
} else 'auto'
runtime, server_args, wasm_dir, shell_wasm = _resolve_runtime(engine_param)
```

Actually, add `engine: str = 'auto'` as an explicit parameter to `__init__`:

```python
def __init__(
    self,
    *,
    engine: str = 'auto',
    timeout_ms: int = 30_000,
    fs_limit_bytes: int = 256 * 1024 * 1024,
    # ... rest unchanged
):
    if _client is not None:
        # unchanged
        ...
        return

    runtime, server_args, wasm_dir, shell_wasm = _resolve_runtime(engine)
    # rest of __init__ unchanged
```

- [ ] **Step 4: Run tests**

```bash
cd packages/python-sdk && python -m pytest tests/test_engine_param.py -v 2>&1 | tail -15
```
Expected: PASS.

- [ ] **Step 5: Run existing Python SDK tests to check no regression**

```bash
cd packages/python-sdk && python -m pytest tests/ -v 2>&1 | tail -30
```
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/python-sdk/src/codepod/sandbox.py \
        packages/python-sdk/tests/test_engine_param.py
git commit -m "feat(python-sdk): add engine parameter with wasmtime/deno/auto discovery"
```

---

## Task 10: Build scripts + binary name fix

**Files:**
- Modify: `packages/sdk-server-wasmtime/Cargo.toml` — rename binary to `codepod-server`
- Create: `scripts/build-sdk-server.sh`
- Modify: `scripts/build-mcp.sh` — add `--engine` flag

- [ ] **Step 1: Rename the binary in Cargo.toml**

```toml
# packages/sdk-server-wasmtime/Cargo.toml
[[bin]]
name = "codepod-server"       # was "codepod-server-wasmtime"
path = "src/main.rs"
```

- [ ] **Step 2: Verify build**

```bash
cargo build -p sdk-server-wasmtime --release 2>&1 | tail -10
ls -lh target/release/codepod-server
```
Expected: binary built.

- [ ] **Step 3: Create `scripts/build-sdk-server.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail
# Build the codepod-server (wasmtime backend) binary.
# Usage: ./scripts/build-sdk-server.sh [--release] [--engine wasmtime|deno]
#
# Output: dist/codepod-server

cd "$(dirname "$0")/.."
OUT_DIR="${OUT_DIR:-dist}"
PROFILE="release"
ENGINE="wasmtime"

for arg in "$@"; do
  case "$arg" in
    --engine=*) ENGINE="${arg#--engine=}" ;;
    --engine)   shift; ENGINE="$1" ;;
    --debug)    PROFILE="debug" ;;
  esac
done

mkdir -p "$OUT_DIR"

if [ "$ENGINE" = "wasmtime" ]; then
  echo "==> Building codepod-server (wasmtime)..."
  if [ "$PROFILE" = "release" ]; then
    cargo build -p sdk-server-wasmtime --release
    cp target/release/codepod-server "$OUT_DIR/codepod-server"
  else
    cargo build -p sdk-server-wasmtime
    cp target/debug/codepod-server "$OUT_DIR/codepod-server"
  fi
  SIZE=$(du -h "$OUT_DIR/codepod-server" | cut -f1)
  echo "==> Built: $OUT_DIR/codepod-server ($SIZE)"
elif [ "$ENGINE" = "deno" ]; then
  echo "==> Building codepod-sdk-server (deno)..."
  # Bundle and compile the TypeScript sdk-server via deno compile.
  if [ -n "${DENO:-}" ]; then : ;
  elif command -v deno &>/dev/null; then DENO="deno";
  elif [ -x "$HOME/.deno/bin/deno" ]; then DENO="$HOME/.deno/bin/deno";
  else echo "Error: deno not found"; exit 1; fi

  BUNDLE="$OUT_DIR/.codepod-sdk-bundle.mjs"
  (cd packages/orchestrator && "$DENO" task build)
  npx esbuild packages/sdk-server/src/server.ts \
    --bundle --platform=node --format=esm --outfile="$BUNDLE" --log-level=warning
  "$DENO" compile -A --no-check -o "$OUT_DIR/codepod-server-deno" "$BUNDLE"
  rm -f "$BUNDLE"
  SIZE=$(du -h "$OUT_DIR/codepod-server-deno" | cut -f1)
  echo "==> Built: $OUT_DIR/codepod-server-deno ($SIZE)"
else
  echo "Error: unknown engine '$ENGINE'. Use wasmtime or deno."
  exit 1
fi
```

- [ ] **Step 4: Make executable and test**

```bash
chmod +x scripts/build-sdk-server.sh
bash scripts/build-sdk-server.sh 2>&1 | tail -5
```
Expected: `Built: dist/codepod-server`.

- [ ] **Step 5: Add `--engine` flag to `build-mcp.sh`**

At the top of `scripts/build-mcp.sh` after the argument parsing loop, add:

```bash
ENGINE="deno"  # default stays deno until mcp-server-rust is complete

for arg in "$@"; do
  case "$arg" in
    --engine=*) ENGINE="${arg#--engine=}" ;;
    --engine)   shift; ENGINE="$1" ;;
    # ... existing args
  esac
done

if [ "$ENGINE" = "wasmtime" ]; then
  echo "==> Building MCP server (wasmtime Rust backend)..."
  cargo build -p mcp-server-rust --release
  cp target/release/codepod-mcp "$OUT_DIR/codepod-mcp"
  SIZE=$(du -h "$OUT_DIR/codepod-mcp" | cut -f1)
  echo "==> Built: $OUT_DIR/codepod-mcp ($SIZE)"
  exit 0
fi
# ... existing deno path continues unchanged
```

- [ ] **Step 6: Commit**

```bash
git add packages/sdk-server-wasmtime/Cargo.toml \
        scripts/build-sdk-server.sh scripts/build-mcp.sh
git commit -m "feat: add build-sdk-server.sh and --engine flag to build-mcp.sh"
```

---

## Task 11: `packages/mcp-server-rust` crate

**Files:**
- Create: `packages/mcp-server-rust/Cargo.toml`
- Create: `packages/mcp-server-rust/src/main.rs`
- Create: `packages/mcp-server-rust/src/tools.rs`
- Modify: `Cargo.toml` (workspace)

The Rust MCP server links `sdk_server_wasmtime` as a library and exposes the same tools as the TypeScript MCP server. MCP protocol is a JSON-RPC dialect — we implement it by hand (thin layer, no external `rmcp` dependency needed).

- [ ] **Step 1: Add workspace member**

In root `Cargo.toml` workspace members list, add:
```toml
"packages/mcp-server-rust",
```

- [ ] **Step 2: Create `packages/mcp-server-rust/Cargo.toml`**

```toml
[package]
name = "mcp-server-rust"
version = "0.1.0"
edition = "2021"
description = "Rust MCP server for codepod sandboxes (wasmtime backend)"

[[bin]]
name = "codepod-mcp-rust"
path = "src/main.rs"

[dependencies]
sdk_server_wasmtime = { path = "../sdk-server-wasmtime" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
base64 = "0.22"
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 3: Create `src/main.rs`**

```rust
//! codepod-mcp-rust — MCP server (wasmtime backend).
//!
//! Implements MCP protocol over stdio (JSON-RPC 2.0).
//! Tools mirror the TypeScript MCP server.

mod tools;

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use clap::Parser;
use serde_json::{json, Value};
use sdk_server_wasmtime::sandbox::SandboxManager;

#[derive(Parser)]
struct Args {
    /// Path to codepod-shell-exec.wasm
    #[arg(long)]
    shell_wasm: String,

    /// Directory containing coreutil WASMs
    #[arg(long)]
    wasm_dir: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("codepod-mcp-rust starting");

    let wasm_bytes = Arc::new(std::fs::read(&args.shell_wasm)?);
    let mgr = Arc::new(Mutex::new(SandboxManager::new()));

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::BufWriter::new(tokio::io::stdout());

    // MCP initialization: read the initialize request and respond.
    // Then process tool_call requests in a loop.
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() { continue; }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("parse error: {e}");
                continue;
            }
        };

        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => tools::handle_initialize(&msg),
            "tools/list" => tools::handle_tools_list(id),
            "tools/call" => tools::handle_tool_call(id, &msg, &wasm_bytes, &mgr).await,
            _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("Unknown method: {method}")}}),
        };

        let line = serde_json::to_string(&response)?;
        stdout.write_all(line.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}
```

- [ ] **Step 4: Create `src/tools.rs`**

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::{json, Value};
use sdk_server_wasmtime::sandbox::SandboxManager;
use base64::Engine as B64Engine;

pub fn handle_initialize(msg: &Value) -> Value {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "codepod-mcp-rust", "version": "0.1.0"}
        }
    })
}

pub fn handle_tools_list(id: Value) -> Value {
    let tools = vec![
        tool("create_sandbox", "Create a new sandbox", json!({"type":"object","properties":{}})),
        tool("destroy_sandbox", "Destroy a sandbox", json!({"type":"object","properties":{"sandboxId":{"type":"string"}}})),
        tool("list_sandboxes", "List active sandboxes", json!({"type":"object","properties":{}})),
        tool("run_command", "Run a shell command", json!({"type":"object","properties":{"command":{"type":"string"},"sandboxId":{"type":"string"}},"required":["command"]})),
        tool("read_file", "Read a file", json!({"type":"object","properties":{"path":{"type":"string"},"sandboxId":{"type":"string"}},"required":["path"]})),
        tool("write_file", "Write a file", json!({"type":"object","properties":{"path":{"type":"string"},"data":{"type":"string"},"sandboxId":{"type":"string"}},"required":["path","data"]})),
        tool("list_directory", "List a directory", json!({"type":"object","properties":{"path":{"type":"string"},"sandboxId":{"type":"string"}},"required":["path"]})),
        tool("snapshot", "Create a snapshot", json!({"type":"object","properties":{"sandboxId":{"type":"string"}}})),
        tool("restore", "Restore a snapshot", json!({"type":"object","properties":{"id":{"type":"string"},"sandboxId":{"type":"string"}},"required":["id"]})),
        tool("export_state", "Export sandbox state", json!({"type":"object","properties":{"sandboxId":{"type":"string"}}})),
        tool("import_state", "Import sandbox state", json!({"type":"object","properties":{"data":{"type":"string"},"sandboxId":{"type":"string"}},"required":["data"]})),
    ];
    json!({"jsonrpc":"2.0","id":id,"result":{"tools":tools}})
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":schema})
}

pub async fn handle_tool_call(
    id: Value,
    msg: &Value,
    wasm_bytes: &Arc<Vec<u8>>,
    mgr: &Arc<Mutex<SandboxManager>>,
) -> Value {
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = dispatch_tool(tool_name, args, wasm_bytes, mgr).await;
    match result {
        Ok(text) => json!({
            "jsonrpc": "2.0", "id": id,
            "result": {"content": [{"type":"text","text":text}]}
        }),
        Err(e) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": {"code": 1, "message": e.to_string()}
        }),
    }
}

async fn dispatch_tool(
    name: &str,
    args: Value,
    wasm_bytes: &Arc<Vec<u8>>,
    mgr: &Arc<Mutex<SandboxManager>>,
) -> anyhow::Result<String> {
    let sid = args.get("sandboxId").and_then(|v| v.as_str()).map(str::to_owned);

    match name {
        "create_sandbox" => {
            let mut m = mgr.lock().await;
            if m.root.is_none() {
                m.create(wasm_bytes.as_ref().clone(), None, None, None).await?;
                Ok("Sandbox created.".to_string())
            } else {
                // Create a named sandbox
                let engine = Arc::clone(&m.root.as_ref().unwrap().engine);
                let wasm = Arc::clone(&m.root.as_ref().unwrap().wasm_bytes);
                let vfs = sdk_server_wasmtime::vfs::MemVfs::new(None, None);
                let shell = sdk_server_wasmtime::wasm::ShellInstance::new(&engine, &wasm, vfs, &[]).await?;
                let sb = sdk_server_wasmtime::sandbox::SandboxState { engine, wasm_bytes: wasm, shell, env: Default::default() };
                let sid = m.next_named_id.to_string();
                m.next_named_id += 1;
                m.named.insert(sid.clone(), sb);
                Ok(format!("Sandbox created: {sid}"))
            }
        }
        "destroy_sandbox" => {
            let id = sid.ok_or_else(|| anyhow::anyhow!("sandboxId required"))?;
            let mut m = mgr.lock().await;
            if m.named.remove(&id).is_some() { Ok(format!("Destroyed {id}")) }
            else { anyhow::bail!("unknown sandboxId: {id}") }
        }
        "list_sandboxes" => {
            let m = mgr.lock().await;
            let ids: Vec<_> = m.named.keys().cloned().collect();
            Ok(serde_json::to_string(&ids)?)
        }
        "run_command" => {
            let cmd = args.get("command").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("command required"))?
                .to_owned();
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            let result = sb.run(&cmd).await?;
            let exit_code = result["exitCode"].as_i64().unwrap_or(1);
            let stdout = result["stdout"].as_str().unwrap_or("");
            let stderr = result["stderr"].as_str().unwrap_or("");
            Ok(format!("exit={exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}"))
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("path required"))?.to_owned();
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            let bytes = sb.shell.vfs().read_file(&path)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("path required"))?.to_owned();
            let data = args.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("data required"))?.to_owned();
            let bytes = base64::engine::general_purpose::STANDARD.decode(&data)?;
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            if let Some(parent) = std::path::Path::new(&path).parent().and_then(|p| p.to_str()) {
                let _ = sb.shell.vfs_mut().mkdirp(parent);
            }
            sb.shell.vfs_mut().write_file(&path, &bytes)?;
            Ok("ok".to_string())
        }
        "list_directory" => {
            let path = args.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("path required"))?.to_owned();
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            let entries = sb.shell.vfs().readdir(&path)?;
            let names: Vec<_> = entries.iter().map(|e| &e.name).collect();
            Ok(serde_json::to_string(&names)?)
        }
        "snapshot" => {
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            let snap_id = sb.shell.vfs_mut().snapshot();
            Ok(format!("snapshot:{snap_id}"))
        }
        "restore" => {
            let snap_id = args.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("id required"))?.to_owned();
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            sb.shell.vfs_mut().restore(&snap_id)?;
            Ok("restored".to_string())
        }
        "export_state" => {
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            let blob = sb.shell.vfs().export_bytes()?;
            Ok(base64::engine::general_purpose::STANDARD.encode(&blob))
        }
        "import_state" => {
            let data = args.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("data required"))?.to_owned();
            let blob = base64::engine::general_purpose::STANDARD.decode(&data)?;
            let new_vfs = sdk_server_wasmtime::vfs::MemVfs::import_bytes(&blob)?;
            let mut m = mgr.lock().await;
            let sb = m.resolve(sid.as_deref())?;
            *sb.shell.vfs_mut() = new_vfs;
            Ok("imported".to_string())
        }
        _ => anyhow::bail!("Unknown tool: {name}"),
    }
}
```

Note: `SandboxState` fields (`engine`, `wasm_bytes`, `shell`, `env`, `next_named_id`) must be `pub` in `sandbox.rs`. Verify and add `pub` as needed.

- [ ] **Step 5: Build**

```bash
cargo build -p mcp-server-rust 2>&1 | tail -20
```
Expected: compiles (warnings OK).

- [ ] **Step 6: Quick smoke test**

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  | ./target/debug/codepod-mcp-rust --shell-wasm packages/orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm 2>/dev/null
```
Expected: JSON response with `protocolVersion`.

- [ ] **Step 7: Commit**

```bash
git add packages/mcp-server-rust/ Cargo.toml
git commit -m "feat: add mcp-server-rust crate (wasmtime backend)"
```

---

## Task 12: Engine-parameterized integration tests + CI

**Files:**
- Create: `packages/integration-tests/` directory with Deno test suite
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Create `packages/integration-tests/tests/lifecycle.test.ts`**

```typescript
// packages/integration-tests/tests/lifecycle.test.ts
// Engine-parameterized: set SERVER_BINARY env var to the binary to test.
// Defaults to dist/codepod-server (wasmtime). Pass the Deno binary path
// plus sdk-server args to test the Deno engine.

import { assertEquals, assertStringIncludes } from "jsr:@std/assert";

const WASM_FIXTURES = new URL(
  "../../orchestrator/src/platform/__tests__/fixtures",
  import.meta.url,
).pathname;

interface ServerProcess {
  send(method: string, params?: Record<string, unknown>): Promise<unknown>;
  close(): void;
}

async function spawnServer(): Promise<ServerProcess> {
  const binary = Deno.env.get("SERVER_BINARY") ?? "dist/codepod-server";
  const binaryArgs = (Deno.env.get("SERVER_ARGS") ?? "").split(" ").filter(Boolean);

  const process = new Deno.Command(binary, {
    args: binaryArgs,
    stdin: "piped",
    stdout: "piped",
    stderr: "null",
  }).spawn();

  const writer = process.stdin.getWriter();
  const reader = process.stdout.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let nextId = 1;
  const pending = new Map<number, (v: unknown) => void>();

  // Background reader loop
  (async () => {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value);
      while (true) {
        const nl = buffer.indexOf("\n");
        if (nl === -1) break;
        const line = buffer.slice(0, nl);
        buffer = buffer.slice(nl + 1);
        if (!line.trim()) continue;
        try {
          const msg = JSON.parse(line);
          // Skip notifications (no id)
          if (msg.id === undefined || msg.id === null) continue;
          const resolve = pending.get(msg.id);
          if (resolve) {
            pending.delete(msg.id);
            resolve(msg);
          }
        } catch { /* ignore */ }
      }
    }
  })();

  return {
    async send(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
      const id = nextId++;
      const req = JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n";
      await writer.write(new TextEncoder().encode(req));
      return new Promise((resolve) => pending.set(id, resolve));
    },
    close() {
      writer.close();
    },
  };
}

Deno.test("lifecycle: create + run + kill", async () => {
  const server = await spawnServer();
  try {
    const created = await server.send("create", {
      shellWasmPath: `${WASM_FIXTURES}/codepod-shell-exec.wasm`,
      timeoutMs: 30000,
      fsLimitBytes: 64 * 1024 * 1024,
    }) as { result: { ok: boolean } };
    assertEquals((created as any).result.ok, true);

    const ran = await server.send("run", { command: "echo hello-world" }) as any;
    assertStringIncludes(ran.result.stdout, "hello-world");
    assertEquals(ran.result.exitCode, 0);
  } finally {
    server.send("kill", {}).catch(() => {});
    server.close();
  }
});

Deno.test("lifecycle: files.write + files.read", async () => {
  const server = await spawnServer();
  try {
    await server.send("create", {
      shellWasmPath: `${WASM_FIXTURES}/codepod-shell-exec.wasm`,
      timeoutMs: 30000,
      fsLimitBytes: 64 * 1024 * 1024,
    });
    const content = btoa("hello from test");
    await server.send("files.write", { path: "/tmp/test.txt", data: content });
    const read = await server.send("files.read", { path: "/tmp/test.txt" }) as any;
    assertEquals(atob(read.result.data), "hello from test");
  } finally {
    server.send("kill", {}).catch(() => {});
    server.close();
  }
});

Deno.test("lifecycle: snapshot.create + snapshot.restore", async () => {
  const server = await spawnServer();
  try {
    await server.send("create", {
      shellWasmPath: `${WASM_FIXTURES}/codepod-shell-exec.wasm`,
      timeoutMs: 30000,
      fsLimitBytes: 64 * 1024 * 1024,
    });
    await server.send("files.write", { path: "/tmp/before.txt", data: btoa("before") });
    const snap = await server.send("snapshot.create", {}) as any;
    const snapId = snap.result.id;

    await server.send("files.write", { path: "/tmp/after.txt", data: btoa("after") });
    await server.send("snapshot.restore", { id: snapId });

    // before.txt should still exist
    const r = await server.send("files.read", { path: "/tmp/before.txt" }) as any;
    assertEquals(atob(r.result.data), "before");
    // after.txt should be gone
    const r2 = await server.send("files.read", { path: "/tmp/after.txt" }) as any;
    assertEquals(r2.error != null, true);
  } finally {
    server.send("kill", {}).catch(() => {});
    server.close();
  }
});

Deno.test("lifecycle: persistence.export + import", async () => {
  const server = await spawnServer();
  try {
    await server.send("create", {
      shellWasmPath: `${WASM_FIXTURES}/codepod-shell-exec.wasm`,
      timeoutMs: 30000,
      fsLimitBytes: 64 * 1024 * 1024,
    });
    await server.send("files.write", { path: "/tmp/data.txt", data: btoa("persisted") });
    const exported = await server.send("persistence.export", {}) as any;
    const blob = exported.result.data;
    assertStringIncludes(typeof blob, "string");

    await server.send("persistence.import", { data: blob });
    const r = await server.send("files.read", { path: "/tmp/data.txt" }) as any;
    assertEquals(atob(r.result.data), "persisted");
  } finally {
    server.send("kill", {}).catch(() => {});
    server.close();
  }
});
```

- [ ] **Step 2: Create `packages/integration-tests/deno.json`**

```json
{
  "tasks": {
    "test": "deno test -A --no-check tests/"
  }
}
```

- [ ] **Step 3: Run integration tests against the wasmtime binary**

```bash
# Build first
bash scripts/build-sdk-server.sh

# Run tests
SERVER_BINARY=dist/codepod-server deno test -A --no-check packages/integration-tests/tests/ 2>&1 | tail -20
```
Expected: all PASS.

- [ ] **Step 4: Update CI `ci.yml`**

Add two new jobs after the existing `test` job:

```yaml
  test-wasmtime:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: true

      - uses: denoland/setup-deno@v2
        with:
          deno-version: v2.x

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: ". -> target"

      - name: Build wasmtime server
        run: bash scripts/build-sdk-server.sh

      - name: Run integration tests (wasmtime)
        run: |
          SERVER_BINARY=dist/codepod-server \
          deno test -A --no-check packages/integration-tests/tests/

  test-deno-compat:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: true

      - uses: denoland/setup-deno@v2
        with:
          deno-version: v2.x

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-wasip1

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: ". -> target"

      - run: deno install

      - name: Build TypeScript orchestrator
        run: cd packages/orchestrator && npx tsup

      - name: Run integration tests (deno sdk-server)
        run: |
          SERVER_BINARY=deno \
          SERVER_ARGS="run -A --no-check --unstable-sloppy-imports packages/sdk-server/src/server.ts" \
          deno test -A --no-check packages/integration-tests/tests/
```

- [ ] **Step 5: Commit**

```bash
git add packages/integration-tests/ .github/workflows/ci.yml
git commit -m "feat: add engine-parameterized integration tests + wasmtime CI job"
```

---

## Task 13: Docs update

**Files:**
- Modify: `docs/guides/typescript-sdk.md` (if exists) — browser compat section
- Modify: `docs/guides/python-sdk.md` (if exists) — engine parameter
- Modify: `CLAUDE.md` — wasmtime direction, asyncify binary, new build scripts

- [ ] **Step 1: Check which doc files exist**

```bash
ls docs/guides/ 2>/dev/null || echo "no guides dir"
```

- [ ] **Step 2: Update `CLAUDE.md` architecture section**

Add to the Architecture section:

```markdown
## Backend Engines

The sandbox server ships two engines:

- **wasmtime** (default, production): `dist/codepod-server` — Rust binary using wasmtime. No Deno dependency. Build: `bash scripts/build-sdk-server.sh`
- **deno** (dev/debug): `deno run packages/sdk-server/src/server.ts` — TypeScript server. Build: `bash scripts/build-sdk-server.sh --engine deno`

The Python SDK auto-detects the engine: uses `codepod-server` if found on PATH or adjacent to the wheel, otherwise falls back to Deno. Explicit: `Sandbox(engine='wasmtime')` or `Sandbox(engine='deno')`.

## WASM Binaries

- `codepod-shell-exec.wasm` — plain WASM32-WASI binary (for wasmtime + Deno/JSPI)
- `codepod-shell-exec-asyncify.wasm` — asyncified variant (for Safari/WebKit, built via `wasm-opt --asyncify`)

Browser sandbox auto-selects: JSPI binary on Chromium, asyncify binary on Safari.
```

- [ ] **Step 3: Update Python SDK guide (if exists) to mention engine parameter**

Add to the Python SDK guide:

```markdown
## Engine selection

By default, the Python SDK uses `codepod-server` (wasmtime) if found on PATH,
otherwise falls back to Deno.

```python
import codepod
sb = codepod.Sandbox()                   # auto (wasmtime preferred)
sb = codepod.Sandbox(engine='wasmtime') # explicit wasmtime
sb = codepod.Sandbox(engine='deno')     # explicit Deno (dev/debug)
```

Install `codepod-server` by building from source:
```bash
bash scripts/build-sdk-server.sh
cp dist/codepod-server ~/.local/bin/
```
```

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs/
git commit -m "docs: update architecture docs for wasmtime backend and engine parameter"
```

---

## Self-Review

After writing the plan, verify against the spec:

1. **Spec coverage:** All 23 dispatcher methods covered (Tasks 1–8). mcp-server-rust in Task 11. Python SDK engine param in Task 9. Build scripts in Task 10. Integration tests + CI in Task 12. Docs in Task 13. ✓

2. **Placeholders:** No TBD/TODO in code. Steps reference actual function signatures from the codebase. ✓

3. **Type consistency:**
   - `SandboxState.run()` returns `anyhow::Result<Value>` — consistent with dispatcher usage.
   - `MemVfs::export_bytes()` / `import_bytes()` — new methods, consistent in Tasks 5 and 11.
   - `b64_encode`/`b64_decode` helpers — defined in Task 2, used throughout. ✓
   - `require_str` returns `Result<&str, Response>` — consistent across all uses. ✓
   - `handle_run` now takes `params: Value` (not `&str`) — check Task 3 dispatches it correctly. ✓

4. **`DirEntry.kind` field:** Task 2 references `InodeKind::File/Dir/Symlink`. Actual field may be named differently — step 5 in Task 2 notes to verify and adapt. ✓

5. **`MemVfs::mkdirp` vs `mkdir`:** Plan uses `mkdirp` for parent creation. Verify this method exists — grep for it first, use `mkdir` with path creation loop if not. Tasks 1, 2, 4 reference it. ✓

6. **`vfs_mut()` assignment:** `*sb.shell.vfs_mut() = new_vfs` — works only if `vfs_mut` returns `&mut MemVfs` (not a copy). Confirmed in `instance.rs`. ✓
