//! Zero-allocation query engine for .nxb files.
//!
//! # Usage
//!
//! ```no_run
//! use nxs::query::{Reader, And, eq, gt};
//!
//! let data = std::fs::read("data.nxb").unwrap();
//! let reader = Reader::new(&data).unwrap();
//!
//! for record in reader.where_pred(And(eq("active", true), gt("score", 80.0f64))) {
//!     println!("{:?}", record.get_str("username"));
//! }
//! ```

use crate::error::{NxsError, Result};

// ── Format constants (local; avoids re-exporting decoder internals) ───────────

const MAGIC_FILE: u32 = 0x4E59_5842; // NYXB
const MAGIC_OBJ: u32 = 0x4E59_584F; // NYXO
const MAGIC_FOOTER: u32 = 0x2153_584E;
const FLAG_SCHEMA_EMBEDDED: u16 = 0x0002;

// ── Reader ────────────────────────────────────────────────────────────────────

/// A zero-copy reader for a .nxb buffer.
/// Parses the preamble and schema on construction; record data is accessed lazily.
pub struct Reader<'a> {
    data: &'a [u8],
    keys: Vec<String>,
    key_sigils: Vec<u8>,
    key_index: std::collections::HashMap<String, usize>,
    record_count: usize,
    tail_start: usize,
}

impl<'a> Reader<'a> {
    /// Validate the file header and build the schema index.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 32 {
            return Err(NxsError::OutOfBounds);
        }
        if u32::from_le_bytes(data[0..4].try_into().map_err(|_| NxsError::OutOfBounds)?)
            != MAGIC_FILE
        {
            return Err(NxsError::BadMagic);
        }
        if u32::from_le_bytes(
            data[data.len() - 4..]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) != MAGIC_FOOTER
        {
            return Err(NxsError::BadMagic);
        }

        let flags = u16::from_le_bytes(data[6..8].try_into().map_err(|_| NxsError::OutOfBounds)?);
        let mut tail_ptr =
            u64::from_le_bytes(data[16..24].try_into().map_err(|_| NxsError::OutOfBounds)?)
                as usize;
        if tail_ptr == 0 {
            if data.len() < 44 {
                return Err(NxsError::OutOfBounds);
            }
            tail_ptr = u64::from_le_bytes(
                data[data.len() - 12..data.len() - 4]
                    .try_into()
                    .map_err(|_| NxsError::OutOfBounds)?,
            ) as usize;
        }

        let (keys, key_sigils, _schema_end) = if flags & FLAG_SCHEMA_EMBEDDED != 0 {
            parse_schema(data, 32)?
        } else {
            (vec![], vec![], 32)
        };

        let key_index: std::collections::HashMap<String, usize> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i))
            .collect();

        if tail_ptr + 4 > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let record_count =
            u32::from_le_bytes(data[tail_ptr..tail_ptr + 4].try_into().unwrap()) as usize;
        let tail_start = tail_ptr + 4;

        Ok(Self {
            data,
            keys,
            key_sigils,
            key_index,
            record_count,
            tail_start,
        })
    }

    /// Number of top-level records in the file.
    pub fn record_count(&self) -> usize {
        self.record_count
    }

    /// Schema key names.
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// Schema sigil bytes, parallel to `keys()`.
    pub fn key_sigils(&self) -> &[u8] {
        &self.key_sigils
    }

    /// Resolve a key name to its slot index. O(1) via HashMap.
    pub fn slot(&self, key: &str) -> Option<usize> {
        self.key_index.get(key).copied()
    }

    /// Access a single record by zero-based index. O(1) via tail-index.
    pub fn record(&self, i: usize) -> Option<Record<'a, '_>> {
        if i >= self.record_count {
            return None;
        }
        let entry = self.tail_start + i * 10;
        let abs =
            u64::from_le_bytes(self.data.get(entry + 2..entry + 10)?.try_into().ok()?) as usize;
        Some(Record {
            data: self.data,
            reader: self,
            offset: abs,
        })
    }

    /// Return an iterator over all records.
    pub fn all(&'a self) -> Records<'a, 'a, AlwaysTrue> {
        Records {
            reader: self,
            pred: AlwaysTrue,
            index: 0,
        }
    }

    /// Return a lazy iterator over records matching `pred`.
    pub fn where_pred<P: Predicate>(&'a self, pred: P) -> Records<'a, 'a, P> {
        Records {
            reader: self,
            pred,
            index: 0,
        }
    }
}

// ── Record ────────────────────────────────────────────────────────────────────

/// A lazy view into a single NYXO object within the buffer.
/// Field reads decode directly from the mapped bytes — no allocation.
pub struct Record<'data, 'reader> {
    data: &'data [u8],
    reader: &'reader Reader<'data>,
    offset: usize,
}

