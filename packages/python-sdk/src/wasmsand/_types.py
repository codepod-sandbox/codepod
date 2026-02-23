from dataclasses import dataclass


@dataclass
class CommandResult:
    stdout: str
    stderr: str
    exit_code: int
    execution_time_ms: float


@dataclass
class FileInfo:
    name: str
    type: str  # "file" or "dir"
    size: int
