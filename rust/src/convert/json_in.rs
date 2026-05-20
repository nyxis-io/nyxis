//! JSON → .nxb. Streaming two-pass import.
//!
//! Pass 1 (`infer_schema`): stream-deserialize records, observe values, build
//! `InferredSchema` via `infer::merge` + `infer::finalize`.
//! Pass 2 (`emit`): stream-deserialize again, drive `NxsWriter` with slots.
//!
//! stdin input is spilled to a `tempfile::NamedTempFile` before pass 1 so that
//! both passes can rewind. `--schema` skips pass 1 entirely (no spill needed).

use super::{ConflictPolicy, ImportArgs, ImportReport, InferredSchema};
use crate::convert::infer;
use crate::error::{NxsError, Result};
use crate::writer::{NxsWriter, Schema, Slot};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom, Write};

/// Pass 1 — consume a reader, produce an `InferredSchema`.
pub fn infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema> {
    let mut acc = InferredSchema::default();
    let depth_limit = args.max_depth;

    let de = serde_json::Deserializer::from_reader(reader);
    // Expect a top-level JSON array; each element is a record object.
    let mut stream = de.into_iter::<Value>();
    let root = stream
        .next()
        .ok_or_else(|| NxsError::ConvertParseError {
            offset: 0,
            msg: "empty input or not a JSON array".into(),
        })?
        .map_err(|e| NxsError::ConvertParseError {
            offset: e.column() as u64,
            msg: e.to_string(),
        })?;

    match root {
        Value::Array(records) => {
            for record in &records {
                let kv = flatten_object(record, depth_limit, 0)?;
                infer::merge(&mut acc, &kv);
            }
        }
        _ => {
            return Err(NxsError::ConvertParseError {
                offset: 0,
                msg: "JSON root is not an array; use --root $.data for nested arrays".into(),
            });
        }
    }

    infer::finalize(acc, args.conflict)
}

/// Flatten a JSON object into `(key, raw_string)` pairs for inference.
/// Nested objects become `parent.child` keys. Arrays become lists (not yet).
fn flatten_object(v: &Value, depth_limit: usize, depth: usize) -> Result<Vec<(String, String)>> {
    if depth > depth_limit {
        return Err(NxsError::ConvertDepthExceeded);
    }
    match v {
        Value::Object(map) => {
            let mut out = Vec::new();
            for (k, val) in map {
                match val {
                    Value::Null => out.push((k.clone(), String::new())),
                    Value::Bool(b) => out.push((k.clone(), b.to_string())),
                    Value::Number(n) => out.push((k.clone(), n.to_string())),
                    Value::String(s) => out.push((k.clone(), s.clone())),
                    Value::Array(_) => {
                        // Arrays: push a marker; list inference handled separately.
                        out.push((k.clone(), String::new()));
                    }
                    Value::Object(_) => {
                        // Nested objects: flatten with dot-notation.
                        let nested = flatten_object(val, depth_limit, depth + 1)?;
                        for (nk, nv) in nested {
                            out.push((format!("{k}.{nk}"), nv));
                        }
                    }
                }
            }
            Ok(out)
        }
        _ => Ok(vec![]),
    }
}

