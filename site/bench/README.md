# Benchmarks (Nyxis core)

Cross-language benchmark fixtures and browser/Node harnesses live here. The MIT drivers in `nyxis-drivers/` run against `site/bench/fixtures/`.

## Fixtures

```bash
# From nyxis/
make fixtures
# or: FIXTURE_COUNT=1000000 make fixtures
```

Writes `site/bench/fixtures/records_<N>.{nxb,json,csv}` via the Rust `gen_fixtures` binary.

## Node benchmark

```bash
node site/bench/bench.js site/bench/fixtures
```

## Browser benchmark

See [demo/README.md](../demo/README.md) — open http://localhost:8000/bench/ through docker compose.

## WASM reducers

Sources and build script: `site/bench/wasm/`. CI rebuilds `nxs_reducers.wasm` on changes (`.github/workflows/build-wasm.yml`).
