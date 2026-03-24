import { assertEquals } from 'jsr:@std/assert';
import { extractCodeBlocks, parseLlmCommand } from './parse.ts';

// ---------------------------------------------------------------------------
// extractCodeBlocks
// ---------------------------------------------------------------------------

Deno.test('extracts a bash block', () => {
  const text = '```bash\necho hello\n```';
  assertEquals(extractCodeBlocks(text), ['echo hello']);
});

Deno.test('wraps python block in heredoc', () => {
  const text = '```python\nimport math; print(math.pi)\n```';
  assertEquals(extractCodeBlocks(text), [
    "python3 << 'PYEOF'\nimport math; print(math.pi)\nPYEOF",
  ]);
});

Deno.test('wraps python3 block in heredoc', () => {
  const text = '```python3\nimport numpy as np\nprint(np.e ** np.pi)\n```';
  assertEquals(extractCodeBlocks(text), [
    "python3 << 'PYEOF'\nimport numpy as np\nprint(np.e ** np.pi)\nPYEOF",
  ]);
});

Deno.test('wraps Python3 (mixed case) block in heredoc', () => {
  const text = '```Python3\nimport os\nprint(os.getcwd())\n```';
  assertEquals(extractCodeBlocks(text), [
    "python3 << 'PYEOF'\nimport os\nprint(os.getcwd())\nPYEOF",
  ]);
});

Deno.test('wraps py block in heredoc', () => {
  const text = '```py\nprint(42)\n```';
  assertEquals(extractCodeBlocks(text), [
    "python3 << 'PYEOF'\nprint(42)\nPYEOF",
  ]);
});

Deno.test('handles trailing whitespace after language tag', () => {
  const text = '```python3  \nimport math\n```';
  assertEquals(extractCodeBlocks(text), [
    "python3 << 'PYEOF'\nimport math\nPYEOF",
  ]);
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
  assertEquals(extractCodeBlocks(text), [
    'echo step1',
    "python3 << 'PYEOF'\nprint(\"step2\")\nPYEOF",
  ]);
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
