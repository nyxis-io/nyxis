# Nyxis Benchmark Suite Specification

**Status:** Draft v0.1
**Purpose:** Define the work required to deliver verified head-to-head benchmarks against Protobuf, FlatBuffers, and Cap'n Proto, fulfilling the commitment made in the use-cases page Act II.
**Non-goal:** Producing a marketing matrix. The matrix is downstream of this work, not part of it.

---

## 1. Guiding principles

**1.1 Honest results, not engineered ones.** If Nyxis loses a workload, publish the loss. A 5-for-5 sweep is not a benchmark, it's a tell. Plan for at least one published loss and at least one published tie.

**1.2 Reproducibility over headline numbers.** Every published number must be reproducible by a third party running `make bench` on commodity hardware. No internal-only datasets, no proprietary fixtures.

**1.3 Methodology before numbers.** Each workload's methodology document must be written, reviewed, and frozen *before* benchmark runs. Numbers are then measured against the frozen methodology, not the other way around.

**1.4 Fair competitors.** Each format gets used the way its maintainers recommend, not the way that flatters Nyxis. Protobuf uses generated accessors, not reflection. FlatBuffers uses generated readers, not the verifier on hot paths. Cap'n Proto uses packed encoding where appropriate.

**1.5 No qualitative comparisons dressed as quantitative ones.** No "Heavy" / "Moderate" / "Bloated" labels. Either there's a number with units or the cell is empty.

---

## 2. Workloads

Three workloads, each with a single primary metric, deliberately chosen so Nyxis is expected to win one, tie one, and lose one.

### 2.1 Workload A — Sparse hierarchical records

**Hypothesis:** Nyxis wins on disk footprint and on read latency when fetching only populated fields.

**Schema:** 50 optional fields per record, mixed types (20 int64, 15 string, 10 float64, 5 bool). Three nesting levels for hierarchical variants.

**Population rate:** Configurable per run. Required runs: 10%, 25%, 50%, 90%. Same logical data across all four formats.

**Dataset size:** 1M records. Generate once into a canonical JSON source; transcode to each binary format from the same source.

**Primary metric:** File size on disk, in bytes, per format per population rate.

**Secondary metrics:**
- Time to read only the 5 populated fields from a randomly sampled record (P50, P99, in nanoseconds)
- Memory high-water mark during a full-scan reducer over one populated field

**Expected outcome:** Nyxis wins on file size at low population rates (≤25%); ties or wins on selective-read latency; ties or modestly loses at high population rates where the bitmask overhead doesn't pay off.

### 2.2 Workload B — Cold-open random access

**Hypothesis:** Nyxis ties FlatBuffers and Cap'n Proto on open time and per-record access; possibly wins on cold-start time for files >1 GB due to the tail-index design.

**Schema:** Flat 8-field record (matches existing BENCHMARK.md baseline so JSON numbers remain comparable).

**Dataset size:** 1M records, 10M records, 100M records. Three file sizes to test scaling.

**Primary metric:** Time from `open()` syscall to first record's first field being readable (P50, P99, in nanoseconds).

**Secondary metrics:**
- Time to access record N at random N (P50, P99)
- Resident set size after open, before any field access
- Time to scan and return field K for all 1M records

**Expected outcome:** All three zero-copy formats tie on open and per-record access within a small constant factor. Nyxis may have a measurable edge on >10M records because the tail-index decouples open time from file size, while FlatBuffers requires reading the root table. This is the workload where Nyxis's headline claims either survive or don't.

### 2.3 Workload C — Dense uniform analytical reducer

**Hypothesis:** Nyxis loses to columnar formats on this workload; this is the use case for the Arrow bridge, not for native NXS.

**Schema:** 8 fields, all populated, mostly numeric.

**Dataset size:** 1M and 10M records.

**Primary metric:** Time to compute `sum_f64(field)` and `count_distinct(field)` across the entire dataset (P50, P99, in milliseconds).

**Competitors:** FlatBuffers, Cap'n Proto, Protobuf, **and** Apache Arrow IPC. Arrow is the honest comparator here because it's what someone running this workload would actually choose.

**Expected outcome:** Arrow wins. Cap'n Proto and FlatBuffers tie each other and beat Nyxis by a small constant factor. Nyxis is the slowest of the zero-copy formats. Publish this loss prominently — it's how the page's Arrow positioning becomes credible.

### 2.4 Workload D — Streaming ingest & time-to-first-record

**Hypothesis:** NXS and Protobuf tie on TTFR (self-delimiting records); NXS pays a one-time seal cost for O(1) post-seal random access; Protobuf wins sustained streaming throughput; FlatBuffers is **not applicable** for native file-level streaming.

