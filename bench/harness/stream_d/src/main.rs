//! Workload D — streaming ingest / time-to-first-record (Phase 1: NXS + Protobuf, D2 file).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use capnp::message::{Builder, ReaderOptions};
use capnp::serialize;
use clap::Parser;
use nyxis::stream_reader::{complete_nyxo_end, StreamReader};
use nyxis::writer::{write_stream_file_footer, write_stream_file_header, NxsWriter, Schema, Slot};
use prost::Message;

mod flat8_capnp {
    include!(concat!(env!("OUT_DIR"), "/flat8_capnp.rs"));
}
mod flat8_pb {
    include!(concat!(env!("OUT_DIR"), "/nyxis.bench.rs"));
}

use flat8_pb::Flat8Record;

#[derive(Parser)]
#[command(name = "bench-stream-d")]
struct Args {
    /// Full dataset size (JSON rows loaded / seal benchmark size).
    #[arg(long, default_value_t = 1000)]
    records: usize,
    /// TTFR trials (each trial writes only `ttfr_records` rows). Spec: 1000 for publication P99.
    #[arg(long, default_value_t = 1000)]
    runs: usize,
    /// Reader poll interval while waiting for first record (diagnose P99 vs poll jitter).
    #[arg(long, default_value_t = 50)]
    poll_us: u64,
    /// Records written per TTFR trial before waiting for the reader (use 1 for TTFR).
    #[arg(long, default_value_t = 1)]
    ttfr_records: usize,
    /// Records written for seal-latency trials (0 = skip seal benchmark).
    #[arg(long, default_value_t = 100)]
    seal_records: usize,
    /// Seal-latency trials.
    #[arg(long, default_value_t = 5)]
    seal_runs: usize,
    #[arg(long, default_value = "1")]
    flush_every: usize,
    #[arg(long, default_value = "nxs,proto,capnp")]
    formats: String,
    #[arg(long)]
    json: Option<PathBuf>,
    /// Break down seal into tail-index write vs fsync; include synthetic sync sizes.
    #[arg(long)]
    seal_profile: bool,
    /// Measure sustained records/s after first record (0 = use full dataset, capped at 10k).
    #[arg(long, default_value_t = 0)]
    throughput_records: usize,
}

#[derive(Debug, serde::Serialize)]
struct SealProfileResult {
    seal_records: usize,
    seal_runs: usize,
    tail_index_bytes: usize,
    tail_write_us: Percentiles,
    sync_after_seal_us: Percentiles,
    seal_total_us: Percentiles,
    synthetic_sync_us: Vec<SyntheticSync>,
}

#[derive(Debug, serde::Serialize)]
struct SyntheticSync {
    label: String,
    bytes: usize,
    sync_us_p50: u64,
}

#[derive(Debug, serde::Serialize)]
struct FormatResult {
    format: String,
    variant: String,
    mechanism: &'static str,
    streaming_native: bool,
    flush_every: usize,
    records: usize,
    ttfr_records_per_trial: usize,
    runs: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttfr_us: Option<Percentiles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    p99_note: Option<&'static str>,
    seal_us_p50: Option<u64>,
    seal_records: Option<usize>,
    throughput_rec_per_s_p50: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    footnote: Option<&'static str>,
}

fn flatbuffers_na(flush_every: usize, records: usize) -> FormatResult {
    FormatResult {
        format: "flatbuffers".into(),
        variant: "d2_file".into(),
        mechanism: "Root offset table at buffer start; complete buffer required",
        streaming_native: false,
        flush_every,
        records,
        ttfr_records_per_trial: 0,
        runs: 0,
        ttfr_us: None,
        p99_note: None,
        seal_us_p50: None,
        seal_records: None,
        throughput_rec_per_s_p50: None,
        footnote: Some(
            "FlatBuffers does not support native file-level streaming. TTFR equals total \
             file transfer time. Streaming requires external message framing (comparable to \
             Cap'n Proto framed numbers above).",
        ),
    }
}

fn throughput_n(args: &Args, dataset: usize) -> usize {
    let cap = 10_000;
    if args.throughput_records == 0 {
        dataset.min(cap)
    } else {
        args.throughput_records.min(dataset).min(cap)
    }
}

fn p99_note_for_runs(runs: usize) -> Option<&'static str> {
    if runs < 1000 {
        Some("P99 from n<1000 trials (worst ~2 obs at n=200); use --runs 1000 for publication")
    } else {
        None
    }
}

#[derive(Debug, serde::Serialize)]
struct Percentiles {
    p50: u64,
    p95: u64,
    p99: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let records = load_records(&args)?;
    if records.is_empty() {
        return Err("no records loaded".into());
    }

