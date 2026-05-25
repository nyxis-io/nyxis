---
room: wasm_workers
subdomain: js
source_paths: js/wasm.js, js/wasm/nxs_reducers.c, js/nxs_worker.js, js/explorer_worker.js, js/json_worker.js, js/test_wasm.js, js/theme.js, js/server.py
see_also: ["js/reader.md"]
hot_paths: wasm.js, nxs_reducers.c, explorer_worker.js
architectural_health: normal
security_tier: normal
---

# js/ — WASM, Workers & Browser Demo

Subdomain: js/
Source paths: js/wasm.js, js/wasm/nxs_reducers.c, js/nxs_worker.js, js/explorer_worker.js, js/json_worker.js, js/test_wasm.js, js/theme.js, js/server.py

## TASK → LOAD

| Task | Load |
|------|------|
| Load WASM reducers and attach to NxsReader | wasm_workers.md |
| Add or modify a WASM aggregate (sum/min/max) | wasm_workers.md |
| Build the browser log-explorer demo | wasm_workers.md |
| Spawn NXS Web Workers with SharedArrayBuffer | wasm_workers.md |
| Run the WASM parity test suite | wasm_workers.md |
| Serve the browser demo with COOP/COEP headers | wasm_workers.md |
| Modify the dark/light theme toggle | wasm_workers.md |

---

# explorer_worker.js

DOES: Background search worker that loads an `.nxb` file (via URL fetch or transferred `ArrayBuffer`), then performs cancellable substring searches over the `username` field using a reusable cursor, streaming progress and trimmed `Int32Array` results back to the main thread.
SYMBOLS:
- (Web Worker — no exports; communicates via postMessage)
- Message types handled: load-url, load, search
- Message types emitted: loaded, load-error, search-progress, search-done
DEPENDS: ./nxs.js
PATTERNS: cancellable-token, chunked-yield, zero-allocation-cursor, transferable-typed-array
USE WHEN: Powering the browser log-explorer UI search box; the only worker that fetches files by URL and supports mid-scan cancellation.

---

# json_worker.js

DOES: Counterpart Web Worker that accepts a structured-cloned JSON array and answers `read` queries against it; used as the JSON baseline in the multi-worker comparison demo to measure structured-clone overhead vs NXS SharedArrayBuffer zero-copy.
SYMBOLS:
- (Web Worker — no exports; communicates via postMessage)
- Message types handled: init, read
- Message types emitted: ready, read-result
PATTERNS: structured-clone-baseline
USE WHEN: Benchmarking JSON worker init cost vs `nxs_worker.js`; illustrates the copy overhead that NXS SAB sharing avoids.

---

# nxs_reducers.c

DOES: Freestanding C source compiled to `wasm/nxs_reducers.wasm`; implements five NXS columnar reducers (`sum_f64`, `sum_i64`, `min_f64`, `max_f64`, `min_max_has_result`) and a zero-copy span encoder (`encode_span`) that builds a complete NYXO record in caller-supplied WASM memory without any libc or allocator.
SYMBOLS:
- sum_f64(base, size, tail_start, record_count, slot)
- sum_i64(base, size, tail_start, record_count, slot)
- min_f64(base, size, tail_start, record_count, slot)
- max_f64(base, size, tail_start, record_count, slot)
- min_max_has_result()
- encode_span(out_ptr, fields_ptr)
PATTERNS: freestanding-wasm, no-libc, inline-bitmask-walk, wasm-export-attribute
USE WHEN: Adding or modifying WASM-accelerated aggregates; the `encode_span` function is the hot path for WAL span ingestion from `WasmSpanWriter` in `wasm.js`.

---

# nxs_worker.js

DOES: General-purpose NXS Web Worker that initialises a reader from a transferred or SAB-backed buffer, handles typed field reads dispatched by sigil, and supports in-place `write-f64` mutations visible to all workers sharing the same `SharedArrayBuffer`.
SYMBOLS:
- (Web Worker — no exports; communicates via postMessage)
- Message types handled: init, read, write-f64, read-f64-fast
- Message types emitted: ready, read-result, write-result, read-fast-result
DEPENDS: ./nxs.js
PATTERNS: sab-zero-copy, offset-cache, sigil-dispatch
USE WHEN: Spawning per-worker NXS readers in a multi-worker demo; use `write-f64` to mutate live records in shared memory; use `read-f64-fast` for low-latency polling without requestId overhead.

---

# server.py

DOES: Minimal `ThreadingHTTPServer` that adds `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` headers to every response, enabling `SharedArrayBuffer` in the browser for the WASM demo.
SYMBOLS:
- main()
- Types: COOPCOEPRequestHandler
PATTERNS: coop-coep-headers, threading-http-server
USE WHEN: Running the browser WASM demo locally (`python3 server.py`); required because `python3 -m http.server` does not set the isolation headers that `SharedArrayBuffer` demands.

---

# test_wasm.js

DOES: Parity test suite that cross-checks all WASM reducer exports (`sum_f64`, `sum_i64`, `min_f64`, `max_f64`) against the pure-JS implementations and the JSON ground truth; also verifies the zero-copy `readNxbIntoWasm` path.
SYMBOLS:
- test(name, fn)
DEPENDS: ./nxs.js, ./wasm.js
PATTERNS: wasm-parity, zero-copy-verification
USE WHEN: Running `node test_wasm.js <fixtures_dir>` after recompiling `nxs_reducers.wasm`; the definitive check that WASM and JS reducers agree.

---

# theme.js

DOES: Self-contained IIFE that reads/writes a `nxs-theme` localStorage key and toggles `data-theme="light"` on `<html>` for all `.theme-toggle` buttons in the WASM browser demo; falls back to dark theme when `localStorage` is unavailable.
SYMBOLS:
- apply(theme, persist)
- toggle()
PATTERNS: iife, localstorage-theme-toggle
USE WHEN: Adding or modifying the dark/light theme toggle in the browser demo HTML page.

---

# wasm.js

DOES: Loads `nxs_reducers.wasm` in both Node and browsers, wraps the instance in `NxsWasm` (which manages the linear memory and payload loading), and exposes `WasmSpanWriter` for zero-copy span encoding and `readNxbIntoWasm` for Node-side direct-into-WASM file reads.
SYMBOLS:
- NxsWasm(instance, memory, dataBase)
- NxsWasm.allocBuffer(n)
- NxsWasm.loadPayload(nxbBytes)
- loadWasm(wasmUrl, opts)
- readNxbIntoWasm(wasm, path)
- WasmSpanWriter(wasm, maxRecordBytes)
- WasmSpanWriter.encode(sp)
- Types: NxsWasm, WasmSpanWriter
DEPENDS: ./wasm/nxs_reducers.wasm
PATTERNS: wasm-memory-management, zero-copy-node-read, dual-env-loader
USE WHEN: Attaching WASM acceleration to an `NxsReader` via `reader.useWasm(wasm)`; use `WasmSpanWriter` for high-throughput WAL span serialisation; use `readNxbIntoWasm` (Node only) to avoid the `readFileSync` + `loadPayload` copy.
