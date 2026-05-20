---
room: implementations/php
source_paths: [php/]
file_count: 7
architectural_health: normal
security_tier: normal
hot_paths: [Nxs.php, nxs_ext/nxs_ext.c]
see_also: [implementations/c.md, spec/format.md]
---

# bench.php

DOES: Benchmarks pure-PHP Nxs\Reader vs json_decode vs CSV across 6 scenarios (open, random-access, cold-start, full-scan, reducer, cold pipeline) at 1K–1M record scales.
SYMBOLS:
- bench(int $iters, callable $fn): float
- fmtNs(float $ns): string
- fmtRatio(float $ns, float $baseline): string
- parseCsv(string $text): array
- printHeader(string $title): void
- printRow(string $label, float $ns, float $baseline): void
DEPENDS: ./Nxs.php
PATTERNS: timing-harness, ratio-formatter, memory-management
USE WHEN: Establishing pure-PHP performance baseline; requires `memory_limit=2G` for 1M JSON.

---

# bench_c.php

DOES: Benchmarks C extension NxsReader vs pure-PHP Nxs\Reader vs json_decode across the same 6-scenario suite with automatic speedup-multiplier reporting.
SYMBOLS:
- bench(int $iters, callable $fn): float
- printRow(string $label, float $ns, float $baseline): void
DEPENDS: ./Nxs.php, nxs_ext/modules/nxs.so
PATTERNS: timing-harness, extension-loader, speedup-reporter
USE WHEN: Demonstrating C extension gains; requires `bash php/nxs_ext/build.sh` first.

---

# nxs_ext/config.h

DOES: Autoconf-generated compile-time flags for the NXS PHP extension (HAVE_STDINT_H, HAVE_DLFCN_H, STDC_HEADERS, etc.).
SYMBOLS: (preprocessor constants only)
DEPENDS: none
PATTERNS: autoconf-config
USE WHEN: Building nxs.so; consumed automatically by nxs_ext.c during compilation.

---

# nxs_ext/nxs_ext.c

DOES: PHP 8 Zend extension implementing NxsReader and NxsObject with zero per-record zval allocation; mirrors py/_nxs.c with inline LEB128 bitmask walk and tight C loops for sumF64/minF64/maxF64/sumI64.
SYMBOLS:
- NxsReader::__construct(string $bytes): void
- NxsReader::recordCount(): int
- NxsReader::keys(): array
- NxsReader::record(int $i): NxsObject
- NxsReader::sumF64(string $key): float
- NxsReader::minF64(string $key): ?float
- NxsReader::maxF64(string $key): ?float
- NxsReader::sumI64(string $key): int
- NxsObject::getStr(string $key): ?string
- NxsObject::getI64(string $key): ?int
- NxsObject::getF64(string $key): ?float
- NxsObject::getBool(string $key): ?bool
- Types: nxs_reader_t, nxs_object_t
TYPE: nxs_reader_t { bytes_zs, data, size, tail_ptr, record_count, tail_start, key_index, keys_zv }
DEPENDS: none
PATTERNS: zend-extension, refcounted-strings, hashtable-schema-cache, inline-bitmask-walk
USE WHEN: Maximum throughput; 3–10× faster than pure PHP on bulk operations.

DISAMBIGUATION: Same API as `Nxs\Reader` in `Nxs.php`. Use this extension for production bulk workloads; use `Nxs.php` for portability or API reference.

---

# nxs_ext/run-tests.php

DOES: PHP test-runner framework harness providing CLI UI, parallel test execution, verbose reporting, and Valgrind memory-leak detection; included as scaffolding for future .phpt test files.
SYMBOLS:
- show_usage(): void
- main(): void
DEPENDS: none
PATTERNS: test-runner-framework, cli-harness
USE WHEN: Adding .phpt-based extension tests; not used directly by the current NXS test suite.

---

# Nxs.php

DOES: Pure-PHP 8.0+ NXS reader: O(1) record access via tail-index, LEB128 bitmask decoding, lazy typed accessors, and bulk reducers (sumF64/minF64/maxF64/sumI64). No external dependencies.
SYMBOLS:
- Nxs\Reader::__construct(string $bytes): void
- Nxs\Reader::recordCount(): int
- Nxs\Reader::keys(): array
- Nxs\Reader::record(int $i): Nxs\NxsObject
- Nxs\Reader::sumF64(string $key): float
- Nxs\Reader::minF64(string $key): ?float
- Nxs\Reader::maxF64(string $key): ?float
- Nxs\Reader::sumI64(string $key): int
- Nxs\NxsObject::getStr(string $key): ?string
- Nxs\NxsObject::getI64(string $key): ?int
- Nxs\NxsObject::getF64(string $key): ?float
- Nxs\NxsObject::getBool(string $key): ?bool
- Types: Nxs\Reader, Nxs\NxsObject
TYPE: Nxs\Reader { bytes, recordCount, keys, keyIndex, tailStart }
DEPENDS: none
PATTERNS: lazy-parsing, leb128-decoding, little-endian-io, sparse-field-bitmask
USE WHEN: Portable reader without extension dependencies; also the canonical API reference.

DISAMBIGUATION: There is also `NxsReader` in `nxs_ext/nxs_ext.c` with the same API and 3–10× higher throughput. Use `Nxs.php` for portability; use the extension for bulk workloads.

---

# test.php

DOES: Parity validation suite: 11 test cases covering reader construction, record count, schema keys, typed accessors (string, i64, f64, bool), bounds checking, and aggregate correctness vs JSON fixture.
SYMBOLS:
- check(string $label, bool $ok, string $detail): void
DEPENDS: ./Nxs.php
PATTERNS: fixture-parity-test, golden-comparison
USE WHEN: Validating PHP reader correctness; run with `php test.php js/fixtures`.