    if args.seal_profile {
        let seal_n = if args.seal_records == 0 {
            records.len()
        } else {
            args.seal_records.min(records.len())
        };
        let profile = run_seal_profile(&records, seal_n, args.seal_runs.max(1), args.flush_every)?;
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    let flush_every = args.flush_every.max(1);
    let ttfr_n = args.ttfr_records.min(records.len()).max(1);
    let seal_n = if args.seal_records == 0 {
        0
    } else {
        args.seal_records.min(records.len())
    };

    let poll_us = args.poll_us.max(1);
    eprintln!(
        "stream_d: dataset={} ttfr_trials={} ttfr_records/trial={} poll_us={} seal_trials={} seal_records={}",
        records.len(),
        args.runs,
        ttfr_n,
        poll_us,
        args.seal_runs,
        seal_n
    );

    let mut out: Vec<FormatResult> = Vec::new();
    for fmt in args
        .formats
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match fmt {
            "nxs" => out.push(run_nxs_d2(
                &records,
                args.runs,
                ttfr_n,
                seal_n,
                args.seal_runs,
                flush_every,
                poll_us,
                throughput_n(&args, records.len()),
            )?),
            "proto" => out.push(run_proto_d2(
                &records,
                args.runs,
                ttfr_n,
                seal_n,
                args.seal_runs,
                flush_every,
                poll_us,
                throughput_n(&args, records.len()),
            )?),
            "capnp" => out.push(run_capnp_d2(
                &records,
                args.runs,
                ttfr_n,
                flush_every,
                poll_us,
                throughput_n(&args, records.len()),
            )?),
            "fb" | "flatbuffers" => out.push(flatbuffers_na(flush_every, records.len())),
            other => eprintln!("unknown format {other:?}, skipping"),
        }
    }

    println!("{}", serde_json::to_string_pretty(&out)?);
    emit_harness_jsonl(&out);
    Ok(())
}

/// One JSON object per line for bench/scripts/report.py (same shape as A/B/C harness).
fn emit_harness_jsonl(results: &[FormatResult]) {
    for r in results {
        let fmt = match r.format.as_str() {
            "flatbuffers" => "fb",
            other => other,
        };
        let base = serde_json::json!({
            "workload": "D",
            "format": fmt,
            "records": r.records,
            "population": -1.0,
            "flush_every": r.flush_every,
            "variant": r.variant,
            "driver": "stream_d",
        });
        if let Some(ttfr) = &r.ttfr_us {
            let mut row = base.clone();
            if let Some(obj) = row.as_object_mut() {
                // Publication TTFR: n≥1000 trials, batched flush (flush_every≥100).
                let metric = if r.runs >= 1000 && r.flush_every >= 100 {
                    "ttfr"
                } else {
                    "ttfr_smoke"
                };
                obj.insert("metric".into(), serde_json::json!(metric));
                obj.insert("p50_us".into(), serde_json::json!(ttfr.p50));
                obj.insert("p95_us".into(), serde_json::json!(ttfr.p95));
                obj.insert("p99_us".into(), serde_json::json!(ttfr.p99));
                obj.insert("samples".into(), serde_json::json!(r.runs));
            }
            let line = serde_json::to_string(&row).expect("jsonl row");
            println!("{line}");
        }
        if let Some(seal_us) = r.seal_us_p50 {
            let mut row = base.clone();
            if let Some(obj) = row.as_object_mut() {
                let seal_n = r.seal_records.unwrap_or(0);
                let metric = if r.flush_every >= 100 && seal_n >= r.records.saturating_sub(1) {
                    "seal"
                } else {
                    "seal_smoke"
                };
                obj.insert("metric".into(), serde_json::json!(metric));
                obj.insert("p50_us".into(), serde_json::json!(seal_us));
                obj.insert(
                    "seal_records".into(),
                    serde_json::json!(r.seal_records.unwrap_or(0)),
                );
                obj.insert("samples".into(), serde_json::json!(r.runs));
            }
            let line = serde_json::to_string(&row).expect("jsonl row");
            println!("{line}");
        }
        if let Some(tput) = r.throughput_rec_per_s_p50 {
            let mut row = base.clone();
            if let Some(obj) = row.as_object_mut() {
                // flush_every=1 smoke runs include poll/fsync overhead — not publication throughput.
                let metric = if r.flush_every >= 100 {
                    "throughput"
                } else {
                    "throughput_smoke"
                };
                obj.insert("metric".into(), serde_json::json!(metric));
                obj.insert("p50_rec_per_s".into(), serde_json::json!(tput));
            }
            let line = serde_json::to_string(&row).expect("jsonl row");
            println!("{line}");
        }
    }
}

