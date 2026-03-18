import { describe, it, afterEach } from '@std/testing/bdd';
import { expect } from '@std/expect';
import { SandboxPool } from '../sandbox-pool.js';
import type { PoolConfig } from '../types.js';
import type { SandboxOptions } from '../../sandbox.js';

// Helper: build minimal SandboxOptions for testing.
function testSandboxOptions(): SandboxOptions {
  const fixturesDir = new URL(
    '../../platform/__tests__/fixtures',
    import.meta.url,
  ).pathname;
  return {
    wasmDir: fixturesDir,
    shellExecWasmPath: `${fixturesDir}/codepod-shell-exec.wasm`,
  };
}

describe('SandboxPool', () => {
  let pool: SandboxPool;

  afterEach(async () => {
    if (pool) await pool.drain();
  });

  it('reports initial stats as all zeros before init', () => {
    const config: PoolConfig = { minSize: 2, maxSize: 5 };
    pool = new SandboxPool(config, testSandboxOptions());
    expect(pool.stats).toEqual({ idle: 0, creating: 0, checkedOut: 0 });
  });
});
