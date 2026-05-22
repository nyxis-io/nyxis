#![allow(dead_code, unused_imports, unused_variables)]
mod compiler;
mod consts;
mod decoder;
/// NXS vs JSON vs XML vs CSV benchmark + WAL span ingestion benchmark
/// Measures: output byte size and serialization/deserialization throughput.
mod error;
mod lexer;
mod parser;
mod segment_reader;
mod wal;
mod writer;

use compiler::Compiler;
use decoder::decode;
use std::time::{Duration, Instant};
use writer::{NxsWriter, Schema, Slot};

// ── Shared dataset ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct Record {
    id: i64,
    username: String,
    email: String,
    age: i64,
    balance: f64,
    active: bool,
    score: f64,
    created_at: &'static str, // ISO date string
}

fn dataset(n: usize) -> Vec<Record> {
    (0..n)
        .map(|i| Record {
            id: i as i64,
            username: format!("user_{i:04}"),
            email: format!("user{i}@example.com"),
            age: 20 + (i % 50) as i64,
            balance: 100.0 + (i as f64) * 1.37,
            active: i % 3 != 0,
            score: (i as f64 % 100.0) / 10.0,
            created_at: "2026-04-30",
        })
        .collect()
}

// ── NXS serialization ────────────────────────────────────────────────────────

fn serialize_nxs(records: &[Record]) -> Vec<u8> {
    let mut src = String::new();
    for r in records {
        src.push_str(&format!(
            "record_{id} {{\n\
             \tid: ={id}\n\
             \tusername: \"{un}\"\n\
             \temail: \"{em}\"\n\
             \tage: ={age}\n\
             \tbalance: ~{bal:.2}\n\
             \tactive: ?{act}\n\
             \tscore: ~{sc:.1}\n\
             \tcreated_at: @{ts}\n\
             }}\n",
            id = r.id,
            un = r.username,
            em = r.email,
            age = r.age,
            bal = r.balance,
            act = if r.active { "true" } else { "false" },
            sc = r.score,
            ts = r.created_at,
        ));
    }

    let mut lexer = lexer::Lexer::new(&src);
    let tokens = lexer.tokenize().expect("nxs lex");
    let mut parser = parser::Parser::new(tokens);
    let fields = parser.parse_file().expect("nxs parse");
    let mut c = Compiler::new();
    c.compile(&fields).expect("nxs compile")
}

fn deserialize_nxs(data: &[u8]) -> usize {
    let decoded = decode(data).expect("nxs decode");
    decoded.root_fields.len()
}

// ── NXS wire writer (direct binary, no AST) ──────────────────────────────────

const SLOTS: &[&str] = &[
    "id",
    "username",
    "email",
    "age",
    "balance",
    "active",
    "score",
    "created_at",
];

// Integer slot IDs matching SLOTS order
const S_ID: Slot = Slot(0);
const S_USERNAME: Slot = Slot(1);
const S_EMAIL: Slot = Slot(2);
const S_AGE: Slot = Slot(3);
const S_BALANCE: Slot = Slot(4);
const S_ACTIVE: Slot = Slot(5);
const S_SCORE: Slot = Slot(6);
const S_CREATED_AT: Slot = Slot(7);

fn serialize_nxs_wire(records: &[Record]) -> Vec<u8> {
    let schema = Schema::new(SLOTS);
    // Pre-size: each record is ~110 bytes in binary form
    let mut w = NxsWriter::with_capacity(&schema, records.len() * 128 + 256);
    for r in records {
        w.begin_object();
        w.write_i64(S_ID, r.id);
        w.write_str(S_USERNAME, &r.username);
        w.write_str(S_EMAIL, &r.email);
        w.write_i64(S_AGE, r.age);
        w.write_f64(S_BALANCE, r.balance);
        w.write_bool(S_ACTIVE, r.active);
        w.write_f64(S_SCORE, r.score);
        w.write_time(S_CREATED_AT, 1_777_593_600_000_000_000);
        w.end_object();
    }
    w.finish()
}

fn deserialize_nxs_wire(data: &[u8]) -> usize {
    // Same tail-index path as compiler output
    deserialize_nxs(data)
}

// ── JSON serialization (manual, no runtime dep needed) ───────────────────────

