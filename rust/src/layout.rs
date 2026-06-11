//! Columnar and PAX `.nxb` layout writers (OLAP.md v0.1).
//!
//! Phase 1: dense numeric columnar (`FLAG_COLUMNAR`).
//! Phase 2: PAX pages with per-page column groups (`FLAG_PAX`).
//! Phase 3: variable-length string/binary columns (u32 offsets + values tail).

// Re-export shared constants so callers that `use crate::layout::…` still compile.
pub use crate::compact::CompactOptions;
pub use crate::consts::{
    FLAG_COLUMNAR, FLAG_DELTA_TAIL, FLAG_DENSE_FRAMES, FLAG_NARROW_CELLS, FLAG_PACKED_BOOLS,
    FLAG_PAX, FLAG_SCHEMA_EMBEDDED, FLAG_V13_COMPACT_MASK, MAGIC_FILE, MAGIC_FOOTER, MAGIC_PAGE,
    VERSION, VERSION_V13,
};
use crate::error::{NxsError, Result};
use crate::parser::{Field, Value};
use crate::writer::{build_schema, murmur3_64, NxsWriter, Schema, Slot};
use std::collections::HashMap;

const FOOTER_ROW: usize = 12;
const FOOTER_COLUMNAR: usize = 20;
const FOOTER_PAX: usize = 28;

/// Minimum driver release that decodes v1.3 compact files (rejection messages cite this).
pub const DECODER_MIN_VERSION_V13: &str = "1.3.0";

/// Batch `nxs compile` default: emit v1.3 compact when `true`.
///
/// `--legacy-v12` forces v1.2 row layout for one release cycle.
pub const COMPILE_DEFAULT_COMPACT: bool = true;

/// Resolve whether batch compile emits v1.3 compact row encoding.
pub fn resolve_compact_encoding(legacy_v12: bool) -> Option<CompactOptions> {
    if legacy_v12 || !COMPILE_DEFAULT_COMPACT {
        None
    } else {
        Some(CompactOptions::compact())
    }
}

fn row_compact_opts(opts: &CompileOptions) -> Option<CompactOptions> {
    if opts.legacy_v12 {
        return None;
    }
    opts.compact
        .clone()
        .or_else(|| resolve_compact_encoding(false))
}

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
    pub compact: Option<CompactOptions>,
    /// When true, emit v1.2 row layout even if `COMPILE_DEFAULT_COMPACT` is enabled.
    pub legacy_v12: bool,
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
    if flags & FLAG_V13_COMPACT_MASK != 0 && (col || pax) {
        return Err(NxsError::IncompatibleFlags);
    }
    crate::compact::validate_reader_flags(flags, true)?;
    Ok(())
}

// ── Record model for layout emitters ─────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Absent,
    Null,
    I64(i64),
    F64(f64),
    Bool(bool),
    Time(i64),
    Str(String),
    Binary(Vec<u8>),
}

impl Cell {
    fn from_value(v: &Value) -> Result<Self> {
        match v {
            Value::Int(n) => Ok(Cell::I64(*n)),
            Value::Float(f) => Ok(Cell::F64(*f)),
            Value::Bool(b) => Ok(Cell::Bool(*b)),
            Value::Time(ns) => Ok(Cell::Time(*ns)),
            Value::Null => Ok(Cell::Null),
            Value::Str(s) => Ok(Cell::Str(s.clone())),
            Value::Binary(b) => Ok(Cell::Binary(b.clone())),
            Value::Keyword(_) => Err(NxsError::UnsupportedFieldType),
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
            Cell::Str(_) => b'"',
            Cell::Binary(_) => b'<',
            Cell::Null => b'^',
            Cell::Absent => 0,
        }
    }
}

/// True when the schema sigil denotes a variable-length column (`"` string, `<` binary).
pub fn is_var_sigil(sigil: u8) -> bool {
    matches!(sigil, b'"' | b'<')
}

