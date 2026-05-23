//! Span WAL — streaming append layer for NXS span/trace data.
//!
//! # Architecture
//!
//! A `.nxsw` (WAL) file stores spans as they arrive without a tail-index,
//! because rewriting the index on every append would be O(N). Instead:
//!
//!   open()     → write NXSW header + schema once
//!   append()   → write one NXS object; record (trace_id, span_id, offset) in RAM
//!   seal()     → replay the in-memory index, emit a full .nxb with tail-index
//!   recover()  → linear scan to rebuild the in-memory index after a crash
//!
//! The WAL file is valid enough to decode span-by-span in order even without the
//! index: NYXO magic allows readers to skip forward record-by-record.
//!
//! # WAL file layout
//!
//!   [NXSW magic 4B][version 2B][flags 2B]   -- 8-byte header
//!   [schema_len u32][schema bytes (padded)]  -- same schema encoding as .nxb
//!   [NYXO record 0 bytes]
//!   [NYXO record 1 bytes]
//!   ...

use crate::error::{NxsError, Result};
use crate::writer::{NxsWriter, Schema};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const MAGIC_WAL: u32 = 0x5753584E; // NXSW
pub const MAGIC_OBJ: u32 = 0x4E59584F; // NYXO
const WAL_VERSION: u16 = 0x0100;
const WAL_FLAG_SCHEMA_EMBEDDED: u16 = 0x0001;
const MAX_RECORD_BYTES: u64 = 10 * 1024 * 1024; // 10 MB OOM guard

/// Span identity extracted from each WAL record for the in-memory index.
#[derive(Debug, Clone)]
pub struct WalEntry {
    pub trace_id: u128,
    pub span_id: u64,
    /// Absolute byte offset of this record's NYXO magic in the WAL file.
    pub offset: u64,
}

/// A span field to write — keeps the API decoupled from slot indices.
#[derive(Debug)]
pub struct SpanFields<'a> {
    pub trace_id_hi: i64,
    pub trace_id_lo: i64,
    pub span_id: i64,
    pub parent_span_id: Option<i64>,
    pub name: &'a str,
    pub service: &'a str,
    pub start_time_ns: i64,
    pub duration_ns: i64,
    pub status_code: i64,
    /// Arbitrary JSON payload (inputs/outputs for LLM spans, tags, etc.) stored as bytes.
    pub payload: Option<&'a [u8]>,
}

/// Canonical schema for a span record.
/// Slot indices are stable — do not reorder.
pub struct SpanSchema {
    pub schema: Schema,
}

impl SpanSchema {
    pub fn new() -> Self {
        SpanSchema {
            schema: Schema::new(&[
                "trace_id_hi",    // 0  =i64
                "trace_id_lo",    // 1  =i64
                "span_id",        // 2  =i64
                "parent_span_id", // 3  =i64 / null
                "name",           // 4  "str
                "service",        // 5  "str
                "start_time_ns",  // 6  @time
                "duration_ns",    // 7  =i64
                "status_code",    // 8  =i64
                "payload",        // 9  <binary (opaque JSON)
            ]),
        }
    }
}

/// Slot constants — compile-time checked indices into `SpanSchema`.
pub mod slot {
    use crate::writer::Slot;
    pub const TRACE_ID_HI: Slot = Slot(0);
    pub const TRACE_ID_LO: Slot = Slot(1);
    pub const SPAN_ID: Slot = Slot(2);
    pub const PARENT_SPAN_ID: Slot = Slot(3);
    pub const NAME: Slot = Slot(4);
    pub const SERVICE: Slot = Slot(5);
    pub const START_TIME_NS: Slot = Slot(6);
    pub const DURATION_NS: Slot = Slot(7);
    pub const STATUS_CODE: Slot = Slot(8);
    pub const PAYLOAD: Slot = Slot(9);
}

/// Streaming WAL writer. Not `Send` — use one per thread or wrap in a Mutex.
pub struct SpanWal {
    path: PathBuf,
    file: BufWriter<File>,
    /// In-memory index rebuilt on crash recovery via `recover()`.
    pub index: Vec<WalEntry>,
    pub record_count: u64,
    schema: SpanSchema,
    /// Byte offset of the first record (right after the WAL header).
    data_start: u64,
    /// Tracked in-process so append() never calls metadata() for a syscall.
    current_offset: u64,
}

