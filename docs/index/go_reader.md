---
room: go_reader
subdomain: (top-level)
source_paths: go/
see_also: ["c_reader.md", "langs/kotlin.md"]
hot_paths: nxs.go, fast.go, writer.go
architectural_health: normal
security_tier: normal
---

# go/ — Go Reader & Writer

Subdomain: go/
Source paths: go/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files from Go | reader.md |
| Write .nxb output from Go | reader.md |
| Use fast unsafe-pointer aggregate reducers | reader.md |
| Run Go tests or add new test cases | reader.md |
| Benchmark Go NXS vs JSON/CSV | reader.md |

---

# bench_wal_test.go

DOES: Benchmarks NxsWriter write throughput against `encoding/json` (json.Marshal) using a realistic distributed-tracing WAL workload of up to 100k spans. Reports best-of-three ns/span and k-spans/s for both serializers.
SYMBOLS:
- runWalNxs(n int) float64
- runWalJSON(n int) float64
- TestWalBench(t *testing.T)
- BenchmarkWalNxs10k(b *testing.B)
- BenchmarkWalJSON10k(b *testing.B)
- Types: spanRecord
DEPENDS: encoding/json, testing, time
PATTERNS: WAL-append benchmark, best-of-N timing
USE WHEN: Measuring write-path throughput of NxsWriter vs JSON for streaming/telemetry workloads; not for read-path benchmarks (use cmd/bench/main.go).

---

# cmd/bench/main.go

DOES: CLI benchmark comparing NXS, `encoding/json`, and `encoding/csv` across six scenarios (open-file, warm random access, cold start, full scan, columnar reducers, cold pipeline) at fixture sizes 1k–1M records.
SYMBOLS:
- main()
- runScale(fixtureDir string, n int)
- timeIt(iters int, fn func()) time.Duration
- sumCsvScore(data []byte) float64
- parseCsvAll(data []byte) ([]record, error)
- fmtDur(d time.Duration) string
- fmtBytes(n int) string
- withCommas(n int) string
- Types: record
DEPENDS: github.com/nyxis-io/nyxis-drivers/go, encoding/json, encoding/csv
PATTERNS: warmup + timed-loop harness, multi-scenario benchmark matrix
USE WHEN: End-to-end performance comparison across formats; use bench_wal_test.go instead for write-path WAL benchmarks.

---

# fast.go

DOES: Provides unsafe-pointer fast-path reducers (SumF64Fast, SumI64Fast, MinF64Fast, MaxF64Fast) and their parallel goroutine variants, plus a FieldIndex pre-computation path, all assuming a uniform per-record bitmask layout.
SYMBOLS:
- (r *Reader) IsUniform() bool
- (r *Reader) computeFastLayout(slot int) fastLayout
- (r *Reader) SumF64Fast(key string) float64
- (r *Reader) SumI64Fast(key string) int64
- (r *Reader) MinF64Fast(key string) (float64, bool)
- (r *Reader) MaxF64Fast(key string) (float64, bool)
- (r *Reader) SumF64FastPar(key string, workers int) float64
- (r *Reader) SumI64FastPar(key string, workers int) int64
- (r *Reader) BuildFieldIndex(key string) (*FieldIndex, bool)
- (r *Reader) SumF64Indexed(idx *FieldIndex) float64
- (r *Reader) SumI64Indexed(idx *FieldIndex) int64
- (r *Reader) MinF64Indexed(idx *FieldIndex) (float64, bool)
- (r *Reader) MaxF64Indexed(idx *FieldIndex) (float64, bool)
- Types: fastLayout, FieldIndex
DEPENDS: math, runtime, sync, unsafe
PATTERNS: unsafe-pointer LE load, goroutine fan-out with WaitGroup, pre-built offset index
USE WHEN: Uniform-schema datasets and maximum aggregate throughput; use nxs.go SumF64/SumI64 for heterogeneous schemas or when safety is required.

---

# nxs.go

