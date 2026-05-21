# Benchmarks (Nyxis core)

Cross-language benchmark fixtures and browser/Node harnesses live here. The MIT drivers in `nyxis-drivers/` run against `site/bench/fixtures/`.

## Fixtures

```bash
# From nyxis/
make fixtures
# or: FIXTURE_COUNT=1000000 make fixtures
```

Writes `site/bench/fixtures/records_<N>.{nxb,json,csv}` via the Rust `gen_fixtures` binary. All three must share the same record count for a given `N` — do not symlink `.json`/`.nxb` to a different size (the browser bench compares formats row-for-row). Default `make fixtures` generates `N=1000`.

## Node benchmark

```bash
node site/bench/bench.js site/bench/fixtures
```

## Browser benchmark

See [demo/README.md](../demo/README.md) — open http://localhost:8000/bench/ through docker compose.

Charts include **open + iterate all** (parse/open the file, then read `username` on every record) alongside open-only, random access, cold first-field, and column reducers.

## WASM reducers

Sources and build script: `site/bench/wasm/`. CI rebuilds `nxs_reducers.wasm` on changes (`.github/workflows/build-wasm.yml`).

Exports include column reducers (`sum_f64`, …), `build_field_index`, `batch_resolve_offsets`, `batch_get_f64`, and WAL `encode_span`. Random-access charts also show pure-JS `cursor` / `buildFieldIndex` paths (see `nyxis-drivers/js/nxs.js`).
