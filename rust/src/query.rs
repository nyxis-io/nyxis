//! Zero-allocation query engine for .nxb files.
//!
//! # Usage
//!
//! ```no_run
//! use nxs::query::{Reader, And, eq, gt};
//!
//! let data = std::fs::read("data.nxb").unwrap();
//! let reader = Reader::new(&data).unwrap();
//!
//! for record in reader.where_pred(And(eq("active", true), gt("score", 80.0f64))) {
//!     println!("{:?}", record.get_str("username"));
//! }
//! ```

use crate::error::{NxsError, Result};
use crate::layout::{col_var_parts, column_sector_len, is_var_sigil, var_str_at};

// ── Format constants ──────────────────────────────────────────────────────────
use crate::consts::{
    FLAG_COLUMNAR, FLAG_PAX, FLAG_SCHEMA_EMBEDDED, MAGIC_FILE, MAGIC_FOOTER, MAGIC_OBJ,
};

/// Data-sector layout (OLAP.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Row,
    Columnar,
    Pax,
}

/// Bytes per entry in the PAX tail index (see conformance `generate.rs`).
const PAX_TAIL_ENTRY_BYTES: usize = 28;

fn footer_size(flags: u16) -> usize {
    if flags & FLAG_PAX != 0 {
        28
    } else if flags & FLAG_COLUMNAR != 0 {
        20
    } else {
        12
    }
}

fn null_bitmap_bytes(n: usize) -> usize {
    let raw = (n + 7) / 8;
    (raw + 7) & !7
}

fn col_bit(bm: &[u8], rec: usize) -> bool {
    (bm[rec / 8] >> (rec % 8)) & 1 == 1
}

// ── Reader ────────────────────────────────────────────────────────────────────

/// A zero-copy reader for a .nxb buffer.
/// Parses the preamble and schema on construction; record data is accessed lazily.
pub struct Reader<'a> {
    data: &'a [u8],
    keys: Vec<String>,
    key_sigils: Vec<u8>,
    key_index: std::collections::HashMap<String, usize>,
    record_count: usize,
    tail_start: usize,
    layout: Layout,
    col_buf_off: Vec<u64>,
    col_buf_len: Vec<u64>,
}

