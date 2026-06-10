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

use crate::column_prefetch::ColumnWarmState;
use crate::compact::{
    self, decode_f64_cell, decode_int_cell, parse_delta_tail_layout, parse_extended_schema,
    read_packed_bool, read_str_cell_len, resolve_field_offset, DeltaTailLayout, ExtendedSchema,
    RowCellPlan,
};
use crate::consts::{FLAG_DELTA_TAIL, FLAG_DENSE_FRAMES, FLAG_V13_COMPACT_MASK};
use crate::error::{NxsError, Result};
use crate::layout::{
    col_var_parts, column_sector_len, is_var_sigil, null_bitmap_bytes, var_str_at,
};
use crate::prefetch::PrefetchEngine;

pub use crate::prefetch::{AccessHint, CacheStats, OpenOptions};

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

fn col_bit(bm: &[u8], rec: usize) -> bool {
    (bm[rec / 8] >> (rec % 8)) & 1 == 1
}

// ── Reader ────────────────────────────────────────────────────────────────────

/// Zero-copy reader for a `.nxb` buffer; supports row, columnar, and PAX layouts.
///
/// When opened with [`Self::with_options`], prefetch state is protected by internal
/// mutexes so the reader remains [`Send`] + [`Sync`].
pub struct Reader<'a> {
    data: &'a [u8],
    flags: u16,
    keys: Vec<String>,
    key_sigils: Vec<u8>,
    ext_schema: Option<ExtendedSchema>,
    cell_plan: Option<RowCellPlan>,
    key_index: std::collections::HashMap<String, usize>,
    record_count: usize,
    tail_start: usize,
    pub(crate) delta_tail: Option<DeltaTailLayout>,
    layout: Layout,
    col_buf_off: Vec<u64>,
    col_buf_len: Vec<u64>,
    prefetch: Option<PrefetchEngine>,
    column: ColumnWarmState,
}

impl<'a> Reader<'a> {
    /// Validate the file header, detect layout, and build the schema index.
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
        crate::layout::validate_preamble_flags(flags)?;
        let preamble_tail =
            u64::from_le_bytes(data[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?);

        let (keys, key_sigils, ext_schema, cell_plan) = if flags & FLAG_SCHEMA_EMBEDDED != 0 {
            if flags & FLAG_V13_COMPACT_MASK != 0 {
                let (ext, _) = parse_extended_schema(data, 32, flags)?;
                let plan = RowCellPlan::new(&ext, flags);
                let keys = ext.keys.clone();
                let sigils = ext.sigils.clone();
                (keys, sigils, Some(ext), Some(plan))
            } else {
                let (keys, sigils, _) = parse_schema(data, 32)?;
                (keys, sigils, None, None)
            }
        } else {
            (vec![], vec![], None, None)
        };

        let key_index: std::collections::HashMap<String, usize> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();

        let mut delta_tail = None;
        let (layout, record_count, tail_start, col_buf_off, col_buf_len) = if flags & FLAG_COLUMNAR
            != 0
        {
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
            let mut tail_ptr = usize::try_from(preamble_tail).map_err(|_| NxsError::OutOfBounds)?;
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
            if tail_ptr > data.len().saturating_sub(4) {
                return Err(NxsError::OutOfBounds);
            }
            if flags & FLAG_DELTA_TAIL != 0 {
                let layout = parse_delta_tail_layout(data, tail_ptr)?;
                let record_count = layout.record_count;
                delta_tail = Some(layout);
                (Layout::Row, record_count, tail_ptr, vec![], vec![])
            } else {
                let record_count =
                    u32::from_le_bytes(data[tail_ptr..tail_ptr + 4].try_into().unwrap()) as usize;
                (Layout::Row, record_count, tail_ptr + 4, vec![], vec![])
            }
        };

        Ok(Self {
            data,
            flags,
            keys,
            key_sigils,
            ext_schema,
            cell_plan,
            key_index,
            record_count,
            tail_start,
            delta_tail,
            layout,
            col_buf_off,
            col_buf_len,
            prefetch: None,
            column: ColumnWarmState::default(),
        })
    }

