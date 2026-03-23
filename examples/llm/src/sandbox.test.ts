import { describe, it, expect } from 'vitest';
import { buildVfsPaths } from './sandbox.js';

describe('buildVfsPaths', () => {
  it('maps glob keys to /src/ paths', () => {
    const globResult = {
      './main.tsx': 'content-a',
      './components/Chat.tsx': 'content-b',
    };
    const result = buildVfsPaths(globResult);
    expect(result).toEqual({
      '/src/main.tsx': 'content-a',
      '/src/components/Chat.tsx': 'content-b',
    });
  });

  it('strips the leading ./ prefix', () => {
    const result = buildVfsPaths({ './deep/nested/file.ts': 'x' });
    expect(result['/src/deep/nested/file.ts']).toBe('x');
  });
});
