//! NXS v1.3 compact encoding: dense frames, packed bools, narrow cells, delta tail-index.

use crate::consts::{
    DEFAULT_DELTA_BLOCK_SIZE, FIELD_ATTR_PROMOTED, FIELD_ATTR_U16_LEN, FLAG_DELTA_TAIL,
    FLAG_DENSE_FRAMES, FLAG_DENSE_WIRE_REORDER, FLAG_NARROW_CELLS, FLAG_PACKED_BOOLS,
    FLAG_V13_COMPACT_MASK, MAGIC_FOOTER, MAGIC_OBJ, RECORD_HDR_DENSE, SIGIL_BOOL, SIGIL_FLOAT,
    SIGIL_INT, SIGIL_KEYWORD, SIGIL_NULL, SIGIL_STR, SIGIL_TIME, VERSION, VERSION_V13,
};
use crate::error::{NxsError, Result};

/// Compiler / writer options for v1.3 compact encodings.
#[derive(Debug, Clone)]
pub struct CompactOptions {
    pub dense_frames: bool,
    pub packed_bools: bool,
    pub narrow_cells: bool,
    pub delta_tail: bool,
    pub keyword_promotion: bool,
    /// Promote when `distinct_values / record_count ≤` this ratio (§7).
    pub keyword_max_ratio: f64,
    /// Require at least this many records before ratio-based promotion applies.
    pub keyword_min_records: usize,
    pub delta_block_size: u32,
    /// Use `u16` length prefixes for inline strings when all values fit (§5.2).
    pub u16_string_lengths: bool,
    /// Emit fixed-width cells in descending alignment width before strings (§4.2).
    pub dense_wire_reorder: bool,
}

impl Default for CompactOptions {
    fn default() -> Self {
        Self {
            dense_frames: false,
            packed_bools: false,
            narrow_cells: false,
            delta_tail: false,
            keyword_promotion: false,
            keyword_max_ratio: 0.5,
            keyword_min_records: 32,
            delta_block_size: DEFAULT_DELTA_BLOCK_SIZE,
            u16_string_lengths: false,
            dense_wire_reorder: false,
        }
    }
}

impl CompactOptions {
    /// All wire-format compact flags (§4 + §5 + §6), plus keyword inference (§7).
    pub fn compact() -> Self {
        Self {
            dense_frames: true,
            packed_bools: true,
            narrow_cells: true,
            delta_tail: true,
            keyword_promotion: true,
            keyword_max_ratio: 0.5,
            keyword_min_records: 32,
            delta_block_size: 1024,
            u16_string_lengths: true,
            dense_wire_reorder: true,
        }
    }

    pub fn any_wire_flag(&self) -> bool {
        self.dense_frames || self.packed_bools || self.narrow_cells || self.delta_tail
    }

    pub fn preamble_flags(&self) -> u16 {
        let mut f = 0u16;
        if self.dense_frames {
            f |= FLAG_DENSE_FRAMES;
        }
        if self.packed_bools {
            f |= FLAG_PACKED_BOOLS;
        }
        if self.narrow_cells {
            f |= FLAG_NARROW_CELLS;
        }
        if self.delta_tail {
            f |= FLAG_DELTA_TAIL;
        }
        if self.dense_wire_reorder {
            f |= FLAG_DENSE_WIRE_REORDER;
        }
        f
    }

    pub fn version(&self) -> u16 {
        if self.any_wire_flag() {
            VERSION_V13
        } else {
            VERSION
        }
    }
}

/// Parsed v1.3 schema extensions (widths, promotion, value pool).
#[derive(Debug, Clone, Default)]
pub struct ExtendedSchema {
    pub keys: Vec<String>,
    pub sigils: Vec<u8>,
    pub widths: Vec<u8>,
    pub field_attrs: Vec<u8>,
    pub value_pool: Vec<String>,
}

impl ExtendedSchema {
    pub fn from_basic(keys: Vec<String>, sigils: Vec<u8>) -> Self {
        let n = keys.len();
        Self {
            keys,
            sigils,
            widths: vec![0u8; n],
            field_attrs: vec![0u8; n],
            value_pool: Vec::new(),
        }
    }

    pub fn is_promoted(&self, slot: usize) -> bool {
        self.field_attrs
            .get(slot)
            .is_some_and(|a| a & FIELD_ATTR_PROMOTED != 0)
    }

    pub fn is_u16_len(&self, slot: usize) -> bool {
        self.field_attrs
            .get(slot)
            .is_some_and(|a| a & FIELD_ATTR_U16_LEN != 0)
    }

    /// Bytes in the length prefix for an inline string/binary cell (`0` when promoted).
    pub fn str_len_prefix(&self, slot: usize) -> usize {
        if self.is_promoted(slot) {
            0
        } else if self.is_u16_len(slot) {
            2
        } else {
            4
        }
    }

    pub fn cell_width(&self, slot: usize) -> u8 {
        if self.is_promoted(slot) || self.sigils.get(slot).is_some_and(|&s| s == SIGIL_KEYWORD) {
            return 2;
        }
        let w = self.widths.get(slot).copied().unwrap_or(0);
        if w == 0 {
            8
        } else {
            w
        }
    }

    pub fn bool_slots(&self) -> Vec<usize> {
        self.sigils
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == SIGIL_BOOL)
            .map(|(i, _)| i)
            .collect()
    }
}

/// Reject unknown v1.3 REQUIRED flags when `supports_v13` is false.
pub fn validate_reader_flags(flags: u16, supports_v13: bool) -> Result<()> {
    let unknown = flags & FLAG_V13_COMPACT_MASK;
    if !supports_v13 && unknown != 0 {
        return Err(NxsError::UnsupportedFlags(unknown));
    }
    Ok(())
}