    /// Open with prefetch options (row-layout viewport cache; phase 1+2).
    pub fn with_options(data: &'a [u8], options: OpenOptions) -> Result<Self> {
        options.validate()?;
        let mut reader = Self::new(data)?;
        if reader.layout == Layout::Row {
            let prefetch = PrefetchEngine::new(options, data.len());
            if prefetch.strategy() == crate::prefetch::PrefetchStrategy::Eager {
                prefetch.start_eager_background(data.to_vec(), reader.tail_start);
            }
            reader.prefetch = Some(prefetch);
        }
        Ok(reader)
    }

    /// Wait for in-progress eager / background prefetch (§8).
    pub fn warmup(&self) {
        if let Some(prefetch) = &self.prefetch {
            prefetch.warmup();
        }
    }

    /// Stop scheduling speculative and eager prefetch (§8.1).
    pub fn pause_prefetch(&self) {
        if let Some(prefetch) = &self.prefetch {
            prefetch.pause_prefetch();
        }
    }

    /// Resume speculative prefetch after [`Self::pause_prefetch`].
    pub fn resume_prefetch(&self) {
        if let Some(prefetch) = &self.prefetch {
            prefetch.resume_prefetch();
        }
    }

    /// Prefetch a single column buffer (columnar layout only; §7.4).
    pub fn prefetch_column(&self, key: &str) -> Result<()> {
        if self.layout != Layout::Columnar {
            return Err(NxsError::UnsupportedLayout);
        }
        let slot = *self
            .key_index
            .get(key)
            .ok_or_else(|| NxsError::ParseError(format!("key not found: {key}")))?;
        let off = *self.col_buf_off.get(slot).ok_or(NxsError::OutOfBounds)? as usize;
        let len = *self.col_buf_len.get(slot).ok_or(NxsError::OutOfBounds)? as usize;
        let end = off.checked_add(len).ok_or(NxsError::OutOfBounds)?;
        if end > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        if self.column.prefetch(slot) {
            const PAGE: usize = 4096;
            let sector = &self.data[off..end];
            for page_start in (0..sector.len()).step_by(PAGE) {
                std::hint::black_box(sector[page_start]);
            }
        }
        Ok(())
    }

    /// Prefetch pages covering records `[start_index, end_index]` (row layout only).
    pub fn prefetch_viewport(&self, start_index: usize, end_index: usize) -> Result<()> {
        if self.layout != Layout::Row {
            return Ok(());
        }
        if let Some(prefetch) = &self.prefetch {
            prefetch.prefetch_viewport(
                self.data,
                self.tail_start,
                self.record_count,
                start_index,
                end_index,
            );
        }
        Ok(())
    }

    /// Page-cache statistics. Row prefetch counters are zero when opened via
    /// [`Self::new`]; columnar readers may still report [`CacheStats::column_fetches_issued`].
    pub fn cache_stats(&self) -> CacheStats {
        let mut stats = if let Some(prefetch) = &self.prefetch {
            prefetch.cache_stats()
        } else {
            CacheStats {
                pages_cached: 0,
                pages_max: 0,
                memory_used_bytes: 0,
                cache_hits: 0,
                cache_misses: 0,
                fetches_issued: 0,
                column_fetches_issued: 0,
                strategy: "disabled".to_string(),
                pattern: "unknown".to_string(),
            }
        };
        if self.layout == Layout::Columnar {
            stats.column_fetches_issued = self.column.fetches();
        }
        stats
    }

