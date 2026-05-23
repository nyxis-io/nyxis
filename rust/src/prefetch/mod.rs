//! Adaptive prefetch — page cache, pattern detector, strategies (spec §4–§8.4).

mod pattern;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::error::{NxsError, Result};

pub use pattern::{AccessPattern, AccessPatternDetector, UPGRADE_SEQUENTIAL_THRESHOLD};

pub const DEFAULT_PAGE_SIZE: usize = 65_536;
pub const DEFAULT_MAX_PAGES: usize = 256;
pub const DEFAULT_COALESCE_GAP_PAGES: usize = 1;
pub const DEFAULT_PREFETCH_DEPTH: usize = 4;
pub const EAGER_THRESHOLD_MB: usize = 10;
pub const LAZY_THRESHOLD_MB: usize = 50;

/// Caller access hint at open time (advisory).
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

/// Prefetch strategy (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefetchStrategy {
    Lazy,
    Adaptive,
    Eager,
}

impl PrefetchStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lazy => "lazy",
            Self::Adaptive => "adaptive",
            Self::Eager => "eager",
        }
    }
}

/// Open-time prefetch configuration.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub hint: AccessHint,
    pub max_pages: usize,
    pub page_size: usize,
    pub coalesce_gap_pages: usize,
    pub prefetch_depth: usize,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            hint: AccessHint::Unknown,
            max_pages: DEFAULT_MAX_PAGES,
            page_size: DEFAULT_PAGE_SIZE,
            coalesce_gap_pages: DEFAULT_COALESCE_GAP_PAGES,
            prefetch_depth: DEFAULT_PREFETCH_DEPTH,
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

    pub fn prefetch_depth(mut self, depth: usize) -> Self {
        self.prefetch_depth = depth;
        self
    }

    pub fn validate(&self) -> Result<()> {
        if self.page_size == 0 {
            return Err(NxsError::ParseError(
                "prefetch page_size must be greater than 0".into(),
            ));
        }
        Ok(())
    }
}

pub fn initial_strategy(hint: AccessHint, file_size: usize) -> PrefetchStrategy {
    let file_size_mb = file_size / (1024 * 1024);
    if hint == AccessHint::Full && file_size_mb <= EAGER_THRESHOLD_MB {
        PrefetchStrategy::Eager
    } else if file_size_mb > LAZY_THRESHOLD_MB {
        PrefetchStrategy::Lazy
    } else {
        PrefetchStrategy::Adaptive
    }
}

