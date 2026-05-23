//! Adaptive prefetch — page cache, range coalescing, in-flight dedup (spec §6–§8.4).
//!
//! Phase 1: row-layout viewport prefetch for buffer-backed readers (sync path).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

pub const DEFAULT_PAGE_SIZE: usize = 65_536;
pub const DEFAULT_MAX_PAGES: usize = 256;
pub const DEFAULT_COALESCE_GAP_PAGES: usize = 1;

/// Caller access hint at open time (advisory; phase 1 stores only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AccessHint {
    #[default]
    Unknown = 0,
    Sequential = 1,
    Random = 2,
    Full = 3,
    Partial = 4,
}

/// Open-time prefetch configuration.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub hint: AccessHint,
    pub max_pages: usize,
    pub page_size: usize,
    pub coalesce_gap_pages: usize,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            hint: AccessHint::Unknown,
            max_pages: DEFAULT_MAX_PAGES,
            page_size: DEFAULT_PAGE_SIZE,
            coalesce_gap_pages: DEFAULT_COALESCE_GAP_PAGES,
        }
    }
}

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hint(mut self, hint: AccessHint) -> Self {
        self.hint = hint;
        self
    }

    pub fn max_pages(mut self, max_pages: usize) -> Self {
        self.max_pages = max_pages;
        self
    }

    pub fn page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    pub fn coalesce_gap_pages(mut self, gap: usize) -> Self {
        self.coalesce_gap_pages = gap;
        self
    }
}

/// A coalesced byte range covering one or more page indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoalescedRange {
    pub page_start: u32,
    pub page_end: u32,
    pub byte_start: usize,
    pub byte_length: usize,
}

/// Merge sorted unique page indices into fetch ranges (gap inclusive).
pub fn coalesce_page_indices(
    indices: &[u32],
    gap_pages: usize,
    page_size: usize,
) -> Vec<CoalescedRange> {
    if indices.is_empty() {
        return Vec::new();
    }
    let mut uniq: Vec<u32> = indices.to_vec();
    uniq.sort_unstable();
    uniq.dedup();

    let mut spans: Vec<(u32, u32)> = Vec::new();
    let mut start = uniq[0];
    let mut end = uniq[0];
    for &idx in &uniq[1..] {
        if idx.saturating_sub(end) <= gap_pages as u32 {
            end = idx;
        } else {
            spans.push((start, end));
            start = idx;
            end = idx;
        }
    }
    spans.push((start, end));

    spans
        .into_iter()
        .map(|(a, b)| CoalescedRange {
            page_start: a,
            page_end: b,
            byte_start: a as usize * page_size,
            byte_length: (b - a + 1) as usize * page_size,
        })
        .collect()
}

/// Clamp coalesced ranges to `file_size` bytes.
pub fn clamp_ranges(ranges: Vec<CoalescedRange>, file_size: usize) -> Vec<CoalescedRange> {
    ranges
        .into_iter()
        .filter_map(|mut r| {
            if r.byte_start >= file_size {
                return None;
            }
            if r.byte_start + r.byte_length > file_size {
                r.byte_length = file_size - r.byte_start;
            }
            if r.byte_length == 0 {
                return None;
            }
            Some(r)
        })
        .collect()
}

struct PageEntry {
    data: Vec<u8>,
    last_used: u64,
    pinned: bool,
}

/// Bounded LRU page cache.
pub struct PageCache {
    max_pages: usize,
    page_size: usize,
    pages: HashMap<u32, PageEntry>,
    clock: u64,
    hits: u64,
    misses: u64,
}

impl PageCache {
    pub fn new(max_pages: usize, page_size: usize) -> Self {
        Self {
            max_pages,
            page_size,
            pages: HashMap::new(),
            clock: 0,
            hits: 0,
            misses: 0,
        }
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn has(&self, page_index: u32) -> bool {
        self.pages.contains_key(&page_index)
    }

    pub fn get(&mut self, page_index: u32) -> Option<&[u8]> {
        let entry = self.pages.get_mut(&page_index)?;
        self.clock = self.clock.saturating_add(1);
        entry.last_used = self.clock;
        self.hits = self.hits.saturating_add(1);
        Some(entry.data.as_slice())
    }

    pub fn set(&mut self, page_index: u32, data: Vec<u8>, pinned: bool) {
        if self.max_pages == 0 {
            return;
        }
        while self.pages.len() >= self.max_pages {
            if !self.evict_one() {
                break;
            }
        }
        self.clock = self.clock.saturating_add(1);
        self.pages.insert(
            page_index,
            PageEntry {
                data,
                last_used: self.clock,
                pinned,
            },
        );
    }

    fn evict_one(&mut self) -> bool {
        let victim = self
            .pages
            .iter()
            .filter(|(_, e)| !e.pinned)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| *k);
        if let Some(v) = victim {
            self.pages.remove(&v);
            true
        } else {
            false
        }
    }

    pub fn pin_pages(&mut self, page_indices: &[u32]) {
        for &p in page_indices {
            if let Some(e) = self.pages.get_mut(&p) {
                e.pinned = true;
            }
        }
    }

