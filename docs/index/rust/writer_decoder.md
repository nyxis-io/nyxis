---
room: writer_decoder
subdomain: rust
source_paths: rust/src/writer.rs, rust/src/decoder.rs, rust/src/lib.rs, rust/src/main.rs, rust/src/gen_fixtures.rs, rust/src/bench.rs, rust/src/wal.rs, rust/src/segment_reader.rs
see_also: rust/compiler_pipeline.md, rust/convert.md, rust/bins.md
hot_paths: writer.rs, wal.rs, segment_reader.rs
architectural_health: normal
security_tier: normal
---

# rust/ — Writer, Decoder & Runtime

Subdomain: rust/
Source paths: rust/src/writer.rs, rust/src/decoder.rs, rust/src/lib.rs, rust/src/main.rs, rust/src/gen_fixtures.rs, rust/src/bench.rs, rust/src/wal.rs, rust/src/segment_reader.rs

## TASK → LOAD

| Task | Load |
|------|------|
| Emit .nxb bytes directly from typed data (hot path) | writer_decoder.md |
| Decode / inspect .nxb for testing | writer_decoder.md |
| Understand WAL append and seal pipeline | writer_decoder.md |
| Query spans across sealed segments | writer_decoder.md |
| Regenerate js/fixtures/ benchmark files | writer_decoder.md |
| Reproduce Rust benchmark numbers | writer_decoder.md |
| Add a new public module to the crate | writer_decoder.md |

---

# bench.rs

DOES: Benchmarks NXS (compiler and wire paths) against JSON, XML, and CSV serialization/deserialization across three record counts (10k, 100k, 1M). Also benchmarks the WAL pipeline: append, recover, seal, and full roundtrip with span data.
SYMBOLS:
- dataset(n: usize) -> Vec<Record>
- serialize_nxs(records: &[Record]) -> Vec<u8>
- serialize_nxs_wire(records: &[Record]) -> Vec<u8>
- serialize_json(records: &[Record]) -> Vec<u8>
- serialize_xml(records: &[Record]) -> Vec<u8>
- serialize_csv(records: &[Record]) -> Vec<u8>
- bench<F: Fn() -> R, R>(iters: u32, f: F) -> Duration
- bench_wal()
- span_dataset(n: usize) -> Vec<wal::SpanFields<'static>>
TYPE: Record { id, username, email, age, balance, active, score, created_at }
DEPENDS: crate::compiler, crate::decoder, crate::wal, crate::segment_reader
PATTERNS: warmup-then-measure, back-to-back format comparison
USE WHEN: Reproducing benchmark numbers from BENCHMARK.md or profiling regression between NXS wire vs compiler path.

---

# decoder.rs

DOES: Minimal `.nxb` reader that validates file/object magic, parses the embedded schema, and walks the first root object to return typed `DecodedValue` fields. Used by tests and the segment reader, not the hot read path.
SYMBOLS:
- decode(data: &[u8]) -> Result<DecodedFile>
- decode_record_at(data: &[u8], offset: usize, keys: &[String], sigils: &[u8]) -> Result<Vec<(String, DecodedValue)>>
TYPE: DecodedFile { version, flags, dict_hash, tail_ptr, keys, key_sigils, root_fields, record_count, tail_start, data_sector_start }
TYPE: DecodedValue (enum: Int, Float, Bool, Str, Time, Binary, Null, List, Object, Raw)
DEPENDS: crate::error
PATTERNS: validate-magic-first, LEB128-bitmask decode, tail-index for record count
USE WHEN: Inspecting or testing `.nxb` output rather than doing high-throughput column reads; prefer slot-based `NxsWriter`/`SegmentReader` on the hot path.

---

# gen_fixtures.rs

DOES: CLI binary that generates matched `.nxb`, `.json`, and `.csv` fixture files at configurable record counts for use by all language benchmarks.
SYMBOLS:
- main()
- build(n: usize) -> Vec<Rec>
- write_nxb(records: &[Rec], path: &Path)
- write_json(records: &[Rec], path: &Path)
- write_csv(records: &[Rec], path: &Path)
- ensure_out_dir_writable(out_dir: &Path)
TYPE: Rec { id, username, email, age, balance, active, score }
DEPENDS: crate::writer
PATTERNS: schema-once-write-many via NxsWriter
USE WHEN: Running `make fixtures` or needing to regenerate `js/fixtures/` before cross-language benchmarks.

---

# lib.rs

DOES: Crate root that publicly re-exports all modules so external consumers and integration tests can access them without module-path guessing.
SYMBOLS:
- Types: compiler, convert, decoder, error, lexer, parser, segment_reader, wal, writer (pub mod declarations)
DEPENDS: crate::compiler, crate::convert, crate::decoder, crate::error
PATTERNS: flat public module re-export
USE WHEN: Adding a new module to the crate or understanding which modules are publicly accessible from outside the crate.

---

# main.rs

