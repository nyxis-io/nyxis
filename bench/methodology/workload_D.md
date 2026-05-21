# Workload D — Streaming ingest & time-to-first-record (methodology)

**Status:** Draft — freeze into `results/<date>_<host>/methodology.md` before publication runs.

## Purpose

Measure wall-clock time from the writer’s first record write to the reader returning the first complete record to the application. This is **not** bulk throughput (see Workload B); it is **time-to-first-record (TTFR)** for live ingest and reporting.

## Schema

Phase 1 reuses the **flat-8** schema from Workload B (`bench/schemas/flat8.nxs`) for comparability with existing fixtures and transcoders.

## Variants

| ID | Transport | Primary for publication |
|----|-----------|-------------------------|
| **D1** | POSIX FIFO (`mkfifo`) | Secondary (pipe buffer artifacts) |
| **D2** | File on disk + poll/`inotify` | **Primary** (matches reporting use case) |

## Streaming mechanism (explains TTFR ordering)

| Format | Mechanism | Native file-level streaming? |
|--------|-----------|------------------------------|
| NXS v1.1 | NYXO cell, self-delimiting (magic + length) | **Yes** |
| Cap'n Proto | Segment framing (fixed header per message) | **Yes** (framed stream) |
| Protobuf v3 | Varint length-prefix per record | **Yes** |
| FlatBuffers | Root offset table at buffer start | **No** |

Read-path cost at P50 maps to decode steps before the first field is accessible: NXS (one pass, no varint) < Cap'n Proto (segment table) < Protobuf (varint + field tags).

## Metrics

| Metric | Definition | Units |
|--------|------------|-------|
| **TTFR** (primary) | `reader_first_record` − `writer_first_write` | µs; P50, P95, P99; **publication:** `metric=ttfr`, n=1000, flush_every≥100 |
| **TTFR (smoke)** | 20 trials, flush_every=1 | `metric=ttfr_smoke` — not for publication tables |
| **Throughput (publication)** | Records/s from first complete record to last while writer still appending; **batched flush** (`flush_every≥100`) | rec/s; `metric=throughput` |
| **Throughput (smoke only)** | Same window but `flush_every=1` + poll overhead — **not publication** | rec/s; `metric=throughput_smoke` |
| **Seal latency** | `writer_seal_call` → durable footer (NXS only) | µs; P50 |
| **Reader RSS HWM** | Peak RSS during full stream read | MB |

**Publication:** `--runs 1000` (default). At n=200, P99 is the worst ~2 observations — label as unstable (harness emits `p99_note`).

## Dev results (macOS, D2 poll 50 µs, 10k dataset, flush every 100, n=200)

| Format | P50 | P95 | P99 | Seal P50 |
|--------|-----|-----|-----|----------|
| NXS | 71 µs | 195 µs | 546 µs | 4.0 ms |
| Cap'n Proto | 109 µs | 198 µs | **433 µs** | n/a |
| Protobuf | 113 µs | 328 µs | 853 µs | n/a |
| FlatBuffers | n/a † | n/a † | n/a † | n/a |

† FlatBuffers does not support native file-level streaming. The root offset table is written at buffer start; readers cannot access any record until the complete buffer is available. TTFR for FlatBuffers equals total file transfer time (~transfer_size / write_bandwidth). Streaming requires an external message-framing layer on top of the format; with external framing, per-message TTFR is expected to be comparable to Cap'n Proto framed streaming.

**Honest read:** NXS leads P50 and P95 on batched flush at n=1000. **P99 ordering depends on flush policy:** an earlier n=1000 **per-record flush** run had Cap'n Proto winning P99 (252 µs vs 321 µs); the publication **batched flush** run (`flush_every=100`) can show NXS ahead at P99 (e.g. 437 µs vs 583 µs). Do **not** claim “NXS wins P99” until Linux + `inotify` confirms which configuration is stable under push notification.

### Publication framing (draft)

> All three formats support sub-150 µs TTFR at P50 for file-on-disk streaming. NXS leads at P50 (~1.5× vs Cap'n Proto) due to simpler read-path framing: a NYXO cell is self-delimiting without varint decode. Cap'n Proto's segment framing produces nearly identical P95 results. Protobuf's varint length-prefix decode produces wider P99 tails under filesystem-notification jitter. NXS pays a seal cost (~4 ms at 10k records on dev macOS, fsync-dominated) that Cap'n Proto and Protobuf do not; in exchange, sealed NXS files support O(1) random access without a sequential scan.

### Seal (macOS dev, 10k records)

Seal breakdown (`make -C bench run-d-seal-profile`): tail-index **write** ~100 µs; **`sync_all`** ~3.9 ms (~97% of total). Synthetic `sync_all` on 100 KB–10 MB payloads: ~3.7–5.3 ms — seal cost is storage-bound, not linear ~400 ns/record.

## P99 investigation

Before claiming tail latency, run on Linux with `inotify` and `--runs 1000`. On macOS dev, sweep poll interval to separate driver spikes from poll misses:

```bash
bench-stream-d --runs 200 --formats nxs --poll-us 10   # ...
bench-stream-d --runs 200 --formats nxs --poll-us 1000
```

If P99 collapses as `poll_us` → 0, spikes are poll-artifact; if stable, report as driver/OS jitter.

## Harness

```bash
make -C bench run-d-smoke                    # 20 trials, fast
make -C bench run-d BENCH_RECORDS=10000      # 1000 trials, per-record + batched flush
make -C bench run-d-seal-profile BENCH_RECORDS_D=10000
```

Implementation: `bench/harness/stream_d/` — NXS, Protobuf, Cap'n Proto; FlatBuffers emits `n/a` with footnote. Default formats: `nxs,proto,capnp`. Add `fb` for FlatBuffers row: `--formats nxs,proto,capnp,fb`.

Requires `capnp` on PATH for build (`capnpc`).

## Throughput (informal, macOS)

Measured as records/s from first complete record until all N rows are visible while the writer appends (`bench-stream-d`, poll-based reader re-reads the file). **10k rows, flush every 100:** NXS ~25.7k · Protobuf ~26.6k · Cap'n Proto ~24.4k rec/s (dev run, May 2026). RSS not yet measured.

```bash
make -C bench run-d-throughput BENCH_RECORDS_D=10000
```

## Remaining before publication

1. Linux bare metal, D2 with `inotify`
2. 1M record run, 1000 TTFR trials (TTFR only; seal/throughput at 1M optional)
3. Reader RSS high-water mark
4. P99 on Linux (confirm Cap'n Proto vs NXS tail ordering)
