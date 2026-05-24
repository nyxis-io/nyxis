# Nyxis — Zero-Copy Binary Serialization Protocol

> Zero-copy binary format with three layouts for three workloads.

Nyxis is an infrastructure serialization standard — not application software. Human-readable `.nxs` compiles to memory-mapped `.nxb` you query by tail-index, without `JSON.parse()` or full-file decode passes.

## Performance highlights

- **Row layout:** 7 µs time-to-first-record on EPYC 9R14; 37 µs on Linux inotify (streaming, P50).
- **Columnar layout:** 1.3× Arrow IPC on AMD EPYC 9R14 AVX-512; at parity on Apple Silicon.
- **PAX layout:** mixed scroll + column scan in one file (v1.2; page streaming per SPEC §4.5).

v1.2 supports row, columnar, and PAX layouts with **streamable sealing**: emit schema and records incrementally, then seal the tail-index when the segment is complete.

## Three layouts, three workloads

| Layout | Strengths | Use cases |
|--------|-----------|-----------|
| **Row** (`.nxb`) | Stream records as they arrive; O(1) seek via tail-index | Virtual scroll, log explorers, APM traces |
| **Columnar** (`.nxb`) | Field buffers for charts; sub-µs open cost | Chart rendering, OLAP, export pipelines |
| **PAX** (`.nxb`) | Pages stream as they fill; dual access patterns | Reporting dashboards, mixed scroll + charts |

## Five pillars

1. **Fast** — 8-byte aligned atomic cells; zero-copy reads without deserialization.
2. **Flexible** — LEB128 bitmask per record; sparse objects pay nothing for absent fields.
3. **Compressible** — Interned field dictionary; records store 2-byte indices, not repeated strings.
4. **Human readable** — `.nxs` is self-describing plain text — the source is the schema.
5. **Streamable** — Writers stream records before the tail-index exists; append-only WAL mode seals to indexed `.nxb` on demand.
6. **AI-native** — The included MCP server exposes NXS files as typed tools for AI agents.

## Bimodal wire format

Sigil-typed `.nxs` compiles to aligned `.nxb` cells. Readers `mmap` the wire image, use the tail-index for record offsets, and decode fields by key — no full-file parse pass.

Example `.nxs`:

```
user {
  id = 42
  name "ada_lovelace"
  score ~ 98.6
  active ? true
}
```

## Benchmarks

Public benchmark suite covering five workloads (A–E) against Protobuf, FlatBuffers, Cap'n Proto, and Apache Arrow — macOS Apple Silicon, Linux Haswell, and AWS EPYC 9R14 (AVX-512).

- Full methodology: [BENCHMARK.md](https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md)
- Interactive charts: [Browser bench](https://nyxis.io/bench/)

## Stream, then seal

Producers can emit aligned `.nxb` bytes while a segment is still open — readers parse complete records as they arrive and only need the footer tail pointer once the writer seals the file.

- **Streamable `.nxb`** — Preamble `TailPtr = 0` during ingest; sealing writes footer at EOF.
- **Append-only WAL** (`.nxsw`) — Hot paths append NYXO rows; seal replays into indexed `.nxb`.
- **Incremental readers** — MIT drivers expose stream parsers for browsers and agents.

## Explore

- [Browser demos](https://nyxis.io/demo/) — Ticker, workers, log explorer, WAL
- [Benchmark suite](https://github.com/nyxis-io/nyxis/blob/main/BENCHMARK.md)
- [JavaScript SDK](https://nyxis.io/sdk/) — MIT-licensed browser reader
- [Specification](https://github.com/nyxis-io/nyxis/blob/main/SPEC.md) — v1.2.1 stable
- [Use cases](https://nyxis.io/use-cases/) — Production topologies
- [Commercial pricing](https://nyxis.io/pricing/)

## Agent discovery

- API catalog: `/.well-known/api-catalog`
- MCP server card: `/.well-known/mcp/server-card.json`
- Agent skills: `/.well-known/agent-skills/index.json`

## Enterprise

Nyxis Core and multi-language SDKs are free and open for production within BSL limits. Enterprise Extensions add in-memory compaction, Apache Arrow zero-copy bridge, schema registry, and platform operations.

- Core & CLI: [BSL 1.1](https://github.com/nyxis-io/nyxis/blob/main/LICENSE)
- SDKs: [MIT](https://github.com/nyxis-io/nyxis-drivers) across ten languages
