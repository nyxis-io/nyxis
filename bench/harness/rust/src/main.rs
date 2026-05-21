//! Cross-format benchmark harness (Rust). Uniform JSON-line output.

use clap::Parser;
use memmap2::Mmap;
use nxs::query::Reader;
use serde::Serialize;
use std::fs::{self, File};
use std::path::PathBuf;
use std::time::Instant;

const WARMUP: usize = 100;
const SAMPLES: usize = 1000;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    workload: String,
    #[arg(long, default_value = "nxs")]
    format: String,
    #[arg(long)]
    records: u32,
    #[arg(long)]
    metric: String,
    #[arg(long, default_value = "-1")]
    population: f64,
    #[arg(long, default_value = "bench/data/bin")]
    data_dir: PathBuf,
    #[arg(long)]
    path: Option<PathBuf>,
}

#[derive(Serialize)]
struct Line<'a> {
    workload: &'a str,
    format: &'a str,
    records: u32,
    metric: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    p50_ns: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    p99_ns: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    iqr_ns: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    samples: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    population: f64,
}

fn default_path(args: &Args) -> PathBuf {
    if let Some(p) = &args.path {
        return p.clone();
    }
    let wl = args.workload.to_uppercase();
    if wl == "A" && args.population >= 0.0 {
        let pct = (args.population * 100.0).round() as u32;
        args.data_dir.join(format!(
            "workload_{wl}_nxs_{}_pop{pct:02}.nxb",
            args.records
        ))
    } else {
        args.data_dir.join(format!("workload_{wl}_nxs_{}.nxb", args.records))
    }
}

fn measure(mut f: impl FnMut()) -> (i64, i64, i64) {
    for _ in 0..WARMUP {
        f();
    }
    let mut buf = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let t0 = Instant::now();
        f();
        buf.push(t0.elapsed().as_nanos() as i64);
    }
    buf.sort_unstable();
    let q1 = buf[SAMPLES / 4];
    let q3 = buf[(3 * SAMPLES) / 4];
    let iqr = q3 - q1;
    let trim = &buf[SAMPLES / 4..=(3 * SAMPLES) / 4];
    let p50 = trim[trim.len() / 2];
    let p99_idx = ((trim.len() as f64 - 1.0) * 0.99).round() as usize;
    let p99 = trim[p99_idx.min(trim.len() - 1)];
    (p50, p99, iqr)
}

fn main() {
    let args = Args::parse();
    if args.format != "nxs" {
        eprintln!("rust harness: only nxs implemented");
        std::process::exit(1);
    }

    let path = default_path(&args);
    if args.metric == "size" {
        let meta = fs::metadata(&path).expect("stat");
        let line = Line {
            workload: &args.workload,
            format: "nxs",
            records: args.records,
            metric: "size",
            p50_ns: None,
            p99_ns: None,
            iqr_ns: None,
            samples: None,
            bytes: Some(meta.len()),
            population: args.population,
        };
        println!("{}", serde_json::to_string(&line).unwrap());
        return;
    }

    let file = File::open(&path).expect("open");
    let mmap = unsafe { Mmap::map(&file).expect("mmap") };
    let data: &[u8] = &mmap;
    let field = if args.workload.eq_ignore_ascii_case("A") {
        "f36"
    } else {
        "score"
    };

    match args.metric.as_str() {
        "open" => {
            let (p50, p99, iqr) = measure(|| {
                let r = Reader::new(&data).expect("open");
                if let Some(o) = r.record(0) {
                    std::hint::black_box(o.get_f64(field));
                }
            });
            print_line(&args, p50, p99, iqr);
        }
        "access" => {
            let mut idx = 0usize;
            let (p50, p99, iqr) = measure(|| {
                let r = Reader::new(&data).expect("open");
                let n = r.record_count();
                if n == 0 {
                    return;
                }
                idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
                if let Some(o) = r.record(idx) {
                    std::hint::black_box(o.get_f64(field));
                }
            });
            print_line(&args, p50, p99, iqr);
        }
        "scan" => {
            let (p50, p99, iqr) = measure(|| {
                let r = Reader::new(&data).expect("open");
                let mut sum = 0.0f64;
                for i in 0..r.record_count() {
                    if let Some(rec) = r.record(i) {
                        if let Some(v) = rec.get_f64(field) {
                            sum += v;
                        }
                    }
                }
                std::hint::black_box(sum);
            });
            print_line(&args, p50, p99, iqr);
        }
        "selective" => {
            if !args.workload.eq_ignore_ascii_case("A") {
                eprintln!("selective metric only for workload A");
                std::process::exit(2);
            }
            let r = Reader::new(&data).expect("open");
            let n = r.record_count();
            let mut idx = 0usize;
            let read_sel = |rec: nxs::query::Record<'_, '_>| {
                std::hint::black_box(rec.get_i64("i01"));
                std::hint::black_box(rec.get_str("s21"));
                std::hint::black_box(rec.get_f64("f36"));
                std::hint::black_box(rec.get_bool("b46"));
                std::hint::black_box(rec.get_i64("i10"));
            };
            for _ in 0..WARMUP {
                if n == 0 {
                    break;
                }
                idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
                if let Some(rec) = r.record(idx) {
                    read_sel(rec);
                }
            }
            let (p50, p99, iqr) = measure(|| {
                if n == 0 {
                    return;
                }
                idx = (idx.wrapping_mul(997).wrapping_add(1)) % n;
                if let Some(rec) = r.record(idx) {
                    read_sel(rec);
                }
            });
            print_line(&args, p50, p99, iqr);
        }
        _ => {
            eprintln!("unknown metric {}", args.metric);
            std::process::exit(2);
        }
    }
}

fn print_line(args: &Args, p50: i64, p99: i64, iqr: i64) {
    let line = Line {
        workload: &args.workload,
        format: "nxs",
        records: args.records,
        metric: &args.metric,
        p50_ns: Some(p50),
        p99_ns: Some(p99),
        iqr_ns: Some(iqr),
        samples: Some(SAMPLES),
        bytes: None,
        population: args.population,
    };
    println!("{}", serde_json::to_string(&line).unwrap());
}