    fn touch_record_page(&self, index: usize) {
        if self.layout != Layout::Row {
            return;
        }
        let Some(prefetch) = &self.prefetch else {
            return;
        };
        prefetch.on_access(self.data, self.tail_start, self.record_count, index);
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
        let e = page_idx
            .checked_mul(PAX_TAIL_ENTRY_BYTES)
            .and_then(|n| self.tail_start.checked_add(n))
            .ok_or(NxsError::OutOfBounds)?;
        let page_off_start = e.checked_add(16).ok_or(NxsError::OutOfBounds)?;
        let page_off_end = e.checked_add(24).ok_or(NxsError::OutOfBounds)?;
        let poff = u64::from_le_bytes(
            self.data
                .get(page_off_start..page_off_end)
                .ok_or(NxsError::OutOfBounds)?
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        if poff > self.data.len().saturating_sub(24) {
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
        let mut body = poff.checked_add(24).ok_or(NxsError::OutOfBounds)?;
        for fi in 0..slot {
            if body > self.data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let sig = self.key_sigils.get(fi).copied().unwrap_or(b'=');
            let slen = column_sector_len(&self.data[body..], rc, sig)?;
            body = body.checked_add(slen).ok_or(NxsError::OutOfBounds)?;
        }
        if body > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let sig = self.key_sigils.get(slot).copied().unwrap_or(b'=');
        let slen = column_sector_len(&self.data[body..], rc, sig)?;
        if body > self.data.len().saturating_sub(slen) {
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
        if tp > self.data.len().saturating_sub(4) {
            return 0;
        }
        // page count stored in footer; re-read from footer
        let fo = self.data.len() - footer_size(FLAG_PAX);
        u32::from_le_bytes(self.data[fo + 16..fo + 20].try_into().unwrap_or([0; 4])) as usize
    }

    fn pax_page_rec_start(&self, page_idx: usize) -> Option<u64> {
        let e = page_idx
            .checked_mul(PAX_TAIL_ENTRY_BYTES)
            .and_then(|n| self.tail_start.checked_add(n))?;
        let start = e.checked_add(4)?;
        let end = e.checked_add(12)?;
        Some(u64::from_le_bytes(
            self.data.get(start..end)?.try_into().ok()?,
        ))
    }

    fn pax_page_rec_count(&self, page_idx: usize) -> Option<u32> {
        let e = page_idx
            .checked_mul(PAX_TAIL_ENTRY_BYTES)
            .and_then(|n| self.tail_start.checked_add(n))?;
        let start = e.checked_add(12)?;
        let end = e.checked_add(16)?;
        Some(u32::from_le_bytes(
            self.data.get(start..end)?.try_into().ok()?,
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
        let end = off.checked_add(len).ok_or(NxsError::OutOfBounds)?;
        if end > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        col_var_parts(&self.data[off..end], self.record_count)
    }

    /// Null bitmap + dense value buffer for a fixed-width columnar field.
    pub fn col_field_parts(&self, slot: usize) -> Result<(&[u8], &[u8])> {
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
        let end = off.checked_add(len).ok_or(NxsError::OutOfBounds)?;
        if end > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let bm_len = null_bitmap_bytes(self.record_count);
        let vals_len = self.record_count.saturating_mul(8);
        let vals_end = bm_len.checked_add(vals_len).ok_or(NxsError::OutOfBounds)?;
        if len < vals_end {
            return Err(NxsError::OutOfBounds);
        }
        let sector = &self.data[off..end];
        Ok((&sector[..bm_len], &sector[bm_len..vals_end]))
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
        self.touch_record_page(i);
        let offset = if self.layout == Layout::Row {
            self.row_record_offset(i)?
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

    fn row_record_offset(&self, index: usize) -> Option<usize> {
        if let Some(ref dt) = self.delta_tail {
            compact::delta_record_offset(self.data, dt, index).ok()
        } else {
            let entry = self.tail_start + index * 10;
            Some(
                u64::from_le_bytes(self.data.get(entry + 2..entry + 10)?.try_into().ok()?) as usize,
            )
        }
    }

    /// Materialise a keyword or promoted-string value by field name.
    pub fn get_keyword(&self, key: &str, record_index: usize) -> Option<String> {
        let slot = self.slot(key)?;
        let ext = self.ext_schema.as_ref()?;
        let sig = *self.key_sigils.get(slot)?;
        if sig != crate::consts::SIGIL_KEYWORD && !ext.is_promoted(slot) {
            return None;
        }
        let rec = self.record(record_index)?;
        let off = rec.resolve(slot)?;
        let idx = u16::from_le_bytes(self.data.get(off..off + 2)?.try_into().ok()?);
        compact::materialise_keyword(ext, slot, idx)
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
    /// Byte offset of this record's NYXO object (row layout only).
    pub fn object_offset(&self) -> Option<usize> {
        match self.reader.layout {
            Layout::Row => Some(self.offset),
            _ => None,
        }
    }

    /// Resolve the byte offset of slot `s` within this object. Returns `None` if absent.
    fn resolve(&self, slot: usize) -> Option<usize> {
        if let (Some(ext), Some(plan)) = (&self.reader.ext_schema, &self.reader.cell_plan) {
            return resolve_field_offset(
                self.data,
                self.offset,
                slot,
                ext,
                plan,
                self.reader.flags & FLAG_DENSE_FRAMES != 0,
            );
        }
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
                if let Some(ext) = &self.reader.ext_schema {
                    let w = ext.cell_width(slot);
                    return decode_int_cell(self.data, off, w).ok();
                }
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
        if let Some(ext) = &self.reader.ext_schema {
            let w = ext.cell_width(slot);
            return decode_f64_cell(self.data, off, w).ok();
        }
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
                if let (Some(ext), Some(plan)) = (&self.reader.ext_schema, &self.reader.cell_plan) {
                    if plan.packed_bools && plan.bool_slots.contains(&slot) {
                        return read_packed_bool(self.data, self.offset, slot, ext, plan);
                    }
                }
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
                if let Some(ext) = &self.reader.ext_schema {
                    if ext.is_promoted(slot) {
                        let idx = u16::from_le_bytes(self.data.get(off..off + 2)?.try_into().ok()?);
                        return ext.value_pool.get(idx as usize).map(|s| s.as_str());
                    }
                    let prefix = ext.str_len_prefix(slot);
                    let len = read_str_cell_len(self.data, off, prefix).ok()?;
                    let bytes = self.data.get(off + prefix..off + prefix + len)?;
                    return std::str::from_utf8(bytes).ok();
                }
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
                Layout::Row => r.row_record_offset(i)?,
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
#[allow(dead_code)]
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
    let mut p = obj_offset.checked_add(8)?; // skip NYXO magic (4) + length (4)
    let mut cur: usize = 0;
    let mut table_idx: usize = 0;
    let mut found = false;
    let mut b: u8;
    loop {
        b = *data.get(p)?;
        p = p.checked_add(1)?;
        let bits = b & 0x7F;
        for bit in 0..7usize {
            if cur == slot {
                if (bits >> bit) & 1 == 0 {
                    return None;
                }
                found = true;
            } else if cur < slot && (bits >> bit) & 1 == 1 {
                table_idx = table_idx.checked_add(1)?;
            }
            cur = cur.checked_add(1)?;
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
        p = p.checked_add(1)?;
    }
    let table_off = table_idx.checked_mul(2)?;
    let table_start = p.checked_add(table_off)?;
    let table_end = table_start.checked_add(2)?;
    let rel = u16::from_le_bytes(data.get(table_start..table_end)?.try_into().ok()?) as usize;
    obj_offset.checked_add(rel)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::{NxsWriter, Schema};

    fn make_nxb() -> Vec<u8> {
        let schema = Schema::new(&["id", "username", "score", "active"]);
        let mut w = NxsWriter::with_capacity(&schema, 4096);
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

    #[test]
    fn columnar_strings_roundtrip() {
        use crate::layout::{finish_columnar, null_bitmap_bytes, var_str_at, Cell, RecordRow};

        let keys = vec!["id".into(), "name".into(), "score".into()];
        let mut rows = Vec::new();
        for i in 0..100usize {
            rows.push(RecordRow {
                cells: vec![
                    Cell::I64(i as i64),
                    Cell::Str(format!("user_{i}")),
                    Cell::F64(i as f64 * 1.25),
                ],
            });
        }
        let bytes = finish_columnar(&keys, &rows).unwrap();
        let r = Reader::new(&bytes).unwrap();
        assert_eq!(r.record_count(), 100);
        for i in 0..100 {
            let rec = r.record(i).unwrap();
            assert_eq!(rec.get_i64("id"), Some(i as i64));
            let want = format!("user_{i}");
            assert_eq!(rec.get_str("name"), Some(want.as_str()));
            assert!((rec.get_f64("score").unwrap() - i as f64 * 1.25).abs() < 1e-9);
        }
        let (bm, offsets, values) = r.col_field_var_parts(1).unwrap();
        assert_eq!(bm.len(), null_bitmap_bytes(100));
        assert_eq!(offsets.len(), 101 * 4);
        assert!(!values.is_empty());
        assert_eq!(var_str_at(offsets, values, 42), Some("user_42"));
    }

    #[test]
    fn pax_strings_roundtrip_across_pages() {
        use crate::layout::{finish_pax, Cell, RecordRow};

        let keys = vec!["id".into(), "name".into(), "score".into()];
        let rows: Vec<RecordRow> = (0..300usize)
            .map(|i| RecordRow {
                cells: vec![
                    Cell::I64(i as i64),
                    Cell::Str(format!("user_{i}")),
                    Cell::F64(i as f64),
                ],
            })
            .collect();
        let bytes = finish_pax(&keys, &rows, 128).unwrap();
        let r = Reader::new(&bytes).unwrap();
        assert_eq!(r.record_count(), 300);
        for i in [0usize, 127, 128, 257, 299] {
            let rec = r.record(i).unwrap();
            let want = format!("user_{i}");
            assert_eq!(rec.get_str("name"), Some(want.as_str()));
            assert_eq!(rec.get_i64("id"), Some(i as i64));
        }
    }

    #[test]
    fn compact_v13_single_record() {
        use crate::compact::CompactOptions;
        use crate::writer::{NxsWriter, Schema, Slot};

        let schema = Schema::new(&["id", "score"]);
        let mut w = NxsWriter::with_compact(&schema, Some(CompactOptions::compact()));
        w.begin_object();
        w.write_i64(Slot(0), 42);
        w.write_f64(Slot(1), 3.5);
        w.end_object();
        let bytes = w.finish();
        let r = Reader::new(&bytes).unwrap();
        let rec = r.record(0).unwrap();
        assert_eq!(rec.get_i64("id"), Some(42));
        assert!((rec.get_f64("score").unwrap() - 3.5).abs() < 1e-9);
    }

    #[test]
    fn get_keyword_promoted_string() {
        use crate::compact::CompactOptions;
        use crate::writer::{NxsWriter, Schema, Slot};

        let schema = Schema::new(&["level", "msg"]);
        let mut w = NxsWriter::with_compact(&schema, Some(CompactOptions::compact()));
        for i in 0..40usize {
            let level = match i % 3 {
                0 => "INFO",
                1 => "WARN",
                _ => "ERROR",
            };
            w.begin_object();
            w.write_str(Slot(0), level);
            w.write_str(Slot(1), "event");
            w.end_object();
        }
        let bytes = w.finish();
        let reader = Reader::new(&bytes).unwrap();
        assert!(reader.ext_schema.as_ref().unwrap().is_promoted(0));
        assert_eq!(reader.get_keyword("level", 1), Some("WARN".to_string()));
        assert_eq!(reader.record(1).unwrap().get_str("level"), Some("WARN"));
    }

    #[test]
    fn compact_v13_roundtrip() {
        use crate::compact::CompactOptions;
        use crate::writer::{NxsWriter, Schema, Slot};

        const SLOTS: &[&str] = &["id", "username", "age", "active", "score"];
        let schema = Schema::new(SLOTS);
        let opts = CompactOptions::compact();
        let mut w = NxsWriter::with_compact(&schema, Some(opts));
        for i in 0..50usize {
            w.begin_object();
            w.write_i64(Slot(0), i as i64);
            w.write_str(Slot(1), &format!("user_{i:04}"));
            w.write_i64(Slot(2), (20 + (i % 50)) as i64);
            w.write_bool(Slot(3), i % 2 == 0);
            w.write_f64(Slot(4), i as f64 * 0.5);
            w.end_object();
        }
        let bytes = w.finish();
        let r = Reader::new(&bytes).unwrap();
        assert_eq!(r.record_count(), 50);
        let rec = r.record(7).expect("record 7");
        assert_eq!(rec.get_i64("id"), Some(7));
        assert_eq!(rec.get_str("username"), Some("user_0007"));
        assert_eq!(rec.get_i64("age"), Some(27));
        assert_eq!(rec.get_bool("active"), Some(false));
        assert!((rec.get_f64("score").unwrap() - 3.5).abs() < 1e-9);
        assert!(bytes.len() < 50 * 60);
    }

    fn records_1000_fixture_rows() -> (Vec<String>, Vec<crate::layout::RecordRow>) {
        use crate::layout::{Cell, RecordRow};

        const KEYS: &[&str] = &[
            "id",
            "username",
            "email",
            "age",
            "balance",
            "active",
            "score",
            "created_at",
        ];
        let rows: Vec<RecordRow> = (0..1000)
            .map(|i| RecordRow {
                cells: vec![
                    Cell::I64(i as i64),
                    Cell::Str(format!("user_{i:07}")),
                    Cell::Str(format!("user{i}@example.com")),
                    Cell::I64(20 + (i % 50) as i64),
                    Cell::F64(100.0 + i as f64 * 1.37),
                    Cell::Bool(i % 3 != 0),
                    Cell::F64((i as f64 % 100.0) / 10.0),
                    Cell::Time(1_700_000_000_000_000_000 + i as i64),
                ],
            })
            .collect();
        let keys: Vec<String> = KEYS.iter().map(|s| (*s).to_string()).collect();
        (keys, rows)
    }

    /// Gate: every field on every record must decode identically to the v1.2 row file.
    #[test]
    fn compact_records_1000_matches_v12_decode() {
        use crate::compact::CompactOptions;
        use crate::layout::finish_row;

        let (keys, rows) = records_1000_fixture_rows();
        let v12 = finish_row(&keys, &rows, None).unwrap();
        let compact = finish_row(&keys, &rows, Some(&CompactOptions::compact())).unwrap();
        let r12 = Reader::new(&v12).unwrap();
        let r13 = Reader::new(&compact).unwrap();
        assert_eq!(r12.record_count(), 1000);
        assert_eq!(r13.record_count(), 1000);
        let ext = r13.ext_schema.as_ref().unwrap();
        assert!(!ext.is_promoted(1));
        assert!(!ext.is_promoted(2));

        for i in 0..1000 {
            let a = r12.record(i).unwrap();
            let b = r13.record(i).unwrap();
            assert_eq!(a.get_i64("id"), b.get_i64("id"), "id @ {i}");
            assert_eq!(
                a.get_str("username"),
                b.get_str("username"),
                "username @ {i}"
            );
            assert_eq!(a.get_str("email"), b.get_str("email"), "email @ {i}");
            assert_eq!(a.get_i64("age"), b.get_i64("age"), "age @ {i}");
            assert_eq!(a.get_f64("score"), b.get_f64("score"), "score @ {i}");
            assert_eq!(a.get_bool("active"), b.get_bool("active"), "active @ {i}");
            assert_eq!(
                a.get_f64("balance").map(|v| (v * 1e9).round() as i64),
                b.get_f64("balance").map(|v| (v * 1e9).round() as i64),
                "balance @ {i}"
            );
            assert_eq!(
                a.get_i64("created_at"),
                b.get_i64("created_at"),
                "created_at @ {i}"
            );
        }
        assert!(
            compact.len() < v12.len(),
            "compact {} vs v12 {}",
            compact.len(),
            v12.len()
        );
        // u16 length prefixes only shrink cells when padding boundary shifts; this
        // fixture's 12/17-char strings stay on the same 8-byte pad grid.
        assert!(
            (86_000..=94_000).contains(&compact.len()),
            "compact size: {} bytes (v12 {})",
            compact.len(),
            v12.len()
        );
    }
}
