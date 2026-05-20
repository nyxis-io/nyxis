---
room: csharp
subdomain: langs
source_paths: csharp/
see_also: langs/kotlin.md, langs/swift.md
hot_paths: NxsReader.cs, NxsWriter.cs
architectural_health: normal
security_tier: normal
---

# langs/ — C# Implementation

Source paths: csharp/

## TASK → LOAD

| Task | Load |
|------|------|
| Read .nxb files from C# | csharp.md |
| Write .nxb output from C# | csharp.md |
| Run or add C# tests | csharp.md |
| Benchmark C# NXS vs JSON/CSV | csharp.md |
| Run C# conformance checks | csharp.md |

---

# Bench.cs

DOES: C# benchmark comparing `NxsReader.SumF64`, `SumI64`, and random access against `System.Text.Json.JsonDocument` and raw CSV byte scanning at 1M records. Reports best-of-5 millisecond timings.
SYMBOLS:
- Bench.Run(dir: string): void
- BenchMs(label: string, baseline: double, body: Action): double
- JsonSumScore(): double
- CsvSumScore(): double
DEPENDS: System.IO, System.Diagnostics, System.Text.Json, Nxs.NxsReader
PATTERNS: best-of-N Stopwatch benchmark, raw-byte CSV scan
USE WHEN: Measuring C# NXS throughput; invoked by `Program.cs` when `--bench` argument is passed.

---

# ConformanceRunner.cs

DOES: C# conformance runner (namespace `Nxs.Conformance`) that reads `.expected.json` vectors and validates `NxsReader` output field-by-field (positive) or asserts expected error codes (negative). Accesses `_tailStart` via reflection.
SYMBOLS:
- ConformanceRunner.Run(args: string[]): int
- RunPositive(dir, name, expected): void
- RunNegative(dir, name, expectedCode): void
- ValuesMatch(actual, expected: JsonElement): bool
- GetFieldValue(data, tailStart, ri, slot, sigil): object?
- ResolveSlotRaw(data, objOffset, slot): int
- ReadList(data, off): object?[]
DEPENDS: System.Text.Json, System.IO, Nxs.NxsReader
PATTERNS: positive/negative vector dispatch, sigil-driven field decode, reflection for private field access
USE WHEN: Running conformance via `dotnet run -- --conformance ../conformance`.

---

# NxsReader.cs

DOES: C# (.NET 8) NXS binary reader. Parses preamble, schema, and tail-index; provides O(1) `Record()` lookup, `Slot()` resolution, bulk reducers (`SumF64`, `SumI64`, `MinF64`, `MaxF64`), and lazy `NxsObject` field access with `AggressiveInlining` LE reads.
SYMBOLS:
- NxsReader(data: byte[]): NxsReader
- NxsReader.Record(i: int): NxsObject
- NxsReader.Slot(key: string): int
- NxsReader.SumF64 / SumI64(key: string): double / long
- NxsReader.MinF64 / MaxF64(key: string): double?
- NxsReader.ScanOffset(objOffset: int, slot: int): int
- NxsObject.GetI64 / GetF64 / GetBool / GetStr(key: string): typed
- NxsObject.GetI64BySlot / GetF64BySlot / GetBoolBySlot / GetStrBySlot(slot: int): typed
TYPE: NxsException(code: string, msg: string) { Code }
DEPENDS: System.Runtime.CompilerServices, System.Runtime.InteropServices, System.Text
PATTERNS: AggressiveInlining LE reads, LEB128 bitmask walk, MurmurHash3-64, unsafe double reinterpret
USE WHEN: Reading `.nxb` from C#; use `Slot()` + `GetXBySlot()` in bulk loops to skip dictionary lookup per record.

---

# NxsWriter.cs

DOES: C# (.NET 8) NXS writer. Emits `.nxb` into a `MemoryStream` using `BinaryPrimitives` for LE encoding, `Span<byte>` stack allocation, and bitmask/offset-table back-patching. Includes `FromRecords` static factory.
SYMBOLS:
- NxsSchema(keys: string[]): NxsSchema
- NxsWriter(schema: NxsSchema): NxsWriter
- NxsWriter.BeginObject / EndObject(): void
- NxsWriter.Finish(): byte[]
- NxsWriter.WriteI64 / WriteF64 / WriteBool / WriteStr / WriteNull / WriteBytes(slot, value): void
- NxsWriter.WriteListI64 / WriteListF64(slot, values): void
- NxsWriter.FromRecords(keys, records): byte[]
TYPE: NxsSchema { Keys, BitmaskBytes, Count }
TYPE: Frame { Start, Bitmask, OffsetTable, SlotOffsets, LastSlot, NeedsSort }
DEPENDS: System.Buffers.Binary, System.IO, System.Text, Nxs.Murmur3
PATTERNS: BinaryPrimitives LE write, stackalloc Span, two-phase write, MurmurHash3-64
USE WHEN: Writing `.nxb` from C#; `Finish()` returns the complete binary blob.

---

# Program.cs

DOES: C# entry point that dispatches to the conformance runner (`--conformance` flag), the parity/round-trip test suite (default), and the benchmark (`--bench` flag). Validates reader output against JSON fixtures.
SYMBOLS:
- Check(name: string, expr: bool): void
DEPENDS: System.IO, System.Text.Json, Nxs.NxsReader, Nxs.NxsWriter, Nxs.Conformance.ConformanceRunner
PATTERNS: top-level entry dispatch, inline test helpers
USE WHEN: Running `dotnet run -- js/fixtures` for tests, `dotnet run -- js/fixtures --bench` for benchmarks, or `dotnet run -- --conformance ../conformance` for conformance.
