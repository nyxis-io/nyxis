//! Incremental reader for streamable `.nxb` files (`TailPtr = 0` until seal).
//!
//! Polls a growing buffer for complete NYXO top-level objects without requiring
//! `MagicFooter` or a tail-index. v1.3 compact dense/sparse frames are forward-
//! decodable the same way as v1.2 streamable rows.

use crate::compact::{
    decode_f64_cell, decode_int_cell, parse_extended_schema, read_packed_bool, read_str_cell_len,
    resolve_field_offset, ExtendedSchema, RowCellPlan,
};
use crate::consts::{FLAG_DENSE_FRAMES, FLAG_SCHEMA_EMBEDDED, FLAG_V13_COMPACT_MASK};
use crate::error::{NxsError, Result};
use crate::query::{parse_schema, resolve_slot};

const MAGIC_FILE: u32 = 0x4E59_5842;
const MAGIC_OBJ: u32 = 0x4E59_584F;
const MAGIC_FOOTER: u32 = 0x2153_584E;

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
    flags: u16,
    keys: Vec<String>,
    #[allow(dead_code)]
    key_sigils: Vec<u8>,
    key_index: std::collections::HashMap<String, usize>,
    ext_schema: Option<ExtendedSchema>,
    cell_plan: Option<RowCellPlan>,
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

        let v13 = flags & FLAG_V13_COMPACT_MASK != 0;
        let (keys, key_sigils, ext_schema, cell_plan, data_start) =
            if flags & FLAG_SCHEMA_EMBEDDED != 0 {
                if v13 {
                    let (schema, end) = parse_extended_schema(data, 32, flags)?;
                    if end > data.len() {
                        return Err(NxsError::OutOfBounds);
                    }
                    let keys = schema.keys.clone();
                    let key_sigils = schema.sigils.clone();
                    let plan = RowCellPlan::new(&schema, flags);
                    (keys, key_sigils, Some(schema), Some(plan), end)
                } else {
                    let (keys, key_sigils, data_start) = parse_schema(data, 32)?;
                    (keys, key_sigils, None, None, data_start)
                }
            } else {
                (vec![], vec![], None, None, 32)
            };

        let key_index: std::collections::HashMap<String, usize> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();

        let sealed = if data.len() < 16 {
            false
        } else {
            let tail_magic = u32::from_le_bytes(
                data[data.len() - 4..]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            );
            if tail_magic != MAGIC_FOOTER {
                false
            } else {
                let footer_tail_ptr = u64::from_le_bytes(
                    data[data.len() - 12..data.len() - 4]
                        .try_into()
                        .map_err(|_| NxsError::OutOfBounds)?,
                );
                let tp = footer_tail_ptr as usize;
                tp > 0 && tp < data.len() && data.len() - tp >= 16
            }
        };

        Ok(Self {
            data,
            flags,
            keys,
            key_sigils,
            key_index,
            ext_schema,
            cell_plan,
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

    fn resolve_field_at(&self, obj_offset: usize, slot: usize) -> Option<usize> {
        if let (Some(ext), Some(plan)) = (&self.ext_schema, &self.cell_plan) {
            resolve_field_offset(
                self.data,
                obj_offset,
                slot,
                ext,
                plan,
                self.flags & FLAG_DENSE_FRAMES != 0,
            )
        } else {
            resolve_slot(self.data, obj_offset, slot)
        }
    }

    /// Read `i64` field at a top-level object offset.
    pub fn get_i64_at(&self, obj_offset: usize, key: &str) -> Option<i64> {
        let slot = *self.key_index.get(key)?;
        let off = self.resolve_field_at(obj_offset, slot)?;
        if let Some(ext) = &self.ext_schema {
            return decode_int_cell(self.data, off, ext.cell_width(slot)).ok();
        }
        Some(i64::from_le_bytes(
            self.data.get(off..off + 8)?.try_into().ok()?,
        ))
    }

    /// Read `f64` field at a top-level object offset.
    pub fn get_f64_at(&self, obj_offset: usize, key: &str) -> Option<f64> {
        let slot = *self.key_index.get(key)?;
        let off = self.resolve_field_at(obj_offset, slot)?;
        if let Some(ext) = &self.ext_schema {
            return decode_f64_cell(self.data, off, ext.cell_width(slot)).ok();
        }
        Some(f64::from_le_bytes(
            self.data.get(off..off + 8)?.try_into().ok()?,
        ))
    }

    /// Read `bool` field at a top-level object offset.
    pub fn get_bool_at(&self, obj_offset: usize, key: &str) -> Option<bool> {
        let slot = *self.key_index.get(key)?;
        if let (Some(ext), Some(plan)) = (&self.ext_schema, &self.cell_plan) {
            if plan.packed_bools && plan.bool_slots.contains(&slot) {
                return read_packed_bool(self.data, obj_offset, slot, ext, plan);
            }
        }
        let off = self.resolve_field_at(obj_offset, slot)?;
        Some(self.data.get(off)? != &0)
    }

    /// Read `&str` field at a top-level object offset.
    pub fn get_str_at(&self, obj_offset: usize, key: &str) -> Option<&str> {
        let slot = *self.key_index.get(key)?;
        let off = self.resolve_field_at(obj_offset, slot)?;
        if let Some(ext) = &self.ext_schema {
            if ext.is_promoted(slot) {
                let idx = u16::from_le_bytes(self.data.get(off..off + 2)?.try_into().ok()?);
                return ext.value_pool.get(idx as usize).map(|s| s.as_str());
            }
            let prefix = ext.str_len_prefix(slot);
            let len = read_str_cell_len(self.data, off, prefix).ok()?;
            let bytes = self.data.get(off + prefix..off + prefix + len)?;
            return std::str::from_utf8(bytes).ok();
        }
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
    use crate::compact::CompactOptions;
    use crate::layout::{finish_row, Cell, RecordRow};
    use crate::query::Reader;
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

    #[test]
    fn compact_forward_decode_before_footer() {
        const LEVELS: [&str; 4] = ["INFO", "WARN", "ERROR", "DEBUG"];
        const VISIBLE: usize = 8;
        let keys = vec!["id".into(), "level".into(), "msg".into()];
        let rows: Vec<RecordRow> = (0..20)
            .map(|i| RecordRow {
                cells: vec![
                    Cell::I64(i as i64),
                    Cell::Str(LEVELS[i % 4].to_string()),
                    Cell::Str(format!("event {i}")),
                ],
            })
            .collect();
        let full = finish_row(&keys, &rows, Some(&CompactOptions::compact())).unwrap();
        let reader = Reader::new(&full).unwrap();
        let last = VISIBLE - 1;
        let off = reader.record(last).unwrap().object_offset().unwrap();
        let len = u32::from_le_bytes(full[off + 4..off + 8].try_into().unwrap()) as usize;
        let mut partial = full[..off + len].to_vec();
        partial[16..24].copy_from_slice(&0u64.to_le_bytes());

        let mut sr = StreamReader::open(&partial).unwrap();
        assert!(!sr.is_sealed());
        let mut count = 0usize;
        while let Some(obj_off) = sr.poll_next_offset() {
            assert_eq!(sr.get_i64_at(obj_off, "id"), Some(count as i64));
            assert_eq!(sr.get_str_at(obj_off, "level"), Some(LEVELS[count % 4]));
            assert_eq!(
                sr.get_str_at(obj_off, "msg"),
                Some(format!("event {count}").as_str())
            );
            count += 1;
        }
        assert_eq!(count, VISIBLE);
    }
}