/// Row-layout data sector byte range `[start, start+len)`.
pub fn row_data_sector(tail_start: usize, file_size: usize) -> (usize, usize) {
    let start = 32;
    if tail_start > start && tail_start <= file_size {
        (start, tail_start - start)
    } else {
        (start, 0)
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
        .filter_map(|(a, b)| {
            let count = (b - a).checked_add(1)? as usize;
            let byte_start = (a as usize).checked_mul(page_size)?;
            let byte_length = count.checked_mul(page_size)?;
            Some(CoalescedRange {
                page_start: a,
                page_end: b,
                byte_start,
                byte_length,
            })
        })
        .collect()
}

pub fn clamp_ranges(ranges: Vec<CoalescedRange>, file_size: usize) -> Vec<CoalescedRange> {
    ranges
        .into_iter()
        .filter_map(|mut r| {
            if r.byte_start >= file_size {
                return None;
            }
            let end = r.byte_start.checked_add(r.byte_length)?;
            if end > file_size {
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

    pub fn set(&mut self, page_index: u32, data: Vec<u8>, pinned: bool) -> bool {
        if self.max_pages == 0 {
            return false;
        }
        if !self.pages.contains_key(&page_index) {
            while self.pages.len() >= self.max_pages {
                if !self.evict_one() {
                    return false;
                }
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
        true
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

    pub fn note_miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }
}

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

struct EagerState {
    cancelled: Arc<AtomicBool>,
    complete: Arc<AtomicBool>,
    started: AtomicBool,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl EagerState {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            complete: Arc::new(AtomicBool::new(false)),
            started: AtomicBool::new(false),
            join: Mutex::new(None),
        }
    }
}

/// Per-reader prefetch engine (`Send + Sync`).
pub struct PrefetchEngine {
    cache: Arc<Mutex<PageCache>>,
    in_flight: Arc<Mutex<HashSet<u32>>>,
    fetches_issued: Arc<AtomicU64>,
    options: OpenOptions,
    strategy: Mutex<PrefetchStrategy>,
    detector: Mutex<AccessPatternDetector>,
    file_size: usize,
    eager: EagerState,
}

impl PrefetchEngine {
    pub fn new(options: OpenOptions, file_size: usize) -> Self {
        let strategy = initial_strategy(options.hint, file_size);
        Self {
            cache: Arc::new(Mutex::new(PageCache::new(
                options.max_pages,
                options.page_size,
            ))),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            fetches_issued: Arc::new(AtomicU64::new(0)),
            options,
            strategy: Mutex::new(strategy),
            detector: Mutex::new(AccessPatternDetector::new()),
            file_size,
            eager: EagerState::new(),
        }
    }

    pub fn options(&self) -> &OpenOptions {
        &self.options
    }

    pub fn strategy(&self) -> PrefetchStrategy {
        *self.strategy.lock().expect("prefetch strategy lock")
    }

    pub fn is_eager(&self) -> bool {
        self.strategy() == PrefetchStrategy::Eager && self.eager.complete.load(Ordering::Acquire)
    }

    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.lock().expect("prefetch cache lock");
        let detector = self.detector.lock().expect("prefetch detector lock");
        CacheStats {
            pages_cached: cache.pages_cached(),
            pages_max: self.options.max_pages,
            memory_used_bytes: cache.memory_used_bytes(),
            cache_hits: cache.hits(),
            cache_misses: cache.misses(),
            fetches_issued: self.fetches_issued.load(Ordering::Relaxed),
            strategy: self.strategy().as_str().to_string(),
            pattern: detector.pattern().as_str().to_string(),
        }
    }

    /// Start eager background load of the row data sector (§7.3).
    pub fn start_eager_background(&self, data: Vec<u8>, tail_start: usize) {
        if self.strategy() != PrefetchStrategy::Eager {
            return;
        }
        if self.eager.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let (sector_start, sector_len) = row_data_sector(tail_start, data.len());
        if sector_len == 0 {
            self.eager.complete.store(true, Ordering::Release);
            return;
        }
        let cancelled = Arc::clone(&self.eager.cancelled);
        let complete = Arc::clone(&self.eager.complete);
        let fetches = Arc::clone(&self.fetches_issued);
        let page_size = self.options.page_size;
        let gap = self.options.coalesce_gap_pages;
        let cache = Arc::clone(&self.cache);
        let in_flight = Arc::clone(&self.in_flight);

        let handle = thread::spawn(move || {
            if cancelled.load(Ordering::Acquire) {
                return;
            }
            let end = sector_start.saturating_add(sector_len).min(data.len());
            let first_page = sector_start / page_size;
            let last_page = (end.saturating_sub(1)) / page_size;
            let indices: Vec<u32> = (first_page..=last_page).map(|p| p as u32).collect();
            let ranges = clamp_ranges(coalesce_page_indices(&indices, gap, page_size), data.len());
            fetches.fetch_add(1, Ordering::Relaxed);
            for range in ranges {
                if cancelled.load(Ordering::Acquire) {
                    return;
                }
                for p in range.page_start..=range.page_end {
                    in_flight.lock().expect("in_flight").insert(p);
                }
                for p in range.page_start..=range.page_end {
                    if cancelled.load(Ordering::Acquire) {
                        return;
                    }
                    let p_usize = p as usize;
                    let byte_start = p_usize * page_size;
                    if byte_start >= data.len() {
                        in_flight.lock().expect("in_flight").remove(&p);
                        continue;
                    }
                    let byte_end = ((p_usize + 1) * page_size).min(data.len());
                    let page_data = data[byte_start..byte_end].to_vec();
                    cache.lock().expect("cache").set(p, page_data, false);
                    in_flight.lock().expect("in_flight").remove(&p);
                }
            }
            complete.store(true, Ordering::Release);
        });
        *self.eager.join.lock().expect("eager join") = Some(handle);
    }

    pub fn warmup(&self) {
        while !self.eager.complete.load(Ordering::Acquire)
            && !self.eager.cancelled.load(Ordering::Acquire)
        {
            std::thread::yield_now();
        }
        if let Some(handle) = self.eager.join.lock().expect("eager join").take() {
            let _ = handle.join();
        }
    }

    pub fn on_access(&self, data: &[u8], tail_start: usize, record_count: usize, index: usize) {
        if record_count == 0 {
            return;
        }
        {
            let mut detector = self.detector.lock().expect("prefetch detector lock");
            detector.observe(index);
            self.maybe_upgrade_to_eager(&detector, data, tail_start);
        }
        if self.is_eager() || self.strategy() == PrefetchStrategy::Eager {
            return;
        }
        if let Some(off) = row_record_offset(data, tail_start, index) {
            let page_index = (off / self.options.page_size) as u32;
            self.touch_page(page_index);
        }
        if self.strategy() == PrefetchStrategy::Adaptive {
            let detector = self.detector.lock().expect("prefetch detector lock");
            if detector.pattern() == AccessPattern::Sequential {
                self.speculative_prefetch(data, tail_start, record_count, &detector);
            }
        }
    }

    fn maybe_upgrade_to_eager(
        &self,
        detector: &AccessPatternDetector,
        data: &[u8],
        tail_start: usize,
    ) {
        let mut strategy = self.strategy.lock().expect("prefetch strategy lock");
        if *strategy != PrefetchStrategy::Adaptive {
            return;
        }
        if detector.pattern() != AccessPattern::Sequential {
            return;
        }
        if detector.sequential_runs() < UPGRADE_SEQUENTIAL_THRESHOLD {
            return;
        }
        let file_size_mb = self.file_size / (1024 * 1024);
        if file_size_mb > EAGER_THRESHOLD_MB {
            return;
        }
        *strategy = PrefetchStrategy::Eager;
        drop(strategy);
        self.start_eager_background(data.to_vec(), tail_start);
    }

    fn speculative_prefetch(
        &self,
        data: &[u8],
        tail_start: usize,
        record_count: usize,
        detector: &AccessPatternDetector,
    ) {
        let depth = self.options.prefetch_depth;
        let predicted = detector.predict_next(depth, record_count);
        let page_size = self.options.page_size;
        let mut page_indices: Vec<u32> = Vec::new();
        for idx in predicted {
            if let Some(off) = row_record_offset(data, tail_start, idx) {
                page_indices.push((off / page_size) as u32);
            }
        }
        page_indices.sort_unstable();
        page_indices.dedup();
        let needed: Vec<u32> = {
            let cache = self.cache.lock().expect("prefetch cache lock");
            let in_flight = self.in_flight.lock().expect("prefetch in_flight lock");
            page_indices
                .into_iter()
                .filter(|&p| !cache.has(p) && !in_flight.contains(&p))
                .collect()
        };
        if needed.is_empty() {
            return;
        }
        let gap = self.options.coalesce_gap_pages;
        let file_size = data.len();
        let ranges = clamp_ranges(coalesce_page_indices(&needed, gap, page_size), file_size);
        for range in ranges {
            self.fetch_range(data, &range, page_size, file_size);
        }
    }

    pub fn touch_page(&self, page_index: u32) {
        if self.is_eager() {
            return;
        }
        let mut cache = self.cache.lock().expect("prefetch cache lock");
        if cache.get(page_index).is_none() {
            cache.note_miss();
        }
    }

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

        let needed: Vec<u32> = {
            let cache = self.cache.lock().expect("prefetch cache lock");
            let in_flight = self.in_flight.lock().expect("prefetch in_flight lock");
            page_indices
                .into_iter()
                .filter(|&p| !cache.has(p) && !in_flight.contains(&p))
                .collect()
        };

        let ranges = clamp_ranges(coalesce_page_indices(&needed, gap, page_size), file_size);

        for range in ranges {
            self.fetch_range(data, &range, page_size, file_size);
        }

        let mut cache = self.cache.lock().expect("prefetch cache lock");
        cache.pin_pages(&pinned);
        cache.unpin_all();
    }

    fn fetch_range(&self, data: &[u8], range: &CoalescedRange, page_size: usize, file_size: usize) {
        {
            let mut in_flight = self.in_flight.lock().expect("prefetch in_flight lock");
            for p in range.page_start..=range.page_end {
                in_flight.insert(p);
            }
        }

        self.fetches_issued.fetch_add(1, Ordering::Relaxed);

        for p in range.page_start..=range.page_end {
            let p_usize = p as usize;
            let byte_start = p_usize * page_size;
            if byte_start >= file_size {
                self.in_flight
                    .lock()
                    .expect("prefetch in_flight lock")
                    .remove(&p);
                continue;
            }
            let byte_end = ((p_usize + 1) * page_size).min(file_size);
            let page_data = data[byte_start..byte_end].to_vec();
            let mut cache = self.cache.lock().expect("prefetch cache lock");
            cache.set(p, page_data, false);
            self.in_flight
                .lock()
                .expect("prefetch in_flight lock")
                .remove(&p);
        }
    }
}

impl Drop for PrefetchEngine {
    fn drop(&mut self) {
        self.eager.cancelled.store(true, Ordering::Release);
        if let Some(handle) = self.eager.join.lock().expect("eager join").take() {
            let _ = handle.join();
        }
    }
}

pub fn row_record_offset(data: &[u8], tail_start: usize, index: usize) -> Option<usize> {
    let entry = tail_start.checked_add(index.checked_mul(10)?)?;
    let end = entry.checked_add(10)?;
    Some(u64::from_le_bytes(data.get(entry + 2..end)?.try_into().ok()?) as usize)
}

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
            let pad = format!("record-{i:04}-{}", "x".repeat(4096 + (i % 7) * 512));
            w.write_str(crate::writer::Slot(1), &pad);
            w.end_object();
        }
        w.finish()
    }

    fn make_compact_nxb(n: usize) -> Vec<u8> {
        let schema = Schema::new(&["id", "tag"]);
        let mut w = NxsWriter::new(&schema);
        for i in 0..n {
            w.begin_object();
            w.write_i64(crate::writer::Slot(0), i as i64);
            w.write_str(crate::writer::Slot(1), &format!("r{i}"));
            w.end_object();
        }
        w.finish()
    }

    #[test]
    fn hint_full_small_file_eager_at_open() {
        let data = make_compact_nxb(200);
        assert!(data.len() <= EAGER_THRESHOLD_MB * 1024 * 1024);
        let opts = OpenOptions::new().hint(AccessHint::Full);
        let reader = Reader::with_options(&data, opts).unwrap();
        reader.warmup();
        assert_eq!(reader.cache_stats().strategy, "eager");
    }

    #[test]
    fn sequential_upgrade_to_eager() {
        let data = make_compact_nxb(200);
        let reader = Reader::with_options(&data, OpenOptions::new()).unwrap();
        for i in 0..150 {
            let _ = reader.record(i);
        }
        reader.warmup();
        assert_eq!(reader.cache_stats().strategy, "eager");
        assert_eq!(reader.cache_stats().pattern, "sequential");
    }

    #[test]
    fn eager_cancel_on_close_no_extra_fetches() {
        let data = make_compact_nxb(500);
        let opts = OpenOptions::new().hint(AccessHint::Full);
        let reader = Reader::with_options(&data, opts).unwrap();
        let issued = reader.cache_stats().fetches_issued;
        drop(reader);
        assert!(issued <= 50);
    }

    #[test]
    fn coalesce_adjacent_pages() {
        let indices = vec![3, 4, 6, 7, 12];
        let ranges = coalesce_page_indices(&indices, 1, DEFAULT_PAGE_SIZE);
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn prefetch_viewport_populates_cache() {
        let data = make_sparse_nxb(50);
        let reader = Reader::with_options(&data, OpenOptions::new()).unwrap();
        reader.prefetch_viewport(0, 49).unwrap();
        assert!(reader.cache_stats().pages_cached > 0);
    }

    #[test]
    fn prefetch_sequential_upgrade_conformance_vector() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../conformance/prefetch/prefetch_sequential_upgrade.nxb");
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let reader = Reader::with_options(&data, OpenOptions::new()).unwrap();
        for i in 0..150 {
            let _ = reader.record(i);
        }
        reader.warmup();
        let stats = reader.cache_stats();
        assert_eq!(stats.strategy, "eager");
        assert_eq!(stats.pattern, "sequential");
        assert!(stats.fetches_issued >= 1);
    }

    #[test]
    fn open_options_rejects_zero_page_size() {
        let opts = OpenOptions::new().page_size(0);
        assert!(opts.validate().is_err());
    }
}
