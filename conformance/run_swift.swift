// NXS conformance runner for Swift.
// Usage: swift conformance/run_swift.swift conformance/
//   (or add as a target to Package.swift)

import Foundation

// ── Load the NXS reader ───────────────────────────────────────────────────────
// We include the NXSReader source directly via a relative path.

// Since swift scripts can't import local modules easily, we include the source
// inline via #sourceLocation. The build system adds the NXS module; here
// we use a standalone approach for the runner.

// In standalone mode we compile the swift files together:
//   swiftc -O swift/Sources/NXS/NXSReader.swift conformance/run_swift.swift -o /tmp/run_swift
//   /tmp/run_swift conformance/

// ── Helpers ───────────────────────────────────────────────────────────────────

private let MAGIC_LIST_SWIFT: UInt32 = 0x4E59584C

// Read raw list from an NXB file's record at slot position
func readListFromReader(reader: NXSReader, ri: Int, key: String) -> [Any]? {
    // We need to access the raw data. Re-read the file from the path stored in
    // the runner's global context — stored via a thread-local workaround.
    guard let rawData = _currentConformanceFileData else { return nil }
    let data = rawData

    // Get tail start
    let tp = Int(reader.tailPtr)
    let tailStart = tp + 4
    let size = data.count

    // Get abs offset of record ri
    if tailStart + ri * 10 + 10 > size { return nil }
    let base = tailStart + ri * 10 + 2
    var lo: UInt64 = 0
    for b in 0..<8 { lo |= UInt64(data[base + b]) << (b * 8) }
    let abs = Int(lo)






    // Resolve slot offset
    guard let slot = try? reader.slot(key) else { return nil }
    guard let off = resolveSlotSwift(data: data, objOffset: abs, slot: slot) else { return nil }

    if off + 4 > size { return nil }
    let magic = UInt32(data[off]) | (UInt32(data[off+1]) << 8) | (UInt32(data[off+2]) << 16) | (UInt32(data[off+3]) << 24)
    if magic != MAGIC_LIST_SWIFT { return nil }
    if off + 9 > size { return nil }
    let elemSigil = data[off + 8]
    let elemCount = Int(UInt32(data[off+9]) | (UInt32(data[off+10]) << 8) | (UInt32(data[off+11]) << 16) | (UInt32(data[off+12]) << 24))
    let dataStart = off + 16

    var result: [Any] = []
    for i in 0..<elemCount {
        let ep = dataStart + i * 8
        if ep + 8 > size { break }
        switch elemSigil {
        case 0x3D:  // int
            var v: Int64 = 0
            withUnsafeMutableBytes(of: &v) { ptr in
                ptr.copyBytes(from: data[ep..<ep+8])
            }
            result.append(Int(v))
        case 0x7E:  // float
            var v: Double = 0
            withUnsafeMutableBytes(of: &v) { ptr in
                ptr.copyBytes(from: data[ep..<ep+8])
            }
            result.append(v)
        default:
            result.append(0)
        }
    }
    return result
}

func resolveSlotSwift(data: Data, objOffset: Int, slot: Int) -> Int? {
    var p = objOffset + 8
    var cur = 0
    var tableIdx = 0
    var b: UInt8 = 0
    while true {
        if p >= data.count { return nil }
        b = data[p]; p += 1
        let bits = b & 0x7F
        for i in 0..<7 {
            if cur == slot {
                if (bits >> i) & 1 == 0 { return nil }
                // drain rest of bitmask
                while (b & 0x80) != 0 {
                    if p >= data.count { break }
                    b = data[p]; p += 1
                }
                if p + tableIdx * 2 + 1 >= data.count { return nil }
                let rel = Int(UInt16(data[p + tableIdx*2]) | (UInt16(data[p + tableIdx*2 + 1]) << 8))
                return objOffset + rel
            }
            if (bits >> i) & 1 == 1 { tableIdx += 1 }
            cur += 1
        }
        if (b & 0x80) == 0 { return nil }
    }
}

// Thread-local storage for current file data (simple global for single-threaded runner)
var _currentConformanceFileData: Data? = nil

func approxEq(_ a: Double, _ b: Double) -> Bool {
    if a == b { return true }
    let diff = abs(a - b)
    let mag = max(abs(a), abs(b))
    if mag < 1e-300 { return diff < 1e-300 }
    return diff / mag < 1e-9
}

func valuesMatch(_ actual: Any?, _ expected: Any?) -> Bool {
    if expected == nil || expected is NSNull { return actual == nil || actual is NSNull }
    if let eb = expected as? Bool, let ab = actual as? Bool { return ab == eb }
    if let en = expected as? Double {
        if let an = actual as? Double { return approxEq(an, en) }
        if let an = actual as? Int    { return approxEq(Double(an), en) }
        if let an = actual as? Int64  { return approxEq(Double(an), en) }
    }
    if let en = expected as? Int {
        if let an = actual as? Int    { return an == en }
        if let an = actual as? Int64  { return Int64(en) == an }
        if let an = actual as? Double { return approxEq(an, Double(en)) }
    }
    if let en = expected as? Int64 {
        if let an = actual as? Int64  { return an == en }
        if let an = actual as? Int    { return an == Int(en) }
    }
    if let es = expected as? String, let as_ = actual as? String { return as_ == es }
    if let ea = expected as? [Any], let aa = actual as? [Any] {
        if ea.count != aa.count { return false }
        return zip(ea, aa).allSatisfy { valuesMatch($1, $0) }
    }
    return false
}

