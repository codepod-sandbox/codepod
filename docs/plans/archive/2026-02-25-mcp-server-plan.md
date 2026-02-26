# MCP Server Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build an MCP server that exposes the wasmsand WASM sandbox as 4 tools (run_command, read_file, write_file, list_directory) over stdio.

**Architecture:** New `packages/mcp-server/` package using `@modelcontextprotocol/sdk` McpServer + StdioServerTransport. Creates a single `Sandbox` instance at startup, registers 4 tools that delegate to it. Uses `zod` for input validation (required by MCP SDK).

**Tech Stack:** TypeScript, Bun, `@modelcontextprotocol/sdk`, `zod`, `@wasmsand/sandbox`

---

### Task 1: Scaffold the package

**Files:**
- Create: `packages/mcp-server/package.json`
- Create: `packages/mcp-server/tsconfig.json`

**Step 1: Create package.json**

```json
{
  "name": "@wasmsand/mcp-server",
  "version": "0.0.1",
  "type": "module",
  "main": "src/index.ts",
  "scripts": {
    "test": "bun test"
  },
  "dependencies": {
    "@wasmsand/sandbox": "*",
    "@modelcontextprotocol/sdk": "^2.0",
    "zod": "^3.25"
  },
  "devDependencies": {
    "typescript": "^5.7"
  }
}
```

**Step 2: Create tsconfig.json**

Copy the pattern from `packages/sdk-server/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "outDir": "dist",
    "rootDir": "src",
    "declaration": true,
    "esModuleInterop": true,
    "skipLibCheck": true
  },
  "include": ["src"],
  "exclude": ["src/**/__tests__", "src/**/*.test.ts"]
}
```

**Step 3: Install dependencies**

Run: `cd /Users/sunny/work/wasmsand && bun install`
Expected: Resolves without errors, `node_modules` updated.

**Step 4: Commit**

```bash
git add packages/mcp-server/package.json packages/mcp-server/tsconfig.json bun.lock
git commit -m "feat(mcp-server): scaffold package"
```

---

### Task 2: Implement the MCP server

**Files:**
- Create: `packages/mcp-server/src/index.ts`

**Context:** The MCP SDK v2 uses `McpServer` class with `registerTool()` and `StdioServerTransport`. Tools receive validated args (via zod) and return `{ content: [{ type: 'text', text: string }] }`. The sandbox is created via `Sandbox.create()` from `@wasmsand/sandbox` with `NodeAdapter` from `@wasmsand/sandbox/node`.

**Key paths:**
- WASM binaries live at `packages/orchestrator/src/platform/__tests__/fixtures/` (dev) or resolved from `WASMSAND_WASM_DIR` env var.
- Shell WASM lives at `packages/orchestrator/src/shell/__tests__/fixtures/wasmsand-shell.wasm` (dev).
- The `Sandbox` API: `run(cmd)` → `RunResult`, `readFile(path)` → `Uint8Array`, `writeFile(path, data)`, `readDir(path)` → `DirEntry[]`, `stat(path)` → `StatResult`.

**Step 1: Write the server**

