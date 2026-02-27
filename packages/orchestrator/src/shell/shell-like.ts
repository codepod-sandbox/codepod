/**
 * Minimal interface for shell command execution.
 *
 * Both ShellRunner (TS executor) and ShellInstance (Rust WASM executor)
 * implement this interface, allowing Sandbox to use either backend
 * interchangeably.
 */

import type { RunResult } from './shell-types.js';
import type { HistoryEntry } from './history.js';

export interface ShellLike {
  run(command: string): Promise<RunResult>;

  // Environment
  getEnv(name: string): string | undefined;
  setEnv(name: string, value: string): void;
  getEnvMap(): Map<string, string>;
  setEnvMap(env: Map<string, string>): void;

  // History
  getHistory(): HistoryEntry[];
  clearHistory(): void;

  // Lifecycle
  cancel(reason: string): void;
  setDeadlineNow(): void;
  resetCancel(deadlineMs: number): void;

  destroy?(): void;
}
