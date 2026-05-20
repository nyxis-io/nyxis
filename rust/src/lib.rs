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

pub mod compiler;
pub mod convert;
pub mod decoder;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod query;
pub mod segment_reader;
pub mod wal;
pub mod writer;
