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

// ── Sigil bytes ───────────────────────────────────────────────────────────────

/// Integer sigil: `=`
pub const SIGIL_INT: u8 = b'=';
/// Float sigil: `~`
pub const SIGIL_FLOAT: u8 = b'~';
/// Boolean sigil: `?`
pub const SIGIL_BOOL: u8 = b'?';
/// Keyword sigil: `$`
pub const SIGIL_KEYWORD: u8 = b'$';
/// String sigil: `"`
pub const SIGIL_STR: u8 = b'"';
/// Timestamp sigil: `@`
pub const SIGIL_TIME: u8 = b'@';
/// Binary sigil: `<`
pub const SIGIL_BINARY: u8 = b'<';
/// Link sigil: `&`
pub const SIGIL_LINK: u8 = b'&';
/// Null sigil: `^`
pub const SIGIL_NULL: u8 = b'^';

// ── Version ───────────────────────────────────────────────────────────────────

/// Preamble version field: major=1, minor=1 (v1.2 row baseline).
pub const VERSION: u16 = 0x0101;

/// Preamble version when any v1.3 compact flag is set.
pub const VERSION_V13: u16 = 0x0103;

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

// ── v1.3 compact encoding flags (REQUIRED class) ─────────────────────────────

/// Bit 4: records may use dense framing (§4).
pub const FLAG_DENSE_FRAMES: u16 = 0x0010;

/// Bit 5: bool fields packed into shared u64 words (§5.1).
pub const FLAG_PACKED_BOOLS: u16 = 0x0020;

/// Bit 6: schema carries per-field width bytes (§5.2).
pub const FLAG_NARROW_CELLS: u16 = 0x0040;

/// Bit 7: tail-index uses block-anchored deltas (§6).
pub const FLAG_DELTA_TAIL: u16 = 0x0080;

/// Bit 8: dense frames use descending-width wire order (§4.2); when clear, schema order.
pub const FLAG_DENSE_WIRE_REORDER: u16 = 0x0100;

/// Mask of all v1.3 compact preamble bits (REQUIRED-class for v1.2 readers).
pub const FLAG_V13_COMPACT_MASK: u16 =
    FLAG_DENSE_FRAMES | FLAG_PACKED_BOOLS | FLAG_NARROW_CELLS | FLAG_DELTA_TAIL;

/// Dense-record header bit (byte after NYXO length field).
pub const RECORD_HDR_DENSE: u8 = 0x01;

/// Default tail-index anchor block size (§6).
pub const DEFAULT_DELTA_BLOCK_SIZE: u32 = 1024;

/// FieldAttrs bit: string field promoted to value-pool index (§7).
pub const FIELD_ATTR_PROMOTED: u8 = 0x01;

/// FieldAttrs bit: inline string/binary uses `u16` length prefix (default `u32`).
pub const FIELD_ATTR_U16_LEN: u8 = 0x02;
