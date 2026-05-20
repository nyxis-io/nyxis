# NXS — Nyxis

**A bi-modal serialization format that opens a 1.5 GB dataset in under 2 microseconds.**

**Author:** Micael Malta · [Live demos →](https://nxs.covibe.us/index.html)

---

## The Problem

JSON was designed to be read by humans and transmitted over HTTP — not to serve as an in-memory query layer for millions of records. At scale, the parsing overhead becomes the bottleneck: every field is a heap allocation, every number is a string that must be converted, and the entire payload must be decoded before the first record can be accessed. CSV has the same decode cost with no type information at all. Protobuf solves the type problem but sacrifices human readability and requires schema compilation tooling that couples producers and consumers. None of them can be memory-mapped and queried without a full parse pass, and none are safe to share across threads or web workers without copying.

---

## What is NXS

NXS (Nyxis) is a bi-modal data format with two representations. The text format (`.nxs`) is a sigil-typed, human-readable source language compiled by a Rust tool. The binary format (`.nxb`) is a zero-copy wire format designed around CPU-native memory alignment. Because the binary layout encodes type and offset information at write time, a reader can locate any record by index, decode any single field by key, and run columnar reducers over millions of records — all without parsing the file.

---

## The Four Pillars


| Pillar             | Mechanism                                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------------------------ |
| **Fast**           | 8-byte aligned atomic cells enable zero-copy reads. No deserialization pass required to access a field.            |
| **Flexible**       | LEB128 bitmask tracks field presence per record. Sparse objects carry no overhead for absent fields.               |
| **Compressible**   | All field names are interned into a dictionary. Records store 2-byte indices, not repeated strings.                |
| **Human Readable** | The `.nxs` source format is self-describing plain text. Each value carries its type via a leading sigil character. |


---

## Benchmark Numbers

All benchmarks use an 8-field record schema on an Apple M-series (arm64), macOS. See `[BENCHMARK.md](./BENCHMARK.md)` for the full cross-language results.

### Open / cold read (1M records)


| Language       | NXS open | JSON baseline | Speedup        |
| -------------- | -------- | ------------- | -------------- |
| Go             | 279 ns   | 1.04 s        | **3,700,000×** |
| PHP (C ext)    | 291 ns   | 532 ms        | **1,829,000×** |
| Python (C ext) | 367 ns   | 774 ms        | **2,109,000×** |
| Ruby (C ext)   | 667 ns   | 339 ms        | **508,000×**   |
| JavaScript     | 620 ns   | 310 ms        | **500,000×**   |


### Reducer `sum_f64("score")` (1M records)


| Language          | NXS     | JSON baseline                | NXS faster by |
| ----------------- | ------- | ---------------------------- | ------------- |
| C                 | 6.8 ms  | 56 ms (raw scan)             | **8×**        |
| Go indexed (hot)  | 249 µs  | 252 µs (pre-parsed)          | **ties**      |
| Kotlin            | 4.3 ms  | 1,286 ms (org.json)          | **296×**      |
| Python (C ext)    | 3.48 ms | 31 ms                        | **8.9×**      |
| Swift             | 8.2 ms  | 2,038 ms (JSONSerialization) | **249×**      |
| C#                | 8.8 ms  | 292 ms (System.Text.Json)    | **33×**       |
| JavaScript (WASM) | 8.1 ms  | ~10 ms (pre-parsed)          | **ties**      |
| Ruby (C ext)      | 7.49 ms | 39 ms                        | **5.2×**      |
| PHP (C ext)       | 2.21 ms | 30.9 ms                      | **14×**       |


### WAL / span ingestion (10k spans, 14 services, 20 OTel ops)


| Language          | NXS WAL   | JSON baseline | NXS faster by |
| ----------------- | --------- | ------------- | ------------- |
| C (C99)           | 82 ns     | 262 ns        | **3.2×**      |
| Go                | 138 ns    | 289 ns        | **2.1×**      |
| Python (C ext)    | 438 ns    | 1,383 ns      | **3.2×**      |
| Ruby (C ext)      | 336 ns    | 383 ns        | **1.1×**      |
| JavaScript (fast) | ~250 ns   | ~620 ns       | **~2.5×**     |
| JavaScript (WASM) | ~280 ns   | ~620 ns       | **~2.2×**     |
| Python (pure)     | 3,800 ns  | 1,383 ns      | 0.4× (slower) |
| Ruby (pure)       | 5,300 ns  | 383 ns        | 0.1× (slower) |

Span schema: 14 services, 20 OTel operation names, realistic per-op duration distributions, ~15% payload rate.

### File size (1M records)


| Format | Size    | vs JSON |
| ------ | ------- | ------- |
| NXS    | 131 MB  | 89%     |
| JSON   | 147 MB  | 100%    |
| CSV    | 73 MB   | 49%     |
| XML    | ~209 MB | 142%    |


---

## Language Support


| Language       | Reader              | C extension      | Bulk reducers                                              | Tests                 |
| -------------- | ------------------- | ---------------- | ---------------------------------------------------------- | --------------------- |
| **Rust**       | ✅ compiler + writer | —                | `sum_f64`, `sum_f64_fast`, `sum_f64_fast_par`              | `cargo test`          |
| **JavaScript** | ✅ Node + Browser    | WASM (`encode_span`, `WasmSpanWriter`) | `sumF64`, `minF64`, `maxF64`, `sumI64` | `node test.js` |
| **Python**     | ✅ pure + C ext      | `_nxs.so`        | `sum_f64`, `min_f64`, `max_f64`, `sum_i64`                 | `python test_nxs.py`  |
| **Go**         | ✅                   | —                | `SumF64`, `SumF64Fast`, `SumF64FastPar`, `BuildFieldIndex` | `go test ./...`       |
| **Ruby**       | ✅ pure + C ext      | `nxs_ext.bundle` | `sum_f64`, `min_f64`, `max_f64`, `sum_i64`                 | `ruby test.rb`        |
| **PHP**        | ✅ pure + C ext      | `nxs.so`         | `sumF64`, `minF64`, `maxF64`, `sumI64`                     | `php test.php`        |
| **C/C++**      | ✅ C99, zero deps    | —                | `nxs_sum_f64`, `nxs_min_f64`, `nxs_max_f64`, `nxs_sum_i64` | `make test && ./test` |
| **Swift**      | ✅ Swift 5.9+        | —                | `sumF64`, `minF64`, `maxF64`, `sumI64`                     | `swift run nxs-test`  |
| **Kotlin**     | ✅ JVM, JDK 17+      | —                | `sumF64`, `minF64`, `maxF64`, `sumI64`                     | `gradle run`          |
| **C#**         | ✅ .NET 9+           | —                | `SumF64`, `MinF64`, `MaxF64`, `SumI64`                     | `dotnet run`          |


All ten implementations read the same `.nxb` binary produced by the Rust compiler.

---

## Browser Demos

Live at **[nxs.covibe.us](https://nxs.covibe.us/index.html)**


| Demo                                | What it shows                                                                        |
| ----------------------------------- | ------------------------------------------------------------------------------------ |
| `[bench.html](js/bench.html)`       | NXS vs JSON vs CSV — open, random access, reducer, cold pipeline — up to 14M records |
| `[ticker.html](js/ticker.html)`     | 60 FPS in-place byte patch vs full JSON re-parse — jank visible in sparkline         |
| `[workers.html](js/workers.html)`   | 4 Web Workers, 1 `SharedArrayBuffer`, 0 bytes copied — vs 57 MB × 4 for JSON         |
| `[explorer.html](js/explorer.html)` | 10M-line log explorer — virtual scroll, live search, zero-copy                       |
| `[wal.html](js/wal.html)`           | WAL ingestion — 5 encoders (generic, fast, sealed, WASM, JSON) — live cross-language chart |


```bash
cd js && python3 server.py   # required for SharedArrayBuffer (sets COOP/COEP headers)
# open http://localhost:8000
```

---

## Example

Every value in a `.nxs` file carries a sigil that declares its machine type — no schema file, no generated code:

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


| Sigil | Type                | Binary encoding              |
| ----- | ------------------- | ---------------------------- |
| `=`   | Int64               | 8 bytes LE                   |
| `~`   | Float64             | 8 bytes IEEE 754 LE          |
| `?`   | Bool                | 1 byte + 7 bytes padding     |
| `$`   | Keyword (interned)  | 2-byte dict index            |
| `"`   | String              | u32 length + UTF-8 bytes     |
| `@`   | Timestamp (Unix ns) | 8 bytes LE                   |
| `<>`  | Binary blob         | u32 length + raw bytes       |
| `&`   | Link                | 4-byte relative offset       |
| `!`   | Macro               | Resolved at compile time     |
| `^`   | Null                | Zero-width (bitmask bit set) |


More examples in `[examples/](./examples/)` and full API usage in `[GETTING_STARTED.md](./GETTING_STARTED.md)`.

---

## Format Overview

A `.nxb` file is four segments: a 32-byte preamble, an embedded schema header, a data sector, and a tail-index. The tail-index holds one `(KeyID u16, AbsoluteOffset u64)` pair per top-level record and is located by reading the final `FooterTailPtr` before `MagicFooter` — enabling O(1) random access with a single seek. All atomic values are 8-byte aligned, allowing zero-copy reads on any little-endian platform.

```
[Preamble 32B][Schema Header][Data Sector][Tail-Index]
```

---

## Quick Start

```bash
# Generate test fixtures (required by all language benchmarks and tests)
cd rust && cargo run --release --bin gen_fixtures -- ../js/fixtures 1000

# Compile a .nxs source file
cargo build --release
./target/release/nxs ../examples/user_profile.nxs

# Run all language tests
cd js     && node test.js ../js/fixtures
cd py     && python test_nxs.py ../js/fixtures
cd go     && go test ./...
ruby ruby/test.rb js/fixtures
php php/test.php js/fixtures
cd c      && make test && ./test ../js/fixtures
cd swift  && swift run nxs-test ../js/fixtures
cd kotlin && gradle run --args="../js/fixtures"
cd csharp && dotnet run -- ../js/fixtures
```

---

## Documentation


| Document                                     | Purpose                                                                         |
| -------------------------------------------- | ------------------------------------------------------------------------------- |
| `[SPEC.md](./SPEC.md)`                       | Canonical binary format specification (ground truth for all implementations)    |
| `[RFC.md](./RFC.md)`                         | Formal RFC with motivation, security guidance, and implementation notes         |
| `[GETTING_STARTED.md](./GETTING_STARTED.md)` | Code examples for all ten languages                                             |
| `[BENCHMARK.md](./BENCHMARK.md)`             | Full benchmark results with methodology for all languages and scenarios         |
| `[SCENARIOS.md](./SCENARIOS.md)`             | Browser stress scenarios (large files, 60 FPS, SharedArrayBuffer, log explorer) |
| `[CONTRIBUTING.md](./CONTRIBUTING.md)`       | How to add a new language implementation or report spec ambiguities             |


---

## MCP Server

`nxs-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server that lets an AI agent (Claude, Cursor, etc.) query `.nxb` files directly. It wraps the Rust CLI binaries as typed MCP tools with no additional parsing logic.

### Build

```bash
# Rust binaries must be built first
cd rust && cargo build --release && cd ..

make build-mcp          # → bin/nxs-mcp
make install-mcp        # install to /usr/local/bin (PREFIX=~/.local to override)
make test-mcp           # run unit tests
make lint-mcp           # gofmt + go vet
```

### Tools

| Tool | Description |
|------|-------------|
| `nxs_schema` | Return the schema (key names + sigil types) of a `.nxb` file |
| `nxs_inspect` | Decode schema + N records (default 3) as JSON |
| `nxs_record` | Decode a single record by zero-based index |
| `nxs_export_json` | Export up to N records as JSON (default 100; pass `-1` for all) |
| `nxs_export_csv` | Export as CSV with optional column selection |
| `nxs_import` | Convert JSON / CSV / XML → `.nxb` |
| `nxs_compile` | Compile a `.nxs` text source → `.nxb` binary |

### Wire into Claude Code

Add to `.claude/settings.json`:

```json
{
  "mcpServers": {
    "nxs": {
      "command": "/absolute/path/to/bin/nxs-mcp",
      "args": [
        "--data-dir", "/absolute/path/to/js/fixtures/",
        "--bin-dir", "/absolute/path/to/rust/target/release/"
      ]
    }
  }
}
```

`--data-dir` makes all `.nxb` files in that directory discoverable as `nxb:///` resources. `--bin-dir` tells the server where to find the Rust binaries (`nxs-inspect`, `nxs-export`, `nxs-import`, `nxs`). If omitted, the server searches `../rust/target/release/` relative to its own location, then `$PATH`.

---

## CI

Every language has its own GitHub Actions workflow triggered on changes to its directory. Fixtures are generated once by the Rust workflow and shared as artifacts. See `[.github/workflows/](.github/workflows/)`.

---

## Licensing

Nyxis is published under the **Business Source License 1.1 (BSL)**. It is completely free for development, testing, and production environments for organizations with less than $1M in annual revenue or processing under 100GB of data per month.

Production use outside of these parameters requires a commercial license. See [COMMERCIAL.md](./COMMERCIAL.md) for pricing packages, enterprise support tiers, and proprietary data connectors, or contact **licensing@nyxis.io**.

---

## Status

**Stable (v1.1).** The spec now supports streamable sealed `.nxb` files: writers can emit schema and record bytes before the final Tail-Index is known, then seal with `FooterTailPtr + MagicFooter`. The `conformance/` directory contains vectors validated by runners in Rust, JS, Python, Go, Ruby, PHP, C, Swift, Kotlin, and C#.
