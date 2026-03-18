# numpy-rust: Refactor Monolithic `__init__.py`

## Problem

`packages/numpy-rust/python/numpy/__init__.py` is 11,700 lines containing ~440 functions, ~94 classes, and ~62 logical sections. This makes it:
- Hard to navigate (finding a function requires searching 12K lines)
- Hard to edit safely (changes risk breaking unrelated code)
- Hard to reason about dependencies between sections
- Slow for tooling (type checkers, editors)

Additionally, ~18 element-wise operations still use Python loops over array data when they should be native Rust.

## Approach

Split `__init__.py` into focused submodules within the `numpy/` package. Each module has one clear responsibility and is 300–1000 lines. `__init__.py` becomes a thin re-export layer (~500 lines) that imports from submodules in dependency order.

Simultaneously, migrate bitwise and logical operations from Python loops to Rust.

Multi-file imports already work in RustPython — `numpy.ma`, `numpy.polynomial`, and `numpy.testing` prove this.

## Accessing `_numpy_native` from submodules

Each submodule needs access to the Rust native module. The pattern:

```python
# At the top of every submodule
import _numpy_native as _native
```

This works because `_numpy_native` is registered as a built-in module by the RustPython interpreter (not imported from a file), so it's available to any Python code regardless of import path.

For `linalg`, `fft`, `random` submodule objects: `_linalg_ext.py`, `_fft_ext.py`, and `_random_ext.py` access them via `_native.linalg`, `_native.fft`, `_native.random`. These are module objects — mutations (monkey-patching like `linalg.norm = ...`) affect the shared object visible to all importers.

Each submodule also needs `import math as _math` for standard library math functions.

## Module Decomposition

### `_helpers.py` (~250 lines)
Internal utilities used by multiple modules. Extracted first so other modules can import them.
- `_copy_into()`, `_apply_order()`, `_is_temporal_dtype()`, `_temporal_dtype_info()`, `_normalize_dtype()`, `_make_temporal_array()`, `_infer_shape()`, `_flatten_nested()`, `_to_float_list()`, `_unsupported_numeric_dtype()`
- `_CLIP_UNSET` sentinel (explicitly included in `__all__` despite leading underscore, since `_math.py` needs it)
- `_ObjectArray` class (used throughout for complex/object dtype fallback)
- `_ComplexResultArray` class
- `_ArrayFlags` class
- `AxisError` exception

Note: `_ObjectArray` only depends on `_numpy_native` directly (not on `asarray` or `concatenate`), so it's safe to import before `_creation.py`.

Depends on: `_numpy_native`, `math`, `sys`

### `_core_types.py` (~1,400 lines)
Type system: scalar types, dtype class, type hierarchy, finfo/iinfo, type casting.
- `_ScalarType`, `_NumpyIntScalar`, `_NumpyFloatScalar`, `_NumpyComplexScalar`
- `_ScalarTypeMeta` metaclass
- Scalar type aliases: `float64`, `float32`, `float16`, `int8`–`int64`, `uint8`–`uint64`, `complex64`, `complex128`, `bool_`, `str_`, `bytes_`, `void`, `object_`
- Type hierarchy: `generic` → `number` → `integer`/`inexact` → concrete types
- `StructuredDtype`, `_DTypeClassMeta`, `dtype` class
- Per-dtype DType classes: `Float64DType`, `Int8DType`, etc.
- `finfo`, `iinfo`
- `typecodes`, `sctypes`, `sctypeDict`
- Type casting: `can_cast`, `result_type`, `promote_types`, `find_common_type`, `common_type`, `mintypecode`
- Constants: `True_`, `False_`, `int_`

Depends on: `_helpers.py`, `_numpy_native`

