---
room: bins
subdomain: rust
source_paths: rust/src/bin/
see_also: rust/convert.md, rust/writer_decoder.md
hot_paths: nxs_import.rs, nxs_export.rs
architectural_health: normal
security_tier: normal
---

# rust/ — CLI Binaries

Subdomain: rust/
Source paths: rust/src/bin/

## TASK → LOAD

| Task | Load |
|------|------|
| Add a CLI flag to nxs-import / nxs-export / nxs-inspect | bins.md |
| Understand the nxs-trace WAL subcommand dispatch | bins.md |
| Wire up a new converter binary | bins.md |

---

# nxs_export.rs

DOES: CLI binary (`nxs-export`) that converts `.nxb` files to JSON or CSV output. Parses clap arguments, resolves format and binary-encoding flags, builds an `ExportArgs` value, and delegates to `convert::run_export`.
SYMBOLS:
- parse_export_format(s: &str) -> Result<ExportFormat, String>
- parse_binary_encoding(s: &str) -> Result<BinaryEncoding, String>
- main() -> ()
TYPE: Cli { to, pretty, ndjson, columns, csv_delimiter, binary, csv_safe, input, output }
DEPENDS: nxs::convert, clap
PATTERNS: thin-cli-wrapper, stdin-stdout-dash-convention
USE WHEN: Exporting `.nxb` data to JSON (including NDJSON) or CSV at the command line; prefer `nxs_import.rs` for the reverse direction.

---

# nxs_import.rs

DOES: CLI binary (`nxs-import`) that ingests JSON, CSV, or XML and writes a `.nxb` file. Parses clap arguments, resolves format/conflict/verify/xml-attrs flags, derives an output path, and delegates to `convert::run_import`.
SYMBOLS:
- parse_import_format(s: &str) -> Result<ImportFormat, String>
- parse_conflict(s: &str) -> Result<ConflictPolicy, String>
- parse_verify(s: &str) -> Result<VerifyPolicy, String>
- parse_xml_attrs(s: &str) -> Result<XmlAttrsMode, String>
- derive_output_path(input: &str, explicit: Option<&str>) -> Option<PathBuf>
- main() -> ()
TYPE: Cli { from, schema, on_conflict, root, csv_delimiter, csv_no_header, xml_record_tag, xml_attrs, buffer_records, max_depth, xml_max_depth, tail_index_spill, verify, input, output }
DEPENDS: nxs::convert, clap
PATTERNS: thin-cli-wrapper, stdin-stdout-dash-convention, schema-hint-single-pass
USE WHEN: Converting external data (JSON/CSV/XML) into the binary `.nxb` format; use `nxs_export.rs` for the reverse.

---

# nxs_inspect.rs

DOES: CLI binary (`nxs-inspect`) that prints a debug dump of a `.nxb` file's structure (preamble, schema, tail-index entries) in human-readable text or structured JSON. Optionally recomputes and validates the DictHash.
SYMBOLS:
- parse_records(s: &str) -> Option<usize>
- main() -> ()
TYPE: Cli { json, records, verify_hash, input }
DEPENDS: nxs::convert, nxs::convert::inspect, clap
PATTERNS: thin-cli-wrapper, dual-output-mode (text/json)
USE WHEN: Diagnosing a `.nxb` file's internal layout or verifying schema hash integrity; not a data-conversion tool.

---

# nxs_trace.rs

DOES: CLI binary (`nxs-trace`) for streaming OpenTelemetry-style span ingestion and query over a directory of `.nxb` segments backed by a WAL. Supports four subcommands: `write` (stdin NDJSON → WAL with optional auto-seal), `seal` (WAL → `.nxb` segment), `query` (by trace-id or time window), and `stats`.
SYMBOLS:
- cmd_write(dir: PathBuf, seal_every: u64) -> ()
- cmd_seal(dir: PathBuf) -> ()
- cmd_query(dir: PathBuf, trace_id: Option<String>, from: Option<i64>, to: Option<i64>) -> ()
- cmd_stats(dir: PathBuf) -> ()
- do_seal(wal: &mut SpanWal, dir: &PathBuf) -> ()
- parse_json_span(v: &Value) -> Option<ParsedSpan>
- parse_trace_id_hex(hex: &str) -> Option<u128>
- parse_trace_id_hex_parts(hex: &str) -> Option<(u64, u64)>
- die(msg: &str) -> !
TYPE: ParsedSpan { trace_id_hi, trace_id_lo, span_id, parent_span_id, name, service, start_time_ns, duration_ns, status_code }
DEPENDS: nxs::wal, nxs::segment_reader, clap, serde_json
PATTERNS: subcommand-dispatch, wal-segment-rotation, stdin-ndjson-ingestion
USE WHEN: Operating the distributed-tracing pipeline that appends to a WAL and seals it into queryable `.nxb` segments; unrelated to the format-conversion bins.
