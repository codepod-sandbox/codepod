import type { Part } from './types.js';
import { SYSTEM_PROMPT } from './prompts.js';
import { extractCodeBlocks, parseLlmCommand } from './parse.js';

export const MAX_TOOL_CALLS = 15;
export const MAX_DEPTH = 2;

export type LLMChunk = { choices: Array<{ delta: { content: string | null }; finish_reason: string | null }> };

export type Engine = {
  chat: {
    completions: {
      create: (opts: object) => Promise<AsyncIterable<LLMChunk>>;
    };
  };
};

export type RunBash = (command: string) => Promise<{ stdout: string; stderr: string; exitCode: number }>;

type LLMMessage =
  | { role: 'system'; content: string }
  | { role: 'user'; content: string }
  | { role: 'assistant'; content: string };


export async function runChat(
  engine: Engine,
  runBash: RunBash,
  query: string,
  onPart: (part: Part) => void,
  depth = 0,
): Promise<void> {
  const history: LLMMessage[] = [
    { role: 'system', content: SYSTEM_PROMPT },
    { role: 'user', content: query },
  ];

  let toolCallCount = 0;

  while (true) {
    const stream = await engine.chat.completions.create({
      messages: history,
      stream: true,
    });

    // Stream and break as soon as a complete code block closes — execute immediately.
    let fullText = '';
    for await (const chunk of stream) {
      const content = (chunk as { choices: Array<{ delta: { content?: string | null } }> }).choices[0].delta.content;
      if (content) {
        fullText += content;
        onPart({ kind: 'text', text: content });
        if (extractCodeBlocks(fullText).length > 0) break;
      }
    }

    const commands = extractCodeBlocks(fullText);
    if (commands.length === 0) break;

    const resultLines: string[] = [];
    for (const cmd of commands) {
      if (toolCallCount >= MAX_TOOL_CALLS) {
        onPart({ kind: 'text', text: '\n\n_Tool call limit reached — stopping._' });
        return;
      }

      const callId = crypto.randomUUID();
      const subQuery = parseLlmCommand(cmd);

      if (subQuery !== null) {
        // Recursive sub-agent call.
        onPart({ kind: 'tool-call', callId, command: cmd });

        if (depth >= MAX_DEPTH) {
          const err = 'Max recursion depth reached.';
          onPart({ kind: 'tool-result', callId, stdout: '', stderr: err, exitCode: 1 });
          resultLines.push(`$ ${cmd}\nstderr: ${err}`);
        } else {
          // Run sub-agent; accumulate its text, forward its tool calls.
          let subText = '';
          await runChat(
            engine,
            runBash,
            subQuery,
            (part) => {
              if (part.kind === 'text') {
                subText += part.text;
              } else {
                // Forward sub-agent tool calls/results so user can see the work.
                onPart(part);
              }
            },
            depth + 1,
          );
          onPart({ kind: 'tool-result', callId, stdout: subText, stderr: '', exitCode: 0 });
          resultLines.push(`$ ${cmd}\n${subText || '(no output)'}`);
        }
      } else {
        // Normal bash / python command.
        onPart({ kind: 'tool-call', callId, command: cmd });
        const result = await runBash(cmd);
        onPart({ kind: 'tool-result', callId, ...result });

        const output = [result.stdout, result.stderr ? `stderr: ${result.stderr}` : '']
          .filter(Boolean)
          .join('\n');
        resultLines.push(`$ ${cmd}\n${output || '(no output)'}`);
      }

      toolCallCount++;
    }

    history.push({ role: 'assistant', content: fullText });
    history.push({ role: 'user', content: resultLines.join('\n\n') });
  }
}
