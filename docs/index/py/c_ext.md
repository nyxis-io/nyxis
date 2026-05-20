---
room: c_ext
subdomain: py
source_paths: py/_nxs.c, py/bench_c.py, py/bench_wal.py, py/test_c_ext.py
see_also: py/reader.md, c/reader.md
hot_paths: _nxs.c
architectural_health: normal
security_tier: sensitive
committee_notes: _nxs.c performs raw CPython buffer-protocol pointer arithmetic over caller-supplied data; review carefully when modifying the C extension's memory access patterns.
---

# py/ — C Extension & High-Throughput Bench

Subdomain: py/
Source paths: py/_nxs.c, py/bench_c.py, py/bench_wal.py, py/test_c_ext.py

## TASK → LOAD

| Task | Load |
|------|------|
| Use the C-accelerated Python reader/writer | c_ext.md |
| Benchmark C extension vs pure Python vs JSON | c_ext.md |
| Measure WAL-append throughput from Python | c_ext.md |
| Verify C extension parity with pure-Python reader | c_ext.md |

---

# _nxs.c

DOES: CPython C extension exposing accelerated `Reader`, `Object`, `Schema`, and `Writer` types for the NXS binary format. The `Reader` holds a raw buffer pointer and builds a key→slot dict once; `Object` eagerly computes a rank prefix-sum over the LEB128 bitmask so all typed accessors (`get_i64`, `get_str`, etc.) are O(1). Additional bulk methods (`scan_f64`, `scan_i64`, `sum_f64`, `min_f64`, `max_f64`, `sum_f64`, `sum_i64`) run entirely in C with no per-record Python overhead.
SYMBOLS:
- Reader.__init__(buffer: buffer-protocol object) -> None
- Reader.record(i: int) -> Object
- Reader.scan_f64(key: str) -> list[float | None]
- Reader.scan_i64(key: str) -> list[int | None]
- Reader.sum_f64(key: str) -> float
- Reader.min_f64(key: str) -> float | None
- Reader.max_f64(key: str) -> float | None
- Reader.sum_i64(key: str) -> int
- Object.get_i64(key: str) -> int | None
- Object.get_f64(key: str) -> float | None
- Object.get_bool(key: str) -> bool | None
- Object.get_str(key: str) -> str | None
- Object.get_time(key: str) -> int | None
- Schema.__init__(keys: list[str]) -> None
- Writer.__init__(schema: Schema) -> None
- Writer.begin_object() -> None
- Writer.end_object() -> None
- Writer.write_i64(slot: int, v: int) -> None
- Writer.write_f64(slot: int, v: float) -> None
- Writer.write_bool(slot: int, v: bool) -> None
- Writer.write_str(slot: int, s: str) -> None
- Writer.write_bytes(slot: int, data: bytes) -> None
- Writer.write_null(slot: int) -> None
- Writer.finish() -> bytes
- Writer.data_sector() -> bytes
- Writer.reset() -> None
- Types: ReaderObject, ObjectView, SchemaObject, WriterObject
DEPENDS: ../c/nxs_writer.h, ../c/nxs_writer.c
PATTERNS: CPython buffer-protocol, rank prefix-sum bitmask, columnar in-C reducer, include-once C writer
USE WHEN: Maximum read/write throughput is required in Python; the API is a drop-in superset of `nxs.py`/`nxs_writer.py` — prefer the C extension for production hot paths and the pure-Python modules for portability.

---

# bench_c.py

DOES: Three-way benchmark comparing `_nxs.Reader` (C extension), `NxsReader` (pure Python), and `json.loads` across open, warm random-access, cold-start, full per-record scan, and columnar/reducer scan scenarios at scales from 1k to 1M records.
SYMBOLS:
- run_scale(fixture_dir: Path, n: int) -> None
- main() -> int
- bench(iters: int, fn: Callable) -> float
DEPENDS: ./_nxs, ./nxs
PATTERNS: multi-scale fixture benchmark, columnar bulk API comparison
USE WHEN: Evaluating the C extension speedup over pure Python and over stdlib JSON; use `bench.py` when the C extension is not available.

---

# bench_wal.py

DOES: WAL-append throughput benchmark for NXS `NxsWriter` and `_nxs.Writer` (C extension) versus `json.dumps`, using a realistic 10-field span schema with mixed integer, string, and optional payload fields. Reports ns/span and k-spans/s with best-of-three timing.
SYMBOLS:
- bench_nxs_wal(n: int, warmup: bool) -> float
- bench_nxs_wal_c(n: int, warmup: bool) -> float
- bench_json_ndjson(n: int, warmup: bool) -> float
- make_span(i: int) -> dict
- run(n: int) -> None
DEPENDS: ./nxs_writer, ./_nxs (optional)
PATTERNS: WAL-append pattern, best-of-N timing, realistic synthetic workload
USE WHEN: Measuring write throughput for observability/tracing ingestion use cases; directly comparable to the Rust `cargo run --bin bench` output.

---

# test_c_ext.py

DOES: Parity test suite asserting that `_nxs.Reader` (C extension) produces byte-for-byte identical results to `NxsReader` (pure Python) and reference JSON for all field types, schema key lists, full-scan sums, absent-key handling, and out-of-bounds error behaviour.
SYMBOLS:
- main() -> int
DEPENDS: ./_nxs, ./nxs
PATTERNS: parity/cross-implementation assertion, fixture-driven
USE WHEN: Verifying C extension correctness after rebuilding `_nxs.so`; run with `python3 test_c_ext.py [fixtures_dir]`.
