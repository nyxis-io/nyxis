---
room: implementations/kotlin
source_paths: [kotlin/src/main/kotlin/nxs/]
file_count: 3
architectural_health: normal
security_tier: normal
hot_paths: [NxsReader.kt]
see_also: [spec/format.md]
---

# Bench.kt

DOES: Benchmarks NXS columnar reducers (sumF64, sumI64) and random access against org.json JSON parsing and raw CSV byte scanning on 1M records; reports best-of-5 times with baseline ratios.
SYMBOLS:
- benchMs(label: String, baseline: Double, runs: Int, body: () -> Unit): Double
- jsonSumScore(jsonBytes: ByteArray): Double
- csvSumScore(csvBytes: ByteArray): Double
- runBench(args: Array<String>): Unit
- main(args: Array<String>): Unit
DEPENDS: nxs.NxsReader, org.json.JSONArray
PATTERNS: benchmark-comparison, format-evaluation, warmup
USE WHEN: Comparing NXS JVM performance against JSON/CSV; run with `gradle bench`.

---

# NxsReader.kt

DOES: Zero-copy .nxb reader for Kotlin/JVM implementing Nyxis v1.1: tail-index record access, LEB128 bitmask parsing, typed field accessors, and bulk reducers (sum/min/max f64, sum i64).
SYMBOLS:
- class NxsReader(data: ByteArray)
- class NxsObject(reader: NxsReader, offset: Int)
- class NxsError(code: String, msg: String): Exception
- NxsReader#record(i: Int): NxsObject
- NxsReader#slot(key: String): Int
- NxsReader#sumF64(key: String): Double
- NxsReader#sumI64(key: String): Long
- NxsReader#minF64(key: String): Double?
- NxsReader#maxF64(key: String): Double?
- NxsObject#getI64(key: String): Long
- NxsObject#getF64(key: String): Double
- NxsObject#getBool(key: String): Boolean
- NxsObject#getStr(key: String): String
- NxsObject#getI64BySlot(slot: Int): Long
- NxsObject#getF64BySlot(slot: Int): Double
- NxsObject#getBoolBySlot(slot: Int): Boolean
- NxsObject#getStrBySlot(slot: Int): String
- Types: NxsReader, NxsObject, NxsError
TYPE: NxsReader { version: Short, flags: Short, dictHash: Long, tailPtr: Long, keys: List<String>, keySigils: ByteArray, recordCount: Int }
DEPENDS: java.nio.ByteBuffer, java.nio.ByteOrder
PATTERNS: zero-copy-reader, little-endian-reader, columnar-accessor, jit-friendly-loop
USE WHEN: Reading .nxb records on the JVM; bulk reducers benefit from JIT warm-up (2 iterations recommended before measuring).

---

# Test.kt

DOES: Smoke tests validating NxsReader field access, schema keys, sum_f64, and out-of-bounds exception handling against the JSON fixture.
SYMBOLS:
- check(name: String, expr: Boolean): Unit
- main(args: Array<String>): Unit
DEPENDS: nxs.NxsReader, nxs.NxsError, org.json.JSONArray
PATTERNS: fixture-based-testing, exception-validation
USE WHEN: Verifying Kotlin reader correctness; run with `gradle run --args="../js/fixtures"`.
