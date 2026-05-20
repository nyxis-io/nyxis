---
room: implementations/rust_convert
source_paths: [rust/src/convert/, rust/src/bin/, rust/tests/convert/]
file_count: 14
architectural_health: normal
security_tier: normal
hot_paths: [json_in.rs, csv_in.rs, xml_in.rs]
see_also: [implementations/rust.md, spec/format.md]
---

# bin/nxs_export.rs

DOES: CLI entrypoint for .nxb → JSON/CSV export; parses flags and dispatches to convert::run_export.
SYMBOLS:
- main()
- parse_export_format(s: &str) -> Result<ExportFormat, String>
- parse_binary_encoding(s: &str) -> Result<BinaryEncoding, String>
DEPENDS: convert, clap
PATTERNS: cli-entrypoint
USE WHEN: Exporting .nxb files to JSON or CSV formats.

---

# bin/nxs_import.rs

DOES: CLI entrypoint for JSON/CSV/XML → .nxb import; parses flags, derives output path, and dispatches to convert::run_import.
SYMBOLS:
- main()
- parse_import_format(s: &str) -> Result<ImportFormat, String>
- parse_conflict(s: &str) -> Result<ConflictPolicy, String>
- parse_verify(s: &str) -> Result<VerifyPolicy, String>
- parse_xml_attrs(s: &str) -> Result<XmlAttrsMode, String>
- derive_output_path(input: &str, explicit: Option<&str>) -> Option<PathBuf>
DEPENDS: convert, clap
PATTERNS: cli-entrypoint, path-derivation
USE WHEN: Importing JSON, CSV, or XML into .nxb format.

---

# bin/nxs_inspect.rs

DOES: CLI entrypoint for .nxb debug inspection; renders structure as text (default) or JSON via convert::inspect.
SYMBOLS:
- main()
- parse_records(s: &str) -> Option<usize>
DEPENDS: convert, clap
PATTERNS: cli-entrypoint
USE WHEN: Debugging .nxb file structure, verifying DictHash, or extracting metadata.

---

# convert/csv_in.rs

DOES: Two-pass CSV importer using the csv crate; pass 1 infers schema via infer::merge, pass 2 emits .nxb with NxsWriter slots.
SYMBOLS:
- infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema>
- emit<R: Read, W: Write>(reader: R, writer: W, schema: &InferredSchema, args: &ImportArgs) -> Result<ImportReport>
DEPENDS: infer, error
PATTERNS: two-pass-import, streaming-csv
USE WHEN: Converting CSV to .nxb with configurable delimiter and optional header.

---

# convert/csv_out.rs

DOES: Single-pass .nxb → CSV exporter; reads tail-index, maps decoded fields to CSV cells with RFC 4180 quoting.
SYMBOLS:
- run<R: Read, W: Write>(reader: R, writer: W, args: &ExportArgs) -> Result<ExportReport>
- csv_row(cells: &[&str]) -> String
- decoded_value_to_csv(val: &DecodedValue) -> String
DEPENDS: decoder, error
PATTERNS: single-pass-export, rfc4180-quoting
USE WHEN: Exporting .nxb to CSV with optional column filtering and reordering.

---

# convert/infer.rs

DOES: Shared two-pass streaming sigil inference for JSON/CSV/XML importers; maintains per-key type lattice (int > float > bool > time > hex > null > string).
SYMBOLS:
- KeyState { seen_int, seen_float, seen_bool, seen_time, seen_binary_hex, seen_string, seen_null, total_records_seen_in, present_count, first_sigil }
- KeyState::observe(&mut self, raw: &str)
- KeyState::resolve_sigil(&self, policy: ConflictPolicy) -> Result<u8>
- merge(acc: &mut InferredSchema, record: &[(String, String)])
- finalize(acc: InferredSchema, policy: ConflictPolicy) -> Result<InferredSchema>
- Constants: SIGIL_INT, SIGIL_FLOAT, SIGIL_BOOL, SIGIL_TIME, SIGIL_HEX, SIGIL_NULL, SIGIL_STRING
DEPENDS: none
PATTERNS: priority-lattice, conflict-resolution
USE WHEN: Inferring record schema from heterogeneous input records.

---

# convert/inspect.rs

DOES: Renders .nxb files as human-readable text or structured JSON for debugging; supports --verify-hash and --records limit.
SYMBOLS:
- render_text<W: Write>(writer: W, args: &InspectArgs) -> Result<InspectReport>
- render_json<W: Write>(writer: W, args: &InspectArgs) -> Result<InspectReport>
- read_object_bitmask_hex(data: &[u8], off: usize) -> String
- read_input(opts: &CommonOpts) -> Result<Vec<u8>>
DEPENDS: decoder, error
PATTERNS: debug-dump, structured-output
USE WHEN: Examining .nxb file structure, schema, record count, or validating DictHash.

---

# convert/json_in.rs

