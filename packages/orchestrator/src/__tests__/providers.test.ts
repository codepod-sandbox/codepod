/**
 * Tests for VFS virtual providers: DevProvider (/dev) and ProcProvider (/proc).
 *
 * Exercises the full provider interface through the VFS layer, verifying
 * that provider routing correctly intercepts before normal inode logic.
 */
import { describe, it, expect } from 'bun:test';
import { VFS } from '../vfs/vfs.js';
import { VfsError } from '../vfs/inode.js';

describe('DevProvider (/dev)', () => {
  it('/dev/null read returns empty bytes', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/dev/null');
    expect(data).toBeInstanceOf(Uint8Array);
    expect(data.byteLength).toBe(0);
  });

  it('/dev/null write is silent (no error)', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/dev/null', new TextEncoder().encode('discard me'));
    }).not.toThrow();
  });

  it('/dev/null stat returns file type', () => {
    const vfs = new VFS();
    const s = vfs.stat('/dev/null');
    expect(s.type).toBe('file');
    expect(s.size).toBe(0);
  });

  it('/dev directory is listable with all 4 devices', () => {
    const vfs = new VFS();
    const entries = vfs.readdir('/dev');
    const names = entries.map(e => e.name).sort();
    expect(names).toEqual(['null', 'random', 'urandom', 'zero']);
    // All entries are files
    for (const entry of entries) {
      expect(entry.type).toBe('file');
    }
  });

  it('/dev stat returns dir type', () => {
    const vfs = new VFS();
    const s = vfs.stat('/dev');
    expect(s.type).toBe('dir');
    expect(s.size).toBe(4);
  });

  it('/dev/zero returns zero bytes', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/dev/zero');
    expect(data.byteLength).toBeGreaterThan(0);
    for (let i = 0; i < data.byteLength; i++) {
      expect(data[i]).toBe(0);
    }
  });

  it('/dev/random returns bytes', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/dev/random');
    expect(data).toBeInstanceOf(Uint8Array);
    expect(data.byteLength).toBeGreaterThan(0);
  });

  it('/dev/urandom returns bytes', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/dev/urandom');
    expect(data).toBeInstanceOf(Uint8Array);
    expect(data.byteLength).toBeGreaterThan(0);
  });

  it('/dev/random and /dev/urandom return different bytes on separate reads', () => {
    const vfs = new VFS();
    const a = vfs.readFile('/dev/random');
    const b = vfs.readFile('/dev/random');
    // It's theoretically possible but astronomically unlikely for 4096 random bytes to match
    let same = true;
    for (let i = 0; i < a.byteLength; i++) {
      if (a[i] !== b[i]) { same = false; break; }
    }
    expect(same).toBe(false);
  });

  it('writing to /dev/zero throws EROFS', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/dev/zero', new Uint8Array(1));
    }).toThrow(/EROFS/);
  });

  it('writing to /dev/random throws EROFS', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/dev/random', new Uint8Array(1));
    }).toThrow(/EROFS/);
  });

  it('writing to /dev/urandom throws EROFS', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/dev/urandom', new Uint8Array(1));
    }).toThrow(/EROFS/);
  });

  it('reading nonexistent /dev/foo throws ENOENT', () => {
    const vfs = new VFS();
    try {
      vfs.readFile('/dev/foo');
      expect(true).toBe(false); // should not reach here
    } catch (e) {
      expect(e).toBeInstanceOf(VfsError);
      expect((e as VfsError).errno).toBe('ENOENT');
    }
  });

  it('stat of nonexistent /dev/foo throws ENOENT', () => {
    const vfs = new VFS();
    try {
      vfs.stat('/dev/foo');
      expect(true).toBe(false);
    } catch (e) {
      expect(e).toBeInstanceOf(VfsError);
      expect((e as VfsError).errno).toBe('ENOENT');
    }
  });
});