fn load_records(args: &Args) -> Result<Vec<Flat8Record>, Box<dyn std::error::Error>> {
    if let Some(path) = &args.json {
        let text = fs::read_to_string(path)?;
        let raw: Vec<serde_json::Value> = serde_json::from_str(&text)?;
        return Ok(raw
            .into_iter()
            .take(args.records)
            .map(|v| Flat8Record {
                id: v["id"].as_i64().unwrap_or(0),
                username: v["username"].as_str().unwrap_or("").to_string(),
                email: v["email"].as_str().unwrap_or("").to_string(),
                age: v["age"].as_i64().unwrap_or(0),
                balance: v["balance"].as_f64().unwrap_or(0.0),
                active: v["active"].as_bool().unwrap_or(false),
                score: v["score"].as_f64().unwrap_or(0.0),
                created_at: v["created_at"].as_i64().unwrap_or(0),
            })
            .collect());
    }
    Ok((0..args.records)
        .map(|i| Flat8Record {
            id: i as i64,
            username: format!("user_{i:07}"),
            email: format!("user{i}@example.com"),
            age: (20 + (i % 50)) as i64,
            balance: 100.0 + i as f64 * 1.37,
            active: i % 3 != 0,
            score: ((i % 100) as f64) / 10.0,
            created_at: 1_777_593_600_000_000_000,
        })
        .collect())
}

fn percentile(mut xs: Vec<u64>, p: f64) -> u64 {
    if xs.is_empty() {
        return 0;
    }
    xs.sort_unstable();
    let idx = ((xs.len() as f64 - 1.0) * p).round() as usize;
    xs[idx.min(xs.len() - 1)]
}

fn percentiles(xs: Vec<u64>) -> Percentiles {
    Percentiles {
        p50: percentile(xs.clone(), 0.50),
        p95: percentile(xs.clone(), 0.95),
        p99: percentile(xs, 0.99),
    }
}

/// Read only new tail bytes from a growing file (one FD; seek-append reads).
struct IncrementalReader {
    file: Option<File>,
    buf: Vec<u8>,
}

impl IncrementalReader {
    fn new() -> Self {
        Self {
            file: None,
            buf: Vec::new(),
        }
    }

    fn poll(&mut self, path: &PathBuf) -> &[u8] {
        let Ok(meta) = fs::metadata(path) else {
            self.file = None;
            return &self.buf;
        };
        let file_len = meta.len() as usize;
        if file_len < self.buf.len() {
            self.buf.clear();
            self.file = None;
        }
        if self.file.is_none() {
            self.file = File::open(path).ok();
        }
        let Some(f) = self.file.as_mut() else {
            return &self.buf;
        };
        if file_len <= self.buf.len() {
            return &self.buf;
        }
        if f.seek(SeekFrom::Start(self.buf.len() as u64)).is_err() {
            self.file = None;
            return &self.buf;
        }
        let mut tail = Vec::with_capacity(file_len - self.buf.len());
        if let Err(e) = f.read_to_end(&mut tail) {
            eprintln!("stream_d: incremental read error: {e}");
            self.file = None;
            return &self.buf;
        }
        self.buf.extend_from_slice(&tail);
        &self.buf
    }
}

struct ProtoScanState {
    parse_pos: usize,
    count: usize,
}

impl ProtoScanState {
    fn new() -> Self {
        Self {
            parse_pos: 0,
            count: 0,
        }
    }

    fn advance(&mut self, buf: &[u8]) -> usize {
        let mut pos = self.parse_pos;
        while pos < buf.len() {
            let Some((len, consumed)) = decode_varint(&buf[pos..]) else {
                break;
            };
            pos += consumed;
            if pos + len > buf.len() {
                break;
            }
            pos += len;
            self.count += 1;
        }
        self.parse_pos = pos;
        self.count
    }
}

struct CapnpScanState {
    cursor_pos: usize,
    count: usize,
}

impl CapnpScanState {
    fn new() -> Self {
        Self {
            cursor_pos: 0,
            count: 0,
        }
    }

    fn advance(&mut self, buf: &[u8]) -> usize {
        let mut cursor = std::io::Cursor::new(&buf[self.cursor_pos..]);
        while serialize::read_message(&mut cursor, ReaderOptions::new()).is_ok() {
            self.count += 1;
        }
        self.cursor_pos += cursor.position() as usize;
        self.count
    }
}

fn poll_nxs_ttfr(
    path: &PathBuf,
    start_reader: Arc<AtomicU64>,
    deadline: Instant,
    poll_us: u64,
) -> u64 {
    let mut inc = IncrementalReader::new();
    loop {
        if Instant::now() > deadline {
            eprintln!("stream_d: nxs reader timed out waiting for first record");
            return 0;
        }
        let buf = inc.poll(path);
        if let Ok(sr) = StreamReader::open(buf) {
            if sr.has_first_complete() {
                let _ = sr.get_i64_at(sr.data_start(), "id");
                let t0w = start_reader.load(Ordering::Acquire);
                if t0w != 0 {
                    return monotonic_ns().saturating_sub(t0w);
                }
            }
        }
        thread::sleep(Duration::from_micros(poll_us));
    }
}

