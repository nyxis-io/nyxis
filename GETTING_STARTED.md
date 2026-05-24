# Getting Started with NXS

Code examples for all ten language implementations. For the format specification see [`SPEC.md`](./SPEC.md); for the RFC see [`RFC.md`](./RFC.md).

---

## Install

Install from your language's package registry — no build tools required for the pure implementations.

| Language | Registry | Install command |
| :--- | :--- | :--- |
| Rust | crates.io | `cargo add nyxis` |
| JavaScript | npm | `npm install nyxis` |
| Python | PyPI | `pip install nyxis` |
| Go | pkg.go.dev | `go get github.com/nyxis-io/nyxis-drivers/go` |
| Ruby | RubyGems | `gem install nyxis` |
| PHP | Packagist | `composer require nyxis/nyxis` |
| C | GitHub Releases | Download `nxs-c-1.0.0.tar.gz` and include `nxs.h` + `nxs.c` |
| Swift | Swift Package Index | `.package(url: "https://github.com/nyxis-io/nyxis", from: "1.0.0")` |
| Kotlin | Maven Central | `implementation("io.github.nyxis:nyxis:1.0.0")` |
| C# | NuGet | `dotnet add package nyxis` |

---

## The `.nxs` Source Format

Every value is prefixed with a sigil that determines its binary encoding:

```text
user {
    id:         =1024
    active:     ?true
    score:      ~9.81
    name:       "Alex"
    role:       $admin
    created_at: @2026-04-30
    avatar:     <DEADBEEF>
    ref:        &-128
    deleted_at: ^
}
```

| Sigil | Type | Binary Encoding |
| :--- | :--- | :--- |
| `=` | Int64 | `int64_t` LE |
| `~` | Float64 | `double` IEEE 754 LE |
| `?` | Bool | `uint8_t` + 7 bytes padding |
| `$` | Keyword (interned) | `uint16_t` dict index |
| `"` | UTF-8 String | `uint32_t` len + bytes + padding |
| `@` | Timestamp (Unix ns) | `int64_t` LE |
| `<>` | Binary blob | `uint32_t` len + bytes + padding |
| `&` | Link (relative offset) | `int32_t` LE |
| `^` | Null | Zero-width (bitmask bit set) |
| `!` | Macro | Resolved at compile time |

---

## Rust

### Compile `.nxs` to `.nxb`

```bash
cd rust
cargo build --release
./target/release/nxs data.nxs data.nxb
# compiled data.nxs → data.nxb (94208 bytes)
```

### Write `.nxb` directly (hot path — no source text round-trip)

```rust
use nxs::writer::{NxsWriter, Schema, Slot};

let schema = Schema::new(&["id", "username", "score"]);
let mut w = NxsWriter::with_capacity(&schema, records.len() * 128 + 256);
for r in &records {
    w.begin_object();
    w.write_i64(Slot(0), r.id);
    w.write_str(Slot(1), &r.username);
    w.write_f64(Slot(2), r.score);
    w.end_object();
}
let bytes: Vec<u8> = w.finish();
```

### Read `.nxb`

```rust
use nxs::reader::NxsReader;

let data = std::fs::read("data.nxb")?;
let reader = NxsReader::new(&data)?;
println!("{} records", reader.record_count());

let obj = reader.record(42);
let (username, _) = obj.get_str("username").unwrap();
println!("{}", username);
```

### Columnar reducers

```rust
// Safe — handles arbitrary per-record bitmasks
let sum = reader.sum_f64("score");

// Fast — assumes uniform schema (bitmask identical across records)
let sum = reader.sum_f64_fast("score");

// Parallel — fans out across CPU cores
let sum = reader.sum_f64_fast_par("score", 0); // 0 = GOMAXPROCS
```

### Run benchmarks

```bash
cd rust && cargo run --bin bench --release
```

---

## JavaScript (Node.js + Browser)

### Install