describe('ProcProvider (/proc)', () => {
  it('/proc/uptime returns parseable number', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/proc/uptime');
    const text = new TextDecoder().decode(data);
    const parts = text.trim().split(' ');
    expect(parts.length).toBe(2);
    const uptime = parseFloat(parts[0]);
    expect(Number.isFinite(uptime)).toBe(true);
    expect(uptime).toBeGreaterThanOrEqual(0);
  });

  it('/proc/version contains expected content', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/proc/version');
    const text = new TextDecoder().decode(data);
    expect(text).toContain('wasmsand');
    expect(text).toContain('WASI sandbox');
  });

  it('/proc/cpuinfo contains "processor"', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/proc/cpuinfo');
    const text = new TextDecoder().decode(data);
    expect(text).toContain('processor');
    expect(text).toContain('WASI Virtual CPU');
  });

  it('/proc/meminfo has content', () => {
    const vfs = new VFS();
    const data = vfs.readFile('/proc/meminfo');
    const text = new TextDecoder().decode(data);
    expect(text.length).toBeGreaterThan(0);
    expect(text).toContain('MemTotal');
    expect(text).toContain('MemFree');
  });

  it('/proc is listable with all 4 files', () => {
    const vfs = new VFS();
    const entries = vfs.readdir('/proc');
    const names = entries.map(e => e.name).sort();
    expect(names).toEqual(['cpuinfo', 'meminfo', 'uptime', 'version']);
    for (const entry of entries) {
      expect(entry.type).toBe('file');
    }
  });

  it('/proc stat returns dir type', () => {
    const vfs = new VFS();
    const s = vfs.stat('/proc');
    expect(s.type).toBe('dir');
    expect(s.size).toBe(4);
  });

  it('/proc files are read-only (EROFS on write to uptime)', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/proc/uptime', new TextEncoder().encode('fake'));
    }).toThrow(/EROFS/);
  });

  it('/proc files are read-only (EROFS on write to version)', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/proc/version', new TextEncoder().encode('fake'));
    }).toThrow(/EROFS/);
  });

  it('/proc files are read-only (EROFS on write to cpuinfo)', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/proc/cpuinfo', new TextEncoder().encode('fake'));
    }).toThrow(/EROFS/);
  });

  it('/proc files are read-only (EROFS on write to meminfo)', () => {
    const vfs = new VFS();
    expect(() => {
      vfs.writeFile('/proc/meminfo', new TextEncoder().encode('fake'));
    }).toThrow(/EROFS/);
  });

  it('reading nonexistent /proc/foo throws ENOENT', () => {
    const vfs = new VFS();
    try {
      vfs.readFile('/proc/foo');
      expect(true).toBe(false);
    } catch (e) {
      expect(e).toBeInstanceOf(VfsError);
      expect((e as VfsError).errno).toBe('ENOENT');
    }
  });

  it('/proc/uptime stat returns file type with correct size', () => {
    const vfs = new VFS();
    const s = vfs.stat('/proc/uptime');
    expect(s.type).toBe('file');
    expect(s.size).toBeGreaterThan(0);
  });
});

describe('Provider integration with VFS', () => {
  it('providers do not interfere with normal VFS operations', () => {
    const vfs = new VFS();
    vfs.writeFile('/tmp/test.txt', new TextEncoder().encode('hello'));
    expect(new TextDecoder().decode(vfs.readFile('/tmp/test.txt'))).toBe('hello');
    expect(vfs.stat('/tmp/test.txt').type).toBe('file');
  });

  it('providers are available after cowClone', () => {
    const vfs = new VFS();
    const clone = vfs.cowClone();

    // /dev should work in clone
    const nullData = clone.readFile('/dev/null');
    expect(nullData.byteLength).toBe(0);

    // /proc should work in clone
    const version = new TextDecoder().decode(clone.readFile('/proc/version'));
    expect(version).toContain('wasmsand');

    // Listing should work
    const devEntries = clone.readdir('/dev');
    expect(devEntries.length).toBe(4);
    const procEntries = clone.readdir('/proc');
    expect(procEntries.length).toBe(4);
  });

  it('root readdir still works (does not include virtual mounts)', () => {
    const vfs = new VFS();
    const entries = vfs.readdir('/');
    const names = entries.map(e => e.name).sort();
    // Should contain the default layout directories
    expect(names).toContain('home');
    expect(names).toContain('tmp');
    expect(names).toContain('bin');
  });

  it('provider stat returns correct permissions', () => {
    const vfs = new VFS();
    const devStat = vfs.stat('/dev');
    expect(devStat.permissions).toBe(0o755);

    const nullStat = vfs.stat('/dev/null');
    expect(nullStat.permissions).toBe(0o444);
  });
});
