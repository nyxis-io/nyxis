//! Columnar layout column warmup (Adaptive-prefetch-spec §7.4).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Per-reader column warmup (separate from row page cache).
#[derive(Default)]
pub struct ColumnWarmState {
    warmed: Mutex<HashSet<usize>>,
    fetches: AtomicU64,
}

impl ColumnWarmState {
    /// Mark `slot` warmed; returns `true` when a new fetch was issued.
    pub fn prefetch(&self, slot: usize) -> bool {
        let mut warmed = self.warmed.lock().expect("column warm lock");
        if warmed.insert(slot) {
            self.fetches.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn fetches(&self) -> u64 {
        self.fetches.load(Ordering::Relaxed)
    }
}
