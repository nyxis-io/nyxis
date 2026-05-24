# Benchmarks (Nyxis core)

Cross-language benchmark fixtures and browser/Node harnesses live here. The MIT drivers in `nyxis-drivers/` run against `site/bench/fixtures/`.

**Not this folder:** the head-to-head Protobuf / FlatBuffers / Cap'n Proto / Arrow suite is under [`bench/`](../../bench/) (see [BENCHMARK_SUITE.md](../../../BENCHMARK_SUITE.md)).

## Fixtures

```bash
# From nyxis/
make fixtures
# or: FIXTURE_COUNT=1000000 make fixtures
```

Writes `site/bench/fixtures/records_<N>.{nxb,json,csv}` and `records_<N>_columnar.nxb` via the Rust `gen_fixtures` binary. All formats must share the same record count for a given `N` — do not symlink fixtures across sizes. Default `make fixtures` generates `N=1000`.

Verify columnar fixtures are served (browser bench fetches them):

```bash
curl -I http://localhost:8000/bench/fixtures/records_10000_columnar.nxb
# expect HTTP 200, Content-Type: application/octet-stream
```

## Browser benchmark scenarios (current order)

Charts on http://localhost:8000/bench/ are numbered to match the page. Bar labels use **(pre-parsed)** for JSON/CSV warm baselines (parse cost excluded) and **(lazy decode)** for row NXS paths.

| § | Chart id | What it measures |
|---|----------|------------------|
| 1 | `chart-open` | Open / parse entire file |
| 2 | `chart-cold-mem` | Cold: one field at `n/2` + optional `prefetch_viewport` bar (F1) |
| 3 | `chart-cold-fetch` | Cold: same as §2 (fetch path label) |
| 4 | `chart-cold-reduce` | Cold: open/parse + sum `score` (no warm state) |
| 5 | `chart-stream` | Time to first usable record (stream vs `JSON.parse`) |
| 6 | `chart-iterate-all` | Open + walk all rows; includes `prefetch_viewport` + scan |
| 7 | `chart-json-scan` | JSON substring scan vs parse+loop vs NXS `cursor.scan` |
| 8 | `chart-iterate-warm` | Warm iterate + `prefetch_viewport` warm scan bar |
| 9 | `chart-random` | Warm: random one-field access |
| 10 | `chart-random-multi` | Warm: random four-field access |
| 11 | `chart-scattered` | Warm: ~500 strided random reads |
| 12 | `chart-multi-scan` | Open + linear four-field scan (`cursor.scan`, not `seekWarm`/row) |
| 13 | `chart-filter` | Warm: count `score > 80` (cursor filter; row bitmask cost) |
| 14 | `chart-reduce` | Warm: sum `score` (JSON/CSV + row `sumF64` + **columnar `colSumF64`**) |
| 15 | `chart-indexed-sum` | Row index loop vs `sumF64` vs columnar reducer |
| 16 | `chart-column-prefetch` | F0: JSON warm/cold vs NXS columnar cold / mistaken prefetch / warm persistent reader |
| 17 | `chart-memory` | Chrome `performance.memory` (indicative) |
| 18 | `chart-worker` | Main-thread vs worker chunk sum |
| 18–19 | WAL charts | Reference data from Rust WAL bench |

Static assets: `bench-worker.js` is served at `/bench/bench-worker.js` (nginx alias to `site/bench/bench-worker.js`). Do not copy it into `site/dist/bench/` — that directory shadows the Vue route `/bench/`.

Harness implementation: `bench-run.js` (browser), `bench.js` (Node). §17 heap deltas are captured **before** the main suite parses JSON (rendered last). §12–13 use `cursor.scan` / cursor filter — not `seekWarm` per row or `Query.count()` allocation.

## Node benchmark

```bash
node site/bench/bench.js site/bench/fixtures
```

## WASM reducers

Sources and build script: `site/bench/wasm/`. CI rebuilds `nxs_reducers.wasm` on changes (`.github/workflows/build-wasm.yml`).

Exports include column reducers (`sum_f64`, …), `build_field_index`, `batch_resolve_offsets`, `batch_get_f64`, and WAL `encode_span`. Random-access charts also show pure-JS `cursor` / `buildFieldIndex` paths (see `nyxis-drivers/js/nxs.js`).
