//! Arrow C Data Interface projection helpers for columnar `.nxb`.
//!
//! ## Intended purpose
//!
//! This module is intended to export NXS columnar fields as **Arrow C Data Interface**
//! buffers, enabling zero-copy interoperability with Arrow-aware runtimes (DataFusion,
//! Polars, DuckDB, etc.) without copying the underlying values data.
//!
//! ## What is NOT implemented
//!
//! - **Offset widening**: NXS variable-length columns use `(N+1) × u32` little-endian
//!   offsets; Arrow `LargeUtf8` / `LargeBinary` require `(N+1) × i64` offsets.
//!   The widening step (copying and sign-extending each `u32` to `i64`) is not yet
//!   implemented — callers currently receive the raw `u32` offsets slice only.
//! - **Arrow C Data Interface structs**: the `ArrowSchema` and `ArrowArray` C structs
//!   (as defined in the Arrow ABI specification) are not emitted; no FFI boundary is
//!   crossed. This module provides only the Rust-side buffer views.
//! - **PAX layout support**: only contiguous columnar (FLAG_COLUMNAR) files are
//!   considered; PAX page-scattered columns are out of scope for this stub.
//!
//! ## Status
//!
//! **Intentionally left as a stub** pending the extensions phase. Full Arrow C Data
//! Interface export (including `ArrowSchema` / `ArrowArray` lifetime management and
//! `u32 → i64` offset widening) is planned for the commercial extensions tier and
//! will not be implemented in this MIT-licensed driver module.

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
