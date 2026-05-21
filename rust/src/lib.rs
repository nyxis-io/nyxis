#![allow(dead_code)]
#![allow(unused_imports)]
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

pub mod col_reduce;
pub mod compiler;
pub mod convert;
pub mod decoder;
pub mod error;
pub mod layout;
pub mod pax_stream;
pub mod lexer;
pub mod parser;
pub mod query;
pub mod segment_reader;
pub mod stream_reader;
pub mod wal;
pub mod writer;

pub use pax_stream::{complete_page_end, PaxPageMeta, PaxPageView, PaxStreamReader, PaxStreamWriter};

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
