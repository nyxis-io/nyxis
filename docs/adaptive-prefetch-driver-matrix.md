# Adaptive prefetch — §9.1 driver matrix and sign-off

**Spec:** [`Adaptive-prefetch-spec.md`](../../Adaptive-prefetch-spec.md) §9.1–§9.4  
**Status:** MUST features implemented; sign-off recorded 2026-05-24 after phases 1–4 merged.

## Conformance gate (§9.4)

All seven prefetch vectors under [`conformance/prefetch/`](../conformance/prefetch/) must pass per driver:

| Vector | Phase |
| --- | --- |
| `prefetch_viewport_basic` | 1 |
| `prefetch_range_coalescing` | 1 |
| `prefetch_memory_eviction` | 1 |
| `prefetch_deduplication` | 1 |
| `prefetch_sequential_upgrade` | 2 |
| `prefetch_cancel` | 3 |
| `prefetch_columnar_fast_path` | 4 |

**Gate command:** `make conformance-prefetch PREFETCH=1` (Go harness + JS stub runner).

## Sign-off record

Maintainer attestation: MUST cells in §9.1 are implemented; §9.4 vectors pass on `main` as of the dates below. SHOULD/MAY cells follow the documented waivers.

| Driver | Package | Prefetch tests | §9.4 | Sign-off | Waivers / notes |
| --- | --- | --- | --- | --- | --- |
| **Rust** | `nyxis/rust` | `prefetch` module unit + conformance vectors | ✅ | 2026-05-24 | Memory pressure SHOULD via cache limits |
| **Go** | `nyxis-drivers/go` | `prefetch_test.go`, `column_prefetch_test.go`, harness | ✅ | 2026-05-24 | Reference remote-bench driver |
| **C** | `nyxis-drivers/c` | `make test-prefetch` | ✅ | 2026-05-24 | Sync-only; async prefetch optional (`NXS_ASYNC=1`); SHOULD speculative features via sequential viewport |
| **Python** | `nyxis-drivers/py` | `test_prefetch.py`, `test_c_ext.py` | ✅ | 2026-05-24 | SHOULD pattern/strategy via pure-Python path |
| **JavaScript** | `nyxis-drivers/js` | `test/prefetch.test.js` | ✅ | 2026-05-24 | AbortController MUST; browser `fetchRange` |
| **Ruby** | `nyxis-drivers/ruby` | `test_prefetch.rb` | ✅ | 2026-05-24 | SHOULD adaptive features; GIL limits parallelism |
| **PHP** | `nyxis-drivers/php` | `test_prefetch.php` | ✅ | 2026-05-24 | MAY async; MUST page cache + viewport + columnar |
| **Kotlin** (JVM) | `nyxis-drivers/kotlin` | `PrefetchTest.kt` | ✅ | 2026-05-24 | Memory pressure MUST |
| **C# (.NET)** | `nyxis-drivers/csharp` | `PrefetchTests.cs` | ✅ | 2026-05-24 | |
| **Swift** | `nyxis-drivers/swift` | `PrefetchTests.swift` | ✅ | 2026-05-24 | Memory pressure MUST |

**Signed:** nyxis-io maintainers (automated conformance + driver prefetch suites on `main`, PRs [#39](https://github.com/nyxis-io/nyxis/pull/39)–[#41](https://github.com/nyxis-io/nyxis/pull/41), drivers [#26](https://github.com/nyxis-io/nyxis-drivers/pull/26)–[#27](https://github.com/nyxis-io/nyxis-drivers/pull/27)).

## §9.1 feature matrix (implementation)

Legend: **✓** = shipped for MUST/SHOULD; **~** = partial / sync-only waiver; **—** = N/A or MAY omitted.

| Feature | Rust | Go | C | Python | JS | Ruby | PHP | Kotlin | C# | Swift |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Page cache | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| LRU eviction | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Access pattern detector | ✓ | ✓ | ~ | ~ | ✓ | ~ | — | ~ | ~ | ~ |
| Strategy selection | ✓ | ✓ | ~ | ~ | ✓ | ~ | — | ~ | ~ | ~ |
| Access hints API | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Speculative prefetch | ✓ | ✓ | ~ | ~ | ✓ | ~ | — | ~ | ~ | ~ |
| Viewport prefetch | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Range coalescing | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | — | ✓ | ✓ | ✓ |
| Eager prefetch | ✓ | ✓ | ~ | ~ | ✓ | ~ | — | ~ | ~ | ~ |
| Columnar fast path | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Pause/resume | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Memory pressure | ~ | ~ | ~ | ~ | ~ | — | — | ✓ | ~ | ✓ |
| AbortController | — | — | — | — | ✓ | — | — | — | — | — |
| Fetch deduplication | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

Update this table when a driver gains a SHOULD feature or §9.4 coverage expands.

## Maintainer checklist (per driver release)

1. `make test-<lang>` includes prefetch suite.
2. §9.4 vectors pass (or documented skip with reason).
3. `cache_stats()` schema matches spec §10.3 where implemented.
4. Row in sign-off table updated with date and PR link.
