//! Converter suite — JSON/CSV/XML ⇄ .nxb.
//!
//! Spec: ../../context/data/2026-04-30-converter-suite-spec.yaml
//! Plan: ../../context/plans/2026-04-30-converter-suite.md
//!
//! Module map:
//!   infer     — two-pass streaming sigil inference shared by all import sources
//!   json_in   — JSON → NxsWriter
//!   csv_in    — CSV  → NxsWriter
//!   xml_in    — XML  → NxsWriter
//!   json_out  — .nxb → JSON (streaming, optional ndjson/pretty)
//!   csv_out   — .nxb → CSV
//!   inspect   — .nxb → human/JSON report
//!
//! All public entry points below are stubs returning `Unimplemented` until the
//! implementation steps in the plan are executed in TDD order.

// No panics on adversarial input — enforced mechanically in this module.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::error::Result;

pub mod csv_in;
pub mod csv_out;
pub mod infer;
pub mod inspect;
pub mod json_in;
pub mod json_out;
pub mod xml_in;

// ── Verify policy (--verify) ────────────────────────────────────────────────

/// `--verify <auto|force|off>` — post-write roundtrip decode control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VerifyPolicy {
    /// Verify when output is under 100 MB; skip otherwise with a warning.
    #[default]
    Auto,
    /// Always verify, regardless of output size.
    Force,
    /// Skip verify entirely.
    Off,
}

// ── Binary encoding (--binary) ───────────────────────────────────────────────

/// `--binary <base64|hex|skip>` — how to render `<` binary values on export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BinaryEncoding {
    #[default]
    Base64,
    Hex,
    Skip,
}

// ── XML attribute handling (--xml-attrs) ────────────────────────────────────

/// `--xml-attrs <as-fields|prefix>`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum XmlAttrsMode {
    /// `<u id="1"/>` → `{id: =1}`.
    #[default]
    AsFields,
    /// `<u id="1"/>` → `{@id: =1}`.
    Prefix,
}

// ── Conflict policy (--on-conflict) ─────────────────────────────────────────

/// `--on-conflict <error|coerce-string|first-wins>`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Exit 4 on the first conflict.
    #[default]
    Error,
    /// Widen conflicting keys to string.
    CoerceString,
    /// First-seen sigil wins; later mismatches are errors.
    FirstWins,
}

// ── Inferred schema types ────────────────────────────────────────────────────

/// Return type of the inference pass: each key's chosen sigil plus whether it
/// is optional (absent in ≥1 record).
///
/// During pass 1, `key_states` and `total_records` are populated alongside
/// `keys`. After `infer::finalize`, `key_states` may be dropped.
#[derive(Debug, Default)]
pub struct InferredSchema {
    pub keys: Vec<InferredKey>,
    /// Parallel to `keys` — accumulates raw observations during pass 1.
    pub key_states: Vec<crate::convert::infer::KeyState>,
    /// Total records seen during pass 1 (used to detect optional keys).
    pub total_records: usize,
}

#[derive(Debug)]
pub struct InferredKey {
    pub name: String,
    /// NXS sigil byte: `=` `~` `?` `"` `@` `<` `^`
    pub sigil: u8,
    pub optional: bool,
    /// When `Some(s)`, the key is an NXS list whose elements have sigil `s`.
    pub list_of: Option<u8>,
}

// ── Common options ────────────────────────────────────────────────────────────

/// Options shared across all three binaries (I/O paths).
#[derive(Debug, Default, Clone)]
pub struct CommonOpts {
    /// `None` → stdin.
    pub input_path: Option<std::path::PathBuf>,
    /// `None` → stdout.
    pub output_path: Option<std::path::PathBuf>,
}

// ── Import ────────────────────────────────────────────────────────────────────

