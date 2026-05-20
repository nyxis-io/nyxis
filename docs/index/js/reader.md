---
room: reader
subdomain: js
source_paths: js/nxs.js, js/nxs_writer.js, js/test.js, js/bench.js, js/eslint.config.js
see_also: js/wasm_workers.md
hot_paths: nxs.js, nxs_writer.js
architectural_health: normal
security_tier: normal
---

# js/ — Core Reader & Writer

Subdomain: js/
Source paths: js/nxs.js, js/nxs_writer.js, js/test.js, js/bench.js, js/eslint.config.js

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files in Node or browser | reader.md |
| Write / emit .nxb from JavaScript | reader.md |
| Run or add JS reader/writer tests | reader.md |
| Benchmark JS NXS vs JSON/CSV | reader.md |
| Configure ESLint rules | reader.md |

---

# bench.js

DOES: Benchmarks NXS binary reads against JSON and CSV across six scenarios (open, warm random access, cold start, full scan, columnar reduce, cold pipeline). Reports per-iteration timing and speedup ratios at four fixture sizes (1k, 10k, 100k, 1M records).
SYMBOLS:
- bench(iters, fn)
- runScale(fixtureDir, n, wasm)
- parseCsv(str)
- sumCsvScore(str)
- readNxbIntoWasmSync(wasm, path)
- main()
DEPENDS: ./nxs.js, ./wasm.js
PATTERNS: warmup-then-measure, columnar-reduce, cold-pipeline
USE WHEN: Measuring NXS read throughput vs JSON/CSV at scale; demonstrating WASM reducer advantage; profiling cursor vs record-allocation overhead.

---

# eslint.config.js

DOES: Flat ESLint configuration for all `*.js` files in the `js/` directory using ES2022 module syntax and Node.js globals. Warns on `no-undef` and `no-unused-vars` (with underscore-prefix escape hatches).
SYMBOLS:
- (default export — flat config array)
DEPENDS: globals
PATTERNS: flat-config
USE WHEN: Running `npm run lint` or adding new lint rules; the only ESLint config for the JS implementation.

---

# nxs.js

DOES: Zero-copy `.nxb` binary reader for JavaScript; parses preamble, schema, and tail-index on construction, then decodes individual fields on demand with no full-file materialisation. Also exposes bulk columnar scan and optional WASM-accelerated reducer methods.
SYMBOLS:
- NxsReader(buffer)
- NxsStreamReader({ onSchema, onRecord, onEnd, onError })
- NxsReader.record(i)
- NxsReader.cursor()
- NxsReader.scan(fn)
- NxsReader.slot(key)
- NxsReader.useWasm(wasm)
- NxsReader.records()
- NxsReader.scanF64(key)
- NxsReader.scanI64(key)
- NxsReader.sumF64(key)
- NxsReader.minF64(key)
- NxsReader.maxF64(key)
- NxsReader.sumI64(key)
- NxsObject.get(key)
- NxsObject.getI64(key) / getI64BySlot(slot)
- NxsObject.getF64(key) / getF64BySlot(slot)
- NxsObject.getBool(key) / getBoolBySlot(slot)
- NxsObject.getStr(key) / getStrBySlot(slot)
- NxsObject.toObject()
- NxsCursor.seek(i)
- WIRE_SIGILS
- Types: NxsReader, NxsObject, NxsCursor, NxsError
PATTERNS: lazy-decode, adaptive-rank-cache, zero-allocation-cursor, slot-optimisation, incremental-stream-parse
USE WHEN: Reading `.nxb` files in Node or browser; use `NxsStreamReader` to parse schema and complete NYXO records while bytes are still downloading; use `cursor()` + `getXBySlot` for tight scan loops; use `sumF64`/`minF64`/`maxF64` for aggregate queries; use `useWasm(wasm)` from `wasm.js` to offload reducers to native WASM.

---

# nxs_writer.js

DOES: Direct-to-buffer `.nxb` emitter that mirrors the Rust `NxsWriter` API; builds NYXO object frames with LEB128 bitmask, back-patches length and offset tables, then assembles the final preamble + schema + data + tail-index into a single `Uint8Array`.
SYMBOLS:
- NxsSchema(keys)
- NxsWriter(schema)
- NxsWriter.beginObject()
- NxsWriter.endObject()
- NxsWriter.finish() — emits streamable v1.1 files with Preamble TailPtr=0 and final FooterTailPtr
- NxsWriter.reset()
- NxsWriter.writeI64(slot, v)
- NxsWriter.writeF64(slot, v)
- NxsWriter.writeBool(slot, v)
- NxsWriter.writeStr(slot, v)
- NxsWriter.writeBytes(slot, data)
- NxsWriter.writeTime(slot, unixNs)
- NxsWriter.writeNull(slot)
- NxsWriter.writeListI64(slot, values)
- NxsWriter.writeListF64(slot, values)
- NxsWriter.fromRecords(keys, records)
- Types: NxsSchema, NxsWriter
PATTERNS: slot-indexed-write, back-patch, grow-by-doubling
USE WHEN: Generating `.nxb` fixtures or WAL records in JS; use `NxsSchema` once and reuse many `NxsWriter` instances; use `fromRecords` for one-shot plain-object serialisation.

---

# test.js

DOES: Smoke-test suite for the JS reader and writer; loads fixture files, validates field values against the JSON ground truth, checks error codes for malformed inputs, and exercises writer round-trips (typed fields, unicode, null, schema evolution).
SYMBOLS:
- test(name, fn)
- assertEq(actual, expected, msg)
- assertClose(actual, expected, eps, msg)
DEPENDS: ./nxs.js, ./nxs_writer.js
PATTERNS: test-harness, fixture-driven, round-trip-validation
USE WHEN: Running `node test.js <fixtures_dir>` to verify reader/writer correctness; adding regression tests for new field types or edge cases.
