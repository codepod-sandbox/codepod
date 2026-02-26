/**
 * Conformance tests for grep — exercises regex features after port to regex crate.
 */
import { describe, it, expect, beforeEach } from 'bun:test';
import { resolve } from 'node:path';

import { ShellRunner } from '../../shell-runner.js';
import { ProcessManager } from '../../../process/manager.js';
import { VFS } from '../../../vfs/vfs.js';
import { NodeAdapter } from '../../../platform/node-adapter.js';

const FIXTURES = resolve(import.meta.dirname, '../../../platform/__tests__/fixtures');
const SHELL_WASM = resolve(import.meta.dirname, '../fixtures/codepod-shell.wasm');

const TOOLS = [
  'cat', 'echo', 'head', 'tail', 'wc', 'sort', 'uniq', 'grep',
  'ls', 'mkdir', 'rm', 'cp', 'mv', 'touch', 'tee', 'tr', 'cut',
  'basename', 'dirname', 'env', 'printf',
  'find', 'sed', 'awk', 'jq',
  'true', 'false',
  'uname', 'whoami', 'id', 'printenv', 'yes', 'rmdir', 'sleep', 'seq',
  'ln', 'readlink', 'realpath', 'mktemp', 'tac',
  'xargs', 'expr',
  'diff',
  'du', 'df',
  'gzip', 'gunzip', 'tar',
  'bc', 'dc',
  'sqlite3',
  'hostname', 'base64', 'sha256sum', 'md5sum', 'stat', 'xxd', 'rev', 'nproc',
  'fmt', 'fold', 'nl', 'expand', 'unexpand', 'paste', 'comm', 'join',
  'split', 'strings', 'od', 'cksum', 'truncate',
  'tree', 'patch', 'file', 'column', 'cmp', 'timeout', 'numfmt', 'csplit', 'zip', 'unzip',
  'rg',
];

function wasmName(tool: string): string {
  if (tool === 'true') return 'true-cmd.wasm';
  if (tool === 'false') return 'false-cmd.wasm';
  if (tool === 'gunzip') return 'gzip.wasm';
  return `${tool}.wasm`;
}

