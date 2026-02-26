# Python Package System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add pip-discoverable, Rust-backed Python packages (numpy, pandas, PIL, matplotlib, sklearn, sqlite3, requests) with Cargo feature gating at build time and opt-in installation at sandbox creation time.

**Architecture:** A `PackageRegistry` holds metadata + Python wrapper files for each package. `Sandbox.create({ packages: ['numpy'] })` writes wrapper files to the VFS. The `python3.wasm` binary is built with Cargo features selecting which native modules (`_numpy_native`, `_pandas_native`, etc.) to compile in. `pip install` at runtime writes wrapper files from the registry (instant, no network).

**Tech Stack:** RustPython (`#[pymodule]`), Cargo features, ndarray, calamine, rust_xlsxwriter, image/imageproc, plotters, resvg, linfa, sqlite3 (C via wasi-sdk)

**Design doc:** `docs/plans/2026-02-25-package-system-design.md`

---

## Phase 1: Infrastructure

### Task 1: PackageRegistry

Create the registry that holds package metadata and Python wrapper file contents.

**Files:**
- Create: `packages/orchestrator/src/packages/registry.ts`
- Create: `packages/orchestrator/src/packages/types.ts`
- Create: `packages/orchestrator/src/packages/__tests__/registry.test.ts`

**Step 1: Write the failing test**

```typescript
// packages/orchestrator/src/packages/__tests__/registry.test.ts
import { describe, it, expect } from 'bun:test';
import { PackageRegistry } from '../registry';

describe('PackageRegistry', () => {
  it('lists available packages', () => {
    const reg = new PackageRegistry();
    const names = reg.available();
    expect(names).toBeInstanceOf(Array);
    expect(names.length).toBeGreaterThan(0);
    // requests is pure Python, always available regardless of features
    expect(names).toContain('requests');
  });

  it('returns metadata for a known package', () => {
    const reg = new PackageRegistry();
    const meta = reg.get('requests');
    expect(meta).toBeDefined();
    expect(meta!.name).toBe('requests');
    expect(meta!.version).toBeDefined();
    expect(meta!.pythonFiles).toBeDefined();
    expect(Object.keys(meta!.pythonFiles).length).toBeGreaterThan(0);
  });

  it('returns undefined for unknown package', () => {
    const reg = new PackageRegistry();
    expect(reg.get('nonexistent')).toBeUndefined();
  });

  it('resolves dependencies', () => {
    const reg = new PackageRegistry();
    const deps = reg.resolveDeps('pandas');
    expect(deps).toContain('numpy');
    expect(deps).toContain('pandas');
  });

  it('resolves packages with no deps to just themselves', () => {
    const reg = new PackageRegistry();
    const deps = reg.resolveDeps('requests');
    expect(deps).toEqual(['requests']);
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd packages/orchestrator && bun test src/packages/__tests__/registry.test.ts`
Expected: FAIL — module not found

**Step 3: Write types**

```typescript
// packages/orchestrator/src/packages/types.ts
export interface PackageMetadata {
  name: string;
  version: string;
  summary: string;
  dependencies: string[];
  /** Map of relative path -> file content, e.g. { 'numpy/__init__.py': '...' } */
  pythonFiles: Record<string, string>;
  /** If true, requires a native module compiled into python3.wasm */
  native: boolean;
}
```

**Step 4: Write PackageRegistry**

```typescript
// packages/orchestrator/src/packages/registry.ts
import type { PackageMetadata } from './types';

const PACKAGES: PackageMetadata[] = [
  {
    name: 'requests',
    version: '2.31.0',
    summary: 'HTTP library (wrapper over urllib.request)',
    dependencies: [],
    native: false,
    pythonFiles: {
      'requests/__init__.py': '# placeholder — real impl in Task 4',
    },
  },
  // Other packages added in later tasks
];

export class PackageRegistry {
  private packages = new Map<string, PackageMetadata>();

  constructor() {
    for (const pkg of PACKAGES) {
      this.packages.set(pkg.name, pkg);
    }
  }

  available(): string[] {
    return [...this.packages.keys()].sort();
  }

  get(name: string): PackageMetadata | undefined {
    return this.packages.get(name);
  }

  has(name: string): boolean {
    return this.packages.has(name);
  }

  /** Returns the package + all transitive dependencies, topologically sorted */
  resolveDeps(name: string): string[] {
    const result: string[] = [];
    const visited = new Set<string>();
    const visit = (n: string) => {
      if (visited.has(n)) return;
      visited.add(n);
      const pkg = this.packages.get(n);
      if (!pkg) return;
      for (const dep of pkg.dependencies) {
        visit(dep);
      }
      result.push(n);
    };
    visit(name);
    return result;
  }
}
```

