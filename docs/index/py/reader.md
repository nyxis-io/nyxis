---
room: reader
subdomain: py
source_paths: py/nxs.py, py/nxs_writer.py, py/test_nxs.py, py/bench.py
see_also: py/c_ext.md
hot_paths: nxs.py, nxs_writer.py
architectural_health: normal
security_tier: normal
---

# py/ — Pure-Python Reader & Writer

Subdomain: py/
Source paths: py/nxs.py, py/nxs_writer.py, py/test_nxs.py, py/bench.py

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files in pure Python | reader.md |
| Write / emit .nxb from Python | reader.md |
| Run or add Python reader/writer tests | reader.md |
| Benchmark pure-Python NXS vs JSON | reader.md |

---

# bench.py

DOES: Benchmarks the pure-Python NXS reader against stdlib `json` across four scenarios (open/parse, warm random-access, cold start, full scan) at four fixture scales (1k–1M records). Outputs formatted tables with speed ratios relative to the JSON baseline.
SYMBOLS:
- bench(iters: int, fn: Callable) -> float
- run_scale(fixture_dir: Path, n: int) -> None
- main() -> int
- fmt_time(s: float) -> str
- fmt_bytes(n: int) -> str
- row(label: str, avg: float, baseline: float) -> None
DEPENDS: ./nxs
PATTERNS: micro-benchmark, warmup-loop, multi-scale fixture
USE WHEN: Measuring pure-Python NXS read performance versus JSON; use `bench_c.py` instead when the C extension is available for a three-way comparison.

---

# nxs.py

DOES: Pure-Python zero-copy `.nxb` reader implementing the Nyxis v1.1 binary wire format. Parses the file preamble, embedded schema, and tail-index on construction; individual records and their fields are decoded lazily and on demand.
SYMBOLS:
- NxsReader.__init__(buffer: Union[bytes, bytearray, memoryview]) -> None
- NxsReader.record(i: int) -> NxsObject
- NxsReader.records() -> Iterator[NxsObject]
- NxsObject.get_i64(key: str) -> Optional[int]
- NxsObject.get_f64(key: str) -> Optional[float]
- NxsObject.get_bool(key: str) -> Optional[bool]
- NxsObject.get_str(key: str) -> Optional[str]
- NxsObject.get_time(key: str) -> Optional[int]
- NxsObject.to_dict() -> dict
- NxsError(code: str, message: str)
- Types: NxsReader, NxsObject, NxsError
PATTERNS: lazy/on-demand decoding, memoryview zero-copy, slot-index lookup, LEB128 bitmask
USE WHEN: Reading `.nxb` files from pure Python without a compiled extension; prefer `_nxs.Reader` (C extension) for hot paths that require maximum throughput.

---

# nxs_writer.py

DOES: Pure-Python slot-based `.nxb` emitter that mirrors the Rust `NxsWriter` API. A `NxsSchema` is compiled once from a key list and shared across `NxsWriter` instances; typed `write_*` methods fill a bytearray buffer that `finish()` assembles into a complete, spec-compliant `.nxb` file.
SYMBOLS:
- NxsSchema.__init__(keys: List[str]) -> None
- NxsWriter.__init__(schema: NxsSchema) -> None
- NxsWriter.begin_object() -> None
- NxsWriter.end_object() -> None
- NxsWriter.write_i64(slot: int, v: int) -> None
- NxsWriter.write_f64(slot: int, v: float) -> None
- NxsWriter.write_bool(slot: int, v: bool) -> None
- NxsWriter.write_str(slot: int, v: str) -> None
- NxsWriter.write_bytes(slot: int, data: bytes) -> None
- NxsWriter.write_time(slot: int, unix_ns: int) -> None
- NxsWriter.write_null(slot: int) -> None
- NxsWriter.write_list_i64(slot: int, values: List[int]) -> None
- NxsWriter.write_list_f64(slot: int, values: List[float]) -> None
- NxsWriter.finish() -> bytes
- NxsWriter.from_records(keys: List[str], records: List[dict]) -> bytes
- Types: NxsSchema, NxsWriter
PATTERNS: slot-indexed write, back-patch header, LEB128 bitmask, schema-once / write-many
USE WHEN: Generating `.nxb` output from Python without the C extension; use `_nxs.Writer` when WAL-append throughput is critical.

---

# test_nxs.py

DOES: Smoke-test suite for the pure-Python NXS reader and writer. Covers schema key enumeration, O(1) record access for all field types, iterator correctness, error-code validation (bad magic, truncation, dict-hash mismatch), and writer round-trip fidelity including schema evolution and Unicode strings.
SYMBOLS:
- main() -> int
DEPENDS: ./nxs, ./nxs_writer
PATTERNS: inline test-case runner, fixture-driven, error-code assertion
USE WHEN: Verifying pure-Python reader/writer correctness after changes; run with `python3 test_nxs.py [fixtures_dir]`.
