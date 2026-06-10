//! `.nxb` byte breakdown — segments and per-field payload vs padding.
//!
//! Used by `nxs stats` to surface where file size goes before tuning encodings.

use crate::compact::{
    dense_field_offset, is_dense_record, parse_extended_schema, resolve_field_offset,
    ExtendedSchema, RowCellPlan,
};
use crate::consts::{
    FLAG_COLUMNAR, FLAG_DENSE_FRAMES, FLAG_PAX, FLAG_V13_COMPACT_MASK, MAGIC_OBJ, SIGIL_BINARY,
    SIGIL_BOOL, SIGIL_FLOAT, SIGIL_INT, SIGIL_KEYWORD, SIGIL_NULL, SIGIL_STR, SIGIL_TIME,
};
use crate::decoder::{self, DecodedFile};
use crate::error::{NxsError, Result};
use crate::layout::{col_var_parts, column_sector_len, is_var_sigil, null_bitmap_bytes};
use crate::pax_stream::complete_page_end;
use crate::query::{resolve_slot, Layout, Reader};
use serde::Serialize;
use std::io::Write;

const PREAMBLE_BYTES: u64 = 32;
const COLUMNAR_TAIL_ENTRY_BYTES: u64 = 20;
const PAX_TAIL_ENTRY_BYTES: u64 = 28;
const PAX_PAGE_HEADER: usize = 24;

#[derive(Debug, Clone, Serialize)]
pub struct FileStats {
    pub path: Option<String>,
    pub file_size: u64,
    pub layout: String,
    pub record_count: u64,
    /// Preamble DictHash (Murmur3-64 of embedded schema header bytes).
    #[serde(alias = "dict_hash")]
    pub schema_hash: String,
    pub segments: SegmentStats,
    pub fields: Vec<FieldStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentStats {
    pub preamble: u64,
    pub schema: u64,
    pub data_sector: u64,
    pub tail_index: u64,
    pub footer: u64,
    /// Row: NYXO headers/bitmasks/offset tables. Columnar/PAX: page headers and alignment gaps.
    pub framing: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldStats {
    pub name: String,
    pub sigil: String,
    pub payload: u64,
    pub padding: u64,
    pub null_bitmap: u64,
    pub offsets: u64,
    pub total: u64,
}

#[derive(Default, Clone)]
struct FieldAccum {
    payload: u64,
    padding: u64,
    null_bitmap: u64,
    offsets: u64,
}

impl FieldAccum {
    fn total(&self) -> u64 {
        self.payload + self.padding + self.null_bitmap + self.offsets
    }

    fn into_stats(self, name: String, sigil: u8) -> FieldStats {
        FieldStats {
            name,
            sigil: (sigil as char).to_string(),
            payload: self.payload,
            padding: self.padding,
            null_bitmap: self.null_bitmap,
            offsets: self.offsets,
            total: self.total(),
        }
    }
}

/// Analyze a sealed `.nxb` buffer.
pub fn analyze(data: &[u8]) -> Result<FileStats> {
    analyze_with_path(data, None)
}

pub fn analyze_with_path(data: &[u8], path: Option<&str>) -> Result<FileStats> {
    let decoded = decoder::decode(data)?;
    let reader = Reader::new(data)?;
    let layout = reader.layout();
    let file_size = data.len() as u64;

    let schema = decoded.data_sector_start as u64 - PREAMBLE_BYTES;
    let footer = footer_bytes(decoded.flags);
    let tail_index = tail_index_bytes(data, &decoded, layout, footer);
    let data_sector = decoded.tail_ptr as u64 - decoded.data_sector_start as u64;

    let mut fields = match layout {
        Layout::Row => analyze_row_fields(data, &reader, &decoded)?,
        Layout::Columnar => analyze_columnar_fields(&reader)?,
        Layout::Pax => analyze_pax_fields(data, &decoded)?,
    };
    fields.sort_by(|a, b| b.total.cmp(&a.total));

    let field_bytes: u64 = fields.iter().map(|f| f.total).sum();
    let framing = data_sector.saturating_sub(field_bytes);

    Ok(FileStats {
        path: path.map(str::to_string),
        file_size,
        layout: layout_name(layout).to_string(),
        record_count: decoded.record_count as u64,
        schema_hash: format!("0x{:016x}", decoded.dict_hash),
        segments: SegmentStats {
            preamble: PREAMBLE_BYTES,
            schema,
            data_sector,
            tail_index,
            footer,
            framing,
        },
        fields,
    })
}

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Row => "row",
        Layout::Columnar => "columnar",
        Layout::Pax => "pax",
    }
}