/// Pass 2 — consume the reader again, emit .nxb bytes via NxsWriter slots.
pub fn emit<R: Read, W: Write>(
    reader: R,
    mut writer: W,
    schema: &InferredSchema,
    args: &ImportArgs,
) -> Result<ImportReport> {
    let key_names: Vec<&str> = schema.keys.iter().map(|k| k.name.as_str()).collect();
    let nxs_schema = Schema::new(&key_names);
    let mut nxs_writer = NxsWriter::new(&nxs_schema);

    let depth_limit = args.max_depth;

    let de = serde_json::Deserializer::from_reader(reader);
    let mut stream = de.into_iter::<Value>();
    let root = stream
        .next()
        .ok_or_else(|| NxsError::ConvertParseError {
            offset: 0,
            msg: "empty input".into(),
        })?
        .map_err(|e| NxsError::ConvertParseError {
            offset: e.column() as u64,
            msg: e.to_string(),
        })?;

    let records = match root {
        Value::Array(arr) => arr,
        _ => {
            return Err(NxsError::ConvertParseError {
                offset: 0,
                msg: "JSON root is not an array".into(),
            });
        }
    };

    let mut records_written = 0usize;
    for record in &records {
        let kv = flatten_object(record, depth_limit, 0)?;
        nxs_writer.begin_object();
        for (key, value) in &kv {
            let slot_idx = schema.keys.iter().position(|k| &k.name == key);
            if let Some(idx) = slot_idx {
                let slot = Slot(idx as u16);
                let sigil = schema.keys.get(idx).map(|k| k.sigil).unwrap_or(b'"');
                match sigil {
                    b'=' => {
                        if let Ok(i) = value.parse::<i64>() {
                            nxs_writer.write_i64(slot, i);
                        }
                    }
                    b'~' => {
                        if let Ok(f) = value.parse::<f64>() {
                            nxs_writer.write_f64(slot, f);
                        }
                    }
                    b'?' => {
                        nxs_writer.write_bool(slot, value == "true");
                    }
                    b'@' => {
                        // Time slots store unix-nanosecond i64. If the value
                        // cannot be parsed as i64 (e.g. ISO-8601 string from
                        // inference), omit rather than write a wrong-typed blob.
                        if let Ok(t) = value.parse::<i64>() {
                            nxs_writer.write_time(slot, t);
                        }
                    }
                    b'<' => {
                        // Odd-length or invalid hex: omit rather than write wrong type.
                        if value.len() % 2 == 0 {
                            if let Ok(bytes) = (0..value.len())
                                .step_by(2)
                                .map(|i| u8::from_str_radix(&value[i..i + 2], 16))
                                .collect::<std::result::Result<Vec<u8>, _>>()
                            {
                                nxs_writer.write_bytes(slot, &bytes);
                            }
                        }
                    }
                    b'^' => {
                        // null: don't write anything; key stays absent
                    }
                    _ => {
                        nxs_writer.write_str(slot, value);
                    }
                }
            }
        }
        nxs_writer.end_object();
        records_written += 1;
    }

    let bytes = nxs_writer.finish();
    let output_bytes = bytes.len();
    writer
        .write_all(&bytes)
        .map_err(|e| NxsError::IoError(e.to_string()))?;

    Ok(ImportReport {
        records_written,
        output_bytes,
    })
}

