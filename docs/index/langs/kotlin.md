---
room: kotlin
subdomain: langs
source_paths: kotlin/src/main/kotlin/nxs/
see_also: langs/csharp.md
hot_paths: NxsReader.kt, NxsWriter.kt
architectural_health: normal
security_tier: normal
---

# langs/ — Kotlin Implementation

Source paths: kotlin/src/main/kotlin/nxs/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files from Kotlin/JVM | kotlin.md |
| Write .nxb output from Kotlin | kotlin.md |
| Run or add Kotlin tests | kotlin.md |
| Benchmark Kotlin NXS vs JSON/CSV | kotlin.md |
| Run Kotlin conformance checks | kotlin.md |

---

# Bench.kt

DOES: Kotlin/JVM benchmark comparing `NxsReader.sumF64`, `NxsReader.sumI64`, and random access against JSON (via `org.json`) and raw CSV byte scanning at 1M records. Reports best-of-N millisecond timings.
SYMBOLS:
- benchMs(label: String, baseline: Double, runs: Int, body: () -> Unit): Double
- jsonSumScore(jsonBytes: ByteArray): Double
- csvSumScore(csvBytes: ByteArray): Double
- runBench(args: Array<String>): Unit
- main(args: Array<String>): Unit
DEPENDS: org.json.JSONArray, java.io.File, NxsReader, NxsWriter
PATTERNS: best-of-N benchmark, JIT warmup
USE WHEN: Measuring Kotlin NXS read throughput against JSON/CSV baselines; run via `./gradlew run --args="../js/fixtures"`.

---

# Conformance.kt

DOES: Kotlin conformance runner that loads every `.expected.json` vector from the conformance directory and validates NxsReader output against expected field values (positive) or expected error codes (negative).
SYMBOLS:
- main(args: Array<String>): Unit
- runPositive(conformanceDir: String, name: String, expected: JSONObject): Unit
- runNegative(conformanceDir: String, name: String, expectedCode: String): Unit
- getFieldValue(data, tailStart, ri, slot, sigilByte): Pair<Any?, Boolean>
- resolveSlotRaw(data, objOffset, slot): Int
- valuesMatch(actual, expected): Boolean
- readList(data, off): List<Any?>?
DEPENDS: org.json.JSONArray, org.json.JSONObject, java.io.File, NxsReader
PATTERNS: positive/negative vector dispatch, sigil-driven field decode
USE WHEN: Running conformance checks via `./gradlew conformance`; exit code 1 on any failure.

---

# NxsReader.kt

DOES: Kotlin/JVM NXS binary reader. Parses preamble, schema, and tail-index on construction; provides O(1) `record()` lookup, slot-keyed bulk reducers (`sumF64`, `sumI64`, `minF64`, `maxF64`), and lazy `NxsObject` field access.
SYMBOLS:
- NxsReader(data: ByteArray): NxsReader
- NxsReader.record(i: Int): NxsObject
- NxsReader.slot(key: String): Int
- NxsReader.sumF64 / sumI64(key: String): Double / Long
- NxsReader.minF64 / maxF64(key: String): Double?
- NxsReader.scanOffset(objOffset: Int, slot: Int): Int
- NxsObject.getI64 / getF64 / getBool / getStr(key: String): typed
- NxsObject.getI64BySlot / getF64BySlot / getBoolBySlot / getStrBySlot(slot: Int): typed
TYPE: NxsError(code: String, msg: String) extends Exception
DEPENDS: java.nio.ByteBuffer, java.nio.ByteOrder
PATTERNS: ByteBuffer LE reads, LEB128 bitmask walk, MurmurHash3-64, tail-index O(1)
USE WHEN: Reading `.nxb` files from Kotlin/JVM; use `slot()`-prefixed methods to avoid per-call map lookups in hot loops.

---

# NxsWriter.kt

DOES: Kotlin/JVM NXS writer. Builds `.nxb` output into a `ByteArrayOutputStream` with schema precompilation, slot-indexed writes, and bitmask/offset-table back-patching. Includes `fromRecords` convenience helper.
SYMBOLS:
- NxsSchema(keys: List<String>): NxsSchema
- NxsWriter(schema: NxsSchema): NxsWriter
- NxsWriter.beginObject / endObject(): Unit
- NxsWriter.finish(): ByteArray
- NxsWriter.writeI64 / writeF64 / writeBool / writeStr / writeNull / writeBytes(slot, value): Unit
- NxsWriter.writeListI64 / writeListF64(slot, values): Unit
- NxsWriter.fromRecords(keys, records): ByteArray
TYPE: NxsSchema { keys, bitmaskBytes, count }
DEPENDS: java.io.ByteArrayOutputStream
PATTERNS: two-phase write, LEB128 bitmask, MurmurHash3-64
USE WHEN: Generating `.nxb` from Kotlin; call `finish()` to obtain the complete binary blob.

---

# Test.kt

DOES: Kotlin smoke-test suite for `NxsReader` and `NxsWriter`. Validates field values against the 1000-record JSON fixture, tests out-of-bounds errors, sum/min/max reducers, and writer round-trips including multi-byte bitmask.
SYMBOLS:
- main(args: Array<String>): Unit
- check(name: String, expr: Boolean): Unit
DEPENDS: org.json.JSONArray, java.io.File, NxsReader, NxsWriter, NxsSchema
PATTERNS: fixture-based parity testing
USE WHEN: Running the Kotlin test suite via `./gradlew run --args="../js/fixtures"`.