Start with `requests` as the first package entry (pure Python, simplest). Placeholder Python files — real implementation is Task 4.

**Step 5: Run test to verify it passes**

Run: `cd packages/orchestrator && bun test src/packages/__tests__/registry.test.ts`
Expected: PASS

**Step 6: Commit**

```bash
git add packages/orchestrator/src/packages/
git commit -m "feat(packages): add PackageRegistry with types and requests stub"
```

---

### Task 2: Wire PackageRegistry into Sandbox.create()

Add `packages` option to `SandboxOptions` and install selected packages into the VFS at creation time.

**Files:**
- Modify: `packages/orchestrator/src/sandbox.ts` (SandboxOptions interface + create method)
- Create: `packages/orchestrator/src/__tests__/packages-integration.test.ts`

**Step 1: Write the failing test**

```typescript
// packages/orchestrator/src/__tests__/packages-integration.test.ts
import { describe, it, expect, afterEach } from 'bun:test';
import { Sandbox } from '../sandbox';
import { NodeAdapter } from '../platform/node-adapter';
import { resolve } from 'path';

const WASM_DIR = resolve(import.meta.dirname, '../platform/__tests__/fixtures');
const SHELL_WASM = resolve(import.meta.dirname, '../shell/__tests__/fixtures/wasmsand-shell.wasm');

describe('Sandbox packages option', () => {
  let sandbox: Sandbox;
  afterEach(() => { sandbox?.destroy(); });

  it('installs requested packages into VFS', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: ['requests'],
    });
    const result = await sandbox.run('python3 -c "import requests; print(requests.__version__)"');
    expect(result.stdout.trim()).toBe('2.31.0');
  });

  it('does not install packages not requested', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: [],
    });
    const result = await sandbox.run('python3 -c "import requests"');
    expect(result.exitCode).not.toBe(0);
  });

  it('auto-installs dependencies', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: ['pandas'],
    });
    // pandas depends on numpy, so numpy should be installed too
    const result = await sandbox.run('python3 -c "import numpy; print(\'ok\')"');
    expect(result.stdout.trim()).toBe('ok');
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd packages/orchestrator && bun test src/__tests__/packages-integration.test.ts`
Expected: FAIL — `packages` not a valid option / import fails

**Step 3: Add `packages` to SandboxOptions and wire into create()**

In `packages/orchestrator/src/sandbox.ts`:

1. Add to `SandboxOptions` interface:
```typescript
  /** Sandbox-native packages to pre-install (e.g. ['numpy', 'pandas']) */
  packages?: string[];
```

2. In `Sandbox.create()`, after extension installation and before PYTHONPATH setup, add:
```typescript
    // Install sandbox-native packages from PackageRegistry
    if (options.packages && options.packages.length > 0) {
      const pkgRegistry = new PackageRegistry();
      const toInstall = new Set<string>();
      for (const name of options.packages) {
        for (const dep of pkgRegistry.resolveDeps(name)) {
          toInstall.add(dep);
        }
      }
      vfs.withWriteAccess(() => {
        for (const name of toInstall) {
          const meta = pkgRegistry.get(name);
          if (!meta) continue;
          for (const [relPath, content] of Object.entries(meta.pythonFiles)) {
            const fullPath = `/usr/lib/python/${relPath}`;
            const dir = fullPath.substring(0, fullPath.lastIndexOf('/'));
            vfs.mkdirp(dir);
            vfs.writeFile(fullPath, new TextEncoder().encode(content));
          }
        }
      });
    }
```

