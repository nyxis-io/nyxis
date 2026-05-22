use std::io::Write;

/// NxsWriter — optimized direct-to-buffer `.nxb` emitter.
///
/// Precompile a [`Schema`] once, then write records with slot-indexed typed
/// methods; call [`NxsWriter::finish`] to obtain the complete `.nxb` bytes.
///
/// # Example
///
/// ```
/// use nxs::writer::{Schema, NxsWriter, Slot};
/// let schema = Schema::new(&["id", "score"]);
/// let mut w = NxsWriter::new(&schema);
/// w.begin_object();
/// w.write_i64(Slot(0), 42);
/// w.write_f64(Slot(1), 9.5);
/// w.end_object();
/// let bytes = w.finish();
/// ```
use crate::consts::{
    FLAG_SCHEMA_EMBEDDED, MAGIC_FILE, MAGIC_FOOTER, MAGIC_LIST, MAGIC_OBJ, VERSION,
};

/// A field slot — an index into the [`Schema`]'s key list; used by all typed write methods.
#[derive(Copy, Clone, Debug)]
pub struct Slot(pub u16);

/// Precompiled schema shared across all objects in a file; build once, reuse for every record.
pub struct Schema {
    keys: Vec<String>,
    /// Precomputed LEB128 bitmask size (one per possible present-bit count)
    bitmask_bytes: usize,
    /// Per-slot sigil (default = SIGIL_STR; reserved for future per-schema type hints).
    /// Actual slot sigils are tracked in `NxsWriter::slot_sigils` at write time.
    #[allow(dead_code)]
    sigils: Vec<u8>,
}

impl Schema {
    pub fn new(keys: &[&str]) -> Self {
        let keys: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
        let n = keys.len();
        // LEB128 bitmask size for `keys.len()` bits
        let bitmask_bytes = (n + 6) / 7;
        let bitmask_bytes = bitmask_bytes.max(1);
        Schema {
            keys,
            bitmask_bytes,
            sigils: vec![0u8; n],
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }
}

/// Holds back-patch info for the currently-open object.
struct Frame {
    /// Byte offset in `buf` of this object's Magic field.
    start: usize,
    /// Bitmask bytes (one per 7 slots). Mutated as fields are written.
    bitmask: Vec<u8>,
    /// Offset table entries (u16 each), pushed in write order.
    /// Each entry is the offset-from-object-start of that value's bytes.
    offset_table: Vec<u16>,
    /// Highest slot written so far (for sort-free insertion detection).
    last_slot: i32,
    /// Whether we saw fields out of slot order.
    needs_sort: bool,
    /// (slot, offset_in_buf) for each value, for sort-if-needed path.
    slot_offsets: Vec<(u16, u32)>,
}

pub struct NxsWriter<'a> {
    schema: &'a Schema,
    buf: Vec<u8>,
    frames: Vec<Frame>,
    /// Byte offset (in `buf`, relative to data sector start) of each top-level object.
    record_offsets: Vec<u32>,
    /// Actual sigil used per slot (set on first write to a slot)
    slot_sigils: Vec<u8>,
}

impl<'a> NxsWriter<'a> {
    /// Create a new writer for the given schema.
    pub fn new(schema: &'a Schema) -> Self {
        let n = schema.keys.len();
        NxsWriter {
            schema,
            buf: Vec::with_capacity(4096),
            frames: Vec::with_capacity(4),
            record_offsets: Vec::new(),
            slot_sigils: vec![0u8; n], // 0 = "not yet set"
        }
    }

    /// Pre-allocate a capacity hint (bytes).
    pub fn with_capacity(schema: &'a Schema, cap: usize) -> Self {
        let n = schema.keys.len();
        NxsWriter {
            schema,
            buf: Vec::with_capacity(cap),
            frames: Vec::with_capacity(4),
            record_offsets: Vec::with_capacity(1024),
            slot_sigils: vec![0u8; n], // 0 = "not yet set"
        }
    }

