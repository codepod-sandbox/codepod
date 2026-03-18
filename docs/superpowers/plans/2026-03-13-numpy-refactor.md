# NumPy Refactor: Split Monolithic `__init__.py` Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the 11,700-line `packages/numpy-rust/python/numpy/__init__.py` into 18 focused submodules and migrate bitwise/logical operations from Python loops to Rust.

**Architecture:** Extract code bottom-up following the dependency graph: helpers → types → creation → math → reductions → manipulation → bitwise → ufunc → domain modules → stubs. Each extraction creates a new `_foo.py` submodule, updates `__init__.py` to import from it, and verifies all tests pass. Rust migrations for logical_and/or/xor are done alongside the `_bitwise.py` extraction.

**Tech Stack:** Python (RustPython VM), Rust (numpy-rust-core, numpy-rust-python crates), WASM target

**Spec:** `docs/superpowers/specs/2026-03-13-numpy-refactor-design.md`

---

## Environment Setup

Before any task, ensure the build environment is ready:

```bash
# From packages/numpy-rust/
source ../../scripts/dev-init.sh

# Build the RustPython binary (needed for all Python tests)
cargo build -p numpy-rust-wasm

# Verify tests pass before starting
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

The test binary is `target/debug/numpy-python` (NOT `python3`). All test files use a self-running test runner pattern (no pytest).

---

## File Structure

### Files to Create (Python submodules)

| File | Responsibility | Approx Lines |
|------|---------------|-------------|
| `python/numpy/_helpers.py` | Internal utilities, sentinel objects, helper classes | ~250 |
| `python/numpy/_core_types.py` | Scalar types, dtype class, type hierarchy, finfo/iinfo | ~1,400 |
| `python/numpy/_datetime.py` | datetime64, timedelta64 classes and helpers | ~500 |
| `python/numpy/_creation.py` | Array creation functions (array, zeros, ones, arange, etc.) | ~700 |
| `python/numpy/_math.py` | Element-wise math, trig, comparison, type checking | ~1,000 |
| `python/numpy/_reductions.py` | Aggregation, statistics, NaN-aware reductions, set ops | ~700 |
| `python/numpy/_manipulation.py` | Shape manipulation, stacking, splitting, broadcasting | ~1,000 |
| `python/numpy/_bitwise.py` | Bitwise and logical operations (delegating to Rust) | ~200 |
| `python/numpy/_ufunc.py` | ufunc class and wrapping registration | ~350 |
| `python/numpy/_poly.py` | Polynomial utilities, convolve, correlate | ~400 |
| `python/numpy/_linalg_ext.py` | Python-level linalg wrappers | ~200 |
| `python/numpy/_fft_ext.py` | Python-level FFT wrappers | ~300 |
| `python/numpy/_random_ext.py` | Random number generation | ~970 |
| `python/numpy/_string_ops.py` | String/char array operations | ~270 |
| `python/numpy/_indexing.py` | Index generation, iteration, histograms | ~500 |
| `python/numpy/_window.py` | Signal window functions | ~100 |
| `python/numpy/_io.py` | File I/O functions | ~200 |
| `python/numpy/_stubs.py` | Module stubs, misc utilities, format functions | ~400 |

### Files to Modify (Rust — for logical/bitwise migration)

| File | Change |
|------|--------|
| `crates/numpy-rust-core/src/ops/logical.rs` | Add `logical_and()`, `logical_or()`, `logical_xor()` |
| `crates/numpy-rust-python/src/lib.rs` | Expose bitwise_and/or/xor/not, left_shift, right_shift, logical_and/or/xor |

### Files to Modify (Python)

| File | Change |
|------|--------|
| `python/numpy/__init__.py` | Gut from ~11,700 to ~500 lines (thin re-export layer) |

---

## Chunk 1: Foundation Modules

### Task 1: Extract `_helpers.py`

Extract internal utilities that multiple modules depend on. This is the base of the dependency graph — no numpy dependencies, only `_numpy_native`, `math`, `sys`.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_helpers.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (from `__init__.py`):
- Lines 14-27: `AxisError` class
- Lines 29-46: `_ArrayFlags` class
- Lines 49-330: `_ObjectArray` class (long but self-contained, only uses `_numpy_native`)
- Lines 332-363: `_ComplexResultArray` class (wraps complex results)
- Helper functions scattered in the "Missing functions" section:
  - `_copy_into()` (search for `def _copy_into`)
  - `_apply_order()` (search for `def _apply_order`)
  - `_is_temporal_dtype()`, `_temporal_dtype_info()`, `_make_temporal_array()`
  - `_infer_shape()`, `_flatten_nested()`, `_to_float_list()`
  - `_unsupported_numeric_dtype()`
  - `_CLIP_UNSET` sentinel
- Line 4882-4884: `_builtin_min`, `_builtin_max`, `_builtin_range` aliases

**NOT in `_helpers.py`:** `_normalize_dtype()` and `_DTYPE_CHAR_MAP` depend on `_DTypeClassMeta` (from `_core_types.py`), so they go in `_core_types.py` instead to avoid circular imports.

- [ ] **Step 1: Identify all helper functions and classes**

Search `__init__.py` for every function/class listed above. Record exact line numbers. Verify each only depends on `_numpy_native`, `math`, `sys`, or other items in this list.

```bash
cd packages/numpy-rust
grep -n "def _copy_into\|def _apply_order\|def _is_temporal_dtype\|def _temporal_dtype_info\|def _normalize_dtype\|def _infer_shape\|def _flatten_nested\|def _to_float_list\|def _unsupported_numeric_dtype\|_CLIP_UNSET" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_helpers.py` with extracted code**

Create `python/numpy/_helpers.py` with all helper code. The file structure:

```python
"""Internal helpers used by multiple numpy submodules."""
import sys as _sys
import math as _math
import _numpy_native as _native

# Builtin aliases (these names get shadowed by numpy functions later)
_builtin_min = __builtins__["min"] if isinstance(__builtins__, dict) else __import__("builtins").min
_builtin_max = __builtins__["max"] if isinstance(__builtins__, dict) else __import__("builtins").max
_builtin_range = __builtins__["range"] if isinstance(__builtins__, dict) else __import__("builtins").range

__all__ = [
    'AxisError', '_ArrayFlags', '_ObjectArray', '_ComplexResultArray',
    '_copy_into', '_apply_order', '_is_temporal_dtype', '_temporal_dtype_info',
    '_make_temporal_array', '_infer_shape', '_flatten_nested', '_to_float_list',
    '_unsupported_numeric_dtype', '_CLIP_UNSET',
    '_builtin_min', '_builtin_max', '_builtin_range',
]

# Paste each class/function exactly as-is from __init__.py.
# Do NOT include _normalize_dtype or _DTYPE_CHAR_MAP here — they go in _core_types.py.
```

**Important:** `_ObjectArray` references `_native.array(...)` directly — verify it does NOT call `asarray()` or `array()` from the numpy namespace (which would create a circular dependency with `_creation.py`).

- [ ] **Step 3: Update `__init__.py` to import from `_helpers`**

At the top of `__init__.py`, after the existing imports (`_numpy_native`, `ndarray`, `dot`, `_native_concatenate`), add:

```python
from ._helpers import *
```

Then delete the extracted code blocks from `__init__.py`. Keep the line `from ._helpers import *` where the first extracted block used to be.

- [ ] **Step 4: Run tests to verify**

```bash
cd packages/numpy-rust
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

