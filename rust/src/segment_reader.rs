//! Multi-segment span reader — queries across a set of sealed .nxb files
//! plus an optional live .nxsw WAL.
//!
//! # Memory model
//!
//! Each sealed `.nxb` is mapped with [`memmap2::Mmap`] (no full-file `Vec` at open).
//! Per segment, an in-memory index `trace_id → [absolute offsets]` is built by scanning
//! the tail index once — **O(records)** heap for the index, not O(file size). WAL bytes
//! are still loaded into a `Vec` for the live `.nxsw` path.
//!
//! # Usage
//!
//!   let reader = SegmentReader::open("traces/")? ;
//!   // find all spans for a trace
//!   let spans = reader.find_by_trace(trace_id)?;
//!   // find one span by (trace_id, span_id)
//!   let span  = reader.find_span(trace_id, span_id)?;

use crate::decoder::{decode, decode_record_at, DecodedValue};
use crate::error::{NxsError, Result};
use crate::wal::SpanWal;
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// A decoded span record returned by queries.
#[derive(Debug, Clone)]
pub struct Span {
    pub trace_id: u128,
    pub span_id: u64,
    pub parent_span_id: Option<u64>,
    pub name: String,
    pub service: String,
    pub start_time_ns: i64,
    pub duration_ns: i64,
    pub status_code: i64,
    pub payload: Option<Vec<u8>>,
}

/// Queries sealed segments + live WAL for span data.
pub struct SegmentReader {
    segments: Vec<SealedSegment>,
    wal: Option<WalReader>,
}

struct SealedSegment {
    path: PathBuf,
    _file: File,
    mmap: Mmap,
    /// (trace_id → [span absolute offsets]) built from the tail-index.
    index: HashMap<u128, Vec<u64>>,
    keys: Vec<String>,
    sigils: Vec<u8>,
}

/// Read-only view of a live WAL — data loaded once at open time.
struct WalReader {
    wal: SpanWal,
    /// Raw WAL file bytes cached so queries don't re-read the file each time.
    data: Vec<u8>,
}

impl SegmentReader {
    /// Open all `.nxb` files and at most one `.nxsw` file found under `dir`.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let mut segments = Vec::new();
        let mut wal_path: Option<PathBuf> = None;

        let entries = fs::read_dir(dir).map_err(|e| NxsError::IoError(e.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|e| NxsError::IoError(e.to_string()))?;
            let path = entry.path();
            match path.extension().and_then(|e| e.to_str()) {
                Some("nxb") => {
                    let seg = SealedSegment::load(path)?;
                    segments.push(seg);
                }
                Some("nxsw") => {
                    // Last .nxsw wins (there should only be one live WAL)
                    wal_path = Some(path);
                }
                _ => {}
            }
        }

        // Sort segments by filename for deterministic ordering (oldest first)
        segments.sort_by(|a, b| a.path.cmp(&b.path));

        let wal = if let Some(p) = wal_path {
            let mut w = SpanWal::open(&p)?;
            w.recover()?;
            let data = fs::read(w.path()).map_err(|e| NxsError::IoError(e.to_string()))?;
            Some(WalReader { wal: w, data })
        } else {
            None
        };

        Ok(SegmentReader { segments, wal })
    }

    /// Return all spans belonging to a trace, sorted by start_time_ns.
    pub fn find_by_trace(&self, trace_id: u128) -> Result<Vec<Span>> {
        let mut spans = Vec::new();

        for seg in &self.segments {
            if let Some(offsets) = seg.index.get(&trace_id) {
                for &abs_off in offsets {
                    if let Ok(span) = seg.decode_span_at(abs_off) {
                        spans.push(span);
                    }
                }
            }
        }

        if let Some(ref wr) = self.wal {
            for entry in &wr.wal.index {
                if entry.trace_id == trace_id {
                    if let Some(span) = decode_span_from_raw(&wr.data, entry.offset as usize) {
                        spans.push(span);
                    }
                }
            }
        }

        spans.sort_by_key(|s| s.start_time_ns);
        Ok(spans)
    }

    /// Find a specific span by (trace_id, span_id).
    pub fn find_span(&self, trace_id: u128, span_id: u64) -> Result<Option<Span>> {
        let spans = self.find_by_trace(trace_id)?;
        Ok(spans.into_iter().find(|s| s.span_id == span_id))
    }

    /// Return spans in a time window across all segments.
    pub fn find_by_time(&self, start_ns: i64, end_ns: i64) -> Result<Vec<Span>> {
        let mut spans = Vec::new();

        for seg in &self.segments {
            for offsets in seg.index.values() {
                for &abs_off in offsets {
                    if let Ok(span) = seg.decode_span_at(abs_off) {
                        if span.start_time_ns >= start_ns && span.start_time_ns <= end_ns {
                            spans.push(span);
                        }
                    }
                }
            }
        }

        if let Some(ref wr) = self.wal {
            for entry in &wr.wal.index {
                if let Some(span) = decode_span_from_raw(&wr.data, entry.offset as usize) {
                    if span.start_time_ns >= start_ns && span.start_time_ns <= end_ns {
                        spans.push(span);
                    }
                }
            }
        }

        spans.sort_by_key(|s| s.start_time_ns);
        Ok(spans)
    }

    /// Summary: how many segments + records are loaded.
    pub fn stats(&self) -> ReaderStats {
        let segment_count = self.segments.len();
        let sealed_records: u64 = self
            .segments
            .iter()
            .map(|s| s.index.values().map(|v| v.len() as u64).sum::<u64>())
            .sum();
        let wal_records = self.wal.as_ref().map(|w| w.wal.record_count()).unwrap_or(0);
        ReaderStats {
            segment_count,
            sealed_records,
            wal_records,
        }
    }
}

