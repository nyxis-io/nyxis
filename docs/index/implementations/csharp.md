---
room: implementations/csharp
source_paths: [csharp/]
file_count: 3
architectural_health: normal
security_tier: normal
hot_paths: [NxsReader.cs]
see_also: [spec_format.md]
---

# Bench.cs

DOES: Benchmarks NXS SumF64/SumI64/random-access against System.Text.Json JsonDocument and raw CSV byte scanning on 1M records; reports best-of-5 times with speedup multipliers.
SYMBOLS:
- static void Run(string dir)
- static double BenchMs(string label, double baseline, Action body)
- static double JsonSumScore()
- static double CsvSumScore()
DEPENDS: System.Text.Json, System.Diagnostics, System.IO, System.Text
PATTERNS: benchmark-suite, format-comparison, stopwatch-timing
USE WHEN: Comparing .NET NXS performance against BCL JSON/CSV; triggered via `dotnet run -- dir --bench`.

---

# NxsReader.cs

DOES: Zero-copy .nxb reader for C# / .NET 10 implementing Nyxis v1.1: tail-index record access, LEB128 bitmask parsing, typed field accessors with AggressiveInlining, and bulk reducers (sum/min/max f64, sum i64).
SYMBOLS:
- sealed class NxsReader
- sealed class NxsObject
- sealed class NxsException(string code, string msg): Exception
- NxsReader#Record(int i): NxsObject
- NxsReader#Slot(string key): int
- NxsReader#SumF64(string key): double
- NxsReader#SumI64(string key): long
- NxsReader#MinF64(string key): double?
- NxsReader#MaxF64(string key): double?
- NxsObject#GetI64(string key): long
- NxsObject#GetF64(string key): double
- NxsObject#GetBool(string key): bool
- NxsObject#GetStr(string key): string
- NxsObject#GetI64BySlot(int slot): long
- NxsObject#GetF64BySlot(int slot): double
- NxsObject#GetBoolBySlot(int slot): bool
- NxsObject#GetStrBySlot(int slot): string
- Types: NxsReader, NxsObject, NxsException
TYPE: NxsReader { Version: ushort, Flags: ushort, DictHash: ulong, TailPtr: ulong, Keys: string[], KeySigils: byte[], RecordCount: int }
DEPENDS: System.Collections.Generic, System.Runtime.CompilerServices, System.Runtime.InteropServices, System.Text
PATTERNS: zero-copy-reader, aggressive-inlining, unsafe-f64-reinterpret, little-endian-helpers
USE WHEN: Reading .nxb records in .NET; use GetBySlot variants on hot paths; SumF64 uses unsafe double* reinterpret for speed.

---

# Program.cs

DOES: Test harness and optional bench entry point: 11 parity checks against JSON fixture (i64, str, f64, bool, OOB, sum), then optionally runs Bench.Run() when `--bench` flag is passed.
SYMBOLS:
- void Check(string name, bool expr)
DEPENDS: Nxs.NxsReader, System.Text.Json
PATTERNS: fixture-parity-test, optional-benchmarking
USE WHEN: Verifying .NET reader correctness; run with `dotnet run -- dir` or `dotnet run -- dir --bench`.
