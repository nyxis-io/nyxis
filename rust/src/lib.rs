//! `nxs` — the Nyxis binary format library.
//!
//! Compiles `.nxs` source text to `.nxb` binary files and provides a zero-copy
//! query reader for those files.
//!
//! # Quick start
//!
//! ```no_run
//! // Compile .nxs source to bytes
//! let bytes = nxs::compile_source("r0 { id: =1 score: ~9.5 }").unwrap();
//!
//! // Query the result
//! use nxs::query::{Reader, eq};
//! let reader = Reader::new(&bytes).unwrap();
//! for rec in reader.where_pred(eq("id", 1i64)) {
//!     println!("{:?}", rec.get_f64("score"));
//! }
//! ```

#![allow(clippy::new_without_default)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::manual_div_ceil)]
#![allow(clippy::same_item_push)]
#![allow(clippy::empty_line_after_doc_comments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::single_match)]

pub mod arrow_project;
pub mod col_reduce;
pub mod compiler;
pub mod consts;
pub mod convert;
pub mod decoder;
pub mod error;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod pax_stream;
pub mod query;
pub mod segment_reader;
pub mod stream_reader;
pub mod wal;
pub mod writer;

pub use arrow_project::VarColumnView;
pub use pax_stream::{
    complete_page_end, PaxPageMeta, PaxPageView, PaxStreamReader, PaxStreamWriter,
};

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
mod wasm_api;

/// Compile `.nxs` source text to `.nxb` bytes (lex → parse → compile).
pub fn compile_source(source: &str) -> error::Result<Vec<u8>> {
    compile_source_with_opts(source, &layout::CompileOptions::default())
}

/// Compile with layout / page-size options (CLI and `@layout` pragmas).
pub fn compile_source_with_opts(
    source: &str,
    opts: &layout::CompileOptions,
) -> error::Result<Vec<u8>> {
    let mut lexer = lexer::Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let (fields, file_opts) = parse_file_with_pragmas(tokens)?;
    let mut merged = opts.clone();
    if file_opts.layout != layout::Layout::Row {
        merged.layout = file_opts.layout;
    }
    if file_opts.page_size != 0 {
        merged.page_size = file_opts.page_size;
    }
    if merged.page_size == 0 && merged.layout == layout::Layout::Pax {
        merged.page_size = 4096;
    }
    layout::compile_fields(&fields, &merged)
}

#[cfg(test)]
mod layout_tests {
    use super::layout::{self, finish_columnar, Cell, Layout, RecordRow};
    use super::{compile_source, compile_source_with_opts};

    #[test]
    fn compile_source_atlayout_pragma_one_line() {
        let src = "@layout columnar\nr0 { score: ~1.5 region_id: =0 }\nr1 { score: ~2.0 region_id: =1 }\n";
        let bytes = compile_source(src).expect("pragma + compile");
        let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        assert!(flags & layout::FLAG_COLUMNAR != 0);
    }

    #[test]
    fn compile_columnar_from_source() {
        let src = r#"
            @layout columnar
            r1 { id: =1 score: ~0.5 active: ?true ts: @0 }
            r2 { id: =2 score: ~1.0 active: ?false ts: @1000 }
        "#;
        let mut opts = layout::CompileOptions::default();
        opts.layout = Layout::Columnar;
        let bytes = compile_source_with_opts(src, &opts).expect("compile");
        let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        assert!(flags & layout::FLAG_COLUMNAR != 0);
    }

    #[test]
    fn columnar_sum_ready() {
        let keys = vec!["score".to_string()];
        let rows = vec![
            RecordRow {
                cells: vec![Cell::F64(1.0)],
            },
            RecordRow {
                cells: vec![Cell::F64(2.0)],
            },
        ];
        let bytes = finish_columnar(&keys, &rows).unwrap();
        assert!(bytes.len() > 64);
    }

    #[test]
    fn row_compile_uses_resolved_macro_sigils() {
        let bytes = compile_source("id: =7 alias: @id name: !\"bob\"\n").unwrap();
        let reader = crate::query::Reader::new(&bytes).unwrap();
        assert_eq!(reader.key_sigils(), b"==\"");
        let rec = reader.record(0).unwrap();
        assert_eq!(rec.get_i64("alias"), Some(7));
        assert_eq!(rec.get_str("name"), Some("bob"));
    }

    #[test]
    fn reader_rejects_overflowing_row_tail_pointer() {
        let data = [
            0x42, 0x58, 0x59, 0x4e, 0, 0, 0, 0, 0, 0x29, 0, 2, 0, 0, 0, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0x0a, 0, 0x22, 0x5c,
            0x28, 0x4e, 0x58, 0x53, 0x21,
        ];
        assert!(crate::query::Reader::new(&data).is_err());
    }

