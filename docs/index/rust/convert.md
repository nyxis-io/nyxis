---
room: convert
subdomain: rust
source_paths: rust/src/convert/
see_also: rust/bins.md, rust/writer_decoder.md
hot_paths: mod.rs, json_in.rs, xml_in.rs
architectural_health: normal
security_tier: sensitive
committee_notes: xml_in.rs hard-rejects DOCTYPE/ENTITY declarations to prevent entity-expansion attacks; this room handles untrusted external data.
---

# rust/ — Format Conversion Pipeline

Subdomain: rust/
Source paths: rust/src/convert/

## TASK → LOAD

| Task | Load |
|------|------|
| Import JSON / CSV / XML into .nxb | convert.md |
| Export .nxb to JSON or CSV | convert.md |
| Inspect a .nxb file's schema and layout | convert.md |
| Add a new import or export format | convert.md |
| Understand CLI argument structs (ImportArgs, ExportArgs) | convert.md |
| Map an NxsError to an exit code | convert.md |
| Add schema-hint / single-pass import | convert.md |

---

# csv_in.rs

DOES: CSV-to-.nxb two-pass streaming import. Pass 1 (`infer_schema`) observes rows to build a sigil-typed schema; pass 2 (`emit`) drives `NxsWriter` with the frozen schema to produce `.nxb` output.
SYMBOLS:
- infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema>
- emit<R: Read, W: Write>(reader: R, writer: W, schema: &InferredSchema, args: &ImportArgs) -> Result<ImportReport>
DEPENDS: crate::convert::infer, crate::error, crate::writer
PATTERNS: two-pass streaming import, positional-keys fallback
USE WHEN: Importing delimited text files into `.nxb`; prefer over `json_in` when the source is tabular CSV/TSV.

---

# csv_out.rs

DOES: `.nxb`-to-CSV single-pass export. Walks the tail-index via `decoder::decode_record_at`, renders each record as an RFC 4180 row, and supports `--columns` filtering/reordering.
SYMBOLS:
- run<R: Read, W: Write>(reader: R, writer: W, args: &ExportArgs) -> Result<ExportReport>
DEPENDS: crate::decoder, crate::error
PATTERNS: tail-index walk, RFC 4180 quoting
USE WHEN: Exporting `.nxb` to CSV; choose `json_out` instead when the consumer needs nested structures or type fidelity beyond flat columns.

---

# infer.rs

DOES: Two-pass streaming sigil inference shared by all three import sources. Maintains a per-key `KeyState` lattice during pass 1 and collapses it to a final sigil in `finalize`, respecting `ConflictPolicy`.
SYMBOLS:
- merge(acc: &mut InferredSchema, record: &[(String, String)]) -> ()
- finalize(acc: InferredSchema, policy: ConflictPolicy) -> Result<InferredSchema>
- KeyState::observe(&mut self, raw: &str) -> ()
- KeyState::resolve_sigil(&self, policy: ConflictPolicy) -> Result<u8>
TYPE: KeyState { seen_int, seen_float, seen_bool, seen_time, seen_binary_hex, seen_string, seen_null, total_records_seen_in, present_count, first_sigil }
DEPENDS: crate::convert, crate::error
PATTERNS: lattice-based type inference, conflict policies (Error / CoerceString / FirstWins)
USE WHEN: Adding a new import source that needs sigil inference; call `merge` per record then `finalize` once before pass 2.

---

# inspect.rs

DOES: `.nxb`-to-human/JSON report for `nxs-inspect`. Decodes the file via `decoder::decode` and renders schema metadata, key list, and per-record offsets as plain text or structured JSON matching the spec's `inspect_json_schema`.
SYMBOLS:
- render_text<W: Write>(writer: W, args: &InspectArgs) -> Result<InspectReport>
- render_json<W: Write>(writer: W, args: &InspectArgs) -> Result<InspectReport>
DEPENDS: crate::decoder, crate::error, crate::convert
PATTERNS: tail-index walk, optional --verify-hash via decoder DictMismatch propagation
USE WHEN: Diagnosing a `.nxb` file's structure or verifying its DictHash; use `render_json` for machine-readable output, `render_text` for human review.