3. Import `PackageRegistry`:
```typescript
import { PackageRegistry } from './packages/registry';
```

**Step 4: Run test to verify it passes**

Run: `cd packages/orchestrator && bun test src/__tests__/packages-integration.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add packages/orchestrator/src/sandbox.ts packages/orchestrator/src/__tests__/packages-integration.test.ts
git commit -m "feat(sandbox): add packages option to Sandbox.create() with auto-dependency resolution"
```

---

### Task 3: Enhance pip builtin

Upgrade the `pip` shell builtin to install/uninstall packages from `PackageRegistry` at runtime.

**Files:**
- Modify: `packages/orchestrator/src/shell/shell-runner.ts` (builtinPip method)
- Create: `packages/orchestrator/src/__tests__/pip-registry.test.ts`

**Step 1: Write the failing test**

```typescript
// packages/orchestrator/src/__tests__/pip-registry.test.ts
import { describe, it, expect, afterEach } from 'bun:test';
import { Sandbox } from '../sandbox';
import { NodeAdapter } from '../platform/node-adapter';
import { resolve } from 'path';

const WASM_DIR = resolve(import.meta.dirname, '../platform/__tests__/fixtures');
const SHELL_WASM = resolve(import.meta.dirname, '../shell/__tests__/fixtures/wasmsand-shell.wasm');

describe('pip with PackageRegistry', () => {
  let sandbox: Sandbox;
  afterEach(() => { sandbox?.destroy(); });

  it('pip install writes package files to VFS', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
    });
    const install = await sandbox.run('pip install requests');
    expect(install.exitCode).toBe(0);
    expect(install.stdout).toContain('Successfully installed requests');

    const check = await sandbox.run('python3 -c "import requests; print(requests.__version__)"');
    expect(check.stdout.trim()).toBe('2.31.0');
  });

  it('pip uninstall removes package files', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: ['requests'],
    });
    const uninstall = await sandbox.run('pip uninstall requests -y');
    expect(uninstall.exitCode).toBe(0);

    const check = await sandbox.run('python3 -c "import requests"');
    expect(check.exitCode).not.toBe(0);
  });

  it('pip list shows installed and available packages', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
      packages: ['requests'],
    });
    const result = await sandbox.run('pip list');
    expect(result.stdout).toContain('requests');
  });

  it('pip install unknown package fails with helpful message', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
    });
    const result = await sandbox.run('pip install nonexistent-pkg');
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain('not found');
    expect(result.stderr).toContain('Available');
  });

  it('pip install auto-installs dependencies', async () => {
    sandbox = await Sandbox.create({
      wasmDir: WASM_DIR,
      shellWasmPath: SHELL_WASM,
      adapter: new NodeAdapter(),
    });
    const result = await sandbox.run('pip install pandas');
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain('numpy');
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd packages/orchestrator && bun test src/__tests__/pip-registry.test.ts`
Expected: FAIL — pip install doesn't write files

**Step 3: Rewrite builtinPip()**

In `packages/orchestrator/src/shell/shell-runner.ts`:

1. Add `packageRegistry: PackageRegistry` field and `installedPackages: Set<string>` to ShellRunner.
2. Initialize in constructor or via setter from Sandbox.create(). When Sandbox.create() pre-installs packages, mark them in `installedPackages`.
3. Rewrite `builtinPip()` to support:
   - `pip install <name>` — lookup in PackageRegistry, write Python files to VFS via `vfs.withWriteAccess()`, add to `installedPackages`
   - `pip uninstall <name> [-y]` — remove Python files from VFS, remove from `installedPackages`
   - `pip list` — show installed packages (from `installedPackages` + extension registry for backwards compat)
   - `pip show <name>` — show package metadata from registry
   - Unknown package → error with list of available packages

**Step 4: Run test to verify it passes**

Run: `cd packages/orchestrator && bun test src/__tests__/pip-registry.test.ts`
Expected: PASS

**Step 5: Run full test suite**

