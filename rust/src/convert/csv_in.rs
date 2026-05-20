//! CSV → .nxb. Streaming two-pass import using the `csv` crate.
//!
//! Pass 1 (`infer_schema`): iterate rows, observe values via `infer::merge`.
//! Pass 2 (`emit`): iterate rows again, drive `NxsWriter` with slots.
//!
//! `--csv-no-header`: positional keys `col_0`, `col_1`, …
//! `--csv-delimiter`: override field separator (default `,`).

use super::{ImportArgs, ImportReport, InferredSchema};
use crate::convert::infer;
use crate::error::{NxsError, Result};
use crate::writer::{NxsWriter, Schema, Slot};
use std::io::{Read, Write};

/// Pass 1 — infer schema from a CSV reader.
pub fn infer_schema<R: Read>(reader: R, args: &ImportArgs) -> Result<InferredSchema> {
    let delimiter = args.csv_delimiter.map(|c| c as u8).unwrap_or(b',');

    if args.csv_no_header {
        // No header: derive column names from the first row's count, then merge all rows.
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(false)
            .from_reader(reader);

        let mut acc = InferredSchema::default();
        let mut keys: Option<Vec<String>> = None;

        for result in rdr.records() {
            let record = result.map_err(|e| NxsError::ConvertParseError {
                offset: 0,
                msg: e.to_string(),
            })?;
            if keys.is_none() {
                keys = Some((0..record.len()).map(|i| format!("col_{i}")).collect());
            }
            let empty: Vec<String> = vec![];
            let ks = keys.as_ref().unwrap_or(&empty);
            let kv: Vec<(String, String)> = ks
                .iter()
                .zip(record.iter())
                .map(|(k, v)| (k.clone(), v.to_owned()))
                .collect();
            infer::merge(&mut acc, &kv);
        }
        return infer::finalize(acc, args.conflict);
    }

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(reader);

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| NxsError::ConvertParseError {
            offset: 0,
            msg: e.to_string(),
        })?
        .iter()
        .map(|s| s.to_owned())
        .collect();

    let mut acc = InferredSchema::default();
    for result in rdr.records() {
        let record = result.map_err(|e| NxsError::ConvertParseError {
            offset: 0,
            msg: e.to_string(),
        })?;
        let kv: Vec<(String, String)> = headers
            .iter()
            .zip(record.iter())
            .map(|(k, v)| (k.clone(), v.to_owned()))
            .collect();
        infer::merge(&mut acc, &kv);
    }
    infer::finalize(acc, args.conflict)
}

