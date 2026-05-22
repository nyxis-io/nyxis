//! Columnar and PAX `.nxb` layout writers (OLAP.md v0.1).
//!
//! Phase 1: dense numeric columnar (`FLAG_COLUMNAR`).
//! Phase 2: PAX pages with per-page column groups (`FLAG_PAX`).

use crate::error::{NxsError, Result};
use crate::parser::{Field, Value};
use crate::writer::{build_schema, murmur3_64, NxsWriter};
use std::collections::HashMap;

/// Columnar layout (OLAP §2). Combined with [`FLAG_SCHEMA_EMBEDDED`] on write.
pub const FLAG_COLUMNAR: u16 = 0x0001;
/// PAX layout — bit 2 so it does not alias schema-embedded (`0x0002`).
pub const FLAG_PAX: u16 = 0x0004;
pub const FLAG_SCHEMA_EMBEDDED: u16 = 0x0002;
pub const MAGIC_FILE: u32 = 0x4E59_5842;
pub const MAGIC_FOOTER: u32 = 0x2153_584E;
pub const MAGIC_PAGE: u32 = 0x4E58_5350; // NXSP
pub const VERSION: u16 = 0x0101;

const FOOTER_ROW: usize = 12;
const FOOTER_COLUMNAR: usize = 20;
const FOOTER_PAX: usize = 28;

/// Layout selection for compile / writer finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    #[default]
    Row,
    Columnar,
    Pax,
}

impl Layout {
    pub fn parse_name(s: &str) -> Option<Self> {
        match s {
            "row" => Some(Layout::Row),
            "columnar" => Some(Layout::Columnar),
            "pax" => Some(Layout::Pax),
            _ => None,
        }
    }

    pub fn flag(self) -> u16 {
        match self {
            Layout::Row => 0,
            Layout::Columnar => FLAG_COLUMNAR,
            Layout::Pax => FLAG_PAX,
        }
    }
}

/// Parsed file directives (`@layout`, `@page-size`).
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    pub layout: Layout,
    pub page_size: u32,
}

impl CompileOptions {
    pub fn validate_flags(&self, tail_ptr_zero: bool) -> Result<()> {
        if self.layout == Layout::Columnar && tail_ptr_zero {
            return Err(NxsError::IncompatibleFlags);
        }
        Ok(())
    }
}

/// Apply pragma from `@name` macro token (value follows as next token).
pub fn apply_pragma(opts: &mut CompileOptions, name: &str, value: &str) -> Result<()> {
    match name {
        "layout" => {
            opts.layout = Layout::parse_name(value)
                .ok_or_else(|| NxsError::ParseError(format!("unknown layout: {value}")))?;
        }
        "page-size" => {
            opts.page_size = value
                .parse()
                .map_err(|_| NxsError::ParseError(format!("bad page-size: {value}")))?;
            if opts.page_size == 0 {
                return Err(NxsError::ParseError("page-size must be > 0".into()));
            }
        }
        other => {
            return Err(NxsError::ParseError(format!("unknown pragma: @{other}")));
        }
    }
    Ok(())
}

/// Validate preamble flag combinations.
pub fn validate_preamble_flags(flags: u16) -> Result<()> {
    let col = flags & FLAG_COLUMNAR != 0;
    let pax = flags & FLAG_PAX != 0;
    if col && pax {
        return Err(NxsError::InvalidFlags);
    }
    if (col || pax) && flags & FLAG_SCHEMA_EMBEDDED == 0 {
        return Err(NxsError::ParseError(
            "columnar/PAX requires FLAG_SCHEMA_EMBEDDED".into(),
        ));
    }
    Ok(())
}

// ── Record model for layout emitters ─────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cell {
    Absent,
    Null,
    I64(i64),
    F64(f64),
    Bool(bool),
    Time(i64),
}

impl Cell {
    fn from_value(v: &Value) -> Result<Self> {
        match v {
            Value::Int(n) => Ok(Cell::I64(*n)),
            Value::Float(f) => Ok(Cell::F64(*f)),
            Value::Bool(b) => Ok(Cell::Bool(*b)),
            Value::Time(ns) => Ok(Cell::Time(*ns)),
            Value::Null => Ok(Cell::Null),
            Value::Str(_) | Value::Keyword(_) | Value::Binary(_) => {
                Err(NxsError::UnsupportedFieldType)
            }
            Value::Object(_) | Value::List(_) | Value::Macro(_) | Value::Link(_) => Err(
                NxsError::ParseError("nested values not supported in columnar/PAX records".into()),
            ),
        }
    }