Run: `cd packages/orchestrator && bun test`
Expected: All existing tests still pass (pip behavior is backwards-compatible)

**Step 6: Commit**

```bash
git add packages/orchestrator/src/shell/shell-runner.ts packages/orchestrator/src/__tests__/pip-registry.test.ts
git commit -m "feat(pip): rewrite pip builtin to install/uninstall from PackageRegistry"
```

---

### Task 4: requests package (pure Python)

The simplest package — pure Python wrapper over `urllib.request`. No native module needed. Validates the full pipeline end-to-end.

**Files:**
- Modify: `packages/orchestrator/src/packages/registry.ts` (replace placeholder with real Python files)

**Step 1: Write the Python wrapper files**

The `requests` package wraps `urllib.request`:
- `requests/__init__.py` — exports `get`, `post`, `put`, `delete`, `head`, `patch`, `Session`, `Response`, `__version__`
- `requests/api.py` — HTTP methods using `urllib.request.urlopen` + `urllib.request.Request`
- `requests/models.py` — `Response` class with `.text`, `.json()`, `.status_code`, `.headers`, `.content`, `.ok`, `.raise_for_status()`

Keep it minimal — match the most common usage patterns (simple GET/POST with JSON), not the full API.

**Step 2: Embed in PackageRegistry**

Replace the placeholder `requests` entry in `registry.ts` with real Python file contents.

**Step 3: Write integration test**

Add to `packages-integration.test.ts`:
```typescript
it('requests.get works with networking', async () => {
  sandbox = await Sandbox.create({
    wasmDir: WASM_DIR,
    shellWasmPath: SHELL_WASM,
    adapter: new NodeAdapter(),
    packages: ['requests'],
    network: { allow: ['*'] },
  });
  const result = await sandbox.run(
    'python3 -c "import requests; r = requests.get(\'https://httpbin.org/get\'); print(r.status_code)"'
  );
  expect(result.stdout.trim()).toBe('200');
});
```

**Step 4: Run test to verify it passes**

Run: `cd packages/orchestrator && bun test src/__tests__/packages-integration.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add packages/orchestrator/src/packages/
git commit -m "feat(packages): add requests package (pure Python over urllib)"
```

---

## Phase 2: Python WASM Build System

### Task 5: Cargo feature gates for python3.wasm

Restructure `packages/python/` to support Cargo features for each native module.

**Files:**
- Modify: `packages/python/Cargo.toml`
- Modify: `packages/python/src/main.rs`
- Modify: `packages/python/build.sh`

**Step 1: Update Cargo.toml with features**

```toml
[package]
name = "wasmsand-python"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "python3"
path = "src/main.rs"

[features]
default = []
all-packages = ["numpy", "pandas", "pil", "matplotlib", "sklearn", "sqlite3"]
numpy = ["dep:numpy-rust-python"]
pandas = ["numpy", "dep:pandas-native"]
pil = ["dep:pil-native"]
matplotlib = ["numpy", "dep:matplotlib-native"]
sklearn = ["numpy", "dep:sklearn-native"]
sqlite3 = ["dep:sqlite3-native"]

[dependencies]
rustpython = { git = "https://github.com/RustPython/RustPython", default-features = false, features = [
  "freeze-stdlib", "importlib", "stdio", "encodings", "host_env",
]}

# Native package crates (optional, gated by features)
numpy-rust-python = { path = "../../numpy-rust/crates/numpy-rust-python", optional = true }
pandas-native = { path = "crates/pandas", optional = true }
pil-native = { path = "crates/pil", optional = true }
matplotlib-native = { path = "crates/matplotlib", optional = true }
sklearn-native = { path = "crates/sklearn", optional = true }
sqlite3-native = { path = "crates/sqlite3", optional = true }
```

**Step 2: Update main.rs with conditional module registration**