DOES: Entry-point binary (`nxs`) that reads a `.nxs` source file, runs the full lex→parse→compile pipeline, and writes the resulting `.nxb` to disk. Contains the primary integration test suite covering all sigil types, format invariants, and writer correctness.
SYMBOLS:
- main()
- compile(source: &str) -> error::Result<Vec<u8>>
DEPENDS: crate::compiler, crate::lexer, crate::parser, crate::decoder
PATTERNS: lex-parse-compile pipeline, file-level integration tests with assert_valid_nxb
USE WHEN: Compiling a `.nxs` source file from the CLI or running `cargo test` for end-to-end format correctness.

---

# segment_reader.rs

DOES: Queries span data across a directory of sealed `.nxb` segments plus an optional live `.nxsw` WAL, building a `trace_id → offsets` index from each segment's tail-index at open time.
SYMBOLS:
- SegmentReader::open(dir: impl AsRef<Path>) -> Result<Self>
- SegmentReader::find_by_trace(&self, trace_id: u128) -> Result<Vec<Span>>
- SegmentReader::find_span(&self, trace_id: u128, span_id: u64) -> Result<Option<Span>>
- SegmentReader::find_by_time(&self, start_ns: i64, end_ns: i64) -> Result<Vec<Span>>
- SegmentReader::stats(&self) -> ReaderStats
TYPE: Span { trace_id, span_id, parent_span_id, name, service, start_time_ns, duration_ns, status_code, payload }
TYPE: ReaderStats { segment_count, sealed_records, wal_records }
DEPENDS: crate::decoder, crate::error, crate::wal
PATTERNS: tail-index-driven O(1) record lookup, WAL fallback for live records
USE WHEN: Querying spans by trace or time window after WAL sealing; chosen over direct decoder use when working with multi-segment trace storage.

---

# wal.rs

DOES: Streaming append-only WAL for span ingestion: writes NYXO records to a `.nxsw` file without a tail-index, maintains an in-memory `trace_id/span_id → offset` index, and seals to a full `.nxb` segment on demand.
SYMBOLS:
- SpanWal::open(path: impl AsRef<Path>) -> Result<Self>
- SpanWal::append(&mut self, span: &SpanFields) -> Result<u64>
- SpanWal::flush(&mut self) -> Result<()>
- SpanWal::recover(&mut self) -> Result<()>
- SpanWal::seal(&mut self, out_path: impl AsRef<Path>) -> Result<SealReport>
- SpanWal::record_count(&self) -> u64
TYPE: SpanFields<'a> { trace_id_hi, trace_id_lo, span_id, parent_span_id, name, service, start_time_ns, duration_ns, status_code, payload }
TYPE: WalEntry { trace_id, span_id, offset }
TYPE: SealReport { records, bytes_written, segment_path }
DEPENDS: crate::error, crate::writer
PATTERNS: append-only WAL, crash-recovery via linear scan, seal-to-segment
USE WHEN: Ingesting spans at high write throughput where tail-index rewrite on every append would be prohibitive; seal when the WAL reaches a size/time threshold.

---

# writer.rs

DOES: Zero-allocation hot-path `.nxb` emitter: takes a precompiled `Schema`, writes typed field values directly into a `Vec<u8>` with LEB128 bitmask and back-patched offset table per object, then finalises with preamble, schema header, and tail-index.
SYMBOLS:
- Schema::new(keys: &[&str]) -> Self
- Schema::len(&self) -> usize
- NxsWriter::new(schema: &'a Schema) -> Self
- NxsWriter::with_capacity(schema: &'a Schema, cap: usize) -> Self
- NxsWriter::begin_object(&mut self)
- NxsWriter::end_object(&mut self)
- NxsWriter::finish(self) -> Vec<u8>
- NxsWriter::write_i64(&mut self, slot: Slot, v: i64)
- NxsWriter::write_f64(&mut self, slot: Slot, v: f64)
- NxsWriter::write_bool(&mut self, slot: Slot, v: bool)
- NxsWriter::write_time(&mut self, slot: Slot, unix_ns: i64)
- NxsWriter::write_null(&mut self, slot: Slot)
- NxsWriter::write_str(&mut self, slot: Slot, v: &str)
- NxsWriter::write_bytes(&mut self, slot: Slot, data: &[u8])
- NxsWriter::write_list_i64(&mut self, slot: Slot, values: &[i64])
- NxsWriter::write_list_f64(&mut self, slot: Slot, values: &[f64])
TYPE: Schema { keys, bitmask_bytes, sigils }
TYPE: Slot(pub u16)
TYPE: NxsWriter<'a> { schema, buf, frames, record_offsets, slot_sigils }
DEPENDS: (none — self-contained)
PATTERNS: back-patch object header, slot-indexed writes, schema-once-reuse
USE WHEN: Emitting `.nxb` directly from typed data (the production hot path); use `Compiler` instead only when starting from `.nxs` source text.