describe('grep conformance', () => {
  let vfs: VFS;
  let runner: ShellRunner;

  beforeEach(() => {
    vfs = new VFS();
    const adapter = new NodeAdapter();
    const mgr = new ProcessManager(vfs, adapter);
    for (const tool of TOOLS) {
      mgr.registerTool(tool, resolve(FIXTURES, wasmName(tool)));
    }
    runner = new ShellRunner(vfs, mgr, adapter, SHELL_WASM);

    // Create test files
    vfs.writeFile('/home/user/test.txt', new TextEncoder().encode(
      'hello world\nHello World\nfoo bar\n123 numbers\ntest_line\nHELLO LOUD\n'
    ));
    vfs.writeFile('/home/user/code.rs', new TextEncoder().encode(
      'fn main() {\n    println!("hello");\n    let x = 42;\n}\n'
    ));
    vfs.writeFile('/home/user/data.csv', new TextEncoder().encode(
      'name,age,city\nalice,30,new york\nbob,25,london\ncharlie,35,paris\n'
    ));
  });

  // ---- Anchors ----
  describe('anchors', () => {
    it('^ matches start of line', async () => {
      const r = await runner.run('grep "^hello" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
      expect(r.stdout).not.toContain('Hello World');
    });

    it('$ matches end of line', async () => {
      const r = await runner.run('grep "bar$" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('foo bar');
    });

    it('^...$ matches entire line', async () => {
      const r = await runner.run('grep "^foo bar$" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout.trim()).toBe('foo bar');
    });
  });

  // ---- Character classes ----
  describe('character classes', () => {
    it('[0-9] matches digits', async () => {
      const r = await runner.run('grep "[0-9]" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('123 numbers');
    });

    it('[a-z] matches lowercase letters', async () => {
      const r = await runner.run('grep "^[a-z]" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
      expect(r.stdout).toContain('foo bar');
      expect(r.stdout).not.toContain('HELLO LOUD');
    });

    it('[^0-9] matches non-digits (negation)', async () => {
      const r = await runner.run('grep "^[^0-9]" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).not.toContain('123 numbers');
      expect(r.stdout).toContain('hello world');
    });

    it('[A-Z] matches uppercase', async () => {
      const r = await runner.run('grep "^[A-Z]" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('Hello World');
      expect(r.stdout).toContain('HELLO LOUD');
      expect(r.stdout).not.toContain('hello world');
    });
  });

  // ---- Dot metacharacter ----
  describe('dot metacharacter', () => {
    it('. matches any character', async () => {
      const r = await runner.run('grep "h.llo" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
    });

    it('multiple dots match', async () => {
      const r = await runner.run('grep "f..\\sb" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('foo bar');
    });
  });

  // ---- Quantifiers (BRE mode) ----
  describe('quantifiers (BRE)', () => {
    it('* matches zero or more', async () => {
      const r = await runner.run('grep "fo*" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('foo bar');
    });

    it('.* matches anything', async () => {
      const r = await runner.run('grep "hello.*world" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
    });
  });

  // ---- Extended regex (-E flag) ----
  describe('extended regex (-E)', () => {
    it('+ matches one or more', async () => {
      const r = await runner.run('grep -E "[0-9]+" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('123 numbers');
    });

    it('? matches zero or one', async () => {
      const r = await runner.run('grep -E "colou?r" /home/user/test.txt');
      expect(r.exitCode).toBe(1); // no match expected
    });

    it('| alternation', async () => {
      const r = await runner.run('grep -E "hello|foo" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
      expect(r.stdout).toContain('foo bar');
    });

    it('() grouping with alternation', async () => {
      const r = await runner.run('grep -E "(hello|foo) " /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
      expect(r.stdout).toContain('foo bar');
    });
  });

  // ---- Case insensitive ----
  describe('case insensitive (-i)', () => {
    it('-i matches case-insensitively', async () => {
      const r = await runner.run('grep -i "hello" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
      expect(r.stdout).toContain('Hello World');
      expect(r.stdout).toContain('HELLO LOUD');
    });

    it('-i works with character classes', async () => {
      const r = await runner.run('grep -i "^hello" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
      expect(r.stdout).toContain('Hello World');
      expect(r.stdout).toContain('HELLO LOUD');
    });
  });

  // ---- Flags ----
  describe('flags', () => {
    it('-v inverts match', async () => {
      const r = await runner.run('grep -v "hello" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).not.toContain('hello world');
      expect(r.stdout).toContain('foo bar');
    });

    it('-c counts matches', async () => {
      const r = await runner.run('grep -c "hello" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout.trim()).toBe('1');
    });

    it('-n shows line numbers', async () => {
      const r = await runner.run('grep -n "foo" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toMatch(/^3:foo bar/m);
    });

    it('-l lists matching files', async () => {
      const r = await runner.run('grep -l "hello" /home/user/test.txt /home/user/code.rs');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('test.txt');
      expect(r.stdout).toContain('code.rs');
    });
  });

  // ---- Escape sequences ----
  describe('escape sequences', () => {
    it('\\d matches digits (BRE extension)', async () => {
      const r = await runner.run('grep "\\d" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('123 numbers');
    });

    it('\\w matches word chars', async () => {
      const r = await runner.run('grep "\\w\\w\\w_" /home/user/test.txt');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('test_line');
    });
  });

  // ---- Edge cases ----
  describe('edge cases', () => {
    it('empty file returns no match', async () => {
      vfs.writeFile('/home/user/empty.txt', new TextEncoder().encode(''));
      const r = await runner.run('grep "hello" /home/user/empty.txt');
      expect(r.exitCode).toBe(1);
    });

    it('stdin input works', async () => {
      const r = await runner.run('echo "hello world" | grep hello');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('hello world');
    });

    it('multiple files show filenames', async () => {
      const r = await runner.run('grep "hello" /home/user/test.txt /home/user/code.rs');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('/home/user/test.txt:');
      expect(r.stdout).toContain('/home/user/code.rs:');
    });

    it('exit code 2 on invalid regex', async () => {
      const r = await runner.run('grep "[invalid" /home/user/test.txt');
      expect(r.exitCode).toBe(2);
    });

    it('literal special chars with backslash', async () => {
      vfs.writeFile('/home/user/special.txt', new TextEncoder().encode('price: $10.00\nfoo.bar\n'));
      const r = await runner.run("grep '\\$10' /home/user/special.txt");
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('$10');
    });
  });

  // ---- Recursive ----
  describe('recursive (-r)', () => {
    it('-r searches directories', async () => {
      vfs.mkdir('/home/user/proj');
      vfs.mkdir('/home/user/proj/src');
      vfs.writeFile('/home/user/proj/src/main.rs', new TextEncoder().encode('fn main() {}\n'));
      vfs.writeFile('/home/user/proj/readme.txt', new TextEncoder().encode('main project\n'));
      const r = await runner.run('grep -r "main" /home/user/proj');
      expect(r.exitCode).toBe(0);
      expect(r.stdout).toContain('src/main.rs');
      expect(r.stdout).toContain('readme.txt');
    });
  });
});
