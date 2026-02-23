import { defineConfig } from 'tsup';

export default defineConfig({
  entry: {
    index: 'src/index.ts',
    'node-adapter': 'src/node-adapter.ts',
    'browser-adapter': 'src/browser-adapter.ts',
  },
  format: ['esm'],
  dts: true,
  splitting: true,
  clean: true,
  target: 'es2022',
});