### `_datetime.py` (~500 lines)
Datetime64 and timedelta64 support.
- Helper functions: `_is_nat_value()`, `_parse_datetime_string()`, `_date_to_days()`, `_days_to_date()`, `_to_common_unit()`, `_common_time_unit()`, `_is_dt64()`, `_is_td64()`, `_infer_datetime_unit()`
- `datetime64` class
- `timedelta64` class
- `isnat()`, `busday_count()`, `is_busday()`, `busday_offset()`

Depends on: `_helpers.py`, `_core_types.py`

### `_creation.py` (~700 lines)
Array creation functions.
- `array()` (the core Python-level wrapper — calls `_array_core()`)
- `_array_core()`, `_make_complex_array()`, `_detect_builtin_str_bytes()`, `_make_str_bytes_result()`, `_like_order()`
- `empty()`, `full()`, `full_like()`, `zeros()`, `zeros_like()`, `ones()`, `ones_like()`, `empty_like()`
- `eye()`, `identity()`, `arange()`, `linspace()`, `logspace()`, `geomspace()`
- `asarray()`, `ascontiguousarray()`, `asfortranarray()`, `asarray_chkfinite()`, `copy()`, `require()`
- `frombuffer()`, `fromfunction()`, `fromfile()`, `fromiter()`, `fromstring()`
- `array_equal()`, `array_equiv()`

Depends on: `_helpers.py`, `_core_types.py`, `_numpy_native`

### `_math.py` (~1,000 lines)
Element-wise math, type checking, comparison, arithmetic operators.
- Trig: `sin`, `cos`, `tan`, `arcsin`, `arccos`, `arctan`, `arctan2`
- Hyperbolic: `sinh`, `cosh`, `tanh`, `arcsinh`, `arccosh`, `arctanh`
- Exponential/log: `exp`, `exp2`, `log`, `log2`, `log10`, `log1p`, `expm1`, `logaddexp`, `logaddexp2`
- Rounding: `floor`, `ceil`, `trunc`, `rint`, `around`, `fix`
- Power: `sqrt`, `cbrt`, `square`, `reciprocal`, `power`, `float_power`
- Arithmetic: `add`, `subtract`, `multiply`, `divide`, `true_divide`, `floor_divide`, `remainder`, `mod`, `divmod`, `negative`, `positive`, `fmod`, `modf`, `fabs`
- Extrema: `maximum`, `minimum`, `fmax`, `fmin`
- Other: `abs`, `absolute`, `sign`, `signbit`, `copysign`, `heaviside`, `ldexp`, `frexp`, `hypot`, `sinc`, `nan_to_num`, `clip`, `nextafter`, `spacing`
- Angle: `deg2rad`, `rad2deg`
- Special: `gamma`, `lgamma`, `erf`, `erfc`, `j0`, `j1`, `y0`, `y1`, `i0`
- Complex: `real`, `imag`, `conj`, `angle`, `unwrap`, `real_if_close`
- Type checking: `isnan`, `isinf`, `isfinite`, `isneginf`, `isposinf`, `isscalar`, `isreal`, `iscomplex`, `isrealobj`, `iscomplexobj`, `issubdtype`, `issubclass_`
- Comparison: `allclose`, `isclose`, `greater`, `less`, `equal`, `not_equal`, `greater_equal`, `less_equal`
- GCD/LCM: `gcd`, `lcm`

Note: `add`, `subtract`, `multiply`, `divide`, `true_divide`, `floor_divide`, `remainder`, `maximum`, `minimum`, `fmax`, `fmin`, `fmod`, `negative`, `positive`, `absolute` are the plain function forms. They get wrapped as ufunc objects in `_ufunc.py` (which imports from `_math` and captures the function references before rebinding).

Depends on: `_helpers.py`, `_core_types.py`, `_creation.py`, `_numpy_native`