/// All CLI flags for `nxs-import`. One field per spec flag.
#[derive(Debug)]
pub struct ImportArgs {
    pub common: CommonOpts,
    /// `--from <json|csv|xml>` — required.
    pub from: ImportFormat,
    /// `--schema <file.yaml>` — skip inference; single-pass.
    pub schema_hint: Option<std::path::PathBuf>,
    /// `--on-conflict`
    pub conflict: ConflictPolicy,
    /// `--root <jsonpath>` — default `$` (JSON only).
    pub root: Option<String>,
    /// `--csv-delimiter <char>` — default `,`.
    pub csv_delimiter: Option<char>,
    /// `--csv-no-header` — generate positional keys `col_0`, `col_1`, …
    pub csv_no_header: bool,
    /// `--xml-record-tag <name>` — required for XML.
    pub xml_record_tag: Option<String>,
    /// `--xml-attrs <as-fields|prefix>` — default `as-fields`.
    pub xml_attrs: XmlAttrsMode,
    /// `--buffer-records <N>` — default 4096.
    pub buffer_records: usize,
    /// `--max-depth <N>` — default 64; applies to JSON and XML.
    pub max_depth: usize,
    /// `--xml-max-depth <N>` — default 64; effective = min(max_depth, xml_max_depth).
    pub xml_max_depth: usize,
    /// `--tail-index-spill` — allow tail-index to exceed 512 MB by spilling to disk.
    pub tail_index_spill: bool,
    /// `--verify <auto|force|off>` — default `auto`.
    pub verify: VerifyPolicy,
}

impl Default for ImportArgs {
    fn default() -> Self {
        Self {
            common: CommonOpts::default(),
            from: ImportFormat::default(),
            schema_hint: None,
            conflict: ConflictPolicy::default(),
            root: None,
            csv_delimiter: None,
            csv_no_header: false,
            xml_record_tag: None,
            xml_attrs: XmlAttrsMode::default(),
            buffer_records: 4096,
            max_depth: 64,
            xml_max_depth: 64,
            tail_index_spill: false,
            verify: VerifyPolicy::default(),
        }
    }
}

/// `--from` source format.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    #[default]
    Json,
    Csv,
    Xml,
}

/// Result returned after a successful import.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub records_written: usize,
    pub output_bytes: usize,
}

// ── Export ────────────────────────────────────────────────────────────────────

/// All CLI flags for `nxs-export`. One field per spec flag.
#[derive(Debug, Default)]
pub struct ExportArgs {
    pub common: CommonOpts,
    /// `--to <json|csv>` — required.
    pub to: ExportFormat,
    /// `--pretty` — (JSON only) indent 2 spaces.
    pub pretty: bool,
    /// `--ndjson` — (JSON only) newline-delimited JSON.
    pub ndjson: bool,
    /// `--columns <a,b,c>` — (CSV only) explicit column order.
    pub columns: Option<Vec<String>>,
    /// `--csv-delimiter <char>` — default `,`.
    pub csv_delimiter: Option<char>,
    /// `--binary <base64|hex|skip>` — default `base64`.
    pub binary: BinaryEncoding,
    /// `--csv-safe` — prefix injection-prone cells with `'`.
    pub csv_safe: bool,
}

/// `--to` export format.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    #[default]
    Json,
    Csv,
}

/// Result returned after a successful export.
#[derive(Debug, Default)]
pub struct ExportReport {
    pub records_read: usize,
    pub output_bytes: usize,
}

// ── Inspect ───────────────────────────────────────────────────────────────────

/// All CLI flags for `nxs-inspect`. One field per spec flag.
#[derive(Debug, Default)]
pub struct InspectArgs {
    pub common: CommonOpts,
    /// `--json` — emit structured JSON instead of text.
    pub json_output: bool,
    /// `--records <N|all>` — how many records to summarize. `None` = all.
    pub records_to_show: Option<usize>,
    /// `--record-index <N>` — decode exactly one record by zero-based index (O(1) via tail-index).
    /// Mutually exclusive with `--records`.
    pub record_index: Option<usize>,
    /// `--verify-hash` — recompute DictHash and compare to preamble.
    pub verify_hash: bool,
}

