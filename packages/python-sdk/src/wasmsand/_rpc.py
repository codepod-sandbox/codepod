import json
import subprocess
from typing import Any


class RpcError(Exception):
    def __init__(self, code: int, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


class RpcClient:
    def __init__(self, node_path: str, server_script: str, node_args: list[str] | None = None):
        self._node_path = node_path
        self._server_script = server_script
        self._node_args = node_args or []
        self._proc: subprocess.Popen | None = None
        self._next_id = 1

    def start(self) -> None:
        self._proc = subprocess.Popen(
            [self._node_path, *self._node_args, self._server_script],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def call(self, method: str, params: dict | None = None) -> Any:
        if self._proc is None or self._proc.stdin is None or self._proc.stdout is None:
            raise RuntimeError("RPC client not started")
        req_id = self._next_id
        self._next_id += 1
        request = {"jsonrpc": "2.0", "id": req_id, "method": method, "params": params or {}}
        line = json.dumps(request) + "\n"
        self._proc.stdin.write(line.encode())
        self._proc.stdin.flush()
        resp_line = self._proc.stdout.readline()
        if not resp_line:
            raise RuntimeError("Server closed connection")
        resp = json.loads(resp_line)
        if "error" in resp:
            raise RpcError(resp["error"]["code"], resp["error"]["message"])
        return resp["result"]

    def stop(self) -> None:
        if self._proc is not None:
            self._proc.terminate()
            self._proc.wait(timeout=5)
            self._proc = None