Expected: All tests pass (same count as before). If any fail, check for:
- Missing imports in `_helpers.py` (e.g., a function that calls something not yet extracted)
- Name resolution issues (e.g., `_builtin_range` used before the alias is defined)

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_helpers.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _helpers.py submodule"
```

---

### Task 2: Extract `_core_types.py`

Extract the type system: scalar types, dtype class, type hierarchy, finfo/iinfo, type casting.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_core_types.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (from `__init__.py`):
- Lines 972-1186: Dtype aliases, `_ScalarType`, `_NumpyIntScalar`, `_NumpyFloatScalar`, `_NumpyComplexScalar`, `_ScalarTypeMeta` metaclass, scalar type classes (`float64`, `float32`, `int8`–`int64`, `uint8`–`uint64`, `complex64`, `complex128`, `bool_`, `str_`, `bytes_`, `void`, `object_`)
- Lines 1187-1207: Constants (`True_`, `False_`, `int_`) — only the type-alias constants, NOT `pi`/`e`/`nan` etc.
- Lines 1208-1219: `typecodes`
- Lines 1220-1324: Type hierarchy classes (`generic`, `number`, `integer`, etc.)
- Lines 4302-4344: `StructuredDtype`
- Lines 4345-4359: `_DTypeClassMeta`
- Lines 4360-4592: `dtype` class
- Lines 4593-4678: Per-dtype DType classes (`Float64DType`, `Int8DType`, etc.)
- Lines 8261-8336: `finfo` class, `_MachAr`
- Lines 8337-8395: `iinfo` class
- Type casting functions: `can_cast`, `result_type`, `promote_types`, `find_common_type`, `common_type`, `mintypecode`
- `_DTYPE_CHAR_MAP` (line ~432) and `_normalize_dtype()` (line ~510) — these depend on `_DTypeClassMeta`, so they live here, not in `_helpers.py`
- Lines 11634-11653: `sctypes`, `sctypeDict`

- [ ] **Step 1: Identify all type-related code**

Search for each class/function. Record line numbers.

```bash
cd packages/numpy-rust
grep -n "class _ScalarType\|class _NumpyIntScalar\|class _NumpyFloatScalar\|class _NumpyComplexScalar\|class _ScalarTypeMeta\|class _StructuredDtype\|class _DTypeClassMeta\|class dtype\|class finfo\|class iinfo\|class _MachAr\|def can_cast\|def result_type\|def promote_types\|def find_common_type\|def common_type\|def mintypecode\|sctypes\|sctypeDict" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_core_types.py` with extracted code**

```python
"""Type system: scalar types, dtype class, type hierarchy, finfo/iinfo, type casting."""
import sys as _sys
import math as _math
import _numpy_native as _native
from ._helpers import _unsupported_numeric_dtype

__all__ = [
    # Scalar types
    'float64', 'float32', 'float16', 'int8', 'int16', 'int32', 'int64',
    'uint8', 'uint16', 'uint32', 'uint64', 'complex64', 'complex128',
    'bool_', 'str_', 'bytes_', 'void', 'object_',
    # Type hierarchy
    'generic', 'number', 'integer', 'signedinteger', 'unsignedinteger',
    'inexact', 'floating', 'complexfloating', 'character', 'flexible',
    # Metaclass and base
    '_ScalarType', '_NumpyIntScalar', '_NumpyFloatScalar', '_NumpyComplexScalar',
    '_ScalarTypeMeta',
    # dtype
    'dtype', 'StructuredDtype', '_DTypeClassMeta',
    # Per-dtype DType classes
    'Float64DType', 'Float32DType', 'Float16DType',
    'Int8DType', 'Int16DType', 'Int32DType', 'Int64DType',
    'UInt8DType', 'UInt16DType', 'UInt32DType', 'UInt64DType',
    'Complex64DType', 'Complex128DType', 'BoolDType', 'StrDType',
    # Info
    'finfo', 'iinfo', '_MachAr',
    # Type casting + dtype normalization
    'can_cast', 'result_type', 'promote_types', 'find_common_type',
    'common_type', 'mintypecode', '_normalize_dtype', '_DTYPE_CHAR_MAP',
    # Constants
    'True_', 'False_', 'int_', 'typecodes', 'sctypes', 'sctypeDict',
]

# Paste all type-related code from __init__.py.
# _DTYPE_CHAR_MAP and _normalize_dtype live HERE (not _helpers) because
# _normalize_dtype uses isinstance(dt, _DTypeClassMeta).
```

**Important:** `_normalize_dtype()` and `_DTYPE_CHAR_MAP` are in `_core_types.py` (not `_helpers.py`) because `_normalize_dtype` references `_DTypeClassMeta`. Other modules that need `_normalize_dtype` import it from `_core_types`.

- [ ] **Step 3: Update `__init__.py`**

Add `from ._core_types import *` after `from ._helpers import *`. Delete extracted code blocks.

- [ ] **Step 4: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_core_types.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _core_types.py submodule"
```

---

### Task 3: Extract `_datetime.py`

Extract datetime64 and timedelta64 support.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_datetime.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:**
- Lines 1325-1490: Datetime/timedelta helper functions (`_is_nat_value`, `_parse_datetime_string`, `_date_to_days`, `_days_to_date`, `_to_common_unit`, `_common_time_unit`, etc.)
- Lines 1491-1822: `datetime64` class, `timedelta64` class
- `isnat()`, `busday_count()`, `is_busday()`, `busday_offset()` (search for their locations)

- [ ] **Step 1: Identify all datetime-related code and record line numbers**

```bash
cd packages/numpy-rust
grep -n "def _is_nat_value\|def _parse_datetime_string\|def _date_to_days\|def _days_to_date\|def _to_common_unit\|def _common_time_unit\|def _is_dt64\|def _is_td64\|def _infer_datetime_unit\|class datetime64\|class timedelta64\|def isnat\|def busday_count\|def is_busday\|def busday_offset" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_datetime.py`**

```python
"""Datetime64 and timedelta64 support."""
import math as _math
import _numpy_native as _native
from ._helpers import _is_temporal_dtype, _temporal_dtype_info
from ._core_types import _normalize_dtype

__all__ = [
    'datetime64', 'timedelta64', 'isnat', 'busday_count', 'is_busday', 'busday_offset',
    # Internal helpers needed by other modules
    '_is_nat_value', '_parse_datetime_string', '_date_to_days', '_days_to_date',
    '_to_common_unit', '_common_time_unit', '_is_dt64', '_is_td64',
    '_infer_datetime_unit',
]
```

- [ ] **Step 3: Update `__init__.py`** — add `from ._datetime import *`, delete extracted blocks

- [ ] **Step 4: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_datetime.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _datetime.py submodule"
```

---

## Chunk 2: Core Operation Modules

### Task 4: Extract `_creation.py`

Extract array creation functions. This is a critical module — almost every other module depends on `asarray()` and `array()`.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_creation.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:**
- `_array_core()`, `_make_complex_array()`, `_detect_builtin_str_bytes()`, `_make_str_bytes_result()`, `_like_order()`
- `array()` (the core Python-level wrapper)
- `concatenate()` (the Python wrapper around `_native_concatenate`)
- `empty()`, `full()`, `full_like()`, `zeros()`, `zeros_like()`, `ones()`, `ones_like()`, `empty_like()`
- `eye()`, `identity()`, `arange()`, `linspace()`, `logspace()`, `geomspace()`
- `asarray()`, `ascontiguousarray()`, `asfortranarray()`, `asarray_chkfinite()`, `copy()`, `require()`
- `frombuffer()`, `fromfunction()`, `fromfile()`, `fromiter()`, `fromstring()`
- `array_equal()`, `array_equiv()`

These are roughly lines 332-970 plus some functions from the "missing stubs" section.

- [ ] **Step 1: Identify all creation functions**

```bash
grep -n "def array\|def asarray\|def zeros\|def ones\|def empty\|def full\|def eye\|def identity\|def arange\|def linspace\|def logspace\|def geomspace\|def copy\b\|def require\|def frombuffer\|def fromfunction\|def fromiter\|def fromstring\|def array_equal\|def array_equiv\|def concatenate\|def _array_core\|def _make_complex\|def _detect_builtin\|def _make_str_bytes\|def _like_order\|def ascontiguousarray\|def asfortranarray\|def asarray_chkfinite\|def fromfile" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_creation.py`**

```python
"""Array creation functions."""
import sys as _sys
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray, dot
from _numpy_native import concatenate as _native_concatenate
from ._helpers import (
    AxisError, _ObjectArray, _ComplexResultArray,
    _copy_into, _apply_order, _infer_shape,
    _flatten_nested, _to_float_list, _is_temporal_dtype,
    _temporal_dtype_info, _CLIP_UNSET, _builtin_range,
)
from ._core_types import dtype, _ScalarType, _normalize_dtype

