//! Incremental reader for streamable `.nxb` files (`TailPtr = 0` until seal).
//!
//! Polls a growing buffer for complete NYXO top-level objects without requiring
//! `MagicFooter` or a tail-index.

use crate::error::{NxsError, Result};
use crate::query::{parse_schema, resolve_slot};

const MAGIC_FILE: u32 = 0x4E59_5842;
const MAGIC_OBJ: u32 = 0x4E59_584F;
const MAGIC_FOOTER: u32 = 0x2153_584E;
const FLAG_SCHEMA_EMBEDDED: u16 = 0x0002;

/// End offset (exclusive) of the NYXO object at `off`, if fully present in `data`.
pub fn complete_nyxo_end(data: &[u8], off: usize) -> Option<usize> {
    if off + 8 > data.len() {
        return None;
    }
    let magic = u32::from_le_bytes(data.get(off..off + 4)?.try_into().ok()?);
    if magic != MAGIC_OBJ {
        return None;
    }
    let obj_len = u32::from_le_bytes(data.get(off + 4..off + 8)?.try_into().ok()?);
    if obj_len < 8 {
        return None;
    }
    let end = off.checked_add(obj_len as usize)?;
    if end <= data.len() {
        Some(end)
    } else {
        None
    }
}

/// Reader for an unsealed `.nxb` stream (growing file or pipe).
pub struct StreamReader<'a> {
    data: &'a [u8],
    keys: Vec<String>,
    #[allow(dead_code)]
    key_sigils: Vec<u8>,
    key_index: std::collections::HashMap<String, usize>,
    data_start: usize,
    cursor: usize,
    sealed: bool,
}

impl<'a> StreamReader<'a> {
    pub fn open(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(NxsError::OutOfBounds);
        }
        if u32::from_le_bytes(data[0..4].try_into().map_err(|_| NxsError::OutOfBounds)?)
            != MAGIC_FILE
        {
            return Err(NxsError::BadMagic);
        }
        let flags = u16::from_le_bytes(data[6..8].try_into().map_err(|_| NxsError::OutOfBounds)?);
        let tail_ptr =
            u64::from_le_bytes(data[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?);
        if tail_ptr != 0 {
            return Err(NxsError::ParseError(
                "not a stream file: TailPtr already set".into(),
            ));
        }

        let (keys, key_sigils, data_start) = if flags & FLAG_SCHEMA_EMBEDDED != 0 {
            parse_schema(data, 32)?
        } else {
            (vec![], vec![], 32)
        };

        let key_index: std::collections::HashMap<String, usize> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();

        let sealed = data.len() >= 12
            && u32::from_le_bytes(
                data[data.len() - 4..]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) == MAGIC_FOOTER;

        Ok(Self {
            data,
            keys,
            key_sigils,
            key_index,
            data_start,
            cursor: data_start,
            sealed,
        })
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn data_start(&self) -> usize {
        self.data_start
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// True when the first top-level NYXO object is fully available.
    pub fn has_first_complete(&self) -> bool {
        complete_nyxo_end(self.data, self.data_start).is_some()
    }

    /// Read `i64` field at a top-level object offset.
    pub fn get_i64_at(&self, obj_offset: usize, key: &str) -> Option<i64> {
        let slot = *self.key_index.get(key)?;
        let off = resolve_slot(self.data, obj_offset, slot)?;
        Some(i64::from_le_bytes(
            self.data.get(off..off + 8)?.try_into().ok()?,
        ))
    }

    /// Read `&str` field at a top-level object offset.
    pub fn get_str_at(&self, obj_offset: usize, key: &str) -> Option<&'a str> {
        let slot = *self.key_index.get(key)?;
        let off = resolve_slot(self.data, obj_offset, slot)?;
        let len = u32::from_le_bytes(self.data.get(off..off + 4)?.try_into().ok()?) as usize;
        let bytes = self.data.get(off + 4..off + 4 + len)?;
        std::str::from_utf8(bytes).ok()
    }

    /// Next complete top-level object offset, if available.
    pub fn poll_next_offset(&mut self) -> Option<usize> {
        let end = complete_nyxo_end(self.data, self.cursor)?;
        let offset = self.cursor;
        self.cursor = end;
        Some(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::{
        write_stream_file_footer, write_stream_file_header, NxsWriter, Schema, Slot,
    };
    use std::io::Write;

    #[test]
    fn incremental_first_record_before_seal() {
        let keys = ["id", "username", "score", "active"];
        let schema = Schema::new(&keys);
        let mut w = NxsWriter::new(&schema);
        w.begin_object();
        w.write_i64(Slot(0), 1);
        w.write_str(Slot(1), "alice");
        w.write_f64(Slot(2), 9.5);
        w.write_bool(Slot(3), true);
        w.end_object();

        let mut file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        {
            let mut f = file.as_file_mut();
            let data_start = write_stream_file_header(&mut f, &schema).unwrap();
            let mut flushed = 0usize;
            w.write_data_sector_since(&mut f, flushed).unwrap();
            flushed = w.data_sector_len();
            f.flush().unwrap();

            let partial = std::fs::read(&path).unwrap();
            let sr = StreamReader::open(&partial).unwrap();
            assert!(sr.has_first_complete());
            assert_eq!(sr.get_i64_at(sr.data_start(), "id"), Some(1));
            assert_eq!(sr.get_str_at(sr.data_start(), "username"), Some("alice"));

            w.begin_object();
            w.write_i64(Slot(0), 2);
            w.write_str(Slot(1), "bob");
            w.write_f64(Slot(2), 1.0);
            w.write_bool(Slot(3), false);
            w.end_object();
            w.write_data_sector_since(&mut f, flushed).unwrap();
            let partial = std::fs::read(&path).unwrap();
            let mut sr = StreamReader::open(&partial).unwrap();
            let o1 = sr.poll_next_offset().unwrap();
            assert_eq!(sr.get_i64_at(o1, "id"), Some(1));
            let o2 = sr.poll_next_offset().unwrap();
            assert_eq!(sr.get_i64_at(o2, "id"), Some(2));

            let off1 = data_start;
            let off2 = data_start + w.record_offsets()[0] as u64;
            write_stream_file_footer(f, data_start, &[off1, off2]).unwrap();
            let partial = std::fs::read(&path).unwrap();
            assert!(StreamReader::open(&partial).unwrap().is_sealed());
        }
    }
}
