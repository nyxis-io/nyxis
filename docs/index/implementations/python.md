---
room: implementations/python
source_paths: [py/]
file_count: 6
architectural_health: normal
security_tier: normal
hot_paths: [nxs.py, _nxs.c]
see_also: [implementations/c.md, spec/format.md]
---

# _nxs.c

DOES: CPython C extension exposing Reader and Object types with eager schema indexing and lazy field decoding; implements LEB128 bitmask walk and prefix-sum rank table for O(1) field lookup without Python overhead.
SYMBOLS:
- Reader(buffer) -> Reader
- Reader#record(i: int) -> Object
- Reader#scan_f64(key: str) -> list[float]
- Reader#sum_f64(key: str) -> float
- Reader#min_f64(key: str) -> float
- Reader#max_f64(key: str) -> float
- Reader#sum_i64(key: str) -> int
- Object#get_i64(key: str) -> int|None
- Object#get_f64(key: str) -> float|None
- Object#get_bool(key: str) -> bool|None
- Object#get_str(key: str) -> str|None
- Types: ReaderObject, ObjectView
DEPENDS: none
PATTERNS: cpython-extension, rank-prefix-sum, leb128-walk, schema-precompute
USE WHEN: Performance-critical bulk scans and reducers; same API as nxs.py, ~10× faster.

DISAMBIGUATION: There is also a pure-Python implementation of the same Reader/Object API in `nxs.py` (`implementations/python.md`). Use `_nxs.c` when throughput matters; use `nxs.py` for portability or as an API reference.

---

# bench.py

DOES: Benchmarks pure-Python NXS reader vs JSON across open, random-access, cold-start, and full-scan scenarios at multiple scales (1K–1M records).
SYMBOLS:
- bench(iters: int, fn) -> float
- run_scale(fixture_dir: Path, n: int) -> None
- main() -> int
DEPENDS: nxs
PATTERNS: warmup, parametric-scaling, comparative-analysis
USE WHEN: Validating pure-Python reader performance against JSON baseline.

---

# bench_c.py

DOES: Benchmarks C extension vs pure-Python vs JSON across all scenarios including columnar reducer paths; reports speedup multipliers.
SYMBOLS:
- bench(iters, fn) -> float
- run_scale(fixture_dir, n) -> None
- main() -> int
DEPENDS: nxs, _nxs
PATTERNS: warmup, parametric-scaling, reducer-comparison
USE WHEN: Demonstrating C extension speedup or validating bulk API improvements.

---

# nxs.py

DOES: Pure-Python zero-copy NXS reader: O(1) record lookup via tail-index, LEB128 bitmask unpacking, lazy typed field decoding. No compiled dependencies.
SYMBOLS:
- NxsError(code: str, message: str)
- NxsReader(buffer) -> NxsReader
- NxsReader#record(i: int) -> NxsObject
- NxsReader#records() -> Iterator[NxsObject]
- NxsObject#get_i64(key: str) -> int|None
- NxsObject#get_f64(key: str) -> float|None
- NxsObject#get_bool(key: str) -> bool|None
- NxsObject#get_str(key: str) -> str|None
- NxsObject#to_dict() -> dict
- Types: NxsReader, NxsObject, NxsError
DEPENDS: none
PATTERNS: lazy-decode, leb128-bitmask-unpacking, schema-precompute
USE WHEN: Portable reader without C compilation; also the canonical API reference for the Python interface.

DISAMBIGUATION: There is also a C extension `_nxs.c` (`implementations/python.md`) with the same API and ~10× higher throughput. Use `nxs.py` for portability; use `_nxs` for bulk workloads.

---

# test_c_ext.py

DOES: Validates C extension parity with pure-Python reader and JSON across i64, str, f64, bool fields, missing keys, and out-of-bounds errors.
SYMBOLS:
- main() -> int
- case(name: str, fn) -> None
DEPENDS: nxs, _nxs
PATTERNS: parity-testing, fixture-driven, exception-cases
USE WHEN: Verifying C extension correctness; run after rebuilding _nxs.so.

---

# test_nxs.py

DOES: Smoke tests for pure-Python NxsReader: file open, schema keys, indexed access, iteration, floating-point precision, and out-of-bounds error handling.
SYMBOLS:
- main() -> int
- case(name: str, fn) -> None
DEPENDS: nxs
PATTERNS: smoke-testing, fixture-driven, type-coverage
USE WHEN: Validating pure-Python reader; run with `python test_nxs.py [fixtures_dir]`.
