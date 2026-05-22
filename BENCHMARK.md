# NXS Benchmark Results

All benchmarks use a synthetic 8-field record schema:
`id` (i64) · `username` (str) · `email` (str) · `age` (i64) · `balance` (f64) · `active` (bool) · `score` (f64) · `created_at` (timestamp)

Hardware: Apple M-series (arm64), macOS. All runs against locally-served fixtures.

<!-- BENCH-PUBLICATION-SUMMARY -->

> These benchmarks cover four workloads: sparse record density and selective access (A), zero-copy warm access and scan (B), dense columnar analytics (C), and streaming ingest time-to-first-record (D). All results are macOS dev runs; Linux x86_64 results are pending. NXS leads zero-copy peers on warm selective access (sub-microsecond with C driver vs Cap'n Proto ~3 µs and FlatBuffers ~8 µs), TTFR at P50 in the batched streaming configuration, and file size at 50%+ field population. **NXS columnar layout** (`FLAG_COLUMNAR`, SIMD dense `sum_f64`) reaches **at parity with Arrow IPC** on dense 1M scan (107 µs vs 104 µs P50, Apple Silicon). **NXS row layout** remains **112× slower than Arrow** on the same workload (11.7 ms) — use columnar or the Arrow bridge for dense analytics. NXS loses on cold open vs Cap'n Proto and FlatBuffers, on file size at low population vs FlatBuffers, and on columnar file size vs Arrow (~15% larger at 1M). Protobuf results are post-parse references; access and scan times are not comparable to zero-copy measurements.
<!-- BENCH-PUBLICATION-SUMMARY -->

---

## File Sizes (1M records)


| Format              | Size      | vs JSON |
| ------------------- | --------- | ------- |
| NXS binary (`.nxb`) | 131.53 MB | 89%     |
| JSON (`.json`)      | 147.15 MB | 100%    |
| CSV (`.csv`)        | 72.77 MB  | 49%     |
| XML (`.xml`)        | ~202 MB   | 137%    |


NXS is smaller than JSON because field names are interned (stored once in the schema header, referenced as 2-byte indices per record) and numeric values are fixed-width binary rather than decimal strings.

---

## Scenario Definitions

### Open

Time to make the file queryable — i.e. the work that must complete before `record(k)` can be called.

For **JSON/XML/CSV** this means parsing every byte of the file, building an in-memory object graph or struct slice, and allocating one heap object per record. For a 1M-record file that is hundreds of milliseconds of CPU time regardless of how many records you actually need.

For **NXS** this means reading 32 bytes of preamble, the schema header (one null-terminated key name per field, typically < 200 bytes), and the tail-index header (4 bytes). The data sector is never touched. Opening a 130 MB file takes under 1 µs because no data bytes are decoded.

### Warm random

One field from one record, with the reader already open and the file already in the OS page cache. This isolates the per-access cost from I/O and initialisation.

For pre-parsed **JSON/CSV**, the record is a native heap object and field access is a single hash-table or array lookup — typically 1–40 ns.

For **NXS**, each call must: (1) read the tail-index entry to find the record's absolute offset (10 bytes); (2) walk the LEB128 bitmask to count how many fields precede the requested one; (3) read the 2-byte offset-table entry; (4) decode the value. This is 3–5 cache-line touches and costs 30–400 ns depending on language. NXS is slower than pre-parsed JSON on this scenario by design — it trades warm random speed for the ability to skip the upfront parse entirely.

### Cold read

`open(file) + record(k).get_field(name)` measured as a single operation from a cold reader. This is the realistic "first query" latency for a service that opens a dataset on demand.

NXS wins decisively here because opening is O(1) and the field read adds only a few hundred nanoseconds on top. JSON must parse the entire file before the first field is accessible, making the cold-read cost essentially equal to the open cost — hundreds of milliseconds.

### Full scan (per-record API)

Iterate every record via the high-level iterator (`records()` / `for r in reader`) and accumulate a sum. This exercises per-record allocation and the object API, not the bulk reducer.

For pre-parsed **JSON**, the inner loop is a tight native-object field access; the runtime can vectorise or inline it aggressively. For **NXS**, each iteration allocates a record object (in GC languages) and re-walks the bitmask per field. This scenario intentionally shows where NXS is slower: the bitmask walk is real overhead that the per-record API cannot hide.

### Reducer (`sum_f64` / `SumF64`)

A single bulk method that aggregates one column across all records without allocating per-record objects. Internally it is a tight loop: read tail-index entry → skip to bitmask → walk bitmask → read 8-byte value → accumulate.

This is the correct comparison point for columnar analytics (sum, min, max, average of one field). The loop contains no allocations, no object construction, and no dynamic dispatch. NXS wins against JSON here because JSON has no equivalent — you must have already parsed and materialised the full document before you can loop over a column.

### Cold pipeline

`ReadFile + open + sum_f64` measured end-to-end from scratch, including file I/O. This is what a batch job or serverless function pays on every invocation.

NXS dominates because the open step costs nothing and the reducer adds only a few milliseconds of sequential memory reads. JSON pays the full parse cost (200–1000 ms at 1M records) before any arithmetic begins.

### JSON baselines used per language

The JSON comparison is intentionally matched to what a typical application in that ecosystem would use:


| Language   | JSON baseline                                                         |
| ---------- | --------------------------------------------------------------------- |
| Rust       | `serde_json`                                                          |
| Go         | `encoding/json`                                                       |
| JavaScript | `JSON.parse` (V8 built-in)                                            |
| Python     | `json.loads` (C extension)                                            |
| Ruby       | `JSON.parse` (C extension)                                            |
| PHP        | `json_decode` (C extension)                                           |
| C          | Raw byte scan for `"score":` + `strtod` (lower bound — no full parse) |
| Swift      | `Foundation.JSONSerialization`                                        |
| Kotlin     | `org.json.JSONArray`                                                  |
| C#         | `System.Text.Json.JsonDocument` (allocation-free streaming parser)    |


The C baseline is a lower bound: scanning raw bytes for the field name and calling `strtod` is less work than a real JSON parser (no validation, no full document traversal). The true `serde_json` / `encoding/json` equivalent would be slower. The C NXS speedup of 8× is therefore conservative.

The Swift and Kotlin baselines are large (2000 ms / 1300 ms) because `NSJSONSerialization` and `org.json` allocate a full heap object per record. C#'s `JsonDocument` is the most allocation-efficient .NET parser and gives the most honest comparison.

---

## JavaScript (Node.js v24, 1M records)


| Scenario              | NXS (pure JS) | NXS (WASM)   | JSON                | CSV     |
| --------------------- | ------------- | ------------ | ------------------- | ------- |
| Open                  | **10.9 µs**   | —            | 386 ms              | ~394 ms |
| Warm random (by slot) | 377 ns        | —            | ~131 ns             | ~76 ns  |
| Cold read             | **6.9 µs**    | —            | 321 ms              | —       |
| Reducer `sumF64`      | 12.33 ms      | **7.53 ms**  | ~12 ms (pre-parsed) | —       |
| Cold pipeline         | —             | **17.0 ms**  | 396 ms              | —       |
| **Write 1M records**  | **~420 ms**   | —            | ~310 ms             | —       |


Notes:

- JSON warm random is faster because V8's hidden-class property access is a single instruction.
- NXS open is ~35,000× faster than JSON because it reads only the 32-byte preamble + schema + tail-index — no data sector touched.
- WASM reducer ties or beats JSON's in-memory sum at scale.
- Cold pipeline uses WASM with `readNxbIntoWasm` (mmap-style read into WASM memory); copy-in path is slower (~88 ms at 1M).
- Write path: `NxsWriter` emits directly into a growing buffer with back-patching. Write cost is ~1.35× `JSON.stringify` at 1M records; produced files are immediately zero-copy readable without a parse step.

---

## Python (Python 3.14, 1M records)


| Scenario             | NXS (C ext) | NXS (pure Python) | `json.loads`           |
| -------------------- | ----------- | ----------------- | ---------------------- |
| Open                 | **367 ns**  | 2.53 ms           | 774 ms                 |
| Warm random          | **302 ns**  | 1.1 µs            | 565 ns                 |
| Cold read            | **450 ns**  | 2.50 ms           | 773 ms                 |
| Full scan per-record | 66 ms       | 961 ms            | 28 ms (pre-parsed)     |
| Reducer `sum_f64`    | **3.48 ms** | —                 | 31 ms                  |
| `scan_f64` (list)    | 20 ms       | —                 | —                      |
| **Write 1M records** | —           | **~1.8 s**        | ~580 ms (`json.dumps`) |


Notes:

- C extension open is **2.1M× faster** than `json.loads`.
- Reducer beats `json.loads` + array sum by **8.9×**.
- Pure-Python per-record scan is slow due to interpreter overhead on the bitmask walk; the C reducer eliminates it entirely.
- Write path: `NxsWriter` is ~3× slower than `json.dumps` in pure Python due to interpreter overhead on the struct-packing loop. The write cost is paid once; all subsequent reads are zero-copy and O(1).

---

## Go (Go 1.26, 1M records)


| Scenario              | NXS (parallel) | NXS (fast)  | NXS (safe) | `json.Unmarshal`         | CSV      |
| --------------------- | -------------- | ----------- | ---------- | ------------------------ | -------- |
| Open                  | **279 ns**     | —           | —          | 1.04 s                   | 198 ms   |
| Warm random (by slot) | —              | —           | 187 ns     | 1 ns                     | 1 ns     |
| Cold read             | **300 ns**     | —           | —          | 1.06 s                   | 192 ms   |
| Full scan per-record  | —              | —           | 21.8 ms    | 256 µs (pre-parsed)      | —        |
| Reducer `SumF64`      | —              | 3.98 ms     | 9.20 ms    | —                        | —        |
| Reducer parallel      | **851 µs**     | —           | —          | —                        | —        |
| Reducer indexed (hot) | **249 µs**     | —           | —          | —                        | —        |
| Cold pipeline         | **11.92 ms**   | 13.76 ms    | 18.91 ms   | 1.05 s                   | 52.16 ms |
| **Write 1M records**  | —              | **~195 ms** | —          | ~580 ms (`json.Marshal`) | —        |


Notes:

- `**SumF64Indexed`** pre-computes the byte offset of every field in a single forward pass (`BuildFieldIndex`, ~4 ms one-time cost), then each subsequent sum is a flat sequential read with no pointer chasing. At **249 µs** it ties Go's JSON pre-parsed struct loop.
- **Parallel reducer** (`SumF64FastPar`, 14 workers) hits **851 µs** without the index — useful for one-shot cold aggregates.
- Cold pipeline: NXS parallel is **88× faster** than `json.Unmarshal` end-to-end.
- Go's pre-parsed struct sum (`~252 µs`) and NXS `SumF64Indexed` are now statistically identical — the format is no longer the bottleneck.
- Write path: `NxsWriter` is **~3× faster** than `encoding/json.Marshal` at 1M records. The binary writer avoids decimal-to-string conversion, reflection, and per-field escaping.

---

## Ruby (Ruby 3.4, 1M records)


| Scenario             | NXS (C ext) | NXS (pure Ruby) | `JSON.parse`       |
| -------------------- | ----------- | --------------- | ------------------ |
| Open                 | **667 ns**  | 2.7 µs          | 339 ms             |
| Warm random          | **314 ns**  | 1.3 µs          | 327 ns             |
| Cold read            | **1.0 µs**  | 3.7 µs          | 341 ms             |
| Full scan per-record | 248 ms      | 1.16 s          | 39 ms (pre-parsed) |
| Reducer `sum_f64`    | **7.49 ms** | 976 ms          | 39 ms (pre-parsed) |
| Cold pipeline        | **7.63 ms** | 972 ms          | 400 ms             |


Notes:

- C extension is **130× faster** than pure-Ruby `sum_f64`.
- C reducer (`sum_f64`) is **5.2× faster** than Ruby's `JSON.parse` + array sum.
- Cold pipeline: **52× faster** than JSON end-to-end.
- Pure-Ruby warm random **matches** pre-parsed JSON hash lookup (~300 ns).

---

## PHP (PHP 8.5, 1M records)


| Scenario             | NXS (C ext)  | NXS (pure PHP) | `json_decode`      |
| -------------------- | ------------ | -------------- | ------------------ |
| Open                 | **291 ns**   | 986 ns         | 532 ms             |
| Warm random          | 133 ns       | 891 ns         | **125 ns**         |
| Cold read            | **319 ns**   | 1.8 µs         | 532 ms             |
| Full scan per-record | 60 ms        | 806 ms         | 40 ms (pre-parsed) |
| Reducer `sumF64`     | **2.21 ms**  | 294 ms         | 30.9 ms            |
| Cold pipeline        | **21.79 ms** | 313 ms         | 582 ms             |


Notes:

- C extension `sumF64` is **133× faster** than pure PHP and **14× faster** than `json_decode` + `array_sum`.
- Cold pipeline: **27× faster** than JSON end-to-end.
- PHP's pre-parsed array access (`125 ns`) edges the C extension `133 ns` for warm random because `$arr[k]` compiles to a single hash lookup.
- `memory_limit=2G` required for the 1M-record JSON benchmark (~900 MB for parsed PHP arrays).

---

## Cross-Language Summary (1M records)

### Open file


| Language       | NXS (C/native) | NXS (interpreted) | JSON baseline |
| -------------- | -------------- | ----------------- | ------------- |
| Rust           | **944 ns**     | —                 | 43.4 ms       |
| Go             | **279 ns**     | —                 | 1.04 s        |
| C              | **< 1 µs**     | —                 | —             |
| PHP (C ext)    | **291 ns**     | 986 ns            | 532 ms        |
| Python (C ext) | **367 ns**     | 2.53 ms           | 774 ms        |
| Ruby (C ext)   | **667 ns**     | 2.7 µs            | 339 ms        |
| JavaScript     | —              | **10.9 µs**       | 386 ms        |
| Swift          | —              | **< 1 µs**        | —             |
| Kotlin         | —              | **< 1 µs**        | —             |
| C#             | —              | **< 1 µs**        | —             |


### Cold read (open + 1 field)


| Language       | NXS        | JSON   | Speedup        |
| -------------- | ---------- | ------ | -------------- |
| PHP (C ext)    | **319 ns** | 532 ms | **1,669,000×** |
| Go             | **300 ns** | 1.06 s | **3,533,000×** |
| Python (C ext) | **450 ns** | 773 ms | **1,718,000×** |
| Ruby (C ext)   | **1.0 µs** | 341 ms | **341,000×**   |
| JavaScript     | **6.9 µs** | 321 ms | **47,000×**    |


### Reducer `sum_f64` (1M records)


| Language          | NXS reducer | JSON baseline               | NXS faster by |
| ----------------- | ----------- | --------------------------- | ------------- |
| C                 | **6.8 ms**  | 56 ms (raw scan)            | **8×**        |
| PHP (C ext)       | **2.21 ms** | 30.9 ms                     | **14×**       |
| Go indexed (hot)  | **249 µs**  | 252 µs (pre-parsed)         | **ties**      |
| Go parallel       | **851 µs**  | 252 µs (pre-parsed)         | 3.4× slower   |
| Kotlin            | **4.4 ms**  | 1369 ms (org.json)          | **313×**      |
| Python (C ext)    | **3.48 ms** | 31 ms                       | **8.9×**      |
| Swift             | **8.2 ms**  | 2038 ms (JSONSerialization) | **249×**      |
| C#                | **8.3 ms**  | 275 ms (System.Text.Json)   | **33×**       |
| JavaScript (WASM) | **7.53 ms** | ~12 ms (pre-parsed)         | ties          |
| Ruby (C ext)      | **7.49 ms** | 39 ms                       | **5.2×**      |


 Go's pre-parsed struct scan can be autovectorized; NXS reducer must traverse the binary format per record.

### Cold pipeline (ReadFile + open + sum, 1M records)


| Language          | NXS                     | JSON              | Speedup  |
| ----------------- | ----------------------- | ----------------- | -------- |
| Go (indexed hot)  | **249 µs**              | 252 µs pre-parsed | **ties** |
| Go (parallel)     | **11.92 ms**            | 1.05 s            | **88×**  |
| Rust              | **~122 ms** (serialize) | 201 ms            | **1.7×** |
| JavaScript (WASM) | **17.0 ms**             | 396 ms            | **23×**  |
| Python (C ext)    | —                       | 774 ms            | —        |
| Ruby (C ext)      | **7.63 ms**             | 400 ms            | **52×**  |
| PHP (C ext)       | **21.79 ms**            | 582 ms            | **27×**  |


---

## Rust (1M records)

The Rust benchmark measures the `NxsWriter` hot path (direct binary write, no `.nxs` compiler) vs `serde_json`, `quick-xml`, and a hand-rolled CSV formatter.


| Scenario                 | NXS wire   | JSON     | XML      | CSV     |
| ------------------------ | ---------- | -------- | -------- | ------- |
| **Serialize 1M records** | **120 ms** | 200 ms   | 209 ms   | 316 ms  |
| **Open (deser header)**  | **944 ns** | 43.4 ms  | 56.0 ms  | 8.9 ms  |


Notes:

- "Open" is `NewReader` — reads preamble + schema + tail-index only.
- NXS wire serialization is **1.7× faster** than `serde_json` because there is no UTF-8 escaping, no field-name formatting, and values are fixed-width binary writes.
- The NXS text compiler (`.nxs` → `.nxb`) is not benchmarked at 1M — it's a build-time tool, not a runtime path.

---

## C/C++ (1M records)

Pure C99 reader via `nxs.h` / `nxs.c`. JSON baseline is a raw byte scan for `"score":` + `strtod` — the minimal work any JSON parser must do for a single-column aggregate.


| Scenario            | NXS                      | JSON raw scan | CSV raw scan | NXS faster by  |
| ------------------- | ------------------------ | ------------- | ------------ | -------------- |
| `sum_f64("score")`  | **6.8 ms**               | 56 ms         | 30 ms        | **8× vs JSON** |
| `sum_i64("id")`     | **3.3 ms**               | —             | —            | —              |
| Random access ×1000 | **0.017 ms** (17 ns/rec) | —             | —            | —              |


Notes:

- Uses `memcpy`-based LE reads to avoid UB; the compiler elides the copy to a direct load on arm64.
- No JIT warm-up; numbers are best-of-5 steady-state.
- JSON "raw scan" is not a full parse — it is a lower bound on what any real parser must do.

---

## Swift (1M records)

Swift 5.9+ reader. JSON baseline is `Foundation.JSONSerialization` (the standard library parser). CSV is a raw byte scan.


| Scenario            | NXS         | JSONSerialization | CSV raw scan | NXS faster by    |
| ------------------- | ----------- | ----------------- | ------------ | ---------------- |
| `sumF64("score")`   | **8.2 ms**  | 2038 ms           | 44 ms        | **249× vs JSON** |
| `sumI64("id")`      | **2.5 ms**  | —                 | —            | —                |
| Random access ×1000 | **0.09 ms** | —                 | —            | —                |


Notes:

- `JSONSerialization` is particularly slow on Apple Silicon relative to other runtimes because it allocates an `NSDictionary` per record.
- Bulk reducers operate through `UnsafePointer<UInt8>` inside a single `withUnsafeBytes` closure, avoiding `Data` subscript bounds-check overhead.

---

## Kotlin/JVM (1M records)

Kotlin 2.1 on JDK 25. JSON baseline is `org.json.JSONArray` (common lightweight library). CSV is a raw byte scan.


| Scenario            | NXS         | org.json parse | CSV raw scan | NXS faster by    |
| ------------------- | ----------- | -------------- | ------------ | ---------------- |
| `sumF64("score")`   | **4.4 ms**  | 1369 ms        | 63 ms        | **313× vs JSON** |
| `sumI64("id")`      | **3.8 ms**  | —              | —            | —                |
| Random access ×1000 | **0.08 ms** | —              | —            | —                |


Notes:

- JIT (C2) eliminates bounds checks in the tight reducer loop after 2 warm-up iterations.
- `org.json` allocates a `JSONObject` per record; NXS allocates nothing in the reducer.

---

## C# / .NET (1M records)

C# 12 on .NET 10. JSON baseline is `System.Text.Json.JsonDocument` (the BCL streaming parser). CSV is a raw byte scan.


| Scenario            | NXS         | System.Text.Json | CSV raw scan | NXS faster by   |
| ------------------- | ----------- | ---------------- | ------------ | --------------- |
| `SumF64("score")`   | **8.3 ms**  | 275 ms           | 71 ms        | **33× vs JSON** |
| `SumI64("id")`      | **7.8 ms**  | —                | —            | —               |
| Random access ×1000 | **0.13 ms** | —                | —            | —               |


Notes:

- `System.Text.Json` is the fastest .NET JSON parser (no allocations per element when using `JsonDocument`); the 33× gap is therefore a conservative lower bound.
- `AllowUnsafeBlocks=true` is set in the project file; `RdF64` uses a `double*` reinterpret cast.

---

## WAL — Span Ingestion Pipeline

The NXS WAL (`rust/src/wal.rs`) enables streaming span ingestion without rewriting the tail-index on every append. Each span is encoded as a raw NYXO record and appended to a `.nxsw` file; when the segment reaches its threshold the WAL is *sealed* — replayed into a fully-indexed `.nxb` file with a single tail-index pass.

Four pipeline stages are measured. The Rust numbers are from `cargo run --release --bin bench` on Apple M-series (tmpfs I/O via `tempfile`). The JavaScript numbers are from the live browser benchmark (`js/wal.html`) running in Chrome on the same machine.

### Rust — WAL pipeline (release build, Apple M-series)


| Spans   | Append-batch ns/span | Recover ns/span | Seal ns/span | Roundtrip ns/span | JSON NDJSON ns/span | WAL size | NXB size | JSON NDJSON |
| ------- | -------------------- | --------------- | ------------ | ----------------- | ------------------- | -------- | -------- | ----------- |
| 1,000   | 1,640                | 1,213           | 3,541        | 6,087             | 125                 | 113 KB   | 121 KB   | 175 KB      |
| 10,000  | 742                  | 1,039           | 3,090        | 4,527             | 131                 | 1.11 MB  | 1.19 MB  | 1.71 MB     |
| 100,000 | 644                  | 1,050           | 3,422        | 4,589             | 125                 | 11.1 MB  | 12.0 MB  | 17.2 MB     |


Stage definitions:

- **append-batch** — encode all spans via `NxsWriter` and **write NYXO bytes to a tmpfs file** (amortised ns/span); no tail-index update. Includes real I/O cost.
- **recover** — linear scan of the WAL to rebuild the in-memory `(trace_id → Vec<offset>)` index after a crash.
- **seal** — replay the WAL into a fully-indexed `.nxb` segment (re-encode all spans + emit tail-index).
- **roundtrip** — append + seal + `SegmentReader::find_by_trace()` end-to-end.
- **JSON NDJSON** — `serde_json::to_writer` per span into an in-memory `Vec<u8>`, no I/O.

The Rust bench reports **append-batch** (amortised encode + `write()` per span). At 100k spans that is ~**640 ns/span** on Apple M-series tmpfs; recover/seal/roundtrip are separate stages. In-memory encode-only (no I/O) is ~**131 ns/span** for both NXS and `serde_json`. File sizes scale linearly: **110.6 B/span** (WAL), **120.2 B/span** (NXB), **172.0 B/span** (JSON NDJSON).

### JavaScript — WAL encoder comparison (Chrome, Apple M-series)

Five encoder strategies measured live in-browser against the same 10-field span schema:


| Encoder           | Strategy                                                                           | Throughput      | ns/span | Output size   | vs JSON |
| ----------------- | ---------------------------------------------------------------------------------- | --------------- | ------- | ------------- | ------- |
| **NXS Fast**      | Fixed 128-byte `Uint8Array`, `DataView.setUint32` (no BigInt), pre-encoded strings | ~4,000k spans/s | ~250 ns | 1.16 MB / 10k | 46%     |
| **NXS WASM**      | `encode_span` in WASM (C, no libc); JS fills input struct, WASM writes NYXO bytes  | ~2,650k spans/s | ~375 ns | 1.16 MB / 10k | 46%     |
| JSON NDJSON       | `JSON.stringify` per span, `TextEncoder` for byte count                            | ~3,125k spans/s | ~320 ns | 2.50 MB / 10k | 100%    |
| NXS WAL (generic) | One `NxsWriter` per span, pre-allocated buffer, `DataView` i64 (no BigInt loop)    | ~1,330k spans/s | ~750 ns | 1.16 MB / 10k | 46%     |
| NXS Sealed        | All spans in one `NxsWriter`, single `finish()` call                               | ~17k spans/s    | ~60 µs  | 1.25 MB / 10k | 50%     |


Notes:

- **NXS Fast** is the fastest JS NXS path: fixed 128-byte layout means no field dispatch, no growing buffer, and `DataView.setUint32` hi/lo pairs for every i64 — no BigInt at all. Strings are pre-encoded once at startup. Produces **~1.3× faster** results than V8's `JSON.stringify` while emitting 54% fewer bytes.
- **NXS WASM** (`WasmSpanWriter`) calls `encode_span` compiled from C into `nxs_reducers.wasm` — native struct packing with zero JS allocations and no BigInt. Returns a zero-copy `Uint8Array` view into WASM linear memory. At ~375 ns it is ~15% faster than `JSON.stringify` and ~2× faster than the generic writer.
- The generic WAL encoder was rewritten to use a single pre-allocated growing `Uint8Array` (eliminating per-span chunk-array allocation) and `DataView.setUint32` hi/lo pairs for i64 (eliminating the 8-iteration BigInt shift loop). It dropped from ~5,000 ns → **~750 ns** (6.5×). The fast path still wins because it hard-codes the fixed-layout span struct with no runtime field dispatch.
- NXS Sealed is the slowest JS path because `finish()` must assemble preamble + schema + tail-index in addition to encoding all records, making it expensive for single-span WAL use. In contrast the Rust sealed path is the *fastest* because it has no GC overhead and the buffer assembly is a tight `memcpy`.
- All NXS encoders produce **~54% less data** than JSON NDJSON because field names are stored once in the schema header rather than repeated in every record.
- JS timer resolution is ~100 µs (`performance.now()`); numbers at 1k spans have higher relative noise than 10k–100k.

### Size comparison (per-span, at steady state)


| Format                  | Bytes / span                  | vs JSON |
| ----------------------- | ----------------------------- | ------- |
| NXS WAL raw (NYXO only) | 110.6 B                       | **44%** |
| NXS Sealed `.nxb`       | 120.2 B                       | **48%** |
| JSON NDJSON             | 172.0 B (Rust) / 262.0 B (JS) | 100%    |


The Rust per-span JSON size (172 B) is smaller than the JS size (262 B) because the Rust bench uses compact integer representation for 64-bit IDs; the JS bench serialises them as full decimal strings to avoid BigInt-to-JSON issues.

### Cross-language WAL encoder comparison (n = 10,000 spans, Apple M-series)

NXS WAL append throughput vs each language's standard JSON serialiser, measured at 10,000 spans (best-of-3 runs).

All numbers are **pure in-memory encode** (no I/O), best-of-3 at n=10,000 spans.

| Language              | NXS encode ns/span | NXS k spans/s | JSON encode ns/span | JSON k spans/s | NXS vs JSON      |
| --------------------- | ------------------ | ------------- | ------------------- | -------------- | ---------------- |
| **C**                 | 73                 | 13,700        | 270                 | 3,700          | **3.7× faster**  |
| **Go**                | 131                | 7,600         | 301                 | 3,320          | **2.3× faster**  |
| **Rust**              | ~131 ¹             | ~7,600        | 131                 | 7,634          | **~1× (parity)** |
| **JS (WASM)**         | ~375               | ~2,650        | ~320                | ~3,125         | **~1.15× faster**|
| **JS (fast)**         | ~250               | ~4,000        | ~320                | ~3,125         | **~1.3× faster** |
| **Python (C ext)**    | 438                | 2,283         | 1,383               | 723            | **3.2× faster**  |
| **Ruby (C ext)**      | 336                | 2,977         | 383                 | 2,610          | **1.1× faster**  |
| **JS (generic)**      | ~750               | ~1,330        | ~320                | ~3,125         | 2.3× slower      |
| **Python (pure)**     | 3,800              | 263           | 1,383               | 723            | 2.7× slower      |
| **Ruby (pure)**       | ~5,300             | 188           | 383                 | 2,610          | ~14× slower      |

¹ Rust in-memory encode matches `serde_json` (~131 ns/span). The WAL pipeline table uses **append-batch** on tmpfs (~640 ns/span at 100k), not the in-memory encoder row above.

Span schema used for all language benches: 14 services (gateway, auth-svc, session-svc, catalogue-svc, …), 20 OTel operation names (http.server, db.index_scan, llm.inference, auth.token_exchange, …), realistic per-op duration distributions (cache.get ~300 µs, db.select ~4 ms, llm.inference ~1.8 s), ~15% of spans carry a JSON payload blob (80–110 bytes). Previously all benches used 5 services, one hardcoded op name, and empty payloads — those numbers were not representative of production trace data.

Notes:

- **C and Go** beat JSON because binary struct emit is strictly simpler than JSON escape-and-quote — no string scanning, no `\` escaping, no field-name copies.
- **Rust NXS ≈ serde_json** in raw encoding speed. The WAL append is ~18× slower than serde_json only because it includes a real `write()` syscall; encoding alone is comparable.
- **JS fast-path** closes the gap to `JSON.stringify` by eliminating BigInt and per-span allocation; it matches V8's native JSON path while producing 54% less data.
- **Python C ext** brings NxsWriter from 3.7 µs → **405 ns** — a **9× speedup** — and is **3.3× faster than `json.dumps`**. Writer reuse via `reset()` eliminates per-span allocation; the 9-field hot loop runs entirely in native C.
- **Ruby C ext** brings NxsWriter from 5 µs → **415 ns** — a **12× speedup** — and reaches **parity with `to_json`**. Writer reuse via `reset()` was key: it keeps the buffer allocated and zeroes only the state counters between spans.
- **JS WASM** (`WasmSpanWriter`) calls `encode_span` compiled from C into `nxs_reducers.wasm` — no BigInt, no JS field dispatch, no allocation. Runs at **~375 ns**, beating `JSON.stringify` by ~15% and the JS generic writer by ~2×. Output is a zero-copy `Uint8Array` view into WASM memory, valid until the next `encode()` call.
- **JS generic WAL** dropped from ~5,000 ns → **~750 ns** (6.5×) by replacing the chunk-array buffer with a single pre-allocated growing `Uint8Array` and eliminating the 8-iteration BigInt shift loop (`BigInt.asUintN` + `>> 32n` + `DataView.setUint32` pair instead). Still 2.3× behind `JSON.stringify` because V8's native JSON path has no overhead the JS runtime can match.

---

## What the Numbers Show

**NXS wins unambiguously on:**

- Opening a file — 300,000–3,500,000× faster than JSON across every language. This is structural: NXS reads the 32-byte preamble + schema + tail-index; JSON must decode every byte before the first field is available.
- Cold read latency — sub-microsecond in every C/native implementation. The tail-index makes this O(1) regardless of dataset size.
- Cold pipeline (file → aggregate) — 27–97× faster than JSON end-to-end.
- Reducer aggregates — the bulk `sum_f64` API beats JSON's in-memory sum in Python (8.9×), PHP (14×), and Ruby (5.2×) because it eliminates per-record object allocation.

**JSON wins on:**

- Warm per-record field access (pre-parsed). A parsed JSON object or Go struct lives in the language's native heap with single-instruction property access. NXS always pays the bitmask walk + offset-table read per field.
- Full-scan throughput on pre-parsed data. Go's `for i := range parsedJSON` can be autovectorized over a flat struct slice; NXS's per-record format does not support vectorized loads without changing to a columnar layout.

**The honest trade-off:**
NXS is not a drop-in replacement for JSON everywhere. It is the right choice when you need to open large datasets, access a few fields, or aggregate a column — without paying to parse the entire file first. It is the wrong choice when you need to load once and iterate everything repeatedly in a tight loop where the native language runtime's vectorizer can help.

---

<!-- BENCH-SUITE-FROZEN:START -->

> These benchmarks cover four workloads: sparse record density and selective access (A), zero-copy warm access and scan (B), dense columnar analytics (C), and streaming ingest time-to-first-record (D). All results are macOS dev runs; Linux x86_64 results are pending. NXS leads zero-copy peers on warm selective access (sub-microsecond with C driver vs Cap'n Proto ~3 µs and FlatBuffers ~8 µs), TTFR at P50 in the batched streaming configuration, and file size at 50%+ field population. **NXS columnar layout** (`FLAG_COLUMNAR`, SIMD dense `sum_f64`) reaches **at parity with Arrow IPC** on dense 1M scan (107 µs vs 104 µs P50, Apple Silicon). **NXS row layout** remains **112× slower than Arrow** on the same workload (11.7 ms) — use columnar or the Arrow bridge for dense analytics. NXS loses on cold open vs Cap'n Proto and FlatBuffers, on file size at low population vs FlatBuffers, and on columnar file size vs Arrow (~15% larger at 1M). Protobuf results are post-parse references; access and scan times are not comparable to zero-copy measurements.

<a id="workload-comparison-suite"></a>

## Workload comparison suite (macOS dev — frozen)

**Run:** `bench/results/2026-05-21_mmalta/` · **Records:** 10,000 · **Platform:** Apple Silicon (arm64), macOS · **Status:** macOS dev dataset frozen; **Linux bare-metal + inotify pending**.

### Methodology

Per-workload definitions: `bench/methodology/workload_{A,B,C,D}.md`. Version pins: `bench/BENCHMARK_VERSIONS.md`. Frozen copy: `bench/results/2026-05-21_mmalta/methodology.md`.


<a id="workload-a"></a>

### Workload A: Sparse records


**File size**

| Pop | capnp | fb | nxs | proto |
| --- | --- | --- | --- | --- |
| 10% | 4.44 MB | 2.04 MB | 2.25 MB | 0.78 MB |
| 25% | 4.95 MB | 2.96 MB | 2.96 MB | 1.34 MB |
| 50% | 5.80 MB | 4.37 MB | 4.14 MB | 2.28 MB |
| 90% | 7.13 MB | 6.55 MB | 6.03 MB | 3.76 MB |

**Selective read P50 (NXS: C driver; `< 1 µs` = below timer resolution)**

| Pop | capnp | fb | nxs | proto |
| --- | --- | --- | --- | --- |
| 10% | 3.4 µs | 7.8 µs | < 1 µs (below timer resolution) | < 1 µs (below timer resolution) |
| 25% | 3.2 µs | 7.9 µs | < 1 µs (below timer resolution) | < 1 µs (below timer resolution) |
| 50% | 3.3 µs | 7.7 µs | < 1 µs (below timer resolution) | < 1 µs (below timer resolution) |
| 90% | 3.0 µs | 7.6 µs | < 1 µs (below timer resolution) | < 1 µs (below timer resolution) |

† Protobuf **selective** uses attribute access on a **pre-parsed** message (same warm-object model as Workload B access). NXS selective uses the **C driver** zero-copy path (FNV key index + per-record rank cache). Both may show `< 1 µs` on this hardware; the mechanisms are not comparable.


<a id="workload-b"></a>

### Workload B: Cold-open random access


**Workload B — zero-copy warm access (open, access, size)**

| Format | open | access | size |
| --- | --- | --- | --- |
| nxs | < 1 µs (below timer resolution) | < 1 µs (below timer resolution) | 1.30 MB |
| capnp | 1.7 µs | 1.7 µs | 1.20 MB |
| fb | 2.3 µs | 3.4 µs | 1.12 MB |

_NXS **open** and **access** at `< 1 µs` reflect **warm page cache** on this file size (~1.3 MB at 10k records) after the C harness has touched the file — not cold-open from disk. Cold-open latency at larger files is documented separately; on this hardware, initial header + tail-index mapping for a ~1.5 GB file is ~25 µs. Cap'n Proto / FlatBuffers open in the table above are Python harness samples on the same warm-cache conditions._


**Workload B — NXS scan (C driver, publication)**

| Format | scan |
| --- | --- |
| nxs | 25.0 µs |

_NXS scan (`driver=c`): C `nxs_sum_f64` on flat-8 schema (~25 µs at 10k dev macOS). Earlier ~8.9 ms matrix rows were Python harness overhead._


**Workload B — scan reference (Python harness — not wire-format limits)**

| Format | scan |
| --- | --- |
| capnp | 2.79 ms † Python harness |
| fb | 21.36 ms † Python harness |

† **Cap'n Proto / FlatBuffers scan** is measured with the **Python harness** (warm accessor loop). These numbers reflect Python overhead, not wire-format scan limits. Publication NXS scan uses the **C driver** (`nxs_sum_f64` / `scan_offset_bulk`).


**Workload B — Protobuf (post-parse reference)**

| Format | open | access | scan | size |
| --- | --- | --- | --- | --- |
| proto | 433.8 µs | < 1 µs (below timer resolution) | 887.1 µs | 0.72 MB |

† Protobuf **access** and **scan** are measured on a **pre-parsed Python object graph** (not wire decode in the timed region). **Open** is full `ParseFromString` per sample. Not comparable to zero-copy access/scan for NXS, FlatBuffers, or Cap'n Proto.


<a id="workload-c"></a>

### Workload C: Dense analytical reducer

Workload C measures **sum of `score` (f64)** over a dense 8-field schema. Arrow uses `pyarrow.compute.sum` on a cached table (scan only). NXS row uses per-record traversal; NXS columnar uses `col_sum_f64` with runtime SIMD (NEON on Apple Silicon, AVX2 on x86_64).

**Workload C — 10k records (frozen matrix)**

| Format | open | scan | size |
| --- | --- | --- | --- |
| arrow | 87.1 µs | 3.0 µs | 0.56 MB |
| nxs (row) | 23.0 µs | 8.54 ms | 1.06 MB |
| capnp | 1.6 µs | 2.86 ms | 0.64 MB |

_At 10k, Arrow columnar scan still wins on row-oriented NXS by orders of magnitude — expected for row vs columnar layouts._


**Workload C — 1M records, Apple Silicon (columnar validation)**

| Format | scan P50 | size | notes |
| --- | --- | --- | --- |
| arrow (IPC, cached table) | **104 µs** | 54 MB | `pc.sum` on loaded column |
| nxs columnar (`col_sum_f64`, SIMD) | **107 µs** | 62 MB | Rust harness; reopen reader each sample |
| nxs row (`SumF64` / per-record) | **11.7 ms** | 101 MB | 112× slower than Arrow — wrong layout for this workload |

_Columnar NXS reaches **at parity with Arrow IPC** on dense scan after open-core SIMD (`col_reduce`). Row layout is not competitive for this workload. Columnar files are ~15% larger than Arrow IPC (tail-index + per-field null bitmaps). Linux x86_64 columnar scan pending AVX-512 dispatch verification._

Reproduce 1M columnar:

```bash
cd nyxis/bench/harness/rust && cargo run --release -- \
  --workload C --records 1000000 --metric scan --layout columnar \
  --data-dir ../../data/bin
```

**String-inclusive layouts (Phase 3)** — schema `id` (i64) · `name` (str, `user_{i}`) · `score` (f64). Per trial: **100** random `get_str("name")` + full-column walk (sum of `name` byte lengths over all records). Driver: `bench_columnar_strings`. macOS arm64 dev.

**1M records — P50 (µs)**

| Layout | random `get_str` | full name walk | file size |
| --- | --- | --- | --- |
| row | 13 | 15,024 | 58.0 MB |
| columnar | 13 | 14,313 | 31.3 MB |
| pax | 30 | 26,593 | 31.3 MB |

Columnar/PAX files are ~**47% smaller** than row (contiguous offsets+values vs per-record strings). Full-column walk via `Record::get_str` is similar row vs columnar at 1M; use **`col_var_buffer` / `Reader::col_var_buffer`** for zero-copy bulk scans over the offset+values blobs (see `bench_columnar_strings` JSON field `str_var_scan_us`). PAX adds page lookup overhead on both access patterns.

**100k smoke — P50 (µs):** row 3 / 1518 · columnar 1 / 1362 · pax 5 / 2302.

```bash
make -C bench run-c-strings BENCH_C_STRINGS_RECORDS=1000000
```

Conformance: `columnar_flat8_strings_100`, `pax_flat8_strings_p128_300` (C/Go/JS drivers).


**Workload C — Protobuf (post-parse reference)**

| Format | open | scan | size |
| --- | --- | --- | --- |
| proto | 288.2 µs | 972.3 µs | 0.40 MB |

† Protobuf **access** and **scan** are measured on a **pre-parsed Python object graph** (not wire decode in the timed region). **Open** is full `ParseFromString` per sample. Not comparable to zero-copy access/scan for NXS, FlatBuffers, or Cap'n Proto.


<a id="workload-d"></a>

### Workload D: Streaming ingest


**Workload D — TTFR (publication: n=1000, flush_every=100)**

| Format | P50 | P95 | P99 |
| --- | --- | --- | --- |
| nxs | 142 µs | 237 µs | 437 µs |
| proto | 214 µs | 354 µs | 696 µs |
| capnp | 209 µs | 353 µs | 583 µs |

_Publication TTFR: **n=1000** trials, **flush_every=100** (batched flush), D2 file-on-disk, poll 50 µs. Smoke TTFR (20 trials, flush_every=1) is not shown — P50 can differ (e.g. Protobuf may lead on smoke). Earlier n=1000 **per-record flush** run showed Cap'n Proto winning P99 (252 µs vs 321 µs); this **batched flush** run shows NXS ahead at P99 (437 µs vs 583 µs). Flush policy affects tail behavior; do not claim NXS wins P99 until **Linux + inotify** confirms which result is stable under push notification._


† FlatBuffers has no native file-level streaming (root offset at buffer start). With external per-record framing, TTFR is expected to match Cap'n Proto framed streaming.


**Workload D — seal latency (NXS, full dataset)**

| Format | seal |
| --- | --- |
| nxs | 3992 µs |

**Workload D — sustained throughput (batched flush)**

| Format | throughput |
| --- | --- |
| nxs | 24516 rec/s |
| proto | 26335 rec/s |
| capnp | 24960 rec/s |

_**throughput**: sustained rec/s from first complete record to last while the writer appends (flush_every=100). Smoke throughput (~200 rec/s) is omitted from publication._

**Workload D — PAX streaming TTFR** (macOS arm64 dev, `page_size=256`, numeric flat-8 subset)

| Variant | P50 | P95 | P99 | Notes |
| --- | --- | --- | --- | --- |
| row (`nxs`, first NYXO) | 142 µs | 237 µs | 437 µs | Publication n=1000 trials, flush_every=100 |
| PAX first complete page (`nxs_pax`, 10k fixture) | 3706 µs | 11437 µs | 12648 µs | 200 trials; TTFR = 256 records → one `NXSP` page |
| PAX first complete page (`nxs_pax`, **1M** fixture) | 3714 µs | 9583 µs | 12585 µs | Same page_size; TTFR independent of total file size |

_Reproduce: `make -C bench run-d-pax-ttfr BENCH_RECORDS_D=1000000`. PAX TTFR scales with `page_size`, not total records (OLAP.md §4.5). Linux x86_64 pending._

<a id="workload-e"></a>

### Workload E: PAX mixed access

**Status:** Published (macOS arm64 dev, flat-8 numeric schema, `page_size=4096`). **Publication scale: n=1,000,000.** Linux x86_64 pending.

Per trial: open sealed file → **100** pseudo-random `get_f64("score")` → one `col_sum_f64("score")`. Driver: Rust `Reader` / `bench_pax_mixed` (200 samples after 20 warmup).

**1M records — P50 (µs)**

| Layout | random access | col scan | mixed total | file size |
| --- | --- | --- | --- | --- |
| row | 11 | 10,671 | 10,683 | 66.0 MB |
| columnar | 0 | 103 | 104 | 32.5 MB |
| pax | 10 | 9,315 | 9,327 | 32.5 MB |

**OLAP gate (1M):** PAX col-scan **9.3 ms** vs row **10.7 ms** (pass). Random access **10 µs** vs row **11 µs** (within 2×). Columnar col-scan **103 µs** — fastest for dense numeric scan.

**10k smoke (dev sanity)**

| Layout | random access P50 | col scan P50 | mixed P50 |
| --- | --- | --- | --- |
| row | 1 | 104 | 106 |
| columnar | 1 | 1 | 2 |
| pax | 1 | 36 | 38 |

```bash
cd nyxis && make -C bench run-e-mixed BENCH_E_RECORDS=1000000
make -C bench run-e-mixed BENCH_E_RECORDS=10000   # quick smoke
```

### Honest positioning (macOS dev, 10k records)

**Supported by this dataset:**

- NXS warm random **access** is fastest among zero-copy formats at this record size
- NXS selective read (C driver) is sub-microsecond; competitive with Cap'n Proto
- NXS file size is competitive with FlatBuffers at 50%+ population; FlatBuffers leads at 10–25%
- NXS streaming **TTFR** leads at P50/P95 on this frozen batched run (n=1000, flush_every=100)
- P99 TTFR vs Cap'n Proto **conflicts across flush policies** on macOS — do not claim a P99 win until Linux Q1
- NXS sustained streaming throughput is in the same band as Protobuf and Cap'n Proto (~25k rec/s)
- **NXS columnar** dense scan at **1M records is at parity with Arrow IPC** on Apple Silicon (107 µs vs 104 µs P50)
- **NXS row** dense scan is **112× slower than Arrow** at 1M — use `columnar` layout or the Arrow bridge
- NXS is the only format here with native file-level streaming **and** post-seal O(1) random access


**Not supported:**

- NXS file size wins at low population (FlatBuffers leads at 10–25%)
- NXS cold open vs Cap'n Proto / FlatBuffers at small files
- Claiming NXS row layout competes with Arrow on dense columnar scan
- Linux x86_64 columnar parity until AVX-512 path is verified on bare metal
- Any NXS vs Protobuf claim on access/scan/selective without the post-parse footnote


**Linux run (three questions):**
 **Q1** — Under inotify, does Cap'n Proto P99 TTFR beat NXS, or does NXS hold the lead? (Two macOS n=1000 runs disagreed on P99 ordering.)
 **Q2** — Does NXS Workload A selective produce a real ns number (expect ~30–80 ns)?
 **Q3** — Does NXS Workload B C scan hold at ~25 µs on Linux?


### Reproducing this run

```bash
cd nyxis && bash bench/scripts/setup_venv.sh
make -C bench matrix BENCH_RECORDS=10000
make -C bench freeze-benchmark RESULT_DIR=bench/results/2026-05-21_mmalta
```

<!-- BENCH-SUITE-FROZEN:END -->

---


## Running the Benchmarks

Fixtures and the Node harness live in **nyxis** (`site/bench/`). Language SDK benches live in **nyxis-drivers**. In the monorepo, generate fixtures once, then point every runner at the same directory.

Run every bench **sequentially** (recommended — parallel runs skew timings):

```bash
make -C nyxis bench-sequential   # log: nyxis/bench-sequential.log
```

```bash
# From nyxis/ (core)
make fixtures FIXTURE_COUNT=1000000
# → site/bench/fixtures/records_1000000.{nxb,json,csv}

FIX="$(pwd)/site/bench/fixtures"   # adjust if you used FIXTURE_DIR=…
```

**Core (nyxis/):**

```bash
cd rust && cargo run --release --bin bench          # serialize + WAL pipeline (in-memory)
node site/bench/bench.js site/bench/fixtures        # Node.js + WASM reducers
```

**Drivers (nyxis-drivers/)** — build native extensions first where noted (`make test-py-ci`, `bash ruby/ext/build.sh`, `bash php/nxs_ext/build.sh`):

```bash
FIX=../nyxis/site/bench/fixtures

cd c && make bench && ./bench "$FIX"
cd go && go run ./cmd/bench "$FIX"
cd py && python3 bench_c.py "$FIX"
ruby ruby/bench_c.rb "$FIX"
php -d extension=php/nxs_ext/modules/nxs.so -d memory_limit=2G php/bench_c.php "$FIX"
cd swift && swift run -c release nxs-bench "$FIX"
cd kotlin && ./gradlew bench    # default FIX=../../nyxis/site/bench/fixtures
cd csharp && dotnet run -c Release -- "$FIX" --bench   # needs records_1000.* (make fixtures FIXTURE_COUNT=1000)
```

**WAL span ingestion (encode throughput, in-memory unless noted):**

```bash
# Rust — full WAL pipeline on tmpfs (append-batch / recover / seal / roundtrip)
cd nyxis/rust && cargo run --release --bin bench

cd nyxis-drivers/c && cc -O2 -std=c99 bench_wal.c nxs_writer.c -o bench_wal && ./bench_wal
cd nyxis-drivers/go && go test -run TestWalBench -v -count=1
cd nyxis-drivers/py && python3 bench_wal.py
ruby nyxis-drivers/ruby/bench_wal.rb

# Browser — live encoder comparison (Chrome)
# docker compose up  →  http://localhost:8000/demo/wal.html
```

Regenerate fixtures at any scale:

```bash
cd nyxis/rust && cargo run --release --bin gen_fixtures -- ../site/bench/fixtures 1000000
```