    fn sigil(self) -> u8 {
        match self {
            Cell::I64(_) => b'=',
            Cell::F64(_) => b'~',
            Cell::Bool(_) => b'?',
            Cell::Time(_) => b'@',
            Cell::Null => b'^',
            Cell::Absent => 0,
        }
    }
}

#[derive(Clone)]
pub struct RecordRow {
    pub cells: Vec<Cell>,
}

/// Extract top-level records from parsed fields (each `key { ... }` object).
pub fn records_from_fields(fields: &[Field]) -> Result<(Vec<String>, Vec<RecordRow>)> {
    let mut key_order: Vec<String> = Vec::new();
    let mut key_index: HashMap<String, usize> = HashMap::new();
    let mut rows: Vec<RecordRow> = Vec::new();

    for field in fields {
        let Value::Object(inner) = &field.value else {
            return Err(NxsError::ParseError(
                "columnar/PAX compile expects top-level `name { ... }` record blocks".into(),
            ));
        };
        let mut cells = Vec::new();
        for f in inner {
            let cell = Cell::from_value(&f.value)?;
            let idx = if let Some(&i) = key_index.get(&f.key) {
                i
            } else {
                let i = key_order.len();
                key_order.push(f.key.clone());
                key_index.insert(f.key.clone(), i);
                cells.resize(i, Cell::Absent);
                i
            };
            if cells.len() <= idx {
                cells.resize(idx + 1, Cell::Absent);
            }
            cells[idx] = cell;
        }
        rows.push(RecordRow { cells });
    }

    if rows.is_empty() {
        return Err(NxsError::ParseError("no records to compile".into()));
    }

    let width = key_order.len();
    for row in &mut rows {
        row.cells.resize(width, Cell::Absent);
    }
    Ok((key_order, rows))
}

fn null_bitmap_bytes(n: usize) -> usize {
    let raw = (n + 7) / 8;
    ((raw + 7) / 8) * 8
}

fn encode_null_bitmap(n: usize, present: impl Fn(usize) -> bool) -> Vec<u8> {
    let len = null_bitmap_bytes(n);
    let mut b = vec![0u8; len];
    for i in 0..n {
        if present(i) {
            b[i / 8] |= 1 << (i % 8);
        }
    }
    b
}

fn cell_populated(c: Cell) -> bool {
    !matches!(c, Cell::Absent)
}

fn write_fixed_buffer(n: usize, cells: &[Cell], encode: impl Fn(Cell) -> [u8; 8]) -> Vec<u8> {
    let mut buf = vec![0u8; n * 8];
    for (i, &c) in cells.iter().enumerate().take(n) {
        if cell_populated(c) {
            buf[i * 8..(i + 1) * 8].copy_from_slice(&encode(c));
        }
    }
    buf
}

fn encode_field_column(n: usize, col: &[Cell], sigil: u8) -> Result<Vec<u8>> {
    let present = |i: usize| cell_populated(col[i]);
    let bitmap = encode_null_bitmap(n, present);
    let values = match sigil {
        b'=' => write_fixed_buffer(n, col, |c| match c {
            Cell::I64(v) => v.to_le_bytes(),
            Cell::Null | Cell::Absent => 0i64.to_le_bytes(),
            _ => [0u8; 8],
        }),
        b'~' => write_fixed_buffer(n, col, |c| match c {
            Cell::F64(v) => v.to_le_bytes(),
            Cell::Null | Cell::Absent => 0f64.to_le_bytes(),
            _ => [0u8; 8],
        }),
        b'?' => write_fixed_buffer(n, col, |c| match c {
            Cell::Bool(v) => {
                let mut b = [0u8; 8];
                b[0] = if v { 1 } else { 0 };
                b
            }
            Cell::Null => [0u8; 8],
            Cell::Absent => [0u8; 8],
            _ => [0u8; 8],
        }),
        b'@' => write_fixed_buffer(n, col, |c| match c {
            Cell::Time(v) => v.to_le_bytes(),
            Cell::Null | Cell::Absent => 0i64.to_le_bytes(),
            _ => [0u8; 8],
        }),
        b'"' | b'$' | b'<' => return Err(NxsError::UnsupportedFieldType),
        _ => write_fixed_buffer(n, col, |c| match c {
            Cell::I64(v) => v.to_le_bytes(),
            Cell::Null | Cell::Absent => 0i64.to_le_bytes(),
            _ => [0u8; 8],
        }),
    };
    let mut out = bitmap;
    out.extend_from_slice(&values);
    Ok(out)
}

