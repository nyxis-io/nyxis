# Scenarios

Four stress scenarios that test the claims NXS makes. Three are implemented and runnable today.

---

## 1. The unparsable dataset

**Claim:** NXS opens files that JSON cannot.

JSON has a hard ceiling of roughly 512 MB in browser environments — `JSON.parse` throws `Invalid string length` before it finishes. At 1.5 GB it does not fail gracefully; it crashes the tab. CSV fares no better. NXS memory-maps the file and reads the tail-index in microseconds regardless of total size, because it never materialises the whole file as a string.

**Test:** Load a 1.5 GB `.nxb` file in a browser tab. Read a field from record 800,000.

**Status: implemented** — `bench.html` runs this scenario. Generate the fixture:

```bash
cd rust && cargo run --release --bin gen_fixtures -- ../js/fixtures 14000000
```

Then serve and open:

```bash
cd js && python3 server.py
# http://localhost:8000/bench.html
```

The benchmark includes a JSON and CSV comparison at the same record count. JSON throws before it gets there.

---

## 2. The 60 FPS ticker

**Claim:** In-place byte patching produces no GC pressure; JSON re-parse does.

A real-time dashboard that re-parses a JSON payload every frame allocates a new object graph on every tick. The garbage collector eventually pauses the main thread, producing frame drops visible in Chrome DevTools as Long Tasks. NXS delta-patching overwrites bytes in an existing `ArrayBuffer` — no allocation, no GC.

**Test:** Update a numeric field 60 times per second. Measure Long Tasks in Chrome DevTools Performance panel.

**Status: implemented** — `ticker.html`.

```bash
cd js && python3 server.py
# http://localhost:8000/ticker.html
```

The demo runs both approaches side by side. The JSON trace shows periodic spikes; the NXS trace is flat.

---

## 3. Shared memory across workers

**Claim:** A `.nxb` file can be shared across Web Workers with zero bytes copied between threads.

A `JSON.parse` result is an ordinary JS object — it cannot cross worker boundaries without structured cloning, which copies the full payload. `SharedArrayBuffer` allows multiple workers to hold views into the same memory region, but JSON gives you no way to use it: you cannot parse JSON into a `SharedArrayBuffer`. NXS can be loaded directly into WASM memory backed by a `SharedArrayBuffer` and accessed by any number of workers simultaneously.

**Test:** Spawn 4 workers. Point each at the same `SharedArrayBuffer` containing a `.nxb` file. Run a columnar reducer on each worker in parallel.

**Status: implemented** — `workers.html`.

```bash
cd js && python3 server.py
# http://localhost:8000/workers.html
```

The demo shows 4 workers each summing a different column, with a byte-transfer counter confirming 0 bytes copied after the initial load.

---

## 4. The log explorer

**Claim:** 10 million records, virtual scroll, live search — all in a browser tab.

This is the scenario JSON cannot approach: a production log file with tens of millions of lines, drag-and-dropped into a browser, with instant search and smooth scroll. JSON would need to parse the entire file before rendering the first line. NXS loads the tail-index, renders the visible window, and jumps to any record in O(1).

**Status: implemented** — `explorer.html`.

```bash
cd rust && cargo run --release --bin gen_fixtures -- ../js/fixtures 10000000
cd js && python3 server.py
# http://localhost:8000/explorer.html
```

---

## 5. WAL ingestion throughput

**Claim:** NXS WAL encoding is 2–3× faster than JSON serialization across all languages.

A write-ahead log appends one span record at a time in the hot path of an observability pipeline. Every nanosecond counts. NXS uses a fixed binary layout (no quoting, no escaping, no field-name repetition) to encode a 10-field span record in a single memcpy-like pass.

**Test:** Encode 10,000 spans using five strategies (generic writer, fast fixed-layout, sealed single-writer, WASM direct, JSON baseline). Measure ns/span.

**Status: implemented** — `wal.html` runs this benchmark live in the browser with a cross-language comparison chart.

```bash
cd js && python3 server.py
# http://localhost:8000/wal.html
```

Cross-language results (Apple M-series, 10k spans, 14 services, 20 OTel ops):

| Language | NXS WAL | JSON | Speedup |
| --- | --- | --- | --- |
| C | 82 ns | 262 ns | 3.2× |
| Go | 138 ns | 289 ns | 2.1× |
| Python (C ext) | 438 ns | 1,383 ns | 3.2× |
| Ruby (C ext) | 336 ns | 383 ns | 1.1× |
| JS (fast/WASM) | ~250–280 ns | ~620 ns | ~2.2–2.5× |

---

## 6. Low-end mobile

**Claim:** NXS is usable on memory-constrained devices where JSON fails.

On a mid-range Android phone, parsing a 100 MB JSON file can stall the main thread for 2–3 seconds and spike memory well above the file size due to the object graph allocation. NXS memory-maps the file and accesses only the bytes it needs.

**Test:** Load a 100 MB `.nxb` file on a 4-year-old Android device. Measure time from file load to first field rendered, and peak memory during the operation.

**Status: not yet run.** The implementation supports it — the same `bench.html` and `NxsReader` code runs on mobile browsers. A controlled measurement with JSON and CSV baselines on real hardware has not been done.