### `_reductions.py` (~700 lines)
Aggregation, statistics, NaN-aware reductions.
- Basic: `sum`, `prod`, `cumsum`, `cumprod`, `diff`, `ediff1d`, `gradient`, `trapz`, `trapezoid`, `cumulative_trapezoid`
- Statistics: `mean`, `std`, `var`, `median`, `average`, `cov`, `corrcoef`
- Extrema: `max`, `min`, `argmax`, `argmin`, `ptp`
- NaN-aware: `nansum`, `nanmean`, `nanstd`, `nanvar`, `nanmin`, `nanmax`, `nanargmin`, `nanargmax`, `nanprod`, `nancumsum`, `nancumprod`, `nanmedian`, `nanpercentile`, `nanquantile`
- Quantile: `quantile`, `percentile`
- Boolean: `all`, `any`, `count_nonzero`
- Search: `nonzero`, `flatnonzero`, `argwhere`, `searchsorted`, `where`
- Set operations: `intersect1d`, `union1d`, `setdiff1d`, `setxor1d`, `in1d`, `isin`
- Memory: `may_share_memory`, `shares_memory`

Depends on: `_helpers.py`, `_core_types.py`, `_creation.py`, `_math.py`, `_numpy_native`

### `_manipulation.py` (~1,000 lines)
Shape manipulation, stacking, splitting, reordering, selection, broadcasting.
- Shape: `reshape`, `ravel`, `flatten`, `expand_dims`, `squeeze`, `transpose`, `moveaxis`, `swapaxes`, `resize`
- Atleast: `atleast_1d`, `atleast_2d`, `atleast_3d`
- Stacking: `stack`, `hstack`, `vstack`, `dstack`, `column_stack`, `row_stack`, `concatenate`, `block`
- Splitting: `split`, `array_split`, `hsplit`, `vsplit`, `dsplit`
- Repetition: `repeat`, `tile`, `append`, `insert`, `delete`
- Reordering: `sort`, `argsort`, `lexsort`, `partition`, `argpartition`, `unique`
- Flipping: `flip`, `flipud`, `fliplr`, `rot90`, `roll`, `rollaxis`
- Selection: `extract`, `select`, `choose`, `take`, `compress`, `put`, `putmask`, `place`, `piecewise`, `copyto`
- Broadcasting: `broadcast` class, `_BroadcastIter`, `broadcast_shapes`, `broadcast_to`, `broadcast_arrays`
- Utility: `trim_zeros`, `apply_along_axis`, `apply_over_axes`, `vectorize` class
- Size/shape: `size` (free function)

Depends on: `_helpers.py`, `_core_types.py`, `_creation.py`, `_math.py`, `_numpy_native`

### `_bitwise.py` (~200 lines)
Bitwise and logical operations. Currently Python loops; the plan migrates these to Rust.
- Binary: `bitwise_and`, `bitwise_or`, `bitwise_xor`, `left_shift`, `right_shift`
- Unary: `bitwise_not`, `invert`
- Logical: `logical_and`, `logical_or`, `logical_xor`, `logical_not`
- Packing: `packbits`, `unpackbits`

Depends on: `_helpers.py`, `_creation.py`, `_numpy_native`

### `_ufunc.py` (~350 lines)
ufunc class and wrapping registration.
- `_UFUNC_SENTINEL` sentinel
- `ufunc` class with `__call__`, `reduce`, `accumulate`, `outer`, `reduceat`, `at`, `out=`, `dtype=`
- `frompyfunc()` (creates user ufuncs)
- Registration block wrapping all ~67 operations as ufunc objects
- Must execute after all function definitions — imports functions from `_math`, `_bitwise`, `_reductions`, etc. and wraps them

The ufunc registration pattern:
```python
from ._math import add as _add_func, subtract as _subtract_func, ...
from ._bitwise import logical_and as _logical_and_func, ...
# Then: add = ufunc._create(_add_func, 2, name='add', ...)
```

Depends on: `_helpers.py`, `_creation.py`, `_math.py`, `_reductions.py`, `_bitwise.py`, `_manipulation.py`

