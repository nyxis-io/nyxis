---
room: implementations/c
source_paths: [c/]
file_count: 4
architectural_health: normal
security_tier: normal
hot_paths: [nxs.c, nxs.h]
see_also: [implementations/rust.md, spec/format.md]
---

# bench.c

DOES: Benchmarks NXS against JSON raw-byte scan and CSV column scan on 1M records, measuring sum_f64, sum_i64, and random-access latency with best-of-5 timing.
SYMBOLS:
- read_file(const char *path, size_t *out_size) -> uint8_t*
- elapsed_ms(struct timespec *a, struct timespec *b) -> double
- json_sum_score(const char *buf, size_t len) -> double
- csv_sum_score(const char *buf, size_t len) -> double
- run_best(struct timespec *t0, struct timespec *t1, void (*fn)(void*), void *ctx) -> double
- Types: json_ctx_t, csv_ctx_t, nxs_f64_ctx_t, nxs_i64_ctx_t, nxs_rand_ctx_t
DEPENDS: nxs.h
PATTERNS: benchmark-harness, manual-json-scan
USE WHEN: Comparing NXS bulk-scan performance against JSON/CSV baselines.

---

# nxs.c

DOES: Full C99 NXS reader implementation: preamble validation, schema extraction, bitmask-based field location, typed accessors, and allocation-free bulk reducers (sum/min/max).
SYMBOLS:
- nxs_open(nxs_reader_t *r, const uint8_t *data, size_t size) -> nxs_err_t
- nxs_close(nxs_reader_t *r) -> void
- nxs_record_count(const nxs_reader_t *r) -> uint32_t
- nxs_slot(const nxs_reader_t *r, const char *key) -> int
- nxs_record(const nxs_reader_t *r, uint32_t i, nxs_object_t *obj) -> nxs_err_t
- nxs_resolve_slot(nxs_object_t *obj, int slot) -> int64_t
- nxs_get_i64(nxs_object_t *obj, const char *key, int64_t *out) -> nxs_err_t
- nxs_get_f64(nxs_object_t *obj, const char *key, double *out) -> nxs_err_t
- nxs_get_bool(nxs_object_t *obj, const char *key, int *out) -> nxs_err_t
- nxs_get_str(nxs_object_t *obj, const char *key, char *buf, size_t buf_len) -> nxs_err_t
- nxs_get_i64_slot, nxs_get_f64_slot, nxs_get_bool_slot, nxs_get_str_slot (slot variants)
- scan_offset_bulk(const uint8_t *data, size_t obj_off, int slot) -> int64_t
- nxs_sum_f64(const nxs_reader_t *r, const char *key) -> double
- nxs_sum_i64(const nxs_reader_t *r, const char *key) -> int64_t
- nxs_min_f64(const nxs_reader_t *r, const char *key, double *out) -> nxs_err_t
- nxs_max_f64(const nxs_reader_t *r, const char *key, double *out) -> nxs_err_t
DEPENDS: nxs.h
PATTERNS: memcpy-endian-reads, bitmask-presence-encoding, offset-table-indirection, bulk-scan-loop
USE WHEN: The implementation of every exported API; read alongside nxs.h for the complete picture.

---

# nxs.h

DOES: Public API header for the C99 NXS reader — error codes, reader and object structs, and all function declarations. Include this and link nxs.c.
SYMBOLS:
- nxs_err_t (enum: NXS_OK, NXS_ERR_BAD_MAGIC, NXS_ERR_OUT_OF_BOUNDS, NXS_ERR_KEY_NOT_FOUND, NXS_ERR_FIELD_ABSENT, NXS_ERR_ALLOC)
- nxs_open, nxs_close, nxs_record_count, nxs_slot, nxs_record, nxs_resolve_slot
- nxs_get_i64, nxs_get_f64, nxs_get_bool, nxs_get_str
- nxs_get_i64_slot, nxs_get_f64_slot, nxs_get_bool_slot, nxs_get_str_slot
- nxs_sum_f64, nxs_sum_i64, nxs_min_f64, nxs_max_f64
- Types: nxs_err_t, nxs_reader_t, nxs_object_t
TYPE: nxs_reader_t { data, size, version, flags, dict_hash, tail_ptr, key_count, keys[NXS_MAX_KEYS], key_sigils[NXS_MAX_KEYS], record_count, tail_start, _pool[NXS_MAX_KEYS*64] }
TYPE: nxs_object_t { reader, offset, bitmask_start, offset_table_start, staged }
DEPENDS: none
PATTERNS: pragma-once, c-extern-guard
USE WHEN: Integrating the NXS reader into any C or C++ project; start here for the full API surface.

---

# test.c

DOES: Smoke test suite validating schema loading, typed accessors (i64, f64, bool, str), out-of-bounds error handling, and bulk reducers (sum, min, max) against the 1000-record fixture.
SYMBOLS:
- read_file(const char *path, size_t *out_size) -> uint8_t*
- CHECK(name, expr) macro
DEPENDS: nxs.h
PATTERNS: fixture-based-testing, smoke-test-harness
USE WHEN: Verifying reader correctness; build with `make test && ./test ../js/fixtures`.