    pub fn unpin_all(&mut self) {
        for e in self.pages.values_mut() {
            e.pinned = false;
        }
    }

    pub fn pages_cached(&self) -> usize {
        self.pages.len()
    }

    pub fn memory_used_bytes(&self) -> usize {
        self.pages.values().map(|e| e.data.len()).sum()
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Register a cache miss without inserting (used when prefetch is disabled).
    pub fn note_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }
}

/// Runtime cache statistics (`cache_stats()` return shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStats {
    pub pages_cached: usize,
    pub pages_max: usize,
    pub memory_used_bytes: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub fetches_issued: u64,
    pub strategy: String,
    pub pattern: String,
}

/// Per-reader prefetch engine (sync buffer path).
pub struct PrefetchEngine {
    cache: RefCell<PageCache>,
    in_flight: RefCell<HashSet<u32>>,
    fetches_issued: Cell<u64>,
    options: OpenOptions,
}

impl PrefetchEngine {
    pub fn new(options: OpenOptions) -> Self {
        let cache = PageCache::new(options.max_pages, options.page_size);
        Self {
            cache: RefCell::new(cache),
            in_flight: RefCell::new(HashSet::new()),
            fetches_issued: Cell::new(0),
            options,
        }
    }

    pub fn options(&self) -> &OpenOptions {
        &self.options
    }

    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.borrow();
        CacheStats {
            pages_cached: cache.pages_cached(),
            pages_max: self.options.max_pages,
            memory_used_bytes: cache.memory_used_bytes(),
            cache_hits: cache.hits(),
            cache_misses: cache.misses(),
            fetches_issued: self.fetches_issued.get(),
            strategy: "lazy".to_string(),
            pattern: "unknown".to_string(),
        }
    }

    pub fn touch_page(&self, page_index: u32) {
        let mut cache = self.cache.borrow_mut();
        if cache.get(page_index).is_none() {
            cache.note_miss();
        }
    }

    /// Populate cache for row-layout viewport `[start_index, end_index]` inclusive.
    pub fn prefetch_viewport(
        &self,
        data: &[u8],
        tail_start: usize,
        record_count: usize,
        start_index: usize,
        end_index: usize,
    ) {
        if record_count == 0 || data.is_empty() {
            return;
        }
        let start = start_index.min(record_count - 1);
        let end = end_index.min(record_count - 1);
        if start > end {
            return;
        }

        let page_size = self.options.page_size;
        let gap = self.options.coalesce_gap_pages;
        let file_size = data.len();

        let mut page_indices: Vec<u32> = Vec::new();
        for i in start..=end {
            if let Some(off) = row_record_offset(data, tail_start, i) {
                page_indices.push((off / page_size) as u32);
            }
        }
        page_indices.sort_unstable();
        page_indices.dedup();

        let pinned: Vec<u32> = page_indices.clone();

        let needed: Vec<u32> = page_indices
            .into_iter()
            .filter(|&p| {
                !self.cache.borrow().has(p) && !self.in_flight.borrow().contains(&p)
            })
            .collect();

        let ranges = clamp_ranges(coalesce_page_indices(&needed, gap, page_size), file_size);

        for range in ranges {
            self.fetch_range(data, &range, page_size, file_size);
        }

        self.cache.borrow_mut().pin_pages(&pinned);
    }

    fn fetch_range(&self, data: &[u8], range: &CoalescedRange, page_size: usize, file_size: usize) {
        for p in range.page_start..=range.page_end {
            self.in_flight.borrow_mut().insert(p);
        }

        self.fetches_issued
            .set(self.fetches_issued.get().saturating_add(1));

        for p in range.page_start..=range.page_end {
            let p_usize = p as usize;
            let byte_start = p_usize * page_size;
            if byte_start >= file_size {
                self.in_flight.borrow_mut().remove(&p);
                continue;
            }
            let byte_end = ((p_usize + 1) * page_size).min(file_size);
            let page_data = data[byte_start..byte_end].to_vec();
            self.cache.borrow_mut().set(p, page_data, false);
            self.in_flight.borrow_mut().remove(&p);
        }
    }
}

/// Tail-index offset for row layout record `index`.
pub fn row_record_offset(data: &[u8], tail_start: usize, index: usize) -> Option<usize> {
    let entry = tail_start.checked_add(index.checked_mul(10)?)?;
    let end = entry.checked_add(10)?;
    Some(u64::from_le_bytes(
        data.get(entry + 2..end)?.try_into().ok()?,
    ) as usize)
}