impl<'a> Reader<'a> {
    /// Validate the file header and build the schema index.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(NxsError::OutOfBounds);
        }
        if u32::from_le_bytes(data[0..4].try_into().map_err(|_| NxsError::OutOfBounds)?)
            != MAGIC_FILE
        {
            return Err(NxsError::BadMagic);
        }
        if u32::from_le_bytes(
            data[data.len() - 4..]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) != MAGIC_FOOTER
        {
            return Err(NxsError::BadMagic);
        }

        let flags = u16::from_le_bytes(data[6..8].try_into().map_err(|_| NxsError::OutOfBounds)?);
        if flags & FLAG_COLUMNAR != 0 && flags & FLAG_PAX != 0 {
            return Err(NxsError::InvalidFlags);
        }
        let preamble_tail =
            u64::from_le_bytes(data[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?);
        if flags & FLAG_COLUMNAR != 0 && preamble_tail == 0 {
            return Err(NxsError::IncompatibleFlags);
        }

        let (keys, key_sigils, _schema_end) = if flags & FLAG_SCHEMA_EMBEDDED != 0 {
            parse_schema(data, 32)?
        } else {
            (vec![], vec![], 32)
        };

        let key_index: std::collections::HashMap<String, usize> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();

        let (layout, record_count, tail_start, col_buf_off, col_buf_len) =
            if flags & FLAG_COLUMNAR != 0 {
                let footer = footer_size(flags);
                let fo = data.len() - footer;
                let tail_ptr = u64::from_le_bytes(
                    data[fo..fo + 8]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                ) as usize;
                let record_count = u64::from_le_bytes(
                    data[fo + 8..fo + 16]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                ) as usize;
                let kc = keys.len();
                let tail_end = tail_ptr
                    .checked_add(kc.checked_mul(20).ok_or(NxsError::OutOfBounds)?)
                    .ok_or(NxsError::OutOfBounds)?;
                if tail_ptr >= fo || tail_end > fo {
                    return Err(NxsError::OutOfBounds);
                }
                let mut off = vec![0u64; kc];
                let mut len = vec![0u64; kc];
                for i in 0..kc {
                    let e = tail_ptr + i * 20;
                    let fid = u16::from_le_bytes(
                        data[e..e + 2]
                            .try_into()
                            .map_err(|_| NxsError::OutOfBounds)?,
                    ) as usize;
                    if fid >= kc {
                        return Err(NxsError::OutOfBounds);
                    }
                    off[fid] = u64::from_le_bytes(
                        data[e + 4..e + 12]
                            .try_into()
                            .map_err(|_| NxsError::OutOfBounds)?,
                    );
                    len[fid] = u64::from_le_bytes(
                        data[e + 12..e + 20]
                            .try_into()
                            .map_err(|_| NxsError::OutOfBounds)?,
                    );
                }
                (Layout::Columnar, record_count, tail_ptr, off, len)
            } else if flags & FLAG_PAX != 0 {
                let footer = footer_size(flags);
                let fo = data.len() - footer;
                let tail_ptr = u64::from_le_bytes(
                    data[fo..fo + 8]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                ) as usize;
                let record_count = u64::from_le_bytes(
                    data[fo + 8..fo + 16]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                ) as usize;
                (Layout::Pax, record_count, tail_ptr, vec![], vec![])
            } else {
                let mut tail_ptr = preamble_tail as usize;
                if tail_ptr == 0 {
                    if data.len() < 44 {
                        return Err(NxsError::OutOfBounds);
                    }
                    tail_ptr = u64::from_le_bytes(
                        data[data.len() - 12..data.len() - 4]
                            .try_into()
                            .map_err(|_| NxsError::OutOfBounds)?,
                    ) as usize;
                }
                if tail_ptr + 4 > data.len() {
                    return Err(NxsError::OutOfBounds);
                }
                let record_count =
                    u32::from_le_bytes(data[tail_ptr..tail_ptr + 4].try_into().unwrap()) as usize;
                (Layout::Row, record_count, tail_ptr + 4, vec![], vec![])
            };

        Ok(Self {
            data,
            keys,
            key_sigils,
            key_index,
            record_count,
            tail_start,
            layout,
            col_buf_off,
            col_buf_len,
        })
    }

    /// Row, columnar, or PAX layout.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Sum `key` as f64 across all records (uses column buffers when columnar/PAX).
    pub fn col_sum_f64(&self, key: &str) -> Option<f64> {
        let slot = self.slot(key)?;
        match self.layout {
            Layout::Row => {
                let mut sum = 0.0;
                let mut any = false;
                for rec in self.all() {
                    if let Some(v) = rec.get_f64(key) {
                        sum += v;
                        any = true;
                    }
                }
                any.then_some(sum)
            }
            Layout::Columnar => {
                let (bm, vals) = self.col_field_parts(slot).ok()?;
                Some(crate::col_reduce::sum_f64_column(
                    vals,
                    bm,
                    self.record_count,
                ))
            }
            Layout::Pax => {
                let mut sum = 0.0;
                for i in 0..self.record_count {
                    if let Some(v) = self.pax_get_f64(i, slot) {
                        sum += v;
                    }
                }
                Some(sum)
            }
        }
    }

    /// Zero-copy slice of a column's dense numeric value buffer (columnar only).
    pub fn col_buffer(&self, key: &str) -> Option<&[u8]> {
        if self.layout != Layout::Columnar {
            return None;
        }
        let slot = self.slot(key)?;
        if is_var_sigil(self.key_sigils.get(slot).copied().unwrap_or(0)) {
            return None;
        }
        let (_, vals) = self.col_field_parts(slot).ok()?;
        Some(vals)
    }

    /// Zero-copy string/binary column (`offsets` + `values`); columnar only.
    pub fn col_var_buffer(&self, key: &str) -> Result<crate::arrow_project::VarColumnView<'_>> {
        if self.layout != Layout::Columnar {
            return Err(NxsError::UnsupportedFieldType);
        }
        let slot = self.slot(key).ok_or(NxsError::OutOfBounds)?;
        if !is_var_sigil(self.key_sigils.get(slot).copied().unwrap_or(0)) {
            return Err(NxsError::UnsupportedFieldType);
        }
        let (bm, offsets, values) = self.col_field_var_parts(slot)?;
        Ok(crate::arrow_project::VarColumnView {
            null_bitmap: bm,
            offsets,
            values,
            record_count: self.record_count,
        })
    }

    fn pax_column_sector(&self, page_idx: usize, slot: usize) -> Result<&[u8]> {
        const MAGIC_PAGE: u32 = 0x4E58_5350;
        let e = self.tail_start + page_idx * PAX_TAIL_ENTRY_BYTES;
        let poff = u64::from_le_bytes(
            self.data
                .get(e + 16..e + 24)
                .ok_or(NxsError::OutOfBounds)?
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        if poff + 24 > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        if u32::from_le_bytes(
            self.data[poff..poff + 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) != MAGIC_PAGE
        {
            return Err(NxsError::InvalidPageMagic);
        }
        let rc = u32::from_le_bytes(
            self.data[poff + 16..poff + 20]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        let field_count = u16::from_le_bytes(
            self.data[poff + 20..poff + 22]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        if slot >= field_count {
            return Err(NxsError::OutOfBounds);
        }
        let mut body = poff + 24;
        for fi in 0..slot {
            let sig = self.key_sigils.get(fi).copied().unwrap_or(b'=');
            let slen = column_sector_len(&self.data[body..], rc, sig)?;
            body += slen;
        }
        let sig = self.key_sigils.get(slot).copied().unwrap_or(b'=');
        let slen = column_sector_len(&self.data[body..], rc, sig)?;
        if body + slen > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        Ok(&self.data[body..body + slen])
    }

    fn pax_page_field_var_parts(
        &self,
        page_idx: usize,
        slot: usize,
    ) -> Result<(&[u8], &[u8], &[u8])> {
        let sector = self.pax_column_sector(page_idx, slot)?;
        let rc = self
            .pax_page_rec_count(page_idx)
            .ok_or(NxsError::OutOfBounds)? as usize;
        col_var_parts(sector, rc)
    }

    fn pax_locate_record(&self, record_index: usize) -> Option<(usize, usize)> {
        let mut lo = 0i32;
        let mut hi = self.page_count().saturating_sub(1) as i32;
        while lo <= hi {
            let mid = ((lo + hi) / 2) as usize;
            let start = self.pax_page_rec_start(mid)?;
            let count = self.pax_page_rec_count(mid)?;
            if (record_index as u64) < start {
                hi = mid as i32 - 1;
            } else if record_index >= start as usize + count as usize {
                lo = mid as i32 + 1;
            } else {
                let local = record_index - start as usize;
                return Some((mid, local));
            }
        }
        None
    }

    fn pax_get_f64(&self, record_index: usize, slot: usize) -> Option<f64> {
        let (pi, local) = self.pax_locate_record(record_index)?;
        if is_var_sigil(*self.key_sigils.get(slot)?) {
            return None;
        }
        let (bm, vals) = self.pax_page_field_parts(pi, slot).ok()?;
        if !col_bit(bm, local) {
            return None;
        }
        let off = local * 8;
        Some(f64::from_le_bytes(vals.get(off..off + 8)?.try_into().ok()?))
    }

    fn pax_get_i64(&self, record_index: usize, slot: usize) -> Option<i64> {
        let (pi, local) = self.pax_locate_record(record_index)?;
        if is_var_sigil(*self.key_sigils.get(slot)?) {
            return None;
        }
        let (bm, vals) = self.pax_page_field_parts(pi, slot).ok()?;
        if !col_bit(bm, local) {
            return None;
        }
        let off = local * 8;
        Some(i64::from_le_bytes(vals.get(off..off + 8)?.try_into().ok()?))
    }

    fn pax_get_bool(&self, record_index: usize, slot: usize) -> Option<bool> {
        let (pi, local) = self.pax_locate_record(record_index)?;
        if is_var_sigil(*self.key_sigils.get(slot)?) {
            return None;
        }
        let (bm, vals) = self.pax_page_field_parts(pi, slot).ok()?;
        if !col_bit(bm, local) {
            return None;
        }
        Some(vals.get(local * 8)? != &0)
    }

    fn pax_get_str(&self, record_index: usize, slot: usize) -> Option<&str> {
        let (pi, local) = self.pax_locate_record(record_index)?;
        if self.key_sigils.get(slot).copied() != Some(b'"') {
            return None;
        }
        let (bm, offsets, values) = self.pax_page_field_var_parts(pi, slot).ok()?;
        if !col_bit(bm, local) {
            return None;
        }
        var_str_at(offsets, values, local)
    }

    fn page_count(&self) -> usize {
        if self.layout != Layout::Pax {
            return 0;
        }
        let tp = self.tail_start;
        if tp + 4 > self.data.len() {
            return 0;
        }
        // page count stored in footer; re-read from footer
        let fo = self.data.len() - footer_size(FLAG_PAX);
        u32::from_le_bytes(self.data[fo + 16..fo + 20].try_into().unwrap_or([0; 4])) as usize
    }

    fn pax_page_rec_start(&self, page_idx: usize) -> Option<u64> {
        let e = self.tail_start + page_idx * PAX_TAIL_ENTRY_BYTES;
        Some(u64::from_le_bytes(
            self.data.get(e + 4..e + 12)?.try_into().ok()?,
        ))
    }

    fn pax_page_rec_count(&self, page_idx: usize) -> Option<u32> {
        let e = self.tail_start + page_idx * PAX_TAIL_ENTRY_BYTES;
        Some(u32::from_le_bytes(
            self.data.get(e + 12..e + 16)?.try_into().ok()?,
        ))
    }

    fn pax_page_field_parts(&self, page_idx: usize, slot: usize) -> Result<(&[u8], &[u8])> {
        let sector = self.pax_column_sector(page_idx, slot)?;
        let rc = self
            .pax_page_rec_count(page_idx)
            .ok_or(NxsError::OutOfBounds)? as usize;
        let bm_len = null_bitmap_bytes(rc);
        if sector.len() < bm_len {
            return Err(NxsError::OutOfBounds);
        }
        let vals_end = bm_len + rc * 8;
        if sector.len() < vals_end {
            return Err(NxsError::OutOfBounds);
        }
        Ok((&sector[..bm_len], &sector[bm_len..vals_end]))
    }

    /// Variable-length column parts (null bitmap, u32 offsets, values) for columnar layout.
    pub fn col_field_var_parts(&self, slot: usize) -> Result<(&[u8], &[u8], &[u8])> {
        let off = *self.col_buf_off.get(slot).ok_or(NxsError::OutOfBounds)? as usize;
        let len = *self.col_buf_len.get(slot).ok_or(NxsError::OutOfBounds)? as usize;
        if off + len > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        col_var_parts(&self.data[off..off + len], self.record_count)
    }

    fn col_field_parts(&self, slot: usize) -> Result<(&[u8], &[u8])> {
        if self
            .key_sigils
            .get(slot)
            .copied()
            .map(is_var_sigil)
            .unwrap_or(false)
        {
            return Err(NxsError::UnsupportedFieldType);
        }
        let off = *self.col_buf_off.get(slot).ok_or(NxsError::OutOfBounds)? as usize;
        let len = *self.col_buf_len.get(slot).ok_or(NxsError::OutOfBounds)? as usize;
        if off + len > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let bm_len = null_bitmap_bytes(self.record_count);
        let vals_len = self.record_count * 8;
        if len < bm_len.saturating_add(vals_len) {
            return Err(NxsError::OutOfBounds);
        }
        let sector = &self.data[off..off + len];
        Ok((&sector[..bm_len], &sector[bm_len..bm_len + vals_len]))
    }

    /// Number of top-level records in the file.
    pub fn record_count(&self) -> usize {
        self.record_count
    }

    /// Schema key names.
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// Schema sigil bytes, parallel to `keys()`.
    pub fn key_sigils(&self) -> &[u8] {
        &self.key_sigils
    }

    /// Resolve a key name to its slot index. O(1) via HashMap.
    pub fn slot(&self, key: &str) -> Option<usize> {
        self.key_index.get(key).copied()
    }

    /// Access a single record by zero-based index. O(1) via tail-index.
    pub fn record(&self, i: usize) -> Option<Record<'a, '_>> {
        if i >= self.record_count {
            return None;
        }
        let offset = if self.layout == Layout::Row {
            let entry = self.tail_start + i * 10;
            u64::from_le_bytes(self.data.get(entry + 2..entry + 10)?.try_into().ok()?) as usize
        } else {
            i
        };
        Some(Record {
            data: self.data,
            reader: self,
            offset,
        })
    }

    /// Return an iterator over all records.
    pub fn all(&'a self) -> Records<'a, 'a, AlwaysTrue> {
        Records {
            reader: self,
            pred: AlwaysTrue,
            index: 0,
        }
    }

    /// Return a lazy iterator over records matching `pred`.
    pub fn where_pred<P: Predicate>(&'a self, pred: P) -> Records<'a, 'a, P> {
        Records {
            reader: self,
            pred,
            index: 0,
        }
    }
}

// ── Record ────────────────────────────────────────────────────────────────────

/// A lazy view into a single NYXO object within the buffer.
/// Field reads decode directly from the mapped bytes — no allocation.
pub struct Record<'data, 'reader> {
    data: &'data [u8],
    reader: &'reader Reader<'data>,
    offset: usize,
}

impl<'data, 'reader> Record<'data, 'reader> {
    /// Resolve the byte offset of slot `s` within this object. Returns `None` if absent.
    fn resolve(&self, slot: usize) -> Option<usize> {
        resolve_slot(self.data, self.offset, slot)
    }

    /// Read an `i64` field.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        let slot = self.reader.slot(key)?;
        match self.reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*self.reader.key_sigils.get(slot)?) {
                    return None;
                }
                let ri = self.offset;
                let (bm, vals) = self.reader.col_field_parts(slot).ok()?;
                if !col_bit(bm, ri) {
                    return None;
                }
                let off = ri * 8;
                Some(i64::from_le_bytes(vals.get(off..off + 8)?.try_into().ok()?))
            }
            Layout::Pax => self.reader.pax_get_i64(self.offset, slot),
            Layout::Row => {
                let off = self.resolve(slot)?;
                Some(i64::from_le_bytes(
                    self.data.get(off..off + 8)?.try_into().ok()?,
                ))
            }
        }
    }

    /// Read an `f64` field.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        let slot = self.reader.slot(key)?;
        if self.reader.layout == Layout::Columnar {
            if is_var_sigil(*self.reader.key_sigils.get(slot)?) {
                return None;
            }
            let ri = self.offset;
            let (bm, vals) = self.reader.col_field_parts(slot).ok()?;
            if !col_bit(bm, ri) {
                return None;
            }
            let off = ri * 8;
            return Some(f64::from_le_bytes(vals.get(off..off + 8)?.try_into().ok()?));
        }
        if self.reader.layout == Layout::Pax {
            return self.reader.pax_get_f64(self.offset, slot);
        }
        let off = self.resolve(slot)?;
        Some(f64::from_le_bytes(
            self.data.get(off..off + 8)?.try_into().ok()?,
        ))
    }

    /// Read a `bool` field.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let slot = self.reader.slot(key)?;
        match self.reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*self.reader.key_sigils.get(slot)?) {
                    return None;
                }
                let ri = self.offset;
                let (bm, vals) = self.reader.col_field_parts(slot).ok()?;
                if !col_bit(bm, ri) {
                    return None;
                }
                Some(vals.get(ri * 8)? != &0)
            }
            Layout::Pax => self.reader.pax_get_bool(self.offset, slot),
            Layout::Row => {
                let off = self.resolve(slot)?;
                Some(*self.data.get(off)? != 0)
            }
        }
    }

    /// Read a `&str` field (zero-copy slice into the buffer).
    pub fn get_str(&self, key: &str) -> Option<&str> {
        let slot = self.reader.slot(key)?;
        match self.reader.layout {
            Layout::Columnar => {
                if self.reader.key_sigils.get(slot).copied() != Some(b'"') {
                    return None;
                }
                let ri = self.offset;
                let (bm, offsets, values) = self.reader.col_field_var_parts(slot).ok()?;
                if !col_bit(bm, ri) {
                    return None;
                }
                var_str_at(offsets, values, ri)
            }
            Layout::Pax => self.reader.pax_get_str(self.offset, slot),
            Layout::Row => {
                let off = self.resolve(slot)?;
                let len =
                    u32::from_le_bytes(self.data.get(off..off + 4)?.try_into().ok()?) as usize;
                let bytes = self.data.get(off + 4..off + 4 + len)?;
                std::str::from_utf8(bytes).ok()
            }
        }
    }

    /// Walk a dot-notated path and read the leaf as `&str`.
    /// Example: `record.get_str_path("address.city")`
    pub fn get_str_path(&self, dot_path: &str) -> Option<&str> {
        let (leaf_off, data) = self.walk_path(dot_path)?;
        let len = u32::from_le_bytes(data.get(leaf_off..leaf_off + 4)?.try_into().ok()?) as usize;
        let bytes = data.get(leaf_off + 4..leaf_off + 4 + len)?;
        std::str::from_utf8(bytes).ok()
    }

    /// Walk a dot-notated path and read the leaf as `i64`.
    pub fn get_i64_path(&self, dot_path: &str) -> Option<i64> {
        let (off, data) = self.walk_path(dot_path)?;
        Some(i64::from_le_bytes(data.get(off..off + 8)?.try_into().ok()?))
    }

    /// Walk a dot-notated path and read the leaf as `f64`.
    pub fn get_f64_path(&self, dot_path: &str) -> Option<f64> {
        let (off, data) = self.walk_path(dot_path)?;
        Some(f64::from_le_bytes(data.get(off..off + 8)?.try_into().ok()?))
    }

    /// Walk a dot-notated path and read the leaf as `bool`.
    pub fn get_bool_path(&self, dot_path: &str) -> Option<bool> {
        let (off, data) = self.walk_path(dot_path)?;
        Some(*data.get(off)? != 0)
    }

    /// Navigate all but the last path segment, returning (leaf_offset, data).
    fn walk_path(&self, dot_path: &str) -> Option<(usize, &'data [u8])> {
        // Use plain split('.') so arbitrarily deep paths work correctly.
        // Previously splitn(8, '.') silently concatenated segments 8+ into the 8th
        // key string, causing incorrect lookups with no error for paths > 8 segments.
        let mut parts = dot_path.split('.');
        let mut obj_offset = self.offset;
        let data = self.data;
        let mut part = parts.next()?;
        loop {
            let slot = self.reader.slot(part)?;
            let field_off = resolve_slot(data, obj_offset, slot)?;
            match parts.next() {
                None => return Some((field_off, data)),
                Some(next) => {
                    // intermediate: must be NYXO
                    let magic =
                        u32::from_le_bytes(data.get(field_off..field_off + 4)?.try_into().ok()?);
                    if magic != MAGIC_OBJ {
                        return None;
                    }
                    obj_offset = field_off;
                    part = next;
                }
            }
        }
    }
}

