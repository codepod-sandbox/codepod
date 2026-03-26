# Codepod Package Registry Design

## Problem

The full codepod sandbox binary bundles numpy, matplotlib, pillow, and (soon) pandas as compiled-in WASM modules. This makes the binary large. A "lean" sandbox should start minimal and install packages on demand via `pip install`, downloading pre-built WASM binaries and Python wheels from a codepod-controlled registry.

## Goals

1. `pip install numpy` downloads pre-built `numpy.wasm` + Python wrappers from the registry
2. `pip install tabulate` downloads a pure-Python wheel and extracts .py files to the VFS
3. All packages served from a single codepod-controlled GitHub Pages repository
4. No PyPI fallback — only packages in the registry are installable
5. CI pipeline builds the registry from source repos and curated PyPI wheels

## Non-Goals

- pip dependency resolution beyond one level (keep it simple: registry declares deps, install them in order)
- PyPI fallback or arbitrary package installation
- Compiling packages at runtime
- Virtual environments or isolation between installed packages

## Architecture

```
codepod-packages repo (GitHub Pages)
├── index.json                    # Package index
├── numpy/
│   ├── numpy-1.26.4.wasm        # Pre-compiled WASM binary
│   └── numpy-1.26.4-py3-none-any.whl  # Python wrapper wheel
├── pandas/
│   ├── pandas-2.2.0.wasm
│   └── pandas-2.2.0-py3-none-any.whl
├── tabulate/
│   └── tabulate-0.9.0-py3-none-any.whl  # Pure Python, no WASM
└── scripts/
    ├── build-index.py            # Regenerates index.json
    ├── add-pypi-package.py       # Downloads + validates a PyPI wheel
    └── add-wasm-package.py       # Packages a WASM build + Python wrappers
```

### index.json Format

```json
{
  "version": 1,
  "packages": {
    "numpy": {
      "version": "1.26.4",
      "summary": "Numerical computing library",
      "wasm": "numpy/numpy-1.26.4.wasm",
      "wheel": "numpy/numpy-1.26.4-py3-none-any.whl",
      "depends": [],
      "size_bytes": 2400000
    },
    "pandas": {
      "version": "2.2.0",
      "summary": "Data analysis library",
      "wasm": "pandas/pandas-2.2.0.wasm",
      "wheel": "pandas/pandas-2.2.0-py3-none-any.whl",
      "depends": ["numpy"],
      "size_bytes": 1800000
    },
    "tabulate": {
      "version": "0.9.0",
      "summary": "Pretty-print tabular data",
      "wasm": null,
      "wheel": "tabulate/tabulate-0.9.0-py3-none-any.whl",
      "depends": [],
      "size_bytes": 48000
    }
  }
}
```

Fields:
- `version`: index format version (for future compat)
- `wasm`: path to WASM binary (null for pure-Python packages)
- `wheel`: path to Python wheel (always present)
- `depends`: list of package names that must be installed first
- `size_bytes`: total download size for progress/limits

### Registry URL Configuration

The registry base URL is compiled into the shell-exec binary:

```rust
// packages/shell-exec/src/virtual_commands.rs
const CODEPOD_REGISTRY_URL: &str = "https://codepod-sandbox.github.io/packages";
```

This can be overridden at runtime via the `CODEPOD_REGISTRY` environment variable, which the orchestrator can set when creating the sandbox.

## pip install Flow

```
pip install pandas
    │
    ├─ 1. Check BUILTIN_PACKAGES → not found (lean binary)
    │
    ├─ 2. Check pip-installed.json → not already installed
    │
    ├─ 3. Fetch {REGISTRY_URL}/index.json
    │     (cached in /etc/codepod/registry-index.json after first fetch)
    │
    ├─ 4. Look up "pandas" in index
    │     → version 2.2.0, depends: [numpy], has WASM + wheel
    │
    ├─ 5. Resolve dependencies (topological sort)
    │     → install order: [numpy, pandas]
    │
    ├─ 6. For each package:
    │     a. If wasm != null:
    │        - Fetch {REGISTRY_URL}/{wasm_path}
    │        - Write to /usr/share/pkg/bin/{name}.wasm
    │        - Register with host.register_tool()
    │     b. Fetch {REGISTRY_URL}/{wheel_path}
    │        - Extract .py files from wheel to /usr/lib/python/
    │     c. Record in pip-installed.json
    │
    └─ 7. Print "Successfully installed numpy-1.26.4 pandas-2.2.0"
```

### Wheel Extraction

A Python wheel (.whl) is a ZIP file. Structure:
```
tabulate-0.9.0-py3-none-any.whl (ZIP)
├── tabulate/__init__.py
├── tabulate/tabulate.py
├── tabulate-0.9.0.dist-info/
│   ├── METADATA
│   ├── RECORD
│   └── WHEEL
```

