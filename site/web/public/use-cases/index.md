# Nyxis — Production Use Cases & System Topologies

Production topologies for mmap `.nxb` ingestion, streamable v1.2 sealing, row/columnar/PAX layouts, append-only WALs, Arrow bridges, and multi-terabyte data-grid deployments.

## Why JSON.parse hits V8 walls

Large JSON payloads force full parse passes and heap churn. Nyxis compiles human-readable `.nxs` to memory-mapped `.nxb` with a tail-index for O(1) record seek — no full-file parse on read.

- **Zero-copy mmap** — readers decode fields by offset, not by re-parsing strings
- **Flat heap under virtual scroll** — fixed row pool maps scroll position to record indices
- **Typed 64-bit segments** — atomic cells for numeric fields without boxing

## Three layouts

| Layout | Best for | Notes |
|--------|----------|-------|
| **Row** | Streaming ingest, virtual scroll, WAL | 7 µs TTFR P50 on EPYC 9R14 |
| **Columnar** | Charts, OLAP, aggregates | 1.3× Arrow IPC on AVX-512 vs Arrow IPC |
| **PAX** | Mixed scroll + column scan | Single artifact for dashboards |

Choose layout at compile time — not one size fits all.

## Streamable ingest & WAL sealing (v1.2)

- **Streamable `.nxb`** — preamble `TailPtr = 0` during ingest; seal writes footer at EOF
- **Append-only WAL (`.nxsw`)** — hot paths append NYXO rows; seal replays into indexed `.nxb`
- **Incremental readers** — MIT drivers expose stream parsers for browsers and agents

## Production topologies

### Live reporting & streaming ingest

High-frequency dashboards and tick feeds. Row layout with streamable sealing avoids JSON stringify/parse per update. Columnar layout for chart aggregates from `col_buffer`.

### Kubernetes stdout aggregators

Cluster log pipelines mmap `.nxb` segments written by sidecars. Tail-index enables seek without re-indexing full files on pod rotation.

### Edge IoT & fleet telemetry

Battery-preserving edge nodes compile `.nxs` locally; columnar or row WAL for upload batches. Zero-copy reads on the server side.

### NGINX, Envoy & API gateway access log viewers

Virtual scroll over millions of log lines backed by mapped `.nxb`. Sub-millisecond warm field access for filter and search UI.

### Zero-copy Web Worker handoffs (SharedArrayBuffer)

JSON structured clone vs SharedArrayBuffer handoff for the same dataset. COOP/COEP required for true zero-copy between workers.

### Apache Arrow bridge

Enterprise extension projects Nyxis column buffers to Arrow C Data Interface for Polars, DuckDB, Snowflake, and Tableau at register speed.

### Intelligent AI agent tools & MCP servers

Go-based `nxs-mcp` exposes `.nxb` inspection, export, import, and compile as typed MCP tools. Agents query binary blocks without JSON round-trips.

### Multi-terabyte data grids

Enterprise Core adds in-memory compaction (`nxs-compactd`), schema registry (`nxs-registryd`), encrypt-at-rest, replication, and read-only query (`nxs-queryd`).

## When to choose NXS

Choose Nyxis when you need human-readable source, git-diffable binary, sub-µs warm access, streaming seal, or native AI agent access via MCP — and when JSON or Protobuf parse cost dominates your hot path.

## Links

- [Specification](https://github.com/nyxis-io/nyxis/blob/main/SPEC.md)
- [Benchmarks](https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md)
- [Browser demos](https://www.nyxis.io/demo/)
- [Commercial pricing](https://www.nyxis.io/pricing/)

Markdown: https://www.nyxis.io/use-cases/index.md
