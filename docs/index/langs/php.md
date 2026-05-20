---
room: php
subdomain: langs
source_paths: php/
see_also: langs/ruby.md, c/reader.md
hot_paths: Nxs.php, nxs_ext.c
architectural_health: normal
security_tier: sensitive
committee_notes: nxs_ext.c uses raw Zend object lifecycle with ecalloc and inline bitmask walks over PHP string data; review carefully when modifying.
---

# langs/ — PHP Implementation

Source paths: php/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files in pure PHP | php.md |
| Write .nxb from PHP | php.md |
| Use the PHP C extension for max throughput | php.md |
| Benchmark PHP NXS vs JSON | php.md |
| Build the PHP C extension | php.md |
| Run PHP tests | php.md |

---

# bench.php

DOES: Six-scenario PHP benchmark comparing `Nxs\Reader`, `json_decode`, and CSV `str_getcsv` across four fixture sizes (1k–1M). Runs formats sequentially with GC cycles between loads to stay within memory limits.
SYMBOLS:
- bench(int $iters, callable $fn): float
- parseCsv(string $text): array
- fmtNs(float $ns): string
- fmtRatio(float $ns, float $baseline): string
DEPENDS: Nxs.php
PATTERNS: benchmark-harness, memory-aware sequential loading
USE WHEN: Benchmarking the pure-PHP reader; use bench_c.php for a three-way comparison including the C extension.

---

# bench_c.php

DOES: Three-way PHP benchmark comparing the `NxsReader` C extension, `Nxs\Reader` (pure PHP), and `json_decode` across six scenarios and four sizes. Reports speedup multipliers.
SYMBOLS:
- bench(int $iters, callable $fn): float
DEPENDS: Nxs.php, nxs extension (nxs.so)
PATTERNS: benchmark-harness, dl() extension loading
USE WHEN: Quantifying C-extension speedup over pure PHP and JSON; requires `bash php/nxs_ext/build.sh` first.

---

# gen_stub.php

DOES: PHP stub generator that reads extension function/method signatures and emits a `nxs_ext.stub.php` file used by the PHP build system for IDE completion and arginfo generation.
SYMBOLS:
- (CLI script — no exported symbols)
PATTERNS: php-extension-stub-gen
USE WHEN: Regenerating stubs after adding new methods to the C extension.

---

# Nxs.php

DOES: Pure-PHP 8.0+ NXS binary reader (namespace `Nxs`). Parses preamble, schema, and tail-index; exposes `Reader` with O(1) record lookup, bulk reducers, and lazy `NxsObject` field decoding via LEB128 bitmask walk.
SYMBOLS:
- Reader::__construct(string $bytes): Reader
- Reader::record(int $i): NxsObject
- Reader::recordCount(): int
- Reader::keys(): array
- Reader::sumF64(string $key): float
- Reader::minF64(string $key): ?float
- Reader::maxF64(string $key): ?float
- Reader::sumI64(string $key): int
- Reader::slotOf(string $key): int
- NxsObject::getStr / getI64 / getF64 / getBool(string $key): mixed
- leb128(string $bytes, int &$pos): int
TYPE: NxsException extends RuntimeException
DEPENDS: gmp (for MurmurHash3-64)
PATTERNS: lazy bitmask parse, inline valueOffset kernel, MurmurHash3-64 via GMP
USE WHEN: Reading `.nxb` files in pure PHP; use the C extension (`NxsReader`) when throughput is critical.

---

# nxs_ext.c

DOES: PHP 8 Zend extension exposing `NxsReader` and `NxsObject` classes. Implements schema/tail-index parsing, lazy per-object bitmask decode, and zero-zval bulk reducers (`sumF64`, `minF64`, `maxF64`, `sumI64`) as tight C loops.
SYMBOLS:
- NxsReader::__construct(string $bytes): void
- NxsReader::recordCount(): int
- NxsReader::keys(): array
- NxsReader::record(int $i): NxsObject
- NxsReader::sumF64 / minF64 / maxF64 / sumI64(string $key): mixed
- NxsObject::getStr / getI64 / getF64 / getBool(string $key): mixed
TYPE: nxs_reader_t { bytes_zs, data, size, tail_ptr, record_count, tail_start, key_index, keys_zv }
TYPE: nxs_object_t { reader_zv, offset, present, rank, present_len, offset_table_start, parsed }
DEPENDS: php.h, ext/standard/info.h, zend_exceptions.h
PATTERNS: Zend object lifecycle (create/free), ecalloc TypedData, inline scan_field_offset, HashTable key_index
USE WHEN: Maximum PHP throughput; loaded via `php -d extension=nxs.so` or system install.

---

# NxsWriter.php

DOES: Pure-PHP 8.0+ NXS writer (namespace `Nxs`). Emits conformant `.nxb` files via a `Schema`-precompiled, slot-indexed API with bitmask and offset-table back-patching. MurmurHash3-64 uses GMP.
SYMBOLS:
- Schema::__construct(array $keys): Schema
- Writer::__construct(Schema $schema): Writer
- Writer::beginObject / endObject(): void
- Writer::finish(): string
- Writer::writeI64 / writeF64 / writeBool / writeStr / writeNull / writeBytes(int $slot, mixed $v): void
- Writer::writeListI64 / writeListF64(int $slot, array $values): void
- Writer::fromRecords(array $keys, array $records): string
TYPE: Schema { keys, bitmaskBytes }
TYPE: _Frame { start, bitmask, offsetTable, slotOffsets, lastSlot, needsSort }
DEPENDS: gmp
PATTERNS: two-phase write (placeholder + back-patch), LEB128 bitmask, MurmurHash3-64 via GMP
USE WHEN: Writing `.nxb` data from PHP without a C extension; `finish()` returns a binary string ready for storage.

---

# run-tests.php

DOES: PHP extension test harness runner that executes `.phpt` test files against the built `nxs.so` extension, checking stdout/stderr against expected output.
SYMBOLS:
- (CLI script — no exported symbols)
PATTERNS: phpt-test-runner
USE WHEN: Running `.phpt` integration tests after building the PHP C extension.

---

# test.php

DOES: Parity and round-trip test suite validating `Nxs\Reader` and `Nxs\Writer` against the 1000-record JSON fixture. Covers field access, bulk reducers, error codes (ERR_BAD_MAGIC, ERR_DICT_MISMATCH), and multi-byte bitmask.
SYMBOLS:
- check(string $label, bool $ok, string $detail): void
DEPENDS: Nxs.php, NxsWriter.php
PATTERNS: fixture-based parity testing
USE WHEN: Verifying the PHP implementation; run via `php php/test.php js/fixtures`.