impl<'data, 'reader> Record<'data, 'reader> {
    /// Resolve the byte offset of slot `s` within this object. Returns `None` if absent.
    fn resolve(&self, slot: usize) -> Option<usize> {
        resolve_slot(self.data, self.offset, slot)
    }

    /// Read an `i64` field.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        let slot = self.reader.slot(key)?;
        let off = self.resolve(slot)?;
        Some(i64::from_le_bytes(
            self.data.get(off..off + 8)?.try_into().ok()?,
        ))
    }

    /// Read an `f64` field.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        let slot = self.reader.slot(key)?;
        let off = self.resolve(slot)?;
        Some(f64::from_le_bytes(
            self.data.get(off..off + 8)?.try_into().ok()?,
        ))
    }

    /// Read a `bool` field.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let slot = self.reader.slot(key)?;
        let off = self.resolve(slot)?;
        Some(*self.data.get(off)? != 0)
    }

    /// Read a `&str` field (zero-copy slice into the buffer).
    pub fn get_str(&self, key: &str) -> Option<&'data str> {
        let slot = self.reader.slot(key)?;
        let off = self.resolve(slot)?;
        let len = u32::from_le_bytes(self.data.get(off..off + 4)?.try_into().ok()?) as usize;
        let bytes = self.data.get(off + 4..off + 4 + len)?;
        std::str::from_utf8(bytes).ok()
    }

    /// Walk a dot-notated path and read the leaf as `&str`.
    /// Example: `record.get_str_path("address.city")`
    pub fn get_str_path(&self, dot_path: &str) -> Option<&'data str> {
        let (leaf_off, data) = self.walk_path(dot_path)?;
        let len = u32::from_le_bytes(data.get(leaf_off..leaf_off + 4)?.try_into().ok()?) as usize;
        let bytes = data.get(leaf_off + 4..leaf_off + 4 + len)?;
        std::str::from_utf8(bytes).ok()
    }

    /// Walk a dot-notated path and read the leaf as `i64`.
    pub fn get_i64_path(&self, dot_path: &str) -> Option<i64> {
        let (off, data) = self.walk_path(dot_path)?;
        Some(i64::from_le_bytes(data.get(off..off + 8)?.try_into().ok()?))
    }

    /// Walk a dot-notated path and read the leaf as `f64`.
    pub fn get_f64_path(&self, dot_path: &str) -> Option<f64> {
        let (off, data) = self.walk_path(dot_path)?;
        Some(f64::from_le_bytes(data.get(off..off + 8)?.try_into().ok()?))
    }

    /// Walk a dot-notated path and read the leaf as `bool`.
    pub fn get_bool_path(&self, dot_path: &str) -> Option<bool> {
        let (off, data) = self.walk_path(dot_path)?;
        Some(*data.get(off)? != 0)
    }

    /// Navigate all but the last path segment, returning (leaf_offset, data).
    fn walk_path(&self, dot_path: &str) -> Option<(usize, &'data [u8])> {
        let mut parts = dot_path.splitn(8, '.'); // cap depth at 8
        let mut obj_offset = self.offset;
        let data = self.data;
        let mut part = parts.next()?;
        loop {
            let slot = self.reader.slot(part)?;
            let field_off = resolve_slot(data, obj_offset, slot)?;
            match parts.next() {
                None => return Some((field_off, data)),
                Some(next) => {
                    // intermediate: must be NYXO
                    let magic =
                        u32::from_le_bytes(data.get(field_off..field_off + 4)?.try_into().ok()?);
                    if magic != MAGIC_OBJ {
                        return None;
                    }
                    obj_offset = field_off;
                    part = next;
                }
            }
        }
    }
}

// ── Iterator ──────────────────────────────────────────────────────────────────

/// A lazy iterator over records filtered by `P`.
/// Does not allocate; predicate evaluation reads directly from the buffer.
pub struct Records<'data, 'reader, P: Predicate> {
    reader: &'reader Reader<'data>,
    pred: P,
    index: usize,
}

impl<'data, 'reader, P: Predicate> Iterator for Records<'data, 'reader, P> {
    type Item = Record<'data, 'reader>;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.reader;
        loop {
            if self.index >= r.record_count {
                return None;
            }
            let i = self.index;
            self.index += 1;
            let entry = r.tail_start + i * 10;
            let abs =
                u64::from_le_bytes(r.data.get(entry + 2..entry + 10)?.try_into().ok()?) as usize;
            if self.pred.test(r.data, r, abs) {
                return Some(Record {
                    data: r.data,
                    reader: r,
                    offset: abs,
                });
            }
        }
    }
}

// ── Predicates ────────────────────────────────────────────────────────────────