```typescript
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';
import { Sandbox } from '@wasmsand/sandbox';
import { NodeAdapter } from '@wasmsand/sandbox/node';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));

function resolveWasmDir(): string {
  if (process.env.WASMSAND_WASM_DIR) return process.env.WASMSAND_WASM_DIR;
  return resolve(__dirname, '../../orchestrator/src/platform/__tests__/fixtures');
}

function resolveShellWasm(): string {
  if (process.env.WASMSAND_SHELL_WASM) return process.env.WASMSAND_SHELL_WASM;
  return resolve(__dirname, '../../orchestrator/src/shell/__tests__/fixtures/wasmsand-shell.wasm');
}

async function main() {
  const sandbox = await Sandbox.create({
    wasmDir: resolveWasmDir(),
    shellWasmPath: resolveShellWasm(),
    adapter: new NodeAdapter(),
    timeoutMs: Number(process.env.WASMSAND_TIMEOUT_MS) || 30_000,
    fsLimitBytes: Number(process.env.WASMSAND_FS_LIMIT_BYTES) || 256 * 1024 * 1024,
  });

  const server = new McpServer({
    name: 'wasmsand',
    version: '0.0.1',
  });

  server.registerTool(
    'run_command',
    {
      description: 'Run a shell command in the WASM sandbox. Supports 95+ coreutils, pipes, redirects, variables, and control flow.',
      inputSchema: z.object({
        command: z.string().describe('Shell command to execute'),
      }),
    },
    async ({ command }) => {
      const result = await sandbox.run(command);
      const output = JSON.stringify({
        exit_code: result.exitCode,
        stdout: result.stdout,
        stderr: result.stderr,
      });
      return { content: [{ type: 'text', text: output }] };
    },
  );

  server.registerTool(
    'read_file',
    {
      description: 'Read a file from the sandbox virtual filesystem.',
      inputSchema: z.object({
        path: z.string().describe('Absolute path to the file'),
      }),
    },
    async ({ path }) => {
      try {
        const data = sandbox.readFile(path);
        const text = new TextDecoder().decode(data);
        return { content: [{ type: 'text', text }] };
      } catch (err) {
        return { content: [{ type: 'text', text: `Error: ${(err as Error).message}` }], isError: true };
      }
    },
  );

  server.registerTool(
    'write_file',
    {
      description: 'Write a file to the sandbox virtual filesystem.',
      inputSchema: z.object({
        path: z.string().describe('Absolute path for the file'),
        contents: z.string().describe('File contents to write'),
      }),
    },
    async ({ path, contents }) => {
      try {
        sandbox.writeFile(path, new TextEncoder().encode(contents));
        return { content: [{ type: 'text', text: `Wrote ${contents.length} bytes to ${path}` }] };
      } catch (err) {
        return { content: [{ type: 'text', text: `Error: ${(err as Error).message}` }], isError: true };
      }
    },
  );

  server.registerTool(
    'list_directory',
    {
      description: 'List files and directories in the sandbox virtual filesystem.',
      inputSchema: z.object({
        path: z.string().default('/home/user').describe('Directory path to list (defaults to /home/user)'),
      }),
    },
    async ({ path }) => {
      try {
        const entries = sandbox.readDir(path);
        const enriched = entries.map((entry) => {
          const fullPath = path.endsWith('/') ? `${path}${entry.name}` : `${path}/${entry.name}`;
          try {
            const st = sandbox.stat(fullPath);
            return { name: entry.name, type: entry.type, size: st.size };
          } catch {
            return { name: entry.name, type: entry.type, size: 0 };
          }
        });
        return { content: [{ type: 'text', text: JSON.stringify(enriched, null, 2) }] };
      } catch (err) {
        return { content: [{ type: 'text', text: `Error: ${(err as Error).message}` }], isError: true };
      }
    },
  );

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  process.stderr.write(`[wasmsand-mcp] Fatal: ${err}\n`);
  process.exit(1);
});
```

**Step 2: Verify it compiles**

Run: `cd /Users/sunny/work/wasmsand && bun build packages/mcp-server/src/index.ts --no-bundle --external '@wasmsand/*' --external '@modelcontextprotocol/*' --external zod --outdir /dev/null`
Expected: No type errors.

**Step 3: Commit**

```bash
git add packages/mcp-server/src/index.ts
git commit -m "feat(mcp-server): implement MCP server with 4 tools"
```

---

### Task 3: Write integration test

**Files:**
- Create: `packages/mcp-server/src/index.test.ts`

**Context:** Follow the pattern from `packages/sdk-server/src/server.test.ts` — spawn the server as a child process, but instead of raw JSON-RPC, use the MCP client SDK to connect over stdio. Alternatively, since the MCP SDK handles framing, we can test by spawning the process and using `@modelcontextprotocol/sdk` Client class with `StdioClientTransport`.

