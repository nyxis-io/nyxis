# NXS v1.3 — Compact Encoding Extensions

**Status:** Draft proposal
**Target spec version:** v1.3 (single batched version bump)
**Applies to:** `.nxb` row layout, columnar layout, tail-index; compiler behavior
**Author:** Micael Malta
**Last updated:** 2026-06-10

---

## 1. Motivation

On-disk size is currently the format's weakest headline metric: the 8-field 1M-record
benchmark fixture compiles to 131 MB vs 147 MB JSON (89%) and 73 MB CSV (49%).

Measured segment breakdown (`nxs stats`, `records_1000.nxb`, 127.1 KB, row layout,
fully dense):

| Category                  | Bytes    | % of file | Nature                          |
| ------------------------- | -------- | --------- | ------------------------------- |
| String payload (2 fields) | ~38.0 KB | 29.9%     | Irreducible (unique values)     |
| Fixed-width payload       | ~40.1 KB | 31.6%     | Reducible via narrow widths     |
| **Record framing**        | **31.2 KB** | **24.6%** | **Reducible via dense path**  |
| Tail-index                | 9.8 KB   | 7.7%      | Reducible via block deltas      |
| Bool padding              | 6.8 KB   | 5.4%      | Reducible via packing           |
| String cell padding       | 1.1 KB   | 0.9%      | Accepted alignment cost         |
| Preamble + schema + footer| 108 B    | <0.1%     | Fixed                           |

Framing — not string payload — is the single largest overhead category on dense
fixtures. This spec defines four wire-format extensions and one compiler behavior
that together target a reduction from ~89% of JSON to roughly **55–65% of JSON**
on dense fixtures, without sacrificing the format's two invariants.

## 2. Design invariants

Every extension in this document MUST preserve:

1. **Zero-copy reads.** All payload cells remain directly readable from mapped
   bytes without a decode pass. Alignment may narrow (§5) but never disappears.
2. **O(1) record seek.** Resolving record *k* remains a constant number of memory
   reads (§6 permits exactly one additional integer add).

Additional constraints:

3. **Opt-in via preamble flags.** A v1.2 file is byte-identical under a v1.3
   writer with no flags set. v1.2 readers MUST reject files carrying unknown
   REQUIRED-class flags (see §8).
4. **In-place patching is load-bearing.** Fixed-width delta patching by byte-range
   offset is a format differentiator. Each section defines its patching semantics
   explicitly; no extension may silently break it.
5. **One version bump.** All four wire changes ship together as v1.3. Each
   independent flag multiplies the 10-driver conformance matrix; batching bounds
   the matrix to one new row per driver per feature, validated in one release.

## 3. Preamble flag allocation

Four new bits in the preamble `Flags` field (exact bit positions to be assigned
against the current SPEC.md reservation table):

| Flag                | Class    | Section | Meaning                                  |
| ------------------- | -------- | ------- | ---------------------------------------- |
| `FLAG_DENSE_FRAMES` | REQUIRED | §4      | File may contain dense-framed records    |
| `FLAG_PACKED_BOOLS` | REQUIRED | §5.1    | Bool fields packed into shared words     |
| `FLAG_NARROW_CELLS` | REQUIRED | §5.2    | Schema header declares per-field widths  |
| `FLAG_DELTA_TAIL`   | REQUIRED | §6      | Tail-index uses block-anchored deltas    |

REQUIRED-class: a reader that does not implement the flag MUST refuse the file
with a versioned error, never misparse it.

---

## 4. Dense-record framing (`FLAG_DENSE_FRAMES`)

### 4.1 Problem

Current row framing costs ~31 bytes per 8-field record: per-field 2-byte slot
indices, type tags, the LEB128 presence bitmask, and per-cell alignment. For a
fully populated record with a fixed schema, all of this is redundant — the schema
header already determines field order, types, and (with §5) widths, so every cell
offset is computable at file-open time.

### 4.2 Wire format