__all__ = [
    'array', 'asarray', 'ascontiguousarray', 'asfortranarray',
    'asarray_chkfinite', 'zeros', 'zeros_like', 'ones', 'ones_like',
    'empty', 'empty_like', 'full', 'full_like', 'eye', 'identity',
    'arange', 'linspace', 'logspace', 'geomspace', 'copy', 'require',
    'frombuffer', 'fromfunction', 'fromfile', 'fromiter', 'fromstring',
    'array_equal', 'array_equiv', 'concatenate',
    # Internal helpers needed by other modules
    '_array_core', '_make_complex_array',
]
```

**Critical:** `concatenate()` is the Python-level wrapper. It must be in `_creation.py` because `_manipulation.py` (`stack`, `hstack`, `vstack`) calls it.

**Critical:** `ndarray` itself is imported from `_numpy_native` — it is NOT defined in Python. Re-export it from `_creation.py` or from `__init__.py` directly.

- [ ] **Step 3: Update `__init__.py`** — add `from ._creation import *`, delete extracted blocks

- [ ] **Step 4: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_creation.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _creation.py submodule"
```

---

### Task 5: Extract `_math.py`

Extract element-wise math, type checking, comparison, and arithmetic operators.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_math.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:** All functions listed in the spec's `_math.py` section. This is the largest extraction by function count. Key sections from `__init__.py`:
- Trig functions (lines ~1823+ in the "Missing functions" section)
- Exponential/log functions
- Rounding functions
- Power functions
- Arithmetic functions (`add`, `subtract`, `multiply`, `divide`, etc.)
- Extrema (`maximum`, `minimum`, `fmax`, `fmin`)
- Type checking (`isnan`, `isinf`, `isfinite`, `isscalar`, etc.)
- Comparison (`allclose`, `isclose`, `greater`, `less`, `equal`, etc.)
- Complex (`real`, `imag`, `conj`, `angle`, `unwrap`, `real_if_close`)
- Special (`gamma`, `lgamma`, `erf`, `erfc`, etc.)
- GCD/LCM: `gcd`, `lcm`
- `clip`, `nan_to_num`, `sign`, `signbit`, `copysign`, `heaviside`, `ldexp`, `frexp`, `hypot`, `sinc`
- `deg2rad`, `rad2deg`, `degrees`, `radians`
- `fabs`, `modf`, `fmod`
- `astype` (the free function version)
- `issubdtype`, `issubclass_`, `isrealobj`, `iscomplexobj`, `isreal`, `iscomplex`

- [ ] **Step 1: Identify all math functions**

This is a large set. Search systematically by category:

```bash
# Trig
grep -n "^def sin\b\|^def cos\b\|^def tan\b\|^def arcsin\|^def arccos\|^def arctan\b\|^def arctan2\|^def sinh\|^def cosh\|^def tanh\|^def arcsinh\|^def arccosh\|^def arctanh" python/numpy/__init__.py

# Math functions
grep -n "^def exp\b\|^def exp2\|^def log\b\|^def log2\|^def log10\|^def log1p\|^def expm1\|^def logaddexp\|^def sqrt\|^def cbrt\|^def square\|^def reciprocal\|^def power\b\|^def float_power" python/numpy/__init__.py

# Comparison & type checking
grep -n "^def isnan\|^def isinf\|^def isfinite\|^def isscalar\|^def greater\b\|^def less\b\|^def equal\b\|^def not_equal\|^def greater_equal\|^def less_equal\|^def allclose\|^def isclose" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_math.py`**

```python
"""Element-wise math, type checking, comparison, arithmetic operators."""
import sys as _sys
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import (
    _ObjectArray, _copy_into, _CLIP_UNSET,
    _builtin_range, _builtin_min, _builtin_max,
)
from ._core_types import (
    _ScalarType, _NumpyIntScalar, _NumpyFloatScalar, _NumpyComplexScalar,
    dtype, finfo, _normalize_dtype,
)
from ._creation import array, asarray, zeros, ones, empty, concatenate, linspace

__all__ = [
    # Trig
    'sin', 'cos', 'tan', 'arcsin', 'arccos', 'arctan', 'arctan2',
    'sinh', 'cosh', 'tanh', 'arcsinh', 'arccosh', 'arctanh',
    # Exp/log
    'exp', 'exp2', 'log', 'log2', 'log10', 'log1p', 'expm1',
    'logaddexp', 'logaddexp2',
    # Rounding
    'floor', 'ceil', 'trunc', 'rint', 'around', 'fix', 'round_',
    # Power
    'sqrt', 'cbrt', 'square', 'reciprocal', 'power', 'float_power',
    # Arithmetic
    'add', 'subtract', 'multiply', 'divide', 'true_divide',
    'floor_divide', 'remainder', 'mod', 'divmod', 'negative', 'positive',
    'fmod', 'modf', 'fabs',
    # Extrema
    'maximum', 'minimum', 'fmax', 'fmin',
    # Other
    'abs', 'absolute', 'sign', 'signbit', 'copysign', 'heaviside',
    'ldexp', 'frexp', 'hypot', 'sinc', 'nan_to_num', 'clip',
    'nextafter', 'spacing',
    # Angle
    'deg2rad', 'rad2deg', 'degrees', 'radians',
    # Special
    'gamma', 'lgamma', 'erf', 'erfc', 'j0', 'j1', 'y0', 'y1', 'i0',
    # Complex
    'real', 'imag', 'conj', 'conjugate', 'angle', 'unwrap', 'real_if_close',
    # Type checking
    'isnan', 'isinf', 'isfinite', 'isneginf', 'isposinf',
    'isscalar', 'isreal', 'iscomplex', 'isrealobj', 'iscomplexobj',
    'issubdtype', 'issubclass_',
    # Comparison
    'allclose', 'isclose', 'greater', 'less', 'equal', 'not_equal',
    'greater_equal', 'less_equal',
    # GCD/LCM
    'gcd', 'lcm',
    # Casting
    'astype',
]
```

**Note:** These are the *plain function forms*. They will be wrapped as ufunc objects later by `_ufunc.py`. The `_ufunc.py` module imports from `_math` and captures function references before rebinding names.

- [ ] **Step 3: Update `__init__.py`** — add `from ._math import *`, delete extracted blocks

- [ ] **Step 4: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_math.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _math.py submodule"
```

---

### Task 6: Extract `_reductions.py`

Extract aggregation, statistics, NaN-aware reductions, and set operations.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_reductions.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:**
- Basic: `sum`, `prod`, `cumsum`, `cumprod`, `diff`, `ediff1d`, `gradient`, `trapz`, `trapezoid`, `cumulative_trapezoid`
- Statistics: `mean`, `std`, `var`, `median`, `average`, `cov`, `corrcoef`
- Extrema: `max`, `min`, `argmax`, `argmin`, `ptp`
- NaN-aware: all `nan*` variants
- Quantile: `quantile`, `percentile`
- Boolean: `all`, `any`, `count_nonzero`
- Search: `nonzero`, `flatnonzero`, `argwhere`, `searchsorted`, `where`
- Set: `intersect1d`, `union1d`, `setdiff1d`, `setxor1d`, `in1d`, `isin`
- Memory: `may_share_memory`, `shares_memory`

- [ ] **Step 1: Identify all reduction functions**

```bash
grep -n "^def sum\b\|^def prod\b\|^def cumsum\|^def cumprod\|^def mean\b\|^def std\b\|^def var\b\|^def median\|^def average\|^def nansum\|^def nanmean\|^def all\b\|^def any\b\|^def nonzero\|^def where\b\|^def argmax\|^def argmin\|^def max\b\|^def min\b\|^def count_nonzero\|^def intersect1d\|^def union1d\|^def diff\b\|^def gradient\|^def cov\b\|^def corrcoef\|^def quantile\|^def percentile\|^def trapz\|^def trapezoid" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_reductions.py`**

```python
"""Aggregation, statistics, NaN-aware reductions."""
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import (
    AxisError, _ObjectArray, _copy_into, _normalize_dtype,
    _builtin_range, _builtin_min, _builtin_max,
)
from ._core_types import dtype, _ScalarType
from ._creation import array, asarray, zeros, ones, empty, arange, concatenate, linspace
from ._math import isnan, isinf, isfinite, absolute as _np_abs, sqrt, add, multiply

