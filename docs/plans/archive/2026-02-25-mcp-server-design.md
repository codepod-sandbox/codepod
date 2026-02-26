# MCP Server Design

## Overview

An MCP (Model Context Protocol) server that exposes the wasmsand WASM sandbox as tools an LLM can use directly. Communicates over stdio using the official MCP TypeScript SDK.

## Package

`packages/mcp-server/` — new package in the monorepo.

Dependencies: `@wasmsand/sandbox` (orchestrator), `@modelcontextprotocol/sdk`.

```
packages/mcp-server/
├── package.json
├── tsconfig.json
└── src/
    └── index.ts      # Entry point: create sandbox, register tools, start server
```

## Tools

| Tool | Params | Returns |
|------|--------|---------|
| `run_command` | `command: string` | `{stdout, stderr, exit_code}` |
| `read_file` | `path: string` | File contents as text |
| `write_file` | `path: string, contents: string` | Success confirmation |
| `list_directory` | `path: string` (optional, defaults to `.`) | Array of `{name, type, size}` |

## Architecture

```
MCP Client (Claude Code, Claude Desktop, etc.)
    │ stdio, MCP protocol
    ▼
MCP Server (this package)
    │ @modelcontextprotocol/sdk StdioServerTransport
    │ single Sandbox instance, persistent across calls
    ▼
Sandbox (@wasmsand/sandbox)
    │ VFS, ShellRunner, ProcessManager, 95+ WASM tools
    ▼
WebAssembly Modules
```

Single Sandbox instance created at startup. VFS persists across tool calls within a session.

## Configuration

Environment variables:
- `WASMSAND_TIMEOUT_MS` — per-command timeout (default: 30000)
- `WASMSAND_FS_LIMIT_BYTES` — VFS size limit (default: 256MB)
- `WASMSAND_WASM_DIR` — path to WASM binaries (default: auto-resolve from package)

## Usage

```json
{
  "mcpServers": {
    "sandbox": {
      "command": "bun",
      "args": ["run", "packages/mcp-server/src/index.ts"]
    }
  }
}
```

## Out of Scope

- SSE/HTTP transport
- MCP resources or prompts
- Multi-sandbox management
- Dynamic tool registration
- Authentication
