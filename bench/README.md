# Nyxis head-to-head benchmark suite

Reproducible benchmarks against Protobuf, FlatBuffers, Cap'n Proto, and Apache Arrow IPC.
See [BENCHMARK_SUITE.md](../BENCHMARK_SUITE.md) for methodology, review gates, and publication rules.

**Note:** Browser fixtures and the Node harness live in [`site/bench/`](../site/bench/) (JSON/NXS/CSV SDK benches). This directory is the **cross-format** suite from the spec (workloads A/B/C).

## Principles

1. Honest results — publish losses and ties, not sweeps.
2. One canonical JSON generator; all binary formats transcoded from the same source.
3. Frozen methodology per run under `results/YYYY-MM-DD_<hardware>/methodology.md`.
4. Uniform CLI across formats: `--workload`, `--format`, `--metric`, `--records`, `--population`.

## Layout

```
bench/
  schemas/           # .proto, .fbs, .capnp, .nxs per workload
  generators/        # gen.py + transcode (Rust NXB, Python other formats)
  harness/{c,go,rust}/
  data/json/         # canonical datasets (gitignored at 1M+)
  data/bin/          # transcoded binaries
  results/           # per-run raw + summary.json
  methodology/       # workload templates (freeze into results/ on run)
  scripts/           # run_all.sh, drop_caches.sh, report.py
```

## Quick start

From `nyxis/`:

```bash
# Small CI-friendly run (10k records)
make bench BENCH_RECORDS=10000

# Full local run (1M records — use results-1m; ~1–2h after fixtures exist)
make -C bench results-1m

# Faster 1M matrix (fewer samples, skips Python FB scan)
make -C bench results-1m-fast

# matrix includes Workload D by default; skip with BENCH_D=0
# At 1M rows for A/B/C, keep D fast: BENCH_RECORDS_D=1000 make -C bench matrix

# Linux cold-open: drop page cache between runs (root)
sudo bash bench/scripts/drop_caches.sh
make bench-cold
```

At **1M+ records** the Python harness automatically uses fewer samples for `scan`/`distinct`, keeps **warm parsed messages** for proto/capnp/arrow access+scan, reuses the **C `sum_f64` reducer** for NXS scan, and delegates NXS scan to the **Rust harness** when available. FlatBuffers `scan` in Python is skipped unless `BENCH_FULL=1`.

## Workloads

| ID | Name | Primary metric | Expected Nyxis outcome |
| --- | --- | --- | --- |
| A | Sparse hierarchical records | File size + selective read | Dev 10k: proto smallest; capnp/fb fastest selective; NXS mid-pack |
| B | Cold-open random access | Open → first field readable | Tie zero-copy peers; edge on huge files |
| C | Dense uniform analytical reducer | `sum` + `count_distinct` | Lose to Arrow; slowest zero-copy |
| D | Streaming ingest (TTFR) | Time to first complete record (D2 file) | Tie proto on TTFR; publish NXS seal cost |

**Workload D (Phase 1):** `make -C bench run-d-smoke` — Rust harness (`bench/harness/stream_d/`), NXS + Protobuf + Cap'n Proto, flat-8 schema. Seal breakdown: `make -C bench run-d-seal-profile`. See `bench/methodology/workload_D.md`. Included in `matrix` by default (`BENCH_D=0` to skip); emits `ttfr` / `seal` / `throughput` JSON lines into `run.log` for `report.py`.

## Harness output

Each harness emits one JSON object per measurement (stdout):

```json
{"workload":"B","format":"nxs","records":1000000,"metric":"open","p50_ns":412,"p99_ns":891,"iqr_ns":120,"samples":1000}
```

## Optional format dependencies

NXS runs with the repo only. Other formats need code generators + Python packages:

```bash
# Required: Python 3.11 or 3.12 only (pyarrow<18 has no cp313 wheels; 3.14 breaks pip on macOS)
# Ubuntu: sudo apt install python3.12 python3.12-venv
bash bench/scripts/setup_venv.sh   # or: PYTHON=python3.12 bash bench/scripts/setup_venv.sh
source .venv-bench/bin/activate

bash bench/generators/codegen.sh   # protoc required; flatc optional; capnp optional
# brew install protobuf flatbuffers capnproto
```

Then:

```bash
make -C bench transcode              # NXB + proto/fb/capnp/arrow when deps exist
make -C bench results BENCH_RECORDS=10000   # full matrix → bench/results/<date>_<host>/
bash bench/scripts/run_all.sh        # same matrix, manual log path

# Regenerate markdown + verdicts from a saved run
make -C bench render-all RESULT_DIR=bench/results/2026-05-21_mmalta
```

## CI and monorepo layout

- **GitHub Actions** (`nyxis/.github/workflows/bench.yml`) checks out **nyxis** only. Go NXS cross-checks in `scripts/run_all.sh` run when `../nyxis-drivers/go` exists (local monorepo); CI skips them without a sibling checkout.
- **Monorepo:** from `nyxis/`, drivers live at `../nyxis-drivers/`. `bench/Makefile` sets `DRV` via `$(abspath ../../nyxis-drivers)`.
- **Cold-open (Linux):** `sudo make bench-cold BENCH_RECORDS=10000` sets `BENCH_COLD=1` and drops page cache before each `open` metric (`drop_caches.sh` no-ops on macOS).

Python harness (all formats):

```bash
python3 bench/harness/python/harness.py --workload B --format proto \
  --records 1000 --metric open --data-dir bench/data/bin
```

## Reproducing published numbers

1. Pin hardware in `results/<date>_<host>/methodology.md` (CPU, RAM, NVMe, kernel, governor).
2. `make bench` with documented `BENCH_RECORDS` and compiler flags (`BENCH_CFLAGS`).
3. Archive `results/<date>_<host>/raw/*.csv` and `summary.json`.
4. Interpret in [BENCHMARK.md](../BENCHMARK.md#workload-comparison-suite).