__all__ = [
    'sum', 'prod', 'cumsum', 'cumprod', 'diff', 'ediff1d', 'gradient',
    'trapz', 'trapezoid', 'cumulative_trapezoid',
    'mean', 'std', 'var', 'median', 'average', 'cov', 'corrcoef',
    'max', 'min', 'argmax', 'argmin', 'ptp',
    'nansum', 'nanmean', 'nanstd', 'nanvar', 'nanmin', 'nanmax',
    'nanargmin', 'nanargmax', 'nanprod', 'nancumsum', 'nancumprod',
    'nanmedian', 'nanpercentile', 'nanquantile',
    'quantile', 'percentile',
    'all', 'any', 'count_nonzero',
    'nonzero', 'flatnonzero', 'argwhere', 'searchsorted',
    'intersect1d', 'union1d', 'setdiff1d', 'setxor1d', 'in1d', 'isin',
    'may_share_memory', 'shares_memory',
]
```

**Note on `abs`:** Import numpy's `absolute` as `_np_abs` to avoid shadowing Python's builtin `abs`. Use `_np_abs(...)` inside reduction functions.

**Note on `where`:** `where` is owned by `_creation.py` (it's a creation function — `np.where(cond, x, y)` creates a new array). Do NOT duplicate it here. If reductions need `where`, import it from `_creation`.

- [ ] **Step 3: Update `__init__.py`** — add `from ._reductions import *`, delete extracted blocks

- [ ] **Step 4: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_reductions.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _reductions.py submodule"
```

---

### Task 7: Extract `_manipulation.py`

Extract shape manipulation, stacking, splitting, reordering, selection, broadcasting.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_manipulation.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:**
- Shape: `reshape`, `ravel`, `flatten`, `expand_dims`, `squeeze`, `transpose`, `moveaxis`, `swapaxes`, `resize`
- Atleast: `atleast_1d`, `atleast_2d`, `atleast_3d`
- Stacking: `stack`, `hstack`, `vstack`, `dstack`, `column_stack`, `row_stack`
- Splitting: `split`, `array_split`, `hsplit`, `vsplit`, `dsplit`
- Repetition: `repeat`, `tile`, `append`, `insert`, `delete`
- Reordering: `sort`, `argsort`, `lexsort`, `partition`, `argpartition`, `unique`
- Flipping: `flip`, `flipud`, `fliplr`, `rot90`, `roll`, `rollaxis`
- Selection: `extract`, `select`, `choose`, `take`, `compress`, `put`, `putmask`, `place`, `piecewise`, `copyto`
- Broadcasting: `broadcast` class, `_BroadcastIter`, `broadcast_shapes`, `broadcast_to`, `broadcast_arrays`
- Utility: `trim_zeros`, `apply_along_axis`, `apply_over_axes`, `vectorize` class
- Size: `size` (free function)

- [ ] **Step 1: Identify all manipulation functions**

```bash
cd packages/numpy-rust
grep -n "^def reshape\|^def ravel\|^def flatten\|^def expand_dims\|^def squeeze\|^def transpose\|^def moveaxis\|^def swapaxes\|^def resize\|^def atleast_\|^def stack\b\|^def hstack\|^def vstack\|^def dstack\|^def column_stack\|^def row_stack\|^def split\b\|^def array_split\|^def hsplit\|^def vsplit\|^def dsplit\|^def repeat\b\|^def tile\|^def append\b\|^def insert\b\|^def delete\b\|^def sort\b\|^def argsort\|^def lexsort\|^def partition\|^def argpartition\|^def unique\|^def flip\b\|^def flipud\|^def fliplr\|^def rot90\|^def roll\b\|^def rollaxis\|^def extract\|^def select\|^def choose\|^def take\b\|^def compress\|^def put\b\|^def putmask\|^def place\|^def piecewise\|^def copyto\|^class broadcast\|^def broadcast_shapes\|^def broadcast_to\|^def broadcast_arrays\|^def trim_zeros\|^def apply_along_axis\|^def apply_over_axes\|^class vectorize\|^def size\b\|^def block\b" python/numpy/__init__.py
```

- [ ] **Step 2: Create `_manipulation.py`**

```python
"""Shape manipulation, stacking, splitting, reordering, selection, broadcasting."""
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import (
    AxisError, _ObjectArray, _copy_into, _normalize_dtype,
    _builtin_range, _builtin_min, _builtin_max,
)
from ._core_types import dtype, _ScalarType
from ._creation import array, asarray, zeros, ones, empty, arange, concatenate

__all__ = [
    'reshape', 'ravel', 'flatten', 'expand_dims', 'squeeze', 'transpose',
    'moveaxis', 'swapaxes', 'resize',
    'atleast_1d', 'atleast_2d', 'atleast_3d',
    'stack', 'hstack', 'vstack', 'dstack', 'column_stack', 'row_stack',
    'split', 'array_split', 'hsplit', 'vsplit', 'dsplit',
    'repeat', 'tile', 'append', 'insert', 'delete',
    'sort', 'argsort', 'lexsort', 'partition', 'argpartition', 'unique',
    'flip', 'flipud', 'fliplr', 'rot90', 'roll', 'rollaxis',
    'extract', 'select', 'choose', 'take', 'compress', 'put', 'putmask',
    'place', 'piecewise', 'copyto',
    'broadcast', 'broadcast_shapes', 'broadcast_to', 'broadcast_arrays',
    'trim_zeros', 'apply_along_axis', 'apply_over_axes', 'vectorize',
    'size', 'block',
]
```

- [ ] **Step 3: Update `__init__.py`** — add `from ._manipulation import *`, delete extracted blocks

- [ ] **Step 4: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/_manipulation.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _manipulation.py submodule"
```

---

## Chunk 3: Bitwise/Logical Rust Migration + Remaining Extractions

### Task 8: Add Rust bindings for bitwise and logical operations

The bitwise operations (`bitwise_and/or/xor`, `left_shift`, `right_shift`, `bitwise_not`) are already implemented in Rust core (`crates/numpy-rust-core/src/ops/logical.rs`) but NOT exposed to Python bindings. The logical operations (`logical_and/or/xor`) need new Rust implementations. This task adds the Rust bindings.

**Note:** The spec mentions creating `bitwise.rs` — we are NOT doing that. All bitwise and logical ops already live in `logical.rs` and we're adding to it there. No new Rust file is needed.

**Files:**
- Modify: `crates/numpy-rust-core/src/ops/logical.rs` — add `logical_and()`, `logical_or()`, `logical_xor()`
- Modify: `crates/numpy-rust-python/src/lib.rs` — expose all bitwise + logical ops

- [ ] **Step 1: Add logical_and/or/xor to Rust core**

Add to `crates/numpy-rust-core/src/ops/logical.rs`, after the existing `logical_not()` and `bitwise_not()` methods:

```rust
/// Prepare two NdArrays for logical ops: broadcast shapes. All dtypes allowed.
fn prepare_logical(lhs: &NdArray, rhs: &NdArray) -> Result<(ArrayData, ArrayData)> {
    let out_shape = broadcast_shape(lhs.shape(), rhs.shape())?;
    let a = broadcast_array_data(&lhs.data, &out_shape);
    let b = broadcast_array_data(&rhs.data, &out_shape);
    Ok((a, b))
}

impl NdArray {
    /// Element-wise logical AND. Returns Bool array.
    pub fn logical_and(&self, other: &NdArray) -> Result<NdArray> {
        let (a, b) = prepare_logical(self, other)?;
        let to_bool = |data: &ArrayData| -> ndarray::ArrayD<bool> {
            match data {
                ArrayData::Bool(a) => a.mapv(|x| x),
                ArrayData::Int32(a) => a.mapv(|x| x != 0),
                ArrayData::Int64(a) => a.mapv(|x| x != 0),
                ArrayData::Float32(a) => a.mapv(|x| x != 0.0),
                ArrayData::Float64(a) => a.mapv(|x| x != 0.0),
                ArrayData::Complex64(a) => a.mapv(|x| x.re != 0.0 || x.im != 0.0),
                ArrayData::Complex128(a) => a.mapv(|x| x.re != 0.0 || x.im != 0.0),
                ArrayData::Str(a) => a.mapv(|ref x| !x.is_empty()),
            }
        };
        let ba = to_bool(&a);
        let bb = to_bool(&b);
        let result = ndarray::Zip::from(&ba).and(&bb).map_collect(|&x, &y| x && y).into_shared();
        Ok(NdArray::from_data(ArrayData::Bool(result)))
    }

