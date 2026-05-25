---
room: harness
subdomain: bench
source_paths: [bench/harness/]
see_also: ["generators.md", "scripts.md", "../rust/prefetch.md"]
hot_paths: [bench/harness/go/main.go, bench/harness/prefetch/main.go]
architectural_health: normal
security_tier: normal
---

# bench/harness/ — Cross-Format Drivers

Subdomain: bench/
Source paths: bench/harness/

## TASK → LOAD

| Task | Load |
|------|------|
| Run standard workloads with Go driver | harness.md |
| Compare NXS vs JSON/XML/CSV in C harness | harness.md |
| Benchmark Workload F prefetch scenarios | harness.md |
| Stream-decode Cap'n Proto / NXB in Rust stream_d | harness.md |

---

# bench/harness/c/harness.c

DOES: C99 benchmark harness mirroring Go CLI; measures P50/P99/IQR latencies and emits JSON lines for workloads A–C across NXS, JSON, XML, CSV.
SYMBOLS:
- main(argc, argv) → int
- measure(fn) → timing stats
- (+format-specific serialize/read loops)
DEPENDS: nyxis-drivers C reader APIs
PATTERNS: warmup-then-sample

---

# bench/harness/c/stats.c

DOES: Percentile and IQR helpers for harness timing arrays (sort, p50, p99, iqr).
SYMBOLS:
- percentile(sorted, p) → int64
- iqr(sorted, n) → int64
- (+sort helpers)

---

# bench/harness/c/stats.h

DOES: Declares stats helpers used by harness.c for latency aggregation.
SYMBOLS:
- percentile, iqr declarations

---

# bench/harness/go/main.go

DOES: Go cross-format benchmark harness; shared CLI with C/Rust drivers; warmup 100 + 1000 samples; JSONL result lines per workload/format.
SYMBOLS:
- main()
- measure(fn func()) → (p50, p99, iqr int64)
- Types: result struct with Workload, Format, Records, Metric, Driver
DEPENDS: github.com/nyxis-io/nyxis-drivers/go
PATTERNS: warmup-then-sample

---

# bench/harness/prefetch/main.go

DOES: Workload F native bench via Go driver; simulates remote fetch latency; runs prefetch scenarios (F1–F4) and emits JSON metrics (fetches, cache hits/misses).
SYMBOLS:
- main()
- newRemoteReader(data, rs, opts) → (*nxs.Reader, error)
- runF1, runF2, runF3, runF4 scenario runners
- emit(l line)
- Types: line, remoteStore
DEPENDS: github.com/nyxis-io/nyxis-drivers/go
PATTERNS: simulated-remote-fetch, jsonl-metrics
USE WHEN: Measuring adaptive prefetch vs lazy/eager under artificial latency

---

# bench/harness/python/harness.py

DOES: Python harness matching Go/C CLI contract; emits JSONL timing rows for benchmark matrix.
SYMBOLS:
- main()
- measure(fn) → percentiles
- (+workload drivers using py/nxs)

---

# bench/harness/rust/src/main.rs

DOES: Rust benchmark harness; same workloads and JSONL output as Go/C drivers for apples-to-apples comparisons.
SYMBOLS:
- fn main()
- measure closure timing
- (+format encode/decode paths)

---

# bench/harness/stream_d/build.rs

DOES: Cargo build script for stream_d harness; invokes capnp/flat codegen for bench schemas.
SYMBOLS:
- fn main() build hooks

---

# bench/harness/stream_d/src/main.rs

DOES: Rust streaming decode benchmark (Cap'n Proto / NXB / flat buffers) for workload B-scale files; reports throughput JSONL.
SYMBOLS:
- fn main()
- (+stream decode loops per format)