impl SpanWal {
    /// Open (or create) a WAL file. If the file already exists and is non-empty,
    /// call `recover()` to rebuild the in-memory index before appending.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let schema = SpanSchema::new();
        let schema_bytes = build_wal_schema_bytes(&schema.schema);

        let file_exists = path.exists();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        let mut writer = BufWriter::new(file);

        // 8-byte fixed header + 4-byte schema_len field + schema bytes
        let data_start = 8 + 4 + schema_bytes.len() as u64;
        if !file_exists {
            // Write WAL header
            writer
                .write_all(&MAGIC_WAL.to_le_bytes())
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            writer
                .write_all(&WAL_VERSION.to_le_bytes())
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            writer
                .write_all(&WAL_FLAG_SCHEMA_EMBEDDED.to_le_bytes())
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            writer
                .write_all(&(schema_bytes.len() as u32).to_le_bytes())
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            writer
                .write_all(&schema_bytes)
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            writer
                .flush()
                .map_err(|e| NxsError::IoError(e.to_string()))?;
        }

        // For an existing file we don't know the true end yet — recover() will
        // set current_offset after scanning.  For a new file it equals data_start.
        let initial_offset = if file_exists {
            0 // will be corrected by recover()
        } else {
            data_start
        };

        Ok(SpanWal {
            path,
            file: writer,
            index: Vec::new(),
            record_count: 0,
            schema,
            data_start,
            current_offset: initial_offset,
        })
    }

    /// Append a span to the WAL. Returns the absolute byte offset of the record.
    pub fn append(&mut self, span: &SpanFields) -> Result<u64> {
        let file_offset = self.current_offset;

        // Encode one NXS object
        let mut w = NxsWriter::new(&self.schema.schema);
        w.begin_object();
        w.write_i64(slot::TRACE_ID_HI, span.trace_id_hi);
        w.write_i64(slot::TRACE_ID_LO, span.trace_id_lo);
        w.write_i64(slot::SPAN_ID, span.span_id);
        match span.parent_span_id {
            Some(p) => w.write_i64(slot::PARENT_SPAN_ID, p),
            None => w.write_null(slot::PARENT_SPAN_ID),
        }
        w.write_str(slot::NAME, span.name);
        w.write_str(slot::SERVICE, span.service);
        w.write_time(slot::START_TIME_NS, span.start_time_ns);
        w.write_i64(slot::DURATION_NS, span.duration_ns);
        w.write_i64(slot::STATUS_CODE, span.status_code);
        if let Some(payload) = span.payload {
            w.write_bytes(slot::PAYLOAD, payload);
        }
        w.end_object();

        // finish() emits a full .nxb file — we only want the data sector bytes
        // (skip preamble + schema, stop before tail-index).
        let nxb = w.finish();
        let data_sector = extract_data_sector(&nxb)?;

        self.file
            .write_all(data_sector)
            .map_err(|e| NxsError::IoError(e.to_string()))?;

        self.current_offset += data_sector.len() as u64;

        // Cast via u64 to preserve bit pattern and avoid sign-extension into the high half.
        let trace_id =
            ((span.trace_id_hi as u64 as u128) << 64) | (span.trace_id_lo as u64 as u128);
        self.index.push(WalEntry {
            trace_id,
            span_id: span.span_id as u64,
            offset: file_offset,
        });
        self.record_count += 1;

        Ok(file_offset)
    }

    /// Flush write buffer to OS.
    pub fn flush(&mut self) -> Result<()> {
        self.file
            .flush()
            .map_err(|e| NxsError::IoError(e.to_string()))
    }

    /// Rebuild `self.index` by scanning the WAL file linearly.
    /// Call this after opening an existing WAL (crash recovery path).
    pub fn recover(&mut self) -> Result<()> {
        let mut file = File::open(&self.path).map_err(|e| NxsError::IoError(e.to_string()))?;
        let file_len = file
            .metadata()
            .map_err(|e| NxsError::IoError(e.to_string()))?
            .len();

        // Validate WAL magic
        let mut header = [0u8; 8];
        file.read_exact(&mut header)
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        let magic = u32::from_le_bytes(
            header[0..4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        );
        if magic != MAGIC_WAL {
            return Err(NxsError::BadMagic);
        }

        // Skip schema
        let mut schema_len_buf = [0u8; 4];
        file.read_exact(&mut schema_len_buf)
            .map_err(|e| NxsError::IoError(e.to_string()))?;
        let schema_len = u32::from_le_bytes(schema_len_buf) as u64;
        file.seek(SeekFrom::Current(schema_len as i64))
            .map_err(|e| NxsError::IoError(e.to_string()))?;

        // Walk NYXO records
        let mut index = Vec::new();
        let mut record_count = 0u64;
        loop {
            let pos = file
                .stream_position()
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            if pos + 8 > file_len {
                break;
            }

            let mut rec_header = [0u8; 8];
            file.read_exact(&mut rec_header)
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            let obj_magic = u32::from_le_bytes(
                rec_header[0..4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            if obj_magic != MAGIC_OBJ {
                break; // truncated or corrupt tail — stop here
            }
            let obj_len = u32::from_le_bytes(
                rec_header[4..8]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as u64;

            // Read the full object to extract trace_id / span_id from bitmask + offsets
            if !(8..=MAX_RECORD_BYTES).contains(&obj_len) || pos + obj_len > file_len {
                break;
            }
            let mut obj_buf = vec![0u8; obj_len as usize];
            obj_buf[0..8].copy_from_slice(&rec_header);
            file.read_exact(&mut obj_buf[8..])
                .map_err(|e| NxsError::IoError(e.to_string()))?;

            if let Some((trace_id, span_id)) = extract_trace_span_id(&obj_buf) {
                index.push(WalEntry {
                    trace_id,
                    span_id,
                    offset: pos,
                });
            }
            record_count += 1;
        }

        self.index = index;
        self.record_count = record_count;
        // Sync offset so subsequent appends don't call metadata().
        self.current_offset = file_len;
        Ok(())
    }

    /// Seal the WAL: write a complete `.nxb` segment file and return its path.
    /// The WAL file is left in place — rotate/delete it externally once the
    /// segment is durably written.
    pub fn seal(&mut self, out_path: impl AsRef<Path>) -> Result<SealReport> {
        self.flush()?;

        let mut file = File::open(&self.path).map_err(|e| NxsError::IoError(e.to_string()))?;

        // Skip WAL header + schema
        file.seek(SeekFrom::Start(self.data_start))
            .map_err(|e| NxsError::IoError(e.to_string()))?;

        // Read all record bytes into NxsWriter
        let schema_for_seal = SpanSchema::new();
        let mut w = NxsWriter::with_capacity(&schema_for_seal.schema, 1024 * 1024);

        // Replay by re-reading each span from the WAL using the in-memory index
        for entry in &self.index {
            file.seek(SeekFrom::Start(entry.offset))
                .map_err(|e| NxsError::IoError(e.to_string()))?;

            let mut hdr = [0u8; 8];
            file.read_exact(&mut hdr)
                .map_err(|e| NxsError::IoError(e.to_string()))?;
            let obj_len = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
            let mut obj_buf = vec![0u8; obj_len];
            obj_buf[0..8].copy_from_slice(&hdr);
            file.read_exact(&mut obj_buf[8..])
                .map_err(|e| NxsError::IoError(e.to_string()))?;

            // Decode the object back to SpanFields and re-encode via NxsWriter
            // so the sealed file has a proper preamble + tail-index.
            if let Some(span) = decode_span_object(&obj_buf) {
                w.begin_object();
                w.write_i64(slot::TRACE_ID_HI, span.trace_id_hi);
                w.write_i64(slot::TRACE_ID_LO, span.trace_id_lo);
                w.write_i64(slot::SPAN_ID, span.span_id);
                match span.parent_span_id {
                    Some(p) => w.write_i64(slot::PARENT_SPAN_ID, p),
                    None => w.write_null(slot::PARENT_SPAN_ID),
                }
                w.write_str(slot::NAME, &span.name_owned);
                w.write_str(slot::SERVICE, &span.service_owned);
                w.write_time(slot::START_TIME_NS, span.start_time_ns);
                w.write_i64(slot::DURATION_NS, span.duration_ns);
                w.write_i64(slot::STATUS_CODE, span.status_code);
                if let Some(ref payload) = span.payload_owned {
                    w.write_bytes(slot::PAYLOAD, payload);
                }
                w.end_object();
            }
        }

        let nxb = w.finish();
        let bytes_written = nxb.len() as u64;
        let records = self.record_count;

        std::fs::write(out_path.as_ref(), &nxb).map_err(|e| NxsError::IoError(e.to_string()))?;

        Ok(SealReport {
            records,
            bytes_written,
            segment_path: out_path.as_ref().to_path_buf(),
        })
    }

    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug)]
pub struct SealReport {
    pub records: u64,
    pub bytes_written: u64,
    pub segment_path: PathBuf,
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Extract just the data sector bytes from a complete .nxb buffer.
/// Skips the 32-byte preamble and embedded schema, strips the tail-index.
fn extract_data_sector(nxb: &[u8]) -> Result<&[u8]> {
    if nxb.len() < 32 {
        return Err(NxsError::OutOfBounds);
    }
    // Read tail_ptr from preamble bytes 16..24
    let mut tail_ptr =
        u64::from_le_bytes(nxb[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?) as usize;
    if tail_ptr == 0 {
        if nxb.len() < 44 {
            return Err(NxsError::OutOfBounds);
        }
        tail_ptr = u64::from_le_bytes(
            nxb[nxb.len() - 12..nxb.len() - 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
    }
    if tail_ptr > nxb.len() {
        return Err(NxsError::OutOfBounds);
    }

    // Schema header starts at byte 32. Find its end (it is 8-byte aligned).
    // Schema: u16 key_count + key_count sigil bytes + NUL-terminated strings, padded to 8.
    let mut pos = 32usize;
    if pos + 2 > nxb.len() {
        return Err(NxsError::OutOfBounds);
    }
    let key_count = u16::from_le_bytes(nxb[pos..pos + 2].try_into().unwrap()) as usize;
    pos += 2 + key_count; // skip sigil bytes
    for _ in 0..key_count {
        while pos < nxb.len() && nxb[pos] != 0 {
            pos += 1;
        }
        pos += 1; // skip NUL
    }
    while pos % 8 != 0 {
        pos += 1;
    }

    // `pos` is now the data sector start; `tail_ptr` is its end.
    if pos > tail_ptr {
        return Err(NxsError::OutOfBounds);
    }
    Ok(&nxb[pos..tail_ptr])
}

/// Build the schema bytes portion written in the WAL header (same encoding as .nxb).
fn build_wal_schema_bytes(schema: &Schema) -> Vec<u8> {
    // We reproduce build_schema() from writer.rs since it is private there.
    let keys = schema_keys(schema);
    let n = keys.len();
    let mut b = Vec::new();
    b.extend_from_slice(&(n as u16).to_le_bytes());
    for _ in 0..n {
        b.push(b'"'); // default sigil — updated on first real write
    }
    for key in &keys {
        b.extend_from_slice(key.as_bytes());
        b.push(0x00);
    }
    while b.len() % 8 != 0 {
        b.push(0x00);
    }
    b
}

/// Extract the canonical key list from a Schema by re-constructing via SpanSchema.
fn schema_keys(schema: &Schema) -> Vec<&'static str> {
    let _ = schema; // Schema doesn't expose keys publicly; we hard-code for SpanSchema.
    vec![
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
    ]
}

/// Decode the (trace_id_hi, trace_id_lo, span_id) from a raw NYXO object buffer
/// for use in the recovery index.
fn extract_trace_span_id(obj: &[u8]) -> Option<(u128, u64)> {
    // Minimal parse: skip magic(4) + length(4) + LEB128 bitmask + offset table,
    // then read slots 0,1,2 (all i64, 8 bytes each, aligned).
    let mut pos = 8usize;

    // Read LEB128 bitmask
    let mut bitmask_bytes = 0usize;
    loop {
        if pos >= obj.len() {
            return None;
        }
        let b = obj[pos];
        pos += 1;
        bitmask_bytes += 1;
        if b & 0x80 == 0 {
            break;
        }
        if bitmask_bytes > 16 {
            return None;
        }
    }

    // Count present bits (slots 0..2 = trace_id_hi, trace_id_lo, span_id)
    // We only care that they are present; offsets are in the offset table.
    // Present count for all slots:
    let bitmask_start = 8;
    let mut present = Vec::new();
    let mut bp = bitmask_start;
    loop {
        if bp >= obj.len() {
            break;
        }
        let byte = obj[bp];
        bp += 1;
        for bit in 0..7 {
            present.push((byte >> bit) & 1 == 1);
        }
        if byte & 0x80 == 0 {
            break;
        }
    }

    let present_count = present.iter().filter(|&&b| b).count();
    // Offset table follows bitmask
    let ot_start = bp;
    if ot_start + present_count * 2 > obj.len() {
        return None;
    }

    // Map slot → offset table index
    let mut slot_to_ot: Vec<Option<usize>> = vec![None; present.len()];
    let mut ot_idx = 0;
    for (slot, &p) in present.iter().enumerate() {
        if p {
            slot_to_ot[slot] = Some(ot_idx);
            ot_idx += 1;
        }
    }

    let read_i64_at_slot = |slot: usize| -> Option<i64> {
        let ot_i = slot_to_ot.get(slot)?.as_ref()?;
        let ot_off = ot_start + ot_i * 2;
        if ot_off + 2 > obj.len() {
            return None;
        }
        let rel = u16::from_le_bytes(obj[ot_off..ot_off + 2].try_into().ok()?) as usize;
        let val_off = rel; // relative to object start
        if val_off + 8 > obj.len() {
            return None;
        }
        Some(i64::from_le_bytes(
            obj[val_off..val_off + 8].try_into().ok()?,
        ))
    };

    let hi = read_i64_at_slot(0)? as u64;
    let lo = read_i64_at_slot(1)? as u64;
    let span_id = read_i64_at_slot(2)? as u64;
    // Cast via u64 first to preserve bit pattern; direct i64→u128 sign-extends.
    let trace_id = ((hi as u128) << 64) | (lo as u128);
    Some((trace_id, span_id))
}

/// Decoded span fields (owned strings/bytes) for use during seal replay.
struct DecodedSpan {
    trace_id_hi: i64,
    trace_id_lo: i64,
    span_id: i64,
    parent_span_id: Option<i64>,
    name_owned: String,
    service_owned: String,
    start_time_ns: i64,
    duration_ns: i64,
    status_code: i64,
    payload_owned: Option<Vec<u8>>,
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

fn decode_span_object(obj: &[u8]) -> Option<DecodedSpan> {
    use crate::decoder::{decode_record_at, DecodedValue};

    let keys: Vec<String> = SPAN_KEYS.iter().map(|s| s.to_string()).collect();
    let sigils = SPAN_SIGILS;

    let fields = decode_record_at(obj, 0, &keys, sigils).ok()?;
    let get_i64 = |name: &str| -> Option<i64> {
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
    };
    let get_str = |name: &str| -> String {
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
    };
    let get_bytes = |name: &str| -> Option<Vec<u8>> {
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
    };
    let get_null = |name: &str| -> bool {
        fields
            .iter()
            .any(|(k, v)| k == name && *v == DecodedValue::Null)
    };

    Some(DecodedSpan {
        trace_id_hi: get_i64("trace_id_hi")?,
        trace_id_lo: get_i64("trace_id_lo")?,
        span_id: get_i64("span_id")?,
        parent_span_id: if get_null("parent_span_id") {
            None
        } else {
            get_i64("parent_span_id").filter(|&v| v != 0)
        },
        name_owned: get_str("name"),
        service_owned: get_str("service"),
        start_time_ns: get_i64("start_time_ns").unwrap_or(0),
        duration_ns: get_i64("duration_ns").unwrap_or(0),
        status_code: get_i64("status_code").unwrap_or(0),
        payload_owned: get_bytes("payload"),
    })
}