// ── Runner ────────────────────────────────────────────────────────────────────

func runPositive(dir: String, name: String, expected: [String: Any]) throws {
    let nxbPath = "\(dir)/\(name).nxb"
    let data = try Data(contentsOf: URL(fileURLWithPath: nxbPath))
    _currentConformanceFileData = data
    let reader = try NXSReader(data)

    // Validate record_count
    if let expCount = expected["record_count"] as? Int {
        guard reader.recordCount == expCount else {
            throw ConformanceError.mismatch("record_count: expected \(expCount), got \(reader.recordCount)")
        }
    }

    // Validate keys
    if let expKeys = expected["keys"] as? [String] {
        for (i, expKey) in expKeys.enumerated() {
            guard i < reader.keys.count else {
                throw ConformanceError.mismatch("key[\(i)] missing (expected \(expKey))")
            }
            guard reader.keys[i] == expKey else {
                throw ConformanceError.mismatch("key[\(i)]: expected \"\(expKey)\", got \"\(reader.keys[i])\"")
            }
        }
    }

    // Validate each record
    if let expRecords = expected["records"] as? [[String: Any]] {
        for (ri, expRec) in expRecords.enumerated() {
            let obj = try reader.record(ri)
            for (key, expVal) in expRec {
                let sigil: UInt8
                if let idx = reader.keys.firstIndex(of: key), idx < reader.keySigils.count {
                    sigil = reader.keySigils[idx]
                } else {
                    sigil = 0x3D
                }

                if expVal is NSNull { continue }

                let actual: Any?
                if expVal is [Any] {
                    // List field — decode directly from raw data
                    actual = readListFromReader(reader: reader, ri: ri, key: key)
                } else {
                    switch sigil {
                    case 0x3D: actual = (try? obj.getI64(key)).map { Int($0) }  // int
                    case 0x7E: actual = try? obj.getF64(key)                    // float
                    case 0x3F: actual = try? obj.getBool(key)                   // bool
                    case 0x22: actual = try? obj.getStr(key)                    // str
                    case 0x40: actual = (try? obj.getI64(key)).map { Int($0) }  // time
                    default:   actual = nil
                    }
                }

                if !valuesMatch(actual, expVal) {
                    throw ConformanceError.mismatch(
                        "rec[\(ri)].\(key): expected \(expVal), got \(String(describing: actual))"
                    )
                }
            }
        }
    }
}

func runNegative(dir: String, name: String, expectedCode: String) throws {
    let nxbPath = "\(dir)/\(name).nxb"
    let data = try Data(contentsOf: URL(fileURLWithPath: nxbPath))

    do {
        _ = try NXSReader(data)
        throw ConformanceError.mismatch("expected error \(expectedCode) but reader succeeded")
    } catch let e as NXSError {
        // Map NXSError to string error code
        let code: String
        // NXSError.badMagic is used for both bad magic AND dict mismatch in this impl
        // We inspect the associated string to distinguish
        let desc = "\(e)"
        if desc.contains("DICT_MISMATCH") || desc.contains("hash mismatch") {
            code = "ERR_DICT_MISMATCH"
        } else if desc.contains("badMagic") || desc.contains("BAD_MAGIC") || desc.contains("preamble") || desc.contains("footer") || desc.contains("magic") {
            code = "ERR_BAD_MAGIC"
        } else {
            code = "ERR_OUT_OF_BOUNDS"
        }
        guard code == expectedCode else {
            throw ConformanceError.mismatch("expected error \(expectedCode), got \(code) (raw: \(e))")
        }
    }
}

enum ConformanceError: Error {
    case mismatch(String)
}

// ── Main ──────────────────────────────────────────────────────────────────────

func runConformance() -> Int32 {
    let conformanceDir = CommandLine.arguments.count > 1
        ? CommandLine.arguments[1]
        : FileManager.default.currentDirectoryPath + "/conformance"

    let fm = FileManager.default
    guard let dirContents = try? fm.contentsOfDirectory(atPath: conformanceDir) else {
        fputs("Cannot read directory: \(conformanceDir)\n", stderr)
        return 1
    }

    let entries = dirContents
        .filter { $0.hasSuffix(".expected.json") }
        .map { String($0.dropLast(".expected.json".count)) }
        .sorted()

    var passed = 0, failed = 0

    for name in entries {
        let jsonPath = "\(conformanceDir)/\(name).expected.json"
        guard let jsonData = fm.contents(atPath: jsonPath),
              let expected = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any]
        else {
            fputs("  FAIL  \(name) — cannot parse expected JSON\n", stderr)
            failed += 1
            continue
        }

        let isNegative = expected["error"] != nil
        do {
            if isNegative {
                let code = expected["error"] as? String ?? ""
                try runNegative(dir: conformanceDir, name: name, expectedCode: code)
            } else {
                try runPositive(dir: conformanceDir, name: name, expected: expected)
            }
            print("  PASS  \(name)")
            passed += 1
        } catch {
            fputs("  FAIL  \(name) — \(error)\n", stderr)
            failed += 1
        }
    }

    print("\n\(passed) passed, \(failed) failed")
    return failed > 0 ? 1 : 0
}

// Top-level entry (only valid when this is the single main file or combined)
let _exitCode = runConformance()
exit(_exitCode)
