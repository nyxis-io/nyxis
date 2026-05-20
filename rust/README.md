# NXS — Rust

The reference compiler (`.nxs` → `.nxb`) and core library. Also provides a direct binary writer for generating `.nxb` without a source text round-trip.

## Requirements

Rust 1.75+ (stable).

## Build

```bash
cd rust
cargo build --release
```

## Compile `.nxs` to `.nxb`

```bash
./target/release/nxs data.nxs           # writes data.nxb
./target/release/nxs data.nxs out.nxb   # explicit output path
```

## Write `.nxb` directly

For bulk generation — no source text round-trip:

```rust
use nxs::writer::{NxsWriter, Schema, Slot};

let schema = Schema::new(&["id", "username", "score", "active"]);
let mut w = NxsWriter::with_capacity(&schema, records.len() * 128 + 256);
for r in &records {
    w.begin_object();
    w.write_i64(Slot(0), r.id);
    w.write_str(Slot(1), &r.username);
    w.write_f64(Slot(2), r.score);
    w.write_bool(Slot(3), r.active);
    w.end_object();
}
let bytes: Vec<u8> = w.finish();
```

## Tests

```bash
cargo test
cargo test test_compile_basic   # single test by name
```

## Benchmarks

```bash
cargo run --release --bin bench
```

## Generate fixtures

All language benchmarks share a fixture directory. Generate before running cross-language benchmarks:

```bash
cargo run --release --bin gen_fixtures -- ../js/fixtures 1000000
# writes records_1000000.{nxb,json,csv}
```

## Source layout

| File | Purpose |
| :--- | :--- |
| `src/main.rs` | `nxs` binary — CLI entry point |
| `src/lexer.rs` | Tokenizes `.nxs` source (sigils, keys, braces, brackets) |
| `src/parser.rs` | Builds an AST of `Field { key, value }` nodes |
| `src/compiler.rs` | Two-pass compiler: key dictionary, then binary emission |
| `src/writer.rs` | `NxsWriter` / `Schema` / `Slot` — direct binary writer API |
| `src/decoder.rs` | Minimal decoder used by tests |
| `src/error.rs` | `NxsError` enum |
| `src/bench.rs` | `bench` binary |
| `src/gen_fixtures.rs` | `gen_fixtures` binary |

## Query engine

```rust
use nxs::query::{Reader, And, eq, gt, not};

let data = std::fs::read("data.nxb")?;
let reader = Reader::new(&data)?;

// Count matching records
let n = reader.where_pred(And(eq("active", true), gt("score", 80.0f64))).count();

// Iterate — early termination via short-circuit
for record in reader.where_pred(eq("active", true)) {
    println!("{:?}", record.get_str("username"));
}

// First match
if let Some(rec) = reader.all().next() {
    println!("{:?}", rec.get_str("username"));
}

// Nested path
let city = record.get_str_path("address.city");
```

### Predicates

| Function | Matches |
|----------|---------|
| `eq(key, value)` | equality — bool, &str, i64, f64 |
| `gt(key, v)` / `lt(key, v)` | numeric comparison |
| `And(p1, p2)` | both pass |
| `Or(p1, p2)` | either passes |
| `Not(p)` | does not pass |

Zero-allocation hot path — predicate evaluation reads directly from `&[u8]` buffer.

---

For the format specification see [`SPEC.md`](../SPEC.md). For cross-language examples see [`GETTING_STARTED.md`](../GETTING_STARTED.md).