### `_poly.py` (~400 lines)
Polynomial utilities.
- `poly1d` class with arithmetic, evaluation, derivatives, integration
- Helper functions: `poly`, `polyfit`, `polyval`, `polyder`, `polyint`, `polymul`, `polyadd`, `polysub`, `polydiv`
- `convolve`, `correlate`
- `vander`

Depends on: `_helpers.py`, `_creation.py`, `_math.py`

### `_linalg_ext.py` (~200 lines)
Python-level linalg wrappers. Monkey-patches the Rust `linalg` module.
- Wrappers: `inv`, `det`, `lstsq`, `qr`, `cholesky`, `eigvals`, `eig`, `svd`, `norm`, `matrix_rank`, `matrix_power`, `pinv`, `solve`, `eigh`, `eigvalsh`, `cond`, `slogdet`, `matrix_transpose`
- Math: `outer`, `inner`, `dot`, `vdot`, `matmul`, `kron`, `trace`, `diagonal`, `diag`, `diagflat`, `cross`, `tensordot`
- `einsum` (basic), `einsum_path` (stub)

Depends on: `_helpers.py`, `_creation.py`, `_manipulation.py` (for `concatenate`, `transpose`, `reshape`), `_numpy_native.linalg`

### `_fft_ext.py` (~300 lines)
Python-level FFT wrappers.
- `fft`, `ifft`, `fftn`, `ifftn`, `rfft`, `irfft`, `rfftn`, `irfftn`
- `fft2`, `ifft2`, `rfft2`, `irfft2`, `hfft`, `ihfft`
- `fftshift`, `ifftshift`, `fftfreq`, `rfftfreq`

Depends on: `_helpers.py`, `_creation.py`, `_manipulation.py` (for `concatenate` in multi-dim FFT), `_numpy_native.fft`

### `_random_ext.py` (~970 lines)
Random number generation.
- `_Generator` class, `_RandomState` class
- All distribution functions: `normal`, `uniform`, `exponential`, `poisson`, `binomial`, `beta`, `gamma` (distribution), `standard_normal`, `standard_exponential`, `standard_gamma`, `randn`, `rand`, `randint`, `random`, `random_sample`, `ranf`, `sample`, `choice`, `seed`, `shuffle`, `permutation`, `lognormal`, `chisquare`, `weibull`, `pareto`, `laplace`, `geometric`, `hypergeometric`, `multivariate_normal`, `multinomial`, `dirichlet`, `zipf`, `f`, `negative_binomial`, `standard_cauchy`, `cauchy`, `logistic`, `wald`, `triangular`

Depends on: `_helpers.py`, `_creation.py`, `_numpy_native.random`

### `_string_ops.py` (~270 lines)
String/char array operations.
- `_char_mod` class with `upper`, `lower`, `capitalize`, `strip`, `str_len`, `startswith`, `endswith`, `replace`, `split`, `join`, `center`, `ljust`, `rjust`, `zfill`, `lstrip`, `rstrip`, `encode`, `decode`, `swapcase`, `title`, `find`, `count`, etc.
- `char` module instance
- `string_types` tuple

Depends on: `_helpers.py`, `_numpy_native`

### `_indexing.py` (~500 lines)
Index generation, iteration, histograms.
- Grid classes: `_MGrid`, `_OGrid`, `_RClass`, `_CClass`, `_SClass`
- Grid instances: `mgrid`, `ogrid`, `r_`, `c_`, `s_`
- Functions: `indices`, `ix_`
- Iterators: `ndindex`, `ndenumerate`, `nditer`
- Index functions: `diag_indices`, `diag_indices_from`, `tril_indices`, `triu_indices`, `tril_indices_from`, `triu_indices_from`, `tril`, `triu`, `fill_diagonal`
- Histograms: `digitize`, `histogram`, `histogram_bin_edges`, `histogram2d`, `histogramdd`, `bincount`
- Index manipulation: `unravel_index`, `ravel_multi_index`
- Utility: `binary_repr`, `base_repr`

Depends on: `_helpers.py`, `_creation.py`, `_reductions.py`

