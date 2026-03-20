# Extensions

Extensions let hosts expose custom capabilities to sandbox code — shell commands that participate in pipes and redirects, and Python packages importable from sandbox scripts.

## Overview

An extension consists of:
- A **name** — becomes a shell command in `/bin/<name>`
- A **description** — one-liner shown by `extensions list`
- A **command handler** — a host-side async function that receives args/stdin and returns stdout/exitCode
- An optional **Python package** — files installed in the VFS at `/usr/lib/python/<name>/`, importable from sandbox Python scripts

Optional discovery metadata:
- **`usage`** — usage string shown by `extensions info <name>` (e.g. `"search <query> [--k N]"`)
- **`examples`** — list of concrete invocations shown by `extensions info <name>`
- **`category`** — free-form grouping label used by `extensions list --category <cat>` (e.g. `"search"`, `"files"`)

## TypeScript

```typescript
const sandbox = await Sandbox.create({
  adapter: new NodeAdapter(),
  wasmDir: './wasm',
  extensions: [
    {
      name: 'llm',
      description: 'Query an LLM',
      usage: 'llm <prompt>',
      examples: ['llm "summarize this document"', 'echo data | llm "what is this?"'],
      category: 'ai',
      command: async ({ args, stdin }) => {
        const prompt = args.join(' ') || stdin;
        const answer = await myLlmApi(prompt);
        return { stdout: answer + '\n', exitCode: 0 };
      },
    },
    {
      name: 'vecdb',
      description: 'Search a vector database',
      usage: 'vecdb <query> [--k N]',
      examples: ['vecdb "similar documents"', 'vecdb "foo" --k 20'],
      category: 'search',
      command: async ({ args }) => {
        const results = await myVecSearch(args.join(' '));
        return { stdout: JSON.stringify(results) + '\n', exitCode: 0 };
      },
      pythonPackage: {
        version: '1.0.0',
        summary: 'Vector database client',
        files: {
          '__init__.py': 'from codepod_ext import call as _call\n\ndef search(q): return _call("vecdb", "search", query=q)\n',
        },
      },
    },
  ],
});

// Extension commands work like any other command
await sandbox.run('echo "summarize this" | llm');
await sandbox.run('vecdb "similar documents" | jq .results');

// Extension Python packages are importable
await sandbox.run('python3 -c "import vecdb; print(vecdb.search(\'test\'))"');

// Discoverable via standard tools
await sandbox.run('which llm');        // /usr/bin/llm
await sandbox.run('pip list');         // shows vecdb 1.0.0
await sandbox.run('pip show vecdb');   // metadata + file list
```

## Python