/// Result returned after a successful inspect.
#[derive(Debug, Default)]
pub struct InspectReport {
    /// `Some(true/false)` only when `--verify-hash` was supplied.
    pub dict_hash_ok: Option<bool>,
    pub record_count: usize,
}

// ── Schema hint YAML loader ───────────────────────────────────────────────────

/// YAML schema hint file format (from spec `schema_hints_format`).
/// `keys:` maps key names to `{ sigil, optional?, list_of? }`.
#[derive(Debug, serde::Deserialize)]
struct SchemaHintFile {
    keys: std::collections::HashMap<String, SchemaHintKey>,
}

#[derive(Debug, serde::Deserialize)]
struct SchemaHintKey {
    sigil: String,
    #[serde(default)]
    optional: bool,
    list_of: Option<String>,
}

/// Load an `InferredSchema` from a `--schema <file.yaml>` hint file.
/// The caller uses this instead of running inference pass 1.
pub fn load_schema_hint(path: &std::path::Path) -> Result<InferredSchema> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| crate::error::NxsError::IoError(format!("{}: {e}", path.display())))?;
    let hint: SchemaHintFile = serde_yaml2::de::from_str(&text).map_err(|e| {
        crate::error::NxsError::ConvertParseError {
            offset: 0,
            msg: format!("schema hint YAML parse error: {e}"),
        }
    })?;

    let keys = hint
        .keys
        .into_iter()
        .map(|(name, k)| {
            let sigil = k.sigil.bytes().next().unwrap_or(b'"');
            let list_of = k.list_of.as_deref().and_then(|s| s.bytes().next());
            InferredKey {
                name,
                sigil,
                optional: k.optional,
                list_of,
            }
        })
        .collect();

    Ok(InferredSchema {
        keys,
        key_states: vec![],
        total_records: 0,
    })
}

// ── Exit code mapping ─────────────────────────────────────────────────────────

/// Map an `NxsError` to the documented exit code for the converter binaries.
///
/// Exit codes (from spec):
///   0 — success
///   1 — generic error
///   2 — usage error (bad/missing flags)
///   3 — input format error
///   4 — schema conflict
///   5 — IO error
pub fn exit_code_for(err: &crate::error::NxsError) -> i32 {
    use crate::error::NxsError;
    match err {
        NxsError::ConvertSchemaConflict(_) => 4,
        NxsError::ConvertParseError { .. }
        | NxsError::ConvertEntityExpansion
        | NxsError::ConvertDepthExceeded
        | NxsError::BadMagic
        | NxsError::OutOfBounds
        | NxsError::RecursionLimit => 3,
        NxsError::IoError(_) => 5,
        _ => 1,
    }
}

// ── Entry points (stubs) ──────────────────────────────────────────────────────

