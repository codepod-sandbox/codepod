import pytest
from unittest.mock import patch, MagicMock
from codepod.sandbox import Sandbox, _find_codepod_server, _find_deno


def test_find_deno_returns_path_or_none():
    result = _find_deno()
    # Either finds deno or returns None — must not raise.
    assert result is None or isinstance(result, str)


def test_find_codepod_server_returns_path_or_none():
    result = _find_codepod_server()
    assert result is None or isinstance(result, str)


def test_sandbox_engine_auto_prefers_wasmtime(monkeypatch):
    """With engine='auto', should prefer wasmtime if codepod-server is on PATH."""
    calls = []
    def fake_rpc(runtime, args):
        calls.append(runtime)
        m = MagicMock()
        m.start.return_value = None
        m.call.return_value = {"ok": True}
        m.register_storage_handlers.return_value = None
        return m

    monkeypatch.setattr("codepod.sandbox.RpcClient", fake_rpc)
    monkeypatch.setattr("codepod.sandbox._find_codepod_server", lambda: "/usr/local/bin/codepod-server")
    monkeypatch.setattr("codepod.sandbox._is_bundled", lambda: False)

    with patch.object(Sandbox, 'kill', return_value=None):
        sb = Sandbox(engine='auto')

    assert calls[0] == "/usr/local/bin/codepod-server"


def test_sandbox_engine_deno_uses_deno(monkeypatch):
    """With engine='deno', should use the deno runtime."""
    calls = []
    def fake_rpc(runtime, args):
        calls.append(runtime)
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

    assert calls[0] == "/usr/bin/deno"


def test_sandbox_engine_wasmtime_raises_if_not_found(monkeypatch):
    """With engine='wasmtime' and no binary, should raise RuntimeError."""
    monkeypatch.setattr("codepod.sandbox._find_codepod_server", lambda: None)
    monkeypatch.setattr("codepod.sandbox._is_bundled", lambda: False)

    with pytest.raises(RuntimeError, match="codepod-server not found"):
        Sandbox(engine='wasmtime')


def test_sandbox_engine_auto_falls_back_to_deno(monkeypatch):
    """With engine='auto' and no codepod-server, should fall back to deno."""
    calls = []
    def fake_rpc(runtime, args):
        calls.append(runtime)
        m = MagicMock()
        m.start.return_value = None
        m.call.return_value = {"ok": True}
        m.register_storage_handlers.return_value = None
        return m

    monkeypatch.setattr("codepod.sandbox.RpcClient", fake_rpc)
    monkeypatch.setattr("codepod.sandbox._find_codepod_server", lambda: None)
    monkeypatch.setattr("codepod.sandbox._find_deno", lambda: "/home/user/.deno/bin/deno")
    monkeypatch.setattr("codepod.sandbox._is_bundled", lambda: False)

    with patch.object(Sandbox, 'kill', return_value=None):
        sb = Sandbox(engine='auto')

    assert calls[0] == "/home/user/.deno/bin/deno"