### `_io.py` (~200 lines)
File I/O functions.
- `loadtxt`, `savetxt`, `genfromtxt`
- `save`, `load`, `savez`, `savez_compressed`
- `_parse_field()` helper

Depends on: `_helpers.py`, `_creation.py`

### `_window.py` (~100 lines)
Signal window functions.
- `hann`, `hamming`, `blackman`, `bartlett`, `kaiser`, `tukey`, `triang`, `flattop`, `parzen`, `gaussian`, `slepian`, `dpss`, `chebwin`, `cosine`, `boxcar`, `bohman`
- `windows` module stub

Depends on: `_helpers.py`, `_creation.py`, `_math.py`

### `_stubs.py` (~400 lines)
Module stubs, misc utilities, and format functions.
- `memmap`, `matrix` class stubs
- `NumpyVersion` class
- Module stubs: `_CoreModule`, `_CompatModule`, `_ExceptionsModule`, `_MatlibModule`, `_CtypeslibModule`, `_LibModule`, `_ScimathModule`, `_dtypes_mod`, `_RecModule`
- Module instances: `core`, `compat`, `exceptions`, `matlib`, `ctypeslib`, `lib`, `scimath`, `dtypes`, `rec`
- Error state: `seterr`, `geterr`, `errstate`, `seterrcall`, `geterrcall`
- Print options: `set_printoptions`, `get_printoptions`, `printoptions`
- Display: `array_str`, `array_repr`, `array2string`, `info`, `source`, `lookfor`, `who`, `__dir__`
- Format: `format_float_positional`, `format_float_scientific`
- Stubs: `show_config`, `get_include`, `add_newdoc`, `deprecate`, `byte_bounds`, `fromfile` (if not in creation)
- Constants: `tracemalloc_domain`, `use_hugepage`, `nested_iters`
- `_TestingModule` class, `_AssertRaisesRegexContext`
- `_MachAr` class
- `interp` function
- `pad` function
- `meshgrid` function
- `take_along_axis`, `put_along_axis`

Depends on: `_helpers.py`, `_core_types.py`, `_creation.py`

### `__init__.py` (~500 lines)
Thin re-export layer.
- Import `_numpy_native` and set up Rust modules (`linalg`, `fft`, `random`)
- Constants: `nan`, `inf`, `pi`, `e`, `euler_gamma`, `newaxis`, `NaN`, `Inf`, `PINF`, `NINF`, `PZERO`, `NZERO`, `ALLOW_THREADS`, `little_endian`
- Import from each submodule (in dependency order)
- `__getattr__` for deprecated aliases (`np.bool` → `builtins.bool`, etc.)
- Import submodules: `numpy.ma`, `numpy.polynomial`

## Rust Migrations

### Bitwise Operations → `crates/numpy-rust-core/src/ops/bitwise.rs`

New file implementing:
- `bitwise_and(a, b)`, `bitwise_or(a, b)`, `bitwise_xor(a, b)` — binary, element-wise on int arrays
- `bitwise_not(a)` — unary, element-wise
- `left_shift(a, b)`, `right_shift(a, b)` — binary, element-wise

Pattern: Cast to i64 arrays, apply operator, return i64 array. Same macro-based approach as `comparison.rs`.

### Logical Operations → extend `crates/numpy-rust-core/src/ops/logical.rs`

Add:
- `logical_and(a, b)`, `logical_or(a, b)`, `logical_xor(a, b)` — binary, returns bool array
- `logical_not(a)` — unary, returns bool array (already partially in Rust via `_native.logical_not`)

Pattern: Cast both inputs to f64, apply `(a != 0.0) && (b != 0.0)` element-wise, return as f64 (0.0/1.0) to match existing behavior.

### Python Bindings → extend `crates/numpy-rust-python/src/lib.rs`

