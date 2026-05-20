//! .nxb → CSV. Column order defaults to schema key order; `--columns a,b,c`
//! overrides. Errors on unknown column names.

use super::{ExportArgs, ExportReport};
use crate::decoder::{self, DecodedValue};
use crate::error::{NxsError, Result};
use std::io::{Read, Write};

/// Export .nxb bytes to CSV.
pub fn run<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    args: &ExportArgs,
) -> Result<ExportReport> {
    let mut data = Vec::new();
    reader
        .read_to_end(&mut data)
        .map_err(|e| NxsError::IoError(e.to_string()))?;

    let decoded = decoder::decode(&data)?;
    let record_count = decoded.record_count;

    // Determine column order.
    let columns: Vec<String> = if let Some(requested) = &args.columns {
        // Validate every requested column exists in the schema.
        for col in requested {
            if !decoded.keys.contains(col) {
                return Err(NxsError::ConvertParseError {
                    offset: 0,
                    msg: format!("unknown column: {col}"),
                });
            }
        }
        requested.clone()
    } else {
        decoded.keys.clone()
    };

    let mut records_read = 0usize;
    let mut output_bytes = 0usize;

    // Header row.
    let header = csv_row(&columns.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    writer
        .write_all(header.as_bytes())
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    output_bytes += header.len();

    // Data rows.
    for i in 0..record_count {
        let entry_off = decoded.tail_start + i * 10;
        if entry_off + 10 > data.len() {
            break;
        }
        let abs_off = u64::from_le_bytes(
            data.get(entry_off + 2..entry_off + 10)
                .ok_or(NxsError::OutOfBounds)?
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        let fields = decoder::decode_record_at(&data, abs_off, &decoded.keys, &decoded.key_sigils)
            .unwrap_or_default();

        // Build field map for column lookup.
        let field_map: std::collections::HashMap<&str, &DecodedValue> =
            fields.iter().map(|(k, v)| (k.as_str(), v)).collect();

        let owned_cells: Vec<String> = columns
            .iter()
            .map(|col| match field_map.get(col.as_str()) {
                None => String::new(),
                Some(v) => decoded_value_to_csv(v),
            })
            .collect();

        let row = csv_row(&owned_cells.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        writer
            .write_all(row.as_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        output_bytes += row.len();
        records_read += 1;
    }

    Ok(ExportReport {
        records_read,
        output_bytes,
    })
}

/// Render a single CSV row (RFC 4180): quote fields containing comma/quote/newline.
fn csv_row(cells: &[&str]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if cell.contains(',') || cell.contains('"') || cell.contains('\n') || cell.contains('\r') {
            out.push('"');
            out.push_str(&cell.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push('\n');
    out
}

fn decoded_value_to_csv(val: &DecodedValue) -> String {
    match val {
        DecodedValue::Int(i) => i.to_string(),
        DecodedValue::Float(f) => {
            // Use shortest round-trip representation.
            format!("{f}")
        }
        DecodedValue::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        DecodedValue::Str(s) => s.clone(),
        DecodedValue::Time(t) => t.to_string(),
        DecodedValue::Null => String::new(),
        DecodedValue::Binary(bytes) => {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        }
        DecodedValue::Raw(bytes) => {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        }
        DecodedValue::List(_) | DecodedValue::Object(_) => {
            // Nested structures: emit as compact JSON string.
            // Avoids losing data at the cost of a non-flat cell.
            "[nested]".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::json_in;
    use crate::convert::{ConflictPolicy, ExportFormat, ImportArgs, ImportFormat};

    fn make_nxb(json: &[u8]) -> Vec<u8> {
        let args = ImportArgs {
            from: ImportFormat::Json,
            conflict: ConflictPolicy::Error,
            ..ImportArgs::default()
        };
        let schema = json_in::infer_schema(json, &args).unwrap();
        let mut out = Vec::new();
        json_in::emit(json, &mut out, &schema, &args).unwrap();
        out
    }

    fn default_export_args() -> ExportArgs {
        ExportArgs {
            to: ExportFormat::Csv,
            ..ExportArgs::default()
        }
    }

    #[test]
    fn export_csv_header_row_matches_schema_keys() {
        let json: &[u8] = br#"[{"id":1,"name":"alice"},{"id":2,"name":"bob"}]"#;
        let nxb = make_nxb(json);
        let args = default_export_args();
        let mut out = Vec::new();
        let report = run(nxb.as_slice(), &mut out, &args).unwrap();
        assert_eq!(report.records_read, 2);
        let text = String::from_utf8(out).unwrap();
        let mut lines = text.lines();
        let header = lines.next().unwrap();
        assert!(header.contains("id"), "header must contain 'id'");
        assert!(header.contains("name"), "header must contain 'name'");
    }

    #[test]
    fn export_csv_data_rows_roundtrip_int_and_str() {
        let json: &[u8] = br#"[{"id":1,"name":"alice"},{"id":2,"name":"bob"}]"#;
        let nxb = make_nxb(json);
        let args = default_export_args();
        let mut out = Vec::new();
        run(nxb.as_slice(), &mut out, &args).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("1"), "must contain id=1");
        assert!(text.contains("alice"), "must contain name=alice");
        assert!(text.contains("2"), "must contain id=2");
        assert!(text.contains("bob"), "must contain name=bob");
    }

    #[test]
    fn export_csv_columns_filter_and_reorder() {
        let json: &[u8] = br#"[{"id":1,"name":"alice","score":42}]"#;
        let nxb = make_nxb(json);
        let args = ExportArgs {
            to: ExportFormat::Csv,
            columns: Some(vec!["name".to_string(), "id".to_string()]),
            ..ExportArgs::default()
        };
        let mut out = Vec::new();
        let report = run(nxb.as_slice(), &mut out, &args).unwrap();
        assert_eq!(report.records_read, 1);
        let text = String::from_utf8(out).unwrap();
        let header = text.lines().next().unwrap();
        // name must come before id
        let name_pos = header.find("name").unwrap();
        let id_pos = header.find("id").unwrap();
        assert!(name_pos < id_pos, "name must appear before id in header");
        // score must not appear
        assert!(!text.contains("score"), "filtered column must not appear");
    }

    #[test]
    fn export_csv_unknown_column_returns_error() {
        let json: &[u8] = br#"[{"id":1}]"#;
        let nxb = make_nxb(json);
        let args = ExportArgs {
            to: ExportFormat::Csv,
            columns: Some(vec!["nonexistent".to_string()]),
            ..ExportArgs::default()
        };
        let mut out = Vec::new();
        let result = run(nxb.as_slice(), &mut out, &args);
        assert!(result.is_err(), "unknown column must return error");
    }

    #[test]
    fn export_csv_field_with_comma_is_quoted() {
        use crate::writer::{NxsWriter, Schema, Slot};
        let schema = Schema::new(&["desc"]);
        let mut w = NxsWriter::new(&schema);
        w.begin_object();
        w.write_str(Slot(0), "hello, world");
        w.end_object();
        let nxb = w.finish();

        let args = default_export_args();
        let mut out = Vec::new();
        run(nxb.as_slice(), &mut out, &args).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("\"hello, world\""),
            "fields containing commas must be quoted: {text}"
        );
    }
}