No package manager required — `nxs.js` and `nxs_writer.js` are single ES module files.

### Write `.nxb` directly (slot-based hot path)

```js
import { NxsSchema, NxsWriter } from "./nxs_writer.js";

// Precompile schema once — reuse across all writer instances.
const schema = new NxsSchema(["id", "username", "score", "active"]);
const w = new NxsWriter(schema);

for (const r of records) {
    w.beginObject();
    w.writeI64(0, BigInt(r.id));   // slot index; i64 takes a BigInt
    w.writeStr(1, r.username);
    w.writeF64(2, r.score);
    w.writeBool(3, r.active);
    w.endObject();
}
const bytes = w.finish(); // Uint8Array — valid .nxb file
```

### Write `.nxb` — convenience wrapper

```js
// Dict-based convenience: resolves key → slot automatically.
const bytes = NxsWriter.fromRecords(
    ["id", "username", "score"],
    [
        { id: 1n, username: "alice", score: 9.5 },
        { id: 2n, username: "bob",   score: 7.2 },
    ]
);
```

### Read `.nxb`

```js
import { NxsReader } from "./nxs.js";

// Node.js
import { readFileSync } from "node:fs";
const buf = readFileSync("data.nxb");

// Browser
const buf = new Uint8Array(await fetch("data.nxb").then(r => r.arrayBuffer()));

const reader = new NxsReader(buf);
console.log(reader.recordCount);          // 1_000_000 — no parse, O(1) from tail-index

const obj = reader.record(42);            // O(1) seek
console.log(obj.getStr("username"));
console.log(obj.getF64("score"));
console.log(obj.getBool("active"));
```

### Adaptive prefetch

Row layout — warm pages for a viewport before random access (virtual scroll):

```js
await reader.prefetch_viewport(0, 49);
for (let i = 0; i <= 49; i++) {
  console.log(reader.record(i).getStr("username"));
}
```

Columnar layout — one range fetch per column buffer (bypasses the row page cache):

```js
reader.prefetch_column("score");
const sum = reader.colSumF64("score");
```

Pass `fetchRange` in options to count or inject HTTP `Range` fetches in tests and remote I/O.

**Other drivers:** same APIs — `prefetch_viewport` / `prefetch_column` (or language casing: `PrefetchColumn`, `prefetch_column`, `nxs_prefetch_column`). See `nyxis-drivers` per-language READMEs and `docs/adaptive-prefetch.md`.

### Slot handles (hot path)

```js
// Resolve key → slot index once; reuse across records.
const slot = reader.slot("username");
for (let i = 0; i < reader.recordCount; i++) {
    const name = reader.record(i).getStrBySlot(slot);
}
```

### Bulk scan / reducers

```js
// Scan all values for a field (returns Array<number | null>)
const scores = reader.scanF64("score");

// In-JS reducers (no intermediate array)
const sum = reader.sumF64("score");
const min = reader.minF64("score");
const max = reader.maxF64("score");
```

### WASM-accelerated reducers (optional)

```js
import { loadWasm } from "./wasm.js";

// Load once per process / page.
const wasm = await loadWasm("./wasm/nxs_reducers.wasm");
reader.useWasm(wasm);

// Now sumF64 / minF64 / maxF64 run in WASM — ~1.3× faster at 1M records.
const sum = reader.sumF64("score");
```

### Zero-copy WASM (Node.js)

```js
import { loadWasm, readNxbIntoWasm } from "./wasm.js";

const wasm = await loadWasm();
// Reads file bytes directly into WASM memory — no intermediate Buffer copy.
const buf = await readNxbIntoWasm(wasm, "data.nxb");
const reader = new NxsReader(buf);
reader.useWasm(wasm);   // no-op: bytes already resident
reader.sumF64("score"); // 10.9 ms at 1M records
```

### SharedArrayBuffer (Web Workers)