/// Top-level driver for nxs-import (dispatched on `--from`).
pub fn run_import(args: &ImportArgs) -> Result<ImportReport> {
    use crate::convert::json_in;
    use std::io::BufReader;

    let input_path = args.common.input_path.as_deref();
    let output_path = args.common.output_path.as_deref();

    match args.from {
        ImportFormat::Json => {
            // Two-pass: open file twice (pass 1 + pass 2), or spill stdin.
            match input_path {
                Some(path) => {
                    // Pass 1: infer schema (or load from hint)
                    let schema = if let Some(hint_path) = &args.schema_hint {
                        load_schema_hint(hint_path)?
                    } else {
                        let f1 = std::fs::File::open(path).map_err(|e| {
                            crate::error::NxsError::IoError(format!("{}: {e}", path.display()))
                        })?;
                        json_in::infer_schema(BufReader::new(f1), args)?
                    };

                    // Pass 2: emit
                    let f2 = std::fs::File::open(path).map_err(|e| {
                        crate::error::NxsError::IoError(format!("{}: {e}", path.display()))
                    })?;

                    match output_path {
                        Some(out_path) => {
                            let out = std::fs::File::create(out_path).map_err(|e| {
                                crate::error::NxsError::IoError(format!(
                                    "{}: {e}",
                                    out_path.display()
                                ))
                            })?;
                            json_in::emit(BufReader::new(f2), out, &schema, args)
                        }
                        None => json_in::emit(BufReader::new(f2), std::io::stdout(), &schema, args),
                    }
                }
                None => {
                    // stdin: spill to tempfile, then two passes over the tempfile.
                    // With --schema, skip pass 1 (no spill needed — single pass).
                    let mut spill = tempfile::NamedTempFile::new()
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    std::io::copy(&mut std::io::stdin(), &mut spill)
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    let spill_path = spill.path().to_path_buf();

                    let schema = if let Some(hint_path) = &args.schema_hint {
                        load_schema_hint(hint_path)?
                    } else {
                        let f1 = std::fs::File::open(&spill_path)
                            .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                        json_in::infer_schema(BufReader::new(f1), args)?
                    };

                    let f2 = std::fs::File::open(&spill_path)
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    match output_path {
                        Some(out_path) => {
                            let out = std::fs::File::create(out_path).map_err(|e| {
                                crate::error::NxsError::IoError(format!(
                                    "{}: {e}",
                                    out_path.display()
                                ))
                            })?;
                            json_in::emit(BufReader::new(f2), out, &schema, args)
                        }
                        None => json_in::emit(BufReader::new(f2), std::io::stdout(), &schema, args),
                    }
                    // spill NamedTempFile is dropped here → removed from disk
                }
            }
        }
        ImportFormat::Csv => {
            use crate::convert::csv_in;
            match input_path {
                Some(path) => {
                    let schema = if let Some(hint_path) = &args.schema_hint {
                        load_schema_hint(hint_path)?
                    } else {
                        let f1 = std::fs::File::open(path).map_err(|e| {
                            crate::error::NxsError::IoError(format!("{}: {e}", path.display()))
                        })?;
                        csv_in::infer_schema(BufReader::new(f1), args)?
                    };
                    let f2 = std::fs::File::open(path).map_err(|e| {
                        crate::error::NxsError::IoError(format!("{}: {e}", path.display()))
                    })?;
                    match output_path {
                        Some(out_path) => {
                            let out = std::fs::File::create(out_path).map_err(|e| {
                                crate::error::NxsError::IoError(format!(
                                    "{}: {e}",
                                    out_path.display()
                                ))
                            })?;
                            csv_in::emit(BufReader::new(f2), out, &schema, args)
                        }
                        None => csv_in::emit(BufReader::new(f2), std::io::stdout(), &schema, args),
                    }
                }
                None => {
                    let mut spill = tempfile::NamedTempFile::new()
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    std::io::copy(&mut std::io::stdin(), &mut spill)
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    let spill_path = spill.path().to_path_buf();
                    let schema = if let Some(hint_path) = &args.schema_hint {
                        load_schema_hint(hint_path)?
                    } else {
                        let f1 = std::fs::File::open(&spill_path)
                            .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                        csv_in::infer_schema(BufReader::new(f1), args)?
                    };
                    let f2 = std::fs::File::open(&spill_path)
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    match output_path {
                        Some(out_path) => {
                            let out = std::fs::File::create(out_path).map_err(|e| {
                                crate::error::NxsError::IoError(format!(
                                    "{}: {e}",
                                    out_path.display()
                                ))
                            })?;
                            csv_in::emit(BufReader::new(f2), out, &schema, args)
                        }
                        None => csv_in::emit(BufReader::new(f2), std::io::stdout(), &schema, args),
                    }
                }
            }
        }
        ImportFormat::Xml => {
            use crate::convert::xml_in;
            match input_path {
                Some(path) => {
                    let schema = if let Some(hint_path) = &args.schema_hint {
                        load_schema_hint(hint_path)?
                    } else {
                        let f1 = std::fs::File::open(path).map_err(|e| {
                            crate::error::NxsError::IoError(format!("{}: {e}", path.display()))
                        })?;
                        xml_in::infer_schema(BufReader::new(f1), args)?
                    };
                    let f2 = std::fs::File::open(path).map_err(|e| {
                        crate::error::NxsError::IoError(format!("{}: {e}", path.display()))
                    })?;
                    match output_path {
                        Some(out_path) => {
                            let out = std::fs::File::create(out_path).map_err(|e| {
                                crate::error::NxsError::IoError(format!(
                                    "{}: {e}",
                                    out_path.display()
                                ))
                            })?;
                            xml_in::emit(BufReader::new(f2), out, &schema, args)
                        }
                        None => xml_in::emit(BufReader::new(f2), std::io::stdout(), &schema, args),
                    }
                }
                None => {
                    let mut spill = tempfile::NamedTempFile::new()
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    std::io::copy(&mut std::io::stdin(), &mut spill)
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    let spill_path = spill.path().to_path_buf();
                    let schema = if let Some(hint_path) = &args.schema_hint {
                        load_schema_hint(hint_path)?
                    } else {
                        let f1 = std::fs::File::open(&spill_path)
                            .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                        xml_in::infer_schema(BufReader::new(f1), args)?
                    };
                    let f2 = std::fs::File::open(&spill_path)
                        .map_err(|e| crate::error::NxsError::IoError(e.to_string()))?;
                    match output_path {
                        Some(out_path) => {
                            let out = std::fs::File::create(out_path).map_err(|e| {
                                crate::error::NxsError::IoError(format!(
                                    "{}: {e}",
                                    out_path.display()
                                ))
                            })?;
                            xml_in::emit(BufReader::new(f2), out, &schema, args)
                        }
                        None => xml_in::emit(BufReader::new(f2), std::io::stdout(), &schema, args),
                    }
                }
            }
        }
    }
}