/// Page indices touched by records `[start_index, end_index]` inclusive.
pub fn page_indices_for_viewport(
    start_index: usize,
    end_index: usize,
    page_size: usize,
    record_offset: impl Fn(usize) -> Option<usize>,
) -> Vec<u32> {
    let mut out = Vec::new();
    for i in start_index..=end_index {
        if let Some(off) = record_offset(i) {
            out.push((off / page_size) as u32);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Reader;
    use crate::writer::{NxsWriter, Schema};

    fn make_sparse_nxb(n: usize) -> Vec<u8> {
        let schema = Schema::new(&["id", "payload"]);
        let mut w = NxsWriter::new(&schema);
        for i in 0..n {
            w.begin_object();
            w.write_i64(crate::writer::Slot(0), i as i64);
            // Pad each record so offsets spread across multiple pages at 64 KiB page size.
            let pad = format!("record-{i:04}-{}", "x".repeat(4096 + (i % 7) * 512));
            w.write_str(crate::writer::Slot(1), &pad);
            w.end_object();
        }
        w.finish()
    }

    #[test]
    fn coalesce_adjacent_pages() {
        let indices = vec![3, 4, 6, 7, 12];
        let ranges = coalesce_page_indices(&indices, 1, DEFAULT_PAGE_SIZE);
        assert_eq!(ranges.len(), 3);
        assert_eq!(
            ranges[0],
            CoalescedRange {
                page_start: 3,
                page_end: 4,
                byte_start: 3 * DEFAULT_PAGE_SIZE,
                byte_length: 2 * DEFAULT_PAGE_SIZE,
            }
        );
        assert_eq!(
            ranges[1],
            CoalescedRange {
                page_start: 6,
                page_end: 7,
                byte_start: 6 * DEFAULT_PAGE_SIZE,
                byte_length: 2 * DEFAULT_PAGE_SIZE,
            }
        );
        assert_eq!(
            ranges[2],
            CoalescedRange {
                page_start: 12,
                page_end: 12,
                byte_start: 12 * DEFAULT_PAGE_SIZE,
                byte_length: DEFAULT_PAGE_SIZE,
            }
        );
    }

    #[test]
    fn coalesce_empty_indices() {
        assert!(coalesce_page_indices(&[], 1, DEFAULT_PAGE_SIZE).is_empty());
    }

    #[test]
    fn coalesce_deduplicates() {
        let indices = vec![5, 5, 6];
        let ranges = coalesce_page_indices(&indices, 1, DEFAULT_PAGE_SIZE);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].page_start, 5);
        assert_eq!(ranges[0].page_end, 6);
    }

    #[test]
    fn page_cache_lru_eviction() {
        let mut cache = PageCache::new(2, 16);
        cache.set(1, vec![1; 16], false);
        cache.set(2, vec![2; 16], false);
        let _ = cache.get(1);
        cache.set(3, vec![3; 16], false);
        assert!(!cache.has(2));
        assert!(cache.has(1));
        assert!(cache.has(3));
    }

    #[test]
    fn page_cache_pinned_not_evicted() {
        let mut cache = PageCache::new(1, 8);
        cache.set(1, vec![1; 8], true);
        cache.set(2, vec![2; 8], false);
        assert!(cache.has(1));
        assert!(cache.has(2));
    }

    #[test]
    fn prefetch_viewport_populates_cache() {
        let data = make_sparse_nxb(50);
        let opts = OpenOptions::new();
        let reader = Reader::with_options(&data, opts).unwrap();
        reader.prefetch_viewport(0, 49).unwrap();
        let stats = reader.cache_stats();
        assert!(stats.pages_cached > 0);
        assert!(stats.fetches_issued > 0);
    }

    #[test]
    fn prefetch_viewport_cache_hits_on_read() {
        let data = make_sparse_nxb(50);
        let opts = OpenOptions::new();
        let reader = Reader::with_options(&data, opts).unwrap();
        reader.prefetch_viewport(0, 49).unwrap();
        for i in 0..50 {
            let rec = reader.record(i).unwrap();
            assert_eq!(rec.get_i64("id"), Some(i as i64));
        }
        let stats = reader.cache_stats();
        assert!(stats.cache_hits > 0);
    }

    #[test]
    fn prefetch_deduplication_skips_cached_pages() {
        let data = make_sparse_nxb(20);
        let opts = OpenOptions::new();
        let reader = Reader::with_options(&data, opts).unwrap();
        reader.prefetch_viewport(0, 19).unwrap();
        let first = reader.cache_stats().fetches_issued;
        reader.prefetch_viewport(0, 19).unwrap();
        let second = reader.cache_stats().fetches_issued;
        assert_eq!(first, second);
    }

    #[test]
    fn prefetch_memory_eviction_respects_max_pages() {
        let data = make_sparse_nxb(80);
        let opts = OpenOptions::new().max_pages(4);
        let reader = Reader::with_options(&data, opts).unwrap();
        reader.prefetch_viewport(0, 79).unwrap();
        let stats = reader.cache_stats();
        assert!(stats.pages_cached <= 4);
    }

    #[test]
    fn open_options_defaults() {
        let opts = OpenOptions::default();
        assert_eq!(opts.max_pages, 256);
        assert_eq!(opts.page_size, 65_536);
        assert_eq!(opts.coalesce_gap_pages, 1);
        assert_eq!(opts.hint, AccessHint::Unknown);
    }
}
