---
room: implementations/rust
source_paths: [rust/src/, rust/fuzz/]
file_count: 13
architectural_health: normal
security_tier: normal
hot_paths: [writer.rs, compiler.rs]
see_also: [implementations/rust_convert.md, implementations/c.md, spec/format.md]
---

# bench.rs

DOES: Measures and compares serialization/deserialization throughput and output sizes between NXS, JSON, XML, and CSV formats at multiple scales (10k-1M records).
SYMBOLS:
- Record { id, username, email, age, balance, active, score, created_at }
- iters_for(n: usize) -> u32
- bench<F, R>(iters: u32, f: F) -> Duration
- serialize_nxs(records: &[Record]) -> Vec<u8>
- serialize_nxs_wire(records: &[Record]) -> Vec<u8>
- serialize_json(records: &[Record]) -> Vec<u8>
- serialize_xml(records: &[Record]) -> Vec<u8>
- serialize_csv(records: &[Record]) -> Vec<u8>
- deserialize_nxs(data: &[u8]) -> usize
- deserialize_json(data: &[u8]) -> usize
- deserialize_xml(data: &[u8]) -> usize
- deserialize_csv(data: &[u8]) -> usize
DEPENDS: crate::compiler, crate::decoder, crate::writer, crate::lexer, crate::parser
PATTERNS: benchmark-harness, format-comparison, warmup-run
USE WHEN: Analyzing serialization performance or comparing NXS wire format against JSON/XML/CSV at scale.

---

# compiler.rs

DOES: Transforms parsed AST (Field/Value) into compact binary format; manages key dictionary, object encoding with bitmasks/offset tables, and macro resolution.
SYMBOLS:
- Compiler { dict, key_map }
- Compiler::new() -> Self
- Compiler::compile(fields: &[Field]) -> Result<Vec<u8>>
- Compiler::collect_keys(fields: &[Field])
- encode_value(v: &Value) -> Result<Vec<u8>>
- encode_list(elems: &[Value]) -> Result<Vec<u8>>
- encode_object(fields: &[Field]) -> Result<Vec<u8>>
- build_bitmask(present_indices: &[usize], total_keys: usize) -> Vec<u8>
- resolve_macro(value: &Value, scope: &[Field]) -> Result<Value>
- eval_macro(expr: &str, scope: &[Field]) -> Result<Value>
- murmur3_64(data: &[u8]) -> u64
DEPENDS: crate::parser, crate::error
PATTERNS: leb128-bitmask, back-patching-offsets, dict-intern, murmur-hash, macro-evaluation
USE WHEN: Converting parsed schema and records into binary format; applying macros like @key and now().

---

# decoder.rs

DOES: Reads .nxb files, validates magic bytes and schema hash, reconstructs schema from embedded key dictionary, and decodes objects/lists using sigil types.
SYMBOLS:
- DecodedFile { version, flags, dict_hash, tail_ptr, keys, key_sigils, root_fields, record_count, tail_start, data_sector_start }
- DecodedValue { Int, Float, Bool, Str, Time, Binary, Null, List, Object, Raw }
- decode(data: &[u8]) -> Result<DecodedFile>
- decode_record_at(data: &[u8], offset: usize, keys: &[String], sigils: &[u8]) -> Result<Vec<(String, DecodedValue)>>
- decode_object(data: &[u8], offset: usize, keys: &[String], sigils: &[u8]) -> Result<Vec<(String, DecodedValue)>>
- decode_value_at(data: &[u8], offset: usize, sigil: u8, keys: &[String], sigils: &[u8]) -> Result<DecodedValue>
- decode_list(data: &[u8], offset: usize) -> Result<DecodedValue>
- murmur3_64(data: &[u8]) -> u64
DEPENDS: crate::error
PATTERNS: magic-validation, leb128-decoding, sigil-dispatch, bounds-checking, out-of-bounds-guard
USE WHEN: Reading and validating binary NXS files, reconstructing schema and root object.

---

# error.rs

DOES: Error type enum covering parsing, I/O, magic validation, bounds, macros, recursion limits, and format conversion errors.
SYMBOLS:
- Types: NxsError, Result<T>
- NxsError::BadMagic, UnknownSigil(char), BadEscape(char), OutOfBounds, DictMismatch, CircularLink, RecursionLimit, MacroUnresolved(String), ListTypeMismatch, Overflow, ParseError(String), IoError(String), ConvertSchemaConflict(String), ConvertParseError { offset, msg }, ConvertEntityExpansion, ConvertDepthExceeded
DEPENDS: none
PATTERNS: error-enum, display-trait
USE WHEN: Propagating parsing, I/O, or validation errors across the compilation and decoding pipeline.

---

# fuzz_decode.rs

DOES: Fuzz target that invokes decoder on arbitrary byte sequences; detects panics and bounds violations.
SYMBOLS:
- fuzz_target!
DEPENDS: nxs::decoder
PATTERNS: libfuzzer-harness, crash-detection
USE WHEN: Running continuous fuzzing to find decoder crashes or invalid-input panics.

---

# fuzz_target_1.rs

DOES: Empty fuzz target placeholder.
SYMBOLS: none
DEPENDS: libfuzzer_sys
PATTERNS: placeholder
USE WHEN: Skeleton for future fuzz tests.

---

# fuzz_writer_roundtrip.rs

