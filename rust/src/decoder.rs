/// Minimal .nxb decoder — reads the preamble and walks the root object,
/// returning a flat list of (key_index, value_bytes) for inspection.
use crate::consts::{
    FLAG_COLUMNAR, FLAG_PAX, MAGIC_FILE, MAGIC_FOOTER, MAGIC_LIST, MAGIC_OBJ, SIGIL_BINARY,
    SIGIL_BOOL, SIGIL_FLOAT, SIGIL_INT, SIGIL_LINK, SIGIL_NULL, SIGIL_STR, SIGIL_TIME,
};
use crate::error::{NxsError, Result};

fn validate_preamble_flags(flags: u16) -> Result<()> {
    if flags & FLAG_COLUMNAR != 0 && flags & FLAG_PAX != 0 {
        return Err(NxsError::InvalidFlags);
    }
    Ok(())
}

fn footer_size(flags: u16) -> usize {
    if flags & FLAG_PAX != 0 {
        28
    } else if flags & FLAG_COLUMNAR != 0 {
        20
    } else {
        12
    }
}

pub struct DecodedFile {
    pub version: u16,
    pub flags: u16,
    pub dict_hash: u64,
    pub tail_ptr: u64,
    pub keys: Vec<String>,
    pub key_sigils: Vec<u8>,
    pub root_fields: Vec<(String, DecodedValue)>,
    pub record_count: usize,
    pub tail_start: usize,
    pub data_sector_start: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Time(i64),
    Binary(Vec<u8>),
    Null,
    List(Vec<DecodedValue>),
    Object(Vec<(String, DecodedValue)>),
    Raw(Vec<u8>),
}

