from dataclasses import dataclass
from codepod._rpc import RpcClient
from codepod.commands import Commands
from codepod.files import Files


@dataclass
class SandboxInfo:
    sandbox_id: str
    label: str | None = None
    created_at: str | None = None


class SandboxRef:
    """Handle to a single sandbox with bound commands/files access."""

    def __init__(self, sandbox_id: str, client: RpcClient):
        self.sandbox_id = sandbox_id
        self.commands = Commands(client, sandbox_id)
        self.files = Files(client, sandbox_id)


class SandboxManager:
    """Manages multiple sandboxes within a single server process."""

    def __init__(self, client: RpcClient):
        self._client = client

    def create(self, label: str | None = None) -> SandboxRef:
        params = {}
        if label is not None:
            params["label"] = label
        result = self._client.call("sandbox.create", params)
        return SandboxRef(result["sandboxId"], self._client)

    def list(self) -> list[SandboxInfo]:
        result = self._client.call("sandbox.list", {})
        return [
            SandboxInfo(
                sandbox_id=entry["sandboxId"],
                label=entry.get("label"),
                created_at=entry.get("createdAt"),
            )
            for entry in result
        ]

    def remove(self, sandbox_id: str) -> None:
        self._client.call("sandbox.remove", {"sandboxId": sandbox_id})