/// A predicate tests a record in-place without allocation.
pub trait Predicate {
    fn test(&self, data: &[u8], reader: &Reader<'_>, obj_offset: usize) -> bool;
}

/// Always-true predicate for `Reader::all()`.
pub struct AlwaysTrue;
impl Predicate for AlwaysTrue {
    fn test(&self, _: &[u8], _: &Reader<'_>, _: usize) -> bool {
        true
    }
}

/// `Eq("key", value)` — equality for bool, &str, i64, f64.
pub struct Eq<'k, V> {
    pub key: &'k str,
    pub value: V,
}

pub fn eq<'k, V>(key: &'k str, value: V) -> crate::query::Eq<'k, V> {
    crate::query::Eq { key, value }
}

impl Predicate for Eq<'_, bool> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff)
            .map(|&b| (b != 0) == self.value)
            .unwrap_or(false)
    }
}

impl<'k> Predicate for Eq<'k, &str> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        let Some(len_bytes) = data.get(foff..foff + 4) else {
            return false;
        };
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
        data.get(foff + 4..foff + 4 + len)
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s == self.value)
            .unwrap_or(false)
    }
}

impl Predicate for Eq<'_, i64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff..foff + 8)
            .and_then(|b| b.try_into().ok())
            .map(|b| i64::from_le_bytes(b) == self.value)
            .unwrap_or(false)
    }
}

impl Predicate for Eq<'_, f64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff..foff + 8)
            .and_then(|b| b.try_into().ok())
            .map(|b| f64::from_le_bytes(b) == self.value)
            .unwrap_or(false)
    }
}

/// `Gt("key", value)` — greater-than for f64 or i64.
pub struct Gt<'k, V> {
    pub key: &'k str,
    pub value: V,
}

pub fn gt<'k, V>(key: &'k str, value: V) -> crate::query::Gt<'k, V> {
    crate::query::Gt { key, value }
}

impl Predicate for Gt<'_, f64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff..foff + 8)
            .and_then(|b| b.try_into().ok())
            .map(|b| f64::from_le_bytes(b) > self.value)
            .unwrap_or(false)
    }
}

impl Predicate for Gt<'_, i64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff..foff + 8)
            .and_then(|b| b.try_into().ok())
            .map(|b| i64::from_le_bytes(b) > self.value)
            .unwrap_or(false)
    }
}

/// `Lt("key", value)` — less-than.
pub struct Lt<'k, V> {
    pub key: &'k str,
    pub value: V,
}

pub fn lt<'k, V>(key: &'k str, value: V) -> crate::query::Lt<'k, V> {
    crate::query::Lt { key, value }
}

impl Predicate for Lt<'_, f64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff..foff + 8)
            .and_then(|b| b.try_into().ok())
            .map(|b| f64::from_le_bytes(b) < self.value)
            .unwrap_or(false)
    }
}

impl Predicate for Lt<'_, i64> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        let Some(slot) = reader.slot(self.key) else {
            return false;
        };
        let Some(foff) = resolve_slot(data, off, slot) else {
            return false;
        };
        data.get(foff..foff + 8)
            .and_then(|b| b.try_into().ok())
            .map(|b| i64::from_le_bytes(b) < self.value)
            .unwrap_or(false)
    }
}

/// `And(p1, p2)` — logical AND of two predicates.
pub struct And<A, B>(pub A, pub B);

impl<A: Predicate, B: Predicate> Predicate for And<A, B> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        self.0.test(data, reader, off) && self.1.test(data, reader, off)
    }
}

/// `Or(p1, p2)` — logical OR of two predicates.
pub struct Or<A, B>(pub A, pub B);

impl<A: Predicate, B: Predicate> Predicate for Or<A, B> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        self.0.test(data, reader, off) || self.1.test(data, reader, off)
    }
}

/// `Not(p)` — logical NOT.
pub struct Not<P>(pub P);

impl<P: Predicate> Predicate for Not<P> {
    fn test(&self, data: &[u8], reader: &Reader<'_>, off: usize) -> bool {
        !self.0.test(data, reader, off)
    }
}

// ── Schema parser ─────────────────────────────────────────────────────────────

pub(crate) fn parse_schema(data: &[u8], offset: usize) -> Result<(Vec<String>, Vec<u8>, usize)> {
    if offset + 2 > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let key_count = u16::from_le_bytes(
        data[offset..offset + 2]
            .try_into()
            .map_err(|_| NxsError::OutOfBounds)?,
    ) as usize;
    let mut pos = offset + 2;

    if pos + key_count > data.len() {
        return Err(NxsError::OutOfBounds);
    }
    let sigils = data[pos..pos + key_count].to_vec();
    pos += key_count;

    let mut keys = Vec::with_capacity(key_count);
    for _ in 0..key_count {
        let start = pos;
        while pos < data.len() && data[pos] != 0 {
            pos += 1;
        }
        if pos >= data.len() {
            return Err(NxsError::OutOfBounds);
        }
        keys.push(
            std::str::from_utf8(&data[start..pos])
                .map_err(|_| NxsError::ParseError("invalid utf-8 key".into()))?
                .to_owned(),
        );
        pos += 1; // skip null terminator
    }
    // align to 8 bytes
    if pos % 8 != 0 {
        pos += 8 - pos % 8;
    }
    Ok((keys, sigils, pos))
}

