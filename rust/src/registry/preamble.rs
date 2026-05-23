//! Minimal NYXB preamble extraction for registry push (open-core; no extensions dep).

const MAGIC_FILE: u32 = 0x4E59_5842; // NYXB
const FLAG_SCHEMA_EMBEDDED: u16 = 0x0002;

pub struct PreambleInfo {
    pub dict_hash: u64,
    pub schema_bytes: Vec<u8>,
}

/// Extract DictHash and embedded schema bytes from an `.nxb` preamble.
pub fn extract_preamble(nxb: &[u8]) -> Result<PreambleInfo, Vec<String>> {
    let mut errors = Vec::new();
    if nxb.len() < 32 {
        errors.push("PAYLOAD_TOO_SHORT".into());
        return Err(errors);
    }
    let magic = u32::from_le_bytes(nxb[0..4].try_into().unwrap());
    if magic != MAGIC_FILE {
        errors.push("BAD_MAGIC".into());
        return Err(errors);
    }
    let flags = u16::from_le_bytes(nxb[6..8].try_into().unwrap());
    if flags & FLAG_SCHEMA_EMBEDDED == 0 {
        errors.push("SCHEMA_NOT_EMBEDDED".into());
        return Err(errors);
    }
    let dict_hash = u64::from_le_bytes(nxb[8..16].try_into().unwrap());
    let mut pos = 32usize;
    if pos + 2 > nxb.len() {
        errors.push("SCHEMA_TRUNCATED".into());
        return Err(errors);
    }
    let schema_start = pos;
    let key_count = u16::from_le_bytes(nxb[pos..pos + 2].try_into().unwrap()) as usize;
    if key_count > 256 {
        errors.push("SCHEMA_KEY_COUNT".into());
        return Err(errors);
    }
    pos += 2 + key_count;
    for _ in 0..key_count {
        while pos < nxb.len() && nxb[pos] != 0 {
            pos += 1;
        }
        if pos >= nxb.len() {
            errors.push("SCHEMA_TRUNCATED".into());
            return Err(errors);
        }
        pos += 1;
    }
    while pos % 8 != 0 {
        if pos >= nxb.len() {
            errors.push("SCHEMA_ALIGN".into());
            return Err(errors);
        }
        pos += 1;
    }
    let schema_bytes = nxb[schema_start..pos].to_vec();
    Ok(PreambleInfo {
        dict_hash,
        schema_bytes,
    })
}
