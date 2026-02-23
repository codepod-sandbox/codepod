import os
import shutil
from wasmsand._rpc import RpcClient
from wasmsand.commands import Commands
from wasmsand.files import Files

_PKG_DIR = os.path.dirname(os.path.abspath(__file__))
_BUNDLED_DIR = os.path.join(_PKG_DIR, "_bundled")


def _is_bundled() -> bool:
    """Check if we're running from an installed wheel with bundled assets."""
    return os.path.isdir(_BUNDLED_DIR)


def _bundled_paths() -> tuple[str, str, str, str]:
    """Return (runtime, server_script, wasm_dir, shell_wasm) for installed mode."""
    runtime = os.path.join(_BUNDLED_DIR, "bun")
    server = os.path.join(_BUNDLED_DIR, "server.js")
    wasm_dir = os.path.join(_BUNDLED_DIR, "wasm")
    shell_wasm = os.path.join(wasm_dir, "wasmsand-shell.wasm")
    return runtime, server, wasm_dir, shell_wasm


def _dev_paths() -> tuple[str, str, str, str]:
    """Return (runtime, server_script, wasm_dir, shell_wasm) for dev mode."""
    runtime_path = shutil.which("bun")
    if runtime_path is None:
        raise RuntimeError("Bun not found on PATH (required for dev mode)")

    repo_root = os.path.abspath(os.path.join(_PKG_DIR, "..", "..", "..", ".."))
    server = os.path.join(repo_root, "packages", "sdk-server", "src", "server.ts")
    wasm_dir = os.path.join(
        repo_root, "packages", "orchestrator", "src", "platform", "__tests__", "fixtures"
    )
    shell_wasm = os.path.join(
        repo_root, "packages", "orchestrator", "src", "shell", "__tests__", "fixtures",
        "wasmsand-shell.wasm",
    )
    return runtime_path, server, wasm_dir, shell_wasm


class Sandbox:
    def __init__(self, *, timeout_ms: int = 30_000, fs_limit_bytes: int = 256 * 1024 * 1024):
        if _is_bundled():
            runtime, server, wasm_dir, shell_wasm = _bundled_paths()
        else:
            runtime, server, wasm_dir, shell_wasm = _dev_paths()

        self._client = RpcClient(runtime, server)
        self._client.start()

        self._client.call("create", {
            "wasmDir": wasm_dir,
            "shellWasmPath": shell_wasm,
            "timeoutMs": timeout_ms,
            "fsLimitBytes": fs_limit_bytes,
        })

        self.commands = Commands(self._client)
        self.files = Files(self._client)

    def snapshot(self) -> str:
        """Save current VFS + env state. Returns snapshot ID."""
        result = self._client.call("snapshot.create", {})
        return result["id"]

    def restore(self, snapshot_id: str) -> None:
        """Restore to a previous snapshot."""
        self._client.call("snapshot.restore", {"id": snapshot_id})

    def fork(self) -> "Sandbox":
        """Create an independent forked sandbox."""
        result = self._client.call("sandbox.fork", {})
        forked = object.__new__(Sandbox)
        forked._client = self._client
        forked.commands = Commands(self._client)
        forked.files = Files(self._client)
        return forked

    def kill(self) -> None:
        try:
            self._client.call("kill", {})
        except Exception:
            pass
        self._client.stop()

    def __enter__(self) -> "Sandbox":
        return self

    def __exit__(self, *exc) -> None:
        self.kill()
