# RLM Demo

A minimal demonstration of the **RLM pattern** ([Recursive Language Models](https://arxiv.org/abs/2410.16671)): rather than passing context into the transformer's forward pass, context lives in an external environment (files, variables) that the model can interrogate with code. This turns long-context processing from an architectural problem into a program synthesis problem.

## The core idea

In a standard LLM, conversation history is injected directly into the context window. The model passively receives everything.

In an RLM, context is stored externally — as a file, a Python variable, a database. The model receives a short system prompt that says the data is there and how to access it. When it needs information, it writes code to retrieve exactly the parts it wants.

This matters because:
- A single LLM call only ever sees a small, relevant slice — no attention dilution over irrelevant tokens
- The model can implement its own retrieval strategies: map-reduce, binary search, chunked iteration
- Memory scales independently of the context window

## How the loop works

Before each turn, the harness writes the conversation history to `~/session.txt` in the sandbox. The LLM call only receives the system prompt and the current query — never the accumulated history. The system prompt tells the agent the file is there; if it needs prior context, it reads it explicitly:

```
```bash
cat ~/session.txt
```
```

Within a turn, the model emits code fences to run commands:

```
```bash
echo hello
```
```

The harness extracts the block, executes it in the sandbox, and feeds the output back as a working message. This inner loop repeats until the model writes a plain-text response with no code block — that ends the turn.

The model can also spawn sub-agents:

```
```bash
llm "what is 2 + 2"
```
```

Sub-agents run the same loop recursively (up to 2 levels deep). Each sub-agent starts fresh with only its query. The sandbox is shared, so it can also read `~/session.txt` if it needs context from the parent conversation.

## Variants

### Browser demo (`src/App.tsx`)

Runs entirely in the browser:
- **LLM**: [WebLLM](https://github.com/mlc-ai/web-llm) with `Hermes-3-Llama-3.1-8B` via WebGPU
- **Sandbox**: Codepod WASM sandbox (shell + Python in the browser)
- Requires Chrome 113+ with WebGPU support and `crossOriginIsolated` headers

**Run:**
```bash
bun install
bun run dev
```

### CLI (`src/chat-cli.ts`)

Uses a real LLM API and real `bash`. Good for testing the chat loop without a browser.

**Run:**
```bash
deno task cli "What is e^pi?"
```

Provider is auto-detected from environment variables (first match wins):

| Env var | Provider | Default model |
|---|---|---|
| `ANTHROPIC_API_KEY` | Anthropic | `claude-haiku-4-5-20251001` |
| `OPENAI_API_KEY` | OpenAI | `gpt-4o-mini` |
| `OPENROUTER_API_KEY` | OpenRouter | `meta-llama/llama-3.2-8b-instruct:free` |
| _(none)_ | Ollama (local) | `llama3.2` |

**Flags:**
```bash
deno task cli --provider ollama --model qwen2.5:0.5b "List /tmp"
deno task cli --provider openrouter --model anthropic/claude-haiku-3 "..."
```

**Ollama base URL** defaults to `http://localhost:11434/v1`; override with `OLLAMA_BASE_URL`.

## Tests

Unit + integration tests run entirely with Deno — no browser, no real LLM needed:

```bash
deno task test
```

- **`src/parse.test.ts`** — `extractCodeBlocks` / `parseLlmCommand` unit tests (bash, python3, Python3, py, sh, shell, edge cases)
- **`src/chat.test.ts`** — full `runChat` loop with a mock engine: plain text, bash execution, Python heredoc wrapping, multi-turn, tool call limit, sub-agent recursion, depth limit, history construction

## Source layout

```
src/
  types.ts          — Part / ChatMessage / BootState types
  prompts.ts        — SYSTEM_PROMPT (shared by browser and CLI)
  parse.ts          — extractCodeBlocks, parseLlmCommand (pure, no deps)
  chat.ts           — runChat loop (Engine/RunBash interfaces, sub-agent recursion)
  llm.ts            — WebLLM engine init (browser only)
  llm.worker.ts     — WebLLM web worker (browser only)
  sandbox.ts        — Codepod WASM sandbox init (browser only)
  App.tsx           — React root: boots model + sandbox, renders chat UI
  components/       — ModelLoader, Chat UI components
  parse.test.ts     — Deno unit tests for parse.ts
  chat.test.ts      — Deno integration tests for chat.ts
  chat-cli.ts       — CLI entry point (Deno, multi-provider)
```

## Key concepts

**`extractCodeBlocks(text)`** — Scans model output for fenced code blocks. Bash/sh/shell/zsh blocks are returned as-is. Python blocks (`python`, `python3`, `py`, any case) are wrapped in a heredoc so they run through `python3`:

```bash
python3 << 'PYEOF'
import math
print(math.pi)
PYEOF
```

**`runChat(engine, runBash, messages, onPart, depth)`** — The main loop. Streams the model response, breaks as soon as a complete code block appears, executes it, appends the result to history, and loops. Emits `Part` values (text, tool-call, tool-result) via `onPart` for the UI or CLI renderer.

**`Engine` interface** — OpenAI-compatible streaming interface (`chat.completions.create` returning `AsyncIterable<LLMChunk>`). All four providers (Anthropic via adapter, OpenAI, OpenRouter, Ollama) map to this same interface.
