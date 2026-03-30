/// <reference path="./jspi.d.ts" />
/**
 * AsyncBridge — abstracts the calling convention between async JS host
 * functions and the synchronous-from-WASM import/export boundary.
 *
 * Three modes (selected at process start, or overridden in tests):
 *
 *  jspi      — WebAssembly.Suspending / WebAssembly.promising (JSPI proposal).
 *              WASM suspends on the JS stack when an import returns a Promise.
 *              Available: Deno 1.40+, Node 25+ (unflagged), Chrome 137+.
 *              WASM binary: standard build.
 *
 *  asyncify  — Binaryen Asyncify transform. Unwinds/rewinds the WASM stack on
 *              the JS side without JSPI. Binary is larger (~40%) but runs
 *              everywhere (Safari, Bun, older browsers).
 *              WASM binary: compiled with Asyncify ('-asyncify' suffix).
 *              TODO: needs -asyncify.wasm build artifacts and unwind/rewind
 *              driver in wrapExport().
 *
 *  threads   — WASM threads (wasi-threads proposal). WASM runs on a Worker
 *              thread; imports block synchronously via Atomics.wait on a
 *              SharedArrayBuffer; the main thread dispatches async host work.
 *              True parallelism — no JSPI or Asyncify needed.
 *              WASM binary: compiled with atomics + threads ('-threads' suffix).
 *              TODO: needs -threads.wasm build artifacts, Worker execution, and
 *              SAB-based request/response protocol.
 */

export type AsyncBridgeType = 'jspi' | 'asyncify' | 'threads';

export interface AsyncBridge {
  readonly type: AsyncBridgeType;

  /**
   * File suffix appended to the WASM binary name for this mode.
   * '' for JSPI (standard binary), '-asyncify', or '-threads'.
   */
  readonly binarySuffix: string;

  /**
   * Whether WASM memory must be SharedArrayBuffer-backed.
   * true only for 'threads' (required for Atomics operations).
   */
  readonly sharedMemory: boolean;

  /**
   * Wrap an async host function into a value suitable as a WASM import.
   *
   * jspi:     returns new WebAssembly.Suspending(fn) — WASM suspends when the
   *           import is called and resumes when the Promise resolves.
   * asyncify: returns a sync wrapper; the async result is stored in a pending
   *           slot and the Asyncify unwind/rewind loop (driven by wrapExport)
   *           re-enters WASM after the Promise resolves.
   * threads:  returns a sync-blocking function; blocks the Worker thread via
   *           Atomics.wait on a SharedArrayBuffer until the main thread has
   *           completed the async work and written the result.
   */
  wrapImport(fn: (...args: number[]) => Promise<number>): unknown;

  /**
   * Wrap a synchronous WASM export so the host can await its completion.
   *
   * jspi:     returns WebAssembly.promising(fn) — returns a Promise that
   *           resolves when WASM returns (possibly after multiple suspensions).
   * asyncify: returns a driver that calls fn(), detects asyncify unwinding,
   *           awaits the pending import Promise, then rewinds and re-enters.
   * threads:  returns a driver that posts the call to a Worker and returns a
   *           Promise that resolves when the Worker thread finishes.
   */
  wrapExport(fn: (...args: number[]) => number): (...args: number[]) => Promise<number>;
}

// ── JSPI ──────────────────────────────────────────────────────────────────────

class JspiAsyncBridge implements AsyncBridge {
  readonly type = 'jspi' as const;
  readonly binarySuffix = '';
  readonly sharedMemory = false;

  wrapImport(fn: (...args: number[]) => Promise<number>): unknown {
    return new WebAssembly.Suspending(fn);
  }

  wrapExport(fn: (...args: number[]) => number): (...args: number[]) => Promise<number> {
    return (WebAssembly as { promising?: (f: unknown) => unknown }).promising!(fn) as (
      ...args: number[]
    ) => Promise<number>;
  }
}

// ── Asyncify ──────────────────────────────────────────────────────────────────

/**
 * Asyncify bridge — Binaryen Asyncify transform.
 *
 * The WASM binary is built with:
 *   wasm-opt --asyncify --pass-arg=asyncify-imports@<list> -O1
 *
 * The binary exports asyncify_start_unwind / asyncify_stop_unwind /
 * asyncify_start_rewind / asyncify_stop_rewind / asyncify_get_state.
 *
 * Protocol (per import call that hits an async host function):
 *   1. WASM calls the import synchronously.
 *   2. wrapImport sees state=0, starts the async work, calls
 *      asyncify_start_unwind(dataAddr), returns 0 (ignored).
 *   3. WASM unwinds its call stack and returns to JS with state=1.
 *   4. wrapExport loop: stopUnwind → await promise → startRewind → re-enter.
 *   5. WASM rewinds back to the import call site; wrapImport sees state=2,
 *      calls stopRewind, returns the awaited result.
 *   6. WASM continues normally.
 *
 * Call initFromInstance() after WebAssembly.instantiate() to wire up the
 * asyncify exports and allocate the data buffer.
 */
class AsyncifyAsyncBridge implements AsyncBridge {
  readonly type = 'asyncify' as const;
  readonly binarySuffix = '-asyncify';
  readonly sharedMemory = false;

  // Set by initFromInstance() after the WASM instance is created.
  private exports: {
    startUnwind: (ptr: number) => void;
    stopUnwind: () => void;
    startRewind: (ptr: number) => void;
    stopRewind: () => void;
    getState: () => number;
    dataAddr: number;
  } | null = null;

  // Single pending slot — only one async import can be in-flight at a time
  // (WASM is single-threaded; imports don't interleave).
  private pendingPromise: Promise<void> | null = null;
  private pendingResult: number = 0;

