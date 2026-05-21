//! Break down where columnar scan time goes vs Arrow-style cached sum.
//!
//! Usage: cargo run --release --bin profile_columnar -- <path-to-columnar.nxb>

use nxs::query::Reader;
use std::time::Instant;

const ITERS: usize = 200;
const ARROW_P50_NS: f64 = 104_000.0; // Workload C 1M reference from prior run

fn ns_per(iter: usize, elapsed: std::time::Duration) -> f64 {
    elapsed.as_nanos() as f64 / iter as f64
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        format!(
            "{}/../bench/data/bin/workload_C_nxs_columnar_1000000.nxb",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let data = std::fs::read(&path).expect("read fixture");
    let data = data.as_slice();

    println!("fixture: {path}");
    println!("size: {} MB, iters: {ITERS}\n", data.len() / (1024 * 1024));

    // 1) Harness-equivalent: reopen Reader every iteration
    let t0 = Instant::now();
    let mut checksum = 0.0f64;
    for _ in 0..ITERS {
        let r = Reader::new(data).expect("open");
        checksum += r.col_sum_f64("score").unwrap();
    }
    let harness_ns = ns_per(ITERS, t0.elapsed());
    print_line(1, "harness_loop (Reader::new + col_sum_f64)", harness_ns);

    // 2) Cached reader: col_sum only
    let r = Reader::new(data).expect("open");
    let t0 = Instant::now();
    for _ in 0..ITERS {
        checksum += r.col_sum_f64("score").unwrap();
    }
    let sum_ns = ns_per(ITERS, t0.elapsed());
    print_line(2, "col_sum_f64 only (cached Reader)", sum_ns);

    // 3) Reader::new only
    let t0 = Instant::now();
    for _ in 0..ITERS {
        std::hint::black_box(Reader::new(data).expect("open"));
    }
    let open_ns = ns_per(ITERS, t0.elapsed());
    print_line(3, "Reader::new only", open_ns);

    // 4) slot + col_buffer resolve
    let t0 = Instant::now();
    for _ in 0..ITERS {
        std::hint::black_box(r.col_buffer("score").unwrap().len());
    }
    let lookup_ns = ns_per(ITERS, t0.elapsed());
    print_line(4, "col_buffer resolve (cached Reader)", lookup_ns);

    // 5) Dense raw f64 sum — from_le_bytes per element
    let vals = r.col_buffer("score").unwrap();
    let t0 = Instant::now();
    for _ in 0..ITERS {
        let mut s = 0.0f64;
        for chunk in vals.chunks_exact(8) {
            s += f64::from_le_bytes(chunk.try_into().unwrap());
        }
        std::hint::black_box(s);
    }
    let le_ns = ns_per(ITERS, t0.elapsed());
    print_line(5, "dense sum via from_le_bytes (1M)", le_ns);

    // 6) Dense f64 slice .sum() — contiguous, Arrow-like
    let t0 = Instant::now();
    for _ in 0..ITERS {
        let n_f64 = vals.len() / 8;
        let ptr = vals.as_ptr() as *const f64;
        let slice = unsafe { std::slice::from_raw_parts(ptr, n_f64) };
        std::hint::black_box(slice.iter().sum::<f64>());
    }
    let slice_ns = ns_per(ITERS, t0.elapsed());
    print_line(6, "dense f64 slice .iter().sum()", slice_ns);

    // 7) HashMap slot lookup only
    let t0 = Instant::now();
    for _ in 0..ITERS {
        std::hint::black_box(r.slot("score"));
    }
    let slot_ns = ns_per(ITERS, t0.elapsed());
    print_line(7, "HashMap slot('score')", slot_ns);

    println!("\n=== Attribution (harness ≈ open + sum) ===");
    let sum_est = harness_ns - open_ns;
    println!(
        "Reader::new:     {:>8.0} ns  ({:>5.1}% of harness)",
        open_ns,
        100.0 * open_ns / harness_ns
    );
    println!(
        "col_sum (est):   {:>8.0} ns  ({:>5.1}% of harness)",
        sum_est,
        100.0 * sum_est / harness_ns
    );
    println!("\n=== vs Arrow IPC scan P50 ≈ {:.0} µs ===", ARROW_P50_NS / 1000.0);
    println!(
        "cached col_sum:  {:.1}× Arrow",
        sum_ns / ARROW_P50_NS
    );
    println!(
        "harness loop:    {:.1}× Arrow",
        harness_ns / ARROW_P50_NS
    );
    println!(
        "theoretical floor (f64 .sum): {:.1}× Arrow",
        slice_ns / ARROW_P50_NS
    );
    println!("\nIf cached sum is ~{:.0}× Arrow but harness is ~{:.0}×, reopening Reader each sample dominates.",
        sum_ns / ARROW_P50_NS,
        harness_ns / ARROW_P50_NS
    );
    println!("checksum: {checksum}");
}

fn print_line(n: u32, label: &str, ns: f64) {
    println!("{n}. {label:<42} {:>10.0} ns  ({:>7.2} µs)", ns, ns / 1000.0);
}