fn footer_bytes(flags: u16) -> u64 {
    if flags & FLAG_PAX != 0 {
        28
    } else if flags & FLAG_COLUMNAR != 0 {
        20
    } else {
        12
    }
}

fn tail_index_bytes(data: &[u8], decoded: &DecodedFile, layout: Layout, footer: u64) -> u64 {
    match layout {
        Layout::Row => {
            let end = data.len().saturating_sub(footer as usize);
            end.saturating_sub(decoded.tail_ptr as usize) as u64
        }
        Layout::Columnar => decoded.keys.len() as u64 * COLUMNAR_TAIL_ENTRY_BYTES,
        Layout::Pax => {
            let fo = data.len().saturating_sub(footer as usize);
            if fo >= 20 {
                let page_count =
                    u32::from_le_bytes(data[fo + 16..fo + 20].try_into().unwrap_or([0; 4])) as u64;
                page_count * PAX_TAIL_ENTRY_BYTES
            } else {
                0
            }
        }
    }
}

fn analyze_row_fields(
    data: &[u8],
    reader: &Reader,
    decoded: &DecodedFile,
) -> Result<Vec<FieldStats>> {
    let nkeys = decoded.keys.len();
    let mut acc = vec![FieldAccum::default(); nkeys];

    let ext_plan = if decoded.flags & FLAG_V13_COMPACT_MASK != 0 {
        let (ext, _) = parse_extended_schema(data, 32, decoded.flags)?;
        let plan = RowCellPlan::new(&ext, decoded.flags);
        Some((ext, plan))
    } else {
        None
    };
    let dense = decoded.flags & FLAG_DENSE_FRAMES != 0;

    for ri in 0..decoded.record_count {
        let rec = reader.record(ri).ok_or(NxsError::OutOfBounds)?;
        let obj_off = rec.object_offset().ok_or(NxsError::OutOfBounds)?;

        if let (true, Some((ext, plan))) = (dense, ext_plan.as_ref()) {
            if is_dense_record(data, obj_off)? {
                accumulate_dense_row_record(data, obj_off, ext, plan, &mut acc)?;
                continue;
            }
        }

        for slot in 0..nkeys {
            let val_off = if let Some((ref ext, ref plan)) = ext_plan {
                resolve_field_offset(data, obj_off, slot, ext, plan, dense)
            } else {
                resolve_slot(data, obj_off, slot)
            };
            let Some(val_off) = val_off else {
                continue;
            };
            let sigil = decoded.key_sigils.get(slot).copied().unwrap_or(0);
            let (payload, padding) = if let Some((ref ext, ref plan)) = ext_plan {
                row_cell_wire_parts(data, val_off, slot, sigil, ext, plan)?
            } else {
                row_value_wire_parts(data, val_off, sigil, None, slot)?
            };
            acc[slot].payload += payload;
            acc[slot].padding += padding;
        }
    }

    Ok(acc
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            a.into_stats(
                decoded.keys[i].clone(),
                decoded.key_sigils.get(i).copied().unwrap_or(0),
            )
        })
        .collect())
}

fn analyze_columnar_fields(reader: &Reader) -> Result<Vec<FieldStats>> {
    let nkeys = reader.keys().len();
    let mut out = Vec::with_capacity(nkeys);
    let rc = reader.record_count();

    for slot in 0..nkeys {
        let sigil = reader.key_sigils().get(slot).copied().unwrap_or(0);
        let accum = if is_var_sigil(sigil) {
            let (bm, offsets, values) = reader.col_field_var_parts(slot)?;
            accumulate_var_column(bm, offsets, values, rc)?
        } else {
            let (bm, vals) = reader.col_field_parts(slot)?;
            accumulate_fixed_column(bm, vals, rc, sigil)?
        };
        out.push(accum.into_stats(reader.keys()[slot].clone(), sigil));
    }
    Ok(out)
}

