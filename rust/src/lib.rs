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
pub mod column_prefetch;
pub mod compact;
pub mod compiler;
pub mod consts;
pub mod convert;
pub mod decoder;
pub mod error;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod pax_stream;
pub mod prefetch;
pub mod query;
pub mod segment_reader;
pub mod stats;
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
    fn row_compile_compact_multi_record() {
        let src = r#"
            r0 { id: =0 username: "alice" age: =20 active: ?true score: ~0.5 }
            r1 { id: =1 username: "bob" age: =21 active: ?false score: ~1.0 }
        "#;
        let mut opts = layout::CompileOptions::default();
        opts.compact = Some(crate::compact::CompactOptions::compact());
        let bytes = compile_source_with_opts(src, &opts).expect("compact compile");
        let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        assert!(flags & crate::consts::FLAG_DENSE_FRAMES != 0);
        assert_eq!(
            u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            crate::consts::VERSION_V13
        );
        let reader = crate::query::Reader::new(&bytes).unwrap();
        assert_eq!(reader.record_count(), 2);
        assert_eq!(reader.record(1).unwrap().get_str("username"), Some("bob"));
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

    /// Conformance `sparse` pattern: sparse frames only on this mask; decode matches v1.2.
    #[test]
    fn compact_sparse_matches_v12_decode() {
        use super::compact::{is_dense_record, CompactOptions};
        use super::layout::{finish_row, Cell, RecordRow};
        use super::query::Reader;

        let keys: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut rows = Vec::new();
        for i in 0..100u64 {
            let mask = i.wrapping_mul(0xB7_E1_51_62_8A_ED_2A6B_u64.wrapping_add(i)) & 0xFF;
            let mask = if mask == 0 { 1 } else { mask };
            let mut cells = vec![Cell::Absent; 8];
            if mask & 1 != 0 {
                cells[0] = Cell::I64(i as i64);
            }
            if mask & 2 != 0 {
                cells[1] = Cell::F64(i as f64 * 0.5);
            }
            if mask & 4 != 0 {
                cells[2] = Cell::Bool(i % 2 == 0);
            }
            if mask & 8 != 0 {
                cells[3] = Cell::Str(format!("s{i}"));
            }
            if mask & 16 != 0 {
                cells[4] = Cell::I64(-(i as i64));
            }
            if mask & 32 != 0 {
                cells[5] = Cell::F64(i as f64 * 1.25);
            }
            if mask & 64 != 0 {
                cells[6] = Cell::Bool(i % 3 == 0);
            }
            if mask & 128 != 0 {
                cells[7] = Cell::I64(i as i64 * 100);
            }
            rows.push(RecordRow { cells });
        }
        let v12 = finish_row(&keys, &rows, None).unwrap();
        let compact = finish_row(&keys, &rows, Some(&CompactOptions::compact())).unwrap();
        let r12 = Reader::new(&v12).unwrap();
        let r13 = Reader::new(&compact).unwrap();
        let mut sparse = 0usize;
        for i in 0..100 {
            let off = r13.record(i).unwrap().object_offset().unwrap();
            if !is_dense_record(&compact, off).unwrap() {
                sparse += 1;
            }
            let a = r12.record(i).unwrap();
            let b = r13.record(i).unwrap();
            for (fi, key) in keys.iter().enumerate() {
                match fi {
                    0 | 4 | 7 => assert_eq!(a.get_i64(key), b.get_i64(key), "{key} @ {i}"),
                    1 | 5 => assert_eq!(a.get_f64(key), b.get_f64(key), "{key} @ {i}"),
                    2 | 6 => assert_eq!(a.get_bool(key), b.get_bool(key), "{key} @ {i}"),
                    3 => assert_eq!(a.get_str(key), b.get_str(key), "{key} @ {i}"),
                    _ => {}
                }
            }
        }
        assert_eq!(sparse, 100);
        assert!(compact.len() < v12.len());
    }

    /// Single-bool dense schema: packed bool word is 1 B (same width as one bool); without packing, bool is 8 B.
    #[test]
    fn single_bool_packed_word_is_one_byte_unpacked_is_eight() {
        use super::compact::{parse_extended_schema, CompactOptions, RowCellPlan};
        use super::layout::{finish_row, Cell, RecordRow};
        let keys = vec!["id".into(), "active".into(), "name".into()];
        let rows: Vec<RecordRow> = (0..100)
            .map(|i| RecordRow {
                cells: vec![
                    Cell::I64(i),
                    Cell::Bool(i % 2 == 0),
                    Cell::Str(format!("u{i}")),
                ],
            })
            .collect();
        let packed = finish_row(&keys, &rows, Some(&CompactOptions::compact())).unwrap();
        let mut no_pack = CompactOptions::compact();
        no_pack.packed_bools = false;
        let unpacked = finish_row(&keys, &rows, Some(&no_pack)).unwrap();
        assert!(
            unpacked.len() > packed.len(),
            "unpacked bool cells should be larger than 1-byte bool word"
        );
        let flags = u16::from_le_bytes(packed[6..8].try_into().unwrap());
        let (ext, _) = parse_extended_schema(&packed, 32, flags).unwrap();
        assert_eq!(ext.widths.get(1).copied().unwrap_or(0), 0);
        assert_eq!(RowCellPlan::new(&ext, flags).bool_word_bytes(), 1);
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