fn poll_capnp_ttfr(
    path: &PathBuf,
    start_reader: Arc<AtomicU64>,
    deadline: Instant,
    poll_us: u64,
) -> u64 {
    let mut inc = IncrementalReader::new();
    loop {
        if Instant::now() > deadline {
            eprintln!("stream_d: capnp reader timed out waiting for first record");
            return 0;
        }
        let buf = inc.poll(path);
        if capnp_first_record_ready(buf) {
            let t0 = start_reader.load(Ordering::Acquire);
            if t0 != 0 {
                return monotonic_ns().saturating_sub(t0);
            }
        }
        thread::sleep(Duration::from_micros(poll_us));
    }
}

fn poll_proto_ttfr(
    path: &PathBuf,
    start_reader: Arc<AtomicU64>,
    deadline: Instant,
    poll_us: u64,
) -> u64 {
    let mut inc = IncrementalReader::new();
    loop {
        if Instant::now() > deadline {
            eprintln!("stream_d: proto reader timed out waiting for first record");
            return 0;
        }
        let buf = inc.poll(path);
        if let Some((msg, _)) = first_delimited_message(buf) {
            if Flat8Record::decode(msg).is_ok() {
                let t0 = start_reader.load(Ordering::Acquire);
                if t0 != 0 {
                    return monotonic_ns().saturating_sub(t0);
                }
            }
        }
        thread::sleep(Duration::from_micros(poll_us));
    }
}

/// Records/s from first complete record until all `n` records are visible (writer still appending).
fn measure_nxs_throughput(
    records: &[Flat8Record],
    n: usize,
    flush_every: usize,
    poll_us: u64,
) -> Result<f64, Box<dyn std::error::Error>> {
    let keys = [
        "id",
        "username",
        "email",
        "age",
        "balance",
        "active",
        "score",
        "created_at",
    ];
    let schema = Schema::new(&keys);
    let slots: [Slot; 8] = std::array::from_fn(|i| Slot(i as u16));
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("stream.nxb");
    let writer_done = Arc::new(AtomicBool::new(false));
    let records_seen = Arc::new(AtomicUsize::new(0));
    let t_first = Arc::new(AtomicU64::new(0));
    let t_last = Arc::new(AtomicU64::new(0));
    let data_start = Arc::new(AtomicU64::new(0));

    let path_r = path.clone();
    let wd = Arc::clone(&writer_done);
    let seen = Arc::clone(&records_seen);
    let tf = Arc::clone(&t_first);
    let tl = Arc::clone(&t_last);
    let ds = Arc::clone(&data_start);

    let reader = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut inc = IncrementalReader::new();
        let mut scan_off = 0usize;
        let mut count = 0usize;
        loop {
            if Instant::now() > deadline {
                return;
            }
            let buf = inc.poll(&path_r);
            if ds.load(Ordering::Acquire) == 0 {
                if let Ok(sr) = StreamReader::open(buf) {
                    ds.store(sr.data_start() as u64, Ordering::Release);
                    scan_off = sr.data_start();
                }
            } else if scan_off == 0 {
                scan_off = ds.load(Ordering::Acquire) as usize;
            }
            while let Some(end) = complete_nyxo_end(buf, scan_off) {
                scan_off = end;
                count += 1;
            }
            let prev = seen.load(Ordering::Acquire);
            if count > prev {
                if prev == 0 {
                    tf.store(monotonic_ns(), Ordering::Release);
                }
                seen.store(count, Ordering::Release);
                tl.store(monotonic_ns(), Ordering::Release);
            }
            if count >= n && wd.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_micros(poll_us));
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    let data_start_abs = write_stream_file_header(&mut file, &schema)?;
    data_start.store(data_start_abs, Ordering::Release);
    let mut w = NxsWriter::with_capacity(&schema, n * 128 + 4096);
    let mut flushed = 0usize;
    for (i, r) in records.iter().take(n).enumerate() {
        write_nxs_record(&mut w, &mut file, &mut flushed, &slots, r)?;
        if (i + 1) % flush_every == 0 {
            file.flush()?;
            file.sync_data()?;
        }
    }
    file.flush()?;
    file.sync_data()?;
    writer_done.store(true, Ordering::Release);
    reader.join().ok();

    let tfv = t_first.load(Ordering::Acquire);
    let tlv = t_last.load(Ordering::Acquire);
    let cnt = records_seen.load(Ordering::Acquire);
    if tfv == 0 || tlv <= tfv || cnt < 2 {
        return Ok(0.0);
    }
    let secs = (tlv - tfv) as f64 / 1_000_000_000.0;
    Ok((cnt - 1) as f64 / secs.max(1e-9))
}

