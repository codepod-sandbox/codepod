# Lean Binary + Native Module Bridge Design

## Problem

The full `python3.wasm` binary bundles numpy, pillow, pandas, etc. as compiled-in RustPython native modules. This makes the binary large (~28MB). A lean binary should start minimal and let users `pip install numpy` to dynamically load native modules via a host-mediated bridge.

## Goals

1. Build a lean `python3-lean.wasm` with only RustPython + stdlib + `_codepod` host bridge
2. `pip install numpy` on a lean binary downloads `numpy.wasm` + Python wrappers + a native bridge shim
3. `import numpy` works identically on both fat and lean binaries — same Python code, two execution paths
4. The bridge uses RPC-style JSON calls through a single `invoke(method, args)` function per native module

## Non-Goals

- Shared memory between WASM modules (too complex, negligible benefit for RPC)
- Near-native performance for bridged modules (that's what the fat binary is for)
- Dynamic loading of arbitrary C extensions

## Architecture

```
Fat binary (compiled-in):                Lean binary (bridge):
  Python: import numpy                     Python: import numpy
    └─ import _numpy_native                  └─ import _numpy_native
       └─ RustPython native module              └─ _numpy_native.py (shim)
          └─ Rust code (fast)                      └─ _codepod.native_call("numpy", ...)
                                                      └─ host_native_invoke (WASM import)
                                                         └─ Host loads numpy.wasm
                                                            └─ numpy.wasm invoke(method, args)
                                                               └─ Rust code (same as fat)
```

### Transparent Compatibility

The key insight: RustPython native modules take priority over PYTHONPATH `.py` files. So:

- **Fat binary**: `_numpy_native` resolves to the compiled-in native module. Any `_numpy_native.py` shim on PYTHONPATH is ignored.
- **Lean binary**: No compiled-in `_numpy_native`. RustPython finds `_numpy_native.py` on PYTHONPATH (installed by `pip install numpy`). The shim routes calls through the bridge.

The pure-Python wrappers (`numpy/__init__.py`, `numpy/core/`, etc.) are identical in both cases. They just call `_numpy_native.method(args)` and don't know whether it's native or bridged.

### Native Module WASM Interface

Each native module WASM (e.g., `numpy.wasm`) exports a single function:

```
invoke(method_ptr: i32, method_len: i32,
       args_ptr: i32, args_len: i32,
       out_ptr: i32, out_cap: i32) -> i32
```

- `method`: UTF-8 string — the function name (e.g., `"array_new"`, `"array_add"`)
- `args`: JSON-encoded argument list
- `out`: caller-provided output buffer for JSON response
- Returns: bytes written to output, or needed size if buffer too small (same pattern as `call_host_json`)

The WASM module is a thin wrapper around the existing Rust native code. It deserializes args, calls the Rust function, serializes the result.

### Host Dispatch

The orchestrator's kernel imports get a new function:

```typescript
// kernel-imports.ts
host_native_invoke(
  module_ptr: number, module_len: number,   // "numpy"
  method_ptr: number, method_len: number,   // "array_new"
  args_ptr: number, args_len: number,       // JSON args
  out_ptr: number, out_cap: number          // output buffer
): number
```

The host:
1. Reads the module name from WASM memory
2. Looks up the module's WASM instance (loaded by `pip install`, cached)
3. Calls `invoke()` on that WASM instance
4. Copies the result into the caller's output buffer

### Python Bridge Shim

When `pip install numpy` runs on a lean binary, it installs `_numpy_native.py`:

```python
"""Bridge shim for numpy native module — routes to WASM via _codepod."""
import _codepod
import json

def _call(method, *args, **kwargs):
    """Call a native numpy function via the host bridge."""
    result = _codepod.native_call("numpy", method,
                                   json.dumps(args), json.dumps(kwargs))
    return json.loads(result) if isinstance(result, str) else result

# Each function that _numpy_native normally exports:
def array_new(*args, **kwargs): return _call("array_new", *args, **kwargs)
def array_add(*args, **kwargs): return _call("array_add", *args, **kwargs)
# ... etc. Generated from the native module's function list.
```

The shim is auto-generated from the native module's exported function list.

### pip install Flow for Native Packages

```
pip install numpy (on lean binary)
  │
  ├─ 1. Download numpy.wasm from registry
  │     Write to /usr/share/pkg/bin/numpy-native.wasm
  │     Host loads it as a native module (not a tool)
  │
  ├─ 2. Download numpy wheel (Python wrappers)
  │     Extract to /usr/lib/python/numpy/
  │
  ├─ 3. Install _numpy_native.py bridge shim
  │     Write to /usr/lib/python/_numpy_native.py
  │
  └─ 4. Record in pip-installed.json
```

### Registry index.json Extension

```json
{
  "numpy": {
    "version": "1.26.4",
    "summary": "Numerical computing library",
    "native_wasm": "packages/numpy/numpy-native-1.26.4.wasm",
    "wheel": "packages/numpy/numpy-1.26.4-py3-none-any.whl",
    "native_shim": "packages/numpy/shims/_numpy_native.py",
    "depends": [],
    "size_bytes": 2400000
  }
}
```

New fields:
- `native_wasm`: path to the native module WASM (null for pure Python packages)
- `native_shim`: path to the `_foo_native.py` bridge shim

The existing `wasm` field is for standalone tool WASMs (registered via `register_tool`). `native_wasm` is for native Python module WASMs (loaded via the bridge).

### Host Native Module Registry

The orchestrator needs a registry of loaded native WASM modules:

```typescript
// process/manager.ts or new file
class NativeModuleRegistry {
  private modules: Map<string, WebAssembly.Instance> = new Map();

  async loadModule(name: string, wasmPath: string): Promise<void> {
    const module = await adapter.loadModule(wasmPath);
    const instance = await adapter.instantiate(module, {
      // Minimal WASI imports for memory allocation
      wasi_snapshot_preview1: { /* fd_write for debug, proc_exit, etc. */ }
    });
    this.modules.set(name, instance);
  }

  invoke(name: string, method: string, args: string): string {
    const instance = this.modules.get(name);
    if (!instance) throw new Error(`native module '${name}' not loaded`);
    // Call the invoke export, copy result from WASM memory
    // ... buffer management ...
  }
}
```

### Building the Lean Binary

```bash
# Fat binary (existing):
cargo build -p codepod-python --features numpy,pil,matplotlib --target wasm32-wasip1

# Lean binary (new):
cargo build -p codepod-python --target wasm32-wasip1
# No features = only RustPython + _codepod bridge
```

The lean binary is built with no feature flags. The `_codepod` module is always included (not feature-gated). The build produces `python3-lean.wasm`.

### Building Native Module WASMs

Each native module needs a standalone WASM binary that exports `invoke()`. This is a new crate that wraps the existing native code:

```
packages/python/crates/numpy-native-wasm/
  Cargo.toml  # depends on numpy-rust-core
  src/lib.rs  # exports invoke() function
```

The `invoke()` function:
1. Parses the method name and JSON args
2. Calls the corresponding numpy-rust-core function
3. Serializes the result as JSON
4. Writes to the output buffer

### Performance Expectations

| Operation | Fat binary | Lean + bridge |
|-----------|-----------|---------------|
| `import numpy` | ~50ms | ~200ms (load WASM + shim) |
| `np.array([1,2,3])` | ~0.1ms | ~5ms (JSON round-trip) |
| `np.dot(A, B)` for 100x100 | ~2ms | ~10ms (data serialization dominates) |
| `np.sum(large_array)` | ~1ms | ~50ms (serialize large array as JSON) |

For interactive/demo use, the bridge is acceptable. For compute-heavy workloads, use the fat binary.

### Future Optimization: Binary Protocol

The JSON serialization is the bottleneck. A future optimization could use a binary protocol (e.g., MessagePack or a custom format) that passes array data as typed buffers instead of JSON arrays. This would reduce the `np.sum(large_array)` case significantly but is not needed for the initial implementation.

## File Map

| File | Action |
|------|--------|
| `packages/python/Cargo.toml` | No change (lean = no features) |
| `packages/python/src/main.rs` | No change (cfg features already gate modules) |
| `packages/python/crates/codepod-host/src/lib.rs` | Modify — add `native_call()` function + `host_native_invoke` import |
| `packages/orchestrator/src/host-imports/kernel-imports.ts` | Modify — add `host_native_invoke` handler |
| `packages/orchestrator/src/process/native-modules.ts` | Create — NativeModuleRegistry |
| `packages/orchestrator/src/process/manager.ts` | Modify — integrate NativeModuleRegistry |
| `packages/shell-exec/src/virtual_commands.rs` | Modify — pip install handles `native_wasm` + `native_shim` fields |
| New crate: `packages/python/crates/numpy-native-wasm/` | Create — standalone numpy WASM with invoke() |
| `codepod-packages` repo | Update — add numpy native WASM + shim to registry |

## Testing

1. **Unit**: `invoke()` on numpy-native-wasm with known inputs
2. **Integration**: lean binary + `pip install numpy` + `python3 -c "import numpy; print(numpy.array([1,2,3]))"`
3. **Compatibility**: same numpy test suite passes on fat and lean binaries
4. **Fallback**: fat binary ignores `_numpy_native.py` shim (native module takes priority)
