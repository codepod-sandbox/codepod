import asyncio
import inspect
import json
import subprocess
import threading
from typing import Any, Callable


class RpcError(Exception):
    def __init__(self, code: int, message: str):
        super().__init__(message)
        self.code = code
        self.message = message


class RpcClient:
    def __init__(self, runtime_path: str, server_args: list[str]):
        self._runtime_path = runtime_path
        self._server_args = server_args
        self._proc: subprocess.Popen | None = None
        self._next_id = 1
        self._extension_handlers: dict[str, Callable] = {}
        self._storage_handlers: dict[str, Callable] = {}
        self._output_handlers: dict[int | str, dict[str, Callable]] = {}
        # Persistent event loop for async extension handlers.
        # Created lazily on the first async handler registration.
        self._async_loop: asyncio.AbstractEventLoop | None = None
        self._async_thread: threading.Thread | None = None

    def start(self) -> None:
        self._proc = subprocess.Popen(
            [self._runtime_path, *self._server_args],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def register_output_handler(
        self, request_id: int, on_stdout: Callable | None, on_stderr: Callable | None
    ) -> None:
        handlers: dict[str, Callable] = {}
        if on_stdout:
            handlers["stdout"] = on_stdout
        if on_stderr:
            handlers["stderr"] = on_stderr
        if handlers:
            self._output_handlers[request_id] = handlers

    def unregister_output_handler(self, request_id: int) -> None:
        self._output_handlers.pop(request_id, None)

    def _ensure_async_loop(self) -> asyncio.AbstractEventLoop:
        """Start a persistent background event loop for async handlers, if needed."""
        if self._async_loop is None:
            self._async_loop = asyncio.new_event_loop()
            self._async_thread = threading.Thread(
                target=self._async_loop.run_forever,
                daemon=True,
                name="codepod-async",
            )
            self._async_thread.start()
        return self._async_loop

    def register_extension_handler(self, name: str, handler: Callable) -> None:
        """Register a handler for extension callback requests from the server.

        The handler can be either a regular function or an async coroutine function.
        Async handlers are dispatched on a persistent background event loop so they
        can reuse connection pools and other async resources.
        """
        if inspect.iscoroutinefunction(handler):
            self._ensure_async_loop()
        self._extension_handlers[name] = handler

    def register_storage_handlers(self, save: "Callable | None", load: "Callable | None") -> None:
        if save:
            self._storage_handlers["storage.save"] = save
        if load:
            self._storage_handlers["storage.load"] = load

    def call(self, method: str, params: dict | None = None) -> Any:
        if self._proc is None or self._proc.stdin is None or self._proc.stdout is None:
            raise RuntimeError("RPC client not started")
        req_id = self._next_id
        self._next_id += 1
        request = {"jsonrpc": "2.0", "id": req_id, "method": method, "params": params or {}}
        line = json.dumps(request) + "\n"
        self._proc.stdin.write(line.encode())
        self._proc.stdin.flush()

        # Read responses, handling interleaved callback requests from the server
        while True:
            resp_line = self._proc.stdout.readline()
            if not resp_line:
                raise RuntimeError("Server closed connection")
            msg = json.loads(resp_line)

            # Output streaming notification (no id, method = "output")
            if "method" in msg and msg["method"] == "output" and "id" not in msg:
                params = msg.get("params", {})
                rid = params.get("request_id")
                handlers = self._output_handlers.get(rid, {})
                stream_type = params.get("stream")
                data = params.get("data", "")
                handler = handlers.get(stream_type)
                if handler:
                    handler(data)
                continue

            # Callback request from server? (id starts with 'cb_' and has a method)
            if (
                "method" in msg
                and isinstance(msg.get("id"), str)
                and msg["id"].startswith("cb_")
            ):
                self._handle_callback(msg)
                continue

            # Normal response to our request
            if "error" in msg and msg["error"]:
                raise RpcError(msg["error"]["code"], msg["error"]["message"])
            return msg.get("result")

    def _handle_callback(self, msg: dict) -> None:
        """Handle a callback request from the server and send back the response."""
        cb_id = msg["id"]
        method = msg["method"]
        params = msg.get("params", {})

        try:
            if method == "extension.invoke":
                name = params.get("name", "")
                handler = self._extension_handlers.get(name)
                if handler is None:
                    self._send_callback_error(cb_id, f"No handler for extension: {name}")
                    return
                kwargs = dict(
                    args=params.get("args", []),
                    stdin=params.get("stdin", ""),
                    env=params.get("env", {}),
                    cwd=params.get("cwd", "/"),
                )
                if inspect.iscoroutinefunction(handler):
                    assert self._async_loop is not None
                    future = asyncio.run_coroutine_threadsafe(
                        handler(**kwargs), self._async_loop,
                    )
                    result = future.result(timeout=30)
                else:
                    result = handler(**kwargs)
                self._send_callback_result(cb_id, result)
            elif method in ("storage.save", "storage.load"):
                handler = self._storage_handlers.get(method)
                if handler is None:
                    self._send_callback_error(cb_id, f"No handler for: {method}")
                    return
                if method == "storage.save":
                    import base64
                    state = base64.b64decode(params.get("state", ""))
                    sandbox_id = params.get("sandbox_id", "")
                    handler(sandbox_id, state)
                    self._send_callback_result(cb_id, None)
                elif method == "storage.load":
                    import base64
                    sandbox_id = params.get("sandbox_id", "")
                    result = handler(sandbox_id)
                    data = base64.b64encode(result).decode("ascii")
                    self._send_callback_result(cb_id, data)
            else:
                self._send_callback_error(cb_id, f"Unknown callback method: {method}")
        except Exception as e:
            self._send_callback_error(cb_id, str(e))

    def _send_callback_result(self, cb_id: str, result: Any) -> None:
        resp = {"jsonrpc": "2.0", "id": cb_id, "result": result}
        line = json.dumps(resp) + "\n"
        self._proc.stdin.write(line.encode())  # type: ignore[union-attr]
        self._proc.stdin.flush()  # type: ignore[union-attr]

    def _send_callback_error(self, cb_id: str, message: str) -> None:
        resp = {"jsonrpc": "2.0", "id": cb_id, "error": {"code": -32603, "message": message}}
        line = json.dumps(resp) + "\n"
        self._proc.stdin.write(line.encode())  # type: ignore[union-attr]
        self._proc.stdin.flush()  # type: ignore[union-attr]

    def stop(self) -> None:
        if self._async_loop is not None:
            self._async_loop.call_soon_threadsafe(self._async_loop.stop)
            if self._async_thread is not None:
                self._async_thread.join(timeout=2)
            self._async_loop = None
            self._async_thread = None
        if self._proc is not None:
            proc, self._proc = self._proc, None
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
