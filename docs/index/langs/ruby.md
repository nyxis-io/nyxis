---
room: ruby
subdomain: langs
source_paths: ruby/
see_also: langs/php.md, c/reader.md
hot_paths: nxs.rb, nxs_ext.c
architectural_health: normal
security_tier: sensitive
committee_notes: nxs_ext.c performs inline LEB128 scan and raw pointer arithmetic over Ruby string data; review carefully when modifying the C extension.
---

# langs/ — Ruby Implementation

Source paths: ruby/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files in pure Ruby | ruby.md |
| Write .nxb from Ruby | ruby.md |
| Use the Ruby C extension for max throughput | ruby.md |
| Benchmark Ruby NXS vs JSON/CSV | ruby.md |
| Build the Ruby C extension | ruby.md |
| Run Ruby tests | ruby.md |

---

# bench.rb

DOES: Six-scenario benchmark comparing NXS, JSON, and CSV across four fixture sizes (1k–1M records). Measures open-file, warm random access, cold start, full scan, reducer, and cold pipeline timings.
SYMBOLS:
- bench(iters, &block): Float
- run_scale(fixture_dir, n): void
- fmt_time(s): String
- fmt_bytes(n): String
- row(label, avg, baseline): void
DEPENDS: json, csv, nxs
PATTERNS: benchmark-harness, warmup-then-measure
USE WHEN: Profiling pure-Ruby reader throughput against JSON/CSV baselines; use bench_c.rb when also benchmarking the C extension.

---

# bench_c.rb

DOES: Three-way benchmark comparing `Nxs::CReader` (C extension), `Nxs::Reader` (pure Ruby), and `JSON.parse` across the same six scenarios and four fixture sizes. Includes parity checks before timing.
SYMBOLS:
- run_parity(fixture_dir): void
- run_scale(fixture_dir, n): void
- bench_nxs_wal_c(n): Float
- bench(iters): Float
DEPENDS: json, nxs, nxs_ext
PATTERNS: benchmark-harness, parity-before-bench
USE WHEN: Measuring the C-extension speedup relative to pure Ruby; use bench.rb for pure-Ruby-only comparisons.

---

# bench_wal.rb

DOES: WAL-append throughput benchmark that generates synthetic 10-field span records and measures serialization latency via `Nxs::Writer`, `Nxs::CWriter`, and `JSON`. Reports spans/second.
SYMBOLS:
- bench_nxs_wal(n): Float
- bench_json(n): Float
- bench_nxs_wal_c(n): Float
- make_span(i): Hash
DEPENDS: json, nxs_writer, nxs_ext (optional)
PATTERNS: write-path benchmark, synthetic-data generation
USE WHEN: Measuring write/append throughput for streaming log or trace workloads.

---

# extconf.rb

DOES: MRI extension build configuration for the `nxs_ext` C extension. Generates a Makefile that compiles `nxs_ext.c` into `nxs/nxs_ext.so`.
SYMBOLS:
- create_makefile("nxs/nxs_ext"): void
DEPENDS: mkmf
PATTERNS: ruby-c-extension-build
USE WHEN: Building the C extension before running bench_c.rb or test_c; not invoked directly — called by `ruby extconf.rb && make`.

---

# nxs.rb

DOES: Pure-Ruby NXS binary reader implementing the Nyxis v1.1 wire format. Parses file preamble, schema header, and tail-index; provides O(1) record lookup and allocation-free bulk reducers.
SYMBOLS:
- Reader.new(bytes): Reader
- Reader#record(i): Object
- Reader#sum_f64(key): Float
- Reader#min_f64(key): Float?
- Reader#max_f64(key): Float?
- Reader#sum_i64(key): Integer
- Reader#keys: Array<String>
- Reader#record_count: Integer
- Object#get_str(key): String?
- Object#get_i64(key): Integer?
- Object#get_f64(key): Float?
- Object#get_bool(key): true/false?
- Reader#_scan_offset(data, obj_offset, slot): Integer?
TYPE: NxsError { code }
DEPENDS: (stdlib only)
PATTERNS: zero-copy reader, LEB128 bitmask walk, tail-index O(1) lookup, MurmurHash3-64
USE WHEN: Reading `.nxb` files in pure Ruby with no compiled dependencies; use `nxs_ext.c` / `Nxs::CReader` when maximum throughput is required.

---

# nxs_ext.c

DOES: MRI Ruby C extension exposing `Nxs::CReader`, `Nxs::CObject`, `Nxs::CSchema`, and `Nxs::CWriter`. Implements bulk reducers (`sum_f64`, `min_f64`, `max_f64`, `sum_i64`) as zero-VALUE-allocation C loops and includes the shared `nxs_writer.h` writer.
SYMBOLS:
- CReader.new(bytes): CReader
- CReader#record(i): CObject
- CReader#record_count: Integer
- CReader#keys: Array
- CReader#sum_f64(key): Float
- CReader#min_f64(key): Float?
- CReader#max_f64(key): Float?
- CReader#sum_i64(key): Integer
- CObject#get_str / get_i64 / get_f64 / get_bool(key): typed
- CSchema.new(keys_ary): CSchema
- CWriter.new(schema): CWriter
- CWriter#begin_object / end_object / finish / data_sector / reset
DEPENDS: ruby.h, ruby/encoding.h, c/nxs_writer.h
PATTERNS: TypedData_Make_Struct, GC-mark/free callbacks, inline LEB128 scan, WAL data_sector path
USE WHEN: Maximum-throughput reading or writing from Ruby; requires `ruby extconf.rb && make` build step.

---

# nxs_writer.rb

DOES: Pure-Ruby NXS writer that emits conformant `.nxb` files directly into a binary buffer. Provides a `Schema`-precompiled, slot-indexed hot path for record writing with bitmask and offset-table back-patching on `end_object`.
SYMBOLS:
- Schema.new(keys): Schema
- Writer.new(schema): Writer
- Writer#begin_object: void
- Writer#end_object: void
- Writer#finish: String (binary)
- Writer#write_i64 / write_f64 / write_bool / write_str / write_null / write_bytes(slot, v): void
- Writer#write_list_i64 / write_list_f64(slot, values): void
- Writer.from_records(keys, records): String
TYPE: Schema { keys, bitmask_bytes }
TYPE: Frame { start, bitmask, offset_table, slot_offsets, last_slot, needs_sort }
DEPENDS: (stdlib only)
PATTERNS: two-phase write (placeholder + back-patch), LEB128 bitmask, MurmurHash3-64
USE WHEN: Generating `.nxb` output in pure Ruby without a C extension; prefer `Nxs::CWriter` for WAL/high-throughput paths.

---

# test.rb

DOES: Parity and round-trip test suite for `Nxs::Reader` and `Nxs::Writer`. Validates field values against the 1000-record JSON fixture, covers edge cases (out-of-bounds, bad magic, corrupt DictHash, multi-byte bitmask).
SYMBOLS:
- check(label, &blk): Boolean
DEPENDS: json, nxs, nxs_writer
PATTERNS: fixture-based parity testing, ANSI-colored pass/fail output
USE WHEN: Verifying the Ruby reader/writer against the shared JSON fixture; run via `ruby ruby/test.rb js/fixtures`.
