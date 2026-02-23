import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { NetworkBridge } from '../bridge.js';
import { NetworkGateway } from '../gateway.js';
import { WasiHost } from '../../wasi/wasi-host.js';
import { VFS } from '../../vfs/vfs.js';
import { spawn, type ChildProcess } from 'node:child_process';

/**
 * Tests for the control fd protocol in WasiHost.
 *
 * We test the handleControlCommand method directly -- it's exposed
 * publicly for testing. The fdWrite/fdRead integration is tested
 * end-to-end via Python in Task 4.
 */

// Use a child-process HTTP server to avoid Atomics.wait deadlock
let serverProcess: ChildProcess;
let baseUrl: string;

beforeEach(async () => {
  const serverScript = `
    const http = require('node:http');
    const server = http.createServer((req, res) => {
      let body = '';
      req.on('data', c => body += c);
      req.on('end', () => {
        if (req.url === '/data') {
          res.writeHead(200, { 'Content-Type': 'text/plain' });
          res.end('control fd response');
          return;
        }
        if (req.url === '/echo') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ method: req.method, body }));
          return;
        }
        res.writeHead(404);
        res.end('not found');
      });
    });
    server.listen(0, '127.0.0.1', () => {
      process.stdout.write(JSON.stringify({ port: server.address().port }) + '\\n');
    });
  `;

  serverProcess = spawn(process.execPath, ['-e', serverScript], {
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  const port = await new Promise<number>((resolve, reject) => {
    let output = '';
    serverProcess.stdout!.on('data', (chunk: Buffer) => {
      output += chunk.toString();
      for (const line of output.split('\n')) {
        if (line.trim()) {
          try {
            const info = JSON.parse(line.trim());
            if (info.port) { resolve(info.port); return; }
          } catch {}
        }
      }
    });
    serverProcess.on('error', reject);
    setTimeout(() => reject(new Error('Timeout')), 5000);
  });

  baseUrl = `http://127.0.0.1:${port}`;
});

afterEach(() => {
  serverProcess?.kill();
});

describe('WasiHost control fd', () => {
  it('handles connect command', () => {
    const vfs = new VFS();
    const gateway = new NetworkGateway({ allowedHosts: ['127.0.0.1'] });
    const bridge = new NetworkBridge(gateway);
    const host = new WasiHost({
      vfs,
      args: ['test'],
      env: {},
      preopens: { '/': '/' },
      networkBridge: bridge,
    });

    const resp = host.handleControlCommand({ cmd: 'connect', host: '127.0.0.1', port: 80 });
    expect(resp.ok).toBe(true);
    expect(resp.id).toBeDefined();
  });

  it('handles request command via bridge', async () => {
    const vfs = new VFS();
    const gateway = new NetworkGateway({ allowedHosts: ['127.0.0.1'] });
    const bridge = new NetworkBridge(gateway);
    await bridge.start();

    const host = new WasiHost({
      vfs,
      args: ['test'],
      env: {},
      preopens: { '/': '/' },
      networkBridge: bridge,
    });

    // First connect
    const connResp = host.handleControlCommand({ cmd: 'connect', host: '127.0.0.1', port: Number(new URL(baseUrl).port) });
    const connId = connResp.id;

    // Then request
    const reqResp = host.handleControlCommand({
      cmd: 'request',
      id: connId,
      method: 'GET',
      path: '/data',
      headers: {},
      body: '',
    });

    expect(reqResp.ok).toBe(true);
    expect(reqResp.status).toBe(200);
    expect(reqResp.body).toBe('control fd response');

    bridge.dispose();
  });

  it('handles POST request with body', async () => {
    const vfs = new VFS();
    const gateway = new NetworkGateway({ allowedHosts: ['127.0.0.1'] });
    const bridge = new NetworkBridge(gateway);
    await bridge.start();

    const host = new WasiHost({
      vfs,
      args: ['test'],
      env: {},
      preopens: { '/': '/' },
      networkBridge: bridge,
    });

    const connResp = host.handleControlCommand({ cmd: 'connect', host: '127.0.0.1', port: Number(new URL(baseUrl).port) });

    const reqResp = host.handleControlCommand({
      cmd: 'request',
      id: connResp.id,
      method: 'POST',
      path: '/echo',
      headers: { 'Content-Type': 'text/plain' },
      body: 'hello world',
    });

    expect(reqResp.ok).toBe(true);
    expect(reqResp.status).toBe(200);
    const parsed = JSON.parse(reqResp.body as string);
    expect(parsed.method).toBe('POST');
    expect(parsed.body).toBe('hello world');

    bridge.dispose();
  });

  it('handles close command', () => {
    const vfs = new VFS();
    const gateway = new NetworkGateway({ allowedHosts: ['127.0.0.1'] });
    const bridge = new NetworkBridge(gateway);
    const host = new WasiHost({
      vfs,
      args: ['test'],
      env: {},
      preopens: { '/': '/' },
      networkBridge: bridge,
    });

    const connResp = host.handleControlCommand({ cmd: 'connect', host: '127.0.0.1', port: 80 });
    const closeResp = host.handleControlCommand({ cmd: 'close', id: connResp.id });
    expect(closeResp.ok).toBe(true);
  });

  it('returns error when no bridge is configured', () => {
    const vfs = new VFS();
    const host = new WasiHost({
      vfs,
      args: ['test'],
      env: {},
      preopens: { '/': '/' },
    });

    const resp = host.handleControlCommand({ cmd: 'connect', host: 'example.com', port: 80 });
    expect(resp.ok).toBe(false);
    expect(resp.error).toBeDefined();
  });
});
