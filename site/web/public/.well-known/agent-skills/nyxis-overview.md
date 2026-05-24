---
name: nyxis-overview
description: Navigate the Nyxis zero-copy binary serialization format, layouts, benchmarks, and documentation.
---

# Nyxis Overview

Nyxis (NXS) is a zero-copy binary serialization standard with three compile-time layouts:

| Layout | Best for | Key metric |
|--------|----------|------------|
| **row** (`.nxb`) | Streaming, virtual scroll, WAL | 7 µs TTFR P50 (EPYC 9R14) |
| **columnar** (`.nxb`) | Charts, OLAP, aggregates | 1.3× Arrow IPC on AVX-512 |
| **pax** (`.nxb`) | Mixed scroll + column scan | Workload E (SPEC §4.5) |

## Key resources

- **Spec**: https://github.com/nyxis-io/nyxis/blob/main/SPEC.md
- **Getting started**: https://github.com/nyxis-io/nyxis/blob/main/GETTING_STARTED.md
- **Benchmarks**: https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md
- **Live demos**: https://nyxis.io/demo/
- **Browser SDK**: https://nyxis.io/sdk/
- **API catalog**: https://nyxis.io/.well-known/api-catalog
- **MCP server card**: https://nyxis.io/.well-known/mcp/server-card.json

## Magic bytes

- `NYXB` — row binary (`.nxb`)
- `NYXO` — append-only WAL (`.nxsw`)
- `NYXL` — columnar binary

## Licensing

- Core compiler: BSL 1.1 (free under revenue/storage limits)
- Language SDKs: MIT ([nyxis-drivers](https://github.com/nyxis-io/nyxis-drivers))
