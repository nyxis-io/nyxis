//! Shared format constants for the NXS binary format (.nxb).
//!
//! All magic numbers and flag bits are defined once here and re-exported so
//! that `compiler.rs`, `decoder.rs`, `layout.rs`, `query.rs`, `writer.rs`,
//! and `pax_stream.rs` can use them without independent redefinitions.

// ── Magic numbers ─────────────────────────────────────────────────────────────

/// File preamble magic: `NYXB` (0x4E595842)
pub const MAGIC_FILE: u32 = 0x4E59_5842;

/// Object magic: `NYXO` (0x4E59584F)
pub const MAGIC_OBJ: u32 = 0x4E59_584F;

/// List magic: `NYXL` (0x4E59584C)
pub const MAGIC_LIST: u32 = 0x4E59_584C;

/// Footer magic: `NXS!` (0x2153584E)
pub const MAGIC_FOOTER: u32 = 0x2153_584E;

/// PAX page magic: `NXSP` (0x4E585350)
pub const MAGIC_PAGE: u32 = 0x4E58_5350;

// ── Version ───────────────────────────────────────────────────────────────────

/// Preamble version field: major=1, minor=1
pub const VERSION: u16 = 0x0101;

// ── Preamble flag bits ────────────────────────────────────────────────────────

/// Bit 0: Jumbo Row offsets (when bit 1 clear) **or** `FLAG_COLUMNAR` (when bit 1 set).
/// See SPEC.md §4.2.1 for the full bit-0/bit-1 disambiguation table.
pub const FLAG_COLUMNAR: u16 = 0x0001;

/// Bit 1: Schema Header is embedded immediately after the Preamble.
pub const FLAG_SCHEMA_EMBEDDED: u16 = 0x0002;

/// Bit 2: PAX (Partition Attributes Across) layout.
pub const FLAG_PAX: u16 = 0x0004;

/// Bit 3: per-PAX-page CRC32 enabled.
pub const FLAG_PAGE_CRC: u16 = 0x0008;
