---
room: conformance/runners
source_paths: [conformance/]
file_count: 21
architectural_health: normal
security_tier: normal
hot_paths: [generate.rs]
see_also: [implementations/rust.md, spec/format.md]
---

# generate.rs

DOES: Generates all NXS conformance test vectors as .nxb and .expected.json pairs, covering positive cases (minimal, all_sigils, nested, large, unicode, lists, null_vs_absent, sparse, max_keys, jumbo_string) and negative error cases (bad_magic, bad_dict_hash, truncated).
SYMBOLS:
- fn main()
- struct Vector
- enum JV
- fn make_minimal() -> Vector
- fn make_nested() -> Vector
- fn make_large() -> Vector
- fn make_bad_magic() -> (Vec<u8>, String)
DEPENDS: nxs::writer, nxs::error
PATTERNS: conformance-vector-generation
USE WHEN: Regenerating test vectors after spec changes; run via `cargo run --bin gen_conformance -- conformance/`.

---

# run_c.c

DOES: Validates NXS conformance using the C99 reader; loads .nxb files, decodes records, and compares against .expected.json specifications including list field support.
SYMBOLS:
- int main(int argc, char **argv)
- int run_positive(const char *dir, const char *name, jv_t *exp)
- int run_negative(const char *dir, const char *name, const char *expected_code)
- jv_t *json_parse(const char *s)
PATTERNS: conformance-runner
USE WHEN: Testing C reader compliance; exits 0 on full pass, 1 on any failure.

---

# run_csharp.cs

DOES: Validates NXS conformance via the C# NxsReader API; compares record counts, keys, and field values against expected outputs; handles list deserialization and approximate float equality.
SYMBOLS:
- public static int Run(string[] args)
- static void RunPositive(string dir, string name, JsonElement expected)
- static void RunNegative(string dir, string name, string expectedCode)
- static bool ValuesMatch(object? actual, JsonElement expected)
PATTERNS: conformance-runner
USE WHEN: Testing C# reader compliance.

---

# run_go.go

DOES: Conformance test runner for Go; decodes .nxb files and validates records against expected JSON with support for list types and floating-point approximation.
SYMBOLS:
- func main()
- func runPositive(conformanceDir, name string, exp expected) error
- func runNegative(conformanceDir, name, expectedCode string) error
- func getFieldValue(data []byte, reader *nxs.Reader, tailStart, ri, slot int, sigilByte byte) (interface{}, bool)
PATTERNS: conformance-runner
USE WHEN: Testing Go reader compliance.

---

# run_js.js

DOES: Node.js conformance runner; loads NXS binaries, decodes records, and validates against expected JSON including list fields and sigil-based type decoding.
SYMBOLS:
- function runPositive(name, nxbPath, expected)
- function runNegative(name, nxbPath, expectedCode)
- function getFieldValue(obj, reader, key, sigil)
- function valuesMatch(actual, expected)
PATTERNS: conformance-runner
USE WHEN: Testing JavaScript reader compliance.

---

# run_kotlin.kt

DOES: Kotlin conformance runner using the NxsReader API; validates record structure and field values with raw data access for list decoding.
SYMBOLS:
- fun main(args: Array<String>)
- fun runPositive(dir: String, name: String, expected: Map<String, Any>)
- fun runNegative(dir: String, name: String, expectedCode: String)
- fun getFieldValue(data: ByteArray, tailStart: Int, ri: Int, slot: Int, sigil: Byte): Any?
PATTERNS: conformance-runner
USE WHEN: Testing Kotlin/JVM reader compliance.

---

# run_php.php

DOES: PHP conformance runner; decodes NXS binary files and validates records against expected JSON using type-specific accessors and raw binary parsing for lists.
SYMBOLS:
- function runPositive(string $conformanceDir, string $name, array $expected): void
- function runNegative(string $conformanceDir, string $name, string $expectedCode): void
- function getFieldValue(string $bytes, int $tailStart, int $ri, int $slot, int $sigil): mixed
- function resolveSlotRaw(string $bytes, int $objOffset, int $slot): int
PATTERNS: conformance-runner
USE WHEN: Testing PHP reader compliance.

---

# run_py.py

DOES: Python conformance runner; validates .nxb decodes against expected JSON with list type support and floating-point tolerances.
SYMBOLS:
- def main()
- def run_positive(conformance_dir: str, name: str, expected: dict) -> None
- def run_negative(conformance_dir: str, name: str, expected_code: str) -> None
- def get_field_value(obj, reader, key)
PATTERNS: conformance-runner
USE WHEN: Testing Python reader compliance.

---

# run_ruby.rb

DOES: Ruby conformance runner; loads NXS binaries and validates record structure, keys, and field values via type-aware comparison and raw binary data access.
SYMBOLS:
- def main
- def run_positive(conformance_dir, name, expected)
- def run_negative(conformance_dir, name, expected_code)
- def get_field_value(data, reader, tail_start, ri, slot, sigil_byte)
PATTERNS: conformance-runner
USE WHEN: Testing Ruby reader compliance.

---

# run_rust.rs

DOES: Rust conformance runner; reads .expected.json test vectors, decodes corresponding .nxb files via the nxs decoder, and validates record structure and field values with approximate float comparison.
SYMBOLS:
- fn main()
- fn run_positive(dir: &Path, name: &str, expected_json: &Jv) -> Result<(), String>
- fn run_negative(dir: &Path, name: &str, expected_code: &str) -> Result<(), String>
- fn decoded_matches(decoded: &DecodedValue, expected: &Jv) -> bool
DEPENDS: nxs::decoder, nxs::error
PATTERNS: conformance-runner
USE WHEN: Testing Rust reader compliance; this runner is the reference implementation.

---

# run_swift.swift

DOES: Swift conformance runner; loads NXS binary files, decodes records using NXSReader, and validates against expected JSON including list field support.
SYMBOLS:
- func runConformance() -> Int32
- func runPositive(dir: String, name: String, expected: [String: Any]) throws
- func runNegative(dir: String, name: String, expectedCode: String) throws
- func readListFromReader(reader: NXSReader, ri: Int, key: String) -> [Any]?
PATTERNS: conformance-runner
USE WHEN: Testing Swift reader compliance on macOS.

---

# vectors/

DOES: Test vector collection of 14 paired .nxb/.expected.json files covering positive cases (minimal, all_sigils, null_vs_absent, sparse, nested, list_i64, list_f64, unicode_strings, large, max_keys, jumbo_string) and negative error cases (bad_magic, bad_dict_hash, truncated).
SYMBOLS:
- all_sigils.{nxb,expected.json}
- bad_dict_hash.{nxb,expected.json}
- bad_magic.{nxb,expected.json}
- jumbo_string.{nxb,expected.json}
- large.{nxb,expected.json}
- list_f64.{nxb,expected.json}
- list_i64.{nxb,expected.json}
- max_keys.{nxb,expected.json}
- minimal.{nxb,expected.json}
- nested.{nxb,expected.json}
- null_vs_absent.{nxb,expected.json}
- sparse.{nxb,expected.json}
- truncated.{nxb,expected.json}
- unicode_strings.{nxb,expected.json}
PATTERNS: conformance-test-vectors
USE WHEN: Running conformance tests; regenerate with `cargo run --bin gen_conformance -- conformance/`.
