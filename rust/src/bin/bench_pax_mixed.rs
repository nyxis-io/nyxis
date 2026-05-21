//! Phase 2 mixed workload: 100 random `get_f64` + one `col_sum_f64` on row / columnar / PAX.
//!
//! Usage: `cargo run --release --bin bench_pax_mixed -- [records] [random_accesses]`

use nxs::layout::{finish_columnar, finish_pax, Cell, RecordRow};
use nxs::query::Reader;
use nxs::writer::{NxsWriter, Schema, Slot};
use serde::Serialize;
use std::time::Instant;

const WARMUP: usize = 20;
const SAMPLES: usize = 200;

#[derive(Serialize)]
struct PercentilesUs {
    p50: i64,
    p95: i64,
    p99: i64,
}

#[derive(Serialize)]
struct LayoutResult {
    layout: &'static str,
    records: usize,
    random_accesses: usize,
    file_bytes: usize,
    access_us: PercentilesUs,
    col_scan_us: PercentilesUs,
    mixed_total_us: PercentilesUs,
    col_sum_checksum: f64,
}

#[derive(Serialize)]
struct BenchOutput {
    workload: &'static str,
    driver: &'static str,
    layouts: Vec<LayoutResult>,
}

fn build_rows(n: usize) -> (Vec<String>, Vec<RecordRow>) {
    let keys = vec!["id".into(), "score".into(), "active".into(), "ts".into()];
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

fn percentiles_ns(mut xs: Vec<i64>) -> PercentilesUs {
    xs.sort_unstable();
    let p = |q: f64| -> i64 {
        let idx = ((xs.len() as f64 - 1.0) * q).round() as usize;
        xs[idx.min(xs.len() - 1)] / 1000
    };
    PercentilesUs {
        p50: p(0.50),
        p95: p(0.95),
        p99: p(0.99),
    }
}

fn measure_mixed(
    data: &[u8],
    n: usize,
    random_n: usize,
) -> (PercentilesUs, PercentilesUs, PercentilesUs, f64) {
    let mut access_samples = Vec::with_capacity(SAMPLES);
    let mut scan_samples = Vec::with_capacity(SAMPLES);
    let mut mixed_samples = Vec::with_capacity(SAMPLES);

    let mut idx = 0usize;
    for _ in 0..WARMUP {
        let r = Reader::new(data).unwrap();
        for _ in 0..random_n {
            idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
            if let Some(rec) = r.record(idx) {
                std::hint::black_box(rec.get_f64("score"));
            }
        }
        std::hint::black_box(r.col_sum_f64("score").unwrap());
    }

    for _ in 0..SAMPLES {
        let t0 = Instant::now();
        let r = Reader::new(data).unwrap();
        let t_access0 = Instant::now();
        for _ in 0..random_n {
            idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
            if let Some(rec) = r.record(idx) {
                std::hint::black_box(rec.get_f64("score"));
            }
        }
        let access_ns = t_access0.elapsed().as_nanos() as i64;
        let t_scan0 = Instant::now();
        let sum = r.col_sum_f64("score").unwrap();
        let scan_ns = t_scan0.elapsed().as_nanos() as i64;
        let total_ns = t0.elapsed().as_nanos() as i64;
        access_samples.push(access_ns);
        scan_samples.push(scan_ns);
        mixed_samples.push(total_ns);
        std::hint::black_box(sum);
    }

    let checksum = Reader::new(data).unwrap().col_sum_f64("score").unwrap();
    (
        percentiles_ns(access_samples),
        percentiles_ns(scan_samples),
        percentiles_ns(mixed_samples),
        checksum,
    )
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let random_n: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let page_size: u32 = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);

    let (keys, rows) = build_rows(n);
    let row_bytes = build_row_oriented(n);
    let col_bytes = finish_columnar(&keys, &rows).expect("columnar");
    let pax_bytes = finish_pax(&keys, &rows, page_size).expect("pax");

    let mut layouts = Vec::new();
    for (label, bytes) in [
        ("row", row_bytes),
        ("columnar", col_bytes),
        ("pax", pax_bytes),
    ] {
        let (access, scan, mixed, sum) = measure_mixed(&bytes, n, random_n);
        layouts.push(LayoutResult {
            layout: label,
            records: n,
            random_accesses: random_n,
            file_bytes: bytes.len(),
            access_us: access,
            col_scan_us: scan,
            mixed_total_us: mixed,
            col_sum_checksum: sum,
        });
    }

    let row_sum = layouts[0].col_sum_checksum;
    for lr in &layouts[1..] {
        assert!(
            (row_sum - lr.col_sum_checksum).abs() < 1e-3,
            "checksum mismatch {} vs row",
            lr.layout
        );
    }

    let out = BenchOutput {
        workload: "E",
        driver: "bench_pax_mixed",
        layouts,
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