#[derive(Debug)]
pub struct ReaderStats {
    pub segment_count: usize,
    pub sealed_records: u64,
    pub wal_records: u64,
}

impl SealedSegment {
    fn load(path: PathBuf) -> Result<Self> {
        let file = File::open(&path).map_err(|e| NxsError::IoError(e.to_string()))?;
        let mmap = unsafe {
            Mmap::map(&file)
                .map_err(|e| NxsError::IoError(format!("mmap {}: {e}", path.display())))?
        };
        let data = &mmap[..];
        let decoded = decode(data)?;

        // Build trace_id → [offsets] index from the tail-index.
        // Each tail-index entry: KeyID(u16) + AbsoluteOffset(u64) = 10 bytes
        let mut index: HashMap<u128, Vec<u64>> = HashMap::new();
        let tail_start = decoded.tail_start;
        let tail_end = data.len().saturating_sub(12); // strip FooterTailPtr + MagicFooter

        let mut pos = tail_start;
        while pos + 10 <= tail_end {
            let _key_id = u16::from_le_bytes(
                data[pos..pos + 2]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            let abs_off = u64::from_le_bytes(
                data[pos + 2..pos + 10]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            pos += 10;

            // Extract trace_id from the span at abs_off to build the lookup map
            if abs_off as usize + 8 <= data.len() {
                let fields =
                    decode_record_at(data, abs_off as usize, &decoded.keys, &decoded.key_sigils)
                        .unwrap_or_default();
                if let Some(trace_id) = extract_trace_id(&fields) {
                    index.entry(trace_id).or_default().push(abs_off);
                }
            }
        }

        Ok(SealedSegment {
            path,
            _file: file,
            mmap,
            index,
            keys: decoded.keys,
            sigils: decoded.key_sigils,
        })
    }

    fn data(&self) -> &[u8] {
        &self.mmap
    }

    fn decode_span_at(&self, abs_off: u64) -> Result<Span> {
        let fields = decode_record_at(self.data(), abs_off as usize, &self.keys, &self.sigils)?;
        fields_to_span(&fields).ok_or(NxsError::OutOfBounds)
    }
}

// ── Field helpers ─────────────────────────────────────────────────────────────

fn extract_trace_id(fields: &[(String, DecodedValue)]) -> Option<u128> {
    let hi = get_i64(fields, "trace_id_hi")? as u64;
    let lo = get_i64(fields, "trace_id_lo")? as u64;
    Some(((hi as u128) << 64) | lo as u128)
}

fn fields_to_span(fields: &[(String, DecodedValue)]) -> Option<Span> {
    let trace_id = extract_trace_id(fields)?;
    let span_id = get_i64(fields, "span_id")? as u64;
    // A null parent is stored as either DecodedValue::Null or as Int(0).
    // Span IDs of 0 are invalid per the OpenTelemetry spec, so 0 == absent.
    let parent_span_id = if is_null(fields, "parent_span_id") {
        None
    } else {
        get_i64(fields, "parent_span_id")
            .map(|v| v as u64)
            .filter(|&v| v != 0)
    };

    Some(Span {
        trace_id,
        span_id,
        parent_span_id,
        name: get_str(fields, "name"),
        service: get_str(fields, "service"),
        start_time_ns: get_i64(fields, "start_time_ns").unwrap_or(0),
        duration_ns: get_i64(fields, "duration_ns").unwrap_or(0),
        status_code: get_i64(fields, "status_code").unwrap_or(0),
        payload: get_bytes(fields, "payload"),
    })
}

const SPAN_KEYS: &[&str] = &[
    "trace_id_hi",
    "trace_id_lo",
    "span_id",
    "parent_span_id",
    "name",
    "service",
    "start_time_ns",
    "duration_ns",
    "status_code",
    "payload",
];
const SPAN_SIGILS: &[u8] = b"====\"\"@==<";

fn decode_span_from_raw(data: &[u8], offset: usize) -> Option<Span> {
    if offset + 4 > data.len() {
        return None;
    }
    let magic = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
    if magic != crate::wal::MAGIC_OBJ {
        return None;
    }

    let keys: Vec<String> = SPAN_KEYS.iter().map(|s| s.to_string()).collect();
    let fields = decode_record_at(data, offset, &keys, SPAN_SIGILS).ok()?;
    fields_to_span(&fields)
}

fn is_null(fields: &[(String, DecodedValue)], name: &str) -> bool {
    fields
        .iter()
        .any(|(k, v)| k == name && matches!(v, DecodedValue::Null))
}

fn get_i64(fields: &[(String, DecodedValue)], name: &str) -> Option<i64> {
    fields.iter().find_map(|(k, v)| {
        if k == name {
            match v {
                DecodedValue::Int(i) => Some(*i),
                DecodedValue::Time(i) => Some(*i),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn get_str(fields: &[(String, DecodedValue)], name: &str) -> String {
    fields
        .iter()
        .find_map(|(k, v)| {
            if k == name {
                if let DecodedValue::Str(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn get_bytes(fields: &[(String, DecodedValue)], name: &str) -> Option<Vec<u8>> {
    fields.iter().find_map(|(k, v)| {
        if k == name {
            if let DecodedValue::Binary(b) = v {
                Some(b.clone())
            } else {
                None
            }
        } else {
            None
        }
    })
}