```rust
use std::process::ExitCode;
use rustpython::InterpreterBuilderExt;

fn main() -> ExitCode {
    let config = rustpython::InterpreterBuilder::new().init_stdlib();

    #[cfg(feature = "numpy")]
    let config = config.add_native_module(numpy_rust_python::numpy_module_def(&config.ctx));

    #[cfg(feature = "pandas")]
    let config = config.add_native_module(pandas_native::module_def(&config.ctx));

    #[cfg(feature = "pil")]
    let config = config.add_native_module(pil_native::module_def(&config.ctx));

    #[cfg(feature = "matplotlib")]
    let config = config.add_native_module(matplotlib_native::module_def(&config.ctx));

    #[cfg(feature = "sklearn")]
    let config = config.add_native_module(sklearn_native::module_def(&config.ctx));

    #[cfg(feature = "sqlite3")]
    let config = config.add_native_module(sqlite3_native::module_def(&config.ctx));

    rustpython::run(config)
}
```

**Step 3: Update build.sh**

```bash
#!/bin/bash
set -e
FEATURES="${1:-}"
rustup target add wasm32-wasip1 2>/dev/null || true
if [ -n "$FEATURES" ]; then
  cargo build --release --target wasm32-wasip1 -p wasmsand-python --features "$FEATURES"
else
  cargo build --release --target wasm32-wasip1 -p wasmsand-python
fi
cp target/wasm32-wasip1/release/python3.wasm \
   packages/orchestrator/src/platform/__tests__/fixtures/python3.wasm
```

**Step 4: Verify bare build still works**

Run: `cd packages/python && bash build.sh`
Expected: Builds without errors, produces python3.wasm

**Step 5: Commit**

```bash
git add packages/python/Cargo.toml packages/python/src/main.rs packages/python/build.sh
git commit -m "feat(python): add Cargo feature gates for native package modules"
```

---

## Phase 3: Native Package Crates

Each native package follows the same pattern. Detailed task for **numpy** (leveraging existing `numpy-rust`), then summarized tasks for the rest.

### Task 6: numpy — integrate numpy-rust

Link the existing `numpy-rust` project into the python3.wasm build and add numpy to the PackageRegistry.

**Files:**
- Modify: `packages/python/Cargo.toml` (numpy dep already points at `../../numpy-rust/crates/numpy-rust-python`)
- Modify: `packages/orchestrator/src/packages/registry.ts` (add numpy entry)
- Create: `packages/orchestrator/src/packages/python/numpy/` (copy/adapt from `../numpy-rust/python/numpy/`)
- Modify: `packages/orchestrator/src/__tests__/packages-integration.test.ts` (add numpy tests)

**Step 1: Write the failing test**

```typescript
it('numpy array operations work', async () => {
  sandbox = await Sandbox.create({
    wasmDir: WASM_DIR,
    shellWasmPath: SHELL_WASM,
    adapter: new NodeAdapter(),
    packages: ['numpy'],
  });
  const result = await sandbox.run(
    'python3 -c "import numpy as np; a = np.array([1,2,3]); print(a.sum())"'
  );
  expect(result.stdout.trim()).toBe('6');
});
```

**Step 2: Copy Python wrapper from numpy-rust**

Copy `../numpy-rust/python/numpy/__init__.py` (and any submodules) into `packages/orchestrator/src/packages/python/numpy/`. Adapt if needed.

**Step 3: Add numpy to PackageRegistry**

In `registry.ts`, add numpy entry with `native: true` and the Python wrapper files.

**Step 4: Build python3.wasm with numpy feature**

Run: `cd packages/python && bash build.sh numpy`

**Step 5: Run test, commit**

```bash
git commit -m "feat(packages): add numpy package (ndarray-backed via numpy-rust)"
```

---

### Task 7: sqlite3 Python module

Wrap the existing C-compiled sqlite3 library with a RustPython native module.

**Files:**
- Create: `packages/python/crates/sqlite3/Cargo.toml`
- Create: `packages/python/crates/sqlite3/src/lib.rs`
- Modify: `packages/orchestrator/src/packages/registry.ts` (add sqlite3 entry)

**Context:** The sqlite3 C library is already compiled to WASM at `packages/sqlite/`. The Rust crate links against it via C FFI and exposes a `#[pymodule] _sqlite3_native` with `connect()`, `Connection`, `Cursor`, and exception types.

