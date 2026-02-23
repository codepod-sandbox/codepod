/**
 * NetworkBridge: sync-async bridge for WASI socket calls.
 *
 * Uses SharedArrayBuffer + Atomics to allow synchronous WASM code to
 * make network requests fulfilled asynchronously by a Worker.
 *
 * Protocol (over SharedArrayBuffer):
 *   Int32[0] = status: 0=idle, 1=request_ready, 2=response_ready, 3=error
 *   Int32[1] = data length (bytes)
 *   Bytes 8+ = JSON request or response payload
 */

import { Worker } from 'node:worker_threads';
import type { NetworkGateway } from './gateway.js';

const SAB_SIZE = 16 * 1024 * 1024; // 16MB
const STATUS_IDLE = 0;
const STATUS_REQUEST_READY = 1;
const STATUS_RESPONSE_READY = 2;
const STATUS_ERROR = 3;

export interface SyncFetchResult {
  status: number;
  body: string;
  headers: Record<string, string>;
  error?: string;
}

export class NetworkBridge {
  private sab: SharedArrayBuffer;
  private int32: Int32Array;
  private uint8: Uint8Array;
  private worker: Worker | null = null;
  private gateway: NetworkGateway;

  constructor(gateway: NetworkGateway) {
    this.gateway = gateway;
    this.sab = new SharedArrayBuffer(SAB_SIZE);
    this.int32 = new Int32Array(this.sab);
    this.uint8 = new Uint8Array(this.sab);
  }

  async start(): Promise<void> {
    const workerCode = `
      const { workerData } = require('node:worker_threads');
      const sab = workerData.sab;
      const int32 = new Int32Array(sab);
      const uint8 = new Uint8Array(sab);
      const encoder = new TextEncoder();
      const decoder = new TextDecoder();

      async function loop() {
        while (true) {
          Atomics.wait(int32, 0, ${STATUS_IDLE});
          if (Atomics.load(int32, 0) !== ${STATUS_REQUEST_READY}) continue;

          const len = Atomics.load(int32, 1);
          const reqJson = decoder.decode(uint8.slice(8, 8 + len));
          const req = JSON.parse(reqJson);

          try {
            const resp = await fetch(req.url, {
              method: req.method,
              headers: req.headers,
              body: req.body || undefined,
            });
            const body = await resp.text();
            const headers = {};
            resp.headers.forEach((v, k) => { headers[k] = v; });
            const result = JSON.stringify({ status: resp.status, body, headers });
            const encoded = encoder.encode(result);
            uint8.set(encoded, 8);
            Atomics.store(int32, 1, encoded.byteLength);
            Atomics.store(int32, 0, ${STATUS_RESPONSE_READY});
          } catch (err) {
            const result = JSON.stringify({ status: 0, body: '', headers: {}, error: err.message });
            const encoded = encoder.encode(result);
            uint8.set(encoded, 8);
            Atomics.store(int32, 1, encoded.byteLength);
            Atomics.store(int32, 0, ${STATUS_ERROR});
          }
          Atomics.notify(int32, 0);
        }
      }
      loop();
    `;

    this.worker = new Worker(workerCode, {
      eval: true,
      workerData: { sab: this.sab },
    });

    // Give the worker a moment to start its event loop
    await new Promise<void>((resolve) => setTimeout(resolve, 50));
  }

  /**
   * Synchronous fetch -- blocks the calling thread until the worker completes.
   * Safe to call from WASI host functions.
   */
  fetchSync(url: string, method: string, headers: Record<string, string>, body?: string): SyncFetchResult {
    // Check gateway policy synchronously first
    const access = this.gateway.checkAccess(url, method);
    if (!access.allowed) {
      return { status: 403, body: '', headers: {}, error: access.reason };
    }

    const encoder = new TextEncoder();
    const decoder = new TextDecoder();

    const reqJson = JSON.stringify({ url, method, headers, body });
    const reqEncoded = encoder.encode(reqJson);
    this.uint8.set(reqEncoded, 8);
    Atomics.store(this.int32, 1, reqEncoded.byteLength);
    Atomics.store(this.int32, 0, STATUS_REQUEST_READY);
    Atomics.notify(this.int32, 0);

    // Block until response
    Atomics.wait(this.int32, 0, STATUS_REQUEST_READY);

    const status = Atomics.load(this.int32, 0);
    const len = Atomics.load(this.int32, 1);
    const respJson = decoder.decode(this.uint8.slice(8, 8 + len));

    // Reset to idle
    Atomics.store(this.int32, 0, STATUS_IDLE);

    const result = JSON.parse(respJson) as SyncFetchResult;
    if (status === STATUS_ERROR) {
      result.error = result.error || 'unknown error';
    }
    return result;
  }

  dispose(): void {
    if (this.worker) {
      this.worker.terminate();
      this.worker = null;
    }
  }
}