/// Parse embedded schema with optional v1.3 extensions.
pub fn parse_extended_schema(
    data: &[u8],
    pos: usize,
    flags: u16,
) -> Result<(ExtendedSchema, usize)> {
    if pos + 2 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let key_count = u16::from_le_bytes(
        data[pos..pos + 2]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    if key_count > 256 {
        return Err(NxsError::OutOfBounds);
    }
    let mut p = pos + 2;
    let end = p.checked_add(key_count).ok_or(NxsError::OutOfBounds)?;
    if end > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let sigils = data[p..end].to_vec();
    p = end;

    let mut keys = Vec::with_capacity(key_count);
    for _ in 0..key_count {
        let start = p;
        while p < data.len() && data[p] != 0 {
            p += 1;
        }
        if p >= data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let name = std::str::from_utf8(&data[start..p])
            .map_err(|_| NxsError::ParseError("invalid UTF-8 in schema".into()))?;
        keys.push(name.to_string());
        p += 1;
    }
    if p % 8 != 0 {
        p += 8 - p % 8;
    }

    let mut widths = vec![0u8; key_count];
    if flags & FLAG_NARROW_CELLS != 0 {
        let end = p.checked_add(key_count).ok_or(NxsError::OutOfBounds)?;
        if end > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        widths.copy_from_slice(&data[p..end]);
        p = end;
    }

    let has_attrs = flags & FLAG_V13_COMPACT_MASK != 0;
    let mut field_attrs = vec![0u8; key_count];
    if has_attrs {
        let end = p.checked_add(key_count).ok_or(NxsError::OutOfBounds)?;
        if end > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        field_attrs.copy_from_slice(&data[p..end]);
        p = end;
    }

    let mut value_pool = Vec::new();
    if p + 2 <= data.len() {
        let value_count = u16::from_le_bytes(
            data[p..p + 2]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        p += 2;
        if value_count > 0 {
            for _ in 0..value_count {
                let start = p;
                while p < data.len() && data[p] != 0 {
                    p += 1;
                }
                if p >= data.len() {
                    return Err(NxsError::OutOfBounds);
                }
                let s = std::str::from_utf8(&data[start..p])
                    .map_err(|_| NxsError::ParseError("invalid UTF-8 in value pool".into()))?;
                value_pool.push(s.to_string());
                p += 1;
            }
            if p % 8 != 0 {
                p += 8 - p % 8;
            }
        }
    }

    Ok((
        ExtendedSchema {
            keys,
            sigils,
            widths,
            field_attrs,
            value_pool,
        },
        p,
    ))
}

/// Build schema bytes including v1.3 extensions.
pub fn build_extended_schema(schema: &ExtendedSchema, flags: u16) -> Vec<u8> {
    let n = schema.keys.len();
    let mut b = Vec::new();
    b.extend_from_slice(&(n as u16).to_le_bytes());
    for &s in &schema.sigils {
        b.push(s);
    }
    for key in &schema.keys {
        b.extend_from_slice(key.as_bytes());
        b.push(0);
    }
    while b.len() % 8 != 0 {
        b.push(0);
    }
    if flags & FLAG_NARROW_CELLS != 0 {
        for i in 0..n {
            b.push(schema.widths.get(i).copied().unwrap_or(0));
        }
    }
    if flags & FLAG_V13_COMPACT_MASK != 0 {
        for i in 0..n {
            b.push(schema.field_attrs.get(i).copied().unwrap_or(0));
        }
    }
    b.extend_from_slice(&(schema.value_pool.len() as u16).to_le_bytes());
    if !schema.value_pool.is_empty() {
        for v in &schema.value_pool {
            b.extend_from_slice(v.as_bytes());
            b.push(0);
        }
        while b.len() % 8 != 0 {
            b.push(0);
        }
    }
    b
}

/// Infer narrow widths from observed integer/float ranges.
pub fn infer_narrow_widths(sigils: &[u8], mins: &[i64], maxs: &[i64]) -> Vec<u8> {
    let n = sigils.len();
    let mut widths = vec![0u8; n];
    for i in 0..n {
        let w = match sigils[i] {
            SIGIL_INT => infer_int_width(
                mins.get(i).copied().unwrap_or(0),
                maxs.get(i).copied().unwrap_or(0),
            ),
            SIGIL_FLOAT => 8u8,
            _ => 0,
        };
        widths[i] = w;
    }
    widths
}

fn infer_int_width(min: i64, max: i64) -> u8 {
    if min >= 0 && max <= u8::MAX as i64 {
        1
    } else if min >= i16::MIN as i64 && max <= i16::MAX as i64 {
        2
    } else if min >= i32::MIN as i64 && max <= i32::MAX as i64 {
        4
    } else {
        8
    }
}

pub fn intern_value(pool: &mut Vec<String>, s: &str) -> u16 {
    if let Some(i) = pool.iter().position(|v| v == s) {
        return i as u16;
    }
    let i = pool.len();
    pool.push(s.to_string());
    i as u16
}

/// Scan string columns for keyword promotion candidates (§7).
///
/// Promotes when `distinct / record_count ≤ max_ratio` and `record_count ≥
/// min_records`, so unique-per-record columns (ratio ≈ 1) are never promoted.
pub fn scan_keyword_promotion(
    sigils: &[u8],
    string_values: &[Vec<String>],
    max_ratio: f64,
    min_records: usize,
) -> (Vec<u8>, Vec<String>) {
    let n = sigils.len();
    let mut field_attrs = vec![0u8; n];
    let mut value_pool = Vec::new();
    let record_count = string_values.iter().map(|v| v.len()).max().unwrap_or(0);
    if record_count < min_records {
        return (field_attrs, value_pool);
    }
    for (slot, &sig) in sigils.iter().enumerate() {
        if sig != SIGIL_STR {
            continue;
        }
        let Some(vals) = string_values.get(slot) else {
            continue;
        };
        if vals.len() < min_records {
            continue;
        }
        let mut distinct = std::collections::HashSet::new();
        for v in vals {
            distinct.insert(v.as_str());
        }
        if distinct.is_empty() {
            continue;
        }
        let ratio = distinct.len() as f64 / vals.len() as f64;
        if ratio > max_ratio {
            continue;
        }
        field_attrs[slot] |= FIELD_ATTR_PROMOTED;
        for v in vals {
            intern_value(&mut value_pool, v);
        }
    }
    (field_attrs, value_pool)
}

/// Mark non-promoted string columns whose values fit in `u16` for narrow length prefixes.
pub fn scan_u16_string_lengths(
    sigils: &[u8],
    string_values: &[Vec<String>],
    field_attrs: &mut [u8],
) {
    for (slot, &sig) in sigils.iter().enumerate() {
        if sig != SIGIL_STR {
            continue;
        }
        if field_attrs[slot] & FIELD_ATTR_PROMOTED != 0 {
            continue;
        }
        let Some(vals) = string_values.get(slot) else {
            continue;
        };
        let max_len = vals.iter().map(|s| s.len()).max().unwrap_or(0);
        if max_len <= u16::MAX as usize {
            field_attrs[slot] |= FIELD_ATTR_U16_LEN;
        }
    }
}

// ── Delta tail-index (§6) ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DeltaTailLayout {
    pub tail_ptr: u64,
    pub record_count: usize,
    pub block_size: u32,
    pub single_key_id: bool,
    pub anchors_off: usize,
    pub deltas_off: usize,
    pub key_ids_off: Option<usize>,
}

pub fn build_delta_tail_index(
    abs_offsets: &[u64],
    tail_ptr: u64,
    block_size: u32,
) -> Result<Vec<u8>> {
    let n = abs_offsets.len();
    let a = block_size.max(1) as usize;
    let anchor_count = if n == 0 { 0 } else { (n + a - 1) / a };
    let mut anchors = Vec::with_capacity(anchor_count);
    for j in 0..anchor_count {
        let rec = j * a;
        anchors.push(abs_offsets.get(rec).copied().unwrap_or(0));
    }
    let mut deltas = Vec::with_capacity(n);
    for (k, &abs) in abs_offsets.iter().enumerate() {
        let anchor = anchors[k / a];
        let delta = abs.checked_sub(anchor).ok_or(NxsError::OutOfBounds)?;
        if delta > u32::MAX as u64 {
            return Err(NxsError::Overflow);
        }
        deltas.push(delta as u32);
    }
    let single_key = true;
    let header_len = 4 + 4 + 2 + 2 + 8 * anchor_count;
    let deltas_len = n * 4;
    let key_ids_len = if single_key { 0 } else { n * 2 };
    let total = header_len + deltas_len + key_ids_len + 12;
    let mut b = Vec::with_capacity(total);
    b.extend_from_slice(&(n as u32).to_le_bytes());
    b.extend_from_slice(&block_size.to_le_bytes());
    let flags: u16 = if single_key { 0x0001 } else { 0 };
    b.extend_from_slice(&flags.to_le_bytes());
    b.extend_from_slice(&(anchor_count as u16).to_le_bytes());
    while b.len() % 8 != 0 {
        b.push(0);
    }
    for aoff in &anchors {
        b.extend_from_slice(&aoff.to_le_bytes());
    }
    for d in &deltas {
        b.extend_from_slice(&d.to_le_bytes());
    }
    if !single_key {
        for i in 0..n {
            b.extend_from_slice(&(i as u16).to_le_bytes());
        }
    }
    b.extend_from_slice(&tail_ptr.to_le_bytes());
    b.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());
    Ok(b)
}

