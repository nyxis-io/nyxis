# Workload F — Adaptive prefetch (methodology)

**Status:** Frozen for native remote-style runs using fetch recorder + optional per-fetch latency.

## Hypothesis

Viewport prefetch and adaptive strategy reduce remote range-fetch count and wall time for sequential scroll patterns (F1, F2) without beating lazy access on random patterns (F3). Bounded `max_pages` keeps client memory near the page-cache ceiling during long scrolls (F4).

## Fixture

- **Schema:** flat 8-field row layout: `id`, `username`, `email`, `age`, `balance`, `active`, `score`, `created_at` (same as Workload B / `site/bench`).
- **Publication size (v1.3 compact default):** 1M records → **~79 MiB** row `.nxb` (`site/bench/fixtures/records_1000000.nxb`, 83,007,958 bytes). Same flat-8 schema as Workload B.
- **Frozen native runs (2026-05):** used v1.2 row **`workload_B_nxs_1000000.nxb`** (~132 MiB). Timings in `bench/results/*/workload_f.jsonl` are valid; **results scale linearly with record count** — extrapolate to compact footprint without rerunning until prefetch engine constants change.
- **Spec targets (§12):** 100 MB (F1/F2) and 500 MB (F3).

## Remote I/O model

Harness: `bench/harness/prefetch/main.go` (Go driver with `WithFetchRange`).

1. Full file bytes live in memory as the **backing store** (simulates object storage / CDN origin).
2. Each prefetch-engine page read goes through `fetchRange`, which copies the slice and optionally sleeps `--latency-us` (default **100 µs** per fetch — SSD order of magnitude; **conservative vs browser RTT**).
3. Open parses preamble + schema + tail-index without traversing the data sector.
4. **Native Go driver note:** field reads resolve from the in-process backing store (analogous to warm OS page cache). `--latency-us` applies only to **`fetchRange` calls issued by the prefetch engine**, not to per-record decode.

**Prefetch fetches column:** counts `fetchRange` calls from the prefetch engine only. **0 in lazy mode** does not mean “no I/O” — the harness reads directly from the backing buffer. A file-on-disk or network reader would issue on-demand fetches in lazy mode.

JSON baselines are **not** used for F1–F4 (remote fetch scenarios).

## Scenarios

| ID | Metric | Procedure |
| --- | --- | --- |
| **F1** | Cold viewport warm (ms, P50) + fetches | New reader → read records 0..49 (lazy) vs `prefetch_viewport(0, end)` then read 0..49. |
| **F2** | Scroll throughput (s) + fetches | Viewport step 50 from 0 to n−1; prefetch mode calls `prefetch_viewport` each step with sequential hint. |
| **F3** | Random access (ms) | 1000 random indices (seed 0x4E595849); lazy vs random hint. |
| **F4** | Peak `MemStats.Sys` (MB) | Full scroll with `max_pages=64`. **Not** production RSS — includes Go runtime + full fixture backing store. Browser steady-state with `max_pages=64` ≈ **4 MB** regardless of file size. |

## Browser projection (publication)

Native harness wall time **understates** prefetch value when RTT ≫ fetch simulation. Publish a **Projected browser (20 ms RTT)** column alongside measured results:

| Scenario | Mode | Native result | Prefetch fetches | Projected browser |
| --- | --- | --- | ---: | --- |
| F1 | lazy | ~µs | 0 | ~40 ms (2 on-demand fetches) |
| F1 | prefetch_viewport | ~µs | 1 | ~20 ms (1 coalesced fetch) |
| F2 | lazy | ~88 ms | 0 | impractical (serial on-demand) |
| F2 | prefetch_adaptive | ~352 ms | 1952 | feasible (pipelined ahead) |

In-process simulation cannot capture pipelining; browser bench §2–3 (JS driver) is the production-relevant F1 path.

## Publication rules

- Freeze rows under `bench/results/<date>_<host>/raw/workload_f.jsonl`.
- Copy this file into the result directory as `methodology_F.md`.
- Update `BENCHMARK.md` Workload F with medians, footnotes, and browser projection.
- Re-run when prefetch engine or default cache constants change.

## Review checklist

- [x] Fetch recorder counts match `cache_stats().fetches_issued` on smoke fixture
- [x] F3 uses fixed RNG seed across modes
- [x] F4 documents Go `MemStats.Sys` vs production `max_pages` cache ceiling
- [x] Prefetch fetches footnote and browser projection column in published tables
