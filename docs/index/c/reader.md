---
room: reader
subdomain: c
source_paths: c/
see_also: go/reader.md, langs/ruby.md, langs/php.md
hot_paths: nxs.c, nxs_writer.c
architectural_health: normal
security_tier: sensitive
committee_notes: nxs.c and nxs_writer.c perform raw pointer arithmetic over caller-supplied buffers; the writer is also shared as an include by py/_nxs.c and ruby/ext/nxs/nxs_ext.c.
---

# c/ — C Reader & Writer

Subdomain: c/
Source paths: c/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files from C | reader.md |
| Write .nxb output from C | reader.md |
| Include NXS reader/writer in a C or C++ project | reader.md |
| Benchmark C NXS vs JSON/CSV | reader.md |
| Run C smoke tests | reader.md |

---

# bench.c

DOES: C benchmark comparing NXS `nxs_sum_f64`/`nxs_sum_i64`/random-access against raw JSON byte-scan and raw CSV byte-scan on 1M-record fixtures; reports best-of-5 wall-clock times for each operation.
SYMBOLS:
- main(int argc, char **argv) int
- json_sum_score(buf, len) double
- csv_sum_score(buf, len) double
- read_file(path, out_size) uint8_t*
- elapsed_ms(a, b) double
- Types: json_ctx_t, csv_ctx_t, nxs_f64_ctx_t, nxs_i64_ctx_t, nxs_rand_ctx_t
DEPENDS: nxs.h, stdio.h, stdlib.h
PATTERNS: best-of-N harness with clock_gettime, function-pointer dispatch, raw-byte column scanner
USE WHEN: Comparing C NXS read performance against JSON/CSV at scale; for WAL write-path benchmarks use bench_wal.c.

---

# bench_wal.c

DOES: Benchmarks NxsWriter WAL-append throughput against snprintf-based JSON serialization using a 10-field distributed-tracing span schema at 1k/10k/100k spans; reports best-of-3 ns/span.
SYMBOLS:
- main(void) int
- ns_per_span_nxs(n int) double
- ns_per_span_json(n int) double
- span_dur_ns(op_idx, i) int64_t
- span_status(i) int
- span_payload(op_idx, i) const char*
- fmt_ns(ns, out, sz) void
DEPENDS: nxs_writer.h, stdio.h, stdlib.h
PATTERNS: best-of-3 timing, deterministic pseudo-random workload generation
USE WHEN: Measuring write-path cost of NxsWriter vs snprintf/JSON for telemetry/logging pipelines; for read-path benchmarks use bench.c.

---

# nxs.c

DOES: C99 implementation of the NXS zero-copy reader: opens and validates .nxb buffers, parses the embedded schema into a fixed-size pool, resolves field slots via LEB128 bitmask walk, and provides typed accessors and allocation-free bulk reducers (sum/min/max).
SYMBOLS:
- nxs_open(r, data, size) nxs_err_t
- nxs_close(r) void
- nxs_record_count(r) uint32_t
- nxs_slot(r, key) int
- nxs_record(r, i, obj) nxs_err_t
- nxs_resolve_slot(obj, slot) int64_t
- nxs_get_i64(obj, key, out) nxs_err_t
- nxs_get_f64(obj, key, out) nxs_err_t
- nxs_get_bool(obj, key, out) nxs_err_t
- nxs_get_str(obj, key, buf, buf_len) nxs_err_t
- nxs_get_i64_slot / nxs_get_f64_slot / nxs_get_bool_slot / nxs_get_str_slot (slot variants)
- nxs_sum_f64(r, key) double
- nxs_sum_i64(r, key) int64_t
- nxs_min_f64(r, key, out) nxs_err_t
- nxs_max_f64(r, key, out) nxs_err_t
DEPENDS: nxs.h, string.h, math.h
PATTERNS: memcpy-based LE reads (no UB), LEB128 bitmask walk, MurmurHash3-64 schema integrity check, linear key lookup
USE WHEN: Reading .nxb buffers from C; pair with nxs_slot() to pre-resolve keys for hot-loop column scans.
DISAMBIGUATION: `murmur3_64` also appears in `compiler.rs` (`rust/compiler_pipeline.md`) and `nxs.go` (`go/reader.md`). This C implementation is used at both read-open time (verify DictHash) and write-finish time (compute DictHash via `nxs_writer.c`). Load `rust/compiler_pipeline.md` if the question is about the Rust crate's hash computation.

---