```js
// main.js — serve with python3 server.py (sets COOP/COEP headers)
import { loadWasm } from "./wasm.js";
const wasm = await loadWasm();
const buf = wasm.allocBuffer(nxbBytes.length);  // allocate inside WASM memory
buf.set(nxbBytes);                              // copy once

// Spawn 4 workers — each gets a Uint8Array view of the same SAB.
for (let i = 0; i < 4; i++) {
    new Worker("./nxs_worker.js", { type: "module" })
        .postMessage({ buffer: wasm.memory.buffer, size: buf.length });
}
```

### Browser demos

```bash
cd js && python3 server.py   # needed for SharedArrayBuffer (COOP/COEP headers)
# then open:
# http://localhost:8000/bench.html     — benchmark
# http://localhost:8000/ticker.html    — 60 FPS jank demo
# http://localhost:8000/workers.html   — SharedArrayBuffer workers
# http://localhost:8000/explorer.html  — 10M-line log explorer
# http://localhost:8000/wal.html       — WAL ingestion (5 encoders + cross-language chart)
```

---

## Python

### Write `.nxb` directly (slot-based hot path)

```python
from nxs_writer import NxsSchema, NxsWriter

# Precompile schema once — reuse across all writer instances.
schema = NxsSchema(["id", "username", "score", "active"])
w = NxsWriter(schema)

for r in records:
    w.begin_object()
    w.write_i64(0, r["id"])
    w.write_str(1, r["username"])
    w.write_f64(2, r["score"])
    w.write_bool(3, r["active"])
    w.end_object()

data: bytes = w.finish()  # valid .nxb file
```

### Write `.nxb` — convenience wrapper

```python
# Dict-based convenience: resolves key → slot automatically.
data = NxsWriter.from_records(
    ["id", "username", "score"],
    [
        {"id": 1, "username": "alice", "score": 9.5},
        {"id": 2, "username": "bob",   "score": 7.2},
    ]
)
```

### Pure-Python reader

```python
from nxs import NxsReader

buf = open("data.nxb", "rb").read()  # or mmap.mmap() for true zero-copy
reader = NxsReader(buf)

print(reader.record_count)              # 1_000_000
obj = reader.record(42)                 # O(1) jump via tail-index
print(obj.get_str("username"))
print(obj.get_f64("score"))
print(obj.get_bool("active"))
```

### C extension (hot path)

```bash
cd py && bash build_ext.sh
```

```python
import _nxs   # C extension — same API as NxsReader

reader = _nxs.Reader(buf)
print(reader.record(42).get_str("username"))  # ~374 ns vs ~1.2 µs pure Python
```

### Columnar scan

```python
# Returns a list of all values for one field.
scores = reader.scan_f64("score")

# In-C reducers — no Python object per record.
total = reader.sum_f64("score")    # 3.15 ms at 1M records (9.6× faster than json.loads)
low   = reader.min_f64("score")
high  = reader.max_f64("score")
total_age = reader.sum_i64("age")
```

### Run benchmarks

```bash
cd py
python bench.py             # pure-Python vs json
python bench_c.py           # C extension vs json
```

---

## Go

### Write `.nxb` directly (slot-based hot path)

```go
import "github.com/nyxis-io/nyxis-drivers/go"

// Precompile schema once — reuse across all writer instances.
schema := nxs.NewSchema([]string{"id", "username", "score", "active"})
w := nxs.NewWriter(schema)

for _, r := range records {
    w.BeginObject()
    w.WriteI64(0, r.ID)
    w.WriteStr(1, r.Username)
    w.WriteF64(2, r.Score)
    w.WriteBool(3, r.Active)
    w.EndObject()
}
data := w.Finish() // []byte — valid .nxb file
```

### Write `.nxb` — convenience function

```go
// Map-based convenience: resolves key → slot automatically.
data := nxs.FromRecords(
    []string{"id", "username", "score"},
    []map[string]interface{}{
        {"id": int64(1), "username": "alice", "score": 9.5},
        {"id": int64(2), "username": "bob",   "score": 7.2},
    },
)
```

