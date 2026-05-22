# Workload E — PAX mixed access + column scan (Phase 2)

**Status:** Draft — freeze with Phase 2 benchmark runs.

## Purpose

Measure OLAP §7.2 **mixed workload**: 100 pseudo-random `get_f64("score")` record accesses plus one full-file `col_sum_f64("score")`, comparing **row**, **columnar**, and **PAX** layouts on the same dense numeric schema.

Success criterion (OLAP Phase 2): column scan on PAX faster than row-oriented scan; random access on PAX within **2×** row-oriented access time.

## Schema

Dense flat-8 numeric subset (same as `bench_columnar` / conformance `flat8_dense`):

| Field | Sigil |
|-------|-------|
| id | `=` |
| score | `~` |
| active | `?` |
| ts | `@` |

Strings are omitted (PAX/columnar v1.2a numeric-only).

## Layouts

| Layout | Emitter | Default page size |
|--------|---------|-------------------|
| Row | `NxsWriter::finish` | n/a |
| Columnar | `finish_columnar` | n/a |
| PAX | `finish_pax` | 4096 (override: 3rd CLI arg) |

## Metrics

| Metric | Definition | JSON field |
|--------|------------|------------|
| Random access | 100 × `record(i).get_f64("score")` after `Reader::open` | `access_us` (P50/P95/P99 µs) |
| Column scan | `col_sum_f64("score")` | `col_scan_us` |
| Mixed total | access + scan in one timed window | `mixed_total_us` |

Warmup: 20 iterations. Samples: 200 (trimmed percentiles in harness binary).

## Harness

```bash
# Default: 10k records, 100 random accesses, PAX page_size 4096
cd nyxis/rust && cargo run --release --bin bench_pax_mixed

# Or via bench Makefile
make -C bench run-e-mixed
make -C bench run-e-mixed BENCH_E_RECORDS=100000
```

Implementation: `nyxis/rust/src/bin/bench_pax_mixed.rs`

Output: single JSON object with `workload: "E"` and per-layout percentile blocks.

## Workload D variant — PAX streaming TTFR

See **§ PAX streaming TTFR** in [workload_D.md](workload_D.md). Summary:

- Row `nxs`: TTFR after **1** NYXO record (`--formats nxs`, `--ttfr-records 1`).
- PAX `nxs_pax`: TTFR after **first complete page** (`--formats nxs_pax`, `--page-size 256`).
- PAX uses `PaxStreamWriter` + incremental `complete_pax_page_end` polling.

```bash
make -C bench run-d-pax-ttfr
```

Honest expectation (OLAP §4.5): PAX TTFR ≫ row TTFR when `page_size=256` (~10 ms vs ~150 µs) because the first record is not readable until the page seals.