pub fn parse_delta_tail_layout(data: &[u8], tail_ptr: usize) -> Result<DeltaTailLayout> {
    if tail_ptr + 8 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let record_count = u32::from_le_bytes(
        data[tail_ptr..tail_ptr + 4]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    let block_size = u32::from_le_bytes(
        data[tail_ptr + 4..tail_ptr + 8]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    let ti_flags = u16::from_le_bytes(
        data[tail_ptr + 8..tail_ptr + 10]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    let anchor_count = u16::from_le_bytes(
        data[tail_ptr + 10..tail_ptr + 12]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    let anchors_off = tail_ptr + align_to(12, 8);
    let mut p = anchors_off;
    p += anchor_count * 8;
    let deltas_off = p;
    p += record_count * 4;
    let single_key_id = ti_flags & 0x0001 != 0;
    let key_ids_off = if single_key_id { None } else { Some(p) };
    Ok(DeltaTailLayout {
        tail_ptr: tail_ptr as u64,
        record_count,
        block_size,
        single_key_id,
        anchors_off,
        deltas_off,
        key_ids_off,
    })
}

pub fn delta_record_offset(data: &[u8], layout: &DeltaTailLayout, index: usize) -> Result<usize> {
    if index >= layout.record_count {
        return Err(NxsError::OutOfBounds);
    }
    let a = layout.block_size.max(1) as usize;
    let anchor_idx = index / a;
    let anchor_off = layout.anchors_off + anchor_idx * 8;
    if anchor_off + 8 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let anchor = u64::from_le_bytes(
        data[anchor_off..anchor_off + 8]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    let delta_off = layout.deltas_off + index * 4;
    let delta = u32::from_le_bytes(
        data[delta_off..delta_off + 4]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as u64;
    Ok((anchor + delta) as usize)
}

/// Classic v1.2 row tail-index entry offset for record `index`.
pub fn classic_record_offset(data: &[u8], tail_start: usize, index: usize) -> Option<usize> {
    let entry = tail_start + index * 10;
    let abs = u64::from_le_bytes(data.get(entry + 2..entry + 10)?.try_into().ok()?) as usize;
    Some(abs)
}

// ── Record framing helpers ────────────────────────────────────────────────────

/// True when the NYXO object at `offset` uses dense framing.
pub fn is_dense_record(data: &[u8], offset: usize) -> Result<bool> {
    if offset + 9 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let magic = u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    if magic != MAGIC_OBJ {
        return Err(NxsError::BadMagic);
    }
    let hdr = data[offset + 8];
    Ok(hdr & RECORD_HDR_DENSE != 0)
}

fn align_to(pos: usize, align: usize) -> usize {
    if align == 0 {
        return pos;
    }
    (pos + align - 1) & !(align - 1)
}

/// Read payload length from an inline string/binary cell.
pub fn read_str_cell_len(data: &[u8], off: usize, prefix_len: usize) -> Result<usize> {
    match prefix_len {
        2 => {
            if off + 2 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            Ok(u16::from_le_bytes(
                data[off..off + 2]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize)
        }
        4 => {
            if off + 4 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            Ok(u32::from_le_bytes(
                data[off..off + 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize)
        }
        _ => Err(NxsError::OutOfBounds),
    }
}

/// Advance past a length-prefixed string cell (matches `append_cell` padding).
fn advance_past_str_cell(pos: usize, payload_len: usize, prefix_len: usize) -> usize {
    let cell_bytes = prefix_len + payload_len;
    let pad = (8 - cell_bytes % 8) % 8;
    pos + cell_bytes + pad
}

fn str_cell_encoded_len(payload_len: usize, prefix_len: usize) -> usize {
    let cell_bytes = prefix_len + payload_len;
    cell_bytes + (8 - cell_bytes % 8) % 8
}

/// Encode a fixed-width integer cell.
pub fn encode_int_cell(v: i64, width: u8) -> Result<Vec<u8>> {
    match width {
        1 => {
            if v < 0 || v > u8::MAX as i64 {
                return Err(NxsError::ValueOutOfRange);
            }
            Ok(vec![v as u8])
        }
        2 => {
            if v < i16::MIN as i64 || v > i16::MAX as i64 {
                return Err(NxsError::ValueOutOfRange);
            }
            Ok((v as i16).to_le_bytes().to_vec())
        }
        4 => {
            if v < i32::MIN as i64 || v > i32::MAX as i64 {
                return Err(NxsError::ValueOutOfRange);
            }
            Ok((v as i32).to_le_bytes().to_vec())
        }
        8 => Ok(v.to_le_bytes().to_vec()),
        _ => Err(NxsError::ParseError(format!("invalid int width: {width}"))),
    }
}

/// Decode fixed-width integer cell at offset.
pub fn decode_int_cell(data: &[u8], offset: usize, width: u8) -> Result<i64> {
    match width {
        1 => Ok(data.get(offset).copied().unwrap_or(0) as i64),
        2 => {
            let b: [u8; 2] = data[offset..offset + 2]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?;
            Ok(i16::from_le_bytes(b) as i64)
        }
        4 => {
            let b: [u8; 4] = data[offset..offset + 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?;
            Ok(i32::from_le_bytes(b) as i64)
        }
        8 => {
            let b: [u8; 8] = data[offset..offset + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?;
            Ok(i64::from_le_bytes(b))
        }
        _ => Err(NxsError::OutOfBounds),
    }
}

pub fn encode_f64_cell(v: f64, width: u8) -> Result<Vec<u8>> {
    match width {
        4 => Ok((v as f32).to_le_bytes().to_vec()),
        8 => Ok(v.to_le_bytes().to_vec()),
        _ => Err(NxsError::ParseError(format!(
            "invalid float width: {width}"
        ))),
    }
}

pub fn decode_f64_cell(data: &[u8], offset: usize, width: u8) -> Result<f64> {
    match width {
        4 => {
            let b: [u8; 4] = data[offset..offset + 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?;
            Ok(f64::from(f32::from_le_bytes(b)))
        }
        8 => {
            let b: [u8; 8] = data[offset..offset + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?;
            Ok(f64::from_le_bytes(b))
        }
        _ => Err(NxsError::OutOfBounds),
    }
}

/// Resolve keyword / promoted-string value from pool index.
pub fn resolve_value_pool(schema: &ExtendedSchema, index: u16) -> Option<&str> {
    schema.value_pool.get(index as usize).map(|s| s.as_str())
}

/// Materialise keyword or promoted string field.
pub fn materialise_keyword(schema: &ExtendedSchema, slot: usize, index: u16) -> Option<String> {
    let sig = schema.sigils.get(slot).copied().unwrap_or(0);
    if sig == SIGIL_KEYWORD || schema.is_promoted(slot) {
        resolve_value_pool(schema, index).map(|s| s.to_string())
    } else {
        None
    }
}

/// Precomputed per-field cell placement for dense/sparse readers.
#[derive(Debug, Clone)]
pub struct RowCellPlan {
    pub bool_slots: Vec<usize>,
    pub first_bool: Option<usize>,
    pub packed_bools: bool,
    pub narrow: bool,
    pub dense_allowed: bool,
    pub dense_wire_reorder: bool,
}

impl RowCellPlan {
    pub fn new(schema: &ExtendedSchema, flags: u16) -> Self {
        Self {
            bool_slots: schema.bool_slots(),
            first_bool: schema.bool_slots().first().copied(),
            packed_bools: flags & FLAG_PACKED_BOOLS != 0,
            narrow: flags & FLAG_NARROW_CELLS != 0,
            dense_allowed: flags & FLAG_DENSE_FRAMES != 0,
            dense_wire_reorder: flags & FLAG_DENSE_WIRE_REORDER != 0,
        }
    }

    /// Wire bytes for the shared packed-bool word (1 B for ≤8 bool fields, up to 8 B).
    pub fn bool_word_bytes(&self) -> usize {
        if !self.packed_bools || self.bool_slots.is_empty() {
            0
        } else {
            ((self.bool_slots.len() + 7) / 8).max(1)
        }
    }

    /// Dense-frame cell order on the wire (schema order when `FLAG_DENSE_WIRE_REORDER` clear).
    pub fn dense_wire_order(&self, schema: &ExtendedSchema) -> Vec<usize> {
        if !self.dense_wire_reorder {
            return (0..schema.keys.len()).collect();
        }
        dense_wire_order(schema, self)
    }
}

fn is_var_sigil(sig: u8) -> bool {
    sig == SIGIL_STR || sig == b'<'
}

fn dense_cell_align_width(fi: usize, schema: &ExtendedSchema, plan: &RowCellPlan) -> usize {
    if plan.packed_bools && plan.bool_slots.contains(&fi) {
        return plan.bool_word_bytes();
    }
    let sig = schema.sigils[fi];
    if is_var_sigil(sig) {
        return 0;
    }
    if schema.is_promoted(fi) || sig == SIGIL_KEYWORD {
        return 2;
    }
    match sig {
        SIGIL_INT | SIGIL_FLOAT => {
            if plan.narrow {
                schema.cell_width(fi) as usize
            } else {
                8
            }
        }
        SIGIL_TIME => 8,
        SIGIL_BOOL if !plan.packed_bools => 8,
        SIGIL_NULL => 1,
        _ => 8,
    }
}

pub fn dense_wire_order(schema: &ExtendedSchema, plan: &RowCellPlan) -> Vec<usize> {
    let n = schema.keys.len();
    let mut fixed: Vec<(usize, usize)> = Vec::new();
    let mut vars: Vec<usize> = Vec::new();
    for fi in 0..n {
        let sig = schema.sigils[fi];
        if is_var_sigil(sig) {
            vars.push(fi);
            continue;
        }
        if plan.packed_bools && plan.bool_slots.contains(&fi) {
            if plan.first_bool == Some(fi) {
                fixed.push((plan.bool_word_bytes(), fi));
            }
            continue;
        }
        fixed.push((dense_cell_align_width(fi, schema, plan), fi));
    }
    fixed.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    fixed.into_iter().map(|(_, s)| s).chain(vars).collect()
}

fn advance_dense_past_cell(
    data: &[u8],
    body_base: usize,
    pos: usize,
    fi: usize,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Result<usize> {
    let sig = schema.sigils[fi];
    let w = if plan.narrow {
        schema.cell_width(fi)
    } else {
        8
    };
    if plan.packed_bools && plan.bool_slots.contains(&fi) {
        return Ok(advance_past_bool_word(pos, plan));
    }
    match sig {
        SIGIL_INT | SIGIL_FLOAT | SIGIL_BOOL if !plan.packed_bools || sig != SIGIL_BOOL => {
            Ok(align_to(pos, w as usize) + w as usize)
        }
        SIGIL_STR | b'<' => {
            if schema.is_promoted(fi) {
                Ok(align_to(pos, 2) + 2)
            } else {
                let prefix = schema.str_len_prefix(fi);
                let abs = body_base + pos;
                if abs + prefix > data.len() {
                    return Err(NxsError::OutOfBounds);
                }
                let len = read_str_cell_len(data, abs, prefix)?;
                Ok(advance_past_str_cell(pos, len, prefix))
            }
        }
        SIGIL_KEYWORD => Ok(align_to(pos, 2) + 2),
        _ => Ok(align_to(pos, 8) + 8),
    }
}

fn emit_bool_word(body: &mut Vec<u8>, word: u64, plan: &RowCellPlan) {
    let bw = plan.bool_word_bytes();
    while body.len() % bw != 0 {
        body.push(0);
    }
    let bytes = word.to_le_bytes();
    body.extend_from_slice(&bytes[..bw]);
}

fn advance_past_bool_word(pos: usize, plan: &RowCellPlan) -> usize {
    let bw = plan.bool_word_bytes();
    align_to(pos, bw) + bw
}

/// Walk dense record cells; returns absolute offset of `slot` (body-relative walk matches encode).
pub fn dense_field_offset(
    data: &[u8],
    obj_offset: usize,
    slot: usize,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Result<Option<usize>> {
    let body_base = obj_offset + 9; // magic + len + hdr
    let mut pos = 0usize; // body-relative
    for &fi in &plan.dense_wire_order(schema) {
        if plan.packed_bools && plan.bool_slots.contains(&fi) {
            if plan.bool_slots.contains(&slot) && Some(fi) == plan.first_bool {
                let byte = plan.bool_slots.iter().position(|&s| s == slot).unwrap() / 8;
                return Ok(Some(
                    body_base + align_to(pos, plan.bool_word_bytes()) + byte,
                ));
            }
            if Some(fi) == plan.first_bool {
                pos = advance_past_bool_word(pos, plan);
            }
            continue;
        }
        let sig = schema.sigils[fi];
        let w = if plan.narrow {
            schema.cell_width(fi)
        } else {
            8
        };
        if fi == slot {
            let off = if schema.is_promoted(fi) || sig == SIGIL_KEYWORD {
                align_to(pos, 2)
            } else if is_var_sigil(sig) {
                pos
            } else {
                align_to(pos, w as usize)
            };
            return Ok(Some(body_base + off));
        }
        pos = advance_dense_past_cell(data, body_base, pos, fi, schema, plan)?;
    }
    Ok(None)
}

/// Extended resolve_slot aware of v1.3 record headers and dense frames.
pub fn resolve_field_offset(
    data: &[u8],
    obj_offset: usize,
    slot: usize,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
    dense_frames: bool,
) -> Option<usize> {
    if dense_frames {
        let hdr = *data.get(obj_offset + 8)?;
        if hdr & RECORD_HDR_DENSE != 0 {
            return dense_field_offset(data, obj_offset, slot, schema, plan)
                .ok()
                .flatten();
        }
        // sparse with record header byte
        return resolve_slot_v13_sparse(data, obj_offset, slot, schema, plan);
    }
    resolve_slot_v12(data, obj_offset, slot)
}

/// v1.2 LEB128 bitmask walker (duplicated from query.rs to avoid module cycles).
fn resolve_slot_v12(data: &[u8], obj_offset: usize, slot: usize) -> Option<usize> {
    let mut p = obj_offset.checked_add(8)?;
    let mut cur = 0usize;
    let mut table_idx = 0usize;
    let mut found = false;
    let mut b: u8;
    loop {
        b = *data.get(p)?;
        p += 1;
        let bits = b & 0x7F;
        for bit in 0..7usize {
            if cur == slot {
                if (bits >> bit) & 1 == 0 {
                    return None;
                }
                found = true;
            } else if cur < slot && (bits >> bit) & 1 == 1 {
                table_idx += 1;
            }
            cur += 1;
        }
        if found && b & 0x80 == 0 {
            break;
        }
        if cur > slot && found {
            break;
        }
        if b & 0x80 == 0 {
            return None;
        }
    }
    while b & 0x80 != 0 {
        b = *data.get(p)?;
        p += 1;
    }
    let table_start = p + table_idx * 2;
    let rel = u16::from_le_bytes(data.get(table_start..table_start + 2)?.try_into().ok()?) as usize;
    obj_offset.checked_add(rel)
}

fn resolve_slot_v13_sparse(
    data: &[u8],
    obj_offset: usize,
    slot: usize,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Option<usize> {
    let mut p = obj_offset.checked_add(9)?;
    let mut cur = 0usize;
    let mut table_idx = 0usize;
    let mut found = false;
    let mut b: u8;
    loop {
        b = *data.get(p)?;
        p += 1;
        let bits = b & 0x7F;
        for bit in 0..7usize {
            if cur == slot {
                if (bits >> bit) & 1 == 0 {
                    return None;
                }
                found = true;
            } else if cur < slot && (bits >> bit) & 1 == 1 {
                if !(plan.packed_bools && plan.bool_slots.contains(&cur)) {
                    table_idx += 1;
                }
            }
            cur += 1;
        }
        if found && b & 0x80 == 0 {
            break;
        }
        if cur > slot && found {
            break;
        }
        if b & 0x80 == 0 {
            return None;
        }
    }
    while b & 0x80 != 0 {
        b = *data.get(p)?;
        p += 1;
    }
    if plan.packed_bools && plan.bool_slots.contains(&slot) {
        if let Some(fb) = plan.first_bool {
            let mut present_any_bool = false;
            for &bs in &plan.bool_slots {
                if is_bit_set_sparse(data, obj_offset, bs) {
                    present_any_bool = true;
                    break;
                }
            }
            if !present_any_bool {
                return None;
            }
            let base = sparse_bool_word_offset(data, obj_offset, fb, schema, plan)?;
            let bit_in_word = plan.bool_slots.iter().position(|&s| s == slot)?;
            return Some(base + bit_in_word / 8);
        }
    }
    let table_start = p + table_idx * 2;
    let rel = u16::from_le_bytes(data.get(table_start..table_start + 2)?.try_into().ok()?) as usize;
    Some(obj_offset + rel)
}

fn is_bit_set_sparse(data: &[u8], obj_offset: usize, slot: usize) -> bool {
    resolve_slot_v13_sparse_header_only(data, obj_offset, slot).unwrap_or(false)
}

fn resolve_slot_v13_sparse_header_only(
    data: &[u8],
    obj_offset: usize,
    target: usize,
) -> Option<bool> {
    let mut p = obj_offset + 9;
    let mut cur = 0usize;
    loop {
        let b = *data.get(p)?;
        p += 1;
        let bits = b & 0x7F;
        for bit in 0..7 {
            if cur == target {
                return Some((bits >> bit) & 1 == 1);
            }
            cur += 1;
        }
        if b & 0x80 == 0 {
            break;
        }
    }
    None
}

fn sparse_bool_word_offset(
    data: &[u8],
    obj_offset: usize,
    first_bool: usize,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Option<usize> {
    let mut p = obj_offset + 9;
    let n = schema.keys.len();
    for fi in 0..n {
        if fi == first_bool {
            return Some(align_to(p, plan.bool_word_bytes()));
        }
        if plan.packed_bools && plan.bool_slots.contains(&fi) {
            continue;
        }
        if !is_bit_set_sparse(data, obj_offset, fi) {
            continue;
        }
        let sig = schema.sigils[fi];
        let w = if plan.narrow {
            schema.cell_width(fi)
        } else {
            8
        };
        match sig {
            SIGIL_INT | SIGIL_FLOAT => {
                p = align_to(p, w as usize);
                p += w as usize;
            }
            SIGIL_STR | b'<' => {
                if schema.is_promoted(fi) {
                    p = align_to(p, 2) + 2;
                } else {
                    let prefix = schema.str_len_prefix(fi);
                    let len = read_str_cell_len(data, p, prefix).ok()?;
                    p = advance_past_str_cell(p, len, prefix);
                }
            }
            SIGIL_KEYWORD => {
                p = align_to(p, 2) + 2;
            }
            _ => {
                p = align_to(p, 8);
                p += 8;
            }
        }
    }
    None
}

/// In-memory cell for compact object encoding.
#[derive(Debug, Clone)]
pub enum CompactCell {
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    Time(i64),
    Null,
    Keyword(u16),
}

/// Encode one NYXO record with v1.3 compact options.
pub fn encode_compact_record(
    cells: &[(u16, CompactCell)],
    schema: &ExtendedSchema,
    opts: &CompactOptions,
) -> Result<Vec<u8>> {
    let n = schema.keys.len();
    let flags = opts.preamble_flags();
    let plan = RowCellPlan::new(schema, flags);
    let mut present = vec![false; n];
    for (s, _) in cells {
        if (*s as usize) < n {
            present[*s as usize] = true;
        }
    }
    let all_present = present.iter().all(|&p| p);
    let dense = opts.dense_frames && all_present;

    let body = if dense {
        encode_dense_body(cells, schema, &plan)?
    } else {
        encode_sparse_body(cells, schema, &present, &plan)?
    };

    let mut obj = Vec::new();
    obj.extend_from_slice(&MAGIC_OBJ.to_le_bytes());
    obj.extend_from_slice(&0u32.to_le_bytes());

    if dense {
        obj.push(RECORD_HDR_DENSE);
        obj.extend_from_slice(&body);
    } else {
        if opts.dense_frames {
            obj.push(0x00);
        }
        let mask = build_bitmask_bytes(&present);
        obj.extend_from_slice(&mask);
        let present_slots: Vec<usize> = (0..n).filter(|&i| present[i]).collect();
        let header_len = obj.len();
        let ot_slots = sparse_offset_table_slots(&present_slots, &plan);
        let data_start = align_to(header_len + ot_slots.len() * 2, 8);
        let mut rel_offs = Vec::new();
        let mut pos = data_start;
        for &fi in &ot_slots {
            rel_offs.push(pos as u16);
            pos += cell_encoded_len(fi, cells, schema, &plan)?;
        }
        for off in &rel_offs {
            obj.extend_from_slice(&off.to_le_bytes());
        }
        while obj.len() < data_start {
            obj.push(0);
        }
        obj.extend_from_slice(&body);
    }

    let total = obj.len();
    obj[4..8].copy_from_slice(&(total as u32).to_le_bytes());
    Ok(obj)
}

fn sparse_offset_table_slots(present_slots: &[usize], plan: &RowCellPlan) -> Vec<usize> {
    if !plan.packed_bools {
        return present_slots.to_vec();
    }
    let mut out = Vec::new();
    let mut bool_word_added = false;
    for &fi in present_slots {
        if plan.bool_slots.contains(&fi) {
            if !bool_word_added {
                if let Some(fb) = plan.first_bool {
                    if present_slots.contains(&fb) {
                        out.push(fb);
                        bool_word_added = true;
                    }
                }
            }
        } else {
            out.push(fi);
        }
    }
    out
}

fn cell_encoded_len(
    fi: usize,
    cells: &[(u16, CompactCell)],
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Result<usize> {
    if plan.packed_bools && plan.bool_slots.contains(&fi) {
        return Ok(plan.bool_word_bytes());
    }
    let w = if plan.narrow {
        schema.cell_width(fi)
    } else {
        8
    };
    let sig = schema.sigils[fi];
    let cell = cells
        .iter()
        .find(|(s, _)| *s as usize == fi)
        .map(|(_, c)| c);
    match cell {
        Some(CompactCell::Str(s)) if schema.is_promoted(fi) => Ok(2),
        Some(CompactCell::Str(s)) => Ok(str_cell_encoded_len(s.len(), schema.str_len_prefix(fi))),
        Some(CompactCell::I64(_)) if sig == SIGIL_INT => Ok(w as usize),
        Some(CompactCell::F64(_)) if sig == SIGIL_FLOAT => Ok(w as usize),
        Some(CompactCell::Bool(_)) => Ok(8),
        Some(CompactCell::Time(_)) => Ok(8),
        Some(CompactCell::Keyword(_)) => Ok(2),
        Some(CompactCell::Null) => Ok(0),
        _ => Ok(8),
    }
}

fn encode_dense_body(
    cells: &[(u16, CompactCell)],
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Result<Vec<u8>> {
    let mut map: std::collections::HashMap<u16, &CompactCell> = std::collections::HashMap::new();
    for (s, c) in cells {
        map.insert(*s, c);
    }
    let mut body = Vec::new();
    for &fi in &plan.dense_wire_order(schema) {
        if plan.packed_bools && plan.bool_slots.contains(&fi) {
            if Some(fi) == plan.first_bool {
                let mut bool_word: u64 = 0;
                for (bi, &bs) in plan.bool_slots.iter().enumerate() {
                    if let Some(CompactCell::Bool(v)) = map.get(&(bs as u16)) {
                        if *v {
                            bool_word |= 1u64 << bi;
                        }
                    }
                }
                emit_bool_word(&mut body, bool_word, plan);
            }
            continue;
        }
        let cell = map.get(&(fi as u16)).ok_or(NxsError::OutOfBounds)?;
        append_cell(&mut body, fi, cell, schema, plan)?;
    }
    Ok(body)
}

fn encode_sparse_body(
    cells: &[(u16, CompactCell)],
    schema: &ExtendedSchema,
    present: &[bool],
    plan: &RowCellPlan,
) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    let mut map: std::collections::HashMap<u16, &CompactCell> = std::collections::HashMap::new();
    for (s, c) in cells {
        map.insert(*s, c);
    }
    let mut bool_word_emitted = false;
    for fi in 0..schema.keys.len() {
        if !present[fi] {
            continue;
        }
        if plan.packed_bools && plan.bool_slots.contains(&fi) {
            if !bool_word_emitted && Some(fi) == plan.first_bool {
                let mut bool_word: u64 = 0;
                for (bi, &bs) in plan.bool_slots.iter().enumerate() {
                    if !present[bs] {
                        continue;
                    }
                    if let Some(CompactCell::Bool(v)) = map.get(&(bs as u16)) {
                        if *v {
                            bool_word |= 1u64 << bi;
                        }
                    }
                }
                emit_bool_word(&mut body, bool_word, plan);
                bool_word_emitted = true;
            }
            continue;
        }
        let cell = map.get(&(fi as u16)).expect("present cell");
        append_cell(&mut body, fi, cell, schema, plan)?;
    }
    Ok(body)
}

fn append_cell(
    body: &mut Vec<u8>,
    fi: usize,
    cell: &CompactCell,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Result<()> {
    let w = if plan.narrow {
        schema.cell_width(fi)
    } else {
        8
    };
    match cell {
        CompactCell::I64(v) => {
            while body.len() % w as usize != 0 {
                body.push(0);
            }
            body.extend_from_slice(&encode_int_cell(*v, w)?);
        }
        CompactCell::F64(v) => {
            while body.len() % w as usize != 0 {
                body.push(0);
            }
            body.extend_from_slice(&encode_f64_cell(*v, w)?);
        }
        CompactCell::Bool(v) if !plan.packed_bools => {
            while body.len() % 8 != 0 {
                body.push(0);
            }
            body.push(if *v { 1 } else { 0 });
            body.extend_from_slice(&[0u8; 7]);
        }
        CompactCell::Bool(_) => {}
        CompactCell::Time(v) => {
            while body.len() % 8 != 0 {
                body.push(0);
            }
            body.extend_from_slice(&v.to_le_bytes());
        }
        CompactCell::Str(s) if schema.is_promoted(fi) => {
            while body.len() % 2 != 0 {
                body.push(0);
            }
            let idx = schema
                .value_pool
                .iter()
                .position(|v| v == s)
                .ok_or(NxsError::DictMismatch)? as u16;
            body.extend_from_slice(&idx.to_le_bytes());
        }
        CompactCell::Str(s) => {
            let bytes = s.as_bytes();
            let prefix = schema.str_len_prefix(fi);
            if prefix == 2 {
                if bytes.len() > u16::MAX as usize {
                    return Err(NxsError::ValueOutOfRange);
                }
                body.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            } else {
                body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            }
            body.extend_from_slice(bytes);
            let pad = (8 - (prefix + bytes.len()) % 8) % 8;
            for _ in 0..pad {
                body.push(0);
            }
        }
        CompactCell::Keyword(idx) => {
            while body.len() % 2 != 0 {
                body.push(0);
            }
            body.extend_from_slice(&idx.to_le_bytes());
        }
        CompactCell::Null => {}
    }
    Ok(())
}

fn build_bitmask_bytes(present: &[bool]) -> Vec<u8> {
    let mut mask = Vec::new();
    let mut i = 0usize;
    while i < present.len() {
        let mut byte = 0u8;
        for bit in 0..7 {
            if i < present.len() && present[i] {
                byte |= 1 << bit;
            }
            i += 1;
        }
        let more = i < present.len();
        if more {
            byte |= 0x80;
        }
        mask.push(byte);
        if !more {
            break;
        }
    }
    if mask.is_empty() {
        mask.push(0);
    }
    mask
}

pub fn read_packed_bool(
    data: &[u8],
    obj_offset: usize,
    slot: usize,
    schema: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Option<bool> {
    let off = resolve_field_offset(data, obj_offset, slot, schema, plan, true)?;
    let bit_pos = plan.bool_slots.iter().position(|&s| s == slot)?;
    let byte = *data.get(off)?;
    Some((byte >> (bit_pos % 8)) & 1 == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_options_flags() {
        let o = CompactOptions::compact();
        assert_eq!(
            o.preamble_flags(),
            FLAG_DENSE_FRAMES
                | FLAG_PACKED_BOOLS
                | FLAG_NARROW_CELLS
                | FLAG_DELTA_TAIL
                | FLAG_DENSE_WIRE_REORDER
        );
        assert_eq!(o.version(), VERSION_V13);
    }

    #[test]
    fn infer_int_widths() {
        assert_eq!(infer_int_width(0, 200), 1);
        assert_eq!(infer_int_width(0, 100_000), 4);
    }

    #[test]
    fn delta_tail_roundtrip() {
        let offsets: Vec<u64> = (0..10).map(|i| 1000 + i * 50).collect();
        let tail_ptr = 5000u64;
        let bytes = build_delta_tail_index(&offsets, tail_ptr, 4).unwrap();
        let layout = parse_delta_tail_layout(&bytes, 0).unwrap();
        assert_eq!(layout.record_count, 10);
        for i in 0..10 {
            let off = delta_record_offset(&bytes, &layout, i).unwrap();
            assert_eq!(off as u64, offsets[i]);
        }
    }

    #[test]
    fn dense_field_offsets_match_encode() {
        let mut ext = ExtendedSchema::from_basic(
            vec!["id".into(), "score".into()],
            vec![SIGIL_INT, SIGIL_FLOAT],
        );
        ext.widths = vec![1, 8];
        let plan = RowCellPlan::new(&ext, CompactOptions::compact().preamble_flags());
        let obj = encode_compact_record(
            &[(0, CompactCell::I64(42)), (1, CompactCell::F64(3.5))],
            &ext,
            &CompactOptions::compact(),
        )
        .unwrap();
        let id_off = dense_field_offset(&obj, 0, 0, &ext, &plan)
            .unwrap()
            .unwrap();
        let score_off = dense_field_offset(&obj, 0, 1, &ext, &plan)
            .unwrap()
            .unwrap();
        assert_eq!(obj[id_off], 42);
        assert!((decode_f64_cell(&obj, score_off, 8).unwrap() - 3.5).abs() < 1e-9);
    }

    #[test]
    fn encode_single_record_bytes() {
        let mut ext = ExtendedSchema::from_basic(
            vec!["id".into(), "score".into()],
            vec![SIGIL_INT, SIGIL_FLOAT],
        );
        ext.widths = vec![1, 8];
        let cells = vec![(0u16, CompactCell::I64(42)), (1u16, CompactCell::F64(3.5))];
        let opts = CompactOptions::compact();
        let plan = RowCellPlan::new(&ext, opts.preamble_flags());
        let obj = encode_compact_record(&cells, &ext, &opts).unwrap();
        assert_eq!(obj.len(), 18);
        let id_off = dense_field_offset(&obj, 0, 0, &ext, &plan)
            .unwrap()
            .unwrap();
        let score_off = dense_field_offset(&obj, 0, 1, &ext, &plan)
            .unwrap()
            .unwrap();
        assert_eq!(obj[id_off], 42);
        assert_eq!(obj[score_off + 7], 0x40);
    }

    #[test]
    fn dense_five_field_offsets_match_encode() {
        let mut ext = ExtendedSchema::from_basic(
            vec![
                "id".into(),
                "username".into(),
                "age".into(),
                "active".into(),
                "score".into(),
            ],
            vec![SIGIL_INT, SIGIL_STR, SIGIL_INT, SIGIL_BOOL, SIGIL_FLOAT],
        );
        ext.widths = vec![1, 0, 1, 0, 8];
        let opts = CompactOptions::compact();
        let plan = RowCellPlan::new(&ext, opts.preamble_flags());
        let cells = vec![
            (0u16, CompactCell::I64(7)),
            (1u16, CompactCell::Str("user_0007".into())),
            (2u16, CompactCell::I64(27)),
            (3u16, CompactCell::Bool(false)),
            (4u16, CompactCell::F64(3.5)),
        ];
        let obj = encode_compact_record(&cells, &ext, &opts).unwrap();
        let id_off = dense_field_offset(&obj, 0, 0, &ext, &plan)
            .unwrap()
            .unwrap();
        let user_off = dense_field_offset(&obj, 0, 1, &ext, &plan)
            .unwrap()
            .unwrap();
        let age_off = dense_field_offset(&obj, 0, 2, &ext, &plan)
            .unwrap()
            .unwrap();
        let active_off = dense_field_offset(&obj, 0, 3, &ext, &plan)
            .unwrap()
            .unwrap();
        let score_off = dense_field_offset(&obj, 0, 4, &ext, &plan)
            .unwrap()
            .unwrap();
        assert_eq!(decode_int_cell(&obj, id_off, 1).unwrap(), 7);
        let ulen = u32::from_le_bytes(obj[user_off..user_off + 4].try_into().unwrap()) as usize;
        assert_eq!(&obj[user_off + 4..user_off + 4 + ulen], b"user_0007");
        assert_eq!(
            decode_int_cell(&obj, age_off, 1).unwrap(),
            27,
            "age_off={age_off} byte={}",
            obj[age_off]
        );
        assert_eq!(read_packed_bool(&obj, 0, 3, &ext, &plan), Some(false));
        assert!((decode_f64_cell(&obj, score_off, 8).unwrap() - 3.5).abs() < 1e-9);
        let body_base = 9usize;
        // With FLAG_DENSE_WIRE_REORDER (default compact()): score first, then narrow fields.
        assert_eq!(score_off, body_base);
        assert_eq!(id_off, body_base + 8);
        assert_eq!(age_off, body_base + 9);
        assert_eq!(active_off, body_base + 10);
    }

    #[test]
    fn dense_wire_order_without_reorder_flag_uses_schema_order() {
        let mut ext = ExtendedSchema::from_basic(
            vec!["id".into(), "score".into()],
            vec![SIGIL_INT, SIGIL_FLOAT],
        );
        ext.widths = vec![1, 8];
        let mut opts = CompactOptions::compact();
        opts.dense_wire_reorder = false;
        use crate::consts::FLAG_SCHEMA_EMBEDDED;
        let flags = opts.preamble_flags() | FLAG_SCHEMA_EMBEDDED;
        let plan = RowCellPlan::new(&ext, flags);
        assert_eq!(plan.dense_wire_order(&ext), vec![0, 1]);
        let obj = encode_compact_record(
            &[(0, CompactCell::I64(42)), (1, CompactCell::F64(3.5))],
            &ext,
            &opts,
        )
        .unwrap();
        assert_eq!(obj.len(), 25);
    }

    #[test]
    fn packed_bool_word_is_one_byte_for_single_bool_field() {
        let ext = ExtendedSchema::from_basic(
            vec!["id".into(), "active".into()],
            vec![SIGIL_INT, SIGIL_BOOL],
        );
        let opts = CompactOptions::compact();
        let plan = RowCellPlan::new(&ext, opts.preamble_flags());
        assert_eq!(plan.bool_word_bytes(), 1);
        let obj = encode_compact_record(
            &[(0, CompactCell::I64(1)), (1, CompactCell::Bool(true))],
            &ext,
            &opts,
        )
        .unwrap();
        let active_off = dense_field_offset(&obj, 0, 1, &ext, &plan)
            .unwrap()
            .unwrap();
        assert_eq!(obj[active_off], 1);
        assert_eq!(read_packed_bool(&obj, 0, 1, &ext, &plan), Some(true));
    }

    #[test]
    fn extended_schema_bytes_roundtrip() {
        let mut ext = ExtendedSchema::from_basic(
            vec![
                "id".into(),
                "username".into(),
                "age".into(),
                "active".into(),
                "score".into(),
            ],
            vec![SIGIL_INT, SIGIL_STR, SIGIL_INT, SIGIL_BOOL, SIGIL_FLOAT],
        );
        ext.widths = vec![1, 0, 1, 0, 8];
        let flags = CompactOptions::compact().preamble_flags();
        let bytes = build_extended_schema(&ext, flags);
        let (parsed, _) = parse_extended_schema(&bytes, 0, flags).unwrap();
        assert_eq!(parsed.sigils, ext.sigils);
        assert_eq!(parsed.widths, ext.widths);
        assert_eq!(parsed.field_attrs, ext.field_attrs);
        assert_eq!(parsed.value_pool, ext.value_pool);
    }

    #[test]
    fn keyword_promotion_skips_unique_per_record_columns() {
        let sigils = vec![SIGIL_STR, SIGIL_STR];
        let vals = vec![
            (0..100).map(|i| format!("user_{i}")).collect(),
            (0..100).map(|i| format!("user{i}@example.com")).collect(),
        ];
        let (attrs, pool) = scan_keyword_promotion(&sigils, &vals, 0.5, 32);
        assert_eq!(attrs, vec![0, 0]);
        assert!(pool.is_empty());
    }

    #[test]
    fn keyword_promotion_accepts_low_cardinality() {
        let sigils = vec![SIGIL_STR];
        let vals = vec![(0..100)
            .map(|i| {
                if i % 3 == 0 {
                    "INFO"
                } else if i % 3 == 1 {
                    "WARN"
                } else {
                    "ERROR"
                }
                .to_string()
            })
            .collect()];
        let (attrs, pool) = scan_keyword_promotion(&sigils, &vals, 0.5, 32);
        assert!(attrs[0] & FIELD_ATTR_PROMOTED != 0);
        assert_eq!(pool.len(), 3);
    }

    #[test]
    fn writer_promoted_string_dense_offsets() {
        use crate::writer::{NxsWriter, Schema, Slot};

        const SLOTS: &[&str] = &["id", "level", "age", "active", "score"];
        let schema = Schema::new(SLOTS);
        let opts = CompactOptions::compact();
        let mut w = NxsWriter::with_compact(&schema, Some(opts.clone()));
        for i in 0..50usize {
            w.begin_object();
            w.write_i64(Slot(0), i as i64);
            let level = match i % 3 {
                0 => "INFO",
                1 => "WARN",
                _ => "ERROR",
            };
            w.write_str(Slot(1), level);
            w.write_i64(Slot(2), (20 + (i % 50)) as i64);
            w.write_bool(Slot(3), i % 2 == 0);
            w.write_f64(Slot(4), i as f64 * 0.5);
            w.end_object();
        }
        let bytes = w.finish();
        let flags = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        let ext = parse_extended_schema(&bytes, 32, flags).unwrap().0;
        assert!(ext.is_promoted(1));
        let plan = RowCellPlan::new(&ext, opts.preamble_flags());
        let footer_tail =
            u64::from_le_bytes(bytes[bytes.len() - 12..bytes.len() - 4].try_into().unwrap())
                as usize;
        let layout = parse_delta_tail_layout(&bytes, footer_tail).unwrap();
        let obj_off = delta_record_offset(&bytes, &layout, 7).unwrap();
        let age_off = dense_field_offset(&bytes, obj_off, 2, &ext, &plan)
            .unwrap()
            .unwrap();
        assert_eq!(
            decode_int_cell(&bytes, age_off, ext.cell_width(2)).unwrap(),
            27
        );
    }

    #[test]
    fn reject_v13_flags_on_v12_reader() {
        assert!(validate_reader_flags(FLAG_DENSE_FRAMES, false).is_err());
        assert!(validate_reader_flags(0, false).is_ok());
    }
}