Each record begins with a 1-byte record header (already present as the record
marker; one reserved bit is claimed here):

- **Bit `D` = 0 (sparse frame):** record encoded exactly as v1.2 — LEB128
  presence bitmask followed by `(slot u16, cell)` pairs. No change.
- **Bit `D` = 1 (dense frame):** record contains *every* schema field, in schema
  declaration order, with **no per-field slot indices, no type tags, and no
  presence bitmask**. Cell offsets within the record are fixed and derived from
  the schema header.

A writer MAY mix dense and sparse frames freely within one file. A writer
SHOULD emit a dense frame whenever all schema fields are present.

Variable-length cells (String `"`, Binary `<>`) inside a dense frame keep their
existing `u32 length + payload (+ pad to 8)` encoding; only the cells *after* the
first variable-length field have non-constant offsets. Readers therefore
precompute, at open time:

- `fixed_prefix[]` — constant offsets for every field up to and including the
  first variable-length field;
- a skip chain for fields after it (each skip is one aligned `u32` length read).

Schemas with all variable-length fields trailing **MUST** use descending-width
wire order when `FLAG_DENSE_WIRE_REORDER` (0x0100) is set; the reference
compiler sets this flag with `--compact`. Logical field order in the schema
header is unchanged. Files without the flag retain schema declaration order on
the wire (v1.3.0-compat readers).

### 4.3 Patching semantics

Unchanged for fixed-width cells in dense frames: offsets are computable, so
in-place patching by byte range continues to work — and gets *cheaper*, since no
bitmask walk is needed. Patching a field to **absent** in a dense frame is not
representable; writers MUST rewrite the record as a sparse frame via the existing
append-and-reindex path.

### 4.4 Expected impact

Dense fixtures: framing collapses from ~31 B/record to 1 B (header) + skip-chain
lengths. On `records_1000.nxb`: **−29 KB (−22.8% of file)**. Sparse fixtures:
zero gain, zero cost (frames stay sparse).

---

## 5. Cell-width reductions

### 5.1 Packed booleans (`FLAG_PACKED_BOOLS`)

**Problem.** A Bool cell is 1 payload byte + 7 padding bytes: 87% of every bool
column is padding (6.8 KB of 7.8 KB on the stats fixture).

**Row layout.** All Bool-sigil fields in a schema are assigned bit positions
(schema declaration order) within a single shared **bool word**: one 8-byte
aligned `u64` per record, present in any frame that contains at least one bool
field, located at the position of the first bool field in cell order. Individual
bool fields no longer occupy cells. Supports up to 64 bool fields per record
type; schemas exceeding 64 bools allocate additional words.

*Presence vs value:* in sparse frames the existing presence bitmask still
declares whether each bool field is present; the bool word carries values only.
Absent bool bits MUST be written as 0 and MUST be ignored by readers.

*Patching:* a bool patch becomes a read-modify-write of one byte within the bool
word. This is in-place but **not atomic with respect to sibling bools in the same
byte**; concurrent patchers of different bool fields in one record MUST
synchronize externally, or use 8-bit-aligned single-bool words via §5.2 widths.
This caveat MUST appear in SPEC.md normative text.

**Columnar / PAX layout.** Bool column buffers change from `N × 8` bytes to a
1-bit-per-record bitmap, padded to 8 bytes. (64× reduction; this is the larger
absolute win and has no patching caveat, since columnar buffers are seal-time.)

**Expected impact** on the stats fixture: **−6.8 KB (−5.4%)**.

### 5.2 Narrow integer cells (`FLAG_NARROW_CELLS`)

**Problem.** Every integer cell is 8 bytes; `age` (fits in `u8`) and `id` (fits
in `u32` at 1k–4B records) burn 7 and 4 wasted bytes per record respectively.