fn analyze_pax_fields(data: &[u8], decoded: &DecodedFile) -> Result<Vec<FieldStats>> {
    let nkeys = decoded.keys.len();
    let mut acc = vec![FieldAccum::default(); nkeys];
    let sigils = &decoded.key_sigils;
    let mut off = decoded.data_sector_start;

    while off < decoded.tail_start {
        let Some(page_end) = complete_page_end(data, off, sigils) else {
            break;
        };
        if off + PAX_PAGE_HEADER > data.len() {
            return Err(NxsError::OutOfBounds);
        }
        let record_count = u32::from_le_bytes(
            data[off + 16..off + 20]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        let field_count = u16::from_le_bytes(
            data[off + 20..off + 22]
                .try_into()
                .map_err(|_| NxsError::OutOfBounds)?,
        ) as usize;
        let mut body = off + PAX_PAGE_HEADER;
        for fi in 0..field_count.min(nkeys) {
            let sigil = sigils.get(fi).copied().unwrap_or(SIGIL_INT);
            let sector_len = column_sector_len(&data[body..], record_count, sigil)?;
            let sector = &data[body..body + sector_len];
            let part = if is_var_sigil(sigil) {
                let (bm, offsets, values) = col_var_parts(sector, record_count)?;
                accumulate_var_column(bm, offsets, values, record_count)?
            } else {
                let bm_len = null_bitmap_bytes(record_count);
                let vals_end = bm_len + record_count * 8;
                accumulate_fixed_column(
                    &sector[..bm_len],
                    &sector[bm_len..vals_end],
                    record_count,
                    sigil,
                )?
            };
            acc[fi].payload += part.payload;
            acc[fi].padding += part.padding;
            acc[fi].null_bitmap += part.null_bitmap;
            acc[fi].offsets += part.offsets;
            body += sector_len;
        }
        off = page_end;
    }

    Ok(acc
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            a.into_stats(
                decoded.keys[i].clone(),
                decoded.key_sigils.get(i).copied().unwrap_or(0),
            )
        })
        .collect())
}

fn accumulate_fixed_column(
    bm: &[u8],
    vals: &[u8],
    record_count: usize,
    sigil: u8,
) -> Result<FieldAccum> {
    let bm_payload = ((record_count + 7) / 8) as u64;
    let mut acc = FieldAccum {
        null_bitmap: bm_payload,
        padding: bm.len() as u64 - bm_payload,
        ..Default::default()
    };
    let cell_size = 8u64;
    if sigil == SIGIL_BOOL || sigil == SIGIL_NULL {
        acc.payload = record_count as u64;
        acc.padding += record_count as u64 * 7;
    } else {
        acc.payload = record_count as u64 * cell_size;
    }
    let expected = record_count * 8;
    if vals.len() != expected {
        return Err(NxsError::OutOfBounds);
    }
    let _ = vals;
    Ok(acc)
}

fn accumulate_var_column(
    bm: &[u8],
    offsets: &[u8],
    values: &[u8],
    record_count: usize,
) -> Result<FieldAccum> {
    let bm_payload = ((record_count + 7) / 8) as u64;
    Ok(FieldAccum {
        null_bitmap: bm_payload,
        padding: bm.len() as u64 - bm_payload,
        offsets: offsets.len() as u64,
        payload: values.len() as u64,
    })
}

fn accumulate_dense_row_record(
    data: &[u8],
    obj_off: usize,
    ext: &ExtendedSchema,
    plan: &RowCellPlan,
    acc: &mut [FieldAccum],
) -> Result<()> {
    let body_base = obj_off + 9;
    let wire = plan.dense_wire_order(ext);
    let mut prev_slot: Option<usize> = None;
    let mut prev_end = 0usize;

    for &slot in &wire {
        if plan.packed_bools && plan.bool_slots.contains(&slot) && plan.first_bool != Some(slot) {
            continue;
        }
        let Some(start) = dense_field_offset(data, obj_off, slot, ext, plan)? else {
            continue;
        };
        let rel_start = start - body_base;
        if rel_start > prev_end {
            if let Some(cause) = prev_slot {
                acc[cause].padding += (rel_start - prev_end) as u64;
            }
        }
        let sigil = ext.sigils[slot];
        let (payload, padding) = row_cell_wire_parts(data, start, slot, sigil, ext, plan)?;
        acc[slot].payload += payload;
        acc[slot].padding += padding;
        prev_end = rel_start + (payload + padding) as usize;
        prev_slot = Some(slot);
    }
    Ok(())
}