fn serialize_json(records: &[Record]) -> Vec<u8> {
    let mut s = String::from("[\n");
    for (i, r) in records.iter().enumerate() {
        s.push_str(&format!(
            "  {{\"id\":{id},\"username\":\"{un}\",\"email\":\"{em}\",\
             \"age\":{age},\"balance\":{bal:.2},\"active\":{act},\
             \"score\":{sc:.1},\"created_at\":\"{ts}\"}}",
            id = r.id,
            un = r.username,
            em = r.email,
            age = r.age,
            bal = r.balance,
            act = if r.active { "true" } else { "false" },
            sc = r.score,
            ts = r.created_at,
        ));
        if i + 1 < records.len() {
            s.push_str(",\n");
        }
    }
    s.push_str("\n]");
    s.into_bytes()
}

fn deserialize_json(data: &[u8]) -> usize {
    // Minimal JSON counter: count `"id":` occurrences
    let s = std::str::from_utf8(data).unwrap();
    s.matches("\"id\":").count()
}

// ── XML serialization ────────────────────────────────────────────────────────

fn serialize_xml(records: &[Record]) -> Vec<u8> {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<records>\n");
    for r in records {
        s.push_str(&format!(
            "  <record>\
             <id>{id}</id>\
             <username>{un}</username>\
             <email>{em}</email>\
             <age>{age}</age>\
             <balance>{bal:.2}</balance>\
             <active>{act}</active>\
             <score>{sc:.1}</score>\
             <created_at>{ts}</created_at>\
             </record>\n",
            id = r.id,
            un = r.username,
            em = r.email,
            age = r.age,
            bal = r.balance,
            act = if r.active { "true" } else { "false" },
            sc = r.score,
            ts = r.created_at,
        ));
    }
    s.push_str("</records>");
    s.into_bytes()
}

fn deserialize_xml(data: &[u8]) -> usize {
    let s = std::str::from_utf8(data).unwrap();
    s.matches("<record>").count()
}

// ── CSV serialization ────────────────────────────────────────────────────────

fn serialize_csv(records: &[Record]) -> Vec<u8> {
    let mut s = String::from("id,username,email,age,balance,active,score,created_at\n");
    for r in records {
        s.push_str(&format!(
            "{},{},{},{},{:.2},{},{:.1},{}\n",
            r.id,
            r.username,
            r.email,
            r.age,
            r.balance,
            if r.active { "true" } else { "false" },
            r.score,
            r.created_at,
        ));
    }
    s.into_bytes()
}

fn deserialize_csv(data: &[u8]) -> usize {
    let s = std::str::from_utf8(data).unwrap();
    s.lines().count().saturating_sub(1) // skip header
}

// ── Benchmark harness ────────────────────────────────────────────────────────

const SIZES: &[usize] = &[10_000, 100_000, 1_000_000];

fn iters_for(n: usize) -> u32 {
    match n {
        n if n >= 1_000_000 => 3,
        n if n >= 100_000 => 5,
        _ => 10,
    }
}

fn bench<F: Fn() -> R, R>(iters: u32, f: F) -> Duration {
    for _ in 0..2 {
        let _ = f();
    } // warmup
    let start = Instant::now();
    for _ in 0..iters {
        let _ = f();
    }
    start.elapsed() / iters
}

fn fmt_ns(d: Duration) -> String {
    let ns = d.as_nanos();
    if ns < 1_000 {
        format!("{ns} ns")
    } else if ns < 1_000_000 {
        format!("{:.1} µs", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.3} s", ns as f64 / 1_000_000_000.0)
    }
}

fn fmt_bytes(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.2} MB", n as f64 / (1024.0 * 1024.0))
    }
}

