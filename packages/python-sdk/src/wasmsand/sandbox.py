import base64
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
    def __init__(self, *, timeout_ms: int = 30_000, fs_limit_bytes: int = 256 * 1024 * 1024,
                 _sandbox_id: str | None = None, _client: RpcClient | None = None):
        if _client is not None:
            # Internal constructor for forked sandboxes
            self._client = _client
            self._sandbox_id = _sandbox_id
            self.commands = Commands(self._client, self._sandbox_id)
            self.files = Files(self._client, self._sandbox_id)
            return

        if _is_bundled():
            runtime, server, wasm_dir, shell_wasm = _bundled_paths()
        else:
            runtime, server, wasm_dir, shell_wasm = _dev_paths()

        self._client = RpcClient(runtime, server)
        self._client.start()
        self._sandbox_id = None

        self._client.call("create", {
            "wasmDir": wasm_dir,
            "shellWasmPath": shell_wasm,
            "timeoutMs": timeout_ms,
            "fsLimitBytes": fs_limit_bytes,
        })

        self.commands = Commands(self._client)
        self.files = Files(self._client)

    def _with_id(self, params: dict) -> dict:
        if self._sandbox_id is not None:
            params["sandboxId"] = self._sandbox_id
        return params

    def snapshot(self) -> str:
        """Save current VFS + env state. Returns snapshot ID."""
        result = self._client.call("snapshot.create", self._with_id({}))
        return result["id"]

    def restore(self, snapshot_id: str) -> None:
        """Restore to a previous snapshot."""
        self._client.call("snapshot.restore", self._with_id({"id": snapshot_id}))

    def export_state(self) -> bytes:
        """Export the full sandbox state (VFS + env) as an opaque blob."""
        result = self._client.call("persistence.export", self._with_id({}))
        return base64.b64decode(result["data"])

    def import_state(self, blob: bytes) -> None:
        """Import a previously exported sandbox state, replacing current state."""
        data = base64.b64encode(blob).decode("ascii")
        self._client.call("persistence.import", self._with_id({"data": data}))

    def fork(self) -> "Sandbox":
        """Create an independent forked sandbox."""
        result = self._client.call("sandbox.fork", self._with_id({}))
        return Sandbox(
            _sandbox_id=result["sandboxId"],
            _client=self._client,
        )

    def destroy(self) -> None:
        """Destroy this forked sandbox. Only valid on forked instances."""
        if self._sandbox_id is None:
            raise RuntimeError("Cannot destroy root sandbox; use kill() instead")
        self._client.call("sandbox.destroy", {"sandboxId": self._sandbox_id})

    def kill(self) -> None:
        try:
            self._client.call("kill", {})
        except Exception:
            pass
        self._client.stop()

    def __enter__(self) -> "Sandbox":
        return self

    def __exit__(self, *exc) -> None:
        if self._sandbox_id is not None:
            try:
                self.destroy()
            except Exception:
                pass
        else:
            self.kill()