**Wire format.** The schema header field manifest gains a per-field `width` byte
for Int64 (`=`) and Float64 (`~`) sigils: `{1, 2, 4, 8}` for ints, `{4, 8}` for
floats (f32 permitted only when the compiler proves exact representability or
the source uses an explicit width pragma). Cells align to **their own width**,
not 8 bytes. The `.nxs` source syntax is unchanged — width is a compile-time
inference (min/max scan over the dataset) or an explicit per-field pragma
(`@width(id, 4)`).

**Patching semantics.** A patch value exceeding the declared width MUST fail
with a range error; it MUST NOT silently truncate. Writers needing headroom use
the width pragma to pin 8 bytes. This trade — patch headroom vs size — is the
author's explicit choice and MUST be documented in GETTING_STARTED.md.

**Decompile fidelity.** `nxb → nxs` decompilation reproduces plain `=` / `~`
sigils plus the inferred width as a comment or pragma, so round-trips are stable.

**Expected impact** on the stats fixture (`age` → u8, `id` → u32): **−10.7 KB
(−8.4%)**. Schema-dependent; the compiler emits an inference report at compile
time so authors see what narrowed.

---

## 6. Block-anchored delta tail-index (`FLAG_DELTA_TAIL`)

### 6.1 Problem

The tail-index stores `(KeyID u16, AbsoluteOffset u64)` per record: 10 B/record,
9.8 KB (7.7%) on the stats fixture, ~10 MB on the 1M benchmark. Offsets are
strictly monotonic; 64 bits per entry is redundant.

### 6.2 Wire format

The tail-index becomes three sections (struct-of-arrays, each 8-byte aligned):

1. **Anchor table:** one `u64` absolute offset per block of `A` records.
   `A = 1024` by default; recorded in the tail-index header. Anchor `j` is the
   absolute offset of record `j·A`.
2. **Delta table:** one `u32` per record: the offset of record `k` **relative to
   its block anchor** (not to the previous record). A block whose span exceeds
   `2^32` bytes is ill-formed; writers MUST shrink `A` for such files (the
   tail-index header permits per-file `A`).
3. **KeyID table:** the existing `u16` per record, now contiguous. Files whose
   records all share one KeyID (the overwhelmingly common case) set a header bit
   and omit this table entirely.

### 6.3 Seek algorithm

```
anchor = anchors[k >> log2(A)]
offset = anchor + deltas[k]        // exactly one extra add vs v1.2
```

O(1) is preserved literally — two reads and one add. Streaming/WAL sealing is
unaffected: the tail-index is written at seal time in both versions.

### 6.4 Expected impact

10 B/record → 4 B/record + (8/A) B/record amortized, single-KeyID case:
**9.8 KB → ~4.0 KB (−4.5% of file)** on the stats fixture; **~6 MB saved** on
the 1M benchmark.

---

## 7. Auto-keyword promotion (compiler behavior; no new wire format)

### 7.1 Behavior

At compile time the compiler performs a cardinality scan per String (`"`) field.
Any field whose distinct-value count is ≤ `--keyword-threshold` (default 4096,
i.e. within the existing 2-byte dictionary index space with headroom) is
**promoted**: values are interned into the existing keyword dictionary and cells
encode as 2-byte indices, exactly as the `$` sigil does today.

The schema header marks the field `promoted_string` (distinct from native `$`)
so that decompilation regenerates `"string"` syntax, preserving source-level
round-trip fidelity. Promotion can be disabled per field (`@no-promote(field)`)
or globally (`--no-keyword-promotion`).

### 7.2 Prerequisites

- `get_keyword` MUST be implemented in the Rust reference reader (currently
  unimplemented) **before** this ships; the manual `$` path must be complete and
  conformance-covered across all ten drivers first.
- Conformance vectors for: promoted field decode, decompile round-trip,
  dictionary-overflow rejection (writer MUST fall back to plain String, with a
  compile warning, if cardinality exceeds the threshold mid-stream in WAL mode —
  streaming writers therefore only promote on explicit pragma, never by
  inference, since cardinality is unknowable upfront).

### 7.3 Expected impact

