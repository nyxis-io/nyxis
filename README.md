# NXS - Nyxis

**A bi-modal serialization format for zero-copy `.nxb` payloads and human-readable `.nxs` source files.**

Nyxis is built for workloads where the first query matters: open a large payload, seek to one record, decode one field, or run a column reducer without parsing the whole file into objects first.

**Author:** Micael Malta | [Live demos](https://nyxis.io/demo)

## What Lives Here

This repository is the open core for the NXS format:

| Area | Contents |
| --- | --- |
| Format | `SPEC.md`, `RFC.md`, conformance vectors, and the `.nxb` binary contract |
| Rust core | Compiler, writer, reader, import/export CLIs, WAL utilities, and tests |
| Benchmarks | Reproducible benchmark harnesses and published methodology |
| Demos | Browser demos for random access, SharedArrayBuffer workers, WAL ingest, and bench charts |
| MCP | `nxs-mcp`, a Model Context Protocol server for querying `.nxb` files from agents |

Language SDKs live in the MIT-licensed sibling repo [`nyxis-drivers`](https://github.com/nyxis-io/nyxis-drivers). Commercial extensions live in the private [`nyxis-extensions`](https://github.com/nyxis-io/nyxis-extensions) repo. See [`COMMERCIAL.md`](./COMMERCIAL.md) for licensing and repository boundaries.

## Why NXS

JSON, CSV, XML, and many schema-based formats make a reader parse or materialize large amounts of data before a single record is useful. NXS moves the expensive work to write time:

| Goal | Mechanism |
| --- | --- |
| Fast first access | Tail-indexed records allow O(1) random access by record index |
| Zero-copy reads | 8-byte aligned atomic cells can be read directly from mapped bytes |
| Sparse records | LEB128 presence bitmasks encode missing fields without per-field payload cost |
| Compact keys | Field names are interned once in the schema header and referenced by slot |
| Human-editable source | `.nxs` text uses sigils to declare machine types without a separate schema file |

For dense analytics, NXS also supports columnar and PAX layouts. See [`OLAP.md`](./OLAP.md) for `FLAG_COLUMNAR`, `FLAG_PAX`, and page-level CRC details.

## Quick Start

Build the core CLI and compile a source file:

```bash
make fixtures
cd rust
cargo build --release
./target/release/nxs ../examples/user_profile.nxs ../examples/user_profile.nxb
```

Run the core test suite:

```bash
make test
```

Run browser demos and benchmark pages through Docker:

```bash
make demo
```

In the multi-repo workspace, run driver tests after generating core fixtures:

```bash
make -C nyxis fixtures
make -C nyxis-drivers test
```

## Source Example

Every value in a `.nxs` file carries a sigil that declares its binary encoding:

```text
user {
    id:         =42
    username:   "alice_wonder"
    email:      "alice@example.com"
    age:        =31
    balance:    ~2874.99
    active:     ?true
    role:       $admin
    created_at: @2022-03-15
    tags:       [$admin, $beta, $verified]
    address {
        city:    "Springfield"
        country: "US"
    }
}
```

| Sigil | Type | Binary encoding |
| --- | --- | --- |
| `=` | Int64 | 8 bytes little-endian |
| `~` | Float64 | 8 bytes IEEE 754 little-endian |
| `?` | Bool | 1 byte plus alignment padding |
| `$` | Keyword | 2-byte interned dictionary index |
| `"` | String | u32 length plus UTF-8 bytes |
| `@` | Timestamp | Unix nanoseconds, 8 bytes little-endian |
| `<>` | Binary blob | u32 length plus raw bytes |
| `&` | Link | 4-byte relative offset |
| `!` | Macro | Resolved at compile time |
| `^` | Null | Zero-width value tracked by bitmask |

More examples are in [`examples/`](./examples/) and [`GETTING_STARTED.md`](./GETTING_STARTED.md).

## Binary Layout

A `.nxb` file is composed of a fixed preamble, embedded schema, data sector, and tail-index:

```text
[Preamble 32B][Schema Header][Data Sector][Tail-Index]
```

The tail-index stores `(KeyID u16, AbsoluteOffset u64)` entries for top-level records, so readers can find a record with one indexed lookup and then decode only the requested field.

## Language Support

The Rust implementation in this repo is the reference compiler and reader. Application-facing SDKs are maintained in [`nyxis-drivers`](https://github.com/nyxis-io/nyxis-drivers):

| Language | Package or source | Notes |
| --- | --- | --- |
| Rust | this repo | Reference compiler, writer, reader, import/export, WAL |
| C/C++ | GitHub Releases / `nyxis-drivers/c` | C99 reader and writer, no runtime dependencies |
| Go | `go get github.com/nyxis-io/nyxis-drivers/go` | Reader, writer, reducers, adaptive prefetch |
| Python | `pip install nyxis` | Pure implementation plus optional C extension |
| JavaScript | `npm install nyxis` | Node/browser reader, writer, WASM helpers |
| Ruby | `gem install nyxis` | Pure implementation plus C extension |
| PHP | `composer require nyxis/nyxis` | Pure implementation plus C extension |
| Kotlin | Maven/Gradle project | JVM reader and reducers |
| C# | `dotnet add package nyxis` | .NET reader and reducers |
| Swift | Swift package | macOS/iOS reader |

## Benchmarks

The benchmark suite covers sparse access, warm zero-copy access, dense columnar analytics, streaming ingest time-to-first-record, PAX mixed access, adaptive prefetch, and cross-language driver behavior. Published results live in [`BENCHMARK.md`](./BENCHMARK.md), with scenario definitions and repeatable harnesses in [`BENCHMARK_SUITE.md`](./BENCHMARK_SUITE.md) and [`bench/`](./bench/).

High-level guidance:

- Use row layout for fast open, random access, sparse records, and first-query latency.
- Use columnar or PAX layout for dense analytical scans.
- Use the Arrow bridge in `nyxis-extensions` when Arrow-native ecosystems are the integration target.

## Browser Demos

Live demos are served at [nyxis.io/demo](https://nyxis.io/demo).

| Demo | What it shows |
| --- | --- |
| [`site/bench/`](./site/bench/) | NXS vs JSON/CSV benchmark charts |
| `/demo/ticker` | In-place byte patching vs full JSON reparse |
| `/demo/workers` | SharedArrayBuffer workers with zero copied payload bytes |
| `/demo/explorer` | Large log exploration with virtual scrolling and search |
| `/demo/wal` | WAL ingestion across generic, fast, sealed, WASM, and JSON encoders |

## MCP Server

`nxs-mcp` exposes `.nxb` inspection, conversion, and record lookup as typed Model Context Protocol tools.

```bash
cd rust && cargo build --release && cd ..
make build-mcp
make test-mcp
```

Available tools include `nxs_schema`, `nxs_inspect`, `nxs_record`, `nxs_export_json`, `nxs_export_csv`, `nxs_import`, and `nxs_compile`. The server can discover `.nxb` resources through `--data-dir` and locate compiled Rust binaries through `--bin-dir`.

## Documentation

| Document | Purpose |
| --- | --- |
| [`SPEC.md`](./SPEC.md) | Canonical binary format specification |
| [`RFC.md`](./RFC.md) | Motivation, security guidance, and implementation notes |
| [`GETTING_STARTED.md`](./GETTING_STARTED.md) | Code examples for supported languages |
| [`BENCHMARK.md`](./BENCHMARK.md) | Published benchmark results and methodology |
| [`BENCHMARK_SUITE.md`](./BENCHMARK_SUITE.md) | Workload definitions and harness overview |
| [`CONFORMANCE.md`](./CONFORMANCE.md) | Cross-language vector generation and validation |
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | Contribution process and implementation guidance |

## CI And Conformance

Core CI builds Rust, generates conformance vectors, builds WASM artifacts, and runs cross-language conformance jobs against [`nyxis-drivers`](https://github.com/nyxis-io/nyxis-drivers). Run the full matrix from the workspace root with:

```bash
make conformance
```

## Status

The current spec is stable at v1.2. It includes row, columnar, and PAX layouts; adaptive prefetch conformance; page-level CRC support; and conformance runners for Rust plus the driver languages.

## License

Nyxis core is published under the **Business Source License 1.1 (BSL)**. Development, testing, research, and qualifying production use are permitted under the free tier described in [`COMMERCIAL.md`](./COMMERCIAL.md). The wire format specification and conformance vectors are documented for clean-room implementations with attribution.

Commercial production use outside the free tier requires a license. Contact **licensing@nyxis.io**.