    /// Open a new object frame; must be paired with [`NxsWriter::end_object`].
    #[inline]
    pub fn begin_object(&mut self) {
        // Record top-level object start offsets (for the tail-index)
        if self.frames.is_empty() {
            self.record_offsets.push(self.buf.len() as u32);
        }
        let start = self.buf.len();
        // Reserve: Magic(4) + Length(4) + Bitmask + max u16 offsets for all slots
        // Then we'll pad to alignment at end_object and back-patch.
        let bitmask = vec![0u8; self.schema.bitmask_bytes];
        // LEB128 continuation bits:
        let mut bitmask = bitmask;
        for i in 0..bitmask.len().saturating_sub(1) {
            bitmask[i] |= 0x80;
        }

        self.frames.push(Frame {
            start,
            bitmask,
            offset_table: Vec::with_capacity(self.schema.keys.len()),
            last_slot: -1,
            needs_sort: false,
            slot_offsets: Vec::with_capacity(self.schema.keys.len()),
        });

        // Write placeholder magic + length (will back-patch length at end)
        self.buf.extend_from_slice(&MAGIC_OBJ.to_le_bytes());
        self.buf.extend_from_slice(&0u32.to_le_bytes()); // length placeholder
                                                         // Reserve bitmask space (will back-patch)
        self.buf
            .extend_from_slice(&self.frames.last().unwrap().bitmask.clone());
        // Reserve offset table space: u16 per possible slot (upper bound)
        let offset_table_reserve = self.schema.keys.len() * 2;
        self.buf.resize(self.buf.len() + offset_table_reserve, 0);
        // Align data_start to 8
        while (self.buf.len() - start) % 8 != 0 {
            self.buf.push(0);
        }
    }

    /// Close the current object frame and back-patch its length and offset table.
    #[inline]
    pub fn end_object(&mut self) {
        let frame = self.frames.pop().expect("end_object without begin_object");
        let total_len = self.buf.len() - frame.start;

        // Back-patch Length field
        let len_offset = frame.start + 4;
        self.buf[len_offset..len_offset + 4].copy_from_slice(&(total_len as u32).to_le_bytes());

        // Back-patch Bitmask
        let bitmask_offset = frame.start + 8;
        self.buf[bitmask_offset..bitmask_offset + frame.bitmask.len()]
            .copy_from_slice(&frame.bitmask);

        // Back-patch Offset Table
        // The reserved space starts after bitmask; each offset is u16
        let offset_table_start = bitmask_offset + frame.bitmask.len();
        let present_count = frame.offset_table.len();

        if !frame.needs_sort {
            // Fast path: fields were written in slot order → offset_table is already correct
            for (i, &off) in frame.offset_table.iter().enumerate() {
                let p = offset_table_start + i * 2;
                self.buf[p..p + 2].copy_from_slice(&off.to_le_bytes());
            }
        } else {
            // Slow path: sort by slot, then write offsets in slot order
            let mut pairs = frame.slot_offsets.clone();
            pairs.sort_unstable_by_key(|(s, _)| *s);
            for (i, (_, buf_off)) in pairs.iter().enumerate() {
                let p = offset_table_start + i * 2;
                let rel = (*buf_off as usize - frame.start) as u16;
                self.buf[p..p + 2].copy_from_slice(&rel.to_le_bytes());
            }
        }

        // The reserved offset table may have excess slots (if fewer fields present than schema keys).
        // Zero the unused portion to keep the output deterministic.
        let used_bytes = present_count * 2;
        let reserved_bytes = self.schema.keys.len() * 2;
        if used_bytes < reserved_bytes {
            let zero_start = offset_table_start + used_bytes;
            let zero_end = offset_table_start + reserved_bytes;
            for b in &mut self.buf[zero_start..zero_end] {
                *b = 0;
            }
        }
    }

    /// Bytes in the data sector buffer (for incremental stream flush).
    pub fn data_sector_len(&self) -> usize {
        self.buf.len()
    }

    /// Append bytes appended to the data sector since `start` to `out`.
    pub fn write_data_sector_since(
        &self,
        out: &mut impl std::io::Write,
        start: usize,
    ) -> std::io::Result<()> {
        if self.buf.len() > start {
            out.write_all(&self.buf[start..])?;
        }
        Ok(())
    }

    /// Relative record offsets in the data sector (for stream seal).
    pub fn record_offsets(&self) -> &[u32] {
        &self.record_offsets
    }

    pub fn schema_keys(&self) -> &[String] {
        &self.schema.keys
    }

    pub fn data_buf(&self) -> &[u8] {
        &self.buf
    }

    pub fn slot_sigils(&self) -> &[u8] {
        &self.slot_sigils
    }

