/**
 * Pure parsing utilities for the RLM chat loop.
 * No browser or sandbox dependencies — easily testable with Deno.
 */

/** Extract executable code blocks from a model response.
 *  bash / sh / shell / zsh → run as-is
 *  python / python3 / py (any case) → wrapped in heredoc so python3 runs it
 */
export function extractCodeBlocks(text: string): string[] {
  const blocks: string[] = [];
  // Match any word-like language tag, optional trailing whitespace before newline.
  const re = /```(\w+)[^\S\n]*\n([\s\S]*?)```/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const lang = m[1].toLowerCase();
    const code = m[2].trim();
    if (!code) continue;

    const isPython = lang.startsWith('python') || lang === 'py';
    const isBash = lang === 'bash' || lang === 'sh' || lang === 'shell' || lang === 'zsh';

    if (isPython) {
      blocks.push(`python3 << 'PYEOF'\n${code}\nPYEOF`);
    } else if (isBash) {
      blocks.push(code);
    }
    // Silently ignore other languages (json, typescript, etc.)
  }
  return blocks;
}

/** If cmd is `llm "query"` or `llm 'query'`, return the query string; else null. */
export function parseLlmCommand(cmd: string): string | null {
  const trimmed = cmd.trim();
  const m = trimmed.match(/^llm\s+["']([^"']+)["']\s*$/s);
  return m ? m[1].trim() : null;
}
