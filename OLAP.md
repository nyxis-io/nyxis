# NXS Columnar & PAX Layout — Technical Specification

**Status:** Draft v0.1  
**Scope:** Wire format extensions for columnar and PAX layouts; compiler flags; driver read paths  
**Depends on:** NXS SPEC.md v1.1, existing preamble reserved flags field  
**Non-goal:** Query language, server-side pivot logic, Arrow bridge changes (separate specs)

---

## 1. Background and motivation

NXS v1.1 is row-oriented. Every record is a self-contained NYXO object; the tail-index provides O(1) access to any record by index. This layout is optimal for:

- Random record access by index or key
- Streaming ingest with progressive render
- Sparse records with variable field population
- Virtual scroll over large datasets

It is suboptimal for:

- Full-column scans (sum, min, max, count-distinct over one field)
- Chart and graph data (all values of `revenue` across 1M records)
- OLAP-style aggregations

The Workload C benchmark result quantifies this: Arrow wins columnar scan by ~2800× over NXS row-oriented scan. The gap is architectural, not a driver bug.

This spec defines two new layouts that close the gap without abandoning the properties that make NXS useful for the row-oriented cases:

- **Columnar layout** (`FLAG_COLUMNAR`): data sector is field buffers. Optimal for analytics. Same use case as Arrow IPC.
- **PAX layout** (`FLAG_PAX`): data sector is fixed-size pages, each page is columnar internally. Optimal for mixed workloads — streaming + analytics in the same file.

A third section defines the **compiler hint** mechanism that lets writers choose the layout at write time.

---

## 2. Preamble flag assignments

The existing 32-byte preamble has a flags byte at offset 8 (currently documented as reserved). This spec assigns two bits:

```
Bit 0 (0x01): FLAG_COLUMNAR  — data sector uses columnar layout
Bit 1 (0x02): FLAG_PAX       — data sector uses PAX layout
```

Rules:

- `FLAG_COLUMNAR` and `FLAG_PAX` are mutually exclusive. A file with both bits set MUST be rejected with `ERR_INVALID_FLAGS`.
- A file with neither bit set uses the existing row-oriented layout. All v1.1 readers remain valid.
- A v1.1 reader encountering `FLAG_COLUMNAR` or `FLAG_PAX` MUST return `ERR_UNSUPPORTED_FLAG` rather than attempting to read the data sector. This is already the specified behavior for unknown flag bits.
- `FLAG_COLUMNAR` and `FLAG_PAX` are only valid in combination with `FLAG_SCHEMA_EMBEDDED` (the schema header must be present to define field types and counts for column buffer sizing).

---

## 3. Columnar layout (`FLAG_COLUMNAR`)

### 3.1 Overview

The data sector is replaced by a **column group**: one contiguous buffer per field, in field-declaration order from the schema header. A separate **null bitmap** per field tracks which records have that field populated.

The tail-index changes semantics: instead of `(KeyID u16, RecordOffset u64)` pairs, it stores `(FieldID u16, BufferOffset u64, BufferLength u64)` triples — one per field. This allows readers to locate any field's buffer in O(1) without scanning the data sector.

### 3.2 Data sector layout

```
[Column Group]
  [Field 0 null bitmap]    — ceil(N / 8) bytes, 1 bit per record, LSB first
  [Field 0 value buffer]   — N × cell_size bytes (only populated records)
  [Field 1 null bitmap]
  [Field 1 value buffer]
  ...
  [Field K-1 null bitmap]
  [Field K-1 value buffer]
```

**Null bitmap:** One bit per record position. Bit `i` is 1 if record `i` has this field populated, 0 if absent. Length is `ceil(record_count / 8)` bytes, rounded up to the nearest 8 bytes for alignment.

