//! Transcode canonical JSON → `.nxb` for benchmark workloads.
//!
//! Usage:
//!   bench-transcode --workload B --json ../data/json/workload_B_1000000.json \
//!       --out ../data/bin/workload_B_nxs_1000000.nxb

use clap::Parser;
use nxs::writer::{NxsWriter, Schema, Slot};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bench-transcode")]
struct Args {
    #[arg(long)]
    workload: char,
    #[arg(long)]
    json: PathBuf,
    #[arg(long)]
    out: PathBuf,
}

#[derive(Deserialize)]
struct Flat8 {
    id: i64,
    username: String,
    email: String,
    age: i64,
    balance: f64,
    active: bool,
    score: f64,
    created_at: i64,
}

#[derive(Deserialize)]
struct Dense8 {
    id: i64,
    bucket: i64,
    quantity: i64,
    amount: f64,
    rate: f64,
    score: f64,
    category: i64,
    active: bool,
}

#[derive(Deserialize)]
struct SparseRecord {
    #[serde(default)]
    i01: Option<i64>,
    #[serde(default)]
    i02: Option<i64>,
    #[serde(default)]
    i03: Option<i64>,
    #[serde(default)]
    i04: Option<i64>,
    #[serde(default)]
    i05: Option<i64>,
    #[serde(default)]
    i06: Option<i64>,
    #[serde(default)]
    i07: Option<i64>,
    #[serde(default)]
    i08: Option<i64>,
    #[serde(default)]
    i09: Option<i64>,
    #[serde(default)]
    i10: Option<i64>,
    #[serde(default)]
    i11: Option<i64>,
    #[serde(default)]
    i12: Option<i64>,
    #[serde(default)]
    i13: Option<i64>,
    #[serde(default)]
    i14: Option<i64>,
    #[serde(default)]
    i15: Option<i64>,
    #[serde(default)]
    i16: Option<i64>,
    #[serde(default)]
    i17: Option<i64>,
    #[serde(default)]
    i18: Option<i64>,
    #[serde(default)]
    i19: Option<i64>,
    #[serde(default)]
    i20: Option<i64>,
    #[serde(default)]
    s21: Option<String>,
    #[serde(default)]
    s22: Option<String>,
    #[serde(default)]
    s23: Option<String>,
    #[serde(default)]
    s24: Option<String>,
    #[serde(default)]
    s25: Option<String>,
    #[serde(default)]
    s26: Option<String>,
    #[serde(default)]
    s27: Option<String>,
    #[serde(default)]
    s28: Option<String>,
    #[serde(default)]
    s29: Option<String>,
    #[serde(default)]
    s30: Option<String>,
    #[serde(default)]
    s31: Option<String>,
    #[serde(default)]
    s32: Option<String>,
    #[serde(default)]
    s33: Option<String>,
    #[serde(default)]
    s34: Option<String>,
    #[serde(default)]
    s35: Option<String>,
    #[serde(default)]
    f36: Option<f64>,
    #[serde(default)]
    f37: Option<f64>,
    #[serde(default)]
    f38: Option<f64>,
    #[serde(default)]
    f39: Option<f64>,
    #[serde(default)]
    f40: Option<f64>,
    #[serde(default)]
    f41: Option<f64>,
    #[serde(default)]
    f42: Option<f64>,
    #[serde(default)]
    f43: Option<f64>,
    #[serde(default)]
    f44: Option<f64>,
    #[serde(default)]
    f45: Option<f64>,
    #[serde(default)]
    b46: Option<bool>,
    #[serde(default)]
    b47: Option<bool>,
    #[serde(default)]
    b48: Option<bool>,
    #[serde(default)]
    b49: Option<bool>,
    #[serde(default)]
    b50: Option<bool>,
}

fn write_flat8(records: &[Flat8], out: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let keys = [
        "id", "username", "email", "age", "balance", "active", "score", "created_at",
    ];
    let schema = Schema::new(&keys);
    let mut w = NxsWriter::with_capacity(&schema, records.len() * 128 + 4096);
    let slots: [Slot; 8] = std::array::from_fn(|i| Slot(i as u16));
    for r in records {
        w.begin_object();
        w.write_i64(slots[0], r.id);
        w.write_str(slots[1], &r.username);
        w.write_str(slots[2], &r.email);
        w.write_i64(slots[3], r.age);
        w.write_f64(slots[4], r.balance);
        w.write_bool(slots[5], r.active);
        w.write_f64(slots[6], r.score);
        w.write_time(slots[7], r.created_at);
        w.end_object();
    }
    fs::write(out, w.finish())?;
    Ok(())
}

fn write_dense8(records: &[Dense8], out: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let keys = [
        "id", "bucket", "quantity", "amount", "rate", "score", "category", "active",
    ];
    let schema = Schema::new(&keys);
    let mut w = NxsWriter::with_capacity(&schema, records.len() * 96 + 4096);
    let slots: [Slot; 8] = std::array::from_fn(|i| Slot(i as u16));
    for r in records {
        w.begin_object();
        w.write_i64(slots[0], r.id);
        w.write_i64(slots[1], r.bucket);
        w.write_i64(slots[2], r.quantity);
        w.write_f64(slots[3], r.amount);
        w.write_f64(slots[4], r.rate);
        w.write_f64(slots[5], r.score);
        w.write_i64(slots[6], r.category);
        w.write_bool(slots[7], r.active);
        w.end_object();
    }
    fs::write(out, w.finish())?;
    Ok(())
}