fn row_cell_wire_parts(
    data: &[u8],
    val_off: usize,
    slot: usize,
    sigil: u8,
    ext: &ExtendedSchema,
    plan: &RowCellPlan,
) -> Result<(u64, u64)> {
    if plan.packed_bools && plan.bool_slots.contains(&slot) {
        let bw = plan.bool_word_bytes() as u64;
        if Some(slot) == plan.first_bool {
            return Ok((bw, 0));
        }
        return Ok((0, 0));
    }
    if ext.is_promoted(slot) || sigil == SIGIL_KEYWORD {
        return Ok((2, 0));
    }
    let w = if plan.narrow { ext.cell_width(slot) } else { 8 };
    match sigil {
        SIGIL_BOOL | SIGIL_NULL => Ok((1, 7)),
        SIGIL_INT | SIGIL_FLOAT | SIGIL_TIME => Ok((w as u64, 0)),
        SIGIL_STR | SIGIL_BINARY => row_value_wire_parts(data, val_off, sigil, Some(ext), slot),
        _ => Ok((8, 0)),
    }
}

fn row_value_wire_parts(
    data: &[u8],
    offset: usize,
    sigil: u8,
    ext: Option<&ExtendedSchema>,
    slot: usize,
) -> Result<(u64, u64)> {
    if offset >= data.len() {
        return Err(NxsError::OutOfBounds);
    }

    if offset + 4 <= data.len() {
        let magic = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        if magic == MAGIC_OBJ {
            if offset + 8 > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let len = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as u64;
            return Ok((len, 0));
        }
    }

    match sigil {
        SIGIL_BOOL | SIGIL_NULL => Ok((1, 7)),
        SIGIL_INT | SIGIL_FLOAT | SIGIL_TIME => Ok((8, 0)),
        SIGIL_KEYWORD => Ok((2, 6)),
        SIGIL_STR | SIGIL_BINARY => {
            let prefix = ext.map(|e| e.str_len_prefix(slot)).unwrap_or(4);
            if offset + prefix > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let len = crate::compact::read_str_cell_len(data, offset, prefix)?;
            if len > 1024 * 1024 * 1024 || offset + prefix + len > data.len() {
                return Err(NxsError::OutOfBounds);
            }
            let content = prefix as u64 + len as u64;
            let total = (content + 7) & !7;
            Ok((content, total - content))
        }
        _ => Ok((8, 0)),
    }
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64) * 100.0
    }
}

fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

/// Human-readable report (default `nxs stats` output).
pub fn render_text<W: Write>(writer: &mut W, stats: &FileStats) -> Result<()> {
    let title = stats.path.as_deref().unwrap_or("<stdin>");
    writeln!(
        writer,
        "NXS file stats: {title} ({})",
        fmt_bytes(stats.file_size)
    )
    .map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(writer, "  layout:      {}", stats.layout)
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(writer, "  records:     {}", stats.record_count)
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(writer, "  schema_hash: {}", stats.schema_hash)
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(
        writer,
        "               (preamble DictHash; Murmur3-64 of schema header — not a value-pool fingerprint)"
    )
    .map_err(|e| NxsError::IoError(e.to_string()))?;

    writeln!(writer).map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(writer, "Segments:").map_err(|e| NxsError::IoError(e.to_string()))?;
    let s = &stats.segments;
    for (label, bytes) in [
        ("preamble", s.preamble),
        ("schema", s.schema),
        ("data sector", s.data_sector),
        ("  framing", s.framing),
        ("tail-index", s.tail_index),
        ("footer", s.footer),
    ] {
        writeln!(
            writer,
            "  {:14} {:>12}  ({:.1}%)",
            label,
            fmt_bytes(bytes),
            pct(bytes, stats.file_size)
        )
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    }

    writeln!(writer).map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(
        writer,
        "Per-field (sorted by total bytes; columnar/PAX includes null bitmap + offset tables):"
    )
    .map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(
        writer,
        "  {:20} {:5} {:>12} {:>12} {:>10} {:>10} {:>10}",
        "field", "sigil", "total", "payload", "padding", "null_bm", "offsets"
    )
    .map_err(|e| NxsError::IoError(e.to_string()))?;

    for f in &stats.fields {
        writeln!(
            writer,
            "  {:20} {:>5} {:>12} {:>12} {:>10} {:>10} {:>10}",
            f.name,
            f.sigil,
            fmt_bytes(f.total),
            fmt_bytes(f.payload),
            fmt_bytes(f.padding),
            if f.null_bitmap > 0 {
                fmt_bytes(f.null_bitmap)
            } else {
                "—".to_string()
            },
            if f.offsets > 0 {
                fmt_bytes(f.offsets)
            } else {
                "—".to_string()
            },
        )
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    }
    Ok(())
}

