import { describe, it, expect, afterEach } from 'bun:test';
import { Sandbox } from '../sandbox';
import { NodeAdapter } from '../platform/node-adapter';
import { resolve } from 'path';

const WASM_DIR = resolve(import.meta.dirname, '../platform/__tests__/fixtures');
const SHELL_WASM = resolve(import.meta.dirname, '../shell/__tests__/fixtures/wasmsand-shell.wasm');

describe('Sandbox packages option', () => {
  let sandbox: Sandbox;
  afterEach(() => { sandbox?.destroy(); });

  it('installs requested packages into VFS', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: ['requests'],
    });
    const result = await sandbox.run('python3 -c "import requests; print(requests.__version__)"');
    expect(result.stdout.trim()).toBe('2.31.0');
  });

  it('does not install packages not requested', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: [],
    });
    const result = await sandbox.run('python3 -c "import requests"');
    expect(result.exitCode).not.toBe(0);
  });

  it('auto-installs dependencies', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: ['pandas'],
    });
    // pandas depends on numpy — numpy should also be installed
    const result = await sandbox.run('python3 -c "import numpy; print(\'ok\')"');
    expect(result.stdout.trim()).toBe('ok');
  });

  it('works with no packages option', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
    });
    // Should work fine without packages option
    const result = await sandbox.run('echo hello');
    expect(result.stdout.trim()).toBe('hello');
  });
});