// ── Iterator ──────────────────────────────────────────────────────────────────

/// A lazy iterator over records filtered by `P`.
/// Does not allocate; predicate evaluation reads directly from the buffer.
pub struct Records<'data, 'reader, P: Predicate> {
    reader: &'reader Reader<'data>,
    pred: P,
    index: usize,
}

impl<'data, 'reader, P: Predicate> Iterator for Records<'data, 'reader, P> {
    type Item = Record<'data, 'reader>;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.reader;
        loop {
            if self.index >= r.record_count {
                return None;
            }
            let i = self.index;
            self.index += 1;
            // For Row layout: look up absolute byte offset via the per-record tail-index entry
            // (each entry is 10 bytes: 2-byte flags + 8-byte absolute offset).
            // For Columnar/PAX layout: there is no row tail-index; the record is identified
            // by its zero-based record index directly.
            let abs = match r.layout {
                Layout::Row => {
                    let entry = r.tail_start + i * 10;
                    u64::from_le_bytes(r.data.get(entry + 2..entry + 10)?.try_into().ok()?) as usize
                }
                Layout::Columnar | Layout::Pax => i,
            };
            if self.pred.test(r.data, r, abs) {
                return Some(Record {
                    data: r.data,
                    reader: r,
                    offset: abs,
                });
            }
        }
    }
}

