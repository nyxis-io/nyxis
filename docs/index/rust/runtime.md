---
room: runtime
subdomain: rust
source_paths: [rust/build.rs, rust/src/query.rs, rust/src/layout.rs, rust/src/arrow_project.rs, rust/src/pax_stream.rs, rust/src/consts.rs, rust/src/col_reduce.rs, rust/src/stream_reader.rs, rust/src/wasm_api.rs]
see_also: ["writer_decoder.md", "prefetch.md", "compiler_pipeline.md"]
hot_paths: [rust/src/query.rs, rust/src/layout.rs]
architectural_health: normal
security_tier: normal
---

# rust/ — Query, Layout & WASM Runtime

Subdomain: rust/
Source paths: rust/, rust/src/, rust/build.rs, rust/src/query.rs, rust/src/layout.rs, rust/src/arrow_project.rs, rust/src/pax_stream.rs, rust/src/consts.rs, rust/src/col_reduce.rs, rust/src/stream_reader.rs, rust/src/wasm_api.rs

## TASK → LOAD

| Task | Load |
|------|------|
| Filter .nxb records with zero-copy predicates | runtime.md |
| Emit columnar or PAX layout from compiler | runtime.md |
| Project columns to Arrow buffers | runtime.md |
| Stream-parse NYXO chunks | runtime.md |
| Expose compile helpers to WASM | runtime.md |

---

# build.rs

DOES: Cargo build script; configures protobuf/codegen hooks for registry gRPC and release metadata.
SYMBOLS:
- fn main() build directives

---

# arrow_project.rs

DOES: Zero-copy Arrow projection from columnar/PAX NXB sectors for analytics export.
SYMBOLS:
- VarColumnView struct
- (+projection builders per sigil)
DEPENDS: layout.rs, query.rs

---

# col_reduce.rs

DOES: Columnar aggregate helpers (dense null bitmap scan, sum_f64 over nullable columns) for benchmarks and WASM reducers.
SYMBOLS:
- null_bitmap_dense(bm, n) → bool
- sum_f64_column(vals, bm, n) → f64
- (+more reducers)

---

# consts.rs

DOES: Shared NXB magic bytes, version, and layout flag constants used across writer, query, and layout modules.
SYMBOLS:
- MAGIC_FILE, MAGIC_FOOTER, MAGIC_OBJ, MAGIC_PAGE
- FLAG_COLUMNAR, FLAG_PAX, FLAG_SCHEMA_EMBEDDED, VERSION

---

# layout.rs

DOES: Columnar and PAX layout writers: schema-once sector emit, footers (12/20/28 byte), variable-length column tails (OLAP.md).
SYMBOLS:
- Layout enum { Row, Columnar, Pax }
- Layout::parse_name(s) → Option<Layout>
- compile_columnar, compile_pax, finish writers
- col_var_parts, column_sector_len helpers
DEPENDS: writer.rs, parser.rs, compiler.rs
PATTERNS: schema-once-columnar, pax-pages

---

# pax_stream.rs

DOES: Streaming PAX page reader for sequential analytics without loading full tail index.
SYMBOLS:
- (+PAX page iteration structs and decode)
DEPENDS: layout.rs, query.rs

---

# query.rs

DOES: Zero-allocation query Reader over mmap'd .nxb; predicate DSL (eq, gt, And); integrates PrefetchEngine and column warmup.
SYMBOLS:
- Reader::new(data) → Result<Reader>
- where_pred(predicate) → RecordIter
- Layout enum, footer_size(flags)
- Types: And, eq, gt predicate builders
DEPENDS: prefetch/, column_prefetch.rs, layout.rs, consts.rs
PATTERNS: zero-copy-query, tail-index-access
USE WHEN: OLAP-style scans with predicates on sealed .nxb files

---

# stream_reader.rs

DOES: Incremental NYXO stream parser; locates chunk boundaries and exposes StreamReader for partial buffers.
SYMBOLS:
- complete_nyxo_end(data, off) → Option<usize>
- StreamReader struct
PATTERNS: streaming-framing

---

# wasm_api.rs

DOES: wasm-bindgen exports compiling .nxs text to .nxb bytes (row and columnar) for browser demos.
SYMBOLS:
- compile_nxs(source &str) → Result<Uint8Array, JsValue>
- compile_nxs_columnar(source &str) → Result<Uint8Array, JsValue>
DEPENDS: compiler.rs, layout.rs
