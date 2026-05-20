//! `nxs-trace` — streaming span ingestion and query CLI.
//!
//! # Subcommands
//!
//!   nxs-trace write  --dir <DIR> [--seal-every <N>]
//!       Reads newline-delimited JSON spans from stdin, appends to the live WAL,
//!       and auto-seals to a .nxb segment when --seal-every spans have been written.
//!
//!   nxs-trace seal   --dir <DIR>
//!       Seal the live WAL immediately, producing a new .nxb segment.
//!
//!   nxs-trace query  --dir <DIR> --trace-id <HEX>
//!       Print all spans for a trace as JSON.
//!
//!   nxs-trace query  --dir <DIR> --from <RFC3339> --to <RFC3339>
//!       Print all spans in a time window.
//!
//!   nxs-trace stats  --dir <DIR>
//!       Print segment count and record counts.
//!
//! # Input JSON schema (one object per line)
//!
//!   {
//!     "trace_id":       "4bf92f3577b34da6a3ce929d0e0e4736",  // 32-hex
//!     "span_id":        "00f067aa0ba902b7",                   // 16-hex
//!     "parent_span_id": "00f067aa0ba902b7" | null,
//!     "name":           "my_operation",
//!     "service":        "my_service",
//!     "start_time_ns":  1715000000000000000,
//!     "duration_ns":    1234567,
//!     "status_code":    0,
//!     "payload":        { ... }   // optional, stored verbatim as bytes
//!   }

use clap::{Parser, Subcommand};
use nxs::segment_reader::SegmentReader;
use nxs::wal::{SpanFields, SpanWal};
use serde_json::Value;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(name = "nxs-trace", about = "NXS span/trace ingestion and query")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest newline-delimited JSON spans from stdin into a WAL
    Write {
        #[arg(long, value_name = "DIR")]
        dir: PathBuf,

        /// Seal to a .nxb segment after this many spans (0 = never auto-seal)
        #[arg(long, default_value = "10000")]
        seal_every: u64,
    },

    /// Seal the live WAL immediately
    Seal {
        #[arg(long, value_name = "DIR")]
        dir: PathBuf,
    },

    /// Query spans by trace-id or time window
    Query {
        #[arg(long, value_name = "DIR")]
        dir: PathBuf,

        /// Trace ID as 32-char hex (e.g. 4bf92f3577b34da6a3ce929d0e0e4736)
        #[arg(long, value_name = "HEX", conflicts_with_all = ["from", "to"])]
        trace_id: Option<String>,

        /// Start of time window (Unix nanoseconds)
        #[arg(long, value_name = "NS", requires = "to")]
        from: Option<i64>,

        /// End of time window (Unix nanoseconds)
        #[arg(long, value_name = "NS", requires = "from")]
        to: Option<i64>,

        /// Also include spans from the live WAL
        #[arg(long)]
        include_wal: bool,
    },

    /// Print summary statistics
    Stats {
        #[arg(long, value_name = "DIR")]
        dir: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Write { dir, seal_every } => cmd_write(dir, seal_every),
        Cmd::Seal { dir } => cmd_seal(dir),
        Cmd::Query {
            dir,
            trace_id,
            from,
            to,
            ..
        } => cmd_query(dir, trace_id, from, to),
        Cmd::Stats { dir } => cmd_stats(dir),
    }
}

// ── write ─────────────────────────────────────────────────────────────────────

fn cmd_write(dir: PathBuf, seal_every: u64) {
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("mkdir: {e}")));

    let wal_path = dir.join("live.nxsw");
    let mut wal = SpanWal::open(&wal_path).unwrap_or_else(|e| die(&format!("open wal: {e}")));

    // If the file existed, recover the in-memory index first
    if wal_path.exists() && wal.record_count() == 0 {
        wal.recover()
            .unwrap_or_else(|e| die(&format!("recover: {e}")));
    }

    let stdin = io::stdin();
    let mut lines_read = 0u64;
    let mut spans_written = 0u64;

    for line in stdin.lock().lines() {
        let line = line.unwrap_or_else(|e| die(&format!("stdin: {e}")));
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        lines_read += 1;

        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("warn: skip malformed JSON at line {lines_read}: {e}");
                continue;
            }
        };

        let span = match parse_json_span(&v) {
            Some(s) => s,
            None => {
                eprintln!("warn: skip incomplete span at line {lines_read}");
                continue;
            }
        };

        let payload_bytes: Option<Vec<u8>> = if let Some(p) = v.get("payload") {
            if !p.is_null() {
                Some(p.to_string().into_bytes())
            } else {
                None
            }
        } else {
            None
        };

        let fields = SpanFields {
            trace_id_hi: span.trace_id_hi,
            trace_id_lo: span.trace_id_lo,
            span_id: span.span_id,
            parent_span_id: span.parent_span_id,
            name: span.name.as_str(),
            service: span.service.as_str(),
            start_time_ns: span.start_time_ns,
            duration_ns: span.duration_ns,
            status_code: span.status_code,
            payload: payload_bytes.as_deref(),
        };

        wal.append(&fields)
            .unwrap_or_else(|e| die(&format!("append: {e}")));
        spans_written += 1;

        if seal_every > 0 && wal.record_count() % seal_every == 0 {
            do_seal(&mut wal, &dir);
            // Re-open a fresh WAL
            wal = SpanWal::open(&wal_path)
                .unwrap_or_else(|e| die(&format!("re-open wal after seal: {e}")));
        }
    }

    wal.flush().unwrap_or_else(|e| die(&format!("flush: {e}")));
    eprintln!("wrote {} spans ({} lines read)", spans_written, lines_read);
}

