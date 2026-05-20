---
room: implementations/go
source_paths: [go/]
file_count: 5
architectural_health: normal
security_tier: normal
hot_paths: [fast.go, nxs.go]
see_also: [implementations/c.md, implementations/rust.md, spec/format.md]
---

# cmd/bench/main.go

DOES: Multi-scenario benchmark harness comparing NXS, JSON, and CSV across open, random-access, cold-start, per-record, columnar, and cold-pipeline workloads at 1K–1M records.
SYMBOLS:
- runScale(fixtureDir string, n int)
- timeIt(iters int, fn func()) time.Duration
- sumCsvScore(data []byte) float64
- parseCsvAll(data []byte) ([]record, error)
- Types: record
DEPENDS: nxs, encoding/csv, encoding/json
PATTERNS: benchmark-harness, timing-loop, warmup-pattern, csv-raw-scan
USE WHEN: Comparing NXS reader performance against stdlib JSON/CSV at various scales.

---

# fast.go

DOES: Zero-copy fast-path reducers for uniform-schema datasets: precomputes per-slot bitmask layout once, then scans all records with no per-record bitmask walk. Includes parallel workers and pre-built field index.
SYMBOLS:
- IsUniform() bool
- SumF64Fast(key string) float64
- SumF64FastPar(key string, workers int) float64
- SumI64Fast(key string) int64
- SumI64FastPar(key string, workers int) int64
- MinF64Fast(key string) (float64, bool)
- MaxF64Fast(key string) (float64, bool)
- BuildFieldIndex(key string) (*FieldIndex, bool)
- SumF64Indexed(idx *FieldIndex) float64
- SumI64Indexed(idx *FieldIndex) int64
- Types: fastLayout, FieldIndex
TYPE: fastLayout { bitmaskStart, bitmaskLen, tableIdx, present }
DEPENDS: runtime, sync, unsafe, math
PATTERNS: precomputed-layout, parallel-workergroup, field-index-precompute, unsafe-pointer-math
USE WHEN: Uniform-schema datasets at scale; use BuildFieldIndex when running multiple reducers on the same field (amortises the one-time layout scan).

DISAMBIGUATION: `SumF64Fast` / `SumF64FastPar` are defined here in `fast.go`. The safe fallback `SumF64` is defined in `nxs.go` and handles non-uniform schemas. Use `fast.go` variants when `IsUniform()` returns true.

---

# nxs.go

DOES: Core zero-copy Reader and Object API for .nxb files: validates headers, extracts schema and tail-index, provides typed field accessors with lazy three-stage bitmask caching, and safe bulk reducers.
SYMBOLS:
- NewReader(data []byte) (*Reader, error)
- RecordCount() int
- Slot(key string) int
- Record(i int) *Object
- GetI64(key string) (int64, bool)
- GetF64(key string) (float64, bool)
- GetBool(key string) (bool, bool)
- GetStr(key string) (string, bool)
- GetI64BySlot(slot int) (int64, bool)
- GetF64BySlot(slot int) (float64, bool)
- GetBoolBySlot(slot int) (bool, bool)
- GetStrBySlot(slot int) (string, bool)
- SumF64(key string) float64
- SumI64(key string) int64
- MinF64(key string) (float64, bool)
- MaxF64(key string) (float64, bool)
- Types: Reader, Object
TYPE: Reader { data, Version, Flags, DictHash, TailPtr, Keys, KeySigils, keyIndex, recordCount, tailStart }
TYPE: Object { reader, offset, stage, bitmaskStart, offsetTableStart, present, rank, firstSlot }
DEPENDS: encoding/binary, fmt, math
PATTERNS: lazy-parsing, three-stage-cache, first-access-optimization, bitmask-walk, inline-rank
USE WHEN: Opening .nxb files and accessing typed fields; use GetBySlot variants on hot paths; use fast.go reducers for uniform bulk scans.

DISAMBIGUATION: `SumF64` / `SumI64` / `MinF64` / `MaxF64` are safe implementations defined here. Fast-path variants (uniform schema, parallel) live in `fast.go` (`implementations/go.md`). For mixed schemas or first-time access, load this file.

---

# nxs_test.go

DOES: Comprehensive validation suite: schema parsing, per-record typed access, safe and fast-path reducers, parallel variants, field-index equality, and JSON fixture parity.
SYMBOLS:
- loadFixtures(t *testing.T, n int) ([]byte, []record)
- closeEnough(a, b float64) bool
- TestReaderOpens, TestSchemaKeys, TestRecordsMatchJSON
- TestSumF64, TestSumI64, TestMinMaxF64
- TestIsUniform, TestSumF64FastMatchesSafe
- TestSumF64FastParMatchesSerial, TestFieldIndexMatchesFast
DEPENDS: testing, encoding/json, os, filepath, math
PATTERNS: fixture-loading, cross-codec-validation, fast-safe-equivalence, parallel-parity-check
USE WHEN: Verifying changes to Reader, fast-path reducers, or parallel workers; run with `go test ./...`.

---

# writer.go

DOES: Slot-based .nxb emitter; pre-compiles schema once, writes records with typed methods (I64, F64, Bool, Str, Bytes, List), back-patches headers and offset tables, final assembly with preamble + tail-index.
SYMBOLS:
- Types: Schema, frame, slotOff, Writer
- NewSchema(keys []string) *Schema
- NewWriter(schema *Schema) *Writer
- NewWriterWithCapacity(schema *Schema, cap int) *Writer
- Writer.BeginObject()
- Writer.EndObject()
- Writer.Finish() []byte
- Writer.WriteI64(slot int, v int64)
- Writer.WriteF64(slot int, v float64)
- Writer.WriteBool(slot int, v bool)
- Writer.WriteTime(slot int, unixNs int64)
- Writer.WriteNull(slot int)
- Writer.WriteStr(slot int, v string)
- Writer.WriteBytes(slot int, data []byte)
- Writer.WriteListI64(slot int, values []int64)
- Writer.WriteListF64(slot int, values []float64)
- FromRecords(keys []string, records []map[string]interface{}) []byte
DEPENDS: encoding/binary, math, sort
PATTERNS: back-patch-header, leb128-bitmask-encode, offset-table-sort
USE WHEN: Generating .nxb files; round-trip serialization; supporting various field types with sparse schemas.
