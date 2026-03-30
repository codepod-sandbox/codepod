//! Integration tests for SandboxManager — exercises the high-level sandbox
//! abstraction that the dispatcher uses.

use sdk_server_wasmtime::sandbox::SandboxManager;

fn wasm_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/orchestrator/src/platform/__tests__/fixtures/codepod-shell-exec.wasm");
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[tokio::test]
async fn test_create_and_run() {
    let wasm = wasm_bytes();
    let mut mgr = SandboxManager::new();
    mgr.create(wasm, None, None, None).await.unwrap();
    let result = mgr.root_run("echo hello").await.unwrap();
    assert_eq!(result["exitCode"].as_i64().unwrap(), 0);
    assert!(result["stdout"].as_str().unwrap().contains("hello"));
}