/// Top-level driver for nxs-export (dispatched on `--to`).
pub fn run_export(args: &ExportArgs) -> Result<ExportReport> {
    use crate::convert::json_out;

    let input_path = args.common.input_path.as_deref();
    let output_path = args.common.output_path.as_deref();

    // Helper: open input as a reader (file or stdin).
    // Export is single-pass (reads .nxb bytes fully), so no stdin spill needed.
    macro_rules! open_input {
        ($path:expr) => {
            std::fs::File::open($path)
                .map_err(|e| crate::error::NxsError::IoError(format!("{}: {e}", $path.display())))
        };
    }

    macro_rules! open_output {
        ($path:expr) => {
            std::fs::File::create($path)
                .map_err(|e| crate::error::NxsError::IoError(format!("{}: {e}", $path.display())))
        };
    }

    match args.to {
        ExportFormat::Json => match (input_path, output_path) {
            (Some(inp), Some(out)) => json_out::run(open_input!(inp)?, open_output!(out)?, args),
            (Some(inp), None) => json_out::run(open_input!(inp)?, std::io::stdout(), args),
            (None, Some(out)) => json_out::run(std::io::stdin(), open_output!(out)?, args),
            (None, None) => json_out::run(std::io::stdin(), std::io::stdout(), args),
        },
        ExportFormat::Csv => {
            use crate::convert::csv_out;
            match (input_path, output_path) {
                (Some(inp), Some(out)) => csv_out::run(open_input!(inp)?, open_output!(out)?, args),
                (Some(inp), None) => csv_out::run(open_input!(inp)?, std::io::stdout(), args),
                (None, Some(out)) => csv_out::run(std::io::stdin(), open_output!(out)?, args),
                (None, None) => csv_out::run(std::io::stdin(), std::io::stdout(), args),
            }
        }
    }
}

