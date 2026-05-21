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

## Cold-open protocol

1. Drop page cache on Linux: `sudo bench/scripts/drop_caches.sh`
2. `mmap` / read file into buffer
3. Measure through first successful field read on record 0
4. 100 warmup, 1000 samples, IQR-trimmed median