fn murmur3_64(data: &[u8]) -> u64 {
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

pub fn decode(data: &[u8]) -> Result<DecodedFile> {
    if data.len() < 32 {
        return Err(NxsError::OutOfBounds);
    }

    let magic = u32::from_le_bytes(data[0..4].try_into().map_err(|_| NxsError::OutOfBounds)?);
    if magic != MAGIC_FILE {
        return Err(NxsError::BadMagic);
    }

    let footer_magic = u32::from_le_bytes(
        data[data.len() - 4..]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    if footer_magic != MAGIC_FOOTER {
        return Err(NxsError::BadMagic);
    }

    let version = u16::from_le_bytes(data[4..6].try_into().map_err(|_| NxsError::OutOfBounds)?);
    let flags = u16::from_le_bytes(data[6..8].try_into().map_err(|_| NxsError::OutOfBounds)?);
    validate_preamble_flags(flags)?;
    let dict_hash = u64::from_le_bytes(data[8..16].try_into().map_err(|_| NxsError::OutOfBounds)?);
    let preamble_tail =
        u64::from_le_bytes(data[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?);
    if flags & FLAG_COLUMNAR != 0 && preamble_tail == 0 {
        return Err(NxsError::IncompatibleFlags);
    }

    let mut tail_ptr = preamble_tail;
    if flags & (FLAG_COLUMNAR | FLAG_PAX) != 0 {
        let footer = footer_size(flags);
        if data.len() < footer {
            return Err(NxsError::OutOfBounds);
        }
        let fo = data.len() - footer;
        tail_ptr = u64::from_le_bytes(
            data[fo..fo + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        );
    } else if tail_ptr == 0 {
        if data.len() < 44 {
            return Err(NxsError::OutOfBounds);
        }
        tail_ptr = u64::from_le_bytes(
            data[data.len() - 12..data.len() - 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        );
    }

    let schema_embedded = flags & 0x0002 != 0;
    let mut pos = 32usize;
    let mut keys: Vec<String> = Vec::new();
    let mut key_sigils: Vec<u8> = Vec::new();

    if schema_embedded && pos + 2 <= data.len() {
        let schema_start = pos;
        let key_count = u16::from_le_bytes(
            data[pos..pos + 2]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        if key_count > 256 {
            return Err(NxsError::OutOfBounds); // spec max is 256 keys
        }
        pos += 2;
        // TypeManifest
        let end = pos.checked_add(key_count).ok_or(NxsError::OutOfBounds)?;
        if end > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        key_sigils = data[pos..end].to_vec();
        pos = end;
        // StringPool — cap key name length to prevent OOM
        for _ in 0..key_count {
            let start = pos;
            while pos < data.len() && data[pos] != 0 {
                if pos - start > 256 {
                    return Err(NxsError::OutOfBounds);
                }
                pos += 1;
            }
            let name = String::from_utf8_lossy(&data[start..pos]).to_string();
            keys.push(name);
            if pos >= data.len() {
                return Err(NxsError::OutOfBounds);
            }
            pos += 1; // skip null terminator
        }
        // align to 8 — guard against pos already past end
        if pos > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        while pos % 8 != 0 {
            pos += 1;
        }
        if pos > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let schema_end = pos;

        // Validate DictHash
        let computed = murmur3_64(&data[schema_start..schema_end]);
        if computed != dict_hash {
            return Err(NxsError::DictMismatch);
        }
    }

    let data_sector_start = pos;

    if flags & FLAG_PAX != 0 {
        const MAGIC_PAGE: u32 = 0x4E58_5350;
        if pos + 4 > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        if u32::from_le_bytes(
            data[pos..pos + 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) != MAGIC_PAGE
        {
            return Err(NxsError::InvalidPageMagic);
        }
    }

    // Decode root object (first record) — row layout only
    let root_fields = if flags & FLAG_PAX == 0 && pos < data.len() {
        decode_object(data, pos, &keys, &key_sigils).unwrap_or_default()
    } else {
        Vec::new()
    };

    let (record_count, tail_start) = if flags & FLAG_COLUMNAR != 0 {
        let footer = footer_size(flags);
        let fo = data.len() - footer;
        let rc = u64::from_le_bytes(
            data[fo + 8..fo + 16]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        (rc, tail_ptr as usize)
    } else if flags & FLAG_PAX != 0 {
        let footer = footer_size(flags);
        let fo = data.len() - footer;
        let rc = u64::from_le_bytes(
            data[fo + 8..fo + 16]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        (rc, tail_ptr as usize)
    } else {
        let tail_offset = if tail_ptr as usize as u64 == tail_ptr {
            tail_ptr as usize
        } else {
            return Err(NxsError::OutOfBounds);
        };
        let rc = if tail_offset.saturating_add(4) <= data.len() {
            u32::from_le_bytes(
                data[tail_offset..tail_offset + 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize
        } else {
            0
        };
        (rc, tail_offset.saturating_add(4))
    };

    Ok(DecodedFile {
        version,
        flags,
        dict_hash,
        tail_ptr,
        keys,
        key_sigils,
        root_fields,
        record_count,
        tail_start,
        data_sector_start,
    })
}

/// Decode a single record at the given absolute offset.
pub fn decode_record_at(
    data: &[u8],
    offset: usize,
    keys: &[String],
    sigils: &[u8],
) -> Result<Vec<(String, DecodedValue)>> {
    decode_object(data, offset, keys, sigils)
}

fn decode_object(
    data: &[u8],
    offset: usize,
    keys: &[String],
    sigils: &[u8],
) -> Result<Vec<(String, DecodedValue)>> {
    let mut pos = offset;

    if pos + 8 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let magic = u32::from_le_bytes(
        data[pos..pos + 4]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    if magic != MAGIC_OBJ {
        return Err(NxsError::BadMagic);
    }
    pos += 4;

    let _obj_len = u32::from_le_bytes(
        data[pos..pos + 4]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    pos += 4;

    // Read LEB128 bitmask — cap at 512 bits (74 bytes) to prevent OOM
    let mut present_bits: Vec<bool> = Vec::new();
    loop {
        if pos >= data.len() {
            return Err(NxsError::OutOfBounds);
        }
        if present_bits.len() >= 512 {
            return Err(NxsError::OutOfBounds);
        }
        let byte = data[pos];
        pos += 1;
        for bit in 0..7 {
            present_bits.push((byte >> bit) & 1 == 1);
        }
        if byte & 0x80 == 0 {
            break;
        }
    }

    // Count present fields — cap to prevent OOM from malformed inputs
    let present_count = present_bits.iter().filter(|&&b| b).count();
    if present_count > 512 {
        return Err(NxsError::OutOfBounds);
    }

    // Read offset table (u16 each)
    let mut offsets: Vec<usize> = Vec::new();
    for _ in 0..present_count {
        if pos + 2 > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let off = u16::from_le_bytes(
            data[pos..pos + 2]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        offsets.push(offset + off);
        pos += 2;
    }

    // Map each present bit to its key and decode its value using sigil type info
    let mut fields = Vec::new();
    let mut offset_idx = 0;
    for (bit_idx, &present) in present_bits.iter().enumerate() {
        if !present {
            continue;
        }
        let key_name = keys
            .get(bit_idx)
            .cloned()
            .unwrap_or_else(|| format!("key_{bit_idx}"));
        let sigil = sigils.get(bit_idx).copied().unwrap_or(0);
        let val_offset = offsets[offset_idx];
        offset_idx += 1;

        let value = decode_value_at(data, val_offset, sigil, keys, sigils)?;
        fields.push((key_name, value));
    }

    Ok(fields)
}

/// Maximum number of `&` link hops before returning `CircularLink` (spec SHOULD limit).
const MAX_LINK_DEPTH: usize = 16;

fn decode_value_at(
    data: &[u8],
    offset: usize,
    sigil: u8,
    keys: &[String],
    sigils: &[u8],
) -> Result<DecodedValue> {
    decode_value_at_depth(data, offset, sigil, keys, sigils, 0)
}

fn decode_value_at_depth(
    data: &[u8],
    offset: usize,
    sigil: u8,
    keys: &[String],
    sigils: &[u8],
    depth: usize,
) -> Result<DecodedValue> {
    let _ = (keys, sigils); // used by recursive calls on nested objects
    if offset >= data.len() {
        return Err(NxsError::OutOfBounds);
    }

    // ── Link resolution ────────────────────────────────────────────────────────
    // A `&` field stores the absolute byte offset of the target value (u64, 8 bytes).
    // Follow the chain up to MAX_LINK_DEPTH hops; deeper chains are adversarial loops.
    if sigil == SIGIL_LINK {
        if depth >= MAX_LINK_DEPTH {
            return Err(NxsError::CircularLink);
        }
        if offset + 8 > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let target = u64::from_le_bytes(
            data[offset..offset + 8]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        // Detect immediate self-loops before recursing.
        if target == offset {
            return Err(NxsError::CircularLink);
        }
        // The sigil at the target is unknown here; peek at the magic bytes to
        // decide how to decode, then recurse with SIGIL_LINK so further hops
        // are also depth-checked.
        return decode_value_at_depth(data, target, SIGIL_LINK, keys, sigils, depth + 1);
    }

    // Check for nested object or list magic first
    if offset + 4 <= data.len() {
        let maybe_magic = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        );
        if maybe_magic == MAGIC_OBJ {
            // Nested objects in the compiler path use a locally-scoped key schema,
            // not the global one. Return Raw to avoid crashing with the wrong schema.
            return Ok(DecodedValue::Raw(
                data[offset..offset + 8.min(data.len() - offset)].to_vec(),
            ));
        }
        if maybe_magic == MAGIC_LIST {
            return decode_list(data, offset);
        }
    }

    // Null sigil
    if sigil == SIGIL_NULL {
        return Ok(DecodedValue::Null);
    }

    // Use sigil to decode the correct type
    match sigil {
        SIGIL_INT => {
            if offset + 8 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let v = i64::from_le_bytes(
                data[offset..offset + 8]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            Ok(DecodedValue::Int(v))
        }
        SIGIL_FLOAT => {
            if offset + 8 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let bits = u64::from_le_bytes(
                data[offset..offset + 8]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            Ok(DecodedValue::Float(f64::from_bits(bits)))
        }
        SIGIL_BOOL => Ok(DecodedValue::Bool(data[offset] != 0)),
        SIGIL_STR => {
            if offset + 4 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let len = u32::from_le_bytes(
                data[offset..offset + 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize;
            // Guard against garbage lengths (compiler uses SIGIL_STR generically)
            if len > 1024 * 1024 || offset + 4 + len > data.len() {
                // Treat as raw i64 — the field is not a string despite the sigil
                if offset + 8 <= data.len() {
                    let v = i64::from_le_bytes(
                        data[offset..offset + 8]
                            .try_into()
                            .map_err(|_| NxsError::OutOfBounds)?,
                    );
                    return Ok(DecodedValue::Int(v));
                }
                return Ok(DecodedValue::Raw(data[offset..].to_vec()));
            }
            let s = String::from_utf8_lossy(&data[offset + 4..offset + 4 + len]).to_string();
            Ok(DecodedValue::Str(s))
        }
        SIGIL_TIME => {
            if offset + 8 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let v = i64::from_le_bytes(
                data[offset..offset + 8]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            Ok(DecodedValue::Time(v))
        }
        SIGIL_BINARY => {
            if offset + 4 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let len = u32::from_le_bytes(
                data[offset..offset + 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize;
            if offset + 4 + len > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            Ok(DecodedValue::Binary(
                data[offset + 4..offset + 4 + len].to_vec(),
            ))
        }
        _ => {
            // Unknown sigil — return raw i64 as best-effort
            if offset + 8 <= data.len() {
                let v = i64::from_le_bytes(
                    data[offset..offset + 8]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                );
                Ok(DecodedValue::Int(v))
            } else {
                Ok(DecodedValue::Raw(data[offset..].to_vec()))
            }
        }
    }
}

fn decode_list(data: &[u8], offset: usize) -> Result<DecodedValue> {
    if offset + 16 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let magic = u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    );
    if magic != MAGIC_LIST {
        return Err(NxsError::BadMagic);
    }
    let elem_sigil = data[offset + 8];
    let elem_count = u32::from_le_bytes(
        data[offset + 9..offset + 13]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    let data_start = offset + 16;
    // Reject impossible counts before allocating — elem slots are 8 bytes each.
    let max_elems = (data.len().saturating_sub(data_start)) / 8;
    if elem_count > max_elems {
        return Err(NxsError::OutOfBounds);
    }
    let mut items = Vec::with_capacity(elem_count);
    for i in 0..elem_count {
        let elem_off = data_start + i * 8;
        if elem_off + 8 > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let v = match elem_sigil {
            SIGIL_INT => {
                let v = i64::from_le_bytes(
                    data[elem_off..elem_off + 8]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                );
                DecodedValue::Int(v)
            }
            SIGIL_FLOAT => {
                let bits = u64::from_le_bytes(
                    data[elem_off..elem_off + 8]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                );
                DecodedValue::Float(f64::from_bits(bits))
            }
            _ => DecodedValue::Raw(data[elem_off..elem_off + 8].to_vec()),
        };
        items.push(v);
    }
    Ok(DecodedValue::List(items))
}