**Value buffer (fixed-width types):** `N × cell_size` contiguous cells, 8-byte aligned per cell, where `N` is the total record count and `cell_size` is 8 bytes for all current atomic types (i64, f64, bool, timestamp). Records where the null bitmap bit is 0 still occupy a cell position (zero-filled) to preserve index arithmetic: `value_for_record_i = buffer[i × cell_size]`. This is the dense layout.

**Value buffer (variable-length types: string, binary):** Two sub-buffers:
- **Offsets buffer:** `(N + 1) × 4` bytes, u32 offsets into the values buffer. `offsets[i]` is the start of record `i`'s value; `offsets[i+1]` is the end. Records where the null bitmap bit is 0 have `offsets[i] == offsets[i+1]` (zero-length sentinel).
- **Values buffer:** Concatenated UTF-8 bytes (string) or raw bytes (binary), no padding between values.

This is identical to Arrow's variable-length layout and is intentional — columnar `.nxb` files can be projected into Arrow record batches with metadata translation only, no data copy.

### 3.3 Tail-index (columnar)

```
[Tail-Index Entry × K fields]
  FieldID:       u16
  _padding:      u16 (reserved, write zero, ignore on read)
  BufferOffset:  u64   — absolute byte offset to field's null bitmap start
  BufferLength:  u64   — total bytes for null bitmap + value buffer(s)

[Footer]
  TailIndexOffset: u64  — absolute offset to first tail-index entry
  RecordCount:     u64  — total number of records (needed for null bitmap sizing)
  MagicFooter:     4 bytes (NXS!)
```

Footer is extended by 8 bytes (RecordCount) vs the row-oriented footer. Readers locate the tail-index by reading the last 20 bytes (vs 12 bytes in v1.1). This is a breaking change for the footer layout — columnar files use `FLAG_COLUMNAR` to signal the extended footer.

### 3.4 Record count

Row-oriented files derive record count from the tail-index entry count. Columnar files store it explicitly in the footer because the tail-index entry count is the field count, not the record count.

### 3.5 Read operations

**Column scan** (`nxs_col_sum_f64`, `nxs_col_min_f64`, etc.):
1. Read footer, locate tail-index
2. Find entry for target field via FieldID lookup (O(K) where K is field count, typically ≤ 50)
3. Load null bitmap for the field
4. Load value buffer for the field
5. Apply reducer over value buffer, skip positions where null bitmap bit is 0

For dense fields (null bitmap all-ones), the null bitmap check can be skipped and the reducer runs directly over the contiguous value buffer — same memory access pattern as Arrow.

**Random record access** (`nxs_col_get_record`):
1. For each requested field, locate its buffer via tail-index
2. Check null bitmap at position `record_index`
3. Read `buffer[record_index × cell_size]` for fixed-width, or `buffer[offsets[record_index]:offsets[record_index+1]]` for variable-length

This is O(number of requested fields) with non-contiguous memory access — worse than row-oriented for multi-field record reads. Columnar layout trades random-record access performance for column scan performance. This tradeoff is explicit and documented.

### 3.6 Streamable columnar

Columnar layout is not natively streamable in the same way as row-oriented v1.1. A writer cannot emit a complete column until all records for that field are known. Two options for streaming writers:

**Option A: Buffer-then-emit.** Writer accumulates all records in memory, then emits columns in one pass. Simple but requires full dataset in writer memory.

**Option B: Segment-based streaming.** Writer emits fixed-size segments, each a complete columnar block (see PAX layout, section 4). This is the recommended path for streaming columnar data.

Columnar `.nxb` files therefore do not support `TailPtr = 0` streaming sealing. Attempting to set `TailPtr = 0` with `FLAG_COLUMNAR` MUST be rejected by the compiler with `ERR_INCOMPATIBLE_FLAGS`.

---

## 4. PAX layout (`FLAG_PAX`)

### 4.1 Overview

PAX (Partition Attributes Across) divides the data sector into fixed-size **pages**. Each page is a mini columnar block covering a contiguous range of records. Across pages the file is row-partitioned (each page covers records `[page_start, page_end)`); within a page the layout is columnar (each field has its own buffer).