**Key APIs:** `sqlite3.connect(path)`, `Connection.execute()`, `Connection.cursor()`, `Connection.commit()`, `Connection.close()`, `Cursor.fetchone()`, `Cursor.fetchall()`, `Cursor.description`

**Test:**
```python
import sqlite3
conn = sqlite3.connect(':memory:')
conn.execute('CREATE TABLE t(x)')
conn.execute('INSERT INTO t VALUES(42)')
print(conn.execute('SELECT * FROM t').fetchone()[0])
# Expected: 42
```

```bash
git commit -m "feat(packages): add sqlite3 Python module wrapping C sqlite3 library"
```

---

### Task 8: PIL/Pillow — image crate

**Files:**
- Create: `packages/python/crates/pil/Cargo.toml` (deps: `image`, `imageproc`, `rustpython-vm`)
- Create: `packages/python/crates/pil/src/lib.rs`
- Modify: `packages/orchestrator/src/packages/registry.ts`

**Native module (`_pil_native`) key APIs:**
- `open(path)`, `new(mode, size, color)`, `resize(handle, w, h)`, `crop(handle, box)`, `rotate(handle, deg)`, `save(handle, path, fmt?)`, `convert(handle, mode)`, `getpixel(handle, x, y)`, `size(handle)`

**Python wrapper (`PIL/Image.py`):** Wraps native handles in an `Image` class matching Pillow's most-used API.

**Test:**
```python
from PIL import Image
img = Image.new('RGB', (100, 100), (255, 0, 0))
img.save('/tmp/test.png')
img2 = Image.open('/tmp/test.png')
print(img2.size)  # (100, 100)
```

```bash
git commit -m "feat(packages): add PIL package (image crate-backed)"
```

---

### Task 9: pandas — calamine + rust_xlsxwriter

**Files:**
- Create: `packages/python/crates/pandas/Cargo.toml` (deps: `calamine`, `rust_xlsxwriter`, `rustpython-vm`)
- Create: `packages/python/crates/pandas/src/lib.rs`
- Modify: `packages/orchestrator/src/packages/registry.ts`

**Native module (`_pandas_native`):** `read_excel(path, sheet?)` (calamine), `write_excel(path, headers, rows)` (rust_xlsxwriter)

**Python wrapper — mostly Python logic:** `DataFrame` class (dict-of-lists storage), `Series`, `read_csv()` (stdlib csv), `read_excel()` / `to_excel()` (native), `to_csv()`, `.head()`, `.tail()`, `.describe()`, `.groupby()`, `.sort_values()`, `.merge()`

**Test:**
```python
import pandas as pd
df = pd.DataFrame({'a': [1,2,3], 'b': [4,5,6]})
print(df.sum())
df.to_csv('/tmp/test.csv')
df2 = pd.read_csv('/tmp/test.csv')
print(len(df2))
```

```bash
git commit -m "feat(packages): add pandas package (calamine + xlsxwriter backed)"
```

---

### Task 10: matplotlib — plotters + resvg

**Files:**
- Create: `packages/python/crates/matplotlib/Cargo.toml` (deps: `plotters`, `resvg`, `rustpython-vm`)
- Create: `packages/python/crates/matplotlib/src/lib.rs`
- Modify: `packages/orchestrator/src/packages/registry.ts`

**Native module (`_matplotlib_native`):** `render_svg(plot_spec_json)` (plotters SVGBackend), `svg_to_png(svg, w, h)` (resvg)

**Python wrapper (`matplotlib/pyplot.py`):** Stateful figure/axes, `plot()`, `scatter()`, `bar()`, `hist()`, `xlabel()`, `ylabel()`, `title()`, `legend()`, `savefig(path, format='svg')`

**Test:**
```python
import matplotlib.pyplot as plt
plt.plot([1,2,3], [4,5,6])
plt.title('Test')
plt.savefig('/tmp/test.svg')
print(open('/tmp/test.svg').read()[:5])  # <svg or <?xml
```

```bash
git commit -m "feat(packages): add matplotlib package (plotters + resvg backed)"
```

---

### Task 11: sklearn — linfa