DOES: Core zero-copy .nxb reader: validates preamble and footer magics, parses the embedded schema (key names, sigils) and tail-index location, then exposes lazy per-record Object accessors and allocation-free bulk reducers (SumF64, SumI64, MinF64, MaxF64).
SYMBOLS:
- NewReader(data []byte) (*Reader, error)
- (r *Reader) RecordCount() int
- (r *Reader) Slot(key string) int
- (r *Reader) Record(i int) *Object
- (r *Reader) SumF64(key string) float64
- (r *Reader) SumI64(key string) int64
- (r *Reader) MinF64(key string) (float64, bool)
- (r *Reader) MaxF64(key string) (float64, bool)
- NewStreamWriter(out io.Writer, schema *Schema) (*StreamWriter, error)
- (o *Object) GetI64(key string) (int64, bool)
- (o *Object) GetF64(key string) (float64, bool)
- (o *Object) GetBool(key string) (bool, bool)
- (o *Object) GetStr(key string) (string, bool)
- (o *Object) GetI64BySlot(slot int) (int64, bool)
- (o *Object) GetF64BySlot(slot int) (float64, bool)
- (o *Object) GetBoolBySlot(slot int) (bool, bool)
- (o *Object) GetStrBySlot(slot int) (string, bool)
- murmur3_64(data []byte) uint64
- Types: Reader, Object
DEPENDS: encoding/binary, fmt, math
PATTERNS: lazy Object stage promotion, LEB128 bitmask walk, tail-index O(1) record lookup, inline-rank single-field fast path
USE WHEN: Reading existing .nxb files; use fast.go variants for aggregate scans over uniform datasets.
DISAMBIGUATION: `murmur3_64` also appears in `compiler.rs` (`rust/compiler_pipeline.md`) and `nxs.c` (`c_reader.md`). The Go implementation is used only at reader open-time for DictHash verification. For the canonical hash algorithm spec, load `rust/compiler_pipeline.md`.

---

# nxs_test.go

DOES: Unit and integration tests for the Reader and Writer, covering schema validation, per-field correctness against JSON fixtures, error paths (bad magic, truncation, dict-hash mismatch), and correctness parity between safe and fast reducers.
SYMBOLS:
- TestReaderOpens(t *testing.T)
- TestSchemaKeys(t *testing.T)
- TestRecordsMatchJSON(t *testing.T)
- TestSumF64(t *testing.T)
- TestSumI64(t *testing.T)
- TestMinMaxF64(t *testing.T)
- TestBadMagic(t *testing.T)
- TestTruncatedFile(t *testing.T)
- TestDictHashMismatch(t *testing.T)
- TestWriterRoundTrip(t *testing.T)
- TestWriterFromRecords(t *testing.T)
- TestIsUniform(t *testing.T)
- TestSumF64FastMatchesSafe(t *testing.T)
- TestSumI64FastMatchesSafe(t *testing.T)
- TestSumF64FastParMatchesSerial(t *testing.T)
DEPENDS: encoding/json, math, os, testing
PATTERNS: fixture-driven table tests, safe-vs-fast parity assertions
USE WHEN: Running `go test ./...`; fixtures from js/fixtures must be generated first via `make fixtures`.

---

# writer.go

DOES: Implements NxsWriter for emitting .nxb binary files from Go: precompiles a Schema once, uses slot-indexed write methods per object, and assembles the full preamble + schema + data + tail-index on Finish.
SYMBOLS:
- NewSchema(keys []string) *Schema
- (s *Schema) Len() int
- NewWriter(schema *Schema) *Writer
- NewWriterWithCapacity(schema *Schema, cap int) *Writer
- (w *Writer) BeginObject()
- (w *Writer) EndObject()
- (w *Writer) Finish() []byte
- (w *Writer) WriteI64(slot int, v int64)
- (w *Writer) WriteF64(slot int, v float64)
- (w *Writer) WriteBool(slot int, v bool)
- (w *Writer) WriteStr(slot int, v string)
- (w *Writer) WriteBytes(slot int, data []byte)
- (w *Writer) WriteListI64(slot int, values []int64)
- (w *Writer) WriteListF64(slot int, values []float64)
- (w *Writer) WriteTime(slot int, unixNs int64)
- (w *Writer) WriteNull(slot int)
- FromRecords(keys []string, records []map[string]interface{}) []byte
- Types: Schema, Writer, frame, slotOff
DEPENDS: encoding/binary, math, sort
PATTERNS: two-pass back-patch, LEB128 bitmask with continuation bits, frame stack for nested objects
USE WHEN: Writing new .nxb files from Go; prefer NewWriterWithCapacity when record count is known to reduce allocations.
