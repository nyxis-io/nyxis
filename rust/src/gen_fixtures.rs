/// Generates matching .nxb and .json fixtures for the JS benchmark.
/// Usage: cargo run --release --bin gen_fixtures -- <out_dir> [sizes...]
use nxs::compact::CompactOptions;
use nxs::layout::{finish_columnar, finish_row, Cell, RecordRow};
use nxs::writer::{NxsWriter, Schema, Slot};

use std::fs;
use std::path::{Path, PathBuf};

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
const S_ID: Slot = Slot(0);
const S_USERNAME: Slot = Slot(1);
const S_EMAIL: Slot = Slot(2);
const S_AGE: Slot = Slot(3);
const S_BALANCE: Slot = Slot(4);
const S_ACTIVE: Slot = Slot(5);
const S_SCORE: Slot = Slot(6);
const S_CREATED_AT: Slot = Slot(7);

struct Rec {
    id: i64,
    username: String,
    email: String,
    age: i64,
    balance: f64,
    active: bool,
    score: f64,
}

fn ensure_out_dir_writable(out_dir: &Path) {
    let probe = out_dir.join(".gen_fixtures_write_probe");
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
        }
        Err(e) => {
            eprintln!(
                "error: output directory is not writable: {}\n  {e}",
                out_dir.display()
            );
            eprintln!(
                "hint: chmod u+w \"{}\"  (if root-owned: sudo chown -R \"$USER\" \"{}\")",
                out_dir.display(),
                out_dir.display()
            );
            eprintln!("hint: or use a writable path (e.g. ../out/fixtures); `make fixtures` picks that when js/fixtures is locked.");
            std::process::exit(1);
        }
    }
}

fn write_file(path: &Path, contents: &[u8], label: &str) {
    if let Err(e) = fs::write(path, contents) {
        eprintln!("error: failed to write {label} {}: {e}", path.display());
        eprintln!(
            "hint: chmod -R u+w \"{}\" or fix ownership of that directory.",
            path.parent().unwrap_or(path).display()
        );
        std::process::exit(1);
    }
}

fn build(n: usize) -> Vec<Rec> {
    (0..n)
        .map(|i| Rec {
            id: i as i64,
            username: format!("user_{i:07}"),
            email: format!("user{i}@example.com"),
            age: 20 + (i % 50) as i64,
            balance: 100.0 + (i as f64) * 1.37,
            active: i % 3 != 0,
            score: (i as f64 % 100.0) / 10.0,
        })
        .collect()
}

fn to_rows(records: &[Rec]) -> (Vec<String>, Vec<RecordRow>) {
    let keys: Vec<String> = SLOTS.iter().map(|s| (*s).to_string()).collect();
    let rows: Vec<RecordRow> = records
        .iter()
        .map(|r| RecordRow {
            cells: vec![
                Cell::I64(r.id),
                Cell::Str(r.username.clone()),
                Cell::Str(r.email.clone()),
                Cell::I64(r.age),
                Cell::F64(r.balance),
                Cell::Bool(r.active),
                Cell::F64(r.score),
                Cell::Time(1_777_593_600_000_000_000),
            ],
        })
        .collect();
    (keys, rows)
}

fn write_nxb(records: &[Rec], path: &Path) {
    let schema = Schema::new(SLOTS);
    let mut w = NxsWriter::with_capacity(&schema, records.len() * 128 + 1024);
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
    let bytes = w.finish();
    write_file(path, &bytes, "nxb");
    println!("  {} → {} bytes", path.display(), bytes.len());
}

