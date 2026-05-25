---
room: prefetch
subdomain: rust
source_paths: [rust/src/prefetch/, rust/src/column_prefetch.rs]
see_also: ["writer_decoder.md", "runtime.md", "../bench/harness.md"]
hot_paths: [rust/src/prefetch/mod.rs, rust/src/prefetch/pattern.rs]
architectural_health: normal
security_tier: normal
---

# rust/ — Adaptive Prefetch

Subdomain: rust/
Source paths: rust/src/prefetch/, rust/src/column_prefetch.rs

## TASK → LOAD

| Task | Load |
|------|------|
| Tune page cache, coalesce, or prefetch depth | prefetch.md |
| Change access-pattern detector thresholds | prefetch.md |
| Warm columnar slots on first read | prefetch.md |

---

# column_prefetch.rs

DOES: Per-reader column warmup state for columnar/PAX layouts; tracks warmed slots and fetch counts separate from row page cache (spec §7.4).
SYMBOLS:
- ColumnWarmState { warmed Mutex, fetches AtomicU64 }
- prefetch(&self, slot usize) → bool
- fetches(&self) → u64
PATTERNS: slot-warmup-cache

---

# prefetch/mod.rs

DOES: Adaptive prefetch engine: page cache, AccessHint/OpenOptions, Lazy/Adaptive/Eager strategies, background prefetch threads, integration with query Reader.
SYMBOLS:
- PrefetchEngine, OpenOptions, AccessHint, PrefetchStrategy
- DEFAULT_PAGE_SIZE, DEFAULT_MAX_PAGES, DEFAULT_PREFETCH_DEPTH constants
- open_with_options(data, opts) → Result<Reader>
- (+page cache, coalesce, upgrade sequential)
TYPE: OpenOptions { hint, max_pages, page_size, coalesce_gap_pages, prefetch_depth }
DEPENDS: prefetch/pattern.rs, error.rs
PATTERNS: adaptive-prefetch, background-worker

---

# prefetch/pattern.rs

DOES: Access pattern detector (sequential vs random) with upgrade threshold for switching prefetch strategy.
SYMBOLS:
- AccessPatternDetector
- AccessPattern enum
- UPGRADE_SEQUENTIAL_THRESHOLD constant
PATTERNS: sequential-detection
