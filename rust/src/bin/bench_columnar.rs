//! Compare row-oriented vs columnar `sum(score)` on a dense flat-8 fixture.
//!
//! Usage: cargo run --release --bin bench_columnar -- [records]
//! Default: 100_000 records.

use nxs::layout::{finish_columnar, finish_pax, Cell, RecordRow};
use nxs::query::Reader;
use nxs::writer::{NxsWriter, Schema, Slot};
use std::time::Instant;

fn build_rows(n: usize) -> (Vec<String>, Vec<RecordRow>) {
    let keys = vec![
        "id".into(),
        "score".into(),
        "active".into(),
        "ts".into(),
    ];
    let rows: Vec<RecordRow> = (0..n)
        .map(|i| RecordRow {
            cells: vec![
                Cell::I64(i as i64),
                Cell::F64(i as f64 * 0.5),
                Cell::Bool(i % 2 == 0),
                Cell::Time(i as i64 * 1_000_000),
            ],
        })
        .collect();
    (keys, rows)
}

fn build_row_oriented(n: usize) -> Vec<u8> {
    let schema = Schema::new(&["id", "score", "active", "ts"]);
    let mut w = NxsWriter::with_capacity(&schema, n * 64);
    for i in 0..n {
        w.begin_object();
        w.write_i64(Slot(0), i as i64);
        w.write_f64(Slot(1), i as f64 * 0.5);
        w.write_bool(Slot(2), i % 2 == 0);
        w.write_time(Slot(3), i as i64 * 1_000_000);
        w.end_object();
    }
    w.finish()
}

fn bench<F: Fn()>(label: &str, iters: u32, f: F) -> std::time::Duration {
    // warmup
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    let per = elapsed.as_secs_f64() * 1e6 / f64::from(iters);
    println!("{label}: {per:.1} µs/iter ({iters} iters, {elapsed:?} total)");
    elapsed
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    println!("Building {n} dense flat-8 records...");
    let row_bytes = build_row_oriented(n);
    let (keys, rows) = build_rows(n);
    let col_bytes = finish_columnar(&keys, &rows).expect("columnar");
    let pax_bytes = finish_pax(&keys, &rows, 4096).expect("pax");

    println!(
        "sizes: row {} KB, columnar {} KB, pax {} KB",
        row_bytes.len() / 1024,
        col_bytes.len() / 1024,
        pax_bytes.len() / 1024
    );

    let iters = 50u32;
    bench("row SumF64 (query col_sum_f64 row path)", iters, || {
        let r = Reader::new(&row_bytes).unwrap();
        std::hint::black_box(r.col_sum_f64("score").unwrap());
    });
    bench("columnar col_sum_f64", iters, || {
        let r = Reader::new(&col_bytes).unwrap();
        std::hint::black_box(r.col_sum_f64("score").unwrap());
    });
    bench("pax col_sum_f64", iters, || {
        let r = Reader::new(&pax_bytes).unwrap();
        std::hint::black_box(r.col_sum_f64("score").unwrap());
    });

    let row_sum = Reader::new(&row_bytes).unwrap().col_sum_f64("score").unwrap();
    let col_sum = Reader::new(&col_bytes).unwrap().col_sum_f64("score").unwrap();
    assert!((row_sum - col_sum).abs() < 1e-3, "checksum mismatch");
    println!("checksum ok: {row_sum}");
}