fn write_sparse(records: &[SparseRecord], out: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let key_strs: Vec<String> = (1..=20)
        .map(|i| format!("i{i:02}"))
        .chain((21..=35).map(|i| format!("s{i:02}")))
        .chain((36..=45).map(|i| format!("f{i:02}")))
        .chain((46..=50).map(|i| format!("b{i:02}")))
        .collect();
    let keys: Vec<&str> = key_strs.iter().map(|s| s.as_str()).collect();
    let schema = Schema::new(&keys);
    let mut w = NxsWriter::with_capacity(&schema, records.len() * 256 + 4096);

    macro_rules! opt_i64 {
        ($w:expr, $slot:expr, $v:expr) => {
            if let Some(v) = $v {
                $w.write_i64($slot, v);
            }
        };
    }
    macro_rules! opt_f64 {
        ($w:expr, $slot:expr, $v:expr) => {
            if let Some(v) = $v {
                $w.write_f64($slot, v);
            }
        };
    }
    macro_rules! opt_bool {
        ($w:expr, $slot:expr, $v:expr) => {
            if let Some(v) = $v {
                $w.write_bool($slot, v);
            }
        };
    }
    macro_rules! opt_str {
        ($w:expr, $slot:expr, $v:expr) => {
            if let Some(ref v) = $v {
                $w.write_str($slot, v);
            }
        };
    }

    for r in records {
        w.begin_object();
        opt_i64!(w, Slot(0), r.i01);
        opt_i64!(w, Slot(1), r.i02);
        opt_i64!(w, Slot(2), r.i03);
        opt_i64!(w, Slot(3), r.i04);
        opt_i64!(w, Slot(4), r.i05);
        opt_i64!(w, Slot(5), r.i06);
        opt_i64!(w, Slot(6), r.i07);
        opt_i64!(w, Slot(7), r.i08);
        opt_i64!(w, Slot(8), r.i09);
        opt_i64!(w, Slot(9), r.i10);
        opt_i64!(w, Slot(10), r.i11);
        opt_i64!(w, Slot(11), r.i12);
        opt_i64!(w, Slot(12), r.i13);
        opt_i64!(w, Slot(13), r.i14);
        opt_i64!(w, Slot(14), r.i15);
        opt_i64!(w, Slot(15), r.i16);
        opt_i64!(w, Slot(16), r.i17);
        opt_i64!(w, Slot(17), r.i18);
        opt_i64!(w, Slot(18), r.i19);
        opt_i64!(w, Slot(19), r.i20);
        opt_str!(w, Slot(20), r.s21);
        opt_str!(w, Slot(21), r.s22);
        opt_str!(w, Slot(22), r.s23);
        opt_str!(w, Slot(23), r.s24);
        opt_str!(w, Slot(24), r.s25);
        opt_str!(w, Slot(25), r.s26);
        opt_str!(w, Slot(26), r.s27);
        opt_str!(w, Slot(27), r.s28);
        opt_str!(w, Slot(28), r.s29);
        opt_str!(w, Slot(29), r.s30);
        opt_str!(w, Slot(30), r.s31);
        opt_str!(w, Slot(31), r.s32);
        opt_str!(w, Slot(32), r.s33);
        opt_str!(w, Slot(33), r.s34);
        opt_str!(w, Slot(34), r.s35);
        opt_f64!(w, Slot(35), r.f36);
        opt_f64!(w, Slot(36), r.f37);
        opt_f64!(w, Slot(37), r.f38);
        opt_f64!(w, Slot(38), r.f39);
        opt_f64!(w, Slot(39), r.f40);
        opt_f64!(w, Slot(40), r.f41);
        opt_f64!(w, Slot(41), r.f42);
        opt_f64!(w, Slot(42), r.f43);
        opt_f64!(w, Slot(43), r.f44);
        opt_f64!(w, Slot(44), r.f45);
        opt_bool!(w, Slot(45), r.b46);
        opt_bool!(w, Slot(46), r.b47);
        opt_bool!(w, Slot(47), r.b48);
        opt_bool!(w, Slot(48), r.b49);
        opt_bool!(w, Slot(49), r.b50);
        w.end_object();
    }
    fs::write(out, w.finish())?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let text = fs::read_to_string(&args.json)?;
    match args.workload {
        'B' | 'b' => {
            let records: Vec<Flat8> = serde_json::from_str(&text)?;
            write_flat8(&records, &args.out)?;
        }
        'C' | 'c' => {
            let records: Vec<Dense8> = serde_json::from_str(&text)?;
            write_dense8(&records, &args.out)?;
        }
        'A' | 'a' => {
            let records: Vec<SparseRecord> = serde_json::from_str(&text)?;
            write_sparse(&records, &args.out)?;
        }
        _ => {
            eprintln!("unknown workload {:?}", args.workload);
            std::process::exit(1);
        }
    }
    let nbytes = fs::metadata(&args.out)?.len();
    println!(
        "wrote {} ({} bytes)",
        args.out.display(),
        nbytes
    );
    Ok(())
}
