//! String-inclusive columnar benchmark: random `get_str` + full-column name walk.
//!
//! Usage: `cargo run --release --bin bench_columnar_strings -- [records] [random_accesses] [page_size]`

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
    str_access_us: PercentilesUs,
    str_scan_us: PercentilesUs,
    str_var_scan_us: Option<PercentilesUs>,
    mixed_total_us: PercentilesUs,
    name_len_checksum: u64,
}

#[derive(Serialize)]
struct BenchOutput {
    workload: &'static str,
    driver: &'static str,
    layouts: Vec<LayoutResult>,
}

fn build_rows(n: usize) -> (Vec<String>, Vec<RecordRow>) {
    let keys = vec!["id".into(), "name".into(), "score".into()];
    let rows: Vec<RecordRow> = (0..n)
        .map(|i| RecordRow {
            cells: vec![
                Cell::I64(i as i64),
                Cell::Str(format!("user_{i}")),
                Cell::F64(i as f64),
            ],
        })
        .collect();
    (keys, rows)
}

fn build_row_oriented(n: usize) -> Vec<u8> {
    let schema = Schema::new(&["id", "name", "score"]);
    let mut w = NxsWriter::with_capacity(&schema, n * 48);
    for i in 0..n {
        w.begin_object();
        w.write_i64(Slot(0), i as i64);
        w.write_str(Slot(1), &format!("user_{i}"));
        w.write_f64(Slot(2), i as f64);
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

fn name_len_checksum_record(data: &[u8], n: usize) -> u64 {
    let r = Reader::new(data).unwrap();
    let mut sum = 0u64;
    for i in 0..n {
        if let Some(rec) = r.record(i) {
            if let Some(s) = rec.get_str("name") {
                sum = sum.wrapping_add(s.len() as u64);
            }
        }
    }
    sum
}

fn col_bit(bm: &[u8], rec: usize) -> bool {
    (bm[rec / 8] >> (rec % 8)) & 1 == 1
}

fn name_len_checksum_var(data: &[u8], n: usize) -> u64 {
    let r = Reader::new(data).unwrap();
    if r.layout() == nxs::query::Layout::Row {
        return name_len_checksum_record(data, n);
    }
    let col = r.col_var_buffer("name").expect("col_var_buffer");
    let mut sum = 0u64;
    for i in 0..n {
        if !col_bit(col.null_bitmap, i) {
            continue;
        }
        let o = i * 4;
        let start = u32::from_le_bytes(col.offsets[o..o + 4].try_into().unwrap());
        let end = u32::from_le_bytes(col.offsets[o + 4..o + 8].try_into().unwrap());
        sum = sum.wrapping_add((end - start) as u64);
    }
    sum
}

fn measure(
    data: &[u8],
    n: usize,
    random_n: usize,
) -> (
    PercentilesUs,
    PercentilesUs,
    Option<PercentilesUs>,
    PercentilesUs,
    u64,
) {
    let mut access_samples = Vec::with_capacity(SAMPLES);
    let mut scan_samples = Vec::with_capacity(SAMPLES);
    let mut var_scan_samples = Vec::with_capacity(SAMPLES);
    let mut mixed_samples = Vec::with_capacity(SAMPLES);
    let mut idx = 0usize;
    let use_var = Reader::new(data).unwrap().layout() == nxs::query::Layout::Columnar;

    for _ in 0..WARMUP {
        let r = Reader::new(data).unwrap();
        for _ in 0..random_n {
            idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
            if let Some(rec) = r.record(idx) {
                std::hint::black_box(rec.get_str("name"));
            }
        }
        std::hint::black_box(name_len_checksum_record(data, n));
        if use_var {
            std::hint::black_box(name_len_checksum_var(data, n));
        }
    }

    for _ in 0..SAMPLES {
        let t0 = Instant::now();
        let r = Reader::new(data).unwrap();
        let t_access0 = Instant::now();
        for _ in 0..random_n {
            idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
            if let Some(rec) = r.record(idx) {
                std::hint::black_box(rec.get_str("name"));
            }
        }
        let access_ns = t_access0.elapsed().as_nanos() as i64;
        let t_scan0 = Instant::now();
        let walk = name_len_checksum_record(data, n);
        let scan_ns = t_scan0.elapsed().as_nanos() as i64;
        if use_var {
            let t_var0 = Instant::now();
            let var_walk = name_len_checksum_var(data, n);
            assert_eq!(var_walk, walk);
            var_scan_samples.push(t_var0.elapsed().as_nanos() as i64);
        }
        let total_ns = t0.elapsed().as_nanos() as i64;
        access_samples.push(access_ns);
        scan_samples.push(scan_ns);
        mixed_samples.push(total_ns);
        std::hint::black_box(walk);
    }

    let var_scan = if use_var {
        Some(percentiles_ns(var_scan_samples))
    } else {
        None
    };
    let checksum = name_len_checksum_record(data, n);
    (
        percentiles_ns(access_samples),
        percentiles_ns(scan_samples),
        var_scan,
        percentiles_ns(mixed_samples),
        checksum,
    )
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
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

    let row_checksum = name_len_checksum_record(&row_bytes, n);
    let mut layouts = Vec::new();
    for (label, bytes) in [
        ("row", row_bytes),
        ("columnar", col_bytes),
        ("pax", pax_bytes),
    ] {
        let (access, scan, var_scan, mixed, sum) = measure(&bytes, n, random_n);
        assert_eq!(sum, row_checksum, "name len checksum mismatch for {label}");
        layouts.push(LayoutResult {
            layout: label,
            records: n,
            random_accesses: random_n,
            file_bytes: bytes.len(),
            str_access_us: access,
            str_scan_us: scan,
            str_var_scan_us: var_scan,
            mixed_total_us: mixed,
            name_len_checksum: sum,
        });
    }

    let out = BenchOutput {
        workload: "C-strings",
        driver: "bench_columnar_strings",
        layouts,
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