    /// Element-wise logical OR. Returns Bool array.
    pub fn logical_or(&self, other: &NdArray) -> Result<NdArray> {
        let (a, b) = prepare_logical(self, other)?;
        let to_bool = |data: &ArrayData| -> ndarray::ArrayD<bool> {
            match data {
                ArrayData::Bool(a) => a.mapv(|x| x),
                ArrayData::Int32(a) => a.mapv(|x| x != 0),
                ArrayData::Int64(a) => a.mapv(|x| x != 0),
                ArrayData::Float32(a) => a.mapv(|x| x != 0.0),
                ArrayData::Float64(a) => a.mapv(|x| x != 0.0),
                ArrayData::Complex64(a) => a.mapv(|x| x.re != 0.0 || x.im != 0.0),
                ArrayData::Complex128(a) => a.mapv(|x| x.re != 0.0 || x.im != 0.0),
                ArrayData::Str(a) => a.mapv(|ref x| !x.is_empty()),
            }
        };
        let ba = to_bool(&a);
        let bb = to_bool(&b);
        let result = ndarray::Zip::from(&ba).and(&bb).map_collect(|&x, &y| x || y).into_shared();
        Ok(NdArray::from_data(ArrayData::Bool(result)))
    }

    /// Element-wise logical XOR. Returns Bool array.
    pub fn logical_xor(&self, other: &NdArray) -> Result<NdArray> {
        let (a, b) = prepare_logical(self, other)?;
        let to_bool = |data: &ArrayData| -> ndarray::ArrayD<bool> {
            match data {
                ArrayData::Bool(a) => a.mapv(|x| x),
                ArrayData::Int32(a) => a.mapv(|x| x != 0),
                ArrayData::Int64(a) => a.mapv(|x| x != 0),
                ArrayData::Float32(a) => a.mapv(|x| x != 0.0),
                ArrayData::Float64(a) => a.mapv(|x| x != 0.0),
                ArrayData::Complex64(a) => a.mapv(|x| x.re != 0.0 || x.im != 0.0),
                ArrayData::Complex128(a) => a.mapv(|x| x.re != 0.0 || x.im != 0.0),
                ArrayData::Str(a) => a.mapv(|ref x| !x.is_empty()),
            }
        };
        let ba = to_bool(&a);
        let bb = to_bool(&b);
        let result = ndarray::Zip::from(&ba).and(&bb).map_collect(|&x, &y| x ^ y).into_shared();
        Ok(NdArray::from_data(ArrayData::Bool(result)))
    }
}
```

- [ ] **Step 2: Add Rust unit tests for logical ops**

Add to the `#[cfg(test)]` module in the same file:

```rust
#[test]
fn test_logical_and() {
    let a = NdArray::from_vec(vec![true, true, false, false]);
    let b = NdArray::from_vec(vec![true, false, true, false]);
    let c = a.logical_and(&b).unwrap();
    assert_eq!(c.dtype(), DType::Bool);
    assert_eq!(c.shape(), &[4]);
    // Verify actual values: T&T=T, T&F=F, F&T=F, F&F=F
    if let ArrayData::Bool(arr) = &c.data {
        assert_eq!(arr.as_slice().unwrap(), &[true, false, false, false]);
    } else { panic!("expected Bool"); }
}

#[test]
fn test_logical_or() {
    let a = NdArray::from_vec(vec![true, true, false, false]);
    let b = NdArray::from_vec(vec![true, false, true, false]);
    let c = a.logical_or(&b).unwrap();
    assert_eq!(c.dtype(), DType::Bool);
    if let ArrayData::Bool(arr) = &c.data {
        assert_eq!(arr.as_slice().unwrap(), &[true, true, true, false]);
    } else { panic!("expected Bool"); }
}

#[test]
fn test_logical_xor() {
    let a = NdArray::from_vec(vec![true, true, false, false]);
    let b = NdArray::from_vec(vec![true, false, true, false]);
    let c = a.logical_xor(&b).unwrap();
    assert_eq!(c.dtype(), DType::Bool);
    if let ArrayData::Bool(arr) = &c.data {
        assert_eq!(arr.as_slice().unwrap(), &[false, true, true, false]);
    } else { panic!("expected Bool"); }
}

#[test]
fn test_logical_and_numeric() {
    let a = NdArray::from_vec(vec![1.0_f64, 0.0, 3.0, 0.0]);
    let b = NdArray::from_vec(vec![1.0_f64, 1.0, 0.0, 0.0]);
    let c = a.logical_and(&b).unwrap();
    assert_eq!(c.dtype(), DType::Bool);
    if let ArrayData::Bool(arr) = &c.data {
        assert_eq!(arr.as_slice().unwrap(), &[true, false, false, false]);
    } else { panic!("expected Bool"); }
}
```

- [ ] **Step 3: Run Rust tests**

```bash
cd packages/numpy-rust
cargo test -p numpy-rust-core -- logical
```

Expected: All new and existing logical tests pass.

- [ ] **Step 4: Add Python bindings in `lib.rs`**

Add to `crates/numpy-rust-python/src/lib.rs`, in the `#[pymodule]` block alongside the existing `logical_not` function:

```rust
#[pyfunction]
fn bitwise_and(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.bitwise_and(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn bitwise_or(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.bitwise_or(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn bitwise_xor(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.bitwise_xor(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn bitwise_not(a: vm::PyRef<PyNdArray>, vm: &VirtualMachine) -> PyResult {
    a.inner()
        .bitwise_not()
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn left_shift(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.left_shift(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn right_shift(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.right_shift(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn logical_and(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.logical_and(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn logical_or(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.logical_or(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}

#[pyfunction]
fn logical_xor(x1: PyObjectRef, x2: PyObjectRef, vm: &VirtualMachine) -> PyResult {
    let a = obj_to_ndarray(&x1, vm)?;
    let b = obj_to_ndarray(&x2, vm)?;
    a.logical_xor(&b)
        .map(|r| PyNdArray::from_core(r).into_pyobject(vm))
        .map_err(|e| vm.new_type_error(e.to_string()))
}
```

Also register these functions in the `#[pymodule]` block. Search for the line `fn logical_not(` in `lib.rs` — the new functions go right next to it. The `#[pyfunction]` attribute handles registration automatically via the `#[pymodule]` macro; no separate registration array is needed.

**Refactoring note:** The `to_bool` closure is duplicated in `logical_and/or/xor`. Factor it out as a standalone `fn to_bool_array(data: &ArrayData) -> ndarray::ArrayD<bool>` at the top of the file, near `prepare_bitwise`.

- [ ] **Step 5: Build and test**

```bash
cd packages/numpy-rust
cargo build -p numpy-rust-wasm
cargo test -p numpy-rust-core -- logical
cargo test -p numpy-rust-core -- bitwise
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 6: Commit**

```bash
git add crates/numpy-rust-core/src/ops/logical.rs crates/numpy-rust-python/src/lib.rs
git commit -m "feat(numpy): expose bitwise and logical ops to Python bindings"
```

---

### Task 9: Extract `_bitwise.py`

Extract bitwise and logical operations, now delegating to Rust native functions.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_bitwise.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (lines 6055-6167 + logical_and/or/xor/not):
- `logical_and`, `logical_or`, `logical_xor`, `logical_not`
- `bitwise_and`, `bitwise_or`, `bitwise_xor`, `bitwise_not`, `invert`
- `left_shift`, `right_shift`
- `packbits`, `unpackbits`

- [ ] **Step 1: Create `_bitwise.py` with Rust-delegating implementations**

Replace the Python loop implementations with native calls:

```python
"""Bitwise and logical operations."""
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import _ObjectArray, _copy_into
from ._creation import array, asarray

__all__ = [
    'bitwise_and', 'bitwise_or', 'bitwise_xor', 'bitwise_not', 'invert',
    'left_shift', 'right_shift',
    'logical_and', 'logical_or', 'logical_xor', 'logical_not',
    'packbits', 'unpackbits',
]


def bitwise_and(x1, x2):
    """Element-wise bitwise AND of integer arrays."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    return _native.bitwise_and(x1, x2)


def bitwise_or(x1, x2):
    """Element-wise bitwise OR of integer arrays."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    return _native.bitwise_or(x1, x2)