**Step 1: Write the test**

```typescript
import { describe, it, expect, afterEach } from 'bun:test';
import { resolve } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const SERVER_PATH = resolve(import.meta.dirname, 'index.ts');

function createClient() {
  const transport = new StdioClientTransport({
    command: 'bun',
    args: ['run', SERVER_PATH],
  });
  const client = new Client({ name: 'test-client', version: '1.0.0' });
  return { client, transport };
}

describe('MCP Server', () => {
  let client: Client;
  let transport: StdioClientTransport;

  afterEach(async () => {
    try { await transport.close(); } catch {}
  });

  it('lists 4 tools', async () => {
    ({ client, transport } = createClient());
    await client.connect(transport);
    const { tools } = await client.listTools();
    const names = tools.map((t) => t.name).sort();
    expect(names).toEqual(['list_directory', 'read_file', 'run_command', 'write_file']);
  }, 30_000);

  it('run_command executes shell commands', async () => {
    ({ client, transport } = createClient());
    await client.connect(transport);
    const result = await client.callTool({ name: 'run_command', arguments: { command: 'echo hello world' } });
    const parsed = JSON.parse((result.content as any)[0].text);
    expect(parsed.exit_code).toBe(0);
    expect(parsed.stdout.trim()).toBe('hello world');
  }, 30_000);

  it('write_file + read_file round-trip', async () => {
    ({ client, transport } = createClient());
    await client.connect(transport);
    await client.callTool({ name: 'write_file', arguments: { path: '/home/user/test.txt', contents: 'hello from MCP' } });
    const result = await client.callTool({ name: 'read_file', arguments: { path: '/home/user/test.txt' } });
    expect((result.content as any)[0].text).toBe('hello from MCP');
  }, 30_000);

  it('list_directory returns entries', async () => {
    ({ client, transport } = createClient());
    await client.connect(transport);
    await client.callTool({ name: 'run_command', arguments: { command: 'echo abc > /home/user/abc.txt' } });
    const result = await client.callTool({ name: 'list_directory', arguments: { path: '/home/user' } });
    const entries = JSON.parse((result.content as any)[0].text);
    expect(entries.some((e: any) => e.name === 'abc.txt')).toBe(true);
  }, 30_000);

  it('run_command handles pipes', async () => {
    ({ client, transport } = createClient());
    await client.connect(transport);
    const result = await client.callTool({ name: 'run_command', arguments: { command: 'echo "one two three" | wc -w' } });
    const parsed = JSON.parse((result.content as any)[0].text);
    expect(parsed.exit_code).toBe(0);
    expect(parsed.stdout.trim()).toBe('3');
  }, 30_000);

  it('read_file returns error for missing file', async () => {
    ({ client, transport } = createClient());
    await client.connect(transport);
    const result = await client.callTool({ name: 'read_file', arguments: { path: '/no/such/file' } });
    expect(result.isError).toBe(true);
  }, 30_000);
});
```

**Step 2: Run the tests**

Run: `cd /Users/sunny/work/wasmsand && bun test packages/mcp-server`
Expected: All 6 tests pass.

**Step 3: Commit**

```bash
git add packages/mcp-server/src/index.test.ts
git commit -m "test(mcp-server): add integration tests"
```

---

### Task 4: Add to CI and update README

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`

**Step 1: Add MCP server tests to CI**

In `.github/workflows/ci.yml`, add a step after the existing TypeScript tests:

```yaml
      - name: Run MCP server tests
        timeout-minutes: 5
        run: bun test packages/mcp-server
```

**Step 2: Add MCP section to README**

Add a section to `README.md` documenting the MCP server — what it is, how to configure it in Claude Code / Claude Desktop, and the 4 tools it exposes.

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml README.md
git commit -m "docs: add MCP server to CI and README"
```