### Read `.nxb`

```go
import (
    "github.com/nyxis-io/nyxis-drivers/go"
    "os"
)

data, _ := os.ReadFile("data.nxb")
r, err := nxs.NewReader(data)
if err != nil { panic(err) }

fmt.Println(r.RecordCount())

obj := r.Record(42)
username, _ := obj.GetStr("username")
score, _    := obj.GetF64("score")
```

### Slot handles (hot path)

```go
slot := r.Slot("score")
for i := 0; i < r.RecordCount(); i++ {
    v, _ := r.Record(i).GetF64BySlot(slot)
    _ = v
}
```

### Reducers

```go
// Safe — handles any bitmask layout.
sum := r.SumF64("score")

// Fast — uniform schema: bitmask layout computed once from record 0.
sum = r.SumF64Fast("score")

// Parallel — fans out across GOMAXPROCS goroutines.
sum = r.SumF64FastPar("score", 0)   // 0 = use GOMAXPROCS

// Other reducers
min, _ := r.MinF64Fast("score")
max, _ := r.MaxF64Fast("score")
ageSum := r.SumI64Fast("age")
```

### Run benchmarks

```bash
cd go
go build -o bench ./cmd/bench
./bench ../js/fixtures
```

### Generate fixtures

```bash
cd rust && cargo run --release --bin gen_fixtures -- ../js/fixtures 1000000
# generates records_1000000.{nxb,json,csv}
```

---

## Ruby

### Read `.nxb`

```ruby
require_relative "ruby/nxs"

bytes = File.binread("data.nxb")
reader = Nxs::Reader.new(bytes)

puts reader.record_count              # 1_000_000
obj = reader.record(42)               # O(1) jump via tail-index
puts obj.get_str("username")
puts obj.get_f64("score")
puts obj.get_bool("active")
```

### Columnar reducers

```ruby
total = reader.sum_f64("score")
low   = reader.min_f64("score")
high  = reader.max_f64("score")
ages  = reader.sum_i64("age")
```

### Run tests and benchmarks

```bash
ruby ruby/test.rb js/fixtures    # 22/22 tests
ruby ruby/bench.rb js/fixtures
```

---

## PHP

### Read `.nxb`

```php
require_once __DIR__ . '/php/Nxs.php';

$bytes = file_get_contents('data.nxb');
$reader = new Nxs\Reader($bytes);

echo $reader->recordCount() . "\n";   // 1_000_000
$obj = $reader->record(42);            // O(1) jump via tail-index
echo $obj->getStr("username") . "\n";
echo $obj->getF64("score") . "\n";
echo ($obj->getBool("active") ? "true" : "false") . "\n";
```

### Columnar reducers

```php
$total = $reader->sumF64("score");
$low   = $reader->minF64("score");
$high  = $reader->maxF64("score");
$ages  = $reader->sumI64("age");
```

### Run tests and benchmarks

```bash
php php/test.php js/fixtures    # 11/11 tests
php php/bench.php js/fixtures
```

---

## Ruby C Extension

### Build

```bash
bash ruby/ext/build.sh
# Built: ruby/ext/nxs/nxs_ext.bundle
```

### Use

```ruby
require_relative "ruby/ext/nxs/nxs_ext"  # loads Nxs::CReader and Nxs::CObject

bytes = File.binread("data.nxb")
reader = Nxs::CReader.new(bytes)

puts reader.record_count
puts reader.record(42).get_str("username")
puts reader.sum_f64("score")   # C loop — 6.78 ms at 1M records
```

### Benchmark

```bash
ruby ruby/bench_c.rb js/fixtures
```

**At 1M records:** `sum_f64` C ext = **6.78 ms** vs pure Ruby 942 ms (**139× faster**), vs JSON 38 ms (**5.6× faster**)

---

## PHP C Extension

### Build

```bash
bash php/nxs_ext/build.sh
# Built: php/nxs_ext/modules/nxs.so
```