def bitwise_xor(x1, x2):
    """Element-wise bitwise XOR of integer arrays."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    return _native.bitwise_xor(x1, x2)


def bitwise_not(x):
    """Element-wise bitwise NOT (invert) of integer array."""
    x = asarray(x)
    return _native.bitwise_not(x)


invert = bitwise_not


def left_shift(x1, x2):
    """Element-wise left bit shift."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    return _native.left_shift(x1, x2)


def right_shift(x1, x2):
    """Element-wise right bit shift."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    return _native.right_shift(x1, x2)


def logical_and(x1, x2, out=None):
    """Element-wise logical AND."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    r = _native.logical_and(x1, x2)
    if out is not None:
        _copy_into(out, r)
        return out
    return r


def logical_or(x1, x2, out=None):
    """Element-wise logical OR."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    r = _native.logical_or(x1, x2)
    if out is not None:
        _copy_into(out, r)
        return out
    return r


def logical_xor(x1, x2, out=None):
    """Element-wise logical XOR."""
    x1 = asarray(x1)
    x2 = asarray(x2)
    r = _native.logical_xor(x1, x2)
    if out is not None:
        _copy_into(out, r)
        return out
    return r


def logical_not(x, out=None):
    """Element-wise logical NOT."""
    if isinstance(x, ndarray):
        r = _native.logical_not(x)
    else:
        return not x
    if out is not None:
        _copy_into(out, r)
        return out
    return r


```

Copy `packbits` (lines 10362-10390) and `unpackbits` (lines 10392-10420) verbatim from `__init__.py` into `_bitwise.py`. These stay as Python — no Rust equivalent needed.

- [ ] **Step 2: Update `__init__.py`** — add `from ._bitwise import *`, delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_bitwise.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _bitwise.py with Rust-native delegation"
```

---

### Task 10: Extract `_ufunc.py`

Extract the ufunc class and wrapping registration. This is the module that converts plain functions (from `_math`, `_bitwise`, etc.) into proper ufunc objects.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_ufunc.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:**
- Lines 10550-10735: `_UFUNC_SENTINEL`, `ufunc` class
- Lines 10737-10897: The wrapping block that saves function references and creates ufunc instances
- `frompyfunc()` (line ~8835)

- [ ] **Step 1: Create `_ufunc.py`**

This module imports plain function versions from other submodules, then re-exports ufunc-wrapped versions:

```python
"""ufunc class and wrapping registration."""
from ._helpers import _copy_into
from ._creation import array, asarray
from ._manipulation import stack, squeeze, take, expand_dims

# Import the plain function forms (before wrapping)
from ._math import (
    add as _add_func, subtract as _subtract_func,
    multiply as _multiply_func, divide as _divide_func,
    true_divide as _true_divide_func, floor_divide as _floor_divide_func,
    power as _power_func, remainder as _remainder_func,
    fmod as _fmod_func, maximum as _maximum_func, minimum as _minimum_func,
    fmax as _fmax_func, fmin as _fmin_func,
    arctan2 as _arctan2_func, hypot as _hypot_func,
    copysign as _copysign_func, ldexp as _ldexp_func,
    heaviside as _heaviside_func, nextafter as _nextafter_func,
    sin as _sin_func, cos as _cos_func, tan as _tan_func,
    arcsin as _arcsin_func, arccos as _arccos_func, arctan as _arctan_func,
    sinh as _sinh_func, cosh as _cosh_func, tanh as _tanh_func,
    exp as _exp_func, exp2 as _exp2_func,
    log as _log_func, log2 as _log2_func, log10 as _log10_func,
    sqrt as _sqrt_func, cbrt as _cbrt_func, square as _square_func,
    reciprocal as _reciprocal_func,
    negative as _negative_func, positive as _positive_func,
    absolute as _absolute_func, sign as _sign_func,
    floor as _floor_func, ceil as _ceil_func, rint as _rint_func,
    trunc as _trunc_func,
    deg2rad as _deg2rad_func, rad2deg as _rad2deg_func,
    signbit as _signbit_func,
    isnan as _isnan_func, isinf as _isinf_func, isfinite as _isfinite_func,
    greater as _greater_func, less as _less_func,
    equal as _equal_func, not_equal as _not_equal_func,
    greater_equal as _greater_equal_func, less_equal as _less_equal_func,
)
from ._bitwise import (
    logical_and as _logical_and_func, logical_or as _logical_or_func,
    logical_xor as _logical_xor_func, logical_not as _logical_not_func,
    bitwise_and as _bitwise_and_func, bitwise_or as _bitwise_or_func,
    bitwise_xor as _bitwise_xor_func,
    left_shift as _left_shift_func, right_shift as _right_shift_func,
    bitwise_not as _bitwise_not_func,
)
from ._reductions import (
    sum, prod, cumsum, cumprod, max, min, all, any,
)

__all__ = [
    'ufunc', 'frompyfunc',
    # Re-exported as ufunc instances (overwrite plain function names):
    'add', 'subtract', 'multiply', 'divide', 'true_divide', 'floor_divide',
    'power', 'remainder', 'mod', 'fmod', 'fmax', 'fmin',
    'maximum', 'minimum',
    'logical_and', 'logical_or', 'logical_xor', 'logical_not',
    'bitwise_and', 'bitwise_or', 'bitwise_xor',
    'left_shift', 'right_shift', 'bitwise_not', 'invert',
    'greater', 'less', 'equal', 'not_equal', 'greater_equal', 'less_equal',
    'arctan2', 'hypot', 'copysign', 'ldexp', 'heaviside', 'nextafter',
    'sin', 'cos', 'tan', 'arcsin', 'arccos', 'arctan',
    'sinh', 'cosh', 'tanh',
    'exp', 'exp2', 'log', 'log2', 'log10',
    'sqrt', 'cbrt', 'square', 'reciprocal',
    'negative', 'positive', 'absolute', 'abs', 'sign',
    'floor', 'ceil', 'rint', 'trunc',
    'deg2rad', 'rad2deg', 'signbit',
    'isnan', 'isinf', 'isfinite',
]

```

Then copy these code blocks verbatim from `__init__.py` into `_ufunc.py`:

1. **ufunc class** (lines 10552-10735): `_UFUNC_SENTINEL`, `class ufunc` with `__init__`, `_create`, `__call__`, `__repr__`, `reduce`, `accumulate`, `outer`, `reduceat`, `at`, `_generic_reduce`, `_generic_accumulate`
2. **Wrapping block** (lines 10737-10897): The `_add_func = add` / `add = ufunc._create(...)` pattern for all ~67 operations. **Remove** these lines since `_ufunc.py` imports functions with aliases (e.g., `from ._math import add as _add_func`). Keep only the `ufunc._create(...)` calls.
3. **frompyfunc** (search for `def frompyfunc` — line ~8835): Move the function definition.

The wrapping block adaptation: In the monolith, `_add_func = add` saves the function before rebinding. In `_ufunc.py`, the import handles this: `from ._math import add as _add_func`. So the `_xxx_func = xxx` lines are removed; only the `xxx = ufunc._create(_xxx_func, ...)` lines remain.

**Critical detail:** The `_ufunc.py` module's `__all__` re-exports names like `add`, `subtract` etc. as ufunc objects. Since `__init__.py` imports `from ._math import *` first (getting plain functions), then `from ._ufunc import *` (overwriting with ufunc objects), the final namespace has ufunc objects. This is the correct behavior — it matches the current monolithic file's behavior.

- [ ] **Step 2: Update `__init__.py`** — add `from ._ufunc import *` (AFTER `from ._math import *` and `from ._bitwise import *`), delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

Pay special attention to `test_ufunc.py` — it tests isinstance, attributes, reduce, accumulate, outer, etc.

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_ufunc.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _ufunc.py submodule"
```

---

## Chunk 4: Domain-Specific Modules

### Task 11: Extract `_poly.py`

**Files:**
- Create: `packages/numpy-rust/python/numpy/_poly.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (lines ~6832-7137 + lines ~10508-10542):
- `poly1d` class
- `poly`, `polyfit`, `polyval`, `polyder`, `polyint`, `polymul`, `polyadd`, `polysub`, `polydiv`
- `convolve`, `correlate`
- `vander`

- [ ] **Step 1: Create `_poly.py`**

```python
"""Polynomial utilities."""
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import _builtin_range
from ._creation import array, asarray, zeros, ones, arange, concatenate

__all__ = [
    'poly1d', 'poly', 'polyfit', 'polyval', 'polyder', 'polyint',
    'polymul', 'polyadd', 'polysub', 'polydiv',
    'convolve', 'correlate', 'vander',
]
```

- [ ] **Step 2: Update `__init__.py`** — add `from ._poly import *`, delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
cd packages/numpy-rust
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_poly.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _poly.py submodule"
```

---

### Task 12: Extract `_linalg_ext.py`

**Files:**
- Create: `packages/numpy-rust/python/numpy/_linalg_ext.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (lines ~8937-9087):
- Linalg wrappers: `inv`, `det`, `lstsq`, `qr`, `cholesky`, `eigvals`, `eig`, `svd`, `norm`, `matrix_rank`, `matrix_power`, `pinv`, `solve`, `eigh`, `eigvalsh`, `cond`, `slogdet`, `matrix_transpose`
- Math: `outer`, `inner`, `dot`, `vdot`, `matmul`, `kron`, `trace`, `diagonal`, `diag`, `diagflat`, `cross`, `tensordot`
- `einsum` (line ~6441), `einsum_path` (line ~11563)

- [ ] **Step 1: Create `_linalg_ext.py`**

```python
"""Python-level linalg wrappers. Monkey-patches the Rust linalg module."""
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import _builtin_range, _copy_into
from ._creation import array, asarray, zeros, ones, eye, empty, concatenate, arange
from ._manipulation import transpose, reshape, stack, expand_dims, squeeze

__all__ = [
    'outer', 'inner', 'dot', 'vdot', 'matmul', 'kron', 'trace',
    'diagonal', 'diag', 'diagflat', 'cross', 'tensordot',
    'einsum', 'einsum_path',
    'matrix_transpose',
]
```

Copy the linalg wrapper functions from `__init__.py` (lines ~8937-9087). These monkey-patch `_native.linalg`:

```python
linalg = _native.linalg

def _linalg_pinv(a, rcond=1e-15):
    # ... copy from __init__.py ...
    pass

# After defining each wrapper:
linalg.pinv = _linalg_pinv
linalg.norm = _linalg_norm
# etc. for: inv, det, lstsq, qr, cholesky, eigvals, eig, svd, norm,
# matrix_rank, matrix_power, pinv, solve, eigh, eigvalsh, cond, slogdet
```

Since `_native.linalg` is a shared module object, mutations here are visible everywhere.

- [ ] **Step 2: Update `__init__.py`** — add `from ._linalg_ext import *`, delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
cd packages/numpy-rust
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_linalg_ext.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _linalg_ext.py submodule"
```

---

### Task 13: Extract `_fft_ext.py`

**Files:**
- Create: `packages/numpy-rust/python/numpy/_fft_ext.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (lines ~9088-9388):
- All `_fft_*` wrapper functions
- FFT module monkey-patching

- [ ] **Step 1: Create `_fft_ext.py`**

```python
"""Python-level FFT wrappers."""
import _numpy_native as _native
from _numpy_native import ndarray
from ._creation import array, asarray, zeros, arange, concatenate
from ._math import exp, sin, cos

# __all__ is empty because FFT functions are monkey-patched onto _native.fft
# (a shared module object). Users access them as np.fft.fft(), np.fft.ifft(), etc.
# The module object `fft` is re-exported from __init__.py as `fft = _native.fft`.
__all__ = []
```

Copy all `_fft_*` wrapper functions from `__init__.py` (lines ~9088-9388) and the monkey-patching that assigns them to `_native.fft`:

```python
fft_mod = _native.fft

def _fft_fftshift(x, axes=None):
    # ... copy verbatim from __init__.py ...

fft_mod.fftshift = _fft_fftshift
# etc. for: fft, ifft, fftn, ifftn, rfft, irfft, rfftn, irfftn,
# fft2, ifft2, rfft2, irfft2, hfft, ihfft, fftshift, ifftshift, fftfreq, rfftfreq
```

- [ ] **Step 2: Update `__init__.py`** — add `from ._fft_ext import *`, delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
cd packages/numpy-rust
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_fft_ext.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _fft_ext.py submodule"
```

---

### Task 14: Extract `_random_ext.py`

**Files:**
- Create: `packages/numpy-rust/python/numpy/_random_ext.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract** (lines ~9389-10360):
- All `_random_*` functions
- `_Generator` class, `_RandomState` class
- Random module setup and monkey-patching

- [ ] **Step 1: Create `_random_ext.py`**

```python
"""Random number generation."""
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import _builtin_range
from ._creation import array, asarray, zeros, ones, empty, arange

# __all__ is empty because random functions are monkey-patched onto _native.random
# (a shared module object). Users access them as np.random.normal(), np.random.seed(), etc.
# The module object `random` is re-exported from __init__.py as `random = _native.random`.
__all__ = []
```

Copy all `_random_*` wrapper functions, `_Generator` class, and `_RandomState` class from `__init__.py` (lines ~9389-10360) and the monkey-patching:

```python
random_mod = _native.random

class _Generator:
    # ... copy verbatim from __init__.py ...

class _RandomState:
    # ... copy verbatim from __init__.py ...

def _random_normal(loc=0.0, scale=1.0, size=None):
    # ... copy verbatim ...

random_mod.normal = _random_normal
# etc. for all ~38 distribution functions
```

- [ ] **Step 2: Update `__init__.py`** — add `from ._random_ext import *`, delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
cd packages/numpy-rust
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_random_ext.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _random_ext.py submodule"
```

---

## Chunk 5: Remaining Modules + Final `__init__.py`

### Task 15: Extract `_string_ops.py`, `_indexing.py`, `_window.py`, `_io.py`

Extract four smaller modules in one task.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_string_ops.py` (lines ~6461-6732)
- Create: `packages/numpy-rust/python/numpy/_indexing.py` (lines ~6733-6831, 7745-8026, 8440-8554, plus histogram functions)
- Create: `packages/numpy-rust/python/numpy/_window.py` (lines ~8555-8630)
- Create: `packages/numpy-rust/python/numpy/_io.py` (lines ~7138-7273, 8787-8834)
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

- [ ] **Step 1: Create `_string_ops.py`**

```python
"""String/char array operations."""
import _numpy_native as _native

__all__ = ['char', 'string_types']
```

Copy the `_char_mod` class verbatim from `__init__.py` (lines 6462-6732). It contains ~30 static methods (`upper`, `lower`, `capitalize`, `strip`, `str_len`, etc.) that delegate to `_native.char_*` functions. After the class:

```python
char = _char_mod()
string_types = (str,)
```

- [ ] **Step 2: Create `_indexing.py`**

```python
"""Index generation, iteration, histograms."""
import math as _math
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import _builtin_range, _builtin_min, _builtin_max
from ._creation import array, asarray, zeros, ones, arange, linspace, concatenate
from ._reductions import sum, diff

__all__ = [
    'mgrid', 'ogrid', 'r_', 'c_', 's_', 'indices', 'ix_',
    'ndindex', 'ndenumerate', 'nditer',
    'diag_indices', 'diag_indices_from', 'tril_indices', 'triu_indices',
    'tril_indices_from', 'triu_indices_from', 'tril', 'triu', 'fill_diagonal',
    'digitize', 'histogram', 'histogram_bin_edges', 'histogram2d',
    'histogramdd', 'bincount',
    'unravel_index', 'ravel_multi_index',
    'binary_repr', 'base_repr',
]
```

- [ ] **Step 3: Create `_window.py`**

```python
"""Signal window functions."""
import math as _math
from ._helpers import _builtin_range
from ._creation import array, asarray, arange, zeros
from ._math import cos, sin
```

Copy all window functions from `__init__.py` (lines ~8555-8630). The `__all__` must include every window function that exists in the source. Check with:

```bash
grep -n "^def bartlett\|^def blackman\|^def hamming\|^def hanning\|^def hann\b\|^def kaiser\|^def tukey\|^def triang\|^def flattop\|^def parzen\|^def gaussian\b\|^def slepian\|^def dpss\|^def chebwin\|^def cosine\b\|^def boxcar\|^def bohman" python/numpy/__init__.py
```

Include ALL found functions in `__all__`. The spec lists up to 16 window functions — only include those that actually exist in the source.

- [ ] **Step 4: Create `_io.py`**

```python
"""File I/O functions."""
from ._creation import array, asarray
from ._helpers import _builtin_range

__all__ = [
    'loadtxt', 'savetxt', 'genfromtxt',
    'save', 'load', 'savez', 'savez_compressed',
]
```

- [ ] **Step 5: Update `__init__.py`** — add imports for all four, delete extracted blocks

- [ ] **Step 6: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 7: Commit**

```bash
git add python/numpy/_string_ops.py python/numpy/_indexing.py python/numpy/_window.py python/numpy/_io.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _string_ops, _indexing, _window, _io submodules"
```

---

### Task 16: Extract `_stubs.py`

Extract module stubs, misc utilities, and format functions — everything remaining that doesn't fit elsewhere.

**Files:**
- Create: `packages/numpy-rust/python/numpy/_stubs.py`
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

**What to extract:**
- Module stubs: `_CoreModule`, `_CompatModule`, `_ExceptionsModule`, `_MatlibModule`, `_CtypeslibModule`, `_LibModule`, `_ScimathModule`, `_dtypes_mod`, `_RecModule`
- Module instances: `core`, `compat`, `exceptions`, `matlib`, `ctypeslib`, `lib`, `scimath`, `dtypes`, `rec`
- `memmap`, `matrix` class stubs
- `NumpyVersion` class
- Error state: `seterr`, `geterr`, `errstate`, `seterrcall`, `geterrcall`
- Print options: `set_printoptions`, `get_printoptions`, `printoptions`
- Display: `array_str`, `array_repr`, `array2string`, `info`, `source`, `lookfor`, `who`, `__dir__`
- Format: `format_float_positional`, `format_float_scientific`
- Stubs: `show_config`, `get_include`, `add_newdoc`, `deprecate`, `byte_bounds`
- Constants: `tracemalloc_domain`, `use_hugepage`, `nested_iters`
- `_TestingModule` class, `_AssertRaisesRegexContext`
- `_MachAr` class (if not already in `_core_types.py`)
- `interp` function
- `pad` function
- `meshgrid` function
- `take_along_axis`, `put_along_axis`

- [ ] **Step 1: Create `_stubs.py`**

```python
"""Module stubs, misc utilities, and format functions."""
import _numpy_native as _native
from _numpy_native import ndarray
from ._helpers import _builtin_range, _builtin_min, _builtin_max, AxisError
from ._core_types import dtype, _ScalarType, Float64DType, Int64DType  # etc.
from ._creation import array, asarray, zeros, ones, empty, arange, linspace, concatenate

__all__ = [
    # Module stubs
    'core', 'compat', 'exceptions', 'matlib', 'ctypeslib', 'lib', 'scimath',
    'dtypes', 'rec',
    # Classes
    'memmap', 'matrix', 'NumpyVersion',
    # Error/print control
    'seterr', 'geterr', 'errstate', 'seterrcall', 'geterrcall',
    'set_printoptions', 'get_printoptions', 'printoptions',
    # Display
    'array_str', 'array_repr', 'array2string', 'info', 'source', 'lookfor', 'who',
    # Format
    'format_float_positional', 'format_float_scientific',
    # Stubs
    'show_config', 'get_include', 'add_newdoc', 'deprecate', 'byte_bounds',
    # Misc
    'tracemalloc_domain', 'use_hugepage', 'nested_iters',
    'interp', 'pad', 'meshgrid', 'take_along_axis', 'put_along_axis',
]
```

- [ ] **Step 2: Update `__init__.py`**, delete extracted blocks

- [ ] **Step 3: Run tests**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

- [ ] **Step 4: Commit**

```bash
git add python/numpy/_stubs.py python/numpy/__init__.py
git commit -m "refactor(numpy): extract _stubs.py submodule"
```

---

### Task 17: Finalize `__init__.py` as thin re-export layer

After all extractions, `__init__.py` should be ~500 lines — just imports, constants, and `__getattr__`.

**Files:**
- Modify: `packages/numpy-rust/python/numpy/__init__.py`

- [ ] **Step 1: Verify remaining content**

At this point, `__init__.py` should only contain:
- Module docstring and version
- `import _numpy_native as _native`
- `from _numpy_native import ndarray`
- `from ._helpers import *` through `from ._stubs import *` (in dependency order)
- Constants: `nan`, `inf`, `pi`, `e`, `euler_gamma`, `newaxis`, `NaN`, `Inf`, `PINF`, `NINF`, `PZERO`, `NZERO`, `ALLOW_THREADS`, `little_endian`
- `__getattr__` for deprecated aliases
- `import numpy.ma` and `import numpy.polynomial`

Check that nothing was missed:

```bash
wc -l python/numpy/__init__.py
# Should be ~500 lines or less
```

- [ ] **Step 2: Clean up `__init__.py`**

Rewrite to be a clean, well-organized re-export layer:

```python
"""NumPy-compatible Python package wrapping the Rust native module."""
import sys as _sys
import math as _math
from functools import reduce as _reduce

__version__ = "1.26.0"

# Import from native Rust module
import _numpy_native as _native
from _numpy_native import ndarray

# 1. Internal helpers (no numpy dependencies)
from ._helpers import *

# 2. Type system
from ._core_types import *

# 3. Datetime
from ._datetime import *

# 4. Array creation (needs types)
from ._creation import *

# 5. Math + arithmetic (needs creation for asarray)
from ._math import *

# 6. Reductions (needs math for nan checks)
from ._reductions import *

# 7. Manipulation (needs creation, math)
from ._manipulation import *

# 8. Bitwise/logical (needs creation)
from ._bitwise import *

# 9. ufunc wrapping (wraps functions into ufunc objects — MUST be after math/bitwise)
from ._ufunc import *

# 10. Domain-specific (independent of each other)
from ._poly import *
from ._linalg_ext import *
from ._fft_ext import *
from ._random_ext import *
from ._string_ops import *
from ._indexing import *
from ._window import *
from ._io import *
from ._stubs import *

# --- Constants ---
nan = NaN = NAN = float('nan')
inf = Inf = Infinity = float('inf')
PINF = float('inf')
NINF = float('-inf')
PZERO = 0.0
NZERO = -0.0
pi = _math.pi
e = _math.e
euler_gamma = 0.5772156649015329
newaxis = None
ALLOW_THREADS = 1
little_endian = _sys.byteorder == 'little'

# Subpackage imports
import numpy.ma
import numpy.polynomial

# Module-level linalg/fft/random references
linalg = _native.linalg
fft = _native.fft
random = _native.random

# Deprecated aliases — copy verbatim from __init__.py lines 11694-11707
def __getattr__(name):
    _deprecated = {
        'bool': bool, 'int': int, 'float': float, 'complex': complex,
        'object': object, 'str': str,
    }
    if name in _deprecated:
        return _deprecated[name]
    raise AttributeError(f"module 'numpy' has no attribute {name!r}")
```

- [ ] **Step 3: Run ALL tests**

```bash
cd packages/numpy-rust
cargo test -q
./tests/python/run_tests.sh target/debug/numpy-python
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

Expected: Same test counts as before the refactor. All pass.

- [ ] **Step 4: Verify line count**

```bash
wc -l python/numpy/__init__.py
# Target: ~500 lines or fewer
```

- [ ] **Step 5: Commit**

```bash
git add python/numpy/__init__.py
git commit -m "refactor(numpy): finalize __init__.py as thin re-export layer"
```

---

### Task 18: Final verification and cleanup

- [ ] **Step 1: Run full Rust test suite**

```bash
cd packages/numpy-rust
cargo test -q
```

- [ ] **Step 2: Run full Python test suite**

```bash
./tests/python/run_tests.sh target/debug/numpy-python
```

Expected: 1,106+ passed, 0 failed.

- [ ] **Step 3: Run compat tests**

```bash
./target/debug/numpy-python tests/numpy_compat/run_compat.py --ci
```

Expected: 1,207 passed, 3-4 xfail (same as before).

- [ ] **Step 4: Verify file sizes**

```bash
wc -l python/numpy/_*.py python/numpy/__init__.py
```

Each submodule should be under ~1,400 lines. `__init__.py` should be ~500 or less.

- [ ] **Step 5: Commit final state**

```bash
git add python/numpy/__init__.py python/numpy/_*.py
git commit -m "refactor(numpy): complete monolith split into 18 submodules"
```