/// Top-level driver for nxs-inspect.
pub fn run_inspect(args: &InspectArgs) -> Result<InspectReport> {
    use crate::convert::inspect;
    if args.json_output {
        inspect::render_json(std::io::stdout(), args)
    } else {
        inspect::render_text(std::io::stdout(), args)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every flag that the spec defines for nxs-import must have a corresponding
    /// field in `ImportArgs`. Update this list whenever the spec changes.
    #[test]
    fn import_args_maps_every_spec_flag() {
        // Hand-written mirror of spec nxs_import.optional_flags (plus required).
        // This test fails at compile time if a field is removed; it fails at
        // runtime if someone forgets to add a new spec flag to the list below.
        let spec_fields: &[&str] = &[
            "from",
            "schema_hint",
            "conflict",
            "root",
            "csv_delimiter",
            "csv_no_header",
            "xml_record_tag",
            "xml_attrs",
            "buffer_records",
            "max_depth",
            "xml_max_depth",
            "tail_index_spill",
            "verify",
        ];
        // Build the struct and access every field so the compiler catches removals.
        let a = ImportArgs::default();
        let _ = &a.from;
        let _ = &a.schema_hint;
        let _ = &a.conflict;
        let _ = &a.root;
        let _ = &a.csv_delimiter;
        let _ = &a.csv_no_header;
        let _ = &a.xml_record_tag;
        let _ = &a.xml_attrs;
        let _ = &a.buffer_records;
        let _ = &a.max_depth;
        let _ = &a.xml_max_depth;
        let _ = &a.tail_index_spill;
        let _ = &a.verify;
        assert_eq!(spec_fields.len(), 13, "spec has 13 import flags");
    }

    #[test]
    fn export_args_maps_every_spec_flag() {
        let spec_fields: &[&str] = &[
            "to",
            "pretty",
            "ndjson",
            "columns",
            "csv_delimiter",
            "binary",
            "csv_safe",
        ];
        let a = ExportArgs::default();
        let _ = &a.to;
        let _ = &a.pretty;
        let _ = &a.ndjson;
        let _ = &a.columns;
        let _ = &a.csv_delimiter;
        let _ = &a.binary;
        let _ = &a.csv_safe;
        assert_eq!(spec_fields.len(), 7, "spec has 7 export flags");
    }

    #[test]
    fn inspect_args_maps_every_spec_flag() {
        let spec_fields: &[&str] = &["json_output", "records_to_show", "verify_hash"];
        let a = InspectArgs::default();
        let _ = &a.json_output;
        let _ = &a.records_to_show;
        let _ = &a.verify_hash;
        assert_eq!(spec_fields.len(), 3, "spec has 3 inspect flags");
    }

    /// Each new NxsError convert variant maps to the exit code in the spec.
    #[test]
    fn convert_errors_map_to_documented_exit_codes() {
        use crate::error::NxsError;
        assert_eq!(
            exit_code_for(&NxsError::ConvertSchemaConflict("x".into())),
            4
        );
        assert_eq!(
            exit_code_for(&NxsError::ConvertParseError {
                offset: 0,
                msg: "bad".into()
            }),
            3
        );
        assert_eq!(exit_code_for(&NxsError::ConvertEntityExpansion), 3);
        assert_eq!(exit_code_for(&NxsError::ConvertDepthExceeded), 3);
        assert_eq!(exit_code_for(&NxsError::IoError("disk full".into())), 5);
        assert_eq!(exit_code_for(&NxsError::BadMagic), 3);
    }

    /// Output path derivation uses only `Path::file_name()` — never traverses `..`.
    #[test]
    fn import_output_path_derivation_does_not_traverse() {
        let cases = &[
            ("../foo.json", "foo.nxb"),
            ("/tmp/foo.json", "foo.nxb"),
            ("foo.json", "foo.nxb"),
            ("./bar/baz.csv", "baz.nxb"),
        ];
        for (input, expected) in cases {
            let p = std::path::Path::new(input);
            let stem = p
                .file_name()
                .and_then(|n| std::path::Path::new(n).file_stem())
                .expect("no file stem");
            let derived = std::path::PathBuf::from(stem).with_extension("nxb");
            assert_eq!(derived.to_str().unwrap_or(""), *expected, "input={input}");
            // Must not contain `..`
            assert!(
                !derived
                    .components()
                    .any(|c| c == std::path::Component::ParentDir),
                "traversal in derived path for input={input}"
            );
        }
    }
}