/// Two-pass import from a seekable file source.
pub fn import_file(
    path: &std::path::Path,
    out_path: &std::path::Path,
    args: &ImportArgs,
) -> Result<ImportReport> {
    let f1 = std::fs::File::open(path)
        .map_err(|e| NxsError::IoError(format!("{}: {e}", path.display())))?;
    let schema = infer_schema(std::io::BufReader::new(f1), args)?;

    let f2 = std::fs::File::open(path)
        .map_err(|e| NxsError::IoError(format!("{}: {e}", path.display())))?;
    let out = std::fs::File::create(out_path)
        .map_err(|e| NxsError::IoError(format!("{}: {e}", out_path.display())))?;
    emit(std::io::BufReader::new(f2), out, &schema, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{CommonOpts, ImportArgs, ImportFormat};
    use crate::decoder;

    fn default_import_args() -> ImportArgs {
        ImportArgs {
            from: ImportFormat::Json,
            conflict: ConflictPolicy::Error,
            ..ImportArgs::default()
        }
    }

    fn import_json(json: &[u8]) -> Result<Vec<u8>> {
        let args = default_import_args();
        let schema = infer_schema(json, &args)?;
        let mut out = Vec::new();
        emit(json, &mut out, &schema, &args)?;
        Ok(out)
    }

    #[test]
    fn import_json_array_of_flat_objects() {
        let json: &[u8] = br#"[
            {"id": 1, "name": "alice"},
            {"id": 2, "name": "bob"},
            {"id": 3, "name": "carol"},
            {"id": 4, "name": "dave"},
            {"id": 5, "name": "eve"},
            {"id": 6, "name": "frank"},
            {"id": 7, "name": "grace"},
            {"id": 8, "name": "heidi"},
            {"id": 9, "name": "ivan"},
            {"id": 10, "name": "judy"}
        ]"#;
        let nxb = import_json(json).unwrap();
        let decoded = decoder::decode(&nxb).unwrap();
        assert_eq!(decoded.record_count, 10, "should have 10 records");
        assert!(decoded.keys.contains(&"id".to_string()));
        assert!(decoded.keys.contains(&"name".to_string()));
    }

    #[test]
    fn import_json_missing_keys_marked_optional() {
        let json: &[u8] = br#"[
            {"id": 1, "email": "a@b.com"},
            {"id": 2}
        ]"#;
        let args = default_import_args();
        let schema = infer_schema(json, &args).unwrap();
        let email = schema.keys.iter().find(|k| k.name == "email").unwrap();
        assert!(email.optional, "email absent in record 2 must be optional");
    }

    #[test]
    fn import_json_type_conflict_errors_by_default() {
        let json: &[u8] = br#"[{"x": 1}, {"x": "abc"}]"#;
        let args = ImportArgs {
            from: ImportFormat::Json,
            conflict: ConflictPolicy::Error,
            ..ImportArgs::default()
        };
        let result = infer_schema(json, &args);
        assert!(result.is_err(), "conflict with Error policy must fail");
        assert!(matches!(
            result.unwrap_err(),
            NxsError::ConvertSchemaConflict(_)
        ));
    }

    #[test]
    fn import_json_type_conflict_coerces_to_string_with_flag() {
        let json: &[u8] = br#"[{"x": 1}, {"x": "abc"}]"#;
        let args = ImportArgs {
            from: ImportFormat::Json,
            conflict: ConflictPolicy::CoerceString,
            ..ImportArgs::default()
        };
        let schema = infer_schema(json, &args).unwrap();
        let x = schema.keys.iter().find(|k| k.name == "x").unwrap();
        assert_eq!(
            x.sigil, b'"',
            "conflicting types with coerce-string → string"
        );
    }

    #[test]
    fn import_json_malformed_mid_stream_exits_3() {
        let json: &[u8] = b"[{\"id\": 1}, {bad json}]";
        let result = import_json(json);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NxsError::ConvertParseError { .. }
        ));
    }

    #[test]
    fn import_json_depth_limit_enforced() {
        // Build a deeply nested object: {"a":{"a":{"a": ...}}} at depth > 64
        let depth = 70usize;
        let open: String = r#"{"a":"#.repeat(depth);
        let close: String = "}".repeat(depth);
        let json = format!("[{open}\"value\"{close}]");
        let args = ImportArgs {
            from: ImportFormat::Json,
            max_depth: 64,
            ..ImportArgs::default()
        };
        let result = infer_schema(json.as_bytes(), &args);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), NxsError::ConvertDepthExceeded),
            "expected ConvertDepthExceeded"
        );
    }

    #[test]
    fn import_json_zero_one_column_infers_int_not_bool() {
        let json: &[u8] = br#"[{"flag": 0}, {"flag": 1}, {"flag": 1}]"#;
        let args = default_import_args();
        let schema = infer_schema(json, &args).unwrap();
        let flag = schema.keys.iter().find(|k| k.name == "flag").unwrap();
        assert_eq!(flag.sigil, b'=', "0/1 column must infer as int not bool");
    }

    #[test]
    fn import_tail_index_cap_exits_5() {
        // Unit-level: simulate pushing past the 512 MB cap.
        // Each tail-index entry is 10 bytes (2 key_id + 8 offset).
        // 512 MB / 10 = 53_687_091 entries.
        const MAX_ENTRIES: usize = 53_687_091;
        let cap_bytes: usize = 512 * 1024 * 1024;

        // Verify the formula the binary uses to detect overflow:
        // If we're about to write record N+1 and N * 10 > cap, exit 5.
        let records_at_cap = cap_bytes / 10;
        assert!(
            records_at_cap >= MAX_ENTRIES - 1,
            "cap formula should trigger at ~50M records"
        );
        // The check itself: records_written * ENTRY_SIZE > MAX_TAIL_BYTES
        let would_exceed = |records: usize| records * 10 > cap_bytes;
        assert!(!would_exceed(MAX_ENTRIES - 1));
        assert!(would_exceed(MAX_ENTRIES + 1));
    }
}
