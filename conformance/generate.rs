#![allow(dead_code, unused_imports, unused_variables)]
//! gen_conformance — generates all NXS conformance test vectors.
//!
//! Usage:  cargo run --release --bin gen_conformance -- <output_dir>
//!
//! Writes <name>.nxb + <name>.expected.json pairs for every positive vector,
//! and the three negative vectors (bad_magic, bad_dict_hash, truncated).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write as IoWrite;
use std::path::Path;

// Bring in the library types
use nxs::writer::{NxsWriter, Schema, Slot};

// ── JSON value enum ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum JV {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<JV>),
    Object(Vec<(String, JV)>),
}

impl JV {
    fn to_json(&self) -> String {
        match self {
            JV::Null => "null".to_string(),
            JV::Bool(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            JV::Int(i) => i.to_string(),
            JV::Float(f) => {
                if f.is_nan() {
                    "null".to_string()
                } else if f.is_infinite() {
                    "null".to_string()
                } else {
                    // Use enough precision to round-trip
                    let s = format!("{:.17}", f);
                    // Trim trailing zeros after decimal point
                    let s = s.trim_end_matches('0');
                    let s = s.trim_end_matches('.');
                    if s.is_empty() || s == "-" {
                        "0.0".to_string()
                    } else if !s.contains('.') && !s.contains('e') {
                        format!("{}.0", s)
                    } else {
                        s.to_string()
                    }
                }
            }
            JV::Str(s) => {
                let mut out = String::from('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => {
                            out.push_str(&format!("\\u{:04x}", c as u32));
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
                out
            }
            JV::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_json()).collect();
                format!("[{}]", items.join(","))
            }
            JV::Object(fields) => {
                let items: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}:{}", JV::Str(k.clone()).to_json(), v.to_json()))
                    .collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }
}

// ── Vector descriptor ─────────────────────────────────────────────────────────

struct Vector {
    name: &'static str,
    nxb: Vec<u8>,
    expected: String,
}

// ── Pretty-print expected JSON ────────────────────────────────────────────────

fn expected_json(keys: &[&str], records: &[Vec<Option<(&str, JV)>>]) -> String {
    // records: each element is a list of (key, value) pairs (None = absent)
    let key_arr = keys
        .iter()
        .map(|k| JV::Str(k.to_string()).to_json())
        .collect::<Vec<_>>()
        .join(",");

    let rec_arr = records
        .iter()
        .map(|rec| {
            let fields: Vec<String> = rec
                .iter()
                .filter_map(|f| f.as_ref())
                .map(|(k, v)| format!("{}:{}", JV::Str(k.to_string()).to_json(), v.to_json()))
                .collect();
            format!("{{{}}}", fields.join(","))
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "{{\"record_count\":{},\"keys\":[{}],\"records\":[{}]}}",
        records.len(),
        key_arr,
        rec_arr
    )
}

// ── Positive vector helpers ───────────────────────────────────────────────────

fn make_minimal() -> Vector {
    let schema = Schema::new(&["id", "name", "active"]);
    let mut w = NxsWriter::new(&schema);

    w.begin_object();
    w.write_i64(Slot(0), 42);
    w.write_str(Slot(1), "hello");
    w.write_bool(Slot(2), true);
    w.end_object();

    let nxb = w.finish();
    let expected = expected_json(
        &["id", "name", "active"],
        &[vec![
            Some(("id", JV::Int(42))),
            Some(("name", JV::Str("hello".into()))),
            Some(("active", JV::Bool(true))),
        ]],
    );
    Vector {
        name: "minimal",
        nxb,
        expected,
    }
}

fn make_all_sigils() -> Vector {
    let schema = Schema::new(&[
        "i64_val",
        "f64_val",
        "bool_val",
        "str_val",
        "time_val",
        "bytes_val",
    ]);
    let mut w = NxsWriter::new(&schema);

    w.begin_object();
    w.write_i64(Slot(0), -9876543210_i64);
    w.write_f64(Slot(1), 3.14159265358979);
    w.write_bool(Slot(2), false);
    w.write_str(Slot(3), "sigil test");
    w.write_time(Slot(4), 1_700_000_000_000_000_000_i64);
    w.write_bytes(Slot(5), &[0xDE, 0xAD, 0xBE, 0xEF]);
    w.end_object();

    let nxb = w.finish();
    // bytes_val is omitted from the records assertion (binary support is optional)
    let expected = expected_json(
        &[
            "i64_val",
            "f64_val",
            "bool_val",
            "str_val",
            "time_val",
            "bytes_val",
        ],
        &[vec![
            Some(("i64_val", JV::Int(-9876543210_i64))),
            Some(("f64_val", JV::Float(3.14159265358979))),
            Some(("bool_val", JV::Bool(false))),
            Some(("str_val", JV::Str("sigil test".into()))),
            Some(("time_val", JV::Int(1_700_000_000_000_000_000_i64))),
            // bytes_val intentionally absent from record assertions (binary optional)
        ]],
    );
    Vector {
        name: "all_sigils",
        nxb,
        expected,
    }
}

fn make_null_vs_absent() -> Vector {
    // 3 records:
    //   rec 0: value = "present"
    //   rec 1: value = null (^)
    //   rec 2: value absent entirely
    let schema = Schema::new(&["id", "value"]);
    let mut w = NxsWriter::new(&schema);

    // Record 0: both fields
    w.begin_object();
    w.write_i64(Slot(0), 1);
    w.write_str(Slot(1), "present");
    w.end_object();

    // Record 1: id + null value
    w.begin_object();
    w.write_i64(Slot(0), 2);
    w.write_null(Slot(1));
    w.end_object();

    // Record 2: id only (value absent)
    w.begin_object();
    w.write_i64(Slot(0), 3);
    w.end_object();

    let nxb = w.finish();
    // For the expected JSON: null value → JSON null; absent → omit key
    let expected = format!(
        "{{\"record_count\":3,\"keys\":[\"id\",\"value\"],\"records\":[\
         {{\"id\":1,\"value\":\"present\"}},\
         {{\"id\":2,\"value\":null}},\
         {{\"id\":3}}\
         ]}}"
    );
    Vector {
        name: "null_vs_absent",
        nxb,
        expected,
    }
}

fn make_sparse() -> Vector {
    // 100 records, schema has 8 fields but each record writes only some
    let schema = Schema::new(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    let mut w = NxsWriter::new(&schema);

    let mut records_json: Vec<String> = Vec::new();

    // Use a simple deterministic pattern to vary which fields each record has
    for i in 0..100u64 {
        let mask = (i * 0xB7_E1_51_62_8A_ED_2A6B_u64.wrapping_add(i)) & 0xFF;
        // ensure at least one field always present
        let mask = if mask == 0 { 1 } else { mask };

        w.begin_object();
        let mut fields_json: Vec<String> = Vec::new();

        // field a = slot 0 (i64)
        if mask & 1 != 0 {
            w.write_i64(Slot(0), i as i64);
            fields_json.push(format!("\"a\":{}", i));
        }
        // field b = slot 1 (f64)
        if mask & 2 != 0 {
            let v = i as f64 * 0.5;
            w.write_f64(Slot(1), v);
            fields_json.push(format!("\"b\":{}", JV::Float(v).to_json()));
        }
        // field c = slot 2 (bool)
        if mask & 4 != 0 {
            let b = i % 2 == 0;
            w.write_bool(Slot(2), b);
            fields_json.push(format!("\"c\":{}", if b { "true" } else { "false" }));
        }
        // field d = slot 3 (str)
        if mask & 8 != 0 {
            let s = format!("s{}", i);
            w.write_str(Slot(3), &s);
            fields_json.push(format!("\"d\":{}", JV::Str(s).to_json()));
        }
        // field e = slot 4 (i64)
        if mask & 16 != 0 {
            w.write_i64(Slot(4), -(i as i64));
            fields_json.push(format!("\"e\":{}", -(i as i64)));
        }
        // field f = slot 5 (f64)
        if mask & 32 != 0 {
            let v = i as f64 * 1.25;
            w.write_f64(Slot(5), v);
            fields_json.push(format!("\"f\":{}", JV::Float(v).to_json()));
        }
        // field g = slot 6 (bool)
        if mask & 64 != 0 {
            let b = i % 3 == 0;
            w.write_bool(Slot(6), b);
            fields_json.push(format!("\"g\":{}", if b { "true" } else { "false" }));
        }
        // field h = slot 7 (i64)
        if mask & 128 != 0 {
            w.write_i64(Slot(7), i as i64 * 100);
            fields_json.push(format!("\"h\":{}", i as i64 * 100));
        }

        w.end_object();
        records_json.push(format!("{{{}}}", fields_json.join(",")));
    }

    let nxb = w.finish();
    let expected = format!(
        "{{\"record_count\":100,\"keys\":[\"a\",\"b\",\"c\",\"d\",\"e\",\"f\",\"g\",\"h\"],\"records\":[{}]}}",
        records_json.join(",")
    );
    Vector {
        name: "sparse",
        nxb,
        expected,
    }
}

fn make_nested() -> Vector {
    // We use the Rust compiler path to generate nested objects,
    // since NxsWriter handles nesting via nested begin_object/end_object calls.
    // We use only string values here so that the SIGIL_STR type hint in the
    // compiler's TypeManifest matches the actual value encoding.

    // Use the compiler to produce nested objects
    let src = r#"
outer_name "top level"
inner {
    inner_name "middle level"
    deepest {
        deep_name "deepest level"
    }
}
"#;

    use nxs::compiler::Compiler;
    use nxs::lexer::Lexer;
    use nxs::parser::Parser;

    let tokens = Lexer::new(src).tokenize().expect("lex");
    let fields = Parser::new(tokens).parse_file().expect("parse");
    let mut compiler = Compiler::new();
    let nxb = compiler.compile(&fields).expect("compile");

    // Build the expected JSON from the actual compiled file's key layout.
    // The compiler uses DFS key ordering: outer_id, inner(obj), inner_id, deepest(obj), ...
    // We generate expected from the actual decoded file.
    use nxs::decoder::decode;
    let decoded = decode(&nxb).expect("decode nested");
    let keys_json: Vec<String> = decoded.keys.iter().map(|k| format!("\"{}\"", k)).collect();
    // Only record top-level integer and string fields for the expected record
    // (nested objects are returned as Raw and skipped)
    let mut rec_fields: Vec<String> = Vec::new();
    for (k, v) in &decoded.root_fields {
        let jv = match v {
            nxs::decoder::DecodedValue::Int(i) => Some(i.to_string()),
            nxs::decoder::DecodedValue::Str(s) => Some(format!("\"{}\"", s)),
            _ => None,
        };
        if let Some(jv_str) = jv {
            rec_fields.push(format!("\"{}\":{}", k, jv_str));
        }
    }
    let expected = format!(
        "{{\"record_count\":1,\"keys\":[{}],\"records\":[{{{}}}]}}",
        keys_json.join(","),
        rec_fields.join(",")
    );

    Vector {
        name: "nested",
        nxb,
        expected: expected.to_string(),
    }
}

fn make_list_i64() -> Vector {
    let schema = Schema::new(&["id", "values"]);
    let mut w = NxsWriter::new(&schema);

    let vals: Vec<i64> = vec![10, 20, 30, -40, 50, 0, i64::MAX, i64::MIN];

    w.begin_object();
    w.write_i64(Slot(0), 1);
    w.write_list_i64(Slot(1), &vals);
    w.end_object();

    let nxb = w.finish();

    let vals_json: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
    let expected = format!(
        "{{\"record_count\":1,\"keys\":[\"id\",\"values\"],\"records\":[{{\"id\":1,\"values\":[{}]}}]}}",
        vals_json.join(",")
    );
    Vector {
        name: "list_i64",
        nxb,
        expected,
    }
}

fn make_list_f64() -> Vector {
    let schema = Schema::new(&["id", "values"]);
    let mut w = NxsWriter::new(&schema);

    let vals: Vec<f64> = vec![
        1.1,
        2.2,
        3.3,
        -4.4,
        0.0,
        f64::INFINITY.is_finite().then(|| 0.0).unwrap_or(0.0),
    ];
    let vals: Vec<f64> = vec![1.1, 2.2, 3.3, -4.4, 0.0, 1e100];

    w.begin_object();
    w.write_i64(Slot(0), 2);
    w.write_list_f64(Slot(1), &vals);
    w.end_object();

    let nxb = w.finish();

    let vals_json: Vec<String> = vals.iter().map(|v| JV::Float(*v).to_json()).collect();
    let expected = format!(
        "{{\"record_count\":1,\"keys\":[\"id\",\"values\"],\"records\":[{{\"id\":2,\"values\":[{}]}}]}}",
        vals_json.join(",")
    );
    Vector {
        name: "list_f64",
        nxb,
        expected,
    }
}

fn make_unicode_strings() -> Vector {
    let schema = Schema::new(&["ascii", "emoji", "cjk", "rtl", "mixed"]);
    let mut w = NxsWriter::new(&schema);

    let ascii = "hello world";
    let emoji = "Hello 🌍 World 🎉";
    let cjk = "日本語テスト";
    let rtl = "مرحبا بالعالم";
    let mixed = "ASCII + Unicode: café résumé naïve";

    w.begin_object();
    w.write_str(Slot(0), ascii);
    w.write_str(Slot(1), emoji);
    w.write_str(Slot(2), cjk);
    w.write_str(Slot(3), rtl);
    w.write_str(Slot(4), mixed);
    w.end_object();

    let nxb = w.finish();
    let expected = expected_json(
        &["ascii", "emoji", "cjk", "rtl", "mixed"],
        &[vec![
            Some(("ascii", JV::Str(ascii.into()))),
            Some(("emoji", JV::Str(emoji.into()))),
            Some(("cjk", JV::Str(cjk.into()))),
            Some(("rtl", JV::Str(rtl.into()))),
            Some(("mixed", JV::Str(mixed.into()))),
        ]],
    );
    Vector {
        name: "unicode_strings",
        nxb,
        expected,
    }
}

fn make_large() -> Vector {
    let schema = Schema::new(&["id", "value"]);
    let n = 10_000usize;
    let mut w = NxsWriter::with_capacity(&schema, n * 64);

    for i in 0..n {
        w.begin_object();
        w.write_i64(Slot(0), i as i64);
        w.write_f64(Slot(1), i as f64 * 0.1);
        w.end_object();
    }

    let nxb = w.finish();

    // For the expected JSON, only assert the first and last record
    // to keep the file small — runners must check all N records
    // but we only validate record_count and spot-check boundary records
    let rec0 = format!("{{\"id\":0,\"value\":{}}}", JV::Float(0.0).to_json());
    let rec_last = format!(
        "{{\"id\":{},\"value\":{}}}",
        n - 1,
        JV::Float((n - 1) as f64 * 0.1).to_json()
    );

    // Include all records (runners will validate them all)
    let mut recs: Vec<String> = Vec::with_capacity(n);
    for i in 0..n {
        recs.push(format!(
            "{{\"id\":{},\"value\":{}}}",
            i,
            JV::Float(i as f64 * 0.1).to_json()
        ));
    }
    let _ = (rec0, rec_last); // suppress warnings

    let expected = format!(
        "{{\"record_count\":{},\"keys\":[\"id\",\"value\"],\"records\":[{}]}}",
        n,
        recs.join(",")
    );
    Vector {
        name: "large",
        nxb,
        expected,
    }
}

fn make_max_keys() -> Vector {
    // 255 keys — near the LEB128 bitmask boundary (255 bits = 37 bytes of LEB128)
    let num_keys = 255usize;
    let keys: Vec<String> = (0..num_keys).map(|i| format!("k{:03}", i)).collect();
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let schema = Schema::new(&key_refs);
    let mut w = NxsWriter::new(&schema);

    w.begin_object();
    // Write every other key to exercise sparse access in large schemas
    for i in (0..num_keys).step_by(2) {
        w.write_i64(Slot(i as u16), i as i64);
    }
    w.end_object();

    let nxb = w.finish();

    let mut fields_json: Vec<String> = Vec::new();
    for i in (0..num_keys).step_by(2) {
        fields_json.push(format!("\"k{:03}\":{}", i, i));
    }

    let keys_json: Vec<String> = keys.iter().map(|k| format!("\"{}\"", k)).collect();
    let expected = format!(
        "{{\"record_count\":1,\"keys\":[{}],\"records\":[{{{}}}]}}",
        keys_json.join(","),
        fields_json.join(",")
    );
    Vector {
        name: "max_keys",
        nxb,
        expected,
    }
}

fn make_jumbo_string() -> Vector {
    let schema = Schema::new(&["id", "blob"]);
    let mut w = NxsWriter::new(&schema);

    // 128 KB string filled with repeating pattern
    let pattern = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz_-";
    let target_len = 128 * 1024;
    let mut s = String::with_capacity(target_len);
    while s.len() < target_len {
        let remaining = target_len - s.len();
        let chunk = &pattern[..remaining.min(pattern.len())];
        s.push_str(chunk);
    }

    w.begin_object();
    w.write_i64(Slot(0), 99);
    w.write_str(Slot(1), &s);
    w.end_object();

    let nxb = w.finish();

    let expected = format!(
        "{{\"record_count\":1,\"keys\":[\"id\",\"blob\"],\"records\":[{{\"id\":99,\"blob\":{}}}]}}",
        JV::Str(s).to_json()
    );
    Vector {
        name: "jumbo_string",
        nxb,
        expected,
    }
}

// ── Negative vectors ──────────────────────────────────────────────────────────

fn make_bad_magic() -> (Vec<u8>, String) {
    // Build a valid file then corrupt the first 4 bytes
    let schema = Schema::new(&["x"]);
    let mut w = NxsWriter::new(&schema);
    w.begin_object();
    w.write_i64(Slot(0), 1);
    w.end_object();
    let mut nxb = w.finish();
    nxb[0] = 0xFF;
    nxb[1] = 0xFF;
    nxb[2] = 0xFF;
    nxb[3] = 0xFF;
    let expected = r#"{"error":"ERR_BAD_MAGIC"}"#.to_string();
    (nxb, expected)
}

fn make_bad_dict_hash() -> (Vec<u8>, String) {
    // Build a valid file then flip the DictHash bytes (offset 8..16)
    let schema = Schema::new(&["x"]);
    let mut w = NxsWriter::new(&schema);
    w.begin_object();
    w.write_i64(Slot(0), 1);
    w.end_object();
    let mut nxb = w.finish();
    // Corrupt the DictHash (bytes 8–15)
    nxb[8] ^= 0xFF;
    nxb[9] ^= 0xFF;
    let expected = r#"{"error":"ERR_DICT_MISMATCH"}"#.to_string();
    (nxb, expected)
}

fn make_truncated() -> (Vec<u8>, String) {
    // Build a valid file then truncate to byte 20
    let schema = Schema::new(&["x"]);
    let mut w = NxsWriter::new(&schema);
    w.begin_object();
    w.write_i64(Slot(0), 1);
    w.end_object();
    let nxb = w.finish();
    let truncated = nxb[..20].to_vec();
    let expected = r#"{"error":"ERR_OUT_OF_BOUNDS"}"#.to_string();
    (truncated, expected)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();
    let out_dir = if args.len() > 1 {
        args[1].clone()
    } else {
        "conformance".to_string()
    };
    let out_path = Path::new(&out_dir);

    fs::create_dir_all(out_path).expect("create output directory");

    // Positive vectors
    let positive: Vec<Vector> = vec![
        make_minimal(),
        make_all_sigils(),
        make_null_vs_absent(),
        make_sparse(),
        make_list_i64(),
        make_list_f64(),
        make_unicode_strings(),
        make_large(),
        make_max_keys(),
        make_jumbo_string(),
    ];

    // Nested uses the compiler — generate separately
    let nested = make_nested();

    for v in positive.iter().chain(std::iter::once(&nested)) {
        let nxb_path = out_path.join(format!("{}.nxb", v.name));
        let json_path = out_path.join(format!("{}.expected.json", v.name));
        fs::write(&nxb_path, &v.nxb)
            .unwrap_or_else(|e| panic!("write {}: {e}", nxb_path.display()));
        fs::write(&json_path, &v.expected)
            .unwrap_or_else(|e| panic!("write {}: {e}", json_path.display()));
        println!(
            "  wrote {}.nxb ({} bytes) + .expected.json ({} bytes)",
            v.name,
            v.nxb.len(),
            v.expected.len()
        );
    }

    // Negative vectors
    let (bad_magic_nxb, bad_magic_json) = make_bad_magic();
    fs::write(out_path.join("bad_magic.nxb"), &bad_magic_nxb).unwrap();
    fs::write(out_path.join("bad_magic.expected.json"), &bad_magic_json).unwrap();
    println!("  wrote bad_magic.nxb + .expected.json");

    let (bad_hash_nxb, bad_hash_json) = make_bad_dict_hash();
    fs::write(out_path.join("bad_dict_hash.nxb"), &bad_hash_nxb).unwrap();
    fs::write(out_path.join("bad_dict_hash.expected.json"), &bad_hash_json).unwrap();
    println!("  wrote bad_dict_hash.nxb + .expected.json");

    let (trunc_nxb, trunc_json) = make_truncated();
    fs::write(out_path.join("truncated.nxb"), &trunc_nxb).unwrap();
    fs::write(out_path.join("truncated.expected.json"), &trunc_json).unwrap();
    println!("  wrote truncated.nxb + .expected.json");

    println!(
        "\nAll conformance vectors written to: {}",
        out_path.display()
    );
}
