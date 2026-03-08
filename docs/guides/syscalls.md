# WASM Syscall Reference

All WASM processes in the sandbox import host functions from the `codepod` namespace. These are the syscalls available to shell, Python, and tool binaries.

## Process management

| Syscall | Signature | Description |
|---------|-----------|-------------|
| `host_pipe` | `(out_ptr, out_cap) → i32` | Creates a pipe. Writes `{ read_fd, write_fd }` JSON to output buffer. |
| `host_spawn` | `(req_ptr, req_len) → i32` | Spawns a child WASM process. Returns PID or -1. Request is JSON `SpawnRequest`. |
| `host_waitpid` | `(pid, out_ptr, out_cap) → i32` | Waits for child to exit. Writes `{ exit_code }`. **Async (JSPI)**. |
| `host_close_fd` | `(fd) → i32` | Closes a file descriptor. Returns 0 on success. |
| `host_read_fd` | `(fd, out_ptr, out_cap) → i32` | Reads from a pipe fd. Returns bytes written, or needed size if buffer too small. |
| `host_write_fd` | `(fd, data_ptr, data_len) → i32` | Writes to a pipe fd. Returns bytes written or negative error. |
| `host_dup` | `(fd, out_ptr, out_cap) → i32` | Duplicates fd. Writes `{ fd: new_fd }` JSON. |
| `host_dup2` | `(src_fd, dst_fd) → i32` | Makes dst_fd point to same target as src_fd. Returns 0 on success. |
| `host_yield` | `() → void` | Yields to JS microtask queue. **Async (JSPI)**. |

## Network

| Syscall | Signature | Description |
|---------|-----------|-------------|
| `host_network_fetch` | `(req_ptr, req_len, out_ptr, out_cap) → i32` | HTTP fetch. Request: `{ url, method, headers, body }`. Response: `{ ok, status, headers, body, error }`. |

## Sockets (full mode only)

These syscalls are available when the network policy `mode` is `"full"`. They proxy to real TCP/TLS connections on the host.

| Syscall | Signature | Description |
|---------|-----------|-------------|
| `host_socket_connect` | `(req_ptr, req_len, out_ptr, out_cap) → i32` | Opens a TCP/TLS socket. Request: `{ host, port, tls }`. Response: `{ ok, socket_id }`. |
| `host_socket_send` | `(req_ptr, req_len, out_ptr, out_cap) → i32` | Sends data. Request: `{ socket_id, data_b64 }`. Response: `{ ok, bytes_sent }`. |
| `host_socket_recv` | `(req_ptr, req_len, out_ptr, out_cap) → i32` | Receives data. Request: `{ socket_id, max_bytes }`. Response: `{ ok, data_b64 }`. |
| `host_socket_close` | `(req_ptr, req_len) → i32` | Closes a socket. Request: `{ socket_id }`. Returns 0 or -1. |

Socket data is base64-encoded in JSON since the bridge protocol is JSON-based.

## Extensions

| Syscall | Signature | Description |
|---------|-----------|-------------|
| `host_extension_invoke` | `(req_ptr, req_len, out_ptr, out_cap) → i32` | Invokes a host extension. Request: `{ name, args, stdin, env, cwd }`. Response: `{ exit_code, stdout, stderr }`. **Async (JSPI)**. |
| `host_is_extension` | `(name_ptr, name_len) → i32` | Returns 1 if the named extension is available, 0 otherwise. |

## JSON protocol

All syscalls use a shared JSON-over-linear-memory protocol:

1. **Input**: Caller writes JSON string to WASM linear memory, passes `(ptr, len)`.
2. **Output**: Host writes JSON response to caller's output buffer at `(out_ptr, out_cap)`. Returns bytes written.
3. **Buffer too small**: If the response exceeds `out_cap`, the syscall returns the required size. Caller should retry with a larger buffer.

The `call_with_outbuf` helper in Rust (and equivalent in the orchestrator) handles the retry loop automatically.

## Python access

Python code accesses these syscalls through the `_codepod` native module:

```python
import _codepod

# HTTP fetch (restricted + full modes)
result = _codepod.fetch("GET", "https://example.com", {}, None)

# Socket operations (full mode only)
sock_id = _codepod.socket_connect("example.com", 443, True)
_codepod.socket_send(sock_id, b"GET / HTTP/1.1\r\n\r\n")
data = _codepod.socket_recv(sock_id, 65536)
_codepod.socket_close(sock_id)
```

## Shell access

Shell builtins (`curl`, `wget`) use `host_network_fetch` internally. The shell executor uses process management syscalls for pipelines and command substitution.