fn measure_proto_throughput(
    records: &[Flat8Record],
    n: usize,
    flush_every: usize,
    poll_us: u64,
) -> Result<f64, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("stream.pb");
    let writer_done = Arc::new(AtomicBool::new(false));
    let records_seen = Arc::new(AtomicUsize::new(0));
    let t_first = Arc::new(AtomicU64::new(0));
    let t_last = Arc::new(AtomicU64::new(0));
    let path_r = path.clone();
    let wd = Arc::clone(&writer_done);
    let seen = Arc::clone(&records_seen);
    let tf = Arc::clone(&t_first);
    let tl = Arc::clone(&t_last);

    let reader = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut inc = IncrementalReader::new();
        let mut scan = ProtoScanState::new();
        loop {
            if Instant::now() > deadline {
                return;
            }
            let buf = inc.poll(&path_r);
            let count = scan.advance(buf);
            let prev = seen.load(Ordering::Acquire);
            if count > prev {
                if prev == 0 {
                    tf.store(monotonic_ns(), Ordering::Release);
                }
                seen.store(count, Ordering::Release);
                tl.store(monotonic_ns(), Ordering::Release);
            }
            if count >= n && wd.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_micros(poll_us));
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    for (i, r) in records.iter().take(n).enumerate() {
        write_delimited(&mut file, r)?;
        if (i + 1) % flush_every == 0 {
            file.flush()?;
            file.sync_data()?;
        }
    }
    file.flush()?;
    file.sync_data()?;
    writer_done.store(true, Ordering::Release);
    reader.join().ok();

    let tfv = t_first.load(Ordering::Acquire);
    let tlv = t_last.load(Ordering::Acquire);
    let cnt = records_seen.load(Ordering::Acquire);
    if tfv == 0 || tlv <= tfv || cnt < 2 {
        return Ok(0.0);
    }
    Ok((cnt - 1) as f64 / ((tlv - tfv) as f64 / 1_000_000_000.0).max(1e-9))
}

fn measure_capnp_throughput(
    records: &[Flat8Record],
    n: usize,
    flush_every: usize,
    poll_us: u64,
) -> Result<f64, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("stream.capnp");
    let writer_done = Arc::new(AtomicBool::new(false));
    let records_seen = Arc::new(AtomicUsize::new(0));
    let t_first = Arc::new(AtomicU64::new(0));
    let t_last = Arc::new(AtomicU64::new(0));
    let path_r = path.clone();
    let wd = Arc::clone(&writer_done);
    let seen = Arc::clone(&records_seen);
    let tf = Arc::clone(&t_first);
    let tl = Arc::clone(&t_last);

    let reader = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut inc = IncrementalReader::new();
        let mut scan = CapnpScanState::new();
        loop {
            if Instant::now() > deadline {
                return;
            }
            let buf = inc.poll(&path_r);
            let count = scan.advance(buf);
            let prev = seen.load(Ordering::Acquire);
            if count > prev {
                if prev == 0 {
                    tf.store(monotonic_ns(), Ordering::Release);
                }
                seen.store(count, Ordering::Release);
                tl.store(monotonic_ns(), Ordering::Release);
            }
            if count >= n && wd.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_micros(poll_us));
        }
    });

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;
    for (i, r) in records.iter().take(n).enumerate() {
        write_capnp_record(&mut file, r)?;
        if (i + 1) % flush_every == 0 {
            file.flush()?;
            file.sync_data()?;
        }
    }
    file.flush()?;
    file.sync_data()?;
    writer_done.store(true, Ordering::Release);
    reader.join().ok();

    let tfv = t_first.load(Ordering::Acquire);
    let tlv = t_last.load(Ordering::Acquire);
    let cnt = records_seen.load(Ordering::Acquire);
    if tfv == 0 || tlv <= tfv || cnt < 2 {
        return Ok(0.0);
    }
    Ok((cnt - 1) as f64 / ((tlv - tfv) as f64 / 1_000_000_000.0).max(1e-9))
}

fn write_nxs_record(
    w: &mut NxsWriter,
    file: &mut File,
    flushed: &mut usize,
    slots: &[Slot; 8],
    r: &Flat8Record,
) -> std::io::Result<u64> {
    let record_start = file.stream_position()?;
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
    w.write_data_sector_since(file, *flushed)?;
    *flushed = w.data_sector_len();
    Ok(record_start)
}

fn tail_index_bytes(record_count: usize) -> usize {
    4 + record_count * 10 + 12
}

