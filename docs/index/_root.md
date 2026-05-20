# LOI Index

Generated: 2026-05-15
Source paths: rust/, js/, py/, go/, c/, ruby/, php/, kotlin/, csharp/, swift/, conformance/

NXS is a bi-modal serialization format: a sigil-typed text source (`.nxs`) compiled by a Rust compiler into a zero-copy binary format (`.nxb`) read by ten language implementations via memory-mapped tail-index + LEB128-bitmask object headers.

## TASK → LOAD

| Task | Load |
|------|------|
| Understand the NXS binary format (preamble, schema, tail-index, bitmask) | spec/format.md |
| Implement a new language reader | spec/format.md → implementations/_root.md |
| Compile .nxs source text to .nxb binary | rust/_root.md |
| Understand the NXS lexer, parser, or AST types | rust/compiler_pipeline.md |
| Add or match an NxsError variant / error code | rust/compiler_pipeline.md |
| Emit .nxb from typed data in Rust (hot path) | rust/writer_decoder.md |
| Understand the WAL append / seal / crash-recovery | rust/writer_decoder.md |
| Query distributed traces across .nxb segments | rust/writer_decoder.md |
| Import JSON / CSV / XML into .nxb | rust/convert.md |
| Export .nxb to JSON or CSV | rust/convert.md |
| Add schema inference type lattice or conflict policy | rust/convert.md |
| Add a CLI flag to nxs-import, nxs-export, nxs-inspect, nxs-trace | rust/bins.md |
| Add integration or fuzz tests for the Rust converter | rust/tests_fuzz.md |
| Read .nxb in JavaScript (Node or browser) | js/_root.md |
| Write .nxb from JavaScript | js/reader.md |
| Add WASM reducers or browser Web Workers | js/wasm_workers.md |
| Read or write .nxb from Python | py/_root.md |
| Use the Python C extension for max throughput | py/c_ext.md |
| Read or write .nxb from Go | go/_root.md |
| Use Go unsafe fast-path aggregate reducers | go/reader.md |
| Read or write .nxb from C or C++ | c/_root.md |
| Read or write .nxb from Ruby | langs/ruby.md |
| Read or write .nxb from PHP | langs/php.md |
| Read or write .nxb from Kotlin/JVM | langs/kotlin.md |
| Read or write .nxb from C# (.NET 8) | langs/csharp.md |
| Read or write .nxb from Swift | langs/swift.md |
| Generate or run conformance test vectors | conformance.md |
| Add a positive or negative conformance vector | conformance.md |
| Debug or add a GitHub Actions CI workflow | ci/workflows.md |
| Regenerate js/fixtures/ for benchmarks | rust/writer_decoder.md |

## PATTERN → LOAD

Cross-cutting behavioral patterns that span multiple rooms.

| Pattern | Load |
|---------|------|
| LEB128 bitmask + offset-table object header | rust/compiler_pipeline.md, rust/writer_decoder.md, c/reader.md |
| Tail-index O(1) random record access | rust/writer_decoder.md, js/reader.md, go/reader.md |
| Schema-once / write-many slot-indexed emit | rust/writer_decoder.md, js/reader.md, py/reader.md, go/reader.md |
| Two-pass streaming import (infer schema + emit) | rust/convert.md |
| WAL append-only + seal-to-segment | rust/writer_decoder.md, rust/bins.md |
| Entity-expansion guard (XML) | rust/convert.md |
| Coverage-guided fuzzing (panic-freedom) | rust/tests_fuzz.md |
| MurmurHash3-64 DictHash integrity | rust/compiler_pipeline.md, c/reader.md, go/reader.md |
| CPython buffer-protocol C extension | py/c_ext.md |
| Freestanding WASM no-libc reducers | js/wasm_workers.md |
| Unsafe-pointer parallel aggregate (Go) | go/reader.md |
| C extension for interpreter languages | py/c_ext.md, langs/ruby.md, langs/php.md |
| Uniform-schema fast path (skip per-record bitmask walk) | go/reader.md, rust/writer_decoder.md |
| Conformance vector positive/negative dispatch | conformance.md |
| Reusable workflow + artifact-passing in CI | ci/workflows.md |

## GOVERNANCE WATCHLIST

Rooms flagged by the Committee for security review.

| Room | Health | Security | Committee Note |
|------|--------|----------|----------------|
| `rust/convert.md` | normal | sensitive | xml_in.rs hard-rejects DOCTYPE/ENTITY; this room processes untrusted external data |
| `c/reader.md` | normal | sensitive | nxs_writer.h/c shared as include by py/_nxs.c and ruby/ext/nxs/nxs_ext.c; changes propagate |
| `py/c_ext.md` | normal | sensitive | _nxs.c performs raw CPython buffer-protocol pointer arithmetic |
| `langs/ruby.md` | normal | sensitive | nxs_ext.c inline LEB128 scan over Ruby string data |
| `langs/php.md` | normal | sensitive | nxs_ext.c Zend object lifecycle with ecalloc over PHP string data |

## Buildings

| Subdomain | Description | Rooms |
|-----------|-------------|-------|
| spec/ | Binary format specification and RFC | format.md |
| rust/ | Compiler pipeline, writer/decoder, WAL, convert, CLI binaries, tests/fuzz | compiler_pipeline.md, writer_decoder.md, convert.md, bins.md, tests_fuzz.md |
| js/ | JavaScript reader/writer, WASM reducers, Web Workers, browser demo | reader.md, wasm_workers.md |
| py/ | Python reader/writer and C extension | reader.md, c_ext.md |
| go/ | Go reader, fast-path reducers, writer, tests, benchmarks | reader.md |
| c/ | C99 reader/writer headers and benchmarks | reader.md |
| langs/ | Ruby, PHP, Kotlin, C#, Swift implementations | ruby.md, php.md, kotlin.md, csharp.md, swift.md |
| conformance.md | Conformance suite — vector generator + 10 language runners | (flat file) |
| implementations/ | Legacy index — per-language summary rooms | rust.md, rust_convert.md, javascript.md, python.md, go.md, c.md, ruby.md, php.md, kotlin.md, csharp.md, swift.md |
| ci/ | GitHub Actions workflows for all languages and publish pipelines | workflows.md |
