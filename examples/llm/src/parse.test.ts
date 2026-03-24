import { assertEquals, assertStringIncludes } from 'jsr:@std/assert';
import { extractCodeBlocks, parseLlmCommand } from './parse.ts';

// Decode the base64 payload from a `python3 -c "..."` command back to source.
function decodePythonCmd(cmd: string): string {
  const m = cmd.match(/b64decode\('([A-Za-z0-9+/=]+)'\)/);
  if (!m) throw new Error(`Not a base64 python3 -c command: ${cmd}`);
  return atob(m[1]);
}

// ---------------------------------------------------------------------------
// extractCodeBlocks
// ---------------------------------------------------------------------------

Deno.test('extracts a bash block', () => {
  const text = '```bash\necho hello\n```';
  assertEquals(extractCodeBlocks(text), ['echo hello']);
});

Deno.test('wraps python block as python3 -c', () => {
  const cmd = extractCodeBlocks('```python\nimport math; print(math.pi)\n```')[0];
  assertStringIncludes(cmd, 'python3 -c');
  assertEquals(decodePythonCmd(cmd), 'import math; print(math.pi)');
});

Deno.test('wraps python3 block as python3 -c', () => {
  const code = 'import numpy as np\nprint(np.e ** np.pi)';
  const cmd = extractCodeBlocks(`\`\`\`python3\n${code}\n\`\`\``)[0];
  assertStringIncludes(cmd, 'python3 -c');
  assertEquals(decodePythonCmd(cmd), code);
});

Deno.test('wraps Python3 (mixed case) block as python3 -c', () => {
  const code = 'import os\nprint(os.getcwd())';
  const cmd = extractCodeBlocks(`\`\`\`Python3\n${code}\n\`\`\``)[0];
  assertStringIncludes(cmd, 'python3 -c');
  assertEquals(decodePythonCmd(cmd), code);
});

Deno.test('wraps py block as python3 -c', () => {
  const cmd = extractCodeBlocks('```py\nprint(42)\n```')[0];
  assertStringIncludes(cmd, 'python3 -c');
  assertEquals(decodePythonCmd(cmd), 'print(42)');
});

Deno.test('handles trailing whitespace after language tag', () => {
  const cmd = extractCodeBlocks('```python3  \nimport math\n```')[0];
  assertStringIncludes(cmd, 'python3 -c');
  assertEquals(decodePythonCmd(cmd), 'import math');
});

Deno.test('handles sh and shell tags', () => {
  assertEquals(extractCodeBlocks('```sh\nls -la\n```'), ['ls -la']);
  assertEquals(extractCodeBlocks('```shell\nls -la\n```'), ['ls -la']);
});

Deno.test('ignores non-executable language blocks', () => {
  const text = '```json\n{"key": "value"}\n```\n```typescript\nconst x = 1;\n```';
  assertEquals(extractCodeBlocks(text), []);
});

Deno.test('extracts multiple blocks in order', () => {
  const text = [
    '```bash\necho step1\n```',
    'some text',
    '```python\nprint("step2")\n```',
  ].join('\n');
  const blocks = extractCodeBlocks(text);
  assertEquals(blocks[0], 'echo step1');
  assertStringIncludes(blocks[1], 'python3 -c');
  assertEquals(decodePythonCmd(blocks[1]), 'print("step2")');
});

Deno.test('skips empty blocks', () => {
  assertEquals(extractCodeBlocks('```bash\n   \n```'), []);
});

Deno.test('stops at first complete block (early-break simulation)', () => {
  // Simulate the stream arriving incrementally: only first block present
  const partial = '```bash\necho hi\n```';
  const blocks = extractCodeBlocks(partial);
  assertEquals(blocks.length, 1);
  assertEquals(blocks[0], 'echo hi');
});

// ---------------------------------------------------------------------------
// parseLlmCommand
// ---------------------------------------------------------------------------

Deno.test('parses llm "query" (double quotes)', () => {
  assertEquals(parseLlmCommand('llm "what is pi?"'), 'what is pi?');
});

Deno.test("parses llm 'query' (single quotes)", () => {
  assertEquals(parseLlmCommand("llm 'what is pi?'"), 'what is pi?');
});

Deno.test('returns null for a non-llm command', () => {
  assertEquals(parseLlmCommand('echo hello'), null);
  assertEquals(parseLlmCommand('python3 script.py'), null);
});

Deno.test('returns null for llm without quotes', () => {
  assertEquals(parseLlmCommand('llm what is pi'), null);
});

Deno.test('trims whitespace from query', () => {
  assertEquals(parseLlmCommand('llm "  hello  "'), 'hello');
});
