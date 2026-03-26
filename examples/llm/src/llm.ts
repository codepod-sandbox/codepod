import * as webllm from '@mlc-ai/web-llm';
export { SYSTEM_PROMPT } from './prompts.js';

export const MODEL_ID = 'Qwen2.5-Coder-3B-Instruct-q4f16_1-MLC';

export type ProgressCallback = (progress: number, text: string) => void;

export async function initEngine(onProgress: ProgressCallback): Promise<webllm.MLCEngineInterface> {
  return webllm.CreateWebWorkerMLCEngine(
    new Worker(new URL('./llm.worker.ts', import.meta.url), { type: 'module' }),
    MODEL_ID,
    { initProgressCallback: (report) => onProgress(report.progress, report.text) },
  );
}