Async handlers are supported — see [Async handlers](#async-handlers).

```python
from codepod import Sandbox, Extension, PythonPackage

with Sandbox(extensions=[
    Extension(
        name="llm",
        description="Query an LLM",
        usage="llm <prompt>",
        examples=['llm "summarize this"', 'echo data | llm "what is this?"'],
        category="ai",
        command=lambda args, stdin, **_: {
            "stdout": call_my_llm(" ".join(args) or stdin) + "\n",
            "exitCode": 0,
        },
    ),
    Extension(
        name="vecdb",
        description="Vector database search",
        usage="vecdb <query> [--k N]",
        examples=['vecdb "similar documents"', 'vecdb "foo" --k 20'],
        category="search",
        command=lambda args, **_: {"stdout": do_search(args), "exitCode": 0},
        python_package=PythonPackage(
            version="1.0.0",
            summary="Vector database client",
            files={
                "__init__.py": (
                    "from codepod_ext import call as _call\n"
                    "def search(q): return _call('vecdb', 'search', query=q)\n"
                ),
            },
        ),
    ),
]) as sb:
    sb.commands.run("echo hello | llm")
    sb.commands.run("pip list")
```

## Built-in discovery command

When any extensions are registered, a built-in `extensions` command is automatically available in the sandbox. Agents can use it to discover available tools without out-of-band documentation.

```bash
# List all extensions
extensions list

# Filter by category
extensions list --category search

# Output as JSON (for programmatic use)
extensions list --json

# Show full details for one extension
extensions info vecdb

# Show help
extensions --help
```

Example output of `extensions list`:

```
NAME    CATEGORY  DESCRIPTION
──────────────────────────────────────────────────────
llm     ai        Query an LLM
vecdb   search    Vector database search
```

Example output of `extensions info vecdb`:

```
Name:        vecdb
Category:    search
Description: Vector database search
Usage:       vecdb <query> [--k N]
Examples:
  vecdb "similar documents"
  vecdb "foo" --k 20
```

The `extensions` command itself does not appear in `extensions list`.

### Suggested system prompt

Applications can collapse their sandbox system prompt to a single line:

```
Available tools are registered as shell commands.
Run `extensions list` to see them, `extensions info <name>` for details.
```

## Handler interface

Extension command handlers receive:

| Field | Type | Description |
|-------|------|-------------|
| `args` | `string[]` | Command arguments (everything after the command name) |
| `stdin` | `string` | Piped input (empty string if no pipe) |
| `env` | `Record<string, string>` | Current environment variables |
| `cwd` | `string` | Current working directory |

Handlers return:

| Field | Type | Description |
|-------|------|-------------|
| `stdout` | `string` | Standard output |
| `stderr` | `string` (optional) | Standard error |
| `exitCode` | `number` | Exit code (0 = success) |

## Async handlers

### TypeScript

All TypeScript extension handlers are async by default — just return a `Promise`.

### Python

The Python SDK supports both sync and async handlers. Use `async_command` for handlers that need async I/O (e.g. aiohttp connection pools):

```python
import aiohttp

async def async_search(args, stdin, env, cwd):
    async with aiohttp.ClientSession() as session:
        resp = await session.get(f"https://api.example.com/search?q={args[0]}")
        data = await resp.json()
    return {"stdout": str(data) + "\n", "exitCode": 0}

with Sandbox(extensions=[
    Extension(
        name="search",
        description="Search the index",
        async_command=async_search,
    )
]) as sb:
    sb.commands.run("search 'my query'")
```

When both `command` and `async_command` are set, `async_command` takes priority.

Async handlers run on a persistent background event loop, so they can safely reuse async resources (connection pools, sessions) across calls.

## Shell integration

Extension commands behave like any other shell command:

- **Pipes** — `echo data | myext | grep result`
- **Redirects** — `myext > output.txt 2> errors.txt`
- **Chaining** — `myext && echo ok || echo fail`
- **Help** — `myext --help` returns the description
- **Discoverability** — `which myext` shows `/usr/bin/myext`

## Python packages

Extensions can include Python packages that are importable from sandbox Python scripts.

Python packages are installed in the VFS at `/usr/lib/python/<name>/` and use the `codepod_ext` bridge module to call back to the host. The bridge is synchronous from the Python side — it uses the WASI fd bridge to call async host handlers.

```python
# In sandbox Python code:
import vecdb
results = vecdb.search("my query")
```

Package metadata is available via pip:

```bash
pip list          # shows installed extension packages
pip show vecdb    # shows version, summary, file list
```

**Note:** Python package extensions require worker execution mode (`security.hardKill: true` in TypeScript) since the synchronous WASI fd bridge needs the main thread free to run async handlers.

**Runtime requirement:** Extensions use [JSPI](https://v8.dev/blog/jspi) (`WebAssembly.Suspending`/`WebAssembly.promising`) to let synchronous WASM code call async host handlers. This requires Deno or Node.js 25+ — Bun does not support JSPI.

## Security model

Extension handlers execute on the host side — they have full host access. This is by design: extensions exist to give sandbox code access to capabilities that require host privileges.

See [Security Architecture](security.md#extension-trust-model) for details on the trust model.
