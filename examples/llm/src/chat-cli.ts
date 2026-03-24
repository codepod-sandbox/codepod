/**
 * CLI runner for the RLM chat loop — uses Anthropic API + real bash.
 * Lets you interact with (and debug) the chat loop without a browser.
 *
 * Usage:
 *   export ANTHROPIC_API_KEY=sk-ant-...
 *   deno run --allow-env --allow-run --allow-net src/chat-cli.ts "What is e^pi?"
 *
 * Or pipe a question:
 *   echo "List files in /tmp" | deno run --allow-env --allow-run --allow-net src/chat-cli.ts
 *
 * Env vars:
 *   ANTHROPIC_API_KEY  required
 *   ANTHROPIC_MODEL    optional, default: claude-haiku-4-5-20251001 (fast/cheap for testing)
 */
import Anthropic from 'npm:@anthropic-ai/sdk';
import { runChat } from './chat.ts';
import type { Part } from './types.ts';

// ---------------------------------------------------------------------------
// Anthropic → OpenAI-streaming adapter
// ---------------------------------------------------------------------------

type OAIChunk = { choices: Array<{ delta: { content?: string | null }; finish_reason: string | null }> };

function makeAnthropicEngine(apiKey: string, model: string) {
  const client = new Anthropic({ apiKey });
  return {
    chat: {
      completions: {
        create: async (opts: { messages: Array<{ role: string; content: string }> }): Promise<AsyncIterable<OAIChunk>> => {
          const systemMsg = opts.messages.find((m) => m.role === 'system');
          const otherMsgs = opts.messages
            .filter((m) => m.role !== 'system')
            .map((m) => ({ role: m.role as 'user' | 'assistant', content: m.content }));

          const stream = await client.messages.stream({
            model,
            max_tokens: 4096,
            ...(systemMsg ? { system: systemMsg.content } : {}),
            messages: otherMsgs,
          });

          return (async function* (): AsyncIterable<OAIChunk> {
            for await (const event of stream) {
              if (event.type === 'content_block_delta' && event.delta.type === 'text_delta') {
                yield { choices: [{ delta: { content: event.delta.text }, finish_reason: null }] };
              }
            }
          })();
        },
      },
    },
  };
}

// ---------------------------------------------------------------------------
// Real bash via Deno.Command
// ---------------------------------------------------------------------------

async function runBash(command: string): Promise<{ stdout: string; stderr: string; exitCode: number }> {
  const proc = new Deno.Command('bash', {
    args: ['-c', command],
    stdout: 'piped',
    stderr: 'piped',
  });
  const { code, stdout, stderr } = await proc.output();
  return {
    stdout: new TextDecoder().decode(stdout),
    stderr: new TextDecoder().decode(stderr),
    exitCode: code,
  };
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

const RESET = '\x1b[0m';
const DIM = '\x1b[2m';
const BOLD = '\x1b[1m';
const CYAN = '\x1b[36m';
const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';

function renderPart(part: Part): void {
  switch (part.kind) {
    case 'text':
      Deno.stdout.writeSync(new TextEncoder().encode(part.text));
      break;
    case 'tool-call':
      console.error(`\n${CYAN}▶ bash${RESET} ${DIM}${part.command.split('\n')[0]}${RESET}`);
      break;
    case 'tool-result': {
      const ok = part.exitCode === 0;
      const icon = ok ? `${GREEN}✓${RESET}` : `${RED}✗ ${part.exitCode}${RESET}`;
      if (part.stdout) console.error(`${DIM}${part.stdout.trimEnd()}${RESET}`);
      if (part.stderr) console.error(`${RED}${part.stderr.trimEnd()}${RESET}`);
      console.error(`${icon}`);
      break;
    }
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const apiKey = Deno.env.get('ANTHROPIC_API_KEY');
if (!apiKey) {
  console.error(`${RED}Error: ANTHROPIC_API_KEY not set${RESET}`);
  Deno.exit(1);
}

const model = Deno.env.get('ANTHROPIC_MODEL') ?? 'claude-haiku-4-5-20251001';
const engine = makeAnthropicEngine(apiKey, model);

// Question from args or stdin
let question = Deno.args.join(' ').trim();
if (!question) {
  const bytes = await Deno.stdin.readable.getReader().read();
  question = new TextDecoder().decode(bytes.value ?? new Uint8Array()).trim();
}
if (!question) {
  console.error(`Usage: deno run ... src/chat-cli.ts "your question"`);
  Deno.exit(1);
}

console.error(`${BOLD}${YELLOW}[${model}]${RESET} ${question}\n`);

await runChat(
  engine,
  runBash,
  [{ role: 'user', content: question }],
  renderPart,
);

console.error('');