/// JSON report (`nxs stats --json`).
pub fn render_json<W: Write>(writer: &mut W, stats: &FileStats) -> Result<()> {
    serde_json::to_writer_pretty(&mut *writer, stats)
        .map_err(|e| NxsError::IoError(e.to_string()))?;
    writeln!(writer).map_err(|e| NxsError::IoError(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::CompactOptions;
    use crate::layout::{finish_columnar, finish_pax, finish_row, Cell, RecordRow};
    use crate::writer::{NxsWriter, Schema};

    fn sample_row_nxb() -> Vec<u8> {
        let schema = Schema::new(&["id", "username", "score", "active"]);
        let mut w = NxsWriter::new(&schema);
        for (id, name, score, active) in [
            (1i64, "alice", 95.0f64, true),
            (2i64, "bob", 42.0f64, false),
            (3i64, "carol", 88.0f64, true),
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
    fn stats_row_segments_sum_to_file_size() {
        let data = sample_row_nxb();
        let stats = analyze(&data).unwrap();
        let sum = stats.segments.preamble
            + stats.segments.schema
            + stats.segments.data_sector
            + stats.segments.tail_index
            + stats.segments.footer;
        assert_eq!(sum, stats.file_size);
        assert_eq!(stats.layout, "row");
        assert_eq!(stats.record_count, 3);
    }

    #[test]
    fn stats_row_bool_field_reports_padding() {
        let data = sample_row_nxb();
        let stats = analyze(&data).unwrap();
        let active = stats.fields.iter().find(|f| f.name == "active").unwrap();
        assert_eq!(active.payload, 3);
        assert_eq!(active.padding, 21);
        assert_eq!(active.total, 24);
    }

    #[test]
    fn stats_row_string_field_counts_length_prefix() {
        let data = sample_row_nxb();
        let stats = analyze(&data).unwrap();
        let user = stats.fields.iter().find(|f| f.name == "username").unwrap();
        assert_eq!(user.payload, 3 * 4 + 5 + 3 + 5);
        assert!(user.padding > 0);
    }

    #[test]
    fn stats_columnar_layout() {
        let keys = vec!["id".into(), "active".into()];
        let rows = vec![
            RecordRow {
                cells: vec![Cell::I64(1), Cell::Bool(true)],
            },
            RecordRow {
                cells: vec![Cell::I64(2), Cell::Bool(false)],
            },
        ];
        let data = finish_columnar(&keys, &rows).unwrap();
        let stats = analyze(&data).unwrap();
        assert_eq!(stats.layout, "columnar");
        assert_eq!(stats.record_count, 2);
        let active = stats.fields.iter().find(|f| f.name == "active").unwrap();
        assert_eq!(active.null_bitmap, 1);
        assert_eq!(active.payload, 2);
        assert_eq!(active.padding, 21);
    }

    #[test]
    fn stats_pax_layout() {
        let keys = vec!["id".into(), "name".into()];
        let rows: Vec<RecordRow> = (0..10)
            .map(|i| RecordRow {
                cells: vec![Cell::I64(i), Cell::Str(format!("user{i}"))],
            })
            .collect();
        let data = finish_pax(&keys, &rows, 4).unwrap();
        let stats = analyze(&data).unwrap();
        assert_eq!(stats.layout, "pax");
        assert_eq!(stats.record_count, 10);
        assert!(!stats.fields.is_empty());
        let sum = stats.segments.preamble
            + stats.segments.schema
            + stats.segments.data_sector
            + stats.segments.tail_index
            + stats.segments.footer;
        assert_eq!(sum, stats.file_size);
    }

    #[test]
    fn u16_string_length_prefix_on_wire_and_saves_when_padding_allows() {
        use crate::compact::dense_field_offset;
        use crate::compact::{
            parse_extended_schema, read_str_cell_len, CompactOptions, RowCellPlan,
        };
        use crate::consts::FLAG_SCHEMA_EMBEDDED;

        let payload = "x".repeat(13);
        let keys = vec!["name".into()];
        let rows = vec![RecordRow {
            cells: vec![Cell::Str(payload.clone())],
        }];
        let with_u16 = CompactOptions::compact();
        let mut without = CompactOptions::compact();
        without.u16_string_lengths = false;
        let narrow = finish_row(&keys, &rows, Some(&with_u16)).unwrap();
        let wide = finish_row(&keys, &rows, Some(&without)).unwrap();
        assert_eq!(wide.len() - narrow.len(), 8);

        let flags = CompactOptions::compact().preamble_flags() | FLAG_SCHEMA_EMBEDDED;
        let (ext, schema_end) = parse_extended_schema(&narrow, 32, flags).unwrap();
        assert!(ext.is_u16_len(0));
        let plan = RowCellPlan::new(&ext, CompactOptions::compact().preamble_flags());
        let off = dense_field_offset(&narrow, schema_end, 0, &ext, &plan)
            .unwrap()
            .unwrap();
        let len = read_str_cell_len(&narrow, off, 2).unwrap();
        assert_eq!(len, 13);
        assert_eq!(&narrow[off + 2..off + 2 + len], payload.as_bytes());
    }

    #[test]
    fn stats_reads_schema_order_demo_fixture_when_present() {
        let path = "/tmp/nxs_demo/records_1000_fixed.nxb";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let data = std::fs::read(path).expect("read demo fixture");
        analyze(&data).expect("stats on schema-order demo fixture");
    }

    #[test]
    fn stats_schema_order_compact_without_wire_reorder_flag() {
        use crate::compact::CompactOptions;

        let keys = vec![
            "id".into(),
            "username".into(),
            "email".into(),
            "age".into(),
            "balance".into(),
            "active".into(),
            "score".into(),
            "created_at".into(),
        ];
        let rows: Vec<RecordRow> = (0..2)
            .map(|i| RecordRow {
                cells: vec![
                    Cell::I64(i as i64),
                    Cell::Str(format!("user_{i:07}")),
                    Cell::Str(format!("user{i}@example.com")),
                    Cell::I64(20 + (i % 50) as i64),
                    Cell::F64(100.0 + i as f64 * 1.37),
                    Cell::Bool(i % 3 != 0),
                    Cell::F64((i as f64 % 100.0) / 10.0),
                    Cell::Time(1_700_000_000_000_000_000 + i as i64),
                ],
            })
            .collect();
        let mut opts = CompactOptions::compact();
        opts.dense_wire_reorder = false;
        let data = finish_row(&keys, &rows, Some(&opts)).unwrap();
        analyze(&data).expect("schema-order compact stats");
    }

    #[test]
    fn stats_compact_bool_field_reports_padding_not_payload() {
        use crate::compact::CompactOptions;

        let keys = vec![
            "id".into(),
            "username".into(),
            "email".into(),
            "age".into(),
            "balance".into(),
            "active".into(),
            "score".into(),
            "created_at".into(),
        ];
        let rows: Vec<RecordRow> = (0..1000)
            .map(|i| RecordRow {
                cells: vec![
                    Cell::I64(i as i64),
                    Cell::Str(format!("user_{i:07}")),
                    Cell::Str(format!("user{i}@example.com")),
                    Cell::I64(20 + (i % 50) as i64),
                    Cell::F64(100.0 + i as f64 * 1.37),
                    Cell::Bool(i % 3 != 0),
                    Cell::F64((i as f64 % 100.0) / 10.0),
                    Cell::Time(1_700_000_000_000_000_000 + i as i64),
                ],
            })
            .collect();
        let data = finish_row(&keys, &rows, Some(&CompactOptions::compact())).unwrap();
        let stats = analyze(&data).unwrap();
        let balance = stats.fields.iter().find(|f| f.name == "balance").unwrap();
        let score = stats.fields.iter().find(|f| f.name == "score").unwrap();
        let active = stats.fields.iter().find(|f| f.name == "active").unwrap();
        assert_eq!(active.payload, 1000);
        // Descending-width wire order: 8-byte floats pack flush; no blame on score/balance.
        assert_eq!(score.padding, 0, "score padding={}", score.padding);
        assert_eq!(balance.padding, 0, "balance padding={}", balance.padding);
    }

    fn compact_fixture_rows(with_strings: bool) -> (Vec<String>, Vec<RecordRow>) {
        if with_strings {
            let keys = vec![
                "id".into(),
                "username".into(),
                "email".into(),
                "age".into(),
                "balance".into(),
                "active".into(),
                "score".into(),
                "created_at".into(),
            ];
            let rows: Vec<RecordRow> = (0..1000)
                .map(|i| RecordRow {
                    cells: vec![
                        Cell::I64(i as i64),
                        Cell::Str(format!("user_{i:07}")),
                        Cell::Str(format!("user{i}@example.com")),
                        Cell::I64(20 + (i % 50) as i64),
                        Cell::F64(100.0 + i as f64 * 1.37),
                        Cell::Bool(i % 3 != 0),
                        Cell::F64((i as f64 % 100.0) / 10.0),
                        Cell::Time(1_700_000_000_000_000_000 + i as i64),
                    ],
                })
                .collect();
            (keys, rows)
        } else {
            let keys = vec![
                "id".into(),
                "age".into(),
                "balance".into(),
                "active".into(),
                "score".into(),
                "created_at".into(),
            ];
            let rows: Vec<RecordRow> = (0..1000)
                .map(|i| RecordRow {
                    cells: vec![
                        Cell::I64(i as i64),
                        Cell::I64(20 + (i % 50) as i64),
                        Cell::F64(100.0 + i as f64 * 1.37),
                        Cell::Bool(i % 3 != 0),
                        Cell::F64((i as f64 % 100.0) / 10.0),
                        Cell::Time(1_700_000_000_000_000_000 + i as i64),
                    ],
                })
                .collect();
            (keys, rows)
        }
    }

    #[test]
    fn stats_compact_framing_is_dense_not_sparse() {
        use crate::compact::{is_dense_record, CompactOptions};
        use crate::query::Reader;

        let (keys_fixed, rows_fixed) = compact_fixture_rows(false);
        let (keys_str, rows_str) = compact_fixture_rows(true);
        let fixed = finish_row(&keys_fixed, &rows_fixed, Some(&CompactOptions::compact())).unwrap();
        let with_str = finish_row(&keys_str, &rows_str, Some(&CompactOptions::compact())).unwrap();
        let v12 = finish_row(&keys_str, &rows_str, None).unwrap();

        for (label, data) in [("fixed", &fixed), ("strings", &with_str)] {
            let reader = Reader::new(data).unwrap();
            for i in [0, 499, 999] {
                let off = reader.record(i).unwrap().object_offset().unwrap();
                assert!(
                    is_dense_record(data, off).unwrap(),
                    "{label} record {i} is not dense-framed"
                );
            }
        }

        let framing_fixed = analyze(&fixed).unwrap().segments.framing;
        let framing_str = analyze(&with_str).unwrap().segments.framing;
        let framing_v12 = analyze(&v12).unwrap().segments.framing;
        let per_fixed = framing_fixed / 1000;
        let per_str = framing_str / 1000;
        let per_v12 = framing_v12 / 1000;

        // Dense NYXO envelope ≈ 14 B/record; v1.2 sparse bitmask+offset table ≈ 31 B/record.
        assert!(
            per_fixed < 25 && per_str < 25,
            "compact framing too high: fixed={per_fixed} B/rec strings={per_str} B/rec"
        );
        assert!(
            per_v12 > 25,
            "v1.2 framing unexpectedly low: {per_v12} B/rec"
        );
        // u32 length prefixes land in string field payload, not framing.
        assert!(
            per_str.abs_diff(per_fixed) <= 2,
            "string fields added {}/rec framing (expected ≤2); fixed={framing_fixed} strings={framing_str}",
            per_str.abs_diff(per_fixed)
        );
    }

    #[test]
    fn stats_compact_row_layout() {
        let keys = vec!["id".into(), "active".into(), "score".into()];
        let rows = vec![
            RecordRow {
                cells: vec![Cell::I64(1), Cell::Bool(true), Cell::F64(1.0)],
            },
            RecordRow {
                cells: vec![Cell::I64(2), Cell::Bool(false), Cell::F64(2.0)],
            },
        ];
        let data = finish_row(&keys, &rows, Some(&CompactOptions::compact())).unwrap();
        let stats = analyze(&data).unwrap();
        assert_eq!(stats.layout, "row");
        assert_eq!(stats.record_count, 2);
        let sum = stats.segments.preamble
            + stats.segments.schema
            + stats.segments.data_sector
            + stats.segments.tail_index
            + stats.segments.footer;
        assert_eq!(sum, stats.file_size);
    }
}
