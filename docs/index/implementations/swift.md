---
room: implementations/swift
source_paths: [swift/Sources/]
file_count: 3
architectural_health: normal
security_tier: normal
hot_paths: [NXSReader.swift]
see_also: [spec/format.md]
---

# Sources/Bench/main.swift

DOES: Benchmarks NXS sumF64/sumI64/random-access against Foundation JSONSerialization and a raw CSV byte scan on 1M records, reporting best-of-5 times with baseline ratios.
SYMBOLS:
- func benchMs(_ label: String, baseline: Double, _ body: () -> Void) -> Double
- func jsonSumScore() -> Double
- func csvSumScore() -> Double
DEPENDS: NXS, Foundation
PATTERNS: benchmark-suite, unsafe-pointer-csv-scan, format-comparison
USE WHEN: Profiling Swift NXS performance on macOS/iOS; run with `swift run -c release nxs-bench dir`.

---

# Sources/NXS/NXSReader.swift

DOES: Zero-copy .nxb reader for Swift 5.9+: tail-index record access, LEB128 bitmask parsing, typed field accessors, and bulk reducers (sum/min/max f64, sum i64). Bulk reducers use UnsafePointer inside withUnsafeBytes for performance; single-field access uses Data subscript.
SYMBOLS:
- final class NXSReader
- final class NYXObject
- enum NXSError: Error
- NXSReader#init(_ data: Data) throws
- NXSReader#record(_ i: Int) throws -> NYXObject
- NXSReader#slot(_ key: String) throws -> Int
- NXSReader#sumF64(_ key: String) throws -> Double
- NXSReader#sumI64(_ key: String) throws -> Int64
- NXSReader#minF64(_ key: String) throws -> Double?
- NXSReader#maxF64(_ key: String) throws -> Double?
- NYXObject#getI64(_ key: String) throws -> Int64
- NYXObject#getF64(_ key: String) throws -> Double
- NYXObject#getBool(_ key: String) throws -> Bool
- NYXObject#getStr(_ key: String) throws -> String
- NYXObject#getI64BySlot(_ slot: Int) throws -> Int64
- NYXObject#getF64BySlot(_ slot: Int) throws -> Double
- NYXObject#getBoolBySlot(_ slot: Int) throws -> Bool
- NYXObject#getStrBySlot(_ slot: Int) throws -> String
- Types: NXSReader, NYXObject, NXSError
TYPE: NXSReader { version: UInt16, flags: UInt16, dictHash: UInt64, tailPtr: UInt64, keys: [String], keySigils: [UInt8], recordCount: Int }
DEPENDS: Foundation
PATTERNS: zero-copy-reader, unsafe-pointer-bulk-loop, loadUnaligned-le-reads, dual-path-access
USE WHEN: Reading .nxb records on Apple platforms; use getBySlot variants on hot paths; bulk reducers are ~3× faster than naïve Data[i] path due to UnsafePointer optimisation.

---

# Sources/Test/main.swift

DOES: Smoke tests for NXSReader: schema keys, typed field access, sum/min/max correctness vs JSON fixture, out-of-bounds throw validation.
SYMBOLS:
- func check(_ name: String, _ expr: Bool)
- func checkThrows(_ name: String, _ body: () throws -> Void)
DEPENDS: NXS, Foundation
PATTERNS: fixture-based-testing, exception-validation
USE WHEN: Verifying reader correctness; run with `swift run nxs-test dir`.
