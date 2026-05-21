//! PAX streaming writer and incremental page reader (OLAP.md §4.5).
//!
//! Unsealed files use `TailPtr = 0` in the preamble; complete `NXSP` pages may be
//! consumed before seal. On seal, the PAX tail-index and 28-byte footer are written.

use crate::error::{NxsError, Result};
use crate::layout::{
    encode_page, sigils_for_keys, FLAG_PAX, FLAG_SCHEMA_EMBEDDED, MAGIC_FILE, MAGIC_FOOTER,
    MAGIC_PAGE, RecordRow, VERSION,
};
use crate::query::parse_schema;
use crate::writer::{build_schema, murmur3_64};
use std::collections::HashMap;
use std::io::Write;

const FOOTER_PAX: usize = 28;
const PAGE_HEADER: usize = 24;

/// Metadata for one flushed page (tail-index entry).
#[derive(Debug, Clone, Copy)]
pub struct PaxPageMeta {
    pub page_index: u32,
    pub record_start: u64,
    pub record_count: u32,
    pub page_offset: u64,
    pub page_length: u32,
}

/// End offset (exclusive) of a complete PAX page at `off`, if fully present in `data`.
pub fn complete_page_end(data: &[u8], off: usize) -> Option<usize> {
    if off + PAGE_HEADER > data.len() {
        return None;
    }
    if u32::from_le_bytes(data.get(off..off + 4)?.try_into().ok()?) != MAGIC_PAGE {
        return None;
    }
    let record_count = u32::from_le_bytes(data.get(off + 16..off + 20)?.try_into().ok()?) as usize;
    let field_count = u16::from_le_bytes(data.get(off + 20..off + 22)?.try_into().ok()?) as usize;
    let mut body_end = off + PAGE_HEADER;
    for _ in 0..field_count {
        let bm = null_bitmap_bytes(record_count);
        body_end = body_end.checked_add(bm + record_count * 8)?;
    }
    if body_end + 4 > data.len() {
        return None;
    }
    let page_len = u32::from_le_bytes(data.get(body_end..body_end + 4)?.try_into().ok()?) as usize;
    let logical_end = off.checked_add(page_len)?;
    if logical_end != body_end + 4 {
        return None;
    }
    let aligned_end = (logical_end + 7) & !7;
    if aligned_end <= data.len() {
        Some(aligned_end)
    } else {
        None
    }
}

fn null_bitmap_bytes(n: usize) -> usize {
    let raw = (n + 7) / 8;
    ((raw + 7) / 8) * 8
}

fn col_bit(bm: &[u8], rec: usize) -> bool {
    (bm[rec / 8] >> (rec % 8)) & 1 == 1
}

/// Streaming PAX writer: accumulates records, flushes full pages, seals with tail-index.
pub struct PaxStreamWriter {
    keys: Vec<String>,
    sigils: Vec<u8>,
    schema_bytes: Vec<u8>,
    dict_hash: u64,
    page_size: u32,
    field_count: usize,
    pending: Vec<RecordRow>,
    data: Vec<u8>,
    pages: Vec<PaxPageMeta>,
    next_page_index: u32,
    next_record_start: u64,
}

impl PaxStreamWriter {
    pub fn new(keys: Vec<String>, sigils: Vec<u8>, page_size: u32) -> Result<Self> {
        if page_size == 0 {
            return Err(NxsError::ParseError("page_size must be > 0".into()));
        }
        if keys.is_empty() {
            return Err(NxsError::ParseError("PAX stream writer requires schema keys".into()));
        }
        let schema_bytes = build_schema(&keys, &sigils);
        let dict_hash = murmur3_64(&schema_bytes);
        Ok(Self {
            field_count: keys.len(),
            keys,
            sigils,
            schema_bytes,
            dict_hash,
            page_size,
            pending: Vec::new(),
            data: Vec::new(),
            pages: Vec::new(),
            next_page_index: 0,
            next_record_start: 0,
        })
    }

