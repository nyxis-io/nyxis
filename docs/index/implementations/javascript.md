---
room: implementations/javascript
source_paths: [js/]
file_count: 10
architectural_health: normal
security_tier: normal
hot_paths: [nxs.js, wasm.js]
see_also: [implementations/rust.md, spec/format.md]
---

# bench.js

DOES: Multi-scenario benchmark comparing NXS (pure-JS and WASM) vs JSON vs CSV across open, random-access, cold-start, full-scan, columnar, and cold-pipeline scenarios at 1K–1M records.
SYMBOLS:
- bench(iters, fn) -> result
- readNxbIntoWasmSync(wasm, path) -> Uint8Array
- parseCsv(str) -> Array<Object>
- sumCsvScore(str) -> number
- runScale(fixtureDir, n, wasm) -> Promise<void>
DEPENDS: ./nxs.js, ./wasm.js
PATTERNS: benchmark-harness, multi-scale-testing, format-comparison
USE WHEN: Measuring read performance or validating WASM reducer gains.

---

# explorer_worker.js

DOES: Web Worker that owns an NxsReader and performs substring search across username/email fields, streaming progress back to the main thread with per-token cancellation support.
SYMBOLS:
- message handler (load-url, load, search)
- activeToken: number
DEPENDS: ./nxs.js
PATTERNS: cancellation-token, batched-progress, worker-pool
USE WHEN: Powering the log-explorer interactive search; cancel a prior search by sending a new token.

---

# json_worker.js

DOES: Baseline Web Worker that parses JSON and responds to read requests via structured-clone; demonstrates copy-cost overhead vs NXS zero-copy SharedArrayBuffer sharing.
SYMBOLS:
- message handler (init, read)
DEPENDS: none
PATTERNS: copy-cost-baseline
USE WHEN: Benchmarking structured-clone cost against SharedArrayBuffer path in workers.html.

---

# nxs.js

DOES: Core zero-copy NXS reader for JavaScript (Node + Browser): O(1) tail-index record access, lazy LEB128 bitmask decoding, typed field accessors, and optional WASM bulk reducers.
SYMBOLS:
- NxsReader(buffer)
- NxsReader#record(i) -> NxsObject
- NxsReader#cursor() -> NxsCursor
- NxsReader#scan(fn) -> void
- NxsReader#useWasm(wasm) -> void
- NxsReader#slot(key) -> number
- NxsReader#sumF64(key) -> number
- NxsReader#minF64(key) -> number|null
- NxsReader#maxF64(key) -> number|null
- NxsReader#sumI64(key) -> number
- NxsReader#scanF64(key) -> Array<number|null>
- NxsObject#getI64(key) -> number
- NxsObject#getF64(key) -> number
- NxsObject#getBool(key) -> boolean
- NxsObject#getStr(key) -> string
- NxsObject#getI64BySlot(slot) -> number
- NxsObject#getF64BySlot(slot) -> number
- NxsObject#getBoolBySlot(slot) -> boolean
- NxsObject#getStrBySlot(slot) -> string
- NxsCursor#seek(i) -> NxsCursor
- NxsError(code, msg) extends Error
- decodeUtf8Fast(bytes, offset, length) -> string
- Types: NxsReader, NxsObject, NxsCursor, NxsError
DEPENDS: none
PATTERNS: zero-copy-reader, adaptive-rank-cache, lazy-decoding, little-endian-inline-reads
USE WHEN: All NXS read workloads; the core building block for every other JS file.

---

# nxs_worker.js

DOES: Web Worker that holds an NxsReader backed by SharedArrayBuffer or per-worker copy, supporting random reads, in-place float64 writes, and high-frequency polling without requestId overhead.
SYMBOLS:
- message handler (init, read, write-f64, read-f64-fast)
DEPENDS: ./nxs.js
PATTERNS: shared-memory-worker, in-place-write, zero-copy-cache
USE WHEN: workers.html multi-worker demo; high-throughput field polling across threads.

---

# server.py

DOES: Development HTTP server that adds Cross-Origin-Opener-Policy and Cross-Origin-Embedder-Policy headers required for SharedArrayBuffer in browsers.
SYMBOLS:
- COOPCOEPRequestHandler#end_headers() -> None
DEPENDS: none
PATTERNS: security-headers, dev-server
USE WHEN: Running browser demos (bench.html, workers.html) that require SharedArrayBuffer.

---

# test.js

DOES: Smoke tests for NxsReader covering schema parsing, record access, typed accessors, iteration, cursor, and sum/scan correctness against the JSON fixture.
SYMBOLS:
- test(name, fn) -> void
- assertEq(actual, expected, msg) -> void
- assertClose(actual, expected, eps, msg) -> void
DEPENDS: ./nxs.js
PATTERNS: regression-test, parity-verification
USE WHEN: Verifying reader correctness after any change to nxs.js.

---

# test_wasm.js

DOES: Parity tests confirming WASM reducers (sum_f64, min/max_f64, sum_i64) match pure-JS results, and that the zero-copy readNxbIntoWasm path produces identical output.
SYMBOLS:
- test(name, fn) -> void
DEPENDS: ./nxs.js, ./wasm.js
PATTERNS: reducer-parity, zero-copy-integration-test
USE WHEN: Validating WASM reducer builds or testing Node/browser cross-compatibility.

---

# wasm.js

DOES: WASM module loader and memory manager: allocates WASM memory, copies or shares the .nxb payload, and exposes bulk reducer exports (sum_f64, min_f64, max_f64, sum_i64). Works in Node and browser.
SYMBOLS:
- NxsWasm(instance, memory, dataBase)
- NxsWasm#allocBuffer(n) -> Uint8Array
- NxsWasm#loadPayload(nxbBytes) -> void
- loadWasm(wasmUrl?, opts?) -> Promise<NxsWasm>
- readNxbIntoWasm(wasm, path) -> Promise<Uint8Array>
- Types: NxsWasm
DEPENDS: none
PATTERNS: wasm-loader, memory-growth, zero-copy-fallback
USE WHEN: Accelerating bulk aggregations; requires wasm/nxs_reducers.wasm compiled from wasm/nxs_reducers.c.

---

# wasm/nxs_reducers.c

DOES: Freestanding C source compiled to WebAssembly; exports sum_f64, sum_i64, min_f64, max_f64 that walk the tail-index and LEB128 bitmask inline without libc or system calls.
SYMBOLS:
- sum_f64(base, size, tail_start, record_count, slot) -> double
- sum_i64(base, size, tail_start, record_count, slot) -> int64_t
- min_f64(base, size, tail_start, record_count, slot) -> double
- max_f64(base, size, tail_start, record_count, slot) -> double
- min_max_has_result() -> int32_t
- field_offset(data, size, obj_offset, slot) -> int64_t
DEPENDS: none
PATTERNS: wasm-reducer, leb128-inline-walk, freestanding-c
USE WHEN: Compiling to WASM via clang --target=wasm32; source of truth for WASM reducer logic.
