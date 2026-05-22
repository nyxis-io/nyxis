//! Arrow C Data Interface projection helpers for columnar `.nxb`.
//!
//! Columnar string/binary columns use `(N+1) × u32` little-endian offsets into a
//! contiguous values buffer. Arrow `LargeUtf8` / `LargeBinary` use `(N+1) × i64`
//! offsets — export widens offsets in metadata only; values buffer is unchanged.

use crate::error::Result;
use crate::layout::col_var_parts;

/// Zero-copy view of a variable-length column sector inside a mapped `.nxb` buffer.
pub struct VarColumnView<'a> {
    pub null_bitmap: &'a [u8],
    /// `(record_count + 1) × 4` bytes, u32 little-endian (NXS; not Arrow i64).
    pub offsets: &'a [u8],
    pub values: &'a [u8],
    pub record_count: usize,
}

impl<'a> VarColumnView<'a> {
    pub fn from_sector(sector: &'a [u8], record_count: usize) -> Result<Self> {
        let (bm, offsets, values) = col_var_parts(sector, record_count)?;
        Ok(Self {
            null_bitmap: bm,
            offsets,
            values,
            record_count,
        })
    }

    /// Number of u32 offset slots (`record_count + 1`).
    pub fn offset_count(&self) -> usize {
        self.record_count + 1
    }
}