### Use

```php
dl(__DIR__ . '/php/nxs_ext/modules/nxs.so');  // or set extension= in php.ini

$bytes = file_get_contents('data.nxb');
$reader = new NxsReader($bytes);

echo $reader->recordCount() . "\n";
echo $reader->record(42)->getStr("username") . "\n";
echo $reader->sumF64("score") . "\n";  // C loop — 2.00 ms at 1M records
```

### Benchmark

```bash
php -d extension=php/nxs_ext/modules/nxs.so php/bench_c.php js/fixtures
```

**At 1M records:** `sumF64` C ext = **2.00 ms** vs pure PHP 286 ms (**143× faster**), vs `json_decode` 30.7 ms (**15× faster**)

---

## C / C++

Zero-dependency C99 reader. Include `nxs.h` and compile `nxs.c` alongside your code — no build system required.

### Read `.nxb`

```c
#include "nxs.h"
#include <stdio.h>

uint8_t *data = /* mmap or malloc+read */;
size_t   size = /* file size */;

nxs_reader_t r;
nxs_open(&r, data, size);

printf("%u records, %d keys\n", r.record_count, r.key_count);

nxs_object_t obj;
nxs_record(&r, 42, &obj);

int64_t id;    nxs_get_i64(&obj, "id", &id);
double  score; nxs_get_f64(&obj, "score", &score);
int     active; nxs_get_bool(&obj, "active", &active);
char    uname[64]; nxs_get_str(&obj, "username", uname, sizeof(uname));

nxs_close(&r);
```

### Slot handles (hot path)

```c
// Resolve key → integer slot once; reuse across records.
int slot = nxs_slot(&r, "score");
for (uint32_t i = 0; i < r.record_count; i++) {
    nxs_object_t obj;
    nxs_record(&r, i, &obj);
    double v; nxs_get_f64_slot(&obj, slot, &v);
}
```

### Bulk reducers

```c
double  sum = nxs_sum_f64(&r, "score");
int64_t ids = nxs_sum_i64(&r, "id");
double  mn, mx;
nxs_min_f64(&r, "score", &mn);
nxs_max_f64(&r, "score", &mx);
```

### Build and run

```bash
cd c
make test  && ./test ../js/fixtures    # 11/11
make bench && ./bench ../js/fixtures
```

---

## Swift

Swift 5.9+ reader using `Foundation.Data`. No third-party dependencies.

### Read `.nxb`

```swift
import NXS
import Foundation

let data   = try Data(contentsOf: URL(fileURLWithPath: "data.nxb"))
let reader = try NXSReader(data)

print(reader.recordCount)   // 1_000_000
print(reader.keys)          // ["id", "username", ...]

let obj    = try reader.record(42)
let id:     Int64  = try obj.getI64("id")
let score:  Double = try obj.getF64("score")
let active: Bool   = try obj.getBool("active")
let name:   String = try obj.getStr("username")
```

### Slot handles (hot path)

```swift
let scoreSlot = try reader.slot("score")
for i in 0..<reader.recordCount {
    let obj = try reader.record(i)
    let s: Double = try obj.getF64BySlot(scoreSlot)
}
```

### Bulk reducers

```swift
let sum: Double  = try reader.sumF64("score")
let sid: Int64   = try reader.sumI64("id")
let mn:  Double? = try reader.minF64("score")
let mx:  Double? = try reader.maxF64("score")
```

### Build and run

```bash
cd swift
swift run nxs-test ../js/fixtures          # 11/11
swift run -c release nxs-bench ../js/fixtures
```

---

## Kotlin

Kotlin/JVM reader. Requires JDK 17+, Gradle 8+.

### Read `.nxb`