**Schema:** flat-8 (same as Workload B) for comparability.

**Primary metric:** Time-to-first-record (TTFR): wall-clock from writer’s first record write to reader returning the first complete record (µs; P50, P95, P99).

**Secondary metrics:** Sustained records/s while writer appends; seal latency (NXS); reader RSS high-water mark.

**Transport:** **D2** (file on disk + poll/read growth) is the publication variant; **D1** (POSIX FIFO) is secondary.

**Harness:** `make -C bench run-d-smoke` (Phase 1: Rust driver, NXS + Protobuf + Cap'n Proto). Methodology: `bench/methodology/workload_D.md`.

**Expected outcome:** NXS ≈ Protobuf on TTFR; NXS seal latency published honestly at 1M+ records; FlatBuffers reported as n/a for native streaming.

---

## 3. Languages and toolchains

**3.1 Primary language: C.** Every workload runs in C first. Reasons: removes managed-runtime variance, all four formats have first-class C support, and reviewers can audit the harness without language-specific noise.

**3.2 Secondary languages: Go, Rust.** Run the same workloads in Go and Rust to confirm the relative ordering holds across runtimes. Don't expand further until C/Go/Rust are stable.

**3.3 Format versions to pin:**
- Protobuf: latest stable (currently 5.x / `protoc` 27+)
- FlatBuffers: latest stable from `google/flatbuffers` main
- Cap'n Proto: 1.0.x
- Apache Arrow: 15.x for the Workload C IPC reader
- Nyxis: v1.1 spec, drivers at the SHA tagged for the benchmark release

Pin exact versions in a `BENCHMARK_VERSIONS.md` and commit a lockfile per language. Re-run on version bumps with explicit "previous version" / "current version" columns.

**3.4 Compiler flags:**
- C: `-O3 -march=native -flto` and a second run with `-O2 -march=x86-64-v3` for portability comparison
- Go: standard, with `GOAMD64=v3` documented if used
- Rust: `--release` with LTO enabled, document any non-default codegen options

---

## 4. Hardware and environment

**4.1 Primary platform:** Single bare-metal Linux box. Specify:
- CPU model (e.g. AMD Ryzen 9 7950X or Intel Xeon Gold 6338)
- RAM type and speed
- Storage: NVMe with specified model and queue depth
- Kernel version, glibc version
- CPU governor pinned to performance, turbo disabled, hyperthreading documented

**4.2 Secondary platform:** Apple M-series for parity with existing BENCHMARK.md numbers and for the ARM/NEON comparison NXS-simd-guard will eventually need.

**4.3 Cloud platform (optional, third tier):** One AWS/GCP instance type so readers can reproduce without local hardware. `c7i.4xlarge` or equivalent.

**4.4 Isolation:**
- No background processes (documented `systemctl` shutdown list)
- File caches dropped between cold-open runs (`echo 3 > /proc/sys/vm/drop_caches`)
- 100-run warmup, 1000-run measurement, IQR-trimmed median reported

---

## 5. Harness architecture

**5.1 Repository layout:**
```
nyxis/bench/
  README.md                # Methodology, principles, how to reproduce
  schemas/
    sparse.proto           # Workload A — Protobuf
    sparse.fbs             # Workload A — FlatBuffers
    sparse.capnp           # Workload A — Cap'n Proto
    sparse.nxs             # Workload A — Nyxis
    flat8.{proto,fbs,capnp,nxs}     # Workload B
    dense8.{proto,fbs,capnp,nxs,arrow}  # Workload C
  generators/
    gen.py                 # One canonical generator, emits JSON
    transcode_*.{c,go,rs}  # Per-format transcoders from JSON
  harness/
    c/  go/  rust/         # Per-language harness, identical structure
  results/
    YYYY-MM-DD_<hardware>/ # Each run gets its own directory
      raw/                 # All raw timing data
      summary.json         # Aggregated statistics
      methodology.md       # Frozen methodology for this run
  scripts/
    run_all.sh
    drop_caches.sh
    report.py              # Generates published tables from results/
```

**5.2 Harness contract:** Each harness binary takes `--workload {A,B,C}`, `--format {proto,fb,capnp,nxs,arrow}`, `--population <float>` (Workload A), `--records <int>`, `--metric {open,access,scan,size}`, and emits a JSON line per measurement run. No format-specific shortcuts — same CLI across all four.

**5.3 Measurement primitive:** `clock_gettime(CLOCK_MONOTONIC_RAW)` in C, equivalents in Go/Rust. Document the resolution and the cost of the timing call itself.

**5.4 No timing tricks:** No measuring "open" by reading only the header when the equivalent in another format would have to do more. If FlatBuffers can't be considered "open" until the root table is accessed, that's what gets measured for both. Define "open" once, apply uniformly.

---

## 6. What gets published

**6.1 Required outputs per workload:**
- Raw measurement data (CSV) in `results/<date>/raw/`
- Summary JSON with P50, P99, IQR, sample count
- A methodology document frozen at run time
- A short prose section in BENCHMARK.md interpreting the result

**6.2 BENCHMARK.md structure additions:**
- Add `## Workload A: Sparse hierarchical records` section with results table
- Add `## Workload B: Cold-open random access` section with results table
- Add `## Workload C: Dense uniform analytical reducer` section, including Arrow, with prose explicitly stating "Nyxis is the slowest zero-copy format here; this workload is what the Arrow bridge is for."
- Add `## Reproducing these benchmarks` with the exact commands

**6.3 What does *not* get published:**
- No qualitative-label matrices ("Heavy," "Moderate," "Bloated"). Tables contain numbers with units, or they don't exist.
- No sweep matrices where one format wins every row.
- No comparison labels like "Protobuf is heavy" without a measured deserialization time supporting the word.

**6.4 The use-cases page update (Act II):**
- Replace the "currently compiling" paragraph with a link to BENCHMARK.md and a 2-3 sentence summary: which workload Nyxis wins, which it ties, which it loses.
- Keep the qualitative architectural comparison bullets — they're complementary to the numbers, not a replacement.

---

## 7. Review gates

Before any number is published:

**7.1 Methodology review.** Post the frozen methodology document for each workload to the repo and request review from a contributor of each competing format (FlatBuffers, Cap'n Proto, Protobuf, Arrow). Wait at least 7 days. Incorporate or explicitly rebut feedback in the doc.

**7.2 Adversarial review.** Ask one person on each side: "Where is this benchmark unfair to your format?" Document the answers and either fix the harness or explain in the methodology why the choice stands.

**7.3 Re-run by a third party.** Before publication, at least one person who is not the benchmark author runs `make bench` on their own hardware and confirms the relative ordering holds. Differences in absolute numbers are expected and fine.

**7.4 Loss disclosure check.** Before publishing, verify that the published results include at least one Nyxis loss. If every workload is a win, something is wrong with the workload selection — go back to section 2.

---

## 8. Timeline (rough)

These are budget estimates, not commitments. Adjust to actual capacity.

| Phase | Work | Estimate |
|---|---|---|
| 1 | Repo scaffold, schemas, generator | 1 week |
| 2 | C harness for all 4 formats, Workload B | 2 weeks |
| 3 | Workload A (sparse) implementation and runs | 1 week |
| 4 | Workload C (dense + Arrow) implementation and runs | 1 week |
| 5 | Go and Rust harness ports | 2 weeks |
| 6 | Methodology review window | 1 week (overlap with Phase 5) |
| 7 | Adversarial review and fixes | 1-2 weeks |
| 8 | Third-party reproduction | 1 week |
| 9 | BENCHMARK.md writeup, use-cases page update, launch | 1 week |

Total: **9-11 weeks** from start to public release if done seriously. Compressing below 6 weeks means cutting either Go/Rust ports, the review window, or the third-party reproduction — all three are how you avoid the "looks too good to be true" trap.

---

## 9. Definition of done

The benchmark suite ships when:

1. All three workloads have published results across at least C, Go, and Rust on the primary platform
2. BENCHMARK.md includes the new workload sections with reproducibility instructions
3. At least one workload publishes a Nyxis loss, transparently and without hedging
4. A contributor to at least one competing format has publicly reviewed the methodology (a GitHub issue or PR comment counts)
5. A third party has reproduced the relative ordering on independent hardware
6. The `nyxis/bench/` directory contains every harness, schema, and run script referenced in BENCHMARK.md, with no broken links
7. The use-cases page Act II "currently compiling" line is replaced with the actual results

---

## 10. Open questions

These need a decision before Phase 1 starts:

- **Q1:** Does the benchmark use the BSL-licensed Nyxis compiler or only the MIT-licensed drivers? Affects who can re-run benchmarks under the $5M / 10 TB threshold.
- **Q2:** Does Workload C include Parquet as well as Arrow IPC? Parquet is the on-disk counterpart and might be the more common comparator.
- **Q3:** What's the policy if a re-run on a future format version (Protobuf 6, FlatBuffers 2.x) changes the ordering? Keep historical results and publish a "current version" addendum, or replace?
- **Q4:** Who's the third-party reviewer in section 7.3? Need someone identified before Phase 1 ends, ideally from outside the project.

---

**End of spec.**

The single most important sentence in this document is in section 7.4. If you publish a sweep, you undo the trust the page just earned. If you publish honest results including a loss, you become the rare new format that engineers can take seriously on day one.
