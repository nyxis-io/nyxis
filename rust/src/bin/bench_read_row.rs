//! Row-layout read benchmark: open, sum(score), random access (default compact vs `--legacy-v12`).
//!
//! Usage: bench_read_row <v12.nxb> <compact.nxb>

use std::env;
use std::time::Instant;

use nxs::query::Reader;

const RUNS: u32 = 5;

fn best_ms(mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..RUNS {
        let t0 = Instant::now();
        f();
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        if ms < best {
            best = ms;
        }
    }
    best
}

fn sum_score(r: &Reader) -> f64 {
    let mut sum = 0.0;
    for i in 0..r.record_count() {
        if let Some(v) = r.record(i).and_then(|rec| rec.get_f64("score")) {
            sum += v;
        }
    }
    sum
}

fn random_score(r: &Reader) {
    let n = r.record_count();
    for i in 0..1000 {
        let k = (i * 997) % n;
        let _ = r.record(k).and_then(|rec| rec.get_f64("score"));
    }
}

fn bench_file(label: &str, path: &str) {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let size_mb = data.len() as f64 / 1_048_576.0;

    let open_ms = best_ms(|| {
        let _ = Reader::new(&data).expect("open");
    });
    let reader = Reader::new(&data).expect("open");

    let sum_ms = best_ms(|| {
        let s = sum_score(&reader);
        std::hint::black_box(s);
    });
    let sum = sum_score(&reader);

    let rand_ms = best_ms(|| {
        random_score(&reader);
    });

    println!(
        "{label} — {} records, {size_mb:.2} MiB",
        reader.record_count()
    );
    println!("  open (Reader::new)     {open_ms:7.3} ms");
    println!("  sum(score)             {sum_ms:7.3} ms  (sum={sum:.2})");
    println!(
        "  random get_f64 ×1000   {rand_ms:7.3} ms  ({:.0} ns/rec)",
        rand_ms * 1e6 / 1000.0
    );
    println!();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: bench_read_row <v12.nxb> <compact.nxb>");
        std::process::exit(1);
    }
    bench_file("v1.2 row", &args[1]);
    bench_file("v1.3 compact", &args[2]);
}