fn run_seal_profile(
    records: &[Flat8Record],
    seal_records: usize,
    seal_runs: usize,
    flush_every: usize,
) -> Result<SealProfileResult, Box<dyn std::error::Error>> {
    let keys = [
        "id",
        "username",
        "email",
        "age",
        "balance",
        "active",
        "score",
        "created_at",
    ];
    let schema = Schema::new(&keys);
    let slots: [Slot; 8] = std::array::from_fn(|i| Slot(i as u16));
    let flush_every = flush_every.max(1);

    let mut tail_write = Vec::with_capacity(seal_runs);
    let mut sync_after = Vec::with_capacity(seal_runs);
    let mut seal_total = Vec::with_capacity(seal_runs);

    for trial in 0..seal_runs {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("stream.nxb");
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        let data_start = write_stream_file_header(&mut file, &schema)?;
        let mut w = NxsWriter::with_capacity(&schema, seal_records * 128 + 4096);
        let mut flushed = 0usize;
        let mut abs_offsets = Vec::with_capacity(seal_records);

        for (i, r) in records.iter().take(seal_records).enumerate() {
            let pos = write_nxs_record(&mut w, &mut file, &mut flushed, &slots, r)?;
            abs_offsets.push(pos);
            if (i + 1) % flush_every == 0 {
                file.flush()?;
                file.sync_data()?;
            }
        }
        file.flush()?;
        file.sync_data()?;

        let t0 = monotonic_ns();
        write_stream_file_footer(&mut file, data_start, &abs_offsets)?;
        let t1 = monotonic_ns();
        file.sync_all()?;
        let t2 = monotonic_ns();

        tail_write.push((t1.saturating_sub(t0)) / 1000);
        sync_after.push((t2.saturating_sub(t1)) / 1000);
        seal_total.push((t2.saturating_sub(t0)) / 1000);
        eprintln!("stream_d: seal-profile {}/{} done", trial + 1, seal_runs);
    }

    let synthetic_sizes = [
        ("100kb_tail", tail_index_bytes(10_000)),
        ("1mb_tail", tail_index_bytes(100_000)),
        ("10mb_tail", tail_index_bytes(1_000_000)),
    ];
    let mut synthetic_sync_us = Vec::new();
    for (label, bytes) in synthetic_sizes {
        synthetic_sync_us.push(SyntheticSync {
            label: label.into(),
            bytes,
            sync_us_p50: synthetic_sync_p50(bytes)?,
        });
    }

    Ok(SealProfileResult {
        seal_records,
        seal_runs,
        tail_index_bytes: tail_index_bytes(seal_records),
        tail_write_us: percentiles(tail_write),
        sync_after_seal_us: percentiles(sync_after),
        seal_total_us: percentiles(seal_total),
        synthetic_sync_us,
    })
}

/// Time `sync_all` on a file containing `bytes` of payload (5 trials, return P50 µs).
fn synthetic_sync_p50(bytes: usize) -> Result<u64, Box<dyn std::error::Error>> {
    let mut samples = Vec::with_capacity(5);
    for _ in 0..5 {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("payload.bin");
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(&vec![0u8; bytes])?;
        file.flush()?;
        let t0 = monotonic_ns();
        file.sync_all()?;
        samples.push((monotonic_ns().saturating_sub(t0)) / 1000);
    }
    Ok(percentile(samples, 0.50))
}