Near zero on the current benchmark fixture (username/email are high-cardinality
by construction). On log/span workloads (`level`, `service`, `region`,
`operation`): typically the **largest single reduction of any item in this
document** — a 16-byte average string cell becomes 2 bytes. Ship a log-shaped
fixture (§10) so this is measurable and publishable.

---

## 8. Versioning, compatibility, conformance

### 8.1 Wire version and rejection

- Files using any §3 flag set spec version `1.3` in the preamble.
- v1.2 readers MUST reject v1.3-flagged files with `ERR_UNSUPPORTED_FLAGS`.
  The error message MUST include the compact flag bits **and** an upgrade hint,
  e.g. `upgrade your nyxis driver to >= 1.3.0` — old lockfiles pinning an
  SDK are the dominant post-launch failure mode once compact is the default.
- Any driver that ignores unknown preamble bits has a latent misparse bug;
  audit all ten before release.

### 8.2 Compiler defaults (launch)

**At launch, compact row encoding is the product — not an opt-in flag.**

| Surface | Default | Escape hatch |
| ------- | ------- | ------------ |
| Batch `nxs compile` | v1.3 compact (§4–§7) | `--legacy-v12` for one release cycle |
| Streaming / WAL writers | Compact **framing** + delta tail-index; **8-byte cells** unless widths declared by pragma | Same; no full-dataset width inference |

Batch compile infers narrow widths and keyword promotion when it can see all
rows. Streaming writers cannot scan min/max upfront — they emit compact framing
but keep default-width cells unless the schema declares widths explicitly.
That split is coherent: infer when you can see the data, declare when you cannot.

Implementation note: `COMPILE_DEFAULT_COMPACT` in `rust/src/layout.rs` flips
in a **one-line launch commit** after driver decode ships — not entangled in the
encoder feature branch.

The hidden `--compact` CLI flag is removed after launch; `--legacy-v12` is the
only row-layout switch. Do not ship tutorials that teach `--compact` as the
good path.

### 8.3 Release train (sequenced, same tag)

1. Merge encoder + **frozen** `conformance/v13/*` vectors (fixed decode target).
2. Ship driver releases that **decode** v1.3 (not rejection-only).
3. Flip `COMPILE_DEFAULT_COMPACT = true` + invert CLI to `--legacy-v12`.
4. Publish benchmarks and site fixtures as a single “NXS” row — not “v1.2 vs compact”.

Pre-launch there is no installed base; this sequencing costs nothing now and
becomes a migration project if deferred.

### 8.4 Driver decode — long pole and triage

Ten independent implementations (dense framing, skip chains, narrow widths,
delta tail-index). Parallelize against frozen conformance vectors from step 1.

| Tier | Drivers | Launch gate |
| ---- | ------- | ----------- |
| **0** | JavaScript | **Blocker** — browser demos are the marketing surface; must read default-compiled files. **Done gate:** re-run the browser bench page against compact fixtures; trailing-string skip-chain walk is the plausible read-latency regression vs v1.2 slot-indexed cells |
| **1** | Go, Python (pure + C ext), C | Benchmark and CLI headliners |
| **2** | Ruby, PHP, C# | Full ecosystem claim |
| **3** | Kotlin, Swift | README “decode landing this week” acceptable; not launch blockers |

### 8.5 Conformance

Each feature contributes (a) a positive vector, (b) a mixed dense/sparse vector
(§4), (c) a rejection vector for v1.2 readers, (d) patching-semantics vectors
where applicable (§4.3, §5.1, §5.2). Tier-0/1 drivers pass the full `v13/`
set before the default flip; tier-3 may trail by days with documented status.

## 9. Explicit non-goals

- **Gorilla / delta-of-delta timestamps, XOR float packing** (row layout):
  rejected — variable-width cells destroy in-place fixed-width patching, a core
  differentiator. MAY be revisited as columnar-only encodings in a later
  version.
- **Beating CSV on dense unique-string tables:** structural non-goal. CSV
  carries no types, no index, no alignment; closing that gap means abandoning
  the format's invariants. The size story is told on enum-heavy fixtures and
  on-wire numbers instead (§10).
