# Workload B — Cold-open random access (methodology template)

**Status:** Draft — freeze before publication.

## Hypothesis

Nyxis ties FlatBuffers and Cap'n Proto on open and per-record access; may lead on very large files (>10M records) due to tail-index open decoupled from file size.

## Schema

Flat 8-field record — identical to `site/bench` fixtures and [BENCHMARK.md](../../BENCHMARK.md) baseline:
`id`, `username`, `email`, `age`, `balance`, `active`, `score`, `created_at`.

## Dataset sizes

1M, 10M, 100M records (`workload_B_<N>.json`).

## Primary metric

Time from bytes available in memory to first field of record 0 readable (P50, P99 ns).

## Secondary metrics

- Random record access (P50, P99)
- RSS after open, before field access
- Full-file scan of one field (all records)

## Harness clock semantics (must be frozen per run)

| Format | `open` | `access` | `scan` |
|--------|--------|----------|--------|
| NXS | New reader + record 0 + field read each sample | Warm reader; random record + one field | C `sum_f64` when extension present, else per-record `get_f64` |
| Protobuf | `ParseFromString` full file each sample | **Warm parsed message**; random `records[i].field` (no parse in timed region) | Iterate **warm** `records` list |
| Cap'n Proto | `from_bytes` + root each sample (or warm per harness version) | Warm message; random record access | Full-file walk |
| FlatBuffers | Buffer + root + field | Warm buffer; vtable field read | Full buffer walk |

**Interpretation:** Protobuf `access` and `scan` measure cost *after* the file is already deserialized into heap objects. NXS measures zero-copy field resolve from a mmap'd buffer. **Do not place Protobuf access/scan in the same column as NXS, FlatBuffers, or Cap'n Proto without a footnote** — they are different operations.

**NXS open/access at `< 1 µs` (C driver, 10k / ~1.3 MB):** reflects **warm page cache** after the harness has read the file — not cold-open from disk. Earlier dev runs reported ~25 µs NXS open on cold or larger files. For publication at 10k, note warm-cache conditions; cold-open at multi-GB scale is documented separately (~25 µs header + tail-index on this hardware).

## Publication tables (Option B — required)

Render **two blocks** (see `bench/scripts/render_tables.py`):

### 1. Zero-copy formats (primary)

Formats: **NXS, FlatBuffers, Cap'n Proto** only.

| Metric | Comparable? | Notes |
|--------|-------------|-------|
| `open` | Yes | Each format's native open-to-first-field path |
| `size` | Yes | On-disk bytes |
| `access` | Yes | Warm random record + one field (zero-copy resolve) |
| `scan` | Yes among peers | NXS uses C `sum_f64` when built; see Workload C for scan caveat |

Use this table for claims like “NXS warm access vs Cap'n Proto / FlatBuffers.”

### 2. Protobuf (post-parse reference)

Separate table or column group labeled **Protobuf (post-parse)** with this footnote (verbatim in published docs):

> Protobuf **access** and **scan** times reflect operations on a **pre-parsed Python object graph** (`ParseFromString` once per run; timed loop touches `records[i].field` only). Protobuf **open** times reflect **full re-parse per sample**. These numbers are **not comparable** to zero-copy access times for NXS, FlatBuffers, and Cap'n Proto. Include Protobuf here as a reference for apps that keep parsed messages in memory, not as a zero-copy peer.

**Do not** drop Protobuf entirely — that reads as excluding it because open is slow (420 µs vs ~25 µs NXS is a fair open comparison and should stay visible).

## NXS scan (Workload B flat-8)

**Publication path:** C harness, `driver=c`, metric `scan` → `nxs_sum_f64("score")`.

**What changed vs earlier dev tables (~8.9 ms):** The matrix previously reported Python harness scan (per-record `get_f64` or extension path). After `driver=c` dedupe, Workload B NXS scan reflects **C bulk scan**: tail-index walk + `scan_offset_bulk` per record (one flat field on uniform flat-8). On dev macOS 10k that lands ~tens of µs, not milliseconds.

**Cap'n Proto / FlatBuffers scan in this harness:** Python accessor loops (2–21 ms at 10k) — publish in a **separate scan-reference table**, not the primary zero-copy table. Flag as harness overhead, not format limits.

Sparse schemas (Workload C) still need uniform fast path (Go `SumF64Fast`) in C/Python; until then:

> NXS scan on sparse/wide schemas reflects per-record bitmask resolution. Route dense columnar analytics to Arrow.

## Cold-open protocol

1. Drop page cache on Linux: `sudo bench/scripts/drop_caches.sh`
2. `mmap` / read file into buffer
3. Measure through first successful field read on record 0
4. 100 warmup, 1000 samples, IQR-trimmed median