This gives approximately:

- **80% of columnar scan performance**: field values for any given page are contiguous, so the CPU prefetcher works within a page. Cross-page scans require one seek per page but each seek lands on a contiguous buffer.
- **80% of row-oriented access performance**: accessing all fields of record `i` requires at most one page read (the page containing record `i`), then one buffer seek per field within that page.
- **Full streaming support**: pages are complete units that can be emitted and sealed independently. The writer emits pages as they fill; the reader can consume complete pages before the file is sealed.

PAX is the recommended layout for mixed workloads: reporting tools, dashboards, and any case where the same file needs to support both virtual scroll (row access) and chart rendering (column scan).

### 4.2 Page structure

```
[Page Header]
  PageMagic:      4 bytes (NXSP)
  PageIndex:      u32     — zero-based page number
  RecordStart:    u64     — index of first record in this page
  RecordCount:    u32     — number of records in this page (≤ PAGE_SIZE)
  FieldCount:     u16     — number of fields (must match schema)
  Flags:          u16     — reserved, write zero

[Column Group — same layout as columnar §3.2]
  [Field 0 null bitmap]
  [Field 0 value buffer]
  ...
  [Field K-1 null bitmap]
  [Field K-1 value buffer]

[Page Footer]
  PageLength:     u32     — total bytes from PageMagic to PageLength inclusive
  PageCRC:        u32     — CRC32 of page bytes excluding PageCRC field (optional, enabled by FLAG_PAGE_CRC in preamble flags)
```

Pages are 8-byte aligned. The writer pads the final byte of each page to the next 8-byte boundary before writing the next page header.

### 4.3 Page size

Default page size: **4096 records**. Rationale: at 8 bytes per cell × 8 fields × 4096 records = 262 KB per page, which fits in L2/L3 cache on most modern CPUs. Writers MAY use a different page size; the page size is not stored in the preamble (each page stores its own `RecordCount`). Readers derive page size from the tail-index.

The compiler accepts a `--page-size N` hint. Recommended values:

| Use case | Page size |
|---|---|
| Reporting / virtual scroll | 1024–4096 |
| Analytics / large scans | 8192–65536 |
| Streaming / low latency | 256–1024 |

### 4.4 Tail-index (PAX)

```
[Tail-Index Entry × P pages]
  PageIndex:      u32
  RecordStart:    u64   — first record index in this page
  RecordCount:    u32   — records in this page
  PageOffset:     u64   — absolute byte offset to PageMagic
  PageLength:     u32   — total page bytes

[Footer]
  TailIndexOffset: u64
  RecordCount:     u64  — total records across all pages
  PageCount:       u32
  PageSize:        u32  — nominal page size (informational)
  MagicFooter:     4 bytes (NXS!)
```

Footer is 28 bytes for PAX (vs 12 bytes for row-oriented, vs 20 bytes for columnar).

**Record lookup by index:**
1. Binary search the tail-index for the page containing `record_index` (compare against `RecordStart` + `RecordCount`)
2. Seek to `PageOffset`
3. Within the page, seek to the column buffer for the target field
4. Read at position `(record_index - RecordStart) × cell_size`

This is O(log P) where P is the number of pages — typically 2–4 binary search steps for million-record files with default page size.

**Column scan:**
1. Iterate tail-index entries in order
2. For each page, seek to `PageOffset` + field buffer offset within page
3. Apply reducer over the contiguous field buffer
4. Accumulate result

Within each page, the field buffer is contiguous and cache-friendly. Cross-page seeks are sequential if the tail-index is iterated in order.

### 4.5 Streaming PAX

PAX supports streaming via page-level sealing. The protocol:

**Writer:**
1. Open file with `FLAG_PAX` and `TailPtr = 0` in preamble
2. Accumulate records until page is full (`RecordCount == PAGE_SIZE`) or a flush is triggered
3. Emit the complete page (header + column group + page footer)
4. Repeat from step 2
5. On stream close: emit final partial page (if any records pending), then write tail-index and footer, seal with `TailPtr` and `MagicFooter`

**Reader:**
1. Open file, detect `FLAG_PAX` and `TailPtr = 0` (unsealed)
2. Poll for complete pages by detecting `NXSP` magic at expected offsets
3. A page is complete when `PageLength` bytes are available from `NXSP`
4. Process complete pages as they arrive
5. On detecting `MagicFooter`: resolve tail-index, switch to random-access mode

This enables the reporting use case: server streams PAX pages as the report generates, browser renders each page's rows as it arrives (row access within page), and chart components accumulate column values across pages. When the stream seals, the full tail-index enables cross-page aggregation.

**TTFR for PAX streaming:**

Time-to-first-record = time to emit first complete page. With `PAGE_SIZE = 256` records and the flat-8 schema at ~130 bytes/record, one page is ~33 KB. At 26k rec/s sustained throughput, first page completes in ~10 ms. This is higher than row-oriented TTFR (142 µs) because PAX requires a complete page before the first record is accessible.

This tradeoff is explicit: PAX trades streaming latency for analytical performance. For minimum TTFR use row-oriented layout; for minimum scan latency use columnar layout; for mixed workloads use PAX.

---

## 5. Compiler hints

### 5.1 CLI flags

```bash
nxs compile input.nxs                        # row-oriented (default, v1.1)
nxs compile --layout columnar input.nxs      # FLAG_COLUMNAR
nxs compile --layout pax input.nxs           # FLAG_PAX, default page size 4096
nxs compile --layout pax --page-size 1024 input.nxs
```

### 5.2 Inline pragma

A layout pragma in the `.nxs` source overrides the CLI flag (CLI flag overrides the default; pragma overrides both):

```nxs
@layout columnar

records {
    id:        =1
    score:     ~0.91
    region:    $"us-east"
}
```

```nxs
@layout pax
@page-size 1024

records {
    ...
}
```

Valid values for `@layout`: `row` (default), `columnar`, `pax`.

### 5.3 Server-side runtime selection