fn run_nxs_d2(
    records: &[Flat8Record],
    runs: usize,
    ttfr_records: usize,
    seal_records: usize,
    seal_runs: usize,
    flush_every: usize,
    poll_us: u64,
    throughput_n: usize,
) -> Result<FormatResult, Box<dyn std::error::Error>> {
    let keys = [
        "id",
        "username",
        "email",
        "age",
        "balance",
        "active",
        "score",
        "created_at",
    ];
    let schema = Schema::new(&keys);
    let slots: [Slot; 8] = std::array::from_fn(|i| Slot(i as u16));

    let mut ttfr_samples = Vec::with_capacity(runs);
    let trial_dir = tempfile::tempdir()?;
    for trial in 0..runs {
        let path = trial_dir.path().join(format!("ttfr_{trial}.nxb"));
        let start_ns = Arc::new(AtomicU64::new(0));
        let path_reader = path.clone();
        let start_reader = Arc::clone(&start_ns);
        let deadline = Instant::now() + Duration::from_secs(5);

        let reader =
            thread::spawn(move || poll_nxs_ttfr(&path_reader, start_reader, deadline, poll_us));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        write_stream_file_header(&mut file, &schema)?;
        let mut w = NxsWriter::with_capacity(&schema, 4096);
        let mut flushed = 0usize;

        for (i, r) in records.iter().take(ttfr_records).enumerate() {
            if i == 0 {
                start_ns.store(monotonic_ns(), Ordering::Release);
            }
            write_nxs_record(&mut w, &mut file, &mut flushed, &slots, r)?;
            if i == 0 || (i + 1) % flush_every == 0 {
                file.flush()?;
                file.sync_data()?;
            }
        }

        let ttfr_ns = reader.join().unwrap();
        if ttfr_ns == 0 {
            return Err(format!("nxs TTFR trial {trial} failed (timeout or race)").into());
        }
        ttfr_samples.push(ttfr_ns / 1000);
        if (trial + 1) % 10 == 0 || trial + 1 == runs {
            eprintln!("stream_d: nxs ttfr {}/{} done", trial + 1, runs);
        }
    }

    let mut seal_samples = Vec::new();
    if seal_records > 0 && seal_runs > 0 {
        let seal_dir = tempfile::tempdir()?;
        for trial in 0..seal_runs {
            let path = seal_dir.path().join(format!("seal_{trial}.nxb"));
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            let data_start = write_stream_file_header(&mut file, &schema)?;
            let mut w = NxsWriter::with_capacity(&schema, seal_records * 128 + 4096);
            let mut flushed = 0usize;
            let mut abs_offsets = Vec::with_capacity(seal_records);

            for (i, r) in records.iter().take(seal_records).enumerate() {
                let pos = write_nxs_record(&mut w, &mut file, &mut flushed, &slots, r)?;
                abs_offsets.push(pos);
                if (i + 1) % flush_every == 0 {
                    file.flush()?;
                    file.sync_data()?;
                }
            }
            file.flush()?;
            file.sync_data()?;

            let seal_start = monotonic_ns();
            write_stream_file_footer(&mut file, data_start, &abs_offsets)?;
            file.sync_all()?;
            seal_samples.push((monotonic_ns().saturating_sub(seal_start)) / 1000);
            eprintln!("stream_d: nxs seal {}/{} done", trial + 1, seal_runs);
        }
    }

    let throughput = if throughput_n >= 100 {
        eprintln!("stream_d: nxs throughput (n={throughput_n})...");
        Some(measure_nxs_throughput(
            records,
            throughput_n,
            flush_every,
            poll_us,
        )?)
    } else {
        None
    };

    Ok(FormatResult {
        format: "nxs".into(),
        variant: "d2_file".into(),
        mechanism: "NYXO cell, self-delimiting (magic + length)",
        streaming_native: true,
        flush_every,
        records: records.len(),
        ttfr_records_per_trial: ttfr_records,
        runs,
        ttfr_us: if runs > 0 {
            Some(percentiles(ttfr_samples))
        } else {
            None
        },
        p99_note: p99_note_for_runs(runs),
        seal_us_p50: if seal_samples.is_empty() {
            None
        } else {
            Some(percentile(seal_samples, 0.50))
        },
        seal_records: if seal_records > 0 {
            Some(seal_records)
        } else {
            None
        },
        throughput_rec_per_s_p50: throughput,
        footnote: None,
    })
}

fn capnp_first_record_ready(buf: &[u8]) -> bool {
    let mut cursor = std::io::Cursor::new(buf);
    let Ok(message) = serialize::read_message(&mut cursor, ReaderOptions::new()) else {
        return false;
    };
    message
        .get_root::<flat8_capnp::flat8_record::Reader>()
        .is_ok()
}

fn write_capnp_record<W: Write>(w: &mut W, r: &Flat8Record) -> std::io::Result<()> {
    let mut message = Builder::new_default();
    {
        let mut rec = message.init_root::<flat8_capnp::flat8_record::Builder>();
        rec.set_id(r.id);
        rec.set_username(&r.username);
        rec.set_email(&r.email);
        rec.set_age(r.age);
        rec.set_balance(r.balance);
        rec.set_active(r.active);
        rec.set_score(r.score);
        rec.set_created_at(r.created_at);
    }
    serialize::write_message(w, &message).map_err(|e| std::io::Error::other(e.to_string()))
}