// ── Predicates ────────────────────────────────────────────────────────────────

/// A predicate tests a record in-place without allocation.
pub trait Predicate {
    fn test(&self, data: &[u8], reader: &Reader<'_>, obj_offset: usize) -> bool;
}

/// Always-true predicate for `Reader::all()`.
pub struct AlwaysTrue;
impl Predicate for AlwaysTrue {
    fn test(&self, _: &[u8], _: &Reader<'_>, _: usize) -> bool {
        true
    }
}

/// `Eq("key", value)` — equality for bool, &str, i64, f64.
pub struct Eq<'k, V> {
    pub key: &'k str,
    pub value: V,
}

pub fn eq<'k, V>(key: &'k str, value: V) -> crate::query::Eq<'k, V> {
    crate::query::Eq { key, value }
}

/// Helper: resolve the byte offset of a field for Row layout, or read directly for
/// Columnar/PAX layout using the reader's column buffers.
/// Returns `None` if the field is absent or the layout doesn't support direct resolution.
fn row_field_offset(data: &[u8], reader: &Reader<'_>, off: usize, slot: usize) -> Option<usize> {
    // For Row layout `off` is the byte offset to a NYXO object.
    // For Columnar/PAX `off` is the record index — row-oriented slot resolution is invalid.
    match reader.layout {
        Layout::Row => resolve_slot(data, off, slot),
        Layout::Columnar | Layout::Pax => None,
    }
}

