# Contributing to NXS

NXS is an experimental format spec with reference implementations. Contributions are welcome in three categories: **spec clarifications**, **new language implementations**, and **bug fixes in existing implementations**.

---

## Reporting spec ambiguities

If the behaviour of a conformant parser is underspecified, or if two implementations disagree on how to handle an edge case, open an issue describing:

1. The section of `SPEC.md` or `RFC.md` that is ambiguous.
2. The two or more reasonable interpretations.
3. Which interpretation you believe is correct and why.

Spec issues take priority over implementation issues because they affect every language at once.

---

## Adding a new language implementation

A conformant reader must:

1. Parse the 32-byte preamble and validate `Magic` and footer `Magic`.
2. Read the embedded schema header (`KeyCount`, `TypeManifest`, `StringPool`).
3. Parse the tail-index to support O(1) `record(i)` access.
4. Decode the LEB128 bitmask and offset table per object to support typed field access.
5. Expose at minimum: `record_count`, `keys`, `record(i)`, `get_i64`, `get_f64`, `get_bool`, `get_str`.
6. Pass the standard smoke-test suite against `js/fixtures/records_1000.nxb` and `records_1000.json`.

### Test requirements

Your test file must cover:

- Opens without error
- Correct `record_count`
- Schema keys present (`id`, `username`, `score`, `active`)
- `record(0).get_i64("id")` matches JSON
- `record(42).get_str("username")` matches JSON
- `record(500).get_f64("score")` matches JSON within 1e-6
- `record(999).get_bool("active")` matches JSON
- Out-of-bounds `record(10000)` raises an error
- Iteration over all 1000 records
- `sum_f64("score")` matches JSON sum within 1e-4

### Directory layout

```
<lang>/
  nxs.<ext>          # reader library
  test.<ext>         # smoke tests (run: <lang cmd> test.<ext> ../js/fixtures)
  bench.<ext>        # benchmark vs JSON + CSV baseline
  README.md          # build instructions and API reference
```

### Generating fixtures

```bash
cd rust && cargo run --release --bin gen_fixtures -- ../js/fixtures 1000
# also: 10000, 100000, 1000000
```

---

## Fixing bugs in existing implementations

- Match the fix to the failing test. If no test covers the bug, add one.
- Do not add features beyond what the bug fix requires.
- Run the full test suite for the affected language before submitting.

### Running all test suites

```bash
# Rust
cd rust && cargo test

# JavaScript
cd js && node test.js ../js/fixtures

# Python
cd py && python test_nxs.py ../js/fixtures

# Go
cd go && go test ./...

# Ruby
ruby ruby/test.rb js/fixtures

# PHP
php php/test.php js/fixtures

# C
cd c && make test && ./test ../js/fixtures

# Swift
cd swift && swift run nxs-test ../js/fixtures

# Kotlin
cd kotlin && gradle run --args="../js/fixtures"

# C#
cd csharp && dotnet run -- ../js/fixtures
```

---

## Code style

Each implementation follows the idiomatic conventions of its language. There is no cross-language style enforcement. The only universal rules are:

- Bounds-check every offset before dereferencing it.
- Never silently ignore an out-of-bounds read — return an error or throw.
- Do not add third-party dependencies to a reader implementation without strong justification. The existing implementations use only standard library facilities.