pub(crate) fn sigils_for_keys(keys: &[String], rows: &[RecordRow]) -> Vec<u8> {
    keys.iter()
        .enumerate()
        .map(|(fi, _)| {
            for row in rows {
                let c = row.cells.get(fi).copied().unwrap_or(Cell::Absent);
                if c != Cell::Absent {
                    return c.sigil();
                }
            }
            b'='
        })
        .collect()
}

/// Build a sealed columnar `.nxb` from record rows.
pub fn finish_columnar(keys: &[String], rows: &[RecordRow]) -> Result<Vec<u8>> {
    let n = rows.len();
    let sigils = sigils_for_keys(keys, rows);
    let schema_bytes = build_schema(
        &keys.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        &sigils,
    );
    let dict_hash = murmur3_64(&schema_bytes);

    let mut data = Vec::new();
    let mut tail_entries: Vec<(u16, u64, u64)> = Vec::new();
    for fi in 0..keys.len() {
        let col: Vec<Cell> = rows
            .iter()
            .map(|r| r.cells.get(fi).copied().unwrap_or(Cell::Absent))
            .collect();
        let field_buf = encode_field_column(n, &col, sigils[fi])?;
        let offset = 32 + schema_bytes.len() as u64 + data.len() as u64;
        let length = field_buf.len() as u64;
        tail_entries.push((fi as u16, offset, length));
        data.extend_from_slice(&field_buf);
    }

    let tail_index_offset = 32 + schema_bytes.len() as u64 + data.len() as u64;
    let mut tail = Vec::new();
    for (fid, off, len) in &tail_entries {
        tail.extend_from_slice(&fid.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&off.to_le_bytes());
        tail.extend_from_slice(&len.to_le_bytes());
    }
    tail.extend_from_slice(&tail_index_offset.to_le_bytes());
    tail.extend_from_slice(&(n as u64).to_le_bytes());
    tail.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());

    let flags = FLAG_SCHEMA_EMBEDDED | FLAG_COLUMNAR;
    let mut out = Vec::with_capacity(32 + schema_bytes.len() + data.len() + tail.len());
    out.extend_from_slice(&MAGIC_FILE.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&dict_hash.to_le_bytes());
    out.extend_from_slice(&tail_index_offset.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&schema_bytes);
    out.extend_from_slice(&data);
    out.extend_from_slice(&tail);
    Ok(out)
}

