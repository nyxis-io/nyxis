# Implementations

Subdomain: implementations/
Source paths: rust/src/, rust/src/convert/, rust/src/bin/, rust/tests/convert/, rust/fuzz/, js/, py/, go/, c/, ruby/, php/, kotlin/, csharp/, swift/

## TASK → LOAD

| Task | Load |
|------|------|
| Understand the Rust compiler pipeline (.nxs → .nxb) | rust.md |
| Write or fix the Rust binary writer (hot path, no AST) | rust.md |
| Debug the Rust lexer or parser | rust.md |
| Work on the converter pipeline (JSON/CSV/XML import/export) | rust_convert.md |
| Add or fix nxs-import, nxs-export, nxs-inspect | rust_convert.md |
| Understand schema inference and conflict policies | rust_convert.md |
| Work on JavaScript reader (NxsReader, NxsObject, NxsCursor) | javascript.md |
| Work on WASM reducers or wasm.js loader | javascript.md |
| Debug or extend the browser demo workers | javascript.md |
| Work on the Python pure reader (nxs.py) | python.md |
| Work on the Python C extension (_nxs.c) | python.md |
| Work on the Go reader, fast-path reducers, or parallel workers | go.md |
| Add or fix a Go columnar reducer (SumF64Fast, BuildFieldIndex) | go.md |
| Work on the C99 header-only reader | c.md |
| Add a bulk reducer to the C reader | c.md |
| Work on the Ruby pure reader or C extension | ruby.md |
| Work on the PHP pure reader or C extension | php.md |
| Work on the Kotlin/JVM reader | kotlin.md |
| Work on the C# / .NET reader | csharp.md |
| Work on the Swift reader | swift.md |

## PATTERN → LOAD

| Pattern | Load |
|---------|------|
| zero-copy / tail-indexed-access | go.md, javascript.md, python.md, ruby.md, php.md, c.md |
| leb128-bitmask-decoding | rust.md, go.md, python.md, ruby.md, php.md, c.md |
| fast-path-reducers / uniform-schema | go.md (fast.go), javascript.md (wasm/) |
| goroutine-parallelism / wasm-integration | go.md (fast.go), javascript.md (wasm/) |
| back-patching / schema-precompilation | rust.md (writer.rs), go.md (writer.go) |
| two-pass-compilation | rust.md (compiler.rs) |
| c-extension | python.md, ruby.md, php.md |
| fixture-based-testing / parity-validation | go.md, javascript.md, python.md, ruby.md, php.md |
| cross-origin-isolation / shared-buffer | javascript.md (server.py, nxs_worker.js) |
| two-pass-import / schema-inference | rust_convert.md |
| entity-expansion-guard / depth-limit | rust_convert.md (xml_in.rs) |

## GOVERNANCE WATCHLIST

No rooms flagged.

## Rooms

| Room | Source paths | Files |
|------|-------------|-------|
| rust.md | rust/src/, rust/fuzz/ | 13 |
| rust_convert.md | rust/src/convert/, rust/src/bin/, rust/tests/convert/ | 14 |
| go.md | go/ | 5 |
| javascript.md | js/ | 12 |
| python.md | py/ | 7 |
| ruby.md | ruby/ | 6 |
| php.md | php/ | 7 |
| c.md | c/ | 4 |
| kotlin.md | kotlin/src/main/kotlin/nxs/ | 4 |
| csharp.md | csharp/ | 4 |
| swift.md | swift/Sources/ | 4 |
