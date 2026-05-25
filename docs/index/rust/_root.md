---
room: _root
subdomain: rust
source_paths: rust/
see_also: docs/index/_root.md
architectural_health: normal
security_tier: normal
---

# Rust — Building Router

Subdomain: rust/
Source paths: rust/src/, rust/fuzz/, rust/tests/

## TASK → LOAD

| Task | Load |
|------|------|
| Compile .nxs source text to .nxb binary | compiler_pipeline.md |
| Understand the lexer, parser, or AST types | compiler_pipeline.md |
| Add or modify an NxsError variant | compiler_pipeline.md |
| Emit .nxb from typed data (hot path, no text parsing) | writer_decoder.md |
| Understand WAL append / seal / crash-recovery | writer_decoder.md |
| Query sealed .nxb segments for span traces | writer_decoder.md |
| Import JSON / CSV / XML into .nxb | convert.md |
| Export .nxb to JSON or CSV | convert.md |
| Add a CLI flag or new converter binary | bins.md |
| Run the nxs-trace WAL pipeline | bins.md |
| Add integration or fuzz tests | tests_fuzz.md |
| Verify exit-code contract for error paths | tests_fuzz.md |
| Query .nxb with predicates or columnar layout | runtime.md |
| Tune adaptive prefetch or column warmup | prefetch.md |
| Use schema registry gRPC client | registry.md |

## Rooms

| Room | Source paths | Files |
|------|-------------|-------|
| compiler_pipeline.md | rust/src/lexer.rs, parser.rs, compiler.rs, error.rs | 4 |
| writer_decoder.md | rust/src/writer.rs, decoder.rs, lib.rs, main.rs, gen_fixtures.rs, bench.rs, wal.rs, segment_reader.rs | 8 |
| convert.md | rust/src/convert/ | 8 |
| bins.md | rust/src/bin/ | 4 |
| tests_fuzz.md | rust/tests/, rust/tests/convert/, rust/fuzz/fuzz_targets/ | 8 |
| prefetch.md | rust/src/prefetch/, rust/src/column_prefetch.rs | 3 |
| registry.md | rust/src/registry/ | 3 |
| runtime.md | rust/src/query.rs, layout.rs, arrow_project.rs, pax_stream.rs, col_reduce.rs, stream_reader.rs, wasm_api.rs, consts.rs, rust/build.rs | 9 |