/// Build a sealed PAX `.nxb`.
pub fn finish_pax(keys: &[String], rows: &[RecordRow], page_size: u32) -> Result<Vec<u8>> {
    if page_size == 0 {
        return Err(NxsError::ParseError("page_size must be > 0".into()));
    }
    let n = rows.len();
    let sigils = sigils_for_keys(keys, rows);
    let schema_bytes = build_schema(
        &keys.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        &sigils,
    );
    let dict_hash = murmur3_64(&schema_bytes);

    let mut data = Vec::new();
    let mut pages: Vec<(u32, u64, u32, u64, u32)> = Vec::new();
    let mut page_idx = 0u32;
    let mut rec_start = 0u64;
    let mut i = 0usize;
    while i < n {
        let count = ((n - i) as u32).min(page_size);
        let page_records = &rows[i..i + count as usize];
        let page_off = 32 + schema_bytes.len() as u64 + data.len() as u64;
        let page_bytes = encode_page(
            page_idx,
            rec_start,
            count,
            keys.len(),
            &sigils,
            page_records,
        )?;
        let page_len = page_bytes.len() as u32;
        pages.push((page_idx, rec_start, count, page_off, page_len));
        data.extend_from_slice(&page_bytes);
        page_idx += 1;
        rec_start += count as u64;
        i += count as usize;
    }

    let tail_index_offset = 32 + schema_bytes.len() as u64 + data.len() as u64;
    let mut tail = Vec::new();
    for (pidx, rstart, rc, poff, plen) in &pages {
        tail.extend_from_slice(&pidx.to_le_bytes());
        tail.extend_from_slice(&rstart.to_le_bytes());
        tail.extend_from_slice(&rc.to_le_bytes());
        tail.extend_from_slice(&poff.to_le_bytes());
        tail.extend_from_slice(&plen.to_le_bytes());
    }
    tail.extend_from_slice(&tail_index_offset.to_le_bytes());
    tail.extend_from_slice(&(n as u64).to_le_bytes());
    tail.extend_from_slice(&(pages.len() as u32).to_le_bytes());
    tail.extend_from_slice(&page_size.to_le_bytes());
    tail.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());

    let flags = FLAG_SCHEMA_EMBEDDED | FLAG_PAX;
    let mut out = Vec::with_capacity(32 + schema_bytes.len() + data.len() + tail.len());
    out.extend_from_slice(&MAGIC_FILE.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&dict_hash.to_le_bytes());
    out.extend_from_slice(&tail_index_offset.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes());
    out.extend_from_slice(&schema_bytes);
    out.extend_from_slice(&data);
    out.extend_from_slice(&tail);
    Ok(out)
}

pub(crate) fn encode_page(
    page_index: u32,
    record_start: u64,
    record_count: u32,
    field_count: usize,
    sigils: &[u8],
    rows: &[RecordRow],
) -> Result<Vec<u8>> {
    let n = rows.len();
    let mut body = Vec::new();
    for fi in 0..field_count {
        let col: Vec<Cell> = rows
            .iter()
            .map(|r| r.cells.get(fi).copied().unwrap_or(Cell::Absent))
            .collect();
        let sig = sigils.get(fi).copied().unwrap_or(b'=');
        body.extend_from_slice(&encode_field_column(n, &col, sig)?);
    }
    let header_len = 4 + 4 + 8 + 4 + 2 + 2; // 24
    let page_len = header_len + body.len() + 4;
    let mut page = Vec::with_capacity(page_len);
    page.extend_from_slice(&MAGIC_PAGE.to_le_bytes());
    page.extend_from_slice(&page_index.to_le_bytes());
    page.extend_from_slice(&record_start.to_le_bytes());
    page.extend_from_slice(&record_count.to_le_bytes());
    page.extend_from_slice(&(field_count as u16).to_le_bytes());
    page.extend_from_slice(&0u16.to_le_bytes());
    page.extend_from_slice(&body);
    page.extend_from_slice(&(page_len as u32).to_le_bytes());
    while page.len() % 8 != 0 {
        page.push(0);
    }
    Ok(page)
}

/// Compile parsed fields with the selected layout.
pub fn compile_fields(fields: &[Field], opts: &CompileOptions) -> Result<Vec<u8>> {
    match opts.layout {
        Layout::Row => {
            let mut compiler = crate::compiler::Compiler::new();
            compiler.compile(fields)
        }
        Layout::Columnar | Layout::Pax => {
            let (keys, rows) = records_from_fields(fields)?;
            if opts.layout == Layout::Columnar {
                finish_columnar(&keys, &rows)
            } else {
                let ps = if opts.page_size == 0 {
                    4096
                } else {
                    opts.page_size
                };
                finish_pax(&keys, &rows, ps)
            }
        }
    }
}

/// Build columnar file from row-oriented writer buffers (conformance generator).
pub fn columnar_from_writer(w: &NxsWriter<'_>) -> Result<Vec<u8>> {
    let keys: Vec<String> = w.schema_keys().to_vec();
    let n = w.record_offsets().len();
    let width = keys.len();
    let mut rows: Vec<RecordRow> = vec![
        RecordRow {
            cells: vec![Cell::Absent; width]
        };
        n
    ];

    for (ri, &rel_off) in w.record_offsets().iter().enumerate() {
        let obj_off = rel_off as usize;
        let cells = decode_row_object(w.data_buf(), obj_off, width, w.slot_sigils())?;
        rows[ri] = RecordRow { cells };
    }
    finish_columnar(&keys, &rows)
}