impl Predicate for Eq<'_, bool> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                vals.get(off * 8)
                    .map(|&b| (b != 0) == self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_bool(off, slot)
                .map(|v| v == self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff)
                    .map(|&b| (b != 0) == self.value)
                    .unwrap_or(false)
            }
        }
    }
}

impl<'k> Predicate for Eq<'k, &str> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => reader
                .col_field_var_parts(slot)
                .ok()
                .and_then(|(bm, offsets, values)| {
                    if !col_bit(bm, off) {
                        return None;
                    }
                    crate::layout::var_str_at(offsets, values, off)
                })
                .map(|s| s == self.value)
                .unwrap_or(false),
            Layout::Pax => reader
                .pax_get_str(off, slot)
                .map(|s| s == self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                let Some(len_bytes) = data.get(foff..foff + 4) else {
                    return false;
                };
                let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
                data.get(foff + 4..foff + 4 + len)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .map(|s| s == self.value)
                    .unwrap_or(false)
            }
        }
    }
}

impl Predicate for Eq<'_, i64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                let o = off * 8;
                vals.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| i64::from_le_bytes(b) == self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_i64(off, slot)
                .map(|v| v == self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff..foff + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| i64::from_le_bytes(b) == self.value)
                    .unwrap_or(false)
            }
        }
    }
}