fn do_seal(wal: &mut SpanWal, dir: &PathBuf) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seg_path = dir.join(format!("seg-{ts:016}.nxb"));
    match wal.seal(&seg_path) {
        Ok(report) => eprintln!(
            "sealed {} records → {} ({} B)",
            report.records,
            seg_path.display(),
            report.bytes_written
        ),
        Err(e) => eprintln!("warn: seal failed: {e}"),
    }
    // Remove old WAL after successful seal
    let _ = std::fs::remove_file(wal.path());
}

// ── seal ──────────────────────────────────────────────────────────────────────

fn cmd_seal(dir: PathBuf) {
    let wal_path = dir.join("live.nxsw");
    if !wal_path.exists() {
        eprintln!("no live WAL at {}", wal_path.display());
        std::process::exit(1);
    }
    let mut wal = SpanWal::open(&wal_path).unwrap_or_else(|e| die(&format!("open wal: {e}")));
    wal.recover()
        .unwrap_or_else(|e| die(&format!("recover: {e}")));
    do_seal(&mut wal, &dir);
}

// ── query ─────────────────────────────────────────────────────────────────────

fn cmd_query(dir: PathBuf, trace_id: Option<String>, from: Option<i64>, to: Option<i64>) {
    let reader = SegmentReader::open(&dir).unwrap_or_else(|e| die(&format!("open: {e}")));

    let spans = if let Some(ref hex) = trace_id {
        let tid = parse_trace_id_hex(hex).unwrap_or_else(|| die("invalid trace-id hex"));
        reader
            .find_by_trace(tid)
            .unwrap_or_else(|e| die(&format!("query: {e}")))
    } else if let (Some(from_ns), Some(to_ns)) = (from, to) {
        reader
            .find_by_time(from_ns, to_ns)
            .unwrap_or_else(|e| die(&format!("query: {e}")))
    } else {
        eprintln!("error: provide --trace-id or --from + --to");
        std::process::exit(2);
    };

    println!("[");
    for (i, span) in spans.iter().enumerate() {
        let comma = if i + 1 < spans.len() { "," } else { "" };
        let parent = match span.parent_span_id {
            Some(p) => format!("\"{p:016x}\""),
            None => "null".to_string(),
        };
        let payload_str = match &span.payload {
            Some(b) => {
                format!("{}", String::from_utf8_lossy(b))
            }
            None => "null".to_string(),
        };
        println!(
            "  {{\"trace_id\":\"{:032x}\",\"span_id\":\"{:016x}\",\"parent_span_id\":{},\
\"name\":{},\"service\":{},\"start_time_ns\":{},\"duration_ns\":{},\"status_code\":{},\
\"payload\":{}}}{}",
            span.trace_id,
            span.span_id,
            parent,
            serde_json::to_string(&span.name).unwrap(),
            serde_json::to_string(&span.service).unwrap(),
            span.start_time_ns,
            span.duration_ns,
            span.status_code,
            payload_str,
            comma
        );
    }
    println!("]");
}

// ── stats ─────────────────────────────────────────────────────────────────────

fn cmd_stats(dir: PathBuf) {
    let reader = SegmentReader::open(&dir).unwrap_or_else(|e| die(&format!("open: {e}")));
    let stats = reader.stats();
    println!(
        "segments={} sealed_records={} wal_records={}",
        stats.segment_count, stats.sealed_records, stats.wal_records
    );
}

// ── JSON span parsing ─────────────────────────────────────────────────────────

struct ParsedSpan {
    trace_id_hi: i64,
    trace_id_lo: i64,
    span_id: i64,
    parent_span_id: Option<i64>,
    name: String,
    service: String,
    start_time_ns: i64,
    duration_ns: i64,
    status_code: i64,
}

fn parse_json_span(v: &Value) -> Option<ParsedSpan> {
    let trace_hex = v.get("trace_id")?.as_str()?;
    let (hi, lo) = parse_trace_id_hex_parts(trace_hex)?;

    let span_hex = v.get("span_id")?.as_str()?;
    let span_id = u64::from_str_radix(span_hex, 16).ok()? as i64;

    let parent_span_id = v.get("parent_span_id").and_then(|p| {
        p.as_str()
            .and_then(|h| u64::from_str_radix(h, 16).ok().map(|v| v as i64))
    });

    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let service = v
        .get("service")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    let start_time_ns = v.get("start_time_ns").and_then(|n| n.as_i64()).unwrap_or(0);
    let duration_ns = v.get("duration_ns").and_then(|n| n.as_i64()).unwrap_or(0);
    let status_code = v.get("status_code").and_then(|n| n.as_i64()).unwrap_or(0);

    Some(ParsedSpan {
        trace_id_hi: hi as i64,
        trace_id_lo: lo as i64,
        span_id,
        parent_span_id,
        name,
        service,
        start_time_ns,
        duration_ns,
        status_code,
    })
}

fn parse_trace_id_hex(hex: &str) -> Option<u128> {
    let (hi, lo) = parse_trace_id_hex_parts(hex)?;
    Some(((hi as u128) << 64) | lo as u128)
}

fn parse_trace_id_hex_parts(hex: &str) -> Option<(u64, u64)> {
    let hex = hex.trim_start_matches("0x");
    if hex.len() != 32 {
        return None;
    }
    let hi = u64::from_str_radix(&hex[..16], 16).ok()?;
    let lo = u64::from_str_radix(&hex[16..], 16).ok()?;
    Some((hi, lo))
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}
