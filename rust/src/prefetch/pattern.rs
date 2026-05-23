//! Access pattern detector (Adaptive-prefetch-spec §4).

pub const SEQUENTIAL_THRESHOLD: u64 = 10;
pub const RANDOM_THRESHOLD: u64 = 100;
pub const HISTORY_SIZE: usize = 32;
pub const MIN_OBSERVATIONS: usize = 8;
pub const UPGRADE_SEQUENTIAL_THRESHOLD: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessPattern {
    Unknown,
    Sequential,
    Random,
    Mixed,
}

impl AccessPattern {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Sequential => "sequential",
            Self::Random => "random",
            Self::Mixed => "mixed",
        }
    }
}

/// Observes `record(index)` / seek calls and classifies access patterns.
#[derive(Debug, Clone)]
pub struct AccessPatternDetector {
    accesses: [i64; HISTORY_SIZE],
    write_pos: usize,
    filled: usize,
    sequential_runs: u32,
    random_jumps: u32,
    last_index: i64,
}

impl Default for AccessPatternDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl AccessPatternDetector {
    pub fn new() -> Self {
        Self {
            accesses: [-1; HISTORY_SIZE],
            write_pos: 0,
            filled: 0,
            sequential_runs: 0,
            random_jumps: 0,
            last_index: -1,
        }
    }

    pub fn sequential_runs(&self) -> u32 {
        self.sequential_runs
    }

    pub fn last_index(&self) -> i64 {
        self.last_index
    }

    pub fn observe(&mut self, index: usize) {
        let idx = index as i64;
        if self.last_index >= 0 {
            let delta = idx.abs_diff(self.last_index);
            if delta <= SEQUENTIAL_THRESHOLD {
                self.sequential_runs = self.sequential_runs.saturating_add(1);
            } else if delta > RANDOM_THRESHOLD {
                self.random_jumps = self.random_jumps.saturating_add(1);
            }
        }
        self.accesses[self.write_pos] = idx;
        self.write_pos = (self.write_pos + 1) % HISTORY_SIZE;
        if self.filled < HISTORY_SIZE {
            self.filled += 1;
        }
        self.last_index = idx;
    }

    pub fn pattern(&self) -> AccessPattern {
        let total = self.sequential_runs + self.random_jumps;
        if (total as usize) < MIN_OBSERVATIONS {
            return AccessPattern::Unknown;
        }
        if self.sequential_runs > self.random_jumps.saturating_mul(3) {
            AccessPattern::Sequential
        } else if self.random_jumps > self.sequential_runs {
            AccessPattern::Random
        } else {
            AccessPattern::Mixed
        }
    }

    /// Predicted next record indices when pattern is sequential (§4.4).
    pub fn predict_next(&self, depth: usize, record_count: usize) -> Vec<usize> {
        if self.pattern() != AccessPattern::Sequential || self.last_index < 0 {
            return Vec::new();
        }
        let start = self.last_index as usize + 1;
        (0..depth)
            .filter_map(|i| {
                let idx = start + i;
                if idx < record_count {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_unknown_until_min_observations() {
        let mut d = AccessPatternDetector::new();
        for i in 0..8 {
            d.observe(i);
        }
        assert_eq!(d.pattern(), AccessPattern::Unknown);
        d.observe(8);
        assert_ne!(d.pattern(), AccessPattern::Unknown);
    }

    #[test]
    fn pattern_sequential_small_deltas() {
        let mut d = AccessPatternDetector::new();
        for i in 0..20 {
            d.observe(i);
        }
        assert_eq!(d.pattern(), AccessPattern::Sequential);
    }

    #[test]
    fn pattern_random_large_jumps() {
        let mut d = AccessPatternDetector::new();
        for i in 0..8 {
            d.observe(i);
        }
        for j in (0..12).map(|k| k * 200) {
            d.observe(j);
        }
        assert_eq!(d.pattern(), AccessPattern::Random);
    }

    #[test]
    fn predict_next_sequential() {
        let mut d = AccessPatternDetector::new();
        for i in 0..10 {
            d.observe(i);
        }
        assert_eq!(d.predict_next(4, 100), vec![10, 11, 12, 13]);
    }
}
