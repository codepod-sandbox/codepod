import * as webllm from '@mlc-ai/web-llm';
export { SYSTEM_PROMPT } from './prompts.js';

export const MODEL_ID = 'Hermes-3-Llama-3.1-8B-q4f16_1-MLC';

export type ProgressCallback = (progress: number, text: string) => void;

export async function initEngine(onProgress: ProgressCallback): Promise<webllm.MLCEngineInterface> {
  return webllm.CreateWebWorkerMLCEngine(
    new Worker(new URL('./llm.worker.ts', import.meta.url), { type: 'module' }),
    MODEL_ID,
    { initProgressCallback: (report) => onProgress(report.progress, report.text) },
  );
}