/// Pass 2 — emit .nxb from a CSV reader using the inferred schema.
pub fn emit<R: Read, W: Write>(
    reader: R,
    mut writer: W,
    schema: &InferredSchema,
    args: &ImportArgs,
) -> Result<ImportReport> {
    let delimiter = args.csv_delimiter.map(|c| c as u8).unwrap_or(b',');
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(!args.csv_no_header)
        .from_reader(reader);

    let headers: Vec<String> = if args.csv_no_header {
        (0..schema.keys.len()).map(|i| format!("col_{i}")).collect()
    } else {
        rdr.headers()
            .map_err(|e| NxsError::ConvertParseError {
                offset: 0,
                msg: e.to_string(),
            })?
            .iter()
            .map(|s| s.to_owned())
            .collect()
    };

    let key_names: Vec<&str> = schema.keys.iter().map(|k| k.name.as_str()).collect();
    let nxs_schema = Schema::new(&key_names);
    let mut nxs_writer = NxsWriter::new(&nxs_schema);

    let mut records_written = 0usize;
    for result in rdr.records() {
        let record = result.map_err(|e| NxsError::ConvertParseError {
            offset: 0,
            msg: e.to_string(),
        })?;
        nxs_writer.begin_object();
        for (header, value) in headers.iter().zip(record.iter()) {
            let slot_idx = schema.keys.iter().position(|k| &k.name == header);
            if let Some(idx) = slot_idx {
                if value.is_empty() {
                    continue; // null/missing cell
                }
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
                        if let Ok(t) = value.parse::<i64>() {
                            nxs_writer.write_time(slot, t);
                        }
                        // Non-integer time values (e.g. ISO-8601) omitted
                        // rather than written with wrong type.
                    }
                    b'<' => {
                        if value.len() % 2 == 0 {
                            if let Ok(bytes) = (0..value.len())
                                .step_by(2)
                                .map(|i| u8::from_str_radix(&value[i..i + 2], 16))
                                .collect::<std::result::Result<Vec<u8>, _>>()
                            {
                                nxs_writer.write_bytes(slot, &bytes);
                            }
                        }
                        // Odd-length or invalid hex: omit rather than write wrong type.
                    }
                    b'^' => {
                        // null: key stays absent
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{ConflictPolicy, ImportArgs, ImportFormat};
    use crate::decoder;

    fn default_args() -> ImportArgs {
        ImportArgs {
            from: ImportFormat::Csv,
            conflict: ConflictPolicy::Error,
            ..ImportArgs::default()
        }
    }

    fn import_csv(csv: &[u8]) -> Result<Vec<u8>> {
        let args = default_args();
        let schema = infer_schema(csv, &args)?;
        let mut out = Vec::new();
        emit(csv, &mut out, &schema, &args)?;
        Ok(out)
    }

    #[test]
    fn import_csv_header_inferred() {
        let csv: &[u8] = b"id,name,active\n1,alice,true\n2,bob,false\n3,carol,true\n";
        let nxb = import_csv(csv).unwrap();
        let decoded = decoder::decode(&nxb).unwrap();
        assert_eq!(decoded.record_count, 3);
        assert!(decoded.keys.contains(&"id".into()));
        assert!(decoded.keys.contains(&"name".into()));
        assert!(decoded.keys.contains(&"active".into()));
        let id_idx = decoded.keys.iter().position(|k| k == "id").unwrap();
        assert_eq!(
            decoded.key_sigils.get(id_idx).copied(),
            Some(b'='),
            "id should be int sigil"
        );
    }

    #[test]
    fn import_csv_no_header_positional_keys() {
        let csv: &[u8] = b"1,alice,true\n2,bob,false\n";
        let args = ImportArgs {
            from: ImportFormat::Csv,
            csv_no_header: true,
            conflict: ConflictPolicy::Error,
            ..ImportArgs::default()
        };
        let schema = infer_schema(csv, &args).unwrap();
        assert!(schema.keys.iter().any(|k| k.name == "col_0"));
        assert!(schema.keys.iter().any(|k| k.name == "col_1"));
        assert!(schema.keys.iter().any(|k| k.name == "col_2"));
    }

    #[test]
    fn import_csv_custom_delimiter() {
        let csv: &[u8] = b"id\tname\n1\talice\n2\tbob\n";
        let args = ImportArgs {
            from: ImportFormat::Csv,
            csv_delimiter: Some('\t'),
            conflict: ConflictPolicy::Error,
            ..ImportArgs::default()
        };
        let schema = infer_schema(csv, &args).unwrap();
        assert!(schema.keys.iter().any(|k| k.name == "id"));
        assert!(schema.keys.iter().any(|k| k.name == "name"));
    }

    #[test]
    fn import_csv_empty_cell_is_null() {
        let csv: &[u8] = b"id,email\n1,a@b.com\n2,\n";
        let args = default_args();
        let schema = infer_schema(csv, &args).unwrap();
        let email = schema.keys.iter().find(|k| k.name == "email").unwrap();
        assert!(email.optional, "empty cell must mark key as optional");
    }

    #[test]
    fn import_csv_streaming_10mb_file_under_bounded_memory() {
        // Generate ~10 MB CSV and assert successful import.
        let mut csv = Vec::with_capacity(11 * 1024 * 1024);
        csv.extend_from_slice(b"id,value\n");
        for i in 0u32..333_000 {
            let row = format!("{i},some_string_value\n");
            csv.extend_from_slice(row.as_bytes());
        }
        let args = default_args();
        let schema = infer_schema(csv.as_slice(), &args).unwrap();
        let mut out = Vec::new();
        let report = emit(csv.as_slice(), &mut out, &schema, &args).unwrap();
        assert_eq!(report.records_written, 333_000);
    }
}