DOES: Two-pass JSON importer; pass 1 infers schema from flattened objects, pass 2 emits .nxb via NxsWriter; stdin spilled to tempfile.
SYMBOLS:
- infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema>
- flatten_object(v: &Value, depth_limit: usize, depth: usize) -> Result<Vec<(String, String)>>
- emit<R: Read, W: Write>(reader: R, writer: W, schema: &InferredSchema, args: &ImportArgs) -> Result<ImportReport>
- import_file(path: &Path, out_path: &Path, args: &ImportArgs) -> Result<ImportReport>
DEPENDS: infer, writer, error
PATTERNS: two-pass-import, nested-object-flattening, stdin-spill
USE WHEN: Converting JSON arrays to .nxb with depth limit and nested object flattening.

---

# convert/json_out.rs

DOES: Single-pass .nxb → JSON exporter; reads tail-index, decodes records, renders as array (default), ndjson, or pretty JSON.
SYMBOLS:
- run<R: Read, W: Write>(reader: R, writer: W, args: &ExportArgs) -> Result<ExportReport>
- fields_to_json(fields: Vec<(String, DecodedValue)>, binary_mode: BinaryEncoding) -> Value
- decoded_value_to_json(val: DecodedValue, binary_mode: BinaryEncoding) -> Value
DEPENDS: decoder, error
PATTERNS: single-pass-export, multiple-output-formats
USE WHEN: Exporting .nxb to JSON with --pretty or --ndjson modes; configurable binary encoding (base64/hex/skip).

---

# convert/mod.rs

DOES: Core conversion module; defines ImportArgs/ExportArgs/InspectArgs, enum policies (ConflictPolicy, VerifyPolicy, BinaryEncoding, XmlAttrsMode), and top-level dispatch (run_import, run_export, run_inspect).
SYMBOLS:
- Types: VerifyPolicy, BinaryEncoding, XmlAttrsMode, ConflictPolicy, ImportArgs, ExportArgs, InspectArgs, ImportFormat, ExportFormat, CommonOpts, ImportReport, ExportReport, InspectReport, InferredSchema, InferredKey
- run_import(args: &ImportArgs) -> Result<ImportReport>
- run_export(args: &ExportArgs) -> Result<ExportReport>
- run_inspect(args: &InspectArgs) -> Result<InspectReport>
- load_schema_hint(path: &Path) -> Result<InferredSchema>
- exit_code_for(err: &NxsError) -> i32
DEPENDS: none
PATTERNS: arg-dispatch, two-pass-orchestration, schema-loading, exit-codes
USE WHEN: Understanding the top-level conversion flow or policy dispatch.

---

# convert/xml_in.rs

DOES: Two-pass XML importer using quick-xml; guards entity expansion and enforces depth limits; parses record elements and collects attributes/text into fields.
SYMBOLS:
- check_for_entity_expansion(src: &[u8]) -> Result<()>
- parse_records<B: BufRead>(reader: Reader<B>, args: &ImportArgs, record_tag: &str, on_record: impl FnMut(Vec<(String, String)>) -> Result<()>) -> Result<()>
- infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema>
- emit<R: Read, W: Write>(reader: R, writer: W, schema: &InferredSchema, args: &ImportArgs) -> Result<ImportReport>
DEPENDS: infer, error, quick_xml
PATTERNS: two-pass-import, entity-expansion-guard, depth-limit-enforcement, attribute-collection
USE WHEN: Converting XML to .nxb with DOCTYPE/entity rejection and nesting depth control.

---

# tests/convert/e2e.rs

DOES: End-to-end integration tests exercising JSON/CSV roundtrips and 10k-record import performance smoke test via cargo_bin.
SYMBOLS:
- e2e_json_roundtrip_value_equivalent()
- e2e_csv_roundtrip_value_equivalent()
- e2e_10k_records_under_threshold()
DEPENDS: assert_cmd, tempfile, serde_json
PATTERNS: roundtrip-validation, performance-smoke-test
USE WHEN: Verifying import→export→parse equivalence and advisory performance thresholds.

---

# tests/convert/exit_codes.rs

DOES: Integration tests asserting exact exit codes (0/2/3/4/5) for nxs-import, nxs-export, nxs-inspect for various error classes.
SYMBOLS:
- import_exits_2_when_from_flag_missing()
- import_exits_3_when_json_malformed()
- export_exits_3_when_nxb_bad_magic()
- inspect_exits_2_when_no_input()
DEPENDS: assert_cmd, tempfile
PATTERNS: exit-code-contract
USE WHEN: Validating error exit codes per spec.

---

# tests/convert/json_import.rs

DOES: Integration tests for nxs-import --from json; validates stdin→stdout roundtrip (with spill), schema-hint single-pass, and tempfile cleanup.
SYMBOLS:
- import_json_stdin_to_stdout_roundtrip()
- import_json_stdin_spills_to_tempfile_and_cleans_up()
- import_json_stdin_schema_hint_single_pass_no_spill()
- import_json_with_schema_hint_skips_inference()
DEPENDS: assert_cmd, tempfile
PATTERNS: stdin-spill-verification, schema-hint-path
USE WHEN: Testing JSON import via compiled binary with stdin/stdout and tempfile paths.