impl Predicate for Eq<'_, f64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                let o = off * 8;
                vals.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| f64::from_le_bytes(b) == self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_f64(off, slot)
                .map(|v| v == self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff..foff + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| f64::from_le_bytes(b) == self.value)
                    .unwrap_or(false)
            }
        }
    }
}

/// `Gt("key", value)` — greater-than for f64 or i64.
pub struct Gt<'k, V> {
    pub key: &'k str,
    pub value: V,
}

pub fn gt<'k, V>(key: &'k str, value: V) -> crate::query::Gt<'k, V> {
    crate::query::Gt { key, value }
}

impl Predicate for Gt<'_, f64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                let o = off * 8;
                vals.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| f64::from_le_bytes(b) > self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_f64(off, slot)
                .map(|v| v > self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff..foff + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| f64::from_le_bytes(b) > self.value)
                    .unwrap_or(false)
            }
        }
    }
}

impl Predicate for Gt<'_, i64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                let o = off * 8;
                vals.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| i64::from_le_bytes(b) > self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_i64(off, slot)
                .map(|v| v > self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff..foff + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| i64::from_le_bytes(b) > self.value)
                    .unwrap_or(false)
            }
        }
    }
}

/// `Lt("key", value)` — less-than.
pub struct Lt<'k, V> {
    pub key: &'k str,
    pub value: V,
}

pub fn lt<'k, V>(key: &'k str, value: V) -> crate::query::Lt<'k, V> {
    crate::query::Lt { key, value }
}

impl Predicate for Lt<'_, f64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                let o = off * 8;
                vals.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| f64::from_le_bytes(b) < self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_f64(off, slot)
                .map(|v| v < self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff..foff + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| f64::from_le_bytes(b) < self.value)
                    .unwrap_or(false)
            }
        }
    }
}

impl Predicate for Lt<'_, i64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        match reader.layout {
            Layout::Columnar => {
                if is_var_sigil(*reader.key_sigils.get(slot).unwrap_or(&0)) {
                    return false;
                }
                let Ok((bm, vals)) = reader.col_field_parts(slot) else {
                    return false;
                };
                if !col_bit(bm, off) {
                    return false;
                }
                let o = off * 8;
                vals.get(o..o + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| i64::from_le_bytes(b) < self.value)
                    .unwrap_or(false)
            }
            Layout::Pax => reader
                .pax_get_i64(off, slot)
                .map(|v| v < self.value)
                .unwrap_or(false),
            Layout::Row => {
                let Some(foff) = resolve_slot(data, off, slot) else {
                    return false;
                };
                data.get(foff..foff + 8)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| i64::from_le_bytes(b) < self.value)
                    .unwrap_or(false)
            }
        }
    }
}