**Files:**
- Create: `packages/python/crates/sklearn/Cargo.toml` (deps: `linfa`, `linfa-clustering`, `linfa-reduction`, `linfa-linear`, `linfa-logistic`, `linfa-trees`, `rustpython-vm`)
- Create: `packages/python/crates/sklearn/src/lib.rs`
- Modify: `packages/orchestrator/src/packages/registry.ts`

**Native module (`_sklearn_native`):** `kmeans_fit()`, `pca_fit_transform()`, `linear_regression_fit()`, `logistic_regression_fit()`, `decision_tree_fit()`, `decision_tree_predict()` — data as flat arrays for numpy interop.

**Python wrapper — sklearn-compatible API:** `KMeans`, `PCA`, `LinearRegression`, `LogisticRegression`, `DecisionTreeClassifier`, `train_test_split` (pure Python), `StandardScaler` (pure Python)

**Test:**
```python
from sklearn.cluster import KMeans
import numpy as np
X = np.array([[1,2],[1,4],[1,0],[10,2],[10,4],[10,0]])
km = KMeans(n_clusters=2).fit(X)
print(sorted(set(km.labels_.tolist())))  # [0, 1]
```

```bash
git commit -m "feat(packages): add sklearn package (linfa-backed)"
```

---

## Phase 4: Final Integration

### Task 12: Full build and end-to-end test

Build `python3.wasm` with all features, run the full test suite, verify all packages work together.

**Step 1: Build with all features**

```bash
cd packages/python && bash build.sh all-packages
```

**Step 2: Run full test suite**

```bash
cd packages/orchestrator && bun test
```

**Step 3: Write end-to-end integration test**

```typescript
it('all packages work together', async () => {
  sandbox = await Sandbox.create({
    wasmDir: WASM_DIR,
    shellWasmPath: SHELL_WASM,
    adapter: new NodeAdapter(),
    packages: ['numpy', 'pandas', 'PIL', 'matplotlib', 'sklearn', 'sqlite3', 'requests'],
  });

  // numpy + pandas interop
  const result = await sandbox.run(`python3 -c "
import numpy as np
import pandas as pd
df = pd.DataFrame({'x': np.arange(5).tolist()})
print(len(df))
"`);
  expect(result.stdout.trim()).toBe('5');

  // sqlite3
  const sql = await sandbox.run(`python3 -c "
import sqlite3
conn = sqlite3.connect(':memory:')
conn.execute('CREATE TABLE t(x INTEGER)')
conn.execute('INSERT INTO t VALUES(1)')
print(conn.execute('SELECT * FROM t').fetchone()[0])
"`);
  expect(sql.stdout.trim()).toBe('1');

  // pip list shows all
  const pip = await sandbox.run('pip list');
  expect(pip.stdout).toContain('numpy');
  expect(pip.stdout).toContain('pandas');
  expect(pip.stdout).toContain('requests');
  expect(pip.stdout).toContain('sqlite3');
});
```

**Step 4: Commit and push**

```bash
git commit -m "feat(packages): full package system integration with all 7 packages"
git push
```

---

## Execution Order Summary

| Task | Package/Component | Native? | Complexity |
|------|-------------------|---------|------------|
| 1 | PackageRegistry + types | — | Low |
| 2 | Sandbox.create() wiring | — | Low |
| 3 | pip builtin rewrite | — | Medium |
| 4 | requests (pure Python) | No | Low |
| 5 | Cargo feature gates | — | Low |
| 6 | numpy (ndarray) | Yes | Medium (existing crate) |
| 7 | sqlite3 (C FFI) | Yes | Medium (C lib already compiled) |
| 8 | PIL (image crate) | Yes | High |
| 9 | pandas (calamine) | Yes | High (big Python layer) |
| 10 | matplotlib (plotters) | Yes | High |
| 11 | sklearn (linfa) | Yes | High |
| 12 | Full integration | — | Medium |

Tasks 1-4 are infrastructure. Tasks 5-6 leverage existing work. Tasks 7-11 are the bulk — each is a Rust crate + Python wrapper. Task 12 ties everything together.
