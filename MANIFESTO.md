# The NXS Manifesto

---

## The parsing tax is a choice we keep making

Every time a service reads a million-row dataset, it pays the same toll: allocate memory for every string, convert every number from text, decode the whole file before touching a single field. We have accepted this as the cost of interoperability. It is not. It is the cost of formats designed before memory mapping existed as a practical tool.

JSON was designed to flow over HTTP between browsers and servers. It does that beautifully. It was not designed to be an in-memory query layer for 1.5 GB of records in a tab that needs to stay responsive at 60 frames per second. We are using a transit format as a storage format, and we are paying for it at runtime, constantly, in milliseconds that add up to seconds.

CSV strips the types out entirely and calls it simplicity. Protocol Buffers puts the schema in a separate file, generates code you have to check in, and still doesn't give you random record access. FlatBuffers gets close — zero-copy within a single object — but the format does not define a cross-record index. Without one, a multi-record file has no standard path to record 800,000 that doesn't involve reading what came before it.

None of them were wrong. They solved the problem they were aimed at. NXS is aimed at a different problem.

---

## What we actually want

A format that a human can write, read in a diff, and commit to version control — without ceremony, without schema registries, without generated code.

A format whose open cost is constant with respect to how many records it contains — touching only the preamble and tail-index on open, not the data sector.

A format where reading one field from record 800,000 does not require loading records 1 through 799,999.

A format that is safe to hand to multiple threads and web workers simultaneously, without copying.

A format where absent is not the same as null, because they are not the same thing.

These are not contradictions. They are the design brief.

---

## The two representations

NXS is bi-modal. The text format (`.nxs`) is a sigil-typed source language. The binary format (`.nxb`) is what the machine reads.

In `.nxs`, every value declares its type with a single leading character — its sigil. `=` for Int64, `~` for Float64, `?` for Bool, `"` for String, `@` for Timestamp, `$` for interned Keyword, `<>` for raw Binary, `&` for Link, `!` for compile-time Macro, `^` for explicit Null. No quotes around keys unless you want them. No trailing commas. No schema file to maintain separately. The source is the schema.

```
user {
    id:     =1024
    name:   "Alex"
    active: ?true
    score:  ~98.6
    joined: @1735689600000000000
}
```

The Rust compiler reads this text and writes a `.nxb` file. That file is what all ten language implementations consume. It is not compiled once and read once. It is compiled once and read arbitrarily — O(1) record lookup via the tail-index, O(1) field access once a record is located — across whatever languages your stack happens to use.

---

## Why the binary works

Three decisions make the binary format fast enough to matter.

**8-byte alignment.** Every atomic value — integer, float, timestamp — sits at a file offset divisible by 8. A memory-mapped `.nxb` file can be cast directly to typed pointers without a copy-to-aligned-buffer step. The processor loads it natively. SIMD loops over lists need no realignment pass. Bool fields waste 7 bytes of padding. That is the honest cost of the guarantee, and it is worth it.

**The Tail-Index.** The final footer of every sealed `.nxb` file points to an index that holds one `(KeyID u16, AbsoluteOffset u64)` pair per top-level record. To read record 14 million: seek to `EOF - 12`, read the footer pointer, jump to the index, read that record's 10-byte entry, follow its absolute offset to the data. The rest of the file has not been touched. Open time is constant with respect to data sector size — it touches only the preamble and tail-index metadata, regardless of how many records the file contains.

**The LEB128 bitmask.** Each object header carries a variable-width presence mask. A set bit means the field is there; a clear bit means it was never written. Sparse objects carry no overhead for absent fields. When every record in a file shares the same schema — the common case — the bitmask and offset table are identical for every record after the first, and implementations can skip parsing them entirely.

---

## What the numbers say

On 1M records, across ten implementations, on the same machine:

Opening and reading one field from a 1.5 GB file: under 2 microseconds for NXS. JSON throws `Invalid string length`. CSV runs out of memory.

A Go cold pipeline — open the file, sum a float column over 1M records — takes 13.5 ms in NXS versus 1.05 seconds parsing JSON. 78× faster, not because of tricks, because the format was designed for the task.

Kotlin's `sumF64` runs in 4.3 ms against 1,286 ms from org.json — 296× faster. Swift's `sumF64` runs in 8.2 ms against 2,038 ms from JSONSerialization — 249× faster. C#'s `SumF64` runs in 8.8 ms against 292 ms from System.Text.Json — 33× faster. Across every language, the pattern holds: the format's access model matches the workload.

In the browser, at 60 FPS, NXS patches a value in place as a direct byte write. JSON re-parses the full payload on every frame. Fanning out to 4 Web Workers costs 25 µs for NXS versus 68 ms for JSON — a 2,700× difference — because NXS passes a SharedArrayBuffer pointer; JSON structured-clones 57 MB per worker.

These are not cherrypicked microbenchmarks. They reflect what happens when a format's access model matches the workload.

---

## What this is not

NXS is not a replacement for JSON over HTTP. JSON is excellent there.

NXS is not a database. It has no query planner, no indexing beyond the tail-index, no transaction model.

NXS is not a columnar analytics engine. Apache Arrow and Parquet are better choices when your workload is full-column aggregates over millions of rows in a pipeline with a query planner. NXS is better when the unit of access is a record — configuration, event logs, entity stores — and when the environment can't absorb a runtime engine dependency.

NXS v1.0 is complete. The spec is stable, the binary format is frozen, and all ten language implementations pass the conformance test suite. The design held up under real implementation pressure.

---

## The design rules

**The source format is authoritative.** `.nxs` files live in version control. The binary is a build artifact.

**The binary format is the contract.** Any conformant reader in any language reads the same bytes the same way. The spec is the contract, not any specific implementation.

**Absent and null are different.** A field with no bitmask bit is absent — it was never written. A field with a bitmask bit pointing to `0x00` is null — it was explicitly set to nothing. These have different semantics. Implementations that conflate them are wrong.

**Alignment is not optional.** The Rule of 8 applies to every atomic value, in every file, with no exceptions. This is what makes zero-copy access to aligned atomic values safe.

**Bounds checking is not optional.** Every offset from an offset table, the tail-index, or any in-file pointer must be validated before the memory access. An out-of-bounds offset is an error; a conformant parser does not attempt recovery.

---

## Ten languages, one format

The reference implementations cover Rust, JavaScript, Python, Go, Ruby, PHP, C, Swift, Kotlin, and C#. Each reads the same `.nxb` file. Each exposes the same lookup model: resolve a key to a slot index once, reuse it across all records. Each provides columnar reducers for aggregate queries. Python, Ruby, and PHP also ship C extensions for maximum throughput. The JavaScript implementation works in Node, in the browser, and in Web Workers sharing a `SharedArrayBuffer` with zero bytes copied between threads.

The point is not comprehensiveness for its own sake. The point is that a `.nxb` file is not owned by one ecosystem. It is a shared artifact. The writer can be Rust; the readers can be anything.

---

## An invitation

The spec is in `SPEC.md`. The RFC with security guidance and implementation notes is in `RFC.md`. Working code for all ten languages is in this repository. The browser demos — live at [nxs.covibe.us](https://nxs.covibe.us/index.html) — show the format under conditions that motivated the design: 14 million records, 60 FPS frame updates, 4 workers sharing one buffer, virtual scroll over 10 million log lines.

Implement it, extend it, tell us where the spec is ambiguous.

---

*NXS — Nyxis. Stable v1.0. April 2026.*