DOES: Fuzz the writer→decoder round-trip by generating random schemas, records, and slot writes, verifying decoder never panics on output.
SYMBOLS:
- fuzz_target!
DEPENDS: nxs::writer, nxs::decoder
PATTERNS: libfuzzer-harness, roundtrip-test, crash-detection
USE WHEN: Regression testing writer stability and ensuring any valid writer output decodes without panic.

---

# gen_fixtures.rs

DOES: Generates matching .nxb, .json, and .csv fixture files for benchmark datasets at specified sizes (1k–1M records).
SYMBOLS:
- Rec { id, username, email, age, balance, active, score }
- build(n: usize) -> Vec<Rec>
- write_nxb(records: &[Rec], path: &Path)
- write_json(records: &[Rec], path: &Path)
- write_csv(records: &[Rec], path: &Path)
- ensure_out_dir_writable(out_dir: &Path)
- write_file(path: &Path, contents: &[u8], label: &str)
DEPENDS: crate::writer
PATTERNS: fixture-generation, multi-format-serialization
USE WHEN: Preparing benchmark datasets or creating test fixtures for comparison benchmarks.

---

# lexer.rs

DOES: Tokenizes source text (sigils, identifiers, strings, binaries, timestamps, macros) into Token stream; handles escapes, string interpolation, and comments.
SYMBOLS:
- Token { Int(i64), Float(f64), Bool(bool), Keyword(String), Str(String), Time(i64), Binary(Vec<u8>), Link(i32), Macro(String), Null, Ident(String), Colon, LBrace, RBrace, LBracket, RBracket, Comma, LParen, RParen, Eof }
- Lexer::new(input: &str) -> Self
- Lexer::tokenize() -> Result<Vec<Token>>
- read_string() -> Result<String>
- read_binary() -> Result<Vec<u8>>
- read_macro_expr() -> String
- parse_temporal(s: &str) -> Result<i64>
- days_since_epoch(year: i64, month: i64, day: i64) -> i64
DEPENDS: crate::error
PATTERNS: sigil-dispatch, escape-handling, unicode-parsing, temporal-parsing
USE WHEN: Parsing .nxs source text into token stream before AST construction.

---

# lib.rs

DOES: Library entry point; exports public modules compiler, decoder, error, lexer, parser, writer, convert.
SYMBOLS:
- pub mod compiler, convert, decoder, error, lexer, parser, writer
DEPENDS: none
PATTERNS: crate-root, re-export
USE WHEN: Importing NXS functionality via `use nxs::*`.

---

# main.rs

DOES: CLI entry point; reads .nxs source file, compiles to .nxb via lexer→parser→compiler pipeline, writes binary output with size summary.
SYMBOLS:
- compile(source: &str) -> error::Result<Vec<u8>>
DEPENDS: crate::compiler, crate::decoder, crate::error, crate::lexer, crate::parser, crate::writer
PATTERNS: pipeline-orchestration, integration-test-suite
USE WHEN: Testing end-to-end compilation, validation, or running the CLI tool.

---

# parser.rs

DOES: Builds AST (Value/Field tree) from Token stream; enforces depth limits and type uniformity in lists.
SYMBOLS:
- Value { Int(i64), Float(f64), Bool(bool), Keyword(String), Str(String), Time(i64), Binary(Vec<u8>), Link(i32), Macro(String), Null, Object(Vec<Field>), List(Vec<Value>) }
- Field { key, value }
- Parser::new(tokens: Vec<Token>) -> Self
- Parser::parse_file() -> Result<Vec<Field>>
- parse_field() -> Result<Field>
- parse_value() -> Result<Value>
- parse_object() -> Result<Value>
- parse_list() -> Result<Value>
- sigil_name(v: &Value) -> &'static str
DEPENDS: crate::error, crate::lexer
PATTERNS: recursive-descent, depth-limit, type-homogeneity-check
USE WHEN: Converting token stream into strongly-typed AST for compilation.

---

# writer.rs

DOES: High-performance binary emitter that writes objects directly to buffer without AST; uses back-patching for length/offset fields and in-order slot tracking.
SYMBOLS:
- Slot(u16)
- Schema { keys, bitmask_bytes, sigils }
- NxsWriter<'a> { schema, buf, frames, record_offsets, slot_sigils }
- NxsWriter::new(schema: &'a Schema) -> Self
- NxsWriter::begin_object()
- NxsWriter::end_object()
- NxsWriter::finish() -> Vec<u8>
- NxsWriter::write_i64(slot: Slot, v: i64)
- NxsWriter::write_f64(slot: Slot, v: f64)
- NxsWriter::write_bool(slot: Slot, v: bool)
- NxsWriter::write_str(slot: Slot, v: &str)
- NxsWriter::write_bytes(slot: Slot, data: &[u8])
- NxsWriter::write_time(slot: Slot, unix_ns: i64)
- NxsWriter::write_null(slot: Slot)
- NxsWriter::write_list_i64(slot: Slot, values: &[i64])
- NxsWriter::write_list_f64(slot: Slot, values: &[f64])
- Frame { start, bitmask, offset_table, last_slot, needs_sort, slot_offsets }
- build_schema(keys: &[String], sigils: &[u8]) -> Vec<u8>
- build_tail_index_records(data_start: u64, record_offsets: &[u32]) -> Vec<u8>
- murmur3_64(data: &[u8]) -> u64
DEPENDS: none
PATTERNS: back-patching, frame-stack, out-of-order-detection, 8-byte-alignment, zero-copy-serialization
USE WHEN: Serializing records directly to .nxb without source-text parsing (hot-path writer).