fn sparse_conformance_rows(n: usize) -> (Vec<String>, Vec<RecordRow>) {
    let keys: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut rows = Vec::with_capacity(n);
    for i in 0..n as u64 {
        let mask = i.wrapping_mul(0xB7E1_5162_8AED_2A6B_u64.wrapping_add(i)) & 0xFF;
        let mask = if mask == 0 { 1 } else { mask };
        let mut cells = vec![Cell::Absent; 8];
        if mask & 1 != 0 {
            cells[0] = Cell::I64(i as i64);
        }
        if mask & 2 != 0 {
            cells[1] = Cell::F64(i as f64 * 0.5);
        }
        if mask & 4 != 0 {
            cells[2] = Cell::Bool(i % 2 == 0);
        }
        if mask & 8 != 0 {
            cells[3] = Cell::Str(format!("s{i}"));
        }
        if mask & 16 != 0 {
            cells[4] = Cell::I64(-(i as i64));
        }
        if mask & 32 != 0 {
            cells[5] = Cell::F64(i as f64 * 1.25);
        }
        if mask & 64 != 0 {
            cells[6] = Cell::Bool(i % 3 == 0);
        }
        if mask & 128 != 0 {
            cells[7] = Cell::I64(i as i64 * 100);
        }
        rows.push(RecordRow { cells });
    }
    (keys, rows)
}

fn write_sparse_compact_100(out_dir: &Path) {
    let (keys, rows) = sparse_conformance_rows(100);
    let bytes = finish_row(&keys, &rows, Some(&CompactOptions::compact())).expect("sparse compact");
    let path = out_dir.join("sparse_100_compact.nxb");
    write_file(&path, &bytes, "sparse compact nxb");
}

fn write_nxb_compact(records: &[Rec], path: &Path) {
    let (keys, rows) = to_rows(records);
    let bytes = finish_row(&keys, &rows, Some(&CompactOptions::compact())).expect("compact row");
    write_file(path, &bytes, "compact nxb");
    println!("  {} → {} bytes", path.display(), bytes.len());
}

fn write_nxb_columnar(records: &[Rec], path: &Path) {
    let (keys, rows) = to_rows(records);
    let bytes = finish_columnar(&keys, &rows).expect("columnar encode");
    write_file(path, &bytes, "columnar nxb");
    println!("  {} → {} bytes", path.display(), bytes.len());
}

fn write_json(records: &[Rec], path: &Path) {
    let mut s = String::with_capacity(records.len() * 180);
    s.push('[');
    for (i, r) in records.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":{},\"username\":\"{}\",\"email\":\"{}\",\"age\":{},\"balance\":{:.2},\"active\":{},\"score\":{:.1},\"created_at\":\"2026-04-30\"}}",
            r.id, r.username, r.email, r.age, r.balance, r.active, r.score
        ));
    }
    s.push(']');
    write_file(path, s.as_bytes(), "json");
    println!("  {} → {} bytes", path.display(), s.len());
}

fn write_csv(records: &[Rec], path: &Path) {
    let mut s = String::with_capacity(records.len() * 80);
    s.push_str("id,username,email,age,balance,active,score,created_at\n");
    for r in records {
        s.push_str(&format!(
            "{},{},{},{},{:.2},{},{:.1},2026-04-30\n",
            r.id, r.username, r.email, r.age, r.balance, r.active, r.score
        ));
    }
    write_file(path, s.as_bytes(), "csv");
    println!("  {} → {} bytes", path.display(), s.len());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: gen_fixtures <out_dir> [sizes...]");
        eprintln!("  default sizes: 1000 10000 100000 1000000");
        std::process::exit(1);
    }
    let out_dir = PathBuf::from(&args[1]);
    fs::create_dir_all(&out_dir).expect("mkdir");
    ensure_out_dir_writable(&out_dir);

    let sizes: Vec<usize> = if args.len() > 2 {
        args[2..]
            .iter()
            .map(|s| s.parse().expect("bad size"))
            .collect()
    } else {
        vec![1_000, 10_000, 100_000, 1_000_000]
    };

    for &n in &sizes {
        println!("Generating n={n}...");
        let records = build(n);
        write_nxb(&records, &out_dir.join(format!("records_{n}.nxb")));
        write_nxb_compact(&records, &out_dir.join(format!("records_{n}_compact.nxb")));
        write_nxb_columnar(&records, &out_dir.join(format!("records_{n}_columnar.nxb")));
        write_json(&records, &out_dir.join(format!("records_{n}.json")));
        write_csv(&records, &out_dir.join(format!("records_{n}.csv")));
    }
    println!("Generating sparse_100_compact (conformance mask)...");
    write_sparse_compact_100(&out_dir);

    println!("Done. Fixtures in {}", out_dir.display());
}