Extraction rules:
1. For each entry in the ZIP:
   - Skip `*.dist-info/` directories (metadata, not needed at runtime)
   - Skip `*.data/` directories (scripts, headers — not applicable)
   - Extract everything else to `/usr/lib/python/`
2. File paths in the ZIP are relative, so `tabulate/__init__.py` becomes `/usr/lib/python/tabulate/__init__.py`

### ZIP Parsing in Rust

The shell-exec binary needs minimal ZIP support. A wheel ZIP uses store or deflate compression. Options:

**A) Use the `zip` crate** — full-featured, adds ~50KB to WASM binary. Handles all compression methods.

**B) Minimal custom parser** — wheels from PyPI almost always use store (no compression) or deflate. A 200-line parser handles the common case.

**Recommendation: Use the `zip` crate.** It's battle-tested, handles edge cases, and 50KB is negligible. Add `zip = { version = "2", default-features = false, features = ["deflate"] }` to Cargo.toml.

### Index Caching

The registry index is fetched once per sandbox session and cached:
```
/etc/codepod/registry-index.json
```

`pip install` checks the cache first. `pip update-index` (or `--no-cache`) forces a re-fetch.

## pip list / pip show Integration

The existing `BUILTIN_PACKAGES` array is for the full binary. For the lean binary, this array is empty. Instead:
- `pip list` reads `pip-installed.json` (packages installed from registry) + `extensions.json` (as before)
- `pip show <pkg>` checks installed packages, then queries the registry index for available packages

For the full binary, `BUILTIN_PACKAGES` stays as-is — those packages report as installed without needing the registry.

## CI Pipeline (codepod-packages repo)

### GitHub Actions Workflows

**1. `add-pypi-package.yml`** — triggered manually or by PR
```yaml
# Input: package name + version
# Steps:
#   1. pip download --no-deps --only-binary=:all: <package>==<version>
#   2. Verify wheel is pure Python (no .so/.pyd/.dylib)
#   3. Copy wheel to packages/<name>/
#   4. Run build-index.py to update index.json
#   5. Commit and push
```

**2. `build-wasm-package.yml`** — triggered by release in source repos
```yaml
# Input: source repo (numpy-rust, pandas-rust), version tag
# Steps:
#   1. Checkout source repo
#   2. cargo build --target wasm32-wasip1 --release
#   3. Create wheel from python/ directory
#   4. Copy .wasm + .whl to packages/<name>/
#   5. Run build-index.py to update index.json
#   6. Commit and push to codepod-packages
```

**3. `build-index.py`** — regenerates index.json from directory contents
```python
# Scans packages/*/
# For each directory:
#   - Find .wasm file (optional)
#   - Find .whl file (required)
#   - Read METADATA from wheel for version/summary/deps
#   - Generate index entry
# Writes index.json
```

### Validation

`add-pypi-package.py` validates before adding:
1. Wheel filename matches `*-py3-none-any.whl` or `*-py2.py3-none-any.whl` (pure Python)
2. No `.so`, `.pyd`, `.dylib` files inside the wheel
3. Package imports successfully in RustPython (optional smoke test)

## Initial Package Set

| Package | Type | Source | Size |
|---------|------|--------|------|
| numpy | WASM + wheel | numpy-rust | ~2.4MB |
| matplotlib | WASM + wheel | matplotlib-py | ~1.5MB |
| Pillow | WASM + wheel | pillow-rust | ~800KB |
| pandas | WASM + wheel | pandas-rust | ~1.8MB |
| requests | wheel only | codepod shim (already in sandbox) | ~12KB |
| tabulate | wheel only | PyPI | ~48KB |

Future additions (not in initial release):
- sympy (~40MB), seaborn (~300KB), beautifulsoup4 (~200KB), sklearn (WASM)

## File Map

| File | Action |
|------|--------|
| `packages/shell-exec/src/virtual_commands.rs` | Modify — enhance `pip install` with registry fetch + wheel extraction |
| `packages/shell-exec/Cargo.toml` | Modify — add `zip` crate dependency |
| New: `packages/shell-exec/src/wheel.rs` | Create — wheel (ZIP) extraction logic |
| New: `codepod-packages/` repo | Create — GitHub Pages package registry |
| New: `codepod-packages/scripts/build-index.py` | Create — index generator |
| New: `codepod-packages/scripts/add-pypi-package.py` | Create — PyPI wheel importer |
| New: `codepod-packages/.github/workflows/` | Create — CI pipelines |

## Testing

1. **Unit test**: wheel extraction on a known .whl file (tabulate)
2. **Integration test**: `pip install tabulate` in sandbox → `python -c "import tabulate"` succeeds
3. **Dependency test**: `pip install pandas` installs numpy first, then pandas
4. **Already-installed test**: `pip install tabulate` twice → "Requirement already satisfied"
5. **Not-found test**: `pip install nonexistent` → clear error
6. **pip list**: shows registry-installed packages with versions