/// `And(p1, p2)` — logical AND of two predicates.
pub struct And<A, B>(pub A, pub B);

impl<A: Predicate, B: Predicate> Predicate for And<A, B> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        self.0.test(data, reader, off) && self.1.test(data, reader, off)
    }
}

/// `Or(p1, p2)` — logical OR of two predicates.
pub struct Or<A, B>(pub A, pub B);

impl<A: Predicate, B: Predicate> Predicate for Or<A, B> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        self.0.test(data, reader, off) || self.1.test(data, reader, off)
    }
}

/// `Not(p)` — logical NOT.
pub struct Not<P>(pub P);

impl<P: Predicate> Predicate for Not<P> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        !self.0.test(data, reader, off)
    }
}

// ── Schema parser ─────────────────────────────────────────────────────────────

pub(crate) fn parse_schema(data: &[u8], offset: usize) -> Result<(Vec<String>, Vec<u8>, usize)> {
    if offset + 2 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let key_count = u16::from_le_bytes(
        data[offset..offset + 2]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    let mut pos = offset + 2;

    if pos + key_count > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let sigils = data[pos..pos + key_count].to_vec();
    pos += key_count;

    let mut keys = Vec::with_capacity(key_count);
    for _ in 0..key_count {
        let start = pos;
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        if pos >= data.len() {
            return Err(NxsError::OutOfBounds);
        }
        keys.push(
            std::str::from_utf8(&data[start..pos])
                .map_err(|_| NxsError::ParseError("invalid utf-8 key".into()))?
                .to_owned(),
        );
        pos += 1; // skip null terminator
    }
    // align to 8 bytes
    if pos % 8 != 0 {
        pos += 8 - pos % 8;
    }
    Ok((keys, sigils, pos))
}

// ── resolveSlot ───────────────────────────────────────────────────────────────

/// Stateless LEB128 bitmask walker — returns the absolute byte offset of
/// the value at `slot` within the NYXO object at `obj_offset`, or `None`.
pub(crate) fn resolve_slot(data: &[u8], obj_offset: usize, slot: usize) -> Option<usize> {
    let mut p = obj_offset + 8; // skip NYXO magic (4) + length (4)
    let mut cur: usize = 0;
    let mut table_idx: usize = 0;
    let mut found = false;
    let mut b: u8;
    loop {
        b = *data.get(p)?;
        p += 1;
        let bits = b & 0x7F;
        for bit in 0..7usize {
            if cur == slot {
                if (bits >> bit) & 1 == 0 {
                    return None;
                }
                found = true;
            } else if cur < slot && (bits >> bit) & 1 == 1 {
                table_idx += 1;
            }
            cur += 1;
        }
        if found && b & 0x80 == 0 {
            break;
        }
        if cur > slot && found {
            break;
        }
        if b & 0x80 == 0 {
            return None;
        }
    }
    // skip remaining continuation bytes
    while b & 0x80 != 0 {
        b = *data.get(p)?;
        p += 1;
    }
    let rel = u16::from_le_bytes(
        data.get(p + table_idx * 2..p + table_idx * 2 + 2)?
            .try_into()
            .ok()?,
    ) as usize;
    Some(obj_offset + rel)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::{NxsWriter, Schema};

    fn make_nxb() -> Vec<u8> {
        let schema = Schema::new(&["id", "username", "score", "active"]);
        let mut w = NxsWriter::new(&schema);
        for (id, name, score, active) in [
            (1i64, "alice", 95.0f64, true),
            (2i64, "bob", 42.0f64, false),
            (3i64, "carol", 88.0f64, true),
            (4i64, "dave", 15.0f64, false),
            (5i64, "eve", 77.0f64, true),
        ] {
            w.begin_object();
            w.write_i64(crate::writer::Slot(0), id);
            w.write_str(crate::writer::Slot(1), name);
            w.write_f64(crate::writer::Slot(2), score);
            w.write_bool(crate::writer::Slot(3), active);
            w.end_object();
        }
        w.finish()
    }

    #[test]
    fn reader_opens_and_counts() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.record_count(), 5);
        assert_eq!(r.keys(), &["id", "username", "score", "active"]);
    }

    #[test]
    fn record_access_by_index() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(2).unwrap();
        assert_eq!(rec.get_str("username"), Some("carol"));
        assert_eq!(rec.get_i64("id"), Some(3));
        assert!((rec.get_f64("score").unwrap() - 88.0).abs() < 1e-9);
        assert_eq!(rec.get_bool("active"), Some(true));
    }

    #[test]
    fn all_iterates_every_record() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.all().count(), 5);
    }

    #[test]
    fn where_eq_bool() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let active: Vec<_> = r
            .where_pred(eq("active", true))
            .map(|rec| rec.get_str("username").unwrap().to_owned())
            .collect();
        assert_eq!(active, vec!["alice", "carol", "eve"]);
    }

    #[test]
    fn where_gt_f64() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r.where_pred(gt("score", 80.0f64)).count();
        assert_eq!(count, 2); // alice(95) + carol(88)
    }

    #[test]
    fn where_lt_f64() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r.where_pred(lt("score", 50.0f64)).count();
        assert_eq!(count, 2); // bob(42) + dave(15)
    }

    #[test]
    fn where_and() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r
            .where_pred(And(eq("active", true), gt("score", 80.0f64)))
            .count();
        assert_eq!(count, 2); // alice + carol
    }

    #[test]
    fn where_or() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r
            .where_pred(Or(gt("score", 90.0f64), lt("score", 20.0f64)))
            .count();
        assert_eq!(count, 2); // alice(95) + dave(15)
    }

    #[test]
    fn where_not() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r.where_pred(Not(eq("active", true))).count();
        assert_eq!(count, 2); // bob + dave
    }

    #[test]
    fn early_termination() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let first = r.all().next().unwrap();
        assert_eq!(first.get_str("username"), Some("alice"));
    }

    #[test]
    fn unknown_key_matches_nothing() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.where_pred(eq("nonexistent", true)).count(), 0);
    }

    #[test]
    fn get_str_path_single_segment() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(0).unwrap();
        assert_eq!(rec.get_str_path("username"), Some("alice"));
    }

    #[test]
    fn get_str_path_absent_returns_none() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(0).unwrap();
        assert_eq!(rec.get_str_path("no.such.path"), None);
    }

    fn make_columnar_nxb() -> Vec<u8> {
        use crate::layout::{finish_columnar, Cell, RecordRow};
        let keys = vec!["id".to_string(), "score".to_string(), "active".to_string()];
        let rows: Vec<RecordRow> = vec![
            RecordRow {
                cells: vec![Cell::I64(1), Cell::F64(95.0), Cell::Bool(true)],
            },
            RecordRow {
                cells: vec![Cell::I64(2), Cell::F64(42.0), Cell::Bool(false)],
            },
            RecordRow {
                cells: vec![Cell::I64(3), Cell::F64(88.0), Cell::Bool(true)],
            },
            RecordRow {
                cells: vec![Cell::I64(4), Cell::F64(15.0), Cell::Bool(false)],
            },
            RecordRow {
                cells: vec![Cell::I64(5), Cell::F64(77.0), Cell::Bool(true)],
            },
        ];
        finish_columnar(&keys, &rows).unwrap()
    }

    #[test]
    fn columnar_where_pred_iterates_correctly() {
        let data = make_columnar_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.layout(), Layout::Columnar);
        assert_eq!(r.record_count(), 5);

        // Test all() iterates every record without reading garbage
        assert_eq!(r.all().count(), 5);

        // Test where_pred with eq on bool (Layout::Columnar must not use row tail-index)
        let active_ids: Vec<i64> = r
            .where_pred(eq("active", true))
            .filter_map(|rec| rec.get_i64("id"))
            .collect();
        assert_eq!(active_ids, vec![1, 3, 5]);

        // Test where_pred with gt on f64
        let high_score_ids: Vec<i64> = r
            .where_pred(gt("score", 80.0f64))
            .filter_map(|rec| rec.get_i64("id"))
            .collect();
        assert_eq!(high_score_ids, vec![1, 3]);

        // Test that record values are correct (not garbage from a bad offset)
        let rec = r.record(2).unwrap();
        assert_eq!(rec.get_i64("id"), Some(3));
        assert!((rec.get_f64("score").unwrap() - 88.0).abs() < 1e-9);
        assert_eq!(rec.get_bool("active"), Some(true));
    }

    #[test]
    fn walk_path_deep_segments_returns_none_not_wrong_key() {
        // Before the fix, splitn(8, '.') would concatenate segments 8+ into "seg8.seg9..."
        // and try to look up a key with a literal dot in its name — returning None silently
        // rather than traversing correctly. With plain split('.') every segment is looked
        // up individually, so a path with 9 non-existent segments correctly returns None.
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(0).unwrap();

        // 9-segment path — all segments non-existent, must return None (not garbage)
        let deep = "a.b.c.d.e.f.g.h.i"; // 9 segments
        assert_eq!(rec.get_str_path(deep), None);
        assert_eq!(rec.get_i64_path(deep), None);

        // 10-segment path
        let deeper = "a.b.c.d.e.f.g.h.i.j"; // 10 segments
        assert_eq!(rec.get_str_path(deeper), None);

        // Existing single-level key at depth 1 must still work
        assert_eq!(rec.get_str_path("username"), Some("alice"));
    }

    #[test]
    fn columnar_conformance_vector_col_sum() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../conformance/columnar_flat8_dense_100.nxb");
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.layout(), Layout::Columnar);
        assert_eq!(r.record_count(), 100);
        let sum = r.col_sum_f64("score").unwrap();
        let want: f64 = (0..100).map(|i| i as f64 * 0.5).sum();
        assert!((sum - want).abs() < 1e-9, "sum {sum} want {want}");
        let buf = r.col_buffer("score").unwrap();
        assert_eq!(buf.len(), 100 * 8);
    }
}