```kotlin
import nxs.NxsReader

val data   = File("data.nxb").readBytes()
val reader = NxsReader(data)

println(reader.recordCount)   // 1_000_000
println(reader.keys)          // [id, username, ...]

val obj    = reader.record(42)
val id:     Long    = obj.getI64("id")
val score:  Double  = obj.getF64("score")
val active: Boolean = obj.getBool("active")
val name:   String  = obj.getStr("username")
```

### Slot handles (hot path)

```kotlin
val scoreSlot = reader.slot("score")
for (i in 0 until reader.recordCount) {
    val s = reader.record(i).getF64BySlot(scoreSlot)
}
```

### Bulk reducers

```kotlin
val sum: Double  = reader.sumF64("score")
val sid: Long    = reader.sumI64("id")
val mn:  Double? = reader.minF64("score")
val mx:  Double? = reader.maxF64("score")
```

### Build and run

```bash
cd kotlin
gradle run --args="../js/fixtures"    # 11/11 tests
gradle bench                          # benchmark vs JSON + CSV
```

---

## C# (.NET)

C# 12 reader targeting .NET 10. No NuGet dependencies.

### Read `.nxb`

```csharp
using Nxs;

byte[] data   = File.ReadAllBytes("data.nxb");
var    reader = new NxsReader(data);

Console.WriteLine(reader.RecordCount);              // 1_000_000
Console.WriteLine(string.Join(", ", reader.Keys));  // id, username, ...

var    obj    = reader.Record(42);
long   id     = obj.GetI64("id");
double score  = obj.GetF64("score");
bool   active = obj.GetBool("active");
string name   = obj.GetStr("username");
```

### Slot handles (hot path)

```csharp
int scoreSlot = reader.Slot("score");
for (int i = 0; i < reader.RecordCount; i++) {
    double s = reader.Record(i).GetF64BySlot(scoreSlot);
}
```

### Bulk reducers

```csharp
double  sum = reader.SumF64("score");
long    sid = reader.SumI64("id");
double? mn  = reader.MinF64("score");
double? mx  = reader.MaxF64("score");
```

### Build and run

```bash
cd csharp
dotnet run -- ../js/fixtures           # 11/11 tests
dotnet run -c Release -- ../js/fixtures --bench
```

## Converters

NXS ships three CLI tools for converting between `.nxb` and JSON/CSV/XML.
See the [Converter Suite spec](context/data/2026-04-30-converter-suite-spec.yaml) for the full flag reference.

### nxs-import — convert to .nxb

```bash
# JSON array of objects → .nxb
nxs-import --from json records.json records.nxb

# CSV → .nxb (auto-detect header, infer types)
nxs-import --from csv data.csv data.nxb

# XML → .nxb (each <item> becomes a record)
nxs-import --from xml --xml-record-tag item data.xml data.nxb

# Read from stdin, write to stdout
cat records.json | nxs-import --from json - -

# Supply a schema hint to skip inference pass (faster; required for stdin)
nxs-import --from json --schema schema.yaml records.json records.nxb
```

### nxs-export — convert from .nxb

```bash
# .nxb → JSON array (compact)
nxs-export --to json records.nxb records.json

# .nxb → JSON (pretty-printed)
nxs-export --to json --pretty records.nxb

# .nxb → newline-delimited JSON (streaming-friendly)
nxs-export --to json --ndjson records.nxb

# .nxb → CSV (schema-key order; override with --columns)
nxs-export --to csv records.nxb records.csv
nxs-export --to csv --columns id,name records.nxb
```

### nxs-inspect — inspect .nxb metadata

```bash
# Human-readable report (schema + first N records)
nxs-inspect records.nxb

# Machine-readable JSON report (schema_version: 1)
nxs-inspect --json records.nxb

# Verify DictHash integrity
nxs-inspect --verify-hash records.nxb
```

### Pipeline demo

```bash
# JSON → .nxb → inspect → export back to JSON
cat records.json \
  | nxs-import --from json - - \
  | tee /tmp/out.nxb \
  | nxs-inspect --json - \
  | jq '.record_count'
nxs-export --to json /tmp/out.nxb
```
