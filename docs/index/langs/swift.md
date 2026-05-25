---
room: swift
subdomain: langs
source_paths: swift/
see_also: ["langs/csharp.md", "langs/kotlin.md"]
hot_paths: NXSReader.swift, NXSWriter.swift
architectural_health: normal
security_tier: normal
---

# langs/ — Swift Implementation

Source paths: swift/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files from Swift | swift.md |
| Write .nxb output from Swift | swift.md |
| Run or add Swift tests | swift.md |
| Benchmark Swift NXS vs JSON/CSV | swift.md |
| Run Swift conformance checks | swift.md |
| Understand the SPM package structure | swift.md |

---

# main.swift (Bench)

DOES: Swift benchmark comparing `NXSReader.sumF64`, `sumI64`, and random record access against `JSONSerialization` and a hand-written raw-byte CSV scan at 1M records. Reports best-of-5 millisecond timings.
SYMBOLS:
- benchMs(_:baseline:_:): Double
- jsonSumScore(): Double
- csvSumScore(): Double
DEPENDS: Foundation, NXS
PATTERNS: best-of-N Date()-based benchmark, raw-pointer CSV scan
USE WHEN: Measuring Swift NXS throughput; run via `swift run -c release nxs-bench <fixtures_dir>`.

---

# main.swift (Conformance)

DOES: Swift conformance runner that loads `.expected.json` vectors and validates `NXSReader` against each positive or negative test case. Handles list fields via `readListFromReader` using raw `Data` access and a global file-data context variable.
SYMBOLS:
- runConformance(): Int32
- runPositive(dir: String, name: String, expected: [String: Any]): void (throws)
- runNegative(dir: String, name: String, expectedCode: String): void (throws)
- resolveSlotSwift(data: Data, objOffset: Int, slot: Int): Int?
- readListFromReader(reader: NXSReader, ri: Int, key: String): [Any]?
- valuesMatch(_:_:): Bool
- approxEq(_:_:): Bool
TYPE: ConformanceError.mismatch(String)
DEPENDS: Foundation, NXS
PATTERNS: global-state file-data context, NXSError string-pattern error-code mapping
USE WHEN: Running conformance via `swift run nxs-conformance <conformance_dir>`.

---

# main.swift (Test)

DOES: Swift smoke-test suite for `NXSReader` and `NXSWriter`. Validates field access against the 1000-record JSON fixture, tests out-of-bounds errors, bulk reducers, and writer round-trips including unicode strings and multi-byte bitmask.
SYMBOLS:
- check(_:_:): void
- checkThrows(_:_:): void
DEPENDS: Foundation, NXS
PATTERNS: fixture-based parity testing, try/catch error assertion
USE WHEN: Running `swift run nxs-test <fixtures_dir>` to validate the Swift implementation.

---

# NXSReader.swift

DOES: Swift 5.9+ NXS binary reader. Parses preamble, schema, and tail-index on `init`; provides O(1) `record()` lookup, bulk reducers using raw-pointer `withUnsafeBytes` loops, and lazy `NYXObject` field access via LEB128 bitmask walk.
SYMBOLS:
- NXSReader.init(_ data: Data) throws: NXSReader
- NXSReader.slot(_ key: String) throws: Int
- NXSReader.record(_ i: Int) throws: NYXObject
- NXSReader.sumF64 / sumI64(_ key: String) throws: Double / Int64
- NXSReader.minF64 / maxF64(_ key: String) throws: Double?
- NYXObject.getI64 / getF64 / getBool / getStr(_ key: String) throws: typed
- NYXObject.getI64BySlot / getF64BySlot / getBoolBySlot / getStrBySlot(_ slot: Int) throws: typed
TYPE: NXSError { badMagic, outOfBounds, keyNotFound, fieldAbsent }
DEPENDS: Foundation
PATTERNS: raw-pointer bulk loops, LEB128 bitmask walk, MurmurHash3-64, Data unaligned LE reads
USE WHEN: Reading `.nxb` from Swift; use raw-pointer `sumF64` path for hot column scans, `NYXObject` for individual record access.

---

# NXSWriter.swift

DOES: Swift 5.9+ NXS writer. Builds `.nxb` output into a `[UInt8]` buffer with schema precompilation, slot-indexed writes, and bitmask/offset-table back-patching via `withUInt32LE`. Includes `fromRecords` static factory.
SYMBOLS:
- NXSSchema.init(keys: [String]): NXSSchema
- NXSWriter.init(schema: NXSSchema): NXSWriter
- NXSWriter.beginObject / endObject(): void
- NXSWriter.finish(): [UInt8]
- NXSWriter.writeI64 / writeF64 / writeBool / writeStr / writeNull / writeBytes / writeTime(slot:value:): void
- NXSWriter.writeListI64 / writeListF64(slot:values:): void
- NXSWriter.fromRecords(keys:records:): [UInt8]
TYPE: NXSSchema { keys, bitmaskBytes, count }
TYPE: Frame { start, bitmask, offsetTable, slotOffsets, lastSlot, needsSort }
DEPENDS: Foundation
PATTERNS: two-phase write (placeholder + back-patch), LEB128 bitmask, MurmurHash3-64
USE WHEN: Writing `.nxb` from Swift; `finish()` returns a `[UInt8]` array; wrap in `Data(bytes)` for file I/O.

---

# Package.swift

DOES: Swift Package Manager manifest defining the `nyxis` package. Declares the `NXS` library target and three executable targets (`nxs-test`, `nxs-bench`, `nxs-conformance`) each depending on `NXS`.
SYMBOLS:
- (no exported symbols — build manifest only)
DEPENDS: swift-tools-version 5.9
PATTERNS: SPM multi-target layout
USE WHEN: Building or running any Swift NXS target; `swift run nxs-test`, `swift run nxs-bench`, or `swift run nxs-conformance`.
