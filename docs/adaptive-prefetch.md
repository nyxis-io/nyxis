# Adaptive prefetch (Accepted)

**Status:** Accepted (2026-05-24)  
**Normative spec:** [`Adaptive-prefetch-spec.md`](../../Adaptive-prefetch-spec.md) v0.1 (workspace root)  
**Contract:** [`context/data/2026-05-23-adaptive-prefetch-spec.yaml`](../../context/data/2026-05-23-adaptive-prefetch-spec.yaml) (local PARA)  
**Conformance:** [`conformance/prefetch/`](../conformance/prefetch/README.md)

## Summary

Tail-index-driven **viewport prefetch** with a bounded LRU page cache, optional **adaptive strategy** and lifecycle controls, and a **columnar fast path** (`prefetch_column`) separate from the row page cache.

## Shipped (phases 1–4)

| Phase | Deliverable |
| --- | --- |
| 1 | `prefetch_viewport`, page cache, coalescing, dedup — all MUST drivers |
| 2 | Pattern detector, lazy/adaptive/eager strategy |
| 3 | `pause_prefetch` / `resume_prefetch`, memory pressure, `prefetch_cancel` conformance |
| 4 | `prefetch_column` (§7.4), `prefetch_columnar_fast_path`, Workload F in `BENCHMARK.md`, bench §16 (F0) |

## Operator entry points

- Row layout: `prefetch_viewport(start, end)` — see [`GETTING_STARTED.md`](../GETTING_STARTED.md#adaptive-prefetch)
- Columnar: `prefetch_column(field)` — one range fetch per column buffer
- Diagnostics: `cache_stats()` / `cacheStats()` (`column_fetches_issued` where applicable)

## Benchmarks

- **F0** — columnar aggregate vs JSON (`site/bench/` chart §16)
- **F1 (browser)** — `prefetch_viewport(0, 49)` on cold read / full scan bars (charts §2–3, §6, warm §8)
- **F1–F4 (native)** — Go fetch-recorder harness; frozen 1M numbers in `BENCHMARK.md` with **prefetch fetch counts** and **projected browser (20 ms RTT)** column. In-memory harness: lazy wins wall time; production win is browser RTT + coalescing (see `bench/methodology/workload_F.md`).

## Driver sign-off

§9.1 matrix and maintainer attestation: [`docs/adaptive-prefetch-driver-matrix.md`](adaptive-prefetch-driver-matrix.md).

## Definition of done (§14)

See spec §14. Optional follow-up: 500 MB F3 fixture; header-only open for native RSS isolation in F4.