fn decode_row_object(buf: &[u8], obj_off: usize, width: usize, sigils: &[u8]) -> Result<Vec<Cell>> {
    const MAGIC_OBJ: u32 = 0x4E59_584F;
    if obj_off + 8 > buf.len() {
        return Err(NxsError::OutOfBounds);
    }
    if u32::from_le_bytes(buf[obj_off..obj_off + 4].try_into().unwrap()) != MAGIC_OBJ {
        return Err(NxsError::BadMagic);
    }
    let mut cells = vec![Cell::Absent; width];
    let mut p = obj_off + 8;
    let mut slot = 0usize;
    let mut present = vec![false; width];
    while slot < width {
        if p >= buf.len() {
            return Err(NxsError::OutOfBounds);
        }
        let b = buf[p];
        p += 1;
        let bits = b & 0x7F;
        for bit in 0..7 {
            if slot >= width {
                break;
            }
            present[slot] = (bits >> bit) & 1 != 0;
            slot += 1;
        }
        if b & 0x80 == 0 {
            break;
        }
    }
    let table_start = p;
    let mut rank = 0u16;
    for s in 0..width {
        if !present[s] {
            continue;
        }
        let ot = table_start + (rank as usize) * 2;
        if ot + 2 > buf.len() {
            return Err(NxsError::OutOfBounds);
        }
        let rel = u16::from_le_bytes(
            buf[ot..ot + 2]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        );
        let off = obj_off + rel as usize;
        let sig = sigils.get(s).copied().unwrap_or(b'=');
        cells[s] = read_cell_at(buf, off, sig)?;
        rank += 1;
    }
    Ok(cells)
}

fn read_cell_at(buf: &[u8], off: usize, sigil: u8) -> Result<Cell> {
    match sigil {
        b'=' => Ok(Cell::I64(i64::from_le_bytes(
            buf[off..off + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ))),
        b'~' => Ok(Cell::F64(f64::from_le_bytes(
            buf[off..off + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ))),
        b'?' => Ok(Cell::Bool(buf[off] != 0)),
        b'@' => Ok(Cell::Time(i64::from_le_bytes(
            buf[off..off + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ))),
        b'^' => Ok(Cell::Null),
        b'"' | b'$' | b'<' => Err(NxsError::UnsupportedFieldType),
        _ => Err(NxsError::OutOfBounds),
    }
}

pub fn footer_size(flags: u16) -> usize {
    if flags & FLAG_PAX != 0 {
        FOOTER_PAX
    } else if flags & FLAG_COLUMNAR != 0 {
        FOOTER_COLUMNAR
    } else {
        FOOTER_ROW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat8_records(n: usize, dense: bool) -> (Vec<String>, Vec<RecordRow>) {
        let keys = vec!["id".into(), "score".into(), "active".into(), "ts".into()];
        let mut rows = Vec::new();
        for i in 0..n {
            let mut cells = vec![Cell::Absent; 4];
            if dense || i % 10 == 0 {
                cells[0] = Cell::I64(i as i64);
                cells[1] = Cell::F64(i as f64 * 0.5);
                cells[2] = Cell::Bool(i % 2 == 0);
                cells[3] = Cell::Time(i as i64 * 1_000_000);
            }
            rows.push(RecordRow { cells });
        }
        (keys, rows)
    }

    #[test]
    fn columnar_roundtrip_magic() {
        let (keys, rows) = flat8_records(100, true);
        let bytes = finish_columnar(&keys, &rows).unwrap();
        assert_eq!(&bytes[0..4], &MAGIC_FILE.to_le_bytes());
        let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        assert!(flags & FLAG_COLUMNAR != 0);
        assert_eq!(
            u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap()),
            MAGIC_FOOTER
        );
    }

    #[test]
    fn pax_roundtrip_footer() {
        let (keys, rows) = flat8_records(1000, true);
        let bytes = finish_pax(&keys, &rows, 256).unwrap();
        let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        assert!(flags & FLAG_PAX != 0);
        assert_eq!(footer_size(flags), FOOTER_PAX);
    }

    #[test]
    fn invalid_flags_rejected() {
        assert!(validate_preamble_flags(FLAG_COLUMNAR | FLAG_PAX).is_err());
    }
}