fn run_capnp_d2(
    records: &[Flat8Record],
    runs: usize,
    ttfr_records: usize,
    flush_every: usize,
    poll_us: u64,
    throughput_n: usize,
) -> Result<FormatResult, Box<dyn std::error::Error>> {
    let mut ttfr_samples = Vec::with_capacity(runs);
    let trial_dir = tempfile::tempdir()?;

    for trial in 0..runs {
        let path = trial_dir.path().join(format!("ttfr_{trial}.capnp"));
        let start_ns = Arc::new(AtomicU64::new(0));
        let path_reader = path.clone();
        let start_reader = Arc::clone(&start_ns);
        let deadline = Instant::now() + Duration::from_secs(5);

        let reader =
            thread::spawn(move || poll_capnp_ttfr(&path_reader, start_reader, deadline, poll_us));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        for (i, r) in records.iter().take(ttfr_records).enumerate() {
            if i == 0 {
                start_ns.store(monotonic_ns(), Ordering::Release);
            }
            write_capnp_record(&mut file, r)?;
            if i == 0 || (i + 1) % flush_every == 0 {
                file.flush()?;
                file.sync_data()?;
            }
        }

        let ttfr_ns = reader.join().unwrap();
        if ttfr_ns == 0 {
            return Err(format!("capnp TTFR trial {trial} failed (timeout or race)").into());
        }
        ttfr_samples.push(ttfr_ns / 1000);
        if (trial + 1) % 10 == 0 || trial + 1 == runs {
            eprintln!("stream_d: capnp ttfr {}/{} done", trial + 1, runs);
        }
    }

    let throughput = if throughput_n >= 100 {
        eprintln!("stream_d: capnp throughput (n={throughput_n})...");
        Some(measure_capnp_throughput(
            records,
            throughput_n,
            flush_every,
            poll_us,
        )?)
    } else {
        None
    };

    Ok(FormatResult {
        format: "capnp".into(),
        variant: "d2_file".into(),
        mechanism: "Segment framing (fixed header per message)",
        streaming_native: true,
        flush_every,
        records: records.len(),
        ttfr_records_per_trial: ttfr_records,
        runs,
        ttfr_us: if runs > 0 {
            Some(percentiles(ttfr_samples))
        } else {
            None
        },
        p99_note: p99_note_for_runs(runs),
        seal_us_p50: None,
        seal_records: None,
        throughput_rec_per_s_p50: throughput,
        footnote: None,
    })
}

fn run_proto_d2(
    records: &[Flat8Record],
    runs: usize,
    ttfr_records: usize,
    seal_records: usize,
    seal_runs: usize,
    flush_every: usize,
    poll_us: u64,
    throughput_n: usize,
) -> Result<FormatResult, Box<dyn std::error::Error>> {
    let mut ttfr_samples = Vec::with_capacity(runs);
    let trial_dir = tempfile::tempdir()?;

    for trial in 0..runs {
        let path = trial_dir.path().join(format!("ttfr_{trial}.pb"));
        let start_ns = Arc::new(AtomicU64::new(0));
        let path_reader = path.clone();
        let start_reader = Arc::clone(&start_ns);
        let deadline = Instant::now() + Duration::from_secs(5);

        let reader =
            thread::spawn(move || poll_proto_ttfr(&path_reader, start_reader, deadline, poll_us));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        for (i, r) in records.iter().take(ttfr_records).enumerate() {
            if i == 0 {
                start_ns.store(monotonic_ns(), Ordering::Release);
            }
            write_delimited(&mut file, r)?;
            if i == 0 || (i + 1) % flush_every == 0 {
                file.flush()?;
                file.sync_data()?;
            }
        }

        let ttfr_ns = reader.join().unwrap();
        if ttfr_ns == 0 {
            return Err(format!("proto TTFR trial {trial} failed (timeout or race)").into());
        }
        ttfr_samples.push(ttfr_ns / 1000);
        if (trial + 1) % 10 == 0 || trial + 1 == runs {
            eprintln!("stream_d: proto ttfr {}/{} done", trial + 1, runs);
        }
    }

    // Proto has no seal step — optional full-file write for sanity only (not timed as seal).
    let _ = (seal_records, seal_runs);

    let throughput = if throughput_n >= 100 {
        eprintln!("stream_d: proto throughput (n={throughput_n})...");
        Some(measure_proto_throughput(
            records,
            throughput_n,
            flush_every,
            poll_us,
        )?)
    } else {
        None
    };

    Ok(FormatResult {
        format: "proto".into(),
        variant: "d2_file".into(),
        mechanism: "Varint length-prefix per record",
        streaming_native: true,
        flush_every,
        records: records.len(),
        ttfr_records_per_trial: ttfr_records,
        runs,
        ttfr_us: if runs > 0 {
            Some(percentiles(ttfr_samples))
        } else {
            None
        },
        p99_note: p99_note_for_runs(runs),
        seal_us_p50: None,
        seal_records: None,
        throughput_rec_per_s_p50: throughput,
        footnote: None,
    })
}

fn write_delimited<W: Write>(w: &mut W, msg: &Flat8Record) -> std::io::Result<()> {
    let bytes = msg.encode_to_vec();
    let mut buf = Vec::with_capacity(10 + bytes.len());
    prost::encoding::encode_varint(bytes.len() as u64, &mut buf);
    buf.extend_from_slice(&bytes);
    w.write_all(&buf)
}

fn decode_varint(buf: &[u8]) -> Option<(usize, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;
    for (i, &b) in buf.iter().enumerate() {
        if i >= 10 {
            return None;
        }
        result |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some((result as usize, i + 1));
        }
        shift += 7;
    }
    None
}

fn first_delimited_message(buf: &[u8]) -> Option<(&[u8], usize)> {
    let (len, consumed) = decode_varint(buf)?;
    let pos = consumed;
    if pos + len > buf.len() {
        return None;
    }
    Some((&buf[pos..pos + len], pos + len))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn monotonic_ns() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}
