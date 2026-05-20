# Benchmarks (Nyxis core)

Cross-language benchmark fixtures and browser/Node harnesses live here. The MIT drivers in `nyxis-drivers/` run against `bench/fixtures/`.

## Fixtures

```bash
# From nyxis/
make fixtures
# or: FIXTURE_COUNT=1000000 make fixtures
```

Writes `bench/fixtures/records_<N>.{nxb,json,csv}` via the Rust `gen_fixtures` binary.

## Node benchmark

```bash
node bench/bench.js bench/fixtures
```

## Browser benchmark

See [demo/README.md](../demo/README.md) — open `/bench/bench.html` through docker compose.

## WASM reducers

Sources and build script: `bench/wasm/`. CI rebuilds `nxs_reducers.wasm` on changes (`.github/workflows/build-wasm.yml`).