    /// Build a writer from keys and rows (infers sigils like sealed `finish_pax`).
    pub fn from_rows(keys: &[String], rows: &[RecordRow], page_size: u32) -> Result<Self> {
        let sigils = sigils_for_keys(keys, rows);
        let mut w = Self::new(keys.to_vec(), sigils, page_size)?;
        for row in rows {
            w.push_record(row.clone())?;
        }
        Ok(w)
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn flushed_page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn data_sector_len(&self) -> usize {
        self.data.len()
    }

    pub fn pages(&self) -> &[PaxPageMeta] {
        &self.pages
    }

    /// Append a record; flushes a complete page when `pending.len() == page_size`.
    pub fn push_record(&mut self, row: RecordRow) -> Result<Option<Vec<u8>>> {
        self.pending.push(row);
        if self.pending.len() as u32 >= self.page_size {
            Ok(Some(self.flush_page()?))
        } else {
            Ok(None)
        }
    }

    /// Emit the current pending slice as one page (partial page allowed).
    pub fn flush_pending(&mut self) -> Result<Option<Vec<u8>>> {
        if self.pending.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.flush_page()?))
    }

    fn flush_page(&mut self) -> Result<Vec<u8>> {
        let count = self.pending.len() as u32;
        let page_off = 32 + self.schema_bytes.len() as u64 + self.data.len() as u64;
        let page_bytes = encode_page(
            self.next_page_index,
            self.next_record_start,
            count,
            self.field_count,
            &self.sigils,
            &self.pending,
        )?;
        let page_len = page_bytes.len() as u32;
        self.pages.push(PaxPageMeta {
            page_index: self.next_page_index,
            record_start: self.next_record_start,
            record_count: count,
            page_offset: page_off,
            page_length: page_len,
        });
        self.data.extend_from_slice(&page_bytes);
        self.next_page_index += 1;
        self.next_record_start += count as u64;
        self.pending.clear();
        Ok(page_bytes)
    }

    fn total_records(&self) -> u64 {
        self.next_record_start + self.pending.len() as u64
    }

    /// Write unsealed preamble + schema (`TailPtr = 0`). Returns absolute data-sector start.
    pub fn write_stream_header(&self, out: &mut impl Write) -> Result<u64> {
        let flags = FLAG_SCHEMA_EMBEDDED | FLAG_PAX;
        out.write_all(&MAGIC_FILE.to_le_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        out.write_all(&VERSION.to_le_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        out.write_all(&flags.to_le_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        out.write_all(&self.dict_hash.to_le_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        out.write_all(&0u64.to_le_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        out.write_all(&0u64.to_le_bytes())
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        out.write_all(&self.schema_bytes)
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        Ok(32 + self.schema_bytes.len() as u64)
    }

    /// Append flushed data-sector bytes since `start`.
    pub fn write_data_sector_since(
        &self,
        out: &mut impl Write,
        start: usize,
    ) -> std::result::Result<(), std::io::Error> {
        if self.data.len() > start {
            out.write_all(&self.data[start..])?;
        }
        Ok(())
    }

    /// Build a sealed in-memory `.nxb` (final partial page + tail-index + footer).
    pub fn finish_stream(mut self) -> Result<Vec<u8>> {
        if !self.pending.is_empty() {
            self.flush_page()?;
        }
        let tail_index_offset = 32 + self.schema_bytes.len() as u64 + self.data.len() as u64;
        let tail = build_pax_tail(
            &self.pages,
            tail_index_offset,
            self.total_records(),
            self.page_size,
        );
        let flags = FLAG_SCHEMA_EMBEDDED | FLAG_PAX;
        let mut out = Vec::with_capacity(32 + self.schema_bytes.len() + self.data.len() + tail.len());
        out.extend_from_slice(&MAGIC_FILE.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&self.dict_hash.to_le_bytes());
        out.extend_from_slice(&tail_index_offset.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&self.schema_bytes);
        out.extend_from_slice(&self.data);
        out.extend_from_slice(&tail);
        Ok(out)
    }

    /// Seal an on-disk stream: flush pending, append tail-index, patch preamble tail offset.
    pub fn seal_stream(mut self, out: &mut impl Write) -> Result<u64> {
        if !self.pending.is_empty() {
            self.flush_page()?;
        }
        let tail_index_offset = 32 + self.schema_bytes.len() as u64 + self.data.len() as u64;
        let tail = build_pax_tail(
            &self.pages,
            tail_index_offset,
            self.total_records(),
            self.page_size,
        );
        out.write_all(&tail)
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        Ok(tail_index_offset)
    }
}

fn build_pax_tail(
    pages: &[PaxPageMeta],
    tail_index_offset: u64,
    record_count: u64,
    page_size: u32,
) -> Vec<u8> {
    let mut tail = Vec::new();
    for p in pages {
        tail.extend_from_slice(&p.page_index.to_le_bytes());
        tail.extend_from_slice(&p.record_start.to_le_bytes());
        tail.extend_from_slice(&p.record_count.to_le_bytes());
        tail.extend_from_slice(&p.page_offset.to_le_bytes());
        tail.extend_from_slice(&p.page_length.to_le_bytes());
    }
    tail.extend_from_slice(&tail_index_offset.to_le_bytes());
    tail.extend_from_slice(&record_count.to_le_bytes());
    tail.extend_from_slice(&(pages.len() as u32).to_le_bytes());
    tail.extend_from_slice(&page_size.to_le_bytes());
    tail.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());
    tail
}

/// View of one complete PAX page in a growing buffer.
#[derive(Debug, Clone, Copy)]
pub struct PaxPageView<'a> {
    pub offset: usize,
    pub page_index: u32,
    pub record_start: u64,
    pub record_count: u32,
    pub field_count: u16,
    data: &'a [u8],
}

impl<'a> PaxPageView<'a> {
    fn field_parts(&self, slot: usize) -> Result<(&'a [u8], &'a [u8])> {
        let rc = self.record_count as usize;
        if slot >= self.field_count as usize {
            return Err(NxsError::OutOfBounds);
        }
        let mut body = self.offset + PAGE_HEADER;
        for _ in 0..slot {
            let bm_len = null_bitmap_bytes(rc);
            body += bm_len + rc * 8;
        }
        let bm_len = null_bitmap_bytes(rc);
        if body + bm_len + rc * 8 > self.data.len() {
            return Err(NxsError::OutOfBounds);
        }
        Ok((
            &self.data[body..body + bm_len],
            &self.data[body + bm_len..body + bm_len + rc * 8],
        ))
    }

    pub fn get_f64(&self, local_record: usize, key: &str, key_index: &HashMap<String, usize>) -> Option<f64> {
        let slot = *key_index.get(key)?;
        let (bm, vals) = self.field_parts(slot).ok()?;
        if !col_bit(bm, local_record) {
            return None;
        }
        let off = local_record * 8;
        Some(f64::from_le_bytes(vals.get(off..off + 8)?.try_into().ok()?))
    }

    pub fn get_i64(&self, local_record: usize, key: &str, key_index: &HashMap<String, usize>) -> Option<i64> {
        let slot = *key_index.get(key)?;
        let (bm, vals) = self.field_parts(slot).ok()?;
        if !col_bit(bm, local_record) {
            return None;
        }
        let off = local_record * 8;
        Some(i64::from_le_bytes(vals.get(off..off + 8)?.try_into().ok()?))
    }
}

/// Incremental reader for unsealed or sealed PAX streams.
pub struct PaxStreamReader<'a> {
    data: &'a [u8],
    keys: Vec<String>,
    key_sigils: Vec<u8>,
    key_index: HashMap<String, usize>,
    data_start: usize,
    page_cursor: usize,
    sealed: bool,
}

impl<'a> PaxStreamReader<'a> {
    pub fn open(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(NxsError::OutOfBounds);
        }
        if u32::from_le_bytes(data[0..4].try_into().map_err(|_| NxsError::OutOfBounds)?)
            != MAGIC_FILE
        {
            return Err(NxsError::BadMagic);
        }
        let flags = u16::from_le_bytes(data[6..8].try_into().map_err(|_| NxsError::OutOfBounds)?);
        if flags & FLAG_PAX == 0 {
            return Err(NxsError::UnsupportedLayout);
        }
        let preamble_tail =
            u64::from_le_bytes(data[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?);
        if flags & FLAG_SCHEMA_EMBEDDED == 0 {
            return Err(NxsError::ParseError(
                "PAX stream requires FLAG_SCHEMA_EMBEDDED".into(),
            ));
        }

        let (keys, key_sigils, data_start) = parse_schema(data, 32)?;

        let key_index: HashMap<String, usize> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();

        let sealed = if data.len() < FOOTER_PAX + 4 {
            false
        } else {
            let tail_magic = u32::from_le_bytes(
                data[data.len() - 4..]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            if tail_magic != MAGIC_FOOTER {
                false
            } else if preamble_tail != 0 {
                true
            } else {
                let footer_tail_ptr = u64::from_le_bytes(
                    data[data.len() - FOOTER_PAX..data.len() - FOOTER_PAX + 8]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                ) as usize;
                footer_tail_ptr > 0
                    && footer_tail_ptr < data.len()
                    && data.len() - footer_tail_ptr >= FOOTER_PAX
            }
        };

        if !sealed && preamble_tail != 0 {
            return Err(NxsError::ParseError(
                "PAX stream: preamble tail offset set before seal".into(),
            ));
        }

        Ok(Self {
            data,
            keys,
            key_sigils,
            key_index,
            data_start,
            page_cursor: data_start,
            sealed,
        })
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn data_start(&self) -> usize {
        self.data_start
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn key_sigils(&self) -> &[u8] {
        &self.key_sigils
    }

    /// Number of records in all **complete** pages (excludes any in-progress page).
    pub fn complete_record_count(&self) -> usize {
        let mut n = 0usize;
        let mut off = self.data_start;
        while let Some(end) = complete_page_end(self.data, off) {
            let rc = u32::from_le_bytes(
                self.data[off + 16..off + 20]
                    .try_into()
                    .unwrap_or([0; 4]),
            ) as usize;
            n += rc;
            off = end;
        }
        n
    }

    /// True when at least one full page is available.
    pub fn has_complete_page(&self) -> bool {
        complete_page_end(self.data, self.data_start).is_some()
    }

    /// Next complete page view, advancing the internal cursor.
    pub fn poll_next_page(&mut self) -> Option<PaxPageView<'a>> {
        let end = complete_page_end(self.data, self.page_cursor)?;
        let off = self.page_cursor;
        self.page_cursor = end;
        Some(parse_page_view(self.data, off))
    }

    /// Reset page iteration to the data-sector start.
    pub fn rewind_pages(&mut self) {
        self.page_cursor = self.data_start;
    }

    /// Sum `key` as f64 over all complete pages (incremental column scan).
    pub fn col_sum_f64_complete_pages(&self, key: &str) -> Option<f64> {
        let slot = *self.key_index.get(key)?;
        let mut sum = 0.0;
        let mut any = false;
        let mut off = self.data_start;
        while let Some(end) = complete_page_end(self.data, off) {
            let view = parse_page_view(self.data, off);
            let (bm, vals) = view.field_parts(slot).ok()?;
            let rc = view.record_count as usize;
            for i in 0..rc {
                if col_bit(bm, i) {
                    let o = i * 8;
                    sum += f64::from_le_bytes(vals[o..o + 8].try_into().ok()?);
                    any = true;
                }
            }
            off = end;
        }
        any.then_some(sum)
    }

    /// Global record index → f64 within complete pages only.
    pub fn get_f64_complete(&self, global_index: usize, key: &str) -> Option<f64> {
        let mut off = self.data_start;
        while let Some(end) = complete_page_end(self.data, off) {
            let view = parse_page_view(self.data, off);
            let start = view.record_start as usize;
            let count = view.record_count as usize;
            if global_index >= start && global_index < start + count {
                return view.get_f64(global_index - start, key, &self.key_index);
            }
            off = end;
        }
        None
    }
}

fn parse_page_view(data: &[u8], off: usize) -> PaxPageView<'_> {
    PaxPageView {
        offset: off,
        page_index: u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap_or([0; 4])),
        record_start: u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap_or([0; 8])),
        record_count: u32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap_or([0; 4])),
        field_count: u16::from_le_bytes(data[off + 20..off + 22].try_into().unwrap_or([0; 2])),
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Cell, RecordRow};
    use crate::query::Reader;

    fn score_rows(n: usize) -> (Vec<String>, Vec<RecordRow>) {
        let keys = vec!["score".into()];
        let rows: Vec<RecordRow> = (0..n)
            .map(|i| RecordRow {
                cells: vec![Cell::F64(i as f64)],
            })
            .collect();
        (keys, rows)
    }

    #[test]
    fn pax_stream_300_records_two_pages_incremental_then_seal() {
        let (keys, rows) = score_rows(300);
        let page_size = 256u32;
        let mut w = PaxStreamWriter::new(keys.clone(), vec![b'~'], page_size).unwrap();

        let mut file = Vec::new();
        let _data_start = w.write_stream_header(&mut file).unwrap();

        let mut flushed_at = 0usize;
        for (i, row) in rows.iter().enumerate() {
            if let Some(page) = w.push_record(row.clone()).unwrap() {
                assert_eq!(page.len() % 8, 0);
                if i + 1 == 256 {
                    assert_eq!(w.flushed_page_count(), 1);
                }
            }
            w.write_data_sector_since(&mut file, flushed_at).unwrap();
            flushed_at = w.data_sector_len();
        }
        assert_eq!(w.flushed_page_count(), 1);
        assert_eq!(w.pending_count(), 44);

        let partial = &file[..];
        let sr = PaxStreamReader::open(partial).unwrap();
        assert!(!sr.is_sealed());
        assert_eq!(sr.complete_record_count(), 256);
        assert!(sr.has_complete_page());
        assert_eq!(sr.get_f64_complete(0, "score"), Some(0.0));
        assert_eq!(sr.get_f64_complete(255, "score"), Some(255.0));
        assert_eq!(sr.get_f64_complete(256, "score"), None);
        let sum_partial = sr.col_sum_f64_complete_pages("score").unwrap();
        let expected_partial: f64 = (0..256).map(|i| i as f64).sum();
        assert!((sum_partial - expected_partial).abs() < 1e-6);

        w.flush_pending().unwrap();
        w.write_data_sector_since(&mut file, flushed_at).unwrap();

        let sealed = w.finish_stream().unwrap();
        assert!(sealed.len() > file.len());
        let flags = u16::from_le_bytes(sealed[6..8].try_into().unwrap());
        assert!(flags & FLAG_PAX != 0);
        let tail_off = u64::from_le_bytes(sealed[16..24].try_into().unwrap());
        assert!(tail_off > 0);

        let sr2 = PaxStreamReader::open(&sealed).unwrap();
        assert!(sr2.is_sealed());
        assert_eq!(sr2.complete_record_count(), 300);

        let reader = Reader::new(&sealed).unwrap();
        assert_eq!(reader.record_count(), 300);
        assert_eq!(reader.layout(), crate::query::Layout::Pax);
        let sum = reader.col_sum_f64("score").unwrap();
        let expected: f64 = (0..300).map(|i| i as f64).sum();
        assert!((sum - expected).abs() < 1e-6);
        assert_eq!(reader.record(299).and_then(|r| r.get_f64("score")), Some(299.0));
    }

    #[test]
    fn complete_page_end_matches_encode_page() {
        let (_keys, rows) = score_rows(10);
        let page = encode_page(0, 0, 10, 1, &[b'~'], &rows).unwrap();
        assert_eq!(complete_page_end(&page, 0), Some(page.len()));
    }
}