    /// Seal the file and return the complete `.nxb` bytes; one tail-index entry per top-level object.
    pub fn finish(self) -> Vec<u8> {
        debug_assert!(self.frames.is_empty(), "unclosed objects");

        let schema_bytes = build_schema(&self.schema.keys, &self.slot_sigils);
        let dict_hash = murmur3_64(&schema_bytes);

        let data_sector = self.buf;
        let data_start_abs = 32u64 + schema_bytes.len() as u64;

        let tail_ptr: u64 = data_start_abs + data_sector.len() as u64;
        // Build per-record tail-index
        let abs_offsets: Vec<u64> = self
            .record_offsets
            .iter()
            .map(|rel| data_start_abs + u64::from(*rel))
            .collect();
        let tail = build_tail_index_records(&abs_offsets, tail_ptr);

        let total = 32 + schema_bytes.len() + data_sector.len() + tail.len();
        let mut out = Vec::with_capacity(total);

        // Preamble
        out.extend_from_slice(&MAGIC_FILE.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&FLAG_SCHEMA_EMBEDDED.to_le_bytes());
        out.extend_from_slice(&dict_hash.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());

        out.extend_from_slice(&schema_bytes);
        out.extend_from_slice(&data_sector);
        out.extend_from_slice(&tail);
        out
    }

    // ── Typed write methods ──────────────────────────────────────────────────

    #[inline(always)]
    fn mark_slot_sigil(&mut self, slot: Slot, sigil: u8) {
        let idx = slot.0 as usize;
        if idx < self.slot_sigils.len() {
            let cur = self.slot_sigils[idx];
            // Priority: real typed sigils win over 0 (unset) or '^' (null).
            // Once a non-null, non-zero sigil is set, don't overwrite.
            if cur == 0 || (cur == b'^' && sigil != b'^') {
                self.slot_sigils[idx] = sigil;
            }
        }
    }

    #[inline(always)]
    fn mark_slot(&mut self, slot: Slot) {
        let frame = self.frames.last_mut().expect("no active object");
        let slot_idx = slot.0 as usize;

        // Set bitmask bit — accounting for continuation bits
        let byte_idx = slot_idx / 7;
        let bit_idx = slot_idx % 7;
        frame.bitmask[byte_idx] |= 1 << bit_idx;

        // Record relative offset from object start
        let rel = (self.buf.len() - frame.start) as u16;

        let slot_u16 = slot.0;
        if (slot_u16 as i32) < frame.last_slot {
            frame.needs_sort = true;
        }
        frame.last_slot = slot_u16 as i32;

        frame.offset_table.push(rel);
        frame.slot_offsets.push((slot_u16, self.buf.len() as u32));
    }