/// Byte length of one encoded field column (null bitmap + value buffer(s)).
pub fn column_sector_len(sector: &[u8], record_count: usize, sigil: u8) -> Result<usize> {
    let bm_len = null_bitmap_bytes(record_count);
    if sector.len() < bm_len {
        return Err(NxsError::OutOfBounds);
    }
    if is_var_sigil(sigil) {
        let off_bytes = record_count
            .checked_add(1)
            .and_then(|n| n.checked_mul(4))
            .ok_or(NxsError::OutOfBounds)?;
        if sector.len() < bm_len.checked_add(off_bytes).ok_or(NxsError::OutOfBounds)? {
            return Err(NxsError::OutOfBounds);
        }
        let end_off = bm_len
            .checked_add(record_count.checked_mul(4).ok_or(NxsError::OutOfBounds)?)
            .ok_or(NxsError::OutOfBounds)?;
        let last = u32::from_le_bytes(
            sector[end_off..end_off + 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        bm_len
            .checked_add(off_bytes)
            .and_then(|x| x.checked_add(last))
            .ok_or(NxsError::OutOfBounds)
    } else {
        let cells = record_count.checked_mul(8).ok_or(NxsError::OutOfBounds)?;
        bm_len.checked_add(cells).ok_or(NxsError::OutOfBounds)
    }
}

/// Column tail after the null bitmap: `(N+1)` little-endian u32 offsets, then UTF-8/raw bytes.
pub fn col_var_parts(sector: &[u8], record_count: usize) -> Result<(&[u8], &[u8], &[u8])> {
    let bm_len = null_bitmap_bytes(record_count);
    let off_bytes = record_count
        .checked_add(1)
        .and_then(|n| n.checked_mul(4))
        .ok_or(NxsError::OutOfBounds)?;
    if sector.len() < bm_len.saturating_add(off_bytes) {
        return Err(NxsError::OutOfBounds);
    }
    let bm = &sector[..bm_len];
    let offsets = &sector[bm_len..bm_len + off_bytes];
    let values = &sector[bm_len + off_bytes..];
    Ok((bm, offsets, values))
}

/// Read one UTF-8 string cell from a variable-length column sector.
pub fn var_str_at<'a>(offsets: &'a [u8], values: &'a [u8], record_index: usize) -> Option<&'a str> {
    let need = record_index.checked_add(2).and_then(|n| n.checked_mul(4))?;
    if offsets.len() < need {
        return None;
    }
    let start = u32::from_le_bytes(
        offsets[record_index * 4..record_index * 4 + 4]
            .try_into()
            .ok()?,
    ) as usize;
    let end = u32::from_le_bytes(
        offsets[record_index * 4 + 4..record_index * 4 + 8]
            .try_into()
            .ok()?,
    ) as usize;
    if end < start || end > values.len() {
        return None;
    }
    std::str::from_utf8(&values[start..end]).ok()
}

/// Read one binary cell from a variable-length column sector.
pub fn var_binary_at<'a>(
    offsets: &'a [u8],
    values: &'a [u8],
    record_index: usize,
) -> Option<&'a [u8]> {
    let need = record_index.checked_add(2).and_then(|n| n.checked_mul(4))?;
    if offsets.len() < need {
        return None;
    }
    let start = u32::from_le_bytes(
        offsets[record_index * 4..record_index * 4 + 4]
            .try_into()
            .ok()?,
    ) as usize;
    let end = u32::from_le_bytes(
        offsets[record_index * 4 + 4..record_index * 4 + 8]
            .try_into()
            .ok()?,
    ) as usize;
    if end < start || end > values.len() {
        return None;
    }
    Some(&values[start..end])
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

/// Round `n` bits up to the nearest multiple of 8 bytes (64 bits).
pub(crate) fn null_bitmap_bytes(n: usize) -> usize {
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

fn cell_populated(c: &Cell) -> bool {
    !matches!(c, Cell::Absent)
}

fn write_fixed_buffer(n: usize, cells: &[&Cell], encode: impl Fn(&Cell) -> [u8; 8]) -> Vec<u8> {
    let mut buf = vec![0u8; n * 8];
    for (i, c) in cells.iter().enumerate().take(n) {
        if cell_populated(c) {
            buf[i * 8..(i + 1) * 8].copy_from_slice(&encode(c));
        }
    }
    buf
}

fn encode_var_column(n: usize, col: &[&Cell]) -> Result<Vec<u8>> {
    let present = |i: usize| cell_populated(col[i]);
    let bitmap = encode_null_bitmap(n, present);
    let mut offsets: Vec<u32> = Vec::with_capacity(n + 1);
    let mut values: Vec<u8> = Vec::new();
    offsets.push(0);
    for cell in col.iter().take(n) {
        if !cell_populated(cell) {
            offsets.push(*offsets.last().unwrap_or(&0));
            continue;
        }
        match cell {
            Cell::Str(s) => values.extend_from_slice(s.as_bytes()),
            Cell::Binary(b) => values.extend_from_slice(b),
            _ => {}
        }
        let end = values.len();
        if end > u32::MAX as usize {
            return Err(NxsError::Overflow);
        }
        offsets.push(end as u32);
    }
    let mut out = bitmap;
    for o in offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&values);
    Ok(out)
}

fn encode_field_column(n: usize, col: &[&Cell], sigil: u8) -> Result<Vec<u8>> {
    if is_var_sigil(sigil) {
        return encode_var_column(n, col);
    }
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
                b[0] = if *v { 1 } else { 0 };
                b
            }
            Cell::Null | Cell::Absent => [0u8; 8],
            _ => [0u8; 8],
        }),
        b'@' => write_fixed_buffer(n, col, |c| match c {
            Cell::Time(v) => v.to_le_bytes(),
            Cell::Null | Cell::Absent => 0i64.to_le_bytes(),
            _ => [0u8; 8],
        }),
        b'$' => return Err(NxsError::UnsupportedFieldType),
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
                let c = row.cells.get(fi).cloned().unwrap_or(Cell::Absent);
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
    for (fi, sigil) in sigils.iter().enumerate() {
        let col: Vec<&Cell> = rows
            .iter()
            .map(|r| r.cells.get(fi).unwrap_or(&Cell::Absent))
            .collect();
        let field_buf = encode_field_column(n, &col, *sigil)?;
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
        let col: Vec<&Cell> = rows
            .iter()
            .map(|r| r.cells.get(fi).unwrap_or(&Cell::Absent))
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

/// Emit a row-layout `.nxb` via [`NxsWriter`] with optional v1.3 compact encoding.
pub fn finish_row(
    keys: &[String],
    rows: &[RecordRow],
    compact: Option<&CompactOptions>,
) -> Result<Vec<u8>> {
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let schema = Schema::new(&key_refs);
    let mut w = NxsWriter::with_compact(&schema, compact.cloned());
    for row in rows {
        w.begin_object();
        for (fi, cell) in row.cells.iter().enumerate() {
            let slot = Slot(fi as u16);
            match cell {
                Cell::Absent => {}
                Cell::Null => w.write_null(slot),
                Cell::I64(v) => w.write_i64(slot, *v),
                Cell::F64(v) => w.write_f64(slot, *v),
                Cell::Bool(v) => w.write_bool(slot, *v),
                Cell::Time(v) => w.write_time(slot, *v),
                Cell::Str(v) => w.write_str(slot, v),
                Cell::Binary(_) => {
                    return Err(NxsError::UnsupportedFieldType);
                }
            }
        }
        w.end_object();
    }
    Ok(w.finish())
}

/// Compile parsed fields with the selected layout.
pub fn compile_fields(fields: &[Field], opts: &CompileOptions) -> Result<Vec<u8>> {
    match opts.layout {
        Layout::Row => {
            if let Some(ref compact) = row_compact_opts(opts) {
                match records_from_fields(fields) {
                    Ok((keys, rows)) => finish_row(&keys, &rows, Some(compact)),
                    // Nested NYXO/NYXL values still use the v1.2 row compiler until a compact nested path exists.
                    Err(NxsError::ParseError(_)) | Err(NxsError::UnsupportedFieldType) => {
                        let mut compiler = crate::compiler::Compiler::new();
                        compiler.compile(fields)
                    }
                    Err(e) => Err(e),
                }
            } else {
                let mut compiler = crate::compiler::Compiler::new();
                compiler.compile(fields)
            }
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
        b'"' => {
            if off + 4 > buf.len() {
                return Err(NxsError::OutOfBounds);
            }
            let len = u32::from_le_bytes(
                buf[off..off + 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize;
            if off + 4 + len > buf.len() {
                return Err(NxsError::OutOfBounds);
            }
            let s = std::str::from_utf8(&buf[off + 4..off + 4 + len])
                .map_err(|_| NxsError::ParseError("invalid UTF-8 in string field".into()))?;
            Ok(Cell::Str(s.to_string()))
        }
        b'<' => {
            if off + 4 > buf.len() {
                return Err(NxsError::OutOfBounds);
            }
            let len = u32::from_le_bytes(
                buf[off..off + 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize;
            if off + 4 + len > buf.len() {
                return Err(NxsError::OutOfBounds);
            }
            Ok(Cell::Binary(buf[off + 4..off + 4 + len].to_vec()))
        }
        b'$' => Err(NxsError::UnsupportedFieldType),
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

    #[test]
    fn resolve_compact_encoding_legacy_wins() {
        assert!(resolve_compact_encoding(true).is_none());
        if COMPILE_DEFAULT_COMPACT {
            assert!(resolve_compact_encoding(false).is_some());
        } else {
            assert!(resolve_compact_encoding(false).is_none());
        }
    }
}
