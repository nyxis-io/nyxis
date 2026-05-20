---
room: implementations/ruby
source_paths: [ruby/]
file_count: 6
architectural_health: normal
security_tier: normal
hot_paths: [nxs.rb, ext/nxs/nxs_ext.c]
see_also: [implementations/c.md, spec/format.md]
---

# bench.rb

DOES: Benchmarks pure-Ruby Nxs::Reader vs JSON.parse vs CSV across 6 scenarios (open, random-access, cold-start, full-scan, reducer, cold pipeline) at 1K–1M record scales.
SYMBOLS:
- bench(iters) -> Float
- fmt_time(s) -> String
- fmt_bytes(n) -> String
- run_scale(fixture_dir, n) -> nil
DEPENDS: ./nxs.rb
PATTERNS: benchmark-suite, parametric-scaling
USE WHEN: Establishing pure-Ruby performance baselines; run with `ruby bench.rb js/fixtures`.

---

# bench_c.rb

DOES: Benchmarks Nxs::CReader (C extension) vs Nxs::Reader vs JSON with parity checks validating correctness of both implementations.
SYMBOLS:
- bench(iters) -> Float
- run_parity(fixture_dir) -> nil
- run_scale(fixture_dir, n) -> nil
DEPENDS: ./nxs.rb, ./ext/nxs/nxs_ext
PATTERNS: benchmark-suite, parity-tests
USE WHEN: Demonstrating C extension speedup; requires `bash ruby/ext/build.sh` first.

---

# ext/nxs/extconf.rb

DOES: MRI C extension Makefile generator; configures compilation of nxs_ext.c into the Nxs module shared library.
SYMBOLS:
- (mkmf DSL only)
DEPENDS: none
PATTERNS: native-extension-config
USE WHEN: Building the C extension via `ruby extconf.rb && make`.

---

# ext/nxs/nxs_ext.c

DOES: MRI C extension exposing Nxs::CReader and Nxs::CObject with zero-alloc bulk reducers (sum/min/max f64, sum i64); implements LEB128 bitmask walk, lazy object header parse, and prefix-sum rank table.
SYMBOLS:
- Nxs::CReader#initialize(bytes)
- Nxs::CReader#record_count() -> Integer
- Nxs::CReader#keys() -> Array<String>
- Nxs::CReader#record(i) -> Nxs::CObject
- Nxs::CReader#sum_f64(key) -> Float
- Nxs::CReader#min_f64(key) -> Float|nil
- Nxs::CReader#max_f64(key) -> Float|nil
- Nxs::CReader#sum_i64(key) -> Integer
- Nxs::CObject#get_str(key) -> String|nil
- Nxs::CObject#get_i64(key) -> Integer|nil
- Nxs::CObject#get_f64(key) -> Float|nil
- Nxs::CObject#get_bool(key) -> Boolean|nil
- Types: NxsReader (C struct), NxsObject (C struct)
DEPENDS: none
PATTERNS: zend-typeddata, leb128-bitmask-walk, lazy-object-parse, zero-alloc-reducers, memcpy-unaligned-reads
USE WHEN: Production workloads; 5–20× faster than pure Ruby on bulk scans.

DISAMBIGUATION: Same API as `nxs.rb` (`Nxs::Reader`). Use `Nxs::CReader` from this extension for throughput; use `Nxs::Reader` from nxs.rb for portability or API reference.

---

# nxs.rb

DOES: Pure-Ruby NXS reader: O(1) record access via tail-index, LEB128 bitmask decoding, lazy typed field accessors, and bulk reducers (sum/min/max f64, sum i64).
SYMBOLS:
- Nxs::Reader#initialize(bytes)
- Nxs::Reader#record(i) -> Nxs::Object
- Nxs::Reader#sum_f64(key) -> Float
- Nxs::Reader#min_f64(key) -> Float|nil
- Nxs::Reader#max_f64(key) -> Float|nil
- Nxs::Reader#sum_i64(key) -> Integer
- Nxs::Object#get_str(key) -> String|nil
- Nxs::Object#get_i64(key) -> Integer|nil
- Nxs::Object#get_f64(key) -> Float|nil
- Nxs::Object#get_bool(key) -> Boolean|nil
- Types: Nxs::Reader, Nxs::Object, Nxs::NxsError
DEPENDS: none
PATTERNS: leb128-bitmask-walk, lazy-object-parse, zero-alloc-reducers
USE WHEN: Portable reader without C compilation; canonical API reference for the Ruby interface.

DISAMBIGUATION: There is also `Nxs::CReader` in `ext/nxs/nxs_ext.c` with the same API and 5–20× higher throughput. Use this file for portability or development; use the C extension for production bulk workloads.

---

# test.rb

DOES: Parity tests for Nxs::Reader across 22 cases: record count, schema keys, string/int/float/bool extraction, missing keys, OOB errors, iteration, and sum_f64 correctness vs JSON fixture.
SYMBOLS:
- check(label, &blk) -> Boolean
DEPENDS: ./nxs.rb
PATTERNS: parity-tests, fixture-driven
USE WHEN: Validating pure-Ruby reader; run with `ruby test.rb js/fixtures`.
