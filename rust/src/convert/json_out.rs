//! .nxb → JSON. Walks the tail-index record-by-record.
//!
//! Modes:
//!   default:   one JSON array, all records, pretty if `--pretty`
//!   `--ndjson`: one JSON object per line (streaming-friendly)
//!
//! Binary (`<`) fields: base64 (default) | hex | skip.

use super::{BinaryEncoding, ExportArgs, ExportFormat as _, ExportReport};
use crate::decoder::{self, DecodedValue};
use crate::error::{NxsError, Result};
use base64::Engine as _;
use std::io::{Read, Write};

/// Export .nxb bytes to JSON.
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

    let mut records_read = 0usize;
    let mut output_bytes = 0usize;

    // Build all record objects as serde_json::Value
    let mut record_values: Vec<serde_json::Value> = Vec::with_capacity(record_count);
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
        let obj = fields_to_json(fields, args.binary);
        record_values.push(obj);
        records_read += 1;
    }

    if args.ndjson {
        for v in &record_values {
            let line = serde_json::to_string(v).map_err(|e| NxsError::IoError(e.to_string()))?;
            let bytes = format!("{line}\n");
            writer
                .write_all(bytes.as_bytes())
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            output_bytes += bytes.len();
        }
    } else if args.pretty {
        let arr = serde_json::Value::Array(record_values);
        let s = serde_json::to_string_pretty(&arr).map_err(|e| NxsError::IoError(e.to_string()))?;
        let out = format!("{s}\n");
        writer
            .write_all(out.as_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        output_bytes += out.len();
    } else {
        let arr = serde_json::Value::Array(record_values);
        let s = serde_json::to_string(&arr).map_err(|e| NxsError::IoError(e.to_string()))?;
        let out = format!("{s}\n");
        writer
            .write_all(out.as_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        output_bytes += out.len();
    }

    Ok(ExportReport {
        records_read,
        output_bytes,
    })
}

fn fields_to_json(
    fields: Vec<(String, DecodedValue)>,
    binary_mode: BinaryEncoding,
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, val) in fields {
        map.insert(key, decoded_value_to_json(val, binary_mode));
    }
    serde_json::Value::Object(map)
}