// ── resolveSlot ───────────────────────────────────────────────────────────────

/// Stateless LEB128 bitmask walker — returns the absolute byte offset of
/// the value at `slot` within the NYXO object at `obj_offset`, or `None`.
pub(crate) fn resolve_slot(data: &[u8], obj_offset: usize, slot: usize) -> Option<usize> {
    let mut p = obj_offset + 8; // skip NYXO magic (4) + length (4)
    let mut cur: usize = 0;
    let mut table_idx: usize = 0;
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
    // skip remaining continuation bytes
    while b & 0x80 != 0 {
        b = *data.get(p)?;
        p += 1;
    }
    let rel = u16::from_le_bytes(
        data.get(p + table_idx * 2..p + table_idx * 2 + 2)?
            .try_into()
            .ok()?,
    ) as usize;
    Some(obj_offset + rel)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::{NxsWriter, Schema};

    fn make_nxb() -> Vec<u8> {
        let schema = Schema::new(&["id", "username", "score", "active"]);
        let mut w = NxsWriter::new(&schema);
        for (id, name, score, active) in [
            (1i64, "alice", 95.0f64, true),
            (2i64, "bob", 42.0f64, false),
            (3i64, "carol", 88.0f64, true),
            (4i64, "dave", 15.0f64, false),
            (5i64, "eve", 77.0f64, true),
        ] {
            w.begin_object();
            w.write_i64(crate::writer::Slot(0), id);
            w.write_str(crate::writer::Slot(1), name);
            w.write_f64(crate::writer::Slot(2), score);
            w.write_bool(crate::writer::Slot(3), active);
            w.end_object();
        }
        w.finish()
    }

    #[test]
    fn reader_opens_and_counts() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.record_count(), 5);
        assert_eq!(r.keys(), &["id", "username", "score", "active"]);
    }

    #[test]
    fn record_access_by_index() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(2).unwrap();
        assert_eq!(rec.get_str("username"), Some("carol"));
        assert_eq!(rec.get_i64("id"), Some(3));
        assert!((rec.get_f64("score").unwrap() - 88.0).abs() < 1e-9);
        assert_eq!(rec.get_bool("active"), Some(true));
    }

    #[test]
    fn all_iterates_every_record() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.all().count(), 5);
    }

    #[test]
    fn where_eq_bool() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let active: Vec<_> = r
            .where_pred(eq("active", true))
            .map(|rec| rec.get_str("username").unwrap().to_owned())
            .collect();
        assert_eq!(active, vec!["alice", "carol", "eve"]);
    }

    #[test]
    fn where_gt_f64() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r.where_pred(gt("score", 80.0f64)).count();
        assert_eq!(count, 2); // alice(95) + carol(88)
    }

    #[test]
    fn where_lt_f64() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r.where_pred(lt("score", 50.0f64)).count();
        assert_eq!(count, 2); // bob(42) + dave(15)
    }

    #[test]
    fn where_and() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r
            .where_pred(And(eq("active", true), gt("score", 80.0f64)))
            .count();
        assert_eq!(count, 2); // alice + carol
    }

    #[test]
    fn where_or() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r
            .where_pred(Or(gt("score", 90.0f64), lt("score", 20.0f64)))
            .count();
        assert_eq!(count, 2); // alice(95) + dave(15)
    }

    #[test]
    fn where_not() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let count = r.where_pred(Not(eq("active", true))).count();
        assert_eq!(count, 2); // bob + dave
    }

    #[test]
    fn early_termination() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let first = r.all().next().unwrap();
        assert_eq!(first.get_str("username"), Some("alice"));
    }

    #[test]
    fn unknown_key_matches_nothing() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        assert_eq!(r.where_pred(eq("nonexistent", true)).count(), 0);
    }

    #[test]
    fn get_str_path_single_segment() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(0).unwrap();
        assert_eq!(rec.get_str_path("username"), Some("alice"));
    }

    #[test]
    fn get_str_path_absent_returns_none() {
        let data = make_nxb();
        let r = Reader::new(&data).unwrap();
        let rec = r.record(0).unwrap();
        assert_eq!(rec.get_str_path("no.such.path"), None);
    }
}