  /**
   * Call once after instantiation.
   *
   * @param instance  The WebAssembly.Instance with asyncify exports.
   * @param dataAddr  Address of the pre-allocated asyncify data buffer (≥16 bytes
   *                  header + stack-save area).  The caller must have already
   *                  written the [start, end] header into WASM memory.
   */
  initFromInstance(instance: WebAssembly.Instance, dataAddr: number): void {
    const exp = instance.exports;
    this.exports = {
      startUnwind: exp.asyncify_start_unwind as (ptr: number) => void,
      stopUnwind:  exp.asyncify_stop_unwind  as () => void,
      startRewind: exp.asyncify_start_rewind as (ptr: number) => void,
      stopRewind:  exp.asyncify_stop_rewind  as () => void,
      getState:    exp.asyncify_get_state    as () => number,
      dataAddr,
    };
  }

  /**
   * Wrap an import function.  The returned sync function is used as a WASM
   * import.  It returns immediately if the underlying fn is synchronous.
   * If fn returns a Promise the wrapper starts an asyncify unwind so WASM
   * suspends until the Promise resolves.
   */
  wrapImport(fn: (...args: number[]) => Promise<number> | number): unknown {
    return (...args: number[]): number => {
      // Before initFromInstance (called during _start), forward calls synchronously.
      if (!this.exports) {
        const ret = fn(...args);
        if (ret instanceof Promise) return 0; // can't suspend yet — ignore async result
        return ret as number;
      }
      const exps = this.exports;
      // Rewinding: WASM is replaying the call — return the stored result.
      if (exps.getState() === 2) {
        exps.stopRewind();
        return this.pendingResult;
      }
      // Normal execution: call the actual host function.
      const ret = fn(...args);
      if (ret instanceof Promise) {
        // Async: save the promise and start unwinding the WASM stack.
        this.pendingPromise = ret.then(r => { this.pendingResult = r; });
        exps.startUnwind(exps.dataAddr);
        return 0; // ignored during unwind
      }
      // Synchronous: pass through directly, no unwind needed.
      return ret as number;
    };
  }

  /**
   * Wrap the __run_command export.  Drives the unwind/await/rewind loop
   * until WASM returns from __run_command without suspending.
   */
  wrapExport(fn: (...args: number[]) => number): (...args: number[]) => Promise<number> {
    return async (...args: number[]): Promise<number> => {
      const exps = this.exports!;
      let result = fn(...args);
      while (exps.getState() === 1) {
        // WASM unwound because an async import was called.
        exps.stopUnwind();
        await this.pendingPromise!;
        exps.startRewind(exps.dataAddr);
        result = fn(...args);
        // asyncify_stop_rewind is called inside wrapImport when state===2.
      }
      return result;
    };
  }
}

// ── Threads ───────────────────────────────────────────────────────────────────

/**
 * Threads bridge — not yet implemented.
 *
 * Requirements:
 * - WASM binary compiled with atomics + wasi-threads support.
 *   Build target: codepod-shell-exec-threads.wasm
 * - SharedArrayBuffer must be available (requires COOP/COEP headers in browsers,
 *   or Node/Deno with --experimental-sharedarraybuffer or equivalent).
 * - wrapImport: allocate a SAB slot; return a sync function that writes the
 *   request to shared memory, Atomics.notify(requestBuf, slot), then
 *   Atomics.wait(responseBuf, slot, 0) until the main thread completes and
 *   Atomics.notify(responseBuf, slot).
 * - wrapExport: spawn a Worker that receives the WASM module + SAB, runs the
 *   export on the thread, and postMessages the result back.
 * - Host dispatcher: a loop on the main thread (or dedicated thread) that
 *   Atomics.waitAsync(requestBuf, slot) and dispatches to the async host fns.
 */
class ThreadsAsyncBridge implements AsyncBridge {
  readonly type = 'threads' as const;
  readonly binarySuffix = '-threads';
  readonly sharedMemory = true;

  wrapImport(_fn: (...args: number[]) => Promise<number>): unknown {
    throw new Error(
      'ThreadsAsyncBridge is not yet implemented. ' +
        'Build codepod-shell-exec-threads.wasm with atomics + wasi-threads first.',
    );
  }

  wrapExport(_fn: (...args: number[]) => number): (...args: number[]) => Promise<number> {
    throw new Error(
      'ThreadsAsyncBridge is not yet implemented. ' +
        'Build codepod-shell-exec-threads.wasm with atomics + wasi-threads first.',
    );
  }
}

// ── Detection ─────────────────────────────────────────────────────────────────

/**
 * Detect and return the best available AsyncBridge for this runtime.
 *
 * Priority:
 * 1. JSPI (WebAssembly.Suspending available) — Deno, Node 25+, Chrome 137+
 * 2. Threads (SharedArrayBuffer + Atomics + wasi-threads binary) — future
 * 3. Asyncify (fallback for Safari, Bun, older environments) — future
 *
 * Currently only JSPI is fully implemented. The others exist as typed stubs
 * so the interface is forward-compatible.
 */
export function detectAsyncBridge(): AsyncBridge {
  if (typeof WebAssembly.Suspending === 'function') {
    return new JspiAsyncBridge();
  }
  // TODO: check for threads when binary is available:
  //   typeof SharedArrayBuffer !== 'undefined' && typeof Atomics !== 'undefined'
  // Asyncify fallback: works everywhere (Safari, Bun, older browsers).
  // Requires codepod-shell-exec-asyncify.wasm (built with wasm-opt --asyncify).
  return new AsyncifyAsyncBridge();
}

// Export implementations for tests and explicit construction.
export { JspiAsyncBridge, AsyncifyAsyncBridge, ThreadsAsyncBridge };