# nxs.h

DOES: Public C99 header for the NXS reader: declares nxs_reader_t and nxs_object_t structs, the nxs_err_t error enum, and all reader, object, accessor, and bulk-reducer function prototypes.
SYMBOLS:
- nxs_open / nxs_close / nxs_record_count / nxs_slot / nxs_record / nxs_resolve_slot
- nxs_get_i64 / nxs_get_f64 / nxs_get_bool / nxs_get_str (key variants)
- nxs_get_i64_slot / nxs_get_f64_slot / nxs_get_bool_slot / nxs_get_str_slot (slot variants)
- nxs_sum_f64 / nxs_sum_i64 / nxs_min_f64 / nxs_max_f64
TYPE: nxs_reader_t { data, size, version, flags, dict_hash, tail_ptr, key_count, keys[256], key_sigils[256], record_count, tail_start, _pool }
TYPE: nxs_object_t { reader, offset, bitmask_start, offset_table_start, staged }
TYPE: nxs_err_t { NXS_OK=0, NXS_ERR_BAD_MAGIC=1, NXS_ERR_OUT_OF_BOUNDS=2, NXS_ERR_KEY_NOT_FOUND=3, NXS_ERR_FIELD_ABSENT=4, NXS_ERR_ALLOC=5, NXS_ERR_DICT_MISMATCH=6 }
DEPENDS: stddef.h, stdint.h
PATTERNS: C/C++ extern-C guard, pragma once
USE WHEN: Including the NXS reader in any C or C++ translation unit; include nxs_writer.h separately for the write path.

---

# nxs_writer.c

DOES: C99 implementation of NxsWriter: manages a growable data-sector buffer and frame stack, emits typed field values with back-patched bitmask and offset-table, then assembles the complete .nxb file (preamble + schema + data + tail-index) in `nxs_writer_finish`.
SYMBOLS:
- nxs_writer_init(w, keys, key_count, initial_cap) int
- nxs_writer_free(w) void
- nxs_writer_reset(w) void
- nxs_writer_begin_object(w) int
- nxs_writer_end_object(w) int
- nxs_write_i64 / nxs_write_f64 / nxs_write_bool / nxs_write_time / nxs_write_null / nxs_write_str / nxs_write_bytes
- nxs_writer_finish(w) int
DEPENDS: nxs_writer.h, stdlib.h, string.h
PATTERNS: realloc-based growable buffer, insertion-sort for out-of-order slot writes, MurmurHash3-64 for DictHash, back-patch on EndObject
USE WHEN: Writing .nxb output from C; call nxs_writer_reset() to reuse the allocated buffer across batches rather than free+init.

---

# nxs_writer.h

DOES: Public C99 header for NxsWriter: declares the nxs_writer_t struct (schema, growable buffer, frame stack, assembled output) and all writer lifecycle and typed-write function prototypes.
SYMBOLS:
- nxs_writer_init / nxs_writer_free / nxs_writer_reset / nxs_writer_begin_object / nxs_writer_end_object / nxs_writer_finish
- nxs_write_i64 / nxs_write_f64 / nxs_write_bool / nxs_write_time / nxs_write_null / nxs_write_str / nxs_write_bytes
TYPE: nxs_writer_t { keys[256], key_count, bitmask_bytes, buf, buf_len, buf_pos, record_offsets, record_count, record_cap, frames[8]{ start, bitmask, offset_table, slot_order, present_count, last_slot, needs_sort }, frame_depth, out, out_size }
DEPENDS: stddef.h, stdint.h
PATTERNS: C/C++ extern-C guard, pragma once, frame-stack nested-object model
USE WHEN: Including the NXS writer in any C or C++ translation unit; output is in w->out / w->out_size after nxs_writer_finish().

---

# test.c

DOES: Smoke-test suite for the C reader and writer: validates schema parsing, per-field reads against fixture data, error-path responses (bad magic, truncation, dict-hash corruption), and writer round-trip correctness for all field types including Unicode strings and multi-byte bitmasks.
SYMBOLS:
- main(int argc, char **argv) int
- read_file(path, out_size) uint8_t*
DEPENDS: nxs.h, nxs_writer.h, stdio.h, stdlib.h
PATTERNS: CHECK macro (pass/fail counter), fixture-driven assertions, writer round-trip via nxs_open on w->out
USE WHEN: Running `make test-c` or `cd c && make test -s && ./test ../js/fixtures`; fixtures must exist before running.