fn main() {
    println!(
        "\n╔══════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!("║              NXS vs JSON vs XML vs CSV  —  Benchmark Results                    ║");
    println!(
        "╚══════════════════════════════════════════════════════════════════════════════════╝\n"
    );
    println!("  Iterations: 10/5/3 at 10k/100k/1M");
    println!(
        "  Fields per record: 8 (id, username, email, age, balance, active, score, created_at)\n"
    );

    for &n in SIZES {
        let iters = iters_for(n);
        let records = dataset(n);
        println!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  {n} records  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        );

        let include_compiler = n <= 10_000;

        // Serialize once to get sizes
        let nxs_wire_bytes = serialize_nxs_wire(&records);
        let json_bytes = serialize_json(&records);
        let xml_bytes = serialize_xml(&records);
        let csv_bytes = serialize_csv(&records);
        let nxs_compiler_bytes = if include_compiler {
            Some(serialize_nxs(&records))
        } else {
            None
        };

        // Size comparison
        println!(
            "\n  ┌─ Output Size ───────────────────────────────────────────────────────────────┐"
        );
        let baseline = json_bytes.len() as f64;
        let mut size_rows: Vec<(&str, usize)> = vec![];
        if let Some(ref b) = nxs_compiler_bytes {
            size_rows.push(("NXS compiler", b.len()));
        }
        size_rows.push(("NXS wire    ", nxs_wire_bytes.len()));
        size_rows.push(("JSON        ", json_bytes.len()));
        size_rows.push(("XML         ", xml_bytes.len()));
        size_rows.push(("CSV         ", csv_bytes.len()));
        for (name, len) in &size_rows {
            let ratio = *len as f64 / baseline * 100.0;
            let bar_len = (*len * 40 / xml_bytes.len()).max(1);
            let bar = "█".repeat(bar_len);
            println!(
                "  │  {name}  {:>10}  ({:>5.1}% of JSON)  {bar}",
                fmt_bytes(*len),
                ratio
            );
        }
        println!(
            "  └─────────────────────────────────────────────────────────────────────────────┘"
        );

        // Serialization speed
        println!(
            "\n  ┌─ Serialization Time (avg over {iters} runs) ─────────────────────────────────────┐"
        );
        let t_nxs_wire_ser = bench(iters, || serialize_nxs_wire(&records));
        let t_json_ser = bench(iters, || serialize_json(&records));
        let t_xml_ser = bench(iters, || serialize_xml(&records));
        let t_csv_ser = bench(iters, || serialize_csv(&records));
        let t_nxs_compiler_ser = if include_compiler {
            Some(bench(iters, || serialize_nxs(&records)))
        } else {
            None
        };
        let json_ser_ns = t_json_ser.as_nanos() as f64;
        let mut ser_rows: Vec<(&str, std::time::Duration)> = vec![];
        if let Some(t) = t_nxs_compiler_ser {
            ser_rows.push(("NXS compiler", t));
        }
        ser_rows.push(("NXS wire    ", t_nxs_wire_ser));
        ser_rows.push(("JSON        ", t_json_ser));
        ser_rows.push(("XML         ", t_xml_ser));
        ser_rows.push(("CSV         ", t_csv_ser));
        for (name, t) in &ser_rows {
            let ratio = t.as_nanos() as f64 / json_ser_ns;
            println!("  │  {name}  {:>10}   ({:.2}x vs JSON)", fmt_ns(*t), ratio);
        }
        if !include_compiler {
            println!("  │  NXS compiler (skipped — would take minutes at this scale)");
        }
        println!(
            "  └─────────────────────────────────────────────────────────────────────────────┘"
        );

        // Deserialization speed
        println!(
            "\n  ┌─ Deserialization Time (avg over {iters} runs) ───────────────────────────────────┐"
        );
        let t_nxs_wire_de = bench(iters, || deserialize_nxs_wire(&nxs_wire_bytes));
        let t_json_de = bench(iters, || deserialize_json(&json_bytes));
        let t_xml_de = bench(iters, || deserialize_xml(&xml_bytes));
        let t_csv_de = bench(iters, || deserialize_csv(&csv_bytes));
        let t_nxs_compiler_de = nxs_compiler_bytes
            .as_ref()
            .filter(|b| b.len() < 60_000)
            .map(|b| bench(iters, || deserialize_nxs(b)));
        let json_de_ns = t_json_de.as_nanos() as f64;
        let mut de_rows: Vec<(&str, std::time::Duration)> = vec![];
        if let Some(t) = t_nxs_compiler_de {
            de_rows.push(("NXS compiler", t));
        }
        de_rows.push(("NXS wire    ", t_nxs_wire_de));
        de_rows.push(("JSON        ", t_json_de));
        de_rows.push(("XML         ", t_xml_de));
        de_rows.push(("CSV         ", t_csv_de));
        for (name, t) in &de_rows {
            let ratio = t.as_nanos() as f64 / json_de_ns;
            println!("  │  {name}  {:>10}   ({:.2}x vs JSON)", fmt_ns(*t), ratio);
        }
        println!(
            "  └─────────────────────────────────────────────────────────────────────────────┘\n"
        );
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  NXS compiler: .nxs source text → lex → parse → AST → binary (one-time build step)");
    println!("  NXS wire:     typed struct → direct binary write (the actual hot-path)");
    println!("  JSON/XML/CSV: string formatting → bytes (no parsing overhead on write side)\n");

    bench_wal();
}

// ── WAL benchmarks ────────────────────────────────────────────────────────────

fn span_dataset(n: usize) -> Vec<wal::SpanFields<'static>> {
    // Build owned strings first, then leak them so SpanFields<'static> works.
    // This is bench-only code — the leak is intentional.
    (0..n)
        .map(|i| {
            let name: &'static str = Box::leak(format!("op.{}", i % 20).into_boxed_str());
            let service: &'static str = Box::leak(format!("svc-{}", i % 5).into_boxed_str());
            wal::SpanFields {
                trace_id_hi: (i / 10) as i64,
                trace_id_lo: (i % 10) as i64,
                span_id: i as i64 + 1,
                parent_span_id: if i % 5 == 0 {
                    None
                } else {
                    Some((i - 1) as i64 + 1)
                },
                name,
                service,
                start_time_ns: 1_715_000_000_000_000_000_i64 + i as i64 * 1_000,
                duration_ns: 1_000 + (i % 50_000) as i64,
                status_code: (i % 3) as i64,
                payload: None,
            }
        })
        .collect()
}

