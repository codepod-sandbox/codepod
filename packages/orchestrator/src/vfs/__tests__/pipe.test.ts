import { describe, it } from '@std/testing/bdd';
import { expect } from '@std/expect';
import { createAsyncPipe } from '../pipe.js';

describe('AsyncPipe', () => {
  it('write then read returns data', async () => {
    const [read, write] = createAsyncPipe();
    const data = new TextEncoder().encode('hello');
    write.write(data);
    const buf = new Uint8Array(16);
    const n = await read.read(buf);
    expect(n).toBe(5);
    expect(new TextDecoder().decode(buf.subarray(0, n))).toBe('hello');
    read.close();
    write.close();
  });

  it('read on empty pipe resolves when data arrives', async () => {
    const [read, write] = createAsyncPipe();
    const buf = new Uint8Array(16);
    const readPromise = read.read(buf);
    // Read is pending — write should unblock it
    const data = new TextEncoder().encode('world');
    write.write(data);
    const n = await readPromise;
    expect(n).toBe(5);
    expect(new TextDecoder().decode(buf.subarray(0, n))).toBe('world');
    read.close();
    write.close();
  });

  it('read returns 0 on EOF (write end closed, buffer empty)', async () => {
    const [read, write] = createAsyncPipe();
    write.close();
    const buf = new Uint8Array(16);
    const n = await read.read(buf);
    expect(n).toBe(0);
    read.close();
  });

  it('read drains buffer before returning EOF', async () => {
    const [read, write] = createAsyncPipe();
    write.write(new TextEncoder().encode('data'));
    write.close();
    const buf = new Uint8Array(16);
    const n1 = await read.read(buf);
    expect(n1).toBe(4);
    const n2 = await read.read(buf);
    expect(n2).toBe(0); // EOF after drain
    read.close();
  });

  it('write returns -1 (EPIPE) when read end closed', () => {
    const [read, write] = createAsyncPipe();
    read.close();
    const result = write.write(new TextEncoder().encode('data'));
    expect(result).toBe(-1); // EPIPE
    write.close();
  });

  it('back-pressure: write blocks when pipe full', async () => {
    const [read, write] = createAsyncPipe(64); // 64 byte capacity
    const big = new Uint8Array(64).fill(0x41);
    write.write(big); // fills pipe
    // Next write should return a Promise (blocked)
    const smallData = new Uint8Array(1).fill(0x42);
    const writePromise = write.writeAsync(smallData);
    // Drain some data to unblock
    const buf = new Uint8Array(32);
    await read.read(buf);
    await writePromise; // should resolve now
    read.close();
    write.close();
  });
});