---

# json_in.rs

DOES: JSON array-to-.nxb two-pass streaming import. Pass 1 (`infer_schema`) flattens objects (dot-notation for nested keys) and infers sigils; pass 2 (`emit`) drives `NxsWriter`. Spills stdin to a `tempfile` when no seekable file path is available.
SYMBOLS:
- infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema>
- emit<R: Read, W: Write>(reader: R, writer: W, schema: &InferredSchema, args: &ImportArgs) -> Result<ImportReport>
- import_file(path: &Path, out_path: &Path, args: &ImportArgs) -> Result<ImportReport>
DEPENDS: crate::convert::infer, crate::error, crate::writer
PATTERNS: two-pass streaming import, stdin spill to tempfile, dot-notation key flattening
USE WHEN: Importing a JSON array of objects into `.nxb`; use `import_file` for file-to-file conversion, `infer_schema` + `emit` for in-process pipelines.

---

# json_out.rs

DOES: `.nxb`-to-JSON single-pass export. Supports array output, `--pretty` indentation, `--ndjson` streaming mode, and `--binary base64|hex|skip` for binary fields.
SYMBOLS:
- run<R: Read, W: Write>(reader: R, writer: W, args: &ExportArgs) -> Result<ExportReport>
DEPENDS: crate::decoder, crate::error, crate::convert
PATTERNS: tail-index walk, ndjson streaming, binary encoding dispatch
USE WHEN: Exporting `.nxb` to JSON; choose `--ndjson` for streaming-friendly output or `csv_out` when the consumer is a spreadsheet tool.

---

# mod.rs

DOES: Converter suite module root. Defines all shared CLI argument structs, policy enums, `InferredSchema`, and the three top-level entry-point functions (`run_import`, `run_export`, `run_inspect`) that dispatch to the per-format sub-modules.
SYMBOLS:
- run_import(args: &ImportArgs) -> Result<ImportReport>
- run_export(args: &ExportArgs) -> Result<ExportReport>
- run_inspect(args: &InspectArgs) -> Result<InspectReport>
- load_schema_hint(path: &Path) -> Result<InferredSchema>
- exit_code_for(err: &NxsError) -> i32
- Types: ImportArgs, ExportArgs, InspectArgs, ImportReport, ExportReport, InspectReport, ImportFormat, ExportFormat, VerifyPolicy, BinaryEncoding, XmlAttrsMode, ConflictPolicy, InferredSchema, InferredKey, CommonOpts
DEPENDS: crate::error, crate::convert::json_in, crate::convert::csv_in, crate::convert::xml_in
PATTERNS: two-pass stdin spill via tempfile, format dispatch, exit-code mapping
USE WHEN: Wiring a new CLI flag — add its field here; choosing the right entry point for a binary — call `run_import`/`run_export`/`run_inspect` rather than sub-module functions directly.

---

# xml_in.rs

DOES: XML-to-.nxb two-pass streaming import using `quick-xml`. Extracts records identified by `--xml-record-tag`, maps attributes as fields per `--xml-attrs`, flattens nested child elements to dot-notation keys, and hard-rejects DOCTYPE/ENTITY declarations to prevent entity-expansion attacks.
SYMBOLS:
- infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema>
- emit<R: Read, W: Write>(reader: R, writer: W, schema: &InferredSchema, args: &ImportArgs) -> Result<ImportReport>
DEPENDS: crate::convert::infer, crate::error, crate::writer
PATTERNS: two-pass streaming import, entity-expansion guard, dot-notation key flattening, depth-limit enforcement
USE WHEN: Importing XML feeds into `.nxb`; requires `--xml-record-tag`; prefer `json_in` when the source can be pre-converted to JSON to avoid XML-specific security concerns.
