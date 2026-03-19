from __future__ import annotations

from dataclasses import dataclass, field
from typing import Awaitable, Callable


@dataclass
class PythonPackage:
    """Metadata and files for a Python package installed in the sandbox."""
    version: str = "1.0.0"
    summary: str = ""
    files: dict[str, str] = field(default_factory=dict)


@dataclass
class Extension:
    """Host-provided extension registered with the sandbox.

    Args:
        name: Extension name (used as command name and/or package name).
        description: Help text shown by ``<command> --help``.
        command: Callable invoked when the extension is run as a shell command.
            Signature: ``(args, stdin, env, cwd) -> {"stdout": ..., "exitCode": ...}``
        async_command: Async callable invoked when the extension is run as a shell
            command. Same signature as *command* but returns an awaitable.
            If both *command* and *async_command* are set, *async_command* takes priority.
        python_package: If provided, installs a Python package in the sandbox.
    """
    name: str
    description: str = ""
    command: Callable | None = None
    async_command: Callable[..., Awaitable[dict]] | None = None
    python_package: PythonPackage | None = None
