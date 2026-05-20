---
room: conformance
subdomain: (top-level)
source_paths: conformance/
see_also: rust/tests_fuzz.md
hot_paths: generate.rs, run_rust.rs
architectural_health: normal
security_tier: normal
---

# Conformance Suite

Source paths: conformance/

## TASK → LOAD

| Task | Load |
|------|------|
| Generate or regenerate conformance test vectors | conformance.md |
| Run conformance against a specific language | conformance.md |
| Add a new positive or negative test vector | conformance.md |
| Understand the expected JSON format for vectors | conformance.md |
| Debug conformance failures in any language | conformance.md |

---

# generate.rs

DOES: Generates all NXS conformance test vectors as `.nxb` + `.expected.json` pairs. Produces ten positive vectors covering minimal, all sigils, null-vs-absent, sparse, lists, unicode, large, max keys, and jumbo strings, plus three negative vectors for bad magic, bad dict hash, and truncated files.
SYMBOLS:
- make_minimal() -> Vector
- make_all_sigils() -> Vector
- make_null_vs_absent() -> Vector
- make_sparse() -> Vector
- make_nested() -> Vector
- make_list_i64() -> Vector
- make_list_f64() -> Vector
- make_unicode_strings() -> Vector
- make_large() -> Vector
- make_max_keys() -> Vector
- make_jumbo_string() -> Vector
- make_bad_magic() -> (Vec<u8>, String)
- make_bad_dict_hash() -> (Vec<u8>, String)
- make_truncated() -> (Vec<u8>, String)
- expected_json(keys: &[&str], records: &[Vec<Option<(&str, JV)>>]) -> String
- Types: JV, Vector
TYPE: Vector { name: &'static str, nxb: Vec<u8>, expected: String }
TYPE: JV { Null | Bool(bool) | Int(i64) | Float(f64) | Str(String) | Array(Vec<JV>) | Object(Vec<(String, JV)>) }
DEPENDS: nxs::writer, nxs::compiler, nxs::lexer, nxs::parser, nxs::decoder
PATTERNS: conformance-vector-generation, two-phase-encode-then-serialize
USE WHEN: Regenerating the canonical `.nxb` test fixtures; use `make_nested` when the compiler path (not `NxsWriter`) must be exercised.

---

# run_c.c

DOES: C99 conformance runner that discovers every `.expected.json` in a directory, loads the matching `.nxb` via `nxs_open`, and validates record counts, key names, and all field values including list fields decoded by walking the raw `NYXL` header. Negative vectors assert the correct `NXS_ERR_*` code.
SYMBOLS:
- run_positive(dir: const char*, name: const char*, exp: jv_t*) -> int
- run_negative(dir: const char*, name: const char*, expected_code: const char*) -> int
- json_parse(s: const char*) -> jv_t*
- jv_free(v: jv_t*) -> void
- read_file(path: const char*, out_size: size_t*) -> uint8_t*
TYPE: jv_t { type: jv_type_t; union { bval, ival, fval, sval, arr, obj } }
DEPENDS: nxs.h, c/nxs.c
PATTERNS: hand-rolled-json-parser, raw-bitmask-list-decode, directory-scan-runner
USE WHEN: Running conformance on the C implementation; the only runner that avoids any libc JSON dependency and instead uses a self-contained recursive-descent parser.

---

# run_csharp.cs

DOES: C# conformance runner (namespace `Nxs.Conformance`) that iterates `.expected.json` files, calls `NxsReader` for positive vectors and asserts `NxsException` error codes for negative vectors. Uses reflection to access private `_tailStart` and manually walks bitmask/offset-table for list and raw-field decoding.
SYMBOLS:
- ConformanceRunner.Run(args: string[]) -> int
- RunPositive(dir: string, name: string, expected: JsonElement) -> void
- RunNegative(dir: string, name: string, expectedCode: string) -> void
- ValuesMatch(actual: object?, expected: JsonElement) -> bool
- ResolveSlotRaw(data: byte[], objOffset: int, slot: int) -> int
- ReadList(data: byte[], off: int) -> object?[]
- GetFieldValue(data: byte[], tailStart: int, ri: int, slot: int, sigil: byte) -> object?
DEPENDS: Nxs.NxsReader, Nxs.NxsException, System.Text.Json, System.Reflection
PATTERNS: reflection-private-field-access, manual-bitmask-walk, json-element-comparison
USE WHEN: Running conformance under the C# implementation; prefer over `dotnet run` integration tests when validating raw binary layout independently of the reader abstraction.

---

# run_go.go

DOES: Go conformance runner (build-tag `ignore`, run with `go run`) that opens `.nxb` files via `nxs.NewReader`, validates record count, key ordering, and per-field values for every positive vector, and checks error strings for negative vectors. Manually resolves slot offsets via bitmask walk to support list fields.
SYMBOLS:
- runPositive(conformanceDir: string, name: string, exp: expected) -> error
- runNegative(conformanceDir: string, name: string, expectedCode: string) -> error
- getFieldValue(data: []byte, reader: *nxs.Reader, tailStart: int, ri: int, slot: int, sigilByte: byte) -> (interface{}, bool)
- resolveSlotRaw(data: []byte, objOffset: int, slot: int) -> int
- readList(data: []byte, off: int) -> (interface{}, bool)
TYPE: expected { RecordCount *int; Keys []string; Records []map[string]interface{}; Error string }
DEPENDS: github.com/nyxis-io/nyxis-drivers/go, encoding/binary, encoding/json
PATTERNS: bitmask-slot-resolution, list-magic-detection, go-run-ignore-build-tag
USE WHEN: Running conformance against the Go reader; the `//go:build ignore` tag means it is invoked with `go run`, not `go test`.

---

# run_js.js

DOES: Node.js ESM conformance runner that loads `NxsReader` from `js/nxs.js`, iterates sorted `.expected.json` files, validates record count, key names, and per-field typed values (using slot-sigil dispatch), and checks `NxsError.code` for negative vectors.
SYMBOLS:
- runPositive(name: string, nxbPath: string, expected: object) -> void
- runNegative(name: string, nxbPath: string, expectedCode: string) -> void
- getFieldValue(obj: NxsObject, reader: NxsReader, key: string, sigil: number) -> any
- approxEq(a: number, b: number) -> boolean
- valuesMatch(actual: any, expected: any) -> boolean
DEPENDS: ../js/nxs.js, node:fs, node:path
PATTERNS: esm-import, sigil-dispatch, bigint-coercion
USE WHEN: Running conformance against the JavaScript implementation; requires Node.js with ESM support.

---

# run_kotlin.kt

DOES: Kotlin JVM conformance runner that uses `NxsReader` from the built Gradle jar, parses expected JSON via Jackson (with `org.json` fallback), validates all record fields with type-aware comparison, and walks raw bitmask/offset tables to support list decoding and reflection-based `tailStart` access.
SYMBOLS:
- runPositive(dir: String, name: String, expected: Map<String, Any>) -> Unit
- runNegative(dir: String, name: String, expectedCode: String) -> Unit
- getFieldValue(data: ByteArray, tailStart: Int, ri: Int, slot: Int, sigil: Byte) -> Any?
- resolveSlotRaw(data: ByteArray, objOffset: Int, slot: Int) -> Int
- readList(data: ByteArray, off: Int) -> List<Any>?
- main(args: Array<String>) -> Unit
DEPENDS: nxs.NxsReader, nxs.NxsError, java.nio.ByteBuffer, com.fasterxml.jackson.databind.ObjectMapper
PATTERNS: reflection-tailStart, jackson-reflection-fallback, jvm-little-endian-bytebuffer
USE WHEN: Running conformance on the Kotlin/JVM implementation; requires the Gradle-assembled jar on the classpath.

---

# run_php.php

DOES: PHP 8 conformance runner that loads `Nxs\Reader` from `php/Nxs.php`, validates record count, keys, and typed field values for positive vectors, and checks `NxsException` message strings for negative vectors. Uses reflection to access private `tailStart` and `keyIndex`, and manually unpacks binary list headers with `unpack()`.
SYMBOLS:
- runPositive(conformanceDir: string, name: string, expected: array) -> void
- runNegative(conformanceDir: string, name: string, expectedCode: string) -> void
- getFieldValue(bytes: string, tailStart: int, ri: int, slot: int, sigil: int) -> mixed
- resolveSlotRaw(bytes: string, objOffset: int, slot: int) -> int
- readList(bytes: string, off: int) -> array|null
DEPENDS: Nxs\Reader, Nxs\NxsException, ReflectionClass
PATTERNS: php-unpack-binary, reflection-private-property, sentinel-PHP_INT_MAX-for-absent
USE WHEN: Running conformance against the PHP implementation; PHP `unpack('q')` requires 64-bit PHP for correct i64 decoding.

---

# run_py.py

DOES: Python 3 conformance runner that imports `NxsReader` and `NxsError` from `py/nxs.py`, validates record count, keys, and per-field values across all positive vectors, and asserts `NxsError.code` for negative vectors. Decodes list fields by inspecting the raw `NYXL` magic via `struct` unpack on the reader's memoryview.
SYMBOLS:
- run_positive(conformance_dir: str, name: str, expected: dict) -> None
- run_negative(conformance_dir: str, name: str, expected_code: str) -> None
- get_field_value(obj: NxsObject, reader: NxsReader, key: str) -> Any
- read_list(mv: memoryview, off: int) -> list
- main() -> None
DEPENDS: py.nxs, struct, json, os, sys
PATTERNS: memoryview-zero-copy, sigil-dispatch, struct-unpack-list
USE WHEN: Running conformance against the Python implementation; uses `reader.mv` (memoryview) for list access without copying the full buffer.

---

# run_ruby.rb

DOES: Ruby conformance runner that requires `ruby/nxs.rb`, validates record count, key order, and typed field values for positive vectors, and checks `Nxs::NxsError#code` for negative vectors. Uses `String#unpack1` with format codes to decode binary fields and accesses `@tail_start` and `@key_sigils` via `instance_variable_get`.
SYMBOLS:
- run_positive(conformance_dir: String, name: String, expected: Hash) -> void
- run_negative(conformance_dir: String, name: String, expected_code: String) -> void
- get_field_value(data: String, reader: Nxs::Reader, tail_start: Integer, ri: Integer, slot: Integer, sigil_byte: Integer) -> Object
- resolve_slot_raw(data: String, obj_offset: Integer, slot: Integer) -> Integer|nil
- read_list(data: String, off: Integer) -> Array|nil
DEPENDS: ruby/nxs, json
PATTERNS: ruby-unpack1-binary, instance_variable_get-internals, frozen-string-literal
USE WHEN: Running conformance against the Ruby implementation; `:absent` sentinel distinguishes missing fields from null fields.

---

# run_rust.rs

DOES: Rust conformance runner that calls `nxs::decoder::decode` on each `.nxb` vector, validates schema keys and first-record field values against expected JSON using a self-contained recursive-descent JSON parser, and maps `NxsError` variants to canonical error code strings for negative vector assertions.
SYMBOLS:
- run_positive(dir: &Path, name: &str, expected_json: &Jv) -> Result<(), String>
- run_negative(dir: &Path, name: &str, expected_code: &str) -> Result<(), String>
- parse_json(s: &str) -> Jv
- decoded_matches(decoded: &DecodedValue, expected: &Jv) -> bool
- Types: Jv
TYPE: Jv { Null | Bool(bool) | Int(i64) | Float(f64) | Str(String) | Array(Vec<Jv>) | Object(Vec<(String, Jv)>) }
DEPENDS: nxs::decoder, nxs::error::NxsError
PATTERNS: self-contained-json-parser, decoder-first-record-only, NxsError-code-mapping
USE WHEN: Running conformance against the Rust decoder; note this runner only validates the first record of multi-record vectors because `decode` returns `root_fields` only.

---

# run_swift.swift

DOES: Swift conformance runner that initialises `NXSReader` from `swift/Sources/NXS/NXSReader.swift` compiled together, validates record count, keys, and per-field typed values, and maps `NXSError` descriptions to canonical error code strings for negative vectors. Decodes list fields by reading raw `NYXL` headers from a module-level `_currentConformanceFileData` global.
SYMBOLS:
- runConformance() -> Int32
- runPositive(dir: String, name: String, expected: [String: Any]) throws -> void
- runNegative(dir: String, name: String, expectedCode: String) throws -> void
- readListFromReader(reader: NXSReader, ri: Int, key: String) -> [Any]?
- resolveSlotSwift(data: Data, objOffset: Int, slot: Int) -> Int?
- approxEq(_ a: Double, _ b: Double) -> Bool
- valuesMatch(_ actual: Any?, _ expected: Any?) -> Bool
TYPE: ConformanceError { mismatch(String) }
DEPENDS: NXSReader, NXSError, Foundation
PATTERNS: global-file-data-workaround, nxserror-string-inspection, swiftc-combined-compile
USE WHEN: Running conformance against the Swift implementation; must be compiled together with `NXSReader.swift` since Swift scripts cannot import local modules via `import`.
