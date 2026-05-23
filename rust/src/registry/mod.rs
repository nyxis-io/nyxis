pub mod client;
pub mod preamble;

pub mod pb {
    tonic::include_proto!("nyxis.registry.v1");
}

/// Parse `0x`-prefixed or plain 16-hex-digit DictHash to 8 little-endian bytes.
pub fn parse_dict_hash_hex(s: &str) -> Result<[u8; 8], String> {
    let hex = s.strip_prefix("0x").unwrap_or(s);
    if hex.len() != 16 {
        return Err(format!(
            "dict hash must be 16 hex digits (got {} chars)",
            hex.len()
        ));
    }
    let value = u64::from_str_radix(hex, 16).map_err(|e| format!("invalid dict hash: {e}"))?;
    Ok(value.to_le_bytes())
}

pub fn format_dict_hash(bytes: &[u8; 8]) -> String {
    let v = u64::from_le_bytes(*bytes);
    format!("0x{v:016x}")
}