    /// Write a signed 64-bit integer field.
    #[inline]
    pub fn write_i64(&mut self, slot: Slot, v: i64) {
        self.mark_slot_sigil(slot, b'=');
        self.mark_slot(slot);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a 64-bit float field.
    #[inline]
    pub fn write_f64(&mut self, slot: Slot, v: f64) {
        self.mark_slot_sigil(slot, b'~');
        self.mark_slot(slot);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Write a boolean field.
    #[inline]
    pub fn write_bool(&mut self, slot: Slot, v: bool) {
        self.mark_slot_sigil(slot, b'?');
        self.mark_slot(slot);
        self.buf.push(if v { 0x01 } else { 0x00 });
        self.buf.extend_from_slice(&[0u8; 7]);
    }

    /// Write a nanosecond-precision Unix timestamp field.
    #[inline]
    pub fn write_time(&mut self, slot: Slot, unix_ns: i64) {
        self.mark_slot_sigil(slot, b'@');
        self.mark_slot(slot);
        self.buf.extend_from_slice(&unix_ns.to_le_bytes());
    }

    /// Write an explicit null field (present in the object but carrying no value).
    #[inline]
    pub fn write_null(&mut self, slot: Slot) {
        self.mark_slot_sigil(slot, b'^');
        self.mark_slot(slot);
        self.buf.push(0x00);
        // pad to 8
        self.buf.extend_from_slice(&[0u8; 7]);
    }

    /// Write a UTF-8 string field.
    #[inline]
    pub fn write_str(&mut self, slot: Slot, v: &str) {
        self.mark_slot_sigil(slot, b'"');
        self.mark_slot(slot);
        let bytes = v.as_bytes();
        self.buf
            .extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(bytes);
        // pad to 8
        let pad = (8 - (4 + bytes.len()) % 8) % 8;
        for _ in 0..pad {
            self.buf.push(0);
        }
    }

    /// Write a raw binary field.
    #[inline]
    pub fn write_bytes(&mut self, slot: Slot, data: &[u8]) {
        self.mark_slot_sigil(slot, b'<');
        self.mark_slot(slot);
        self.buf
            .extend_from_slice(&(data.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(data);
        let pad = (8 - (4 + data.len()) % 8) % 8;
        for _ in 0..pad {
            self.buf.push(0);
        }
    }

    pub fn write_list_i64(&mut self, slot: Slot, values: &[i64]) {
        self.mark_slot_sigil(slot, b'L');
        self.mark_slot(slot);
        let total = 16 + values.len() * 8;
        self.buf.extend_from_slice(&MAGIC_LIST.to_le_bytes());
        self.buf.extend_from_slice(&(total as u32).to_le_bytes());
        self.buf.push(b'=');
        self.buf
            .extend_from_slice(&(values.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(&[0u8; 3]);
        for v in values {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    pub fn write_list_f64(&mut self, slot: Slot, values: &[f64]) {
        self.mark_slot_sigil(slot, b'L');
        self.mark_slot(slot);
        let total = 16 + values.len() * 8;
        self.buf.extend_from_slice(&MAGIC_LIST.to_le_bytes());
        self.buf.extend_from_slice(&(total as u32).to_le_bytes());
        self.buf.push(b'~');
        self.buf
            .extend_from_slice(&(values.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(&[0u8; 3]);
        for v in values {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn build_schema(keys: &[String], sigils: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&(keys.len() as u16).to_le_bytes());
    for (i, _) in keys.iter().enumerate() {
        let s = sigils.get(i).copied().unwrap_or(0);
        // 0 means "not yet observed" — use default SIGIL_STR
        b.push(if s == 0 { b'"' } else { s });
    }
    for key in keys {
        b.extend_from_slice(key.as_bytes());
        b.push(0x00);
    }
    while b.len() % 8 != 0 {
        b.push(0x00);
    }
    b
}

#[allow(dead_code)]
fn build_tail_index(data_start: u64) -> Vec<u8> {
    let mut b = Vec::with_capacity(26);
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&data_start.to_le_bytes());
    b.extend_from_slice(&data_start.to_le_bytes());
    b.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());
    b
}

/// Write streamable v1.1 preamble + embedded schema (`TailPtr = 0`). Returns data-sector start offset.
pub fn write_stream_file_header(
    out: &mut impl std::io::Write,
    schema: &Schema,
) -> std::io::Result<u64> {
    let schema_bytes = build_schema(&schema.keys, &[]);
    let dict_hash = murmur3_64(&schema_bytes);
    let data_start_abs = 32u64 + schema_bytes.len() as u64;
    out.write_all(&MAGIC_FILE.to_le_bytes())?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&FLAG_SCHEMA_EMBEDDED.to_le_bytes())?;
    out.write_all(&dict_hash.to_le_bytes())?;
    out.write_all(&0u64.to_le_bytes())?;
    out.write_all(&0u64.to_le_bytes())?;
    out.write_all(&schema_bytes)?;
    Ok(data_start_abs)
}

/// Append tail-index + footer for a streamable file (after all records are on disk).
pub fn write_stream_file_footer(
    out: &mut std::fs::File,
    _data_start_abs: u64,
    record_abs_offsets: &[u64],
) -> std::io::Result<u64> {
    use std::io::Seek;
    let tail_ptr = out.seek(std::io::SeekFrom::End(0))?;
    let tail = build_tail_index_records(record_abs_offsets, tail_ptr);
    out.write_all(&tail)?;
    Ok(tail_ptr)
}

fn build_tail_index_records(record_abs_offsets: &[u64], tail_ptr: u64) -> Vec<u8> {
    // EntryCount (4) + N * [KeyID (2) + AbsoluteOffset (8)] + FooterTailPtr (8) + Magic (4)
    let n = record_abs_offsets.len();
    let mut b = Vec::with_capacity(4 + n * 10 + 12);
    b.extend_from_slice(&(n as u32).to_le_bytes());
    for (i, &abs) in record_abs_offsets.iter().enumerate() {
        b.extend_from_slice(&(i as u16).to_le_bytes()); // KeyID = record index
        b.extend_from_slice(&abs.to_le_bytes());
    }
    b.extend_from_slice(&tail_ptr.to_le_bytes());
    b.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());
    b
}

pub(crate) fn murmur3_64(data: &[u8]) -> u64 {
    let mut h: u64 = 0x9368_1D62_5531_3A99;
    for chunk in data.chunks(8) {
        let mut k = 0u64;
        for (i, &b) in chunk.iter().enumerate() {
            k |= (b as u64) << (i * 8);
        }
        k = k.wrapping_mul(0xFF51AFD7ED558CCD);
        k ^= k >> 33;
        h ^= k;
        h = h.wrapping_mul(0xC4CEB9FE1A85EC53);
        h ^= h >> 33;
    }
    h ^= data.len() as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51AFD7ED558CCD);
    h ^= h >> 33;
    h
}