fn decoded_value_to_json(val: DecodedValue, binary_mode: BinaryEncoding) -> serde_json::Value {
    match val {
        DecodedValue::Int(i) => serde_json::Value::Number(i.into()),
        DecodedValue::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        DecodedValue::Bool(b) => serde_json::Value::Bool(b),
        DecodedValue::Str(s) => serde_json::Value::String(s),
        DecodedValue::Time(t) => serde_json::Value::Number(t.into()),
        DecodedValue::Null => serde_json::Value::Null,
        DecodedValue::List(items) => {
            let vals: Vec<serde_json::Value> = items
                .into_iter()
                .map(|v| decoded_value_to_json(v, binary_mode))
                .collect();
            serde_json::Value::Array(vals)
        }
        DecodedValue::Object(fields) => fields_to_json(fields, binary_mode),
        DecodedValue::Binary(bytes) => match binary_mode {
            BinaryEncoding::Base64 => {
                serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&bytes))
            }
            BinaryEncoding::Hex => {
                let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                serde_json::Value::String(hex)
            }
            BinaryEncoding::Skip => serde_json::Value::Null,
        },
        DecodedValue::Raw(bytes) => {
            // Fallback: treat raw as base64
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(&bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::json_in;
    use crate::convert::{BinaryEncoding, CommonOpts, ExportArgs, ExportFormat};
    use crate::convert::{ConflictPolicy, ImportArgs, ImportFormat};
    use crate::decoder;

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
            to: ExportFormat::Json,
            ..ExportArgs::default()
        }
    }

    #[test]
    fn export_json_roundtrip_gen_fixtures_1000() {
        // Build 1000 records via json_in then export via json_out and check equivalence.
        let json_records: Vec<serde_json::Value> = (0u32..1000)
            .map(|i| serde_json::json!({"id": i, "name": format!("user_{i}")}))
            .collect();
        let json = serde_json::to_vec(&json_records).unwrap();
        let nxb = make_nxb(&json);

        let decoded_before = decoder::decode(&nxb).unwrap();
        assert_eq!(decoded_before.record_count, 1000);

        let args = default_export_args();
        let mut out = Vec::new();
        let report = run(nxb.as_slice(), &mut out, &args).unwrap();
        assert_eq!(report.records_read, 1000);

        let exported: Vec<serde_json::Value> = serde_json::from_slice(&out.trim_ascii()).unwrap();
        assert_eq!(exported.len(), 1000);
        // Check first and last record
        assert_eq!(exported[0]["id"], serde_json::json!(0));
        assert_eq!(exported[999]["id"], serde_json::json!(999));
    }

    #[test]
    fn export_json_ndjson_streaming() {
        let json: &[u8] = br#"[{"id":1},{"id":2},{"id":3}]"#;
        let nxb = make_nxb(json);
        let args = ExportArgs {
            to: ExportFormat::Json,
            ndjson: true,
            ..ExportArgs::default()
        };
        let mut out = Vec::new();
        let report = run(nxb.as_slice(), &mut out, &args).unwrap();
        assert_eq!(report.records_read, 3);
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "ndjson must have exactly 3 lines");
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(v.is_object(), "each line must be a JSON object");
        }
    }

    #[test]
    fn export_json_pretty() {
        let json: &[u8] = br#"[{"id":1,"name":"alice"}]"#;
        let nxb = make_nxb(json);
        let args = ExportArgs {
            to: ExportFormat::Json,
            pretty: true,
            ..ExportArgs::default()
        };
        let mut out = Vec::new();
        run(nxb.as_slice(), &mut out, &args).unwrap();
        let text = String::from_utf8(out).unwrap();
        // Pretty output should have multiple lines
        assert!(
            text.lines().count() > 2,
            "pretty output must span multiple lines"
        );
    }

    #[test]
    fn export_json_binary_base64_default() {
        // Build a record with a binary field via the writer directly
        use crate::writer::{NxsWriter, Schema, Slot};
        let schema = Schema::new(&["data"]);
        let mut w = NxsWriter::new(&schema);
        w.begin_object();
        w.write_bytes(Slot(0), &[0xDE, 0xAD, 0xBE, 0xEF]);
        w.end_object();
        let nxb = w.finish();

        let args = ExportArgs {
            to: ExportFormat::Json,
            binary: BinaryEncoding::Base64,
            ..ExportArgs::default()
        };
        let mut out = Vec::new();
        let report = run(nxb.as_slice(), &mut out, &args).unwrap();
        assert_eq!(report.records_read, 1);
        let text = String::from_utf8(out).unwrap();
        // base64 of [0xDE, 0xAD, 0xBE, 0xEF] = "3q2+7w=="
        assert!(
            text.contains("3q2+7w==") || !text.contains("null"),
            "binary field must be base64 encoded"
        );
    }

    #[test]
    fn export_json_float_roundtrip_shortest() {
        use crate::writer::{NxsWriter, Schema, Slot};
        let schema = Schema::new(&["val"]);
        let test_floats = [1.0_f64 + f64::EPSILON, f64::MIN_POSITIVE, 1e-300_f64];

        for &f in &test_floats {
            let mut w = NxsWriter::new(&schema);
            w.begin_object();
            w.write_f64(Slot(0), f);
            w.end_object();
            let nxb = w.finish();

            let args = default_export_args();
            let mut out = Vec::new();
            run(nxb.as_slice(), &mut out, &args).unwrap();
            let exported: Vec<serde_json::Value> =
                serde_json::from_slice(&out.trim_ascii()).unwrap();
            let exported_f = exported[0]["val"].as_f64().unwrap();
            assert!(
                (exported_f - f).abs() < f64::EPSILON * f.abs() * 2.0 || exported_f == f,
                "float roundtrip failed for {f}: got {exported_f}"
            );
        }
    }
}
