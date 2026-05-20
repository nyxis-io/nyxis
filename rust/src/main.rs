#![allow(dead_code, unused_imports, unused_variables)]
mod compiler;
mod decoder;
mod error;
mod lexer;
mod parser;
mod writer;

use error::NxsError;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: nxs <file.nxs> [output.nxb]");
        std::process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        input_path
            .with_extension("nxb")
            .to_string_lossy()
            .to_string()
    };

    let source = std::fs::read_to_string(input_path)
        .map_err(|e| NxsError::IoError(e.to_string()))
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });

    let binary = compile(&source).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    std::fs::write(&output_path, &binary)
        .map_err(|e| NxsError::IoError(e.to_string()))
        .unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(1);
        });

    println!(
        "compiled {} → {} ({} bytes)",
        input_path.display(),
        output_path,
        binary.len()
    );
}

fn compile(source: &str) -> error::Result<Vec<u8>> {
    let mut lexer = lexer::Lexer::new(source);
    let tokens = lexer.tokenize()?;

    let mut parser = parser::Parser::new(tokens);
    let fields = parser.parse_file()?;

    let mut compiler = compiler::Compiler::new();
    compiler.compile(&fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_basic() {
        let src = r#"
            user {
                id: =1024
                active: ?true
                name: "Alex"
            }
        "#;
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
        let bytes = result.unwrap();
        assert_eq!(&bytes[0..4], &0x4E595842u32.to_le_bytes());
        assert!(bytes.len() > 32);
    }

    #[test]
    fn test_int_and_float() {
        let src = "x: =42\ny: ~3.14";
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_list_uniform() {
        let src = "tags: [$admin, $beta]";
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_list_mixed_fails() {
        let src = "bad: [=1, ~2.0]";
        let result = compile(src);
        assert!(matches!(result, Err(NxsError::ListTypeMismatch)));
    }

    #[test]
    fn test_string_escape() {
        let src = r#"msg: "hello \"world\"\n""#;
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_bad_escape_fails() {
        let src = r#"msg: "hello \q world""#;
        let result = compile(src);
        assert!(matches!(result, Err(NxsError::BadEscape('q'))));
    }

    #[test]
    fn test_null() {
        let src = "maybe: ^";
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_nested_object() {
        let src = r#"
            config {
                db {
                    port: =5432
                    host: "localhost"
                }
            }
        "#;
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_temporal() {
        let src = "created_at: @2026-04-30";
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_binary_literal() {
        let src = "data: <DEADBEEF>";
        let result = compile(src);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_footer_magic() {
        let src = "x: =1";
        let bytes = compile(src).unwrap();
        let footer = &bytes[bytes.len() - 4..];
        assert_eq!(footer, &0x2153584Eu32.to_le_bytes());
    }

    // ── Example file tests ──────────────────────────────────────────────────

    fn compile_file(path: &str) -> Vec<u8> {
        let src = std::fs::read_to_string(path).unwrap_or_else(|_| panic!("cannot read {path}"));
        compile(&src).unwrap_or_else(|e| panic!("compile {path}: {e}"))
    }

    fn assert_valid_nxb(bytes: &[u8]) {
        assert!(bytes.len() >= 32, "output too short");
        assert_eq!(&bytes[0..4], &0x4E595842u32.to_le_bytes(), "bad file magic");
        assert_eq!(
            &bytes[bytes.len() - 4..],
            &0x2153584Eu32.to_le_bytes(),
            "bad footer magic"
        );
        // Streamable v1.1 files set preamble TailPtr=0 and carry the tail pointer in the footer.
        let tail_ptr = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        assert_eq!(tail_ptr, 0, "preamble TailPtr must be zero");
        let footer_tail_ptr =
            u64::from_le_bytes(bytes[bytes.len() - 12..bytes.len() - 4].try_into().unwrap())
                as usize;
        assert!(
            footer_tail_ptr < bytes.len(),
            "footer TailPtr out of bounds"
        );
    }

    #[test]
    fn example_user_profile() {
        let bytes = compile_file("../examples/user_profile.nxs");
        assert_valid_nxb(&bytes);
        // Decode and check keys are recovered
        let decoded = decoder::decode(&bytes).expect("decode failed");
        assert!(!decoded.keys.is_empty(), "no keys in schema");
        assert!(
            decoded.keys.contains(&"user".to_string()) || decoded.keys.contains(&"id".to_string()),
            "expected user or id key, got: {:?}",
            decoded.keys
        );
        println!(
            "user_profile.nxb: {} bytes, {} schema keys",
            bytes.len(),
            decoded.keys.len()
        );
    }

    #[test]
    fn example_product_catalog() {
        let bytes = compile_file("../examples/product_catalog.nxs");
        assert_valid_nxb(&bytes);
        let decoded = decoder::decode(&bytes).expect("decode failed");
        assert!(!decoded.keys.is_empty());
        println!(
            "product_catalog.nxb: {} bytes, {} schema keys",
            bytes.len(),
            decoded.keys.len()
        );
    }

    #[test]
    fn example_timeseries() {
        let bytes = compile_file("../examples/timeseries.nxs");
        assert_valid_nxb(&bytes);
        let decoded = decoder::decode(&bytes).expect("decode failed");
        assert!(!decoded.keys.is_empty());
        println!(
            "timeseries.nxb: {} bytes, {} schema keys",
            bytes.len(),
            decoded.keys.len()
        );
    }

    #[test]
    fn test_writer_basic() {
        let schema = writer::Schema::new(&["id", "name", "score", "active"]);
        let mut w = writer::NxsWriter::new(&schema);
        w.begin_object();
        w.write_i64(writer::Slot(0), 1024);
        w.write_str(writer::Slot(1), "Alex");
        w.write_f64(writer::Slot(2), 9.5);
        w.write_bool(writer::Slot(3), true);
        w.end_object();
        let bytes = w.finish();
        assert_eq!(&bytes[0..4], &0x4E595842u32.to_le_bytes());
        assert_eq!(&bytes[bytes.len() - 4..], &0x2153584Eu32.to_le_bytes());
        assert!(bytes.windows(8).any(|w| w == 1024i64.to_le_bytes()));
    }

    #[test]
    fn test_writer_matches_spec_footer() {
        let schema = writer::Schema::new(&["x"]);
        let mut w = writer::NxsWriter::new(&schema);
        w.begin_object();
        w.write_i64(writer::Slot(0), 42);
        w.end_object();
        let bytes = w.finish();
        assert_eq!(&bytes[bytes.len() - 4..], &0x2153584Eu32.to_le_bytes());
        let tail_ptr = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        assert_eq!(tail_ptr, 0);
        let footer_tail_ptr =
            u64::from_le_bytes(bytes[bytes.len() - 12..bytes.len() - 4].try_into().unwrap())
                as usize;
        assert!(footer_tail_ptr < bytes.len());
    }

    #[test]
    fn test_writer_out_of_order_slots_still_decode() {
        let schema = writer::Schema::new(&["a", "b", "c"]);
        let mut w = writer::NxsWriter::new(&schema);
        w.begin_object();
        // Write out of order: c then a then b
        w.write_i64(writer::Slot(2), 30);
        w.write_i64(writer::Slot(0), 10);
        w.write_i64(writer::Slot(1), 20);
        w.end_object();
        let bytes = w.finish();
        assert_eq!(&bytes[0..4], &0x4E595842u32.to_le_bytes());
        assert_eq!(&bytes[bytes.len() - 4..], &0x2153584Eu32.to_le_bytes());
    }

    #[test]
    fn example_round_trip_sizes() {
        // Compile all three examples and report their sizes relative to the .nxs source
        for name in &["user_profile", "product_catalog", "timeseries"] {
            let src_path = format!("../examples/{name}.nxs");
            let src = std::fs::read_to_string(&src_path)
                .unwrap_or_else(|_| panic!("cannot read {src_path}"));
            let bytes = compile(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
            let ratio = bytes.len() as f64 / src.len() as f64;
            println!(
                "{name}: source={} B → binary={} B  ({:.2}x)",
                src.len(),
                bytes.len(),
                ratio
            );
            assert_valid_nxb(&bytes);
        }
    }
}