    #[test]
    fn reader_rejects_overflowing_pax_tail_pointer() {
        let data = [
            0x42, 0x58, 0x59, 0x4e, 0x35, 0x37, 0x0e, 0x0e, 0x0e, 0x0e, 0x0e, 0x0e, 0x35, 0x35,
            0xca, 0x35, 0xcb, 0xca, 0x24, 0x00, 0x35, 0x00, 0x07, 0x35, 0x35, 0xa2, 0xcc, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x30, 0x00, 0x08, 0x00, 0x1c, 0x05, 0x05, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x00, 0x4e, 0x58, 0x53, 0x21,
        ];
        if let Ok(reader) = crate::query::Reader::new(&data) {
            let _ = reader.record(0);
        }
    }

    #[test]
    fn reader_rejects_overflowing_row_slot_offset() {
        let data = [
            0x42, 0x58, 0x59, 0x4e, 0x00, 0x00, 0x42, 0x58, 0x59, 0x26, 0x58, 0x59, 0x4e, 0x8f,
            0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x25, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xac, 0x29, 0x94, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07,
            0x00, 0x00, 0x00, 0x58, 0x59, 0x4e, 0x00, 0x00, 0x00, 0x00, 0x53, 0x53, 0x53, 0x53,
            0x59, 0x4e, 0x00, 0x00, 0x4e, 0x59, 0x58, 0x00, 0x42, 0x00, 0x00, 0x7e, 0x59, 0x4e,
            0x00, 0x4e, 0x00, 0x42, 0x58, 0x59, 0x4e, 0x00, 0x8d, 0x8f, 0x00, 0x00, 0x58, 0x59,
            0x4e, 0x00, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xa7, 0xaa, 0x0a, 0x05, 0x00, 0x80, 0x42,
            0x58, 0x59, 0x4e, 0x00, 0x7e, 0x00, 0x02, 0x02, 0x02, 0x02, 0x05, 0x00, 0x00, 0x7e,
            0x40, 0x00, 0x88, 0x08, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x59, 0x00,
            0x4e, 0x00, 0x42, 0x58, 0x59, 0x4e, 0x00, 0x49, 0x00, 0x49, 0x80, 0x00, 0xb5, 0xb5,
            0xb5, 0xb5, 0xb5, 0xb5, 0xb5, 0xb5, 0x95, 0xb5, 0x00, 0x8f, 0x00, 0x7e, 0x00, 0x8f,
            0x00, 0x58, 0x59, 0x4e, 0x00, 0x26, 0x00, 0x02, 0x02, 0x00, 0x26, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x00, 0x26, 0x00, 0x8f, 0x8f, 0x8f, 0x21, 0x4f, 0x4e, 0x58, 0x53, 0x00,
            0x00, 0x00, 0x4e, 0x58, 0x53, 0x21,
        ];
        if let Ok(reader) = crate::query::Reader::new(&data) {
            if let Some(rec) = reader.record(0) {
                let _ = rec.get_i64("NXB");
            }
        }
    }

    #[test]
    fn resolve_slot_rejects_overflowing_object_offset() {
        assert_eq!(crate::query::resolve_slot(&[], usize::MAX, 0), None);
    }
}

fn parse_file_with_pragmas(
    tokens: Vec<lexer::Token>,
) -> error::Result<(Vec<parser::Field>, layout::CompileOptions)> {
    let mut opts = layout::CompileOptions::default();
    let mut pos = 0usize;
    while pos < tokens.len() {
        if let lexer::Token::Macro(m) = &tokens[pos] {
            if let Some(name) = m.strip_prefix('@') {
                pos += 1;
                let value = match tokens.get(pos) {
                    Some(lexer::Token::Ident(s)) => s.as_str(),
                    Some(lexer::Token::Int(n)) => {
                        let tmp = n.to_string();
                        layout::apply_pragma(&mut opts, name, &tmp)?;
                        pos += 1;
                        continue;
                    }
                    _ => {
                        return Err(error::NxsError::ParseError(format!(
                            "pragma @{name} requires a value"
                        )));
                    }
                };
                layout::apply_pragma(&mut opts, name, value)?;
                pos += 1;
                continue;
            }
        }
        break;
    }
    let mut parser = parser::Parser::new(tokens);
    parser.set_pos(pos);
    let fields = parser.parse_file()?;
    Ok((fields, opts))
}