- **General-purpose page compression (Zstd/LZ4):** deferred to a separate
  proposal (candidate v1.4, PAX-first). It breaks on-disk zero-copy and
  deserves its own design review. Until then, **transport compression**
  (`Content-Encoding: zstd`/`br`) is the recommended wire-size answer; it
  requires no spec change and SHOULD be documented in GETTING_STARTED.md and
  reflected as an "on wire" row in BENCHMARK.md.

## 10. Measurement & publication plan

1. `nxs stats` gains `--markdown` output; BENCHMARK.md gains a per-segment
   byte-attribution table for every published fixture.
2. New canonical fixtures: `spans_sparse_*` (sparse, bool/enum-heavy) and
   `logs_dense_*` (low-cardinality strings) alongside the existing
   `records_*` — chosen so each §4–§7 lever is independently visible.
3. Published size table: one **NXS** row (default compile output) against JSON,
   JSON+gzip, CSV, Protobuf, FlatBuffers; optional *on-wire (zstd)* transport row.
   `--legacy-v12` is a footnote, not a headline column.

### Measured impact (`records_1000`, dense 8-field fixture, 2026-06-10)

| Artifact | Bytes | vs JSON (145.5 KB) | vs v1.2 row (127.1 KB) |
| -------- | ----: | -----------------: | ---------------------: |
| v1.2 row (`records_1000.nxb`) | 130,112 | 89.4% | — |
| v1.3 `--compact` (schema-order wire, no `FLAG_DENSE_WIRE_REORDER`) | 93,150 | 64.0% | 71.5% |
| v1.3 `--compact` + `FLAG_DENSE_WIRE_REORDER` (mandatory compiler behavior) | **81,150** | **55.8%** | **62.4%** |

Measured `records_1_000_000` (same schema, dense fixture):

| Artifact | Bytes | vs JSON (147.1 MB) | vs v1.2 row (131.5 MB) |
| -------- | ----: | -----------------: | ---------------------: |
| v1.2 row | 137,920,112 | 89.4% | — |
| v1.3 `--compact` + wire reorder | **83,007,958** | **53.8%** | **60.2%** |

Read path (Rust `Reader`, best of 5): at 1M rows `sum(score)` is **14.8 ms**
(v1.2) vs **14.2 ms** (compact); random `get_f64("score")` ×1000 is **15–16 ns/rec**
both layouts. Wire reorder makes fixed-width body offsets constant; the reference
reader precomputes them at open (`dense_fixed_body_offsets`) so dense resolve
stays O(1) for fixed fields.

Field-by-field decode gate: `compact_records_1000_matches_v12_decode` — all 8
fields × 1,000 records identical to v1.2.

Remaining file budget on the measured compact fixture (`nxs stats`): framing
~8.8 B/record (1-byte dense header + 2× `u32` string length prefixes); string
cell padding ~5 KB (pad-to-8 on variable cells at tail). Optional `u16` length
prefixes (~3.9 KB) and relaxed var-cell alignment are diminishing returns.

## 11. Open questions

1. ~~§4.2 cell reordering~~ **Resolved:** mandatory via `FLAG_DENSE_WIRE_REORDER`
   on `--compact` output; load-bearing for padding elimination and the dense
   skip chain.
2. §5.1 bool-word placement when all bool fields are absent in a sparse frame:
   omit the word (bitmask-gated) or always emit when schema has bools? Omitting
   saves 8 B/record on bool-sparse data; always-emitting simplifies offsets.
3. §6 default block size `A`: 1024 assumed; validate against P99 record sizes
   in WAL workloads so `u32` deltas never force tiny blocks.
4. ~~Default encoding~~ **Resolved:** compact is the launch default for batch
   compile; `--legacy-v12` for one cycle; flip via `COMPILE_DEFAULT_COMPACT` in
   the same release train as tier-0 driver decode (§8.3).