For the server-side pivot use case (Brian's scenario), the compiler is not invoked at write time. Instead the driver accepts a layout hint at writer-open time:

```c
// C driver
nxs_writer_t *w = nxs_writer_open(path, schema, NXS_LAYOUT_PAX, .page_size = 1024);

// Go driver
w, err := nxs.OpenWriter(path, schema, nxs.WithLayout(nxs.LayoutPAX), nxs.WithPageSize(1024))
```

The server inspects the incoming query, selects the layout, opens the writer with the appropriate hint, and streams records. The client reads the preamble flag and uses the appropriate read path. No client-side configuration required.

### 5.4 Layout selection guide

Embed this in the compiler documentation:

| Use case | Recommended layout | Rationale |
|---|---|---|
| Virtual scroll, record viewer | `row` | O(1) record access, lowest TTFR |
| OLAP, charts, aggregations | `columnar` | Contiguous field buffers, Arrow-compatible |
| Reporting (scroll + charts) | `pax` | Both access patterns, streaming support |
| Streaming ingest, log tailing | `row` | Lowest TTFR, native v1.1 streaming |
| Audit log, append-only ledger | `row` | WAL mode, crash-resilient sealing |
| Large export, offline analysis | `columnar` | Minimum scan time, Arrow bridge |

---

## 6. Driver changes

### 6.1 Read path

Every driver MUST check `FLAG_COLUMNAR` and `FLAG_PAX` at file open and dispatch to the appropriate read path. The public API is unchanged — `nxs_get_f64(record, "score")` works on all three layouts. The driver internally routes to:

- Row path: existing NYXO + tail-index logic
- Columnar path: field buffer lookup via columnar tail-index
- PAX path: page lookup via PAX tail-index → within-page field buffer

### 6.2 New public API additions

```c
// Column scan (all layouts — row falls back to per-record sum)
double nxs_col_sum_f64(nxs_reader_t *r, const char *field);
double nxs_col_min_f64(nxs_reader_t *r, const char *field);
double nxs_col_max_f64(nxs_reader_t *r, const char *field);
int64_t nxs_col_sum_i64(nxs_reader_t *r, const char *field);

// Column buffer direct access (columnar and PAX only)
// Returns pointer to the raw value buffer for zero-copy chart rendering
const void* nxs_col_buffer(nxs_reader_t *r, const char *field, size_t *out_len);
const uint8_t* nxs_col_null_bitmap(nxs_reader_t *r, const char *field, size_t *out_len);

// PAX page iteration
nxs_page_t* nxs_page_first(nxs_reader_t *r);
nxs_page_t* nxs_page_next(nxs_page_t *page);
const void* nxs_page_col_buffer(nxs_page_t *page, const char *field, size_t *out_len);

// Writer layout hints
nxs_writer_t* nxs_writer_open_pax(const char *path, const nxs_schema_t *schema, uint32_t page_size);
nxs_writer_t* nxs_writer_open_columnar(const char *path, const nxs_schema_t *schema);
```

### 6.3 Arrow projection (columnar layout)

For `FLAG_COLUMNAR` files, drivers SHOULD provide a zero-copy Arrow projection path:

```c
// Projects columnar .nxb field buffers into Arrow C Data Interface structures
// No data copy — ArrowArray buffers point into the mmap'd .nxb data sector
int nxs_arrow_project(nxs_reader_t *r, ArrowSchema *out_schema, ArrowArray *out_array);
```

This makes the open-source Arrow bridge trivial: columnar `.nxb` is already Arrow-compatible by construction. The enterprise bridge for row-oriented files (transpose + copy) remains the paid offering.

---

## 7. Conformance

### 7.1 New error codes

| Code | Value | Meaning |
|---|---|---|
| `ERR_INVALID_FLAGS` | 0x10 | Both FLAG_COLUMNAR and FLAG_PAX set |
| `ERR_INCOMPATIBLE_FLAGS` | 0x11 | FLAG_COLUMNAR with TailPtr=0 (streaming not supported for columnar) |
| `ERR_UNSUPPORTED_LAYOUT` | 0x12 | Reader does not implement the requested layout |
| `ERR_INVALID_PAGE_MAGIC` | 0x13 | Expected NXSP at page boundary, found other bytes |
| `ERR_PAGE_CRC_MISMATCH` | 0x14 | Page CRC32 does not match computed value |

### 7.2 Conformance vectors

Add to `conformance/` directory:

```
columnar/
  flat8_dense_100.nxb          # 100 records, all fields populated, columnar
  flat8_sparse_10pct_100.nxb   # 100 records, 10% population, columnar
  flat8_strings_100.nxb        # 100 records with string fields, columnar
  invalid_flags_both.nxb       # FLAG_COLUMNAR + FLAG_PAX — must reject
  invalid_streaming.nxb        # FLAG_COLUMNAR + TailPtr=0 — must reject

pax/
  flat8_dense_p256_1000.nxb    # 1000 records, page_size=256, dense
  flat8_sparse_10pct_p256.nxb  # 1000 records, 10% population, page_size=256
  pax_streaming_unsealed.nxb   # Unsealed PAX (TailPtr=0, 3 pages, no footer) — batch open → ERR_BAD_MAGIC
  pax_invalid_page_magic.nxb   # Corrupt page boundary — must return ERR_INVALID_PAGE_MAGIC
```

All existing row-oriented conformance vectors remain valid and unchanged.

### 7.3 Performance conformance

Per the earlier recommendation, add performance gates to CI:

```
# Columnar: field scan over 1M records must complete within 2× Arrow IPC time
# on reference Linux hardware
bench_columnar_scan: nxs_col_sum_f64 on dense float field, 1M records, ≤ 2× arrow_scan_time

# PAX: mixed workload (100 random record accesses + 1 full column scan)
# must complete faster than pure row-oriented for the column scan component
bench_pax_mixed: record_access_time ≤ 2× row_access_time AND col_scan_time ≤ 4× columnar_scan_time
```

---

## 8. Open questions before implementation

**Q1: Dense vs sparse value buffer for columnar**

The spec above uses a **dense** value buffer (every record position has a cell, null bitmap indicates absence). This matches Arrow's layout and enables zero-copy Arrow projection. The alternative is a **sparse** value buffer (only populated records have cells, null bitmap gives rank for position lookup). Sparse saves memory for low-population fields but breaks Arrow compatibility and adds rank computation to every access.

Recommendation: dense for the initial implementation. Sparse columnar is a v1.3 optimization if memory pressure on low-population columnar files becomes a measured problem.

**Q2: Page size flexibility**

The spec allows variable page sizes within a file (each page stores its own `RecordCount`). This adds flexibility (final page is always partial) but complicates the tail-index binary search. Alternative: fix page size per file in the preamble and pad the final page to `PAGE_SIZE` with null records. Simpler implementation, slightly wasteful on the final page.

Recommendation: variable page size (final page is partial) with `PAGE_SIZE` stored in the PAX footer as informational. Binary search uses `RecordStart` + `RecordCount` per entry, not a fixed stride.

**Q3: String support in initial release**

Variable-length strings in columnar layout require the offsets+values sub-buffer layout. This adds implementation complexity and breaks the "columnar `.nxb` == Arrow IPC by construction" claim slightly (Arrow uses 64-bit offsets; this spec uses 32-bit). Options:

- **v1.2a:** Numeric fields only. Strings in columnar layout return `ERR_UNSUPPORTED_FIELD_TYPE`. Implement string support in v1.3.
- **v1.2b:** Full string support with 32-bit offsets. Document the Arrow offset-width difference; bridge code handles the conversion.
- **v1.2c:** Full string support with 64-bit offsets (Arrow-compatible). 8 bytes per record per string field regardless of string length — expensive for short strings.

Recommendation: **v1.2a** for initial release. Numerics cover the charts/aggregations use case. String support in a follow-up once the numeric path is benchmarked and stable.

**Q4: Keyword (`$`) fields in columnar layout**

Keywords are dictionary-encoded at the schema level (2-byte index per record). In columnar layout, a keyword field's value buffer is `N × 2` bytes of u16 dictionary indices. This is not Arrow-compatible (Arrow uses dictionary-encoded arrays with a separate dictionary batch). For the initial implementation, keyword fields in columnar layout are stored as u16 index arrays; Arrow projection for keyword fields requires a dictionary batch synthesis step.

Recommendation: document the limitation, implement keyword columnar as u16 arrays, handle Arrow projection separately.

**Q5: PAX page CRC**

The spec includes an optional `FLAG_PAGE_CRC` that adds a 4-byte CRC32 per page. This is useful for the crash-resilient WAL use case (detect corrupt pages on recovery) but adds 4 bytes per page and CRC computation on write. Given the existing crash-WAL spec already relies on `MagicFooter` detection, page CRC may be redundant for most use cases.

Recommendation: include `FLAG_PAGE_CRC` as an optional flag, disabled by default. Enable for audit log and financial ledger use cases where per-page integrity verification is worth the overhead.

---

## 9. Implementation phases

### Phase 1: Columnar numeric (v1.2) — **done**

- [x] Preamble flag assignment (`FLAG_COLUMNAR`, `FLAG_PAX`)
- [x] Columnar layout spec for numeric fields (i64, f64, bool, timestamp)
- [x] Compiler `--layout columnar` flag and `@layout` pragma
- [x] C driver read path: `nxs_col_sum_f64`, `nxs_col_min_f64`, `nxs_col_max_f64`
- [x] C driver `nxs_col_buffer` for zero-copy chart data access
- [x] Conformance vectors: `columnar_flat8_dense_100`, `columnar_flat8_sparse_10pct_100`
- [x] Benchmark: Workload C re-run with `FLAG_COLUMNAR` vs Arrow IPC (at parity on Apple Silicon)

**Success criterion:** Workload C columnar `.nxb` scan within 2× Arrow IPC on the same data. **Met** (107 µs vs 104 µs P50 at 1M records, macOS dev).

### Phase 2: PAX layout — **in progress**

- [x] PAX page structure and page-level streaming protocol (SPEC.md §4.5, OLAP.md §4.5)
- [x] Compiler `--layout pax` and `--page-size` flags
- [x] C driver PAX read path: page iteration, within-page field buffers (sealed files)
- [x] Conformance vectors: `pax_flat8_dense_p256_1000`, `pax_flat8_sparse_10pct_p256`, `pax_streaming_unsealed`, `pax_invalid_page_magic`
- [ ] PAX streaming writer: page-level flush, seal on close (reference writer batch-seals today)
- [ ] Workload D variant: PAX streaming TTFR vs row-oriented TTFR
- [ ] Benchmark: Workload E mixed workload (random access + column scan) on PAX vs row vs columnar

**Success criterion:** PAX mixed workload (100 random accesses + 1 full column scan) outperforms row-oriented on the column scan component and stays within 2× row-oriented on the random access component. **Pending** Workload E publication (see BENCHMARK.md §Workload E).

### Phase 3: String support in columnar (~2 weeks after Phase 2)

- Offsets + values sub-buffer for string and binary fields in columnar and PAX
- Arrow projection: handle offset-width difference for string fields
- Conformance vectors: `columnar/flat8_strings`
- Update BENCHMARK.md with string-inclusive columnar results

### Phase 4: Driver ports (~ongoing, parallel to Phases 2–3)

- Go driver: columnar and PAX read paths (Go already has `SumF64Fast` — columnar should be straightforward)
- Python C extension: expose `nxs_col_buffer` as numpy-compatible array
- JavaScript WASM: `colBuffer()` returning `Float64Array` / `BigInt64Array` for direct chart library consumption
- Rust: columnar and PAX read paths

---

## 10. Definition of done (v1.2)

The columnar and PAX layouts ship when:

1. `FLAG_COLUMNAR` and `FLAG_PAX` are assigned in SPEC.md with full binary layout documentation
2. C driver implements columnar and PAX read paths with all error codes
3. Compiler emits correct `FLAG_COLUMNAR` and `FLAG_PAX` files for numeric schemas
4. All conformance vectors pass in C driver
5. Workload C re-run shows columnar `.nxb` scan within **2× Arrow IPC** on dense numeric data, using the open-core AVX2/NEON dense fast path in `col_reduce` (runtime CPU feature detection). ARM NEON tuning and AVX-512 multi-accumulator paths remain under `nyxis-simd-guard`.
6. PAX mixed workload benchmark published with honest results including the TTFR tradeoff vs row-oriented
7. `nxs_col_buffer` returns a pointer suitable for direct use as chart library input (verified with one real chart integration — Chart.js or Recharts in the browser demo)
8. String fields return `ERR_UNSUPPORTED_FIELD_TYPE` in columnar/PAX with a clear error message pointing to the roadmap
9. BENCHMARK.md updated with columnar and PAX workload results
10. Use-cases page updated with the layout selection guide from §5.4

---

**End of spec.**

The single most important criterion is item 5 in the definition of done. If columnar `.nxb` doesn't come within 2× Arrow on dense numeric scan, the spec needs to be revised before shipping. Everything else — PAX, streaming, string support, driver ports — is downstream of that validation. Run Phase 1, measure against Arrow, then decide whether Phase 2 is worth starting.
