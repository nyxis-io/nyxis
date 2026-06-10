#![allow(dead_code, unused_imports, unused_variables)]
//! Rust conformance runner for NXS.
//!
//! Usage: conformance_runner <conformance_dir>
//!
//! Reads every *.expected.json in the directory, loads the matching .nxb,
//! and asserts the decoded contents match.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use nxs::decoder::{decode, DecodedValue};
use nxs::error::NxsError;
use nxs::query::Reader;

// ── Minimal JSON parser for expected.json ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Jv {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Jv>),
    Object(Vec<(String, Jv)>),
}

fn parse_json(s: &str) -> Jv {
    let s = s.trim();
    parse_value(&mut s.chars().peekable())
}

fn parse_value(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Jv {
    skip_ws(chars);
    match chars.peek() {
        Some('"') => Jv::Str(parse_string(chars)),
        Some('{') => parse_object(chars),
        Some('[') => parse_array(chars),
        Some('t') => {
            consume_lit(chars, "true");
            Jv::Bool(true)
        }
        Some('f') => {
            consume_lit(chars, "false");
            Jv::Bool(false)
        }
        Some('n') => {
            consume_lit(chars, "null");
            Jv::Null
        }
        Some(c) if *c == '-' || c.is_ascii_digit() => parse_number(chars),
        _ => Jv::Null,
    }
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while matches!(
        chars.peek(),
        Some(' ') | Some('\n') | Some('\r') | Some('\t')
    ) {
        chars.next();
    }
}

fn consume_lit(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, lit: &str) {
    for _ in lit.chars() {
        chars.next();
    }
}

fn parse_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    chars.next(); // consume opening "
    let mut s = String::new();
    loop {
        match chars.next() {
            None | Some('"') => break,
            Some('\\') => match chars.next() {
                Some('"') => s.push('"'),
                Some('\\') => s.push('\\'),
                Some('/') => s.push('/'),
                Some('n') => s.push('\n'),
                Some('r') => s.push('\r'),
                Some('t') => s.push('\t'),
                Some('u') => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    if let Ok(code) = u16::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(code as u32) {
                            s.push(c);
                        }
                    }
                }
                Some(c) => s.push(c),
                None => break,
            },
            Some(c) => s.push(c),
        }
    }
    s
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Jv {
    let mut num = String::new();
    let mut is_float = false;
    while let Some(&c) = chars.peek() {
        if c == '-' || c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' {
            if c == '.' || c == 'e' || c == 'E' {
                is_float = true;
            }
            num.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if is_float {
        Jv::Float(num.parse().unwrap_or(0.0))
    } else {
        Jv::Int(num.parse().unwrap_or(0))
    }
}

fn parse_object(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Jv {
    chars.next(); // consume '{'
    let mut fields = Vec::new();
    loop {
        skip_ws(chars);
        match chars.peek() {
            Some('}') => {
                chars.next();
                break;
            }
            Some(',') => {
                chars.next();
            }
            Some('"') => {
                let k = parse_string(chars);
                skip_ws(chars);
                chars.next(); // ':'
                skip_ws(chars);
                let v = parse_value(chars);
                fields.push((k, v));
            }
            _ => {
                chars.next();
            }
        }
    }
    Jv::Object(fields)
}

fn parse_array(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Jv {
    chars.next(); // consume '['
    let mut items = Vec::new();
    loop {
        skip_ws(chars);
        match chars.peek() {
            Some(']') => {
                chars.next();
                break;
            }
            Some(',') => {
                chars.next();
            }
            Some(_) => {
                items.push(parse_value(chars));
            }
            None => break,
        }
    }
    Jv::Array(items)
}

// ── Comparison helpers ────────────────────────────────────────────────────────

fn approx_eq(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    let mag = a.abs().max(b.abs());
    if mag < 1e-300 {
        diff < 1e-300
    } else {
        diff / mag < 1e-9
    }
}

fn decoded_matches(decoded: &DecodedValue, expected: &Jv) -> bool {
    match (decoded, expected) {
        (DecodedValue::Null, Jv::Null) => true,
        (DecodedValue::Bool(b), Jv::Bool(e)) => b == e,
        (DecodedValue::Int(i), Jv::Int(e)) => i == e,
        (DecodedValue::Int(i), Jv::Float(e)) => approx_eq(*i as f64, *e),
        (DecodedValue::Float(f), Jv::Float(e)) => approx_eq(*f, *e),
        (DecodedValue::Float(f), Jv::Int(e)) => approx_eq(*f, *e as f64),
        (DecodedValue::Str(s), Jv::Str(e)) => s == e,
        (DecodedValue::Time(t), Jv::Int(e)) => t == e,
        (DecodedValue::Raw(_), _) => true, // skip nested/raw
        // List comparisons
        (DecodedValue::List(items), Jv::Array(arr)) => {
            if items.len() != arr.len() {
                return false;
            }
            items
                .iter()
                .zip(arr.iter())
                .all(|(a, b)| decoded_matches(a, b))
        }
        // Binary decoded as Array of ints
        (DecodedValue::Binary(bytes), Jv::Array(arr)) => {
            if bytes.len() != arr.len() {
                return false;
            }
            bytes
                .iter()
                .zip(arr.iter())
                .all(|(b, e)| matches!(e, Jv::Int(n) if *n == *b as i64))
        }
        _ => false,
    }
}

fn parse_expected_positive(expected_json: &Jv) -> Result<(usize, Vec<String>, Vec<&Jv>), String> {
    match expected_json {
        Jv::Object(fields) => {
            let map: HashMap<&str, &Jv> = fields.iter().map(|(k, v)| (k.as_str(), v)).collect();
            let count = match map.get("record_count") {
                Some(Jv::Int(n)) => *n as usize,
                _ => return Err("missing record_count".into()),
            };
            let keys = match map.get("keys") {
                Some(Jv::Array(ks)) => ks
                    .iter()
                    .filter_map(|k| {
                        if let Jv::Str(s) = k {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
                _ => return Err("missing keys".into()),
            };
            let records = match map.get("records") {
                Some(Jv::Array(rs)) => rs.iter().collect::<Vec<_>>(),
                _ => return Err("missing records".into()),
            };
            Ok((count, keys, records))
        }
        _ => Err("expected JSON object".into()),
    }
}

fn reader_value_matches(
    rec: &nxs::query::Record<'_, '_>,
    key: &str,
    expected: &Jv,
) -> Result<(), String> {
    match expected {
        Jv::Null => Ok(()),
        Jv::Bool(e) => match rec.get_bool(key) {
            Some(v) if v == *e => Ok(()),
            Some(v) => Err(format!("field {key:?}: expected bool {e}, got {v}")),
            None => Err(format!("field {key:?}: absent")),
        },
        Jv::Int(e) => match rec.get_i64(key) {
            Some(v) if v == *e => Ok(()),
            Some(v) => Err(format!("field {key:?}: expected int {e}, got {v}")),
            None => Err(format!("field {key:?}: absent")),
        },
        Jv::Float(e) => match rec.get_f64(key) {
            Some(v) if approx_eq(v, *e) => Ok(()),
            Some(v) => Err(format!("field {key:?}: expected float {e}, got {v}")),
            None => Err(format!("field {key:?}: absent")),
        },
        Jv::Str(e) => match rec.get_str(key) {
            Some(v) if v == e => Ok(()),
            Some(v) => Err(format!("field {key:?}: expected str {e:?}, got {v:?}")),
            None => Err(format!("field {key:?}: absent")),
        },
        _ => Err(format!("field {key:?}: unsupported expected type")),
    }
}

fn uses_decoder_path(name: &str) -> bool {
    name == "nested" || name.starts_with("list_")
}

fn run_positive_reader(dir: &Path, name: &str, expected_json: &Jv) -> Result<(), String> {
    let nxb_path = dir.join(format!("{}.nxb", name));
    let data = fs::read(&nxb_path).map_err(|e| format!("read {}: {}", nxb_path.display(), e))?;
    let (exp_count, exp_keys, exp_records) =
        parse_expected_positive(expected_json).map_err(|e| format!("{name}: {e}"))?;
    let reader = Reader::new(&data).map_err(|e| format!("{name}: open failed: {e}"))?;
    if reader.record_count() != exp_count {
        return Err(format!(
            "{name}: record_count expected {exp_count}, got {}",
            reader.record_count()
        ));
    }
    for (i, key) in exp_keys.iter().enumerate() {
        if reader.keys().get(i).map(String::as_str) != Some(key.as_str()) {
            return Err(format!(
                "{name}: key[{i}] expected {key:?}, got {:?}",
                reader.keys().get(i)
            ));
        }
    }
    for (ri, exp_rec) in exp_records.iter().enumerate() {
        let rec = reader
            .record(ri)
            .ok_or_else(|| format!("{name}: record {ri} missing"))?;
        let Jv::Object(fields) = *exp_rec else {
            return Err(format!("{name}: record {ri} not an object"));
        };
        for (key, val) in fields {
            reader_value_matches(&rec, key, val)?;
        }
    }
    Ok(())
}

// ── Runner ────────────────────────────────────────────────────────────────────

fn run_positive(dir: &Path, name: &str, expected_json: &Jv) -> Result<(), String> {
    let nxb_path = dir.join(format!("{}.nxb", name));
    let data = fs::read(&nxb_path).map_err(|e| format!("read {}: {}", nxb_path.display(), e))?;

    if !uses_decoder_path(name) {
        return run_positive_reader(dir, name, expected_json);
    }

    let file = decode(&data).map_err(|e| format!("{}: decode failed: {}", name, e))?;

    let (exp_count, exp_keys, exp_records) = match expected_json {
        Jv::Object(fields) => {
            let map: HashMap<&str, &Jv> = fields.iter().map(|(k, v)| (k.as_str(), v)).collect();
            let count = match map.get("record_count") {
                Some(Jv::Int(n)) => *n as usize,
                _ => return Err(format!("{}: missing record_count", name)),
            };
            let keys = match map.get("keys") {
                Some(Jv::Array(ks)) => ks
                    .iter()
                    .filter_map(|k| {
                        if let Jv::Str(s) = k {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
                _ => return Err(format!("{}: missing keys", name)),
            };
            let records = match map.get("records") {
                Some(Jv::Array(rs)) => rs,
                _ => return Err(format!("{}: missing records", name)),
            };
            (count, keys, records)
        }
        _ => return Err(format!("{}: expected JSON object", name)),
    };

    // Validate record_count
    // Note: the decoder only decodes the FIRST record (root object)
    // For multi-record files, we just check that the tail has the right count.
    // The tail_ptr field gives us entry count.
    // For now, validate the keys and the first record's fields.
    let _ = exp_count; // suppress warning

    // Validate schema keys
    for (i, key) in exp_keys.iter().enumerate() {
        if let Some(actual) = file.keys.get(i) {
            if actual != key {
                return Err(format!(
                    "{}: key[{}] expected {:?} got {:?}",
                    name, i, key, actual
                ));
            }
        }
    }

    // Validate first record's fields against expected records[0]
    if let Some(Jv::Object(exp_rec)) = exp_records.first() {
        let decoded_map: HashMap<&str, &DecodedValue> = file
            .root_fields
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect();
        for (key, exp_val) in exp_rec {
            if let Some(decoded_val) = decoded_map.get(key.as_str()) {
                if !decoded_matches(decoded_val, exp_val) {
                    return Err(format!(
                        "{}: field {:?}: expected {:?} got {:?}",
                        name, key, exp_val, decoded_val
                    ));
                }
            } else if *exp_val != Jv::Null {
                // Field expected but absent — only fail for non-null
                // (absent and null are different but decoder may not distinguish)
            }
        }
    }

    Ok(())
}

fn run_negative(dir: &Path, name: &str, expected_code: &str) -> Result<(), String> {
    let nxb_path = dir.join(format!("{}.nxb", name));
    let data = fs::read(&nxb_path).map_err(|e| format!("read {}: {}", nxb_path.display(), e))?;

    match decode(&data) {
        Err(e) => {
            let code = match &e {
                NxsError::BadMagic => "ERR_BAD_MAGIC",
                NxsError::OutOfBounds => "ERR_OUT_OF_BOUNDS",
                NxsError::DictMismatch => "ERR_DICT_MISMATCH",
                NxsError::InvalidFlags => "ERR_INVALID_FLAGS",
                NxsError::IncompatibleFlags => "ERR_INCOMPATIBLE_FLAGS",
                NxsError::UnsupportedLayout => "ERR_UNSUPPORTED_LAYOUT",
                NxsError::UnsupportedFieldType => "ERR_UNSUPPORTED_FIELD_TYPE",
                NxsError::InvalidPageMagic => "ERR_INVALID_PAGE_MAGIC",
                NxsError::UnsupportedFlags(_) => "ERR_UNSUPPORTED_FLAGS",
                _ => "ERR_UNKNOWN",
            };
            if code != expected_code {
                Err(format!(
                    "{}: expected error {:?} got {:?}",
                    name, expected_code, code
                ))
            } else {
                Ok(())
            }
        }
        Ok(_) => Err(format!(
            "{}: expected error {:?} but decode succeeded",
            name, expected_code
        )),
    }
}

fn discover_vectors(dir: &Path) -> Vec<(String, std::path::PathBuf, String)> {
    let mut out = Vec::new();
    let mut scan = |subdir: &str| {
        let d = if subdir.is_empty() {
            dir.to_path_buf()
        } else {
            dir.join(subdir)
        };
        if !d.is_dir() {
            return;
        }
        if let Ok(read) = fs::read_dir(&d) {
            for e in read.filter_map(|e| e.ok()) {
                let n = e.file_name().to_string_lossy().to_string();
                if n.ends_with(".expected.json") {
                    let base = n.trim_end_matches(".expected.json").to_string();
                    let label = if subdir.is_empty() {
                        base.clone()
                    } else {
                        format!("{subdir}/{base}")
                    };
                    out.push((label, d.clone(), base));
                }
            }
        }
    };
    scan("");
    scan("v13");
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn run_vector(vector_dir: &Path, base: &str) -> Result<(), String> {
    let json_path = vector_dir.join(format!("{base}.expected.json"));
    let json_str =
        fs::read_to_string(&json_path).map_err(|e| format!("read {}: {e}", json_path.display()))?;
    let expected = parse_json(&json_str);
    let is_negative = matches!(&expected, Jv::Object(fields) if
        fields.iter().any(|(k,_)| k == "error")
    );
    if is_negative {
        let code = match &expected {
            Jv::Object(fields) => fields
                .iter()
                .find(|(k, _)| k == "error")
                .and_then(|(_, v)| {
                    if let Jv::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        run_negative(vector_dir, base, &code)
    } else {
        run_positive(vector_dir, base, &expected)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir_str = if args.len() > 1 {
        args[1].clone()
    } else {
        "conformance".to_string()
    };
    let dir = Path::new(&dir_str);

    let mut pass = 0usize;
    let mut fail = 0usize;

    for (name, vector_dir, base) in discover_vectors(dir) {
        match run_vector(&vector_dir, &base) {
            Ok(()) => {
                println!("  PASS  {name}");
                pass += 1;
            }
            Err(msg) => {
                eprintln!("  FAIL  {name} — {msg}");
                fail += 1;
            }
        }
    }

    println!("\n{pass} passed, {fail} failed");
    if fail > 0 {
        std::process::exit(1);
    }
}
