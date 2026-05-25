# Nyxis — Open massive structured datasets in the browser

> Stream, filter, and explore GB-scale structured data without JSON hydration bottlenecks.

Nyxis compiles human-readable `.nxs` to memory-mapped `.nxb` for browser-native workloads: virtual scroll, streaming ingest, and selective field reads — without `JSON.parse()` on the full export.

## Why JSON breaks

- V8 string limits on multi-hundred-MB payloads
- AST heap inflation and UI freezes
- Full hydration before first interaction
- Silent integer truncation above `2^53−1`

## Proof

- [Log explorer](https://nyxis.io/demo/explorer) — millions of rows, live telemetry
- [Browser benchmarks](https://nyxis.io/bench/) — time to interactive, memory, filter latency
- [5-minute quickstart](https://nyxis.io/docs/)

## How it works

Zero-copy reads, tail-index seeks, streamable v1.2 sealing, row / columnar / PAX layouts. Full depth: [use cases](https://nyxis.io/use-cases/), [BENCHMARK.md](https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md), [SPEC.md](https://github.com/nyxis-io/nyxis/blob/main/SPEC.md).