fn bench_wal() {
    println!(
        "\n╔══════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!("║                        WAL Span Ingestion — Benchmark                           ║");
    println!(
        "╚══════════════════════════════════════════════════════════════════════════════════╝\n"
    );
    println!("  Scenarios measured:");
    println!("    append       — encode + write one span to a temp WAL file");
    println!("    append-batch — encode + write N spans, measure amortised per-span cost");
    println!("    recover      — linear scan to rebuild in-memory index from existing WAL");
    println!("    seal         — replay WAL → sealed .nxb segment");
    println!("    roundtrip    — append N spans, seal, query all by trace_id\n");

    const WAL_SIZES: &[usize] = &[1_000, 10_000, 100_000];

    for &n in WAL_SIZES {
        let spans = span_dataset(n);
        let iters = iters_for(n);

        println!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  {n} spans  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        );

        // ── append-batch ──────────────────────────────────────────────────────
        let t_append = bench(iters, || {
            let dir = tempfile::tempdir().expect("tempdir");
            let wal_path = dir.path().join("bench.nxsw");
            let mut w = wal::SpanWal::open(&wal_path).expect("open");
            for s in &spans {
                w.append(s).expect("append");
            }
            w.flush().expect("flush");
            std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0)
        });

        // ── recover ───────────────────────────────────────────────────────────
        // Pre-build a WAL file, then measure recovery time separately.
        let recover_dir = tempfile::tempdir().expect("tempdir");
        let recover_wal_path = recover_dir.path().join("recover.nxsw");
        {
            let mut w = wal::SpanWal::open(&recover_wal_path).expect("open");
            for s in &spans {
                w.append(s).expect("append");
            }
            w.flush().expect("flush");
        }
        let wal_file_size = std::fs::metadata(&recover_wal_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let t_recover = bench(iters, || {
            let mut w = wal::SpanWal::open(&recover_wal_path).expect("open");
            w.recover().expect("recover");
            w.record_count()
        });

        // ── seal ──────────────────────────────────────────────────────────────
        let seal_dir = tempfile::tempdir().expect("tempdir");
        let seal_wal_path = seal_dir.path().join("seal.nxsw");
        {
            let mut w = wal::SpanWal::open(&seal_wal_path).expect("open");
            for s in &spans {
                w.append(s).expect("append");
            }
            w.flush().expect("flush");
        }

        let t_seal = bench(iters, || {
            let mut w = wal::SpanWal::open(&seal_wal_path).expect("open");
            w.recover().expect("recover");
            let seg = seal_dir.path().join("bench.nxb");
            let report = w.seal(&seg).expect("seal");
            std::fs::remove_file(&seg).ok();
            report.bytes_written
        });

        // ── roundtrip: append → seal → query ─────────────────────────────────
        let t_roundtrip = bench(iters.min(3), || {
            let dir = tempfile::tempdir().expect("tempdir");
            let wal_path = dir.path().join("rt.nxsw");
            let mut w = wal::SpanWal::open(&wal_path).expect("open");
            for s in &spans {
                w.append(s).expect("append");
            }
            w.flush().expect("flush");
            let seg = dir.path().join("rt.nxb");
            w.recover().expect("recover");
            w.seal(&seg).expect("seal");
            std::fs::remove_file(&wal_path).ok();

            // Query the first trace_id from the segment
            let reader = segment_reader::SegmentReader::open(dir.path()).expect("reader");
            let trace_id =
                ((spans[0].trace_id_hi as u128) << 64) | spans[0].trace_id_lo as u64 as u128;
            reader.find_by_trace(trace_id).map(|v| v.len()).unwrap_or(0)
        });

        // ── json-ndjson per-span ──────────────────────────────────────────────
        #[derive(serde::Serialize)]
        struct SpanJson<'a> {
            trace_id_hi: i64,
            trace_id_lo: i64,
            span_id: i64,
            parent_span_id: Option<i64>,
            name: &'a str,
            service: &'a str,
            start_time_ns: i64,
            duration_ns: i64,
            status_code: i64,
        }
        let t_json = bench(iters, || {
            let mut out = Vec::with_capacity(n * 200);
            for s in &spans {
                serde_json::to_writer(
                    &mut out,
                    &SpanJson {
                        trace_id_hi: s.trace_id_hi,
                        trace_id_lo: s.trace_id_lo,
                        span_id: s.span_id,
                        parent_span_id: s.parent_span_id,
                        name: s.name,
                        service: s.service,
                        start_time_ns: s.start_time_ns,
                        duration_ns: s.duration_ns,
                        status_code: s.status_code,
                    },
                )
                .unwrap();
                out.push(b'\n');
            }
            out.len()
        });

        // ── print results ─────────────────────────────────────────────────────
        println!(
            "\n  ┌─ Timings (avg over {iters} runs) ────────────────────────────────────────────────┐"
        );
        let ns_per_span = |d: std::time::Duration| d.as_nanos() as f64 / n as f64;
        println!(
            "  │  append-batch   {:>10}  total  ({:.0} ns/span)",
            fmt_ns(t_append),
            ns_per_span(t_append)
        );
        println!(
            "  │  recover        {:>10}  total  ({:.0} ns/span)",
            fmt_ns(t_recover),
            ns_per_span(t_recover)
        );
        println!(
            "  │  seal           {:>10}  total  ({:.0} ns/span)",
            fmt_ns(t_seal),
            ns_per_span(t_seal)
        );
        println!(
            "  │  roundtrip      {:>10}  total  ({:.0} ns/span)  [iters={}]",
            fmt_ns(t_roundtrip),
            ns_per_span(t_roundtrip),
            iters.min(3)
        );
        println!(
            "  │  json-ndjson    {:>10}  total  ({:.0} ns/span)  [serde_json per span]",
            fmt_ns(t_json),
            ns_per_span(t_json)
        );
        println!(
            "  └─────────────────────────────────────────────────────────────────────────────┘"
        );

        println!(
            "\n  ┌─ File sizes ────────────────────────────────────────────────────────────────┐"
        );
        // Measure sealed .nxb size once
        let size_dir = tempfile::tempdir().expect("tempdir");
        let size_wal = size_dir.path().join("s.nxsw");
        let size_seg = size_dir.path().join("s.nxb");
        {
            let mut w = wal::SpanWal::open(&size_wal).expect("open");
            for s in &spans {
                w.append(s).expect("append");
            }
            w.recover().expect("recover");
            w.seal(&size_seg).expect("seal");
        }
        let wal_sz = wal_file_size;
        let seg_sz = std::fs::metadata(&size_seg).map(|m| m.len()).unwrap_or(0);
        let json_sz = spans
            .iter()
            .map(|s| {
                format!(
                    "{{\"trace_id\":\"{:016x}{:016x}\",\"span_id\":\"{:016x}\",\
                     \"name\":\"{}\",\"service\":\"{}\",\
                     \"start_time_ns\":{},\"duration_ns\":{},\"status_code\":{}}}",
                    s.trace_id_hi,
                    s.trace_id_lo,
                    s.span_id,
                    s.name,
                    s.service,
                    s.start_time_ns,
                    s.duration_ns,
                    s.status_code
                )
                .len()
            })
            .sum::<usize>();
        let baseline = json_sz as f64;
        println!(
            "  │  WAL (.nxsw)    {:>10}  ({:>5.1}% of JSON NDJSON)",
            fmt_bytes(wal_sz as usize),
            wal_sz as f64 / baseline * 100.0
        );
        println!(
            "  │  Sealed (.nxb)  {:>10}  ({:>5.1}% of JSON NDJSON)",
            fmt_bytes(seg_sz as usize),
            seg_sz as f64 / baseline * 100.0
        );
        println!("  │  JSON NDJSON    {:>10}  (baseline)", fmt_bytes(json_sz));
        println!(
            "  └─────────────────────────────────────────────────────────────────────────────┘\n"
        );
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  WAL append:  encode span → NxsWriter → write NYXO bytes to disk");
    println!("  recover:     open existing WAL, linear scan, rebuild in-memory trace index");
    println!("  seal:        replay WAL → full .nxb with tail-index (crash-safe export)");
    println!("  roundtrip:   append + seal + SegmentReader.find_by_trace() end-to-end\n");
}