Expose new Rust functions to `_numpy_native`:
- `bitwise_and`, `bitwise_or`, `bitwise_xor`, `bitwise_not`, `left_shift`, `right_shift`
- `logical_and`, `logical_or`, `logical_xor`

The Python wrappers in `_bitwise.py` then become thin delegation layers (try native, fall back to `_ObjectArray` loop for complex).

## Import Order

`__init__.py` imports in dependency order:

```python
# 1. Native module
import _numpy_native as _native

# 2. Internal helpers (no numpy dependencies)
from ._helpers import *

# 3. Type system
from ._core_types import *

# 4. Datetime
from ._datetime import *

# 5. Array creation (needs types)
from ._creation import *

# 6. Math + arithmetic (needs creation for asarray)
from ._math import *

# 7. Reductions (needs math for nan checks)
from ._reductions import *

# 8. Manipulation (needs creation, math)
from ._manipulation import *

# 9. Bitwise/logical (needs creation)
from ._bitwise import *

# 10. ufunc wrapping (needs everything above — wraps functions into ufunc objects)
from ._ufunc import *

# 11. Domain-specific (independent of each other, may depend on manipulation)
from ._poly import *
from ._linalg_ext import *
from ._fft_ext import *
from ._random_ext import *
from ._string_ops import *
from ._indexing import *
from ._window import *
from ._io import *
from ._stubs import *
```

## Constraints

- **No behavior changes**: Every existing test must pass unchanged
- **No Rust wasm rebuild for the split**: The Python refactor is pure file moves. Rust migrations are a separate build step.
- **Submodule imports work**: `_numpy_native` is available as a built-in module; submodules access it via `import _numpy_native as _native`
- **`from ._X import *` pattern**: Each submodule defines `__all__` to control what's exported. Private helpers starting with `_` that other modules need (like `_CLIP_UNSET`) must be explicitly listed in `__all__`.
- **Cross-module helpers**: Functions that multiple modules need (like `asarray`, `_copy_into`) live in `_helpers.py` or `_creation.py` and are imported by dependents.
- **Standard lib in submodules**: Each submodule that needs `math`, `sys`, `functools`, etc. imports them at its own top level.

## What Doesn't Change

- External API (`import numpy as np; np.add(...)`) is identical
- `numpy.ma`, `numpy.polynomial`, `numpy.testing` subpackages untouched
- Rust core (`numpy-rust-core`, `numpy-rust-python`) only changes for bitwise/logical migrations
- Test files unchanged (except the compat runner timeout we already fixed)

## Files to Create

- `python/numpy/_helpers.py`
- `python/numpy/_core_types.py`
- `python/numpy/_datetime.py`
- `python/numpy/_creation.py`
- `python/numpy/_math.py`
- `python/numpy/_reductions.py`
- `python/numpy/_manipulation.py`
- `python/numpy/_bitwise.py`
- `python/numpy/_ufunc.py`
- `python/numpy/_poly.py`
- `python/numpy/_linalg_ext.py`
- `python/numpy/_fft_ext.py`
- `python/numpy/_random_ext.py`
- `python/numpy/_string_ops.py`
- `python/numpy/_indexing.py`
- `python/numpy/_window.py`
- `python/numpy/_io.py`
- `python/numpy/_stubs.py`

## Files to Modify

- `python/numpy/__init__.py` — gutted to ~500 lines of re-exports
- `crates/numpy-rust-core/src/ops/mod.rs` — add `bitwise` module
- `crates/numpy-rust-core/src/ops/bitwise.rs` — new: bitwise ops
- `crates/numpy-rust-core/src/ops/logical.rs` — extend: logical_and/or/xor
- `crates/numpy-rust-python/src/lib.rs` — expose new native functions

## Out of Scope

- Splitting Rust core into smaller files (it's already well-structured)
- Refactoring `_ObjectArray` (it works, it's ugly, leave it)
- Adding new functionality — this is a pure structural refactor + targeted Rust migrations
- Performance optimization beyond the bitwise/logical migrations
