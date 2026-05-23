This is the formal, exhaustive specification for the **Nyxis (NXS)**. This document is designed to be the "Ground Truth" for developers implementing NXS compilers, parsers, and runtime engines.

**Spec License:** This specification document and all conformance vectors in `conformance/` are licensed under [Creative Commons Attribution 4.0 International (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/). Anyone may implement the NXS wire format for any purpose, including commercial products, without a license from Micael Malta, provided they attribute this spec. This grant is irrevocable and independent of the BSL that governs the Rust tooling in this repository.

---

# RFC 001: The Nyxis (NXS) Specification v1.2

**Date:** April 30, 2026
**Status:** Stable (v1.2)
**Editors:** Micael Malta
**MIME Types:** `application/nxb` (Binary), `application/nxs` (Text)

---

## 1. Abstract
NXS is a high-performance, bi-modal data serialization format that prioritizes **CPU-native memory alignment**, **O(1) record lookup** via a tail-index, and **O(1) field access** within a located record. By utilizing a sigil-based type system in text and a bitmask-driven architecture in binary with zero-copy access for aligned atomic values, NXS eliminates the "parsing tax" common in JSON and the "rigidity tax" common in Protobuf.

## 2. Terminology
The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in **RFC 2119**.

---

## 3. The Source Format (.nxs)
The source format is a human-readable UTF-8 representation.

### 3.1 Sigils and Data Types
Every value **MUST** be prefixed with a Sigil to define its machine representation.

| Sigil | Type | Description | Binary Encoding |
| :--- | :--- | :--- | :--- |
| `=` | **Int64** | Signed 64-bit integer | `int64_t` (Little-Endian) |
| `~` | **Float64** | 64-bit floating point | `double` (IEEE 754, Little-Endian) |
| `?` | **Bool** | Truth value | `uint8_t` (0x01 or 0x00) |
| `$` | **Keyword** | Dictionary-interned key | `uint16_t` (Dict Index, Little-Endian) |
| `"` | **String** | UTF-8 Text | `uint32_t` (Len, LE) + Bytes |
| `@` | **Time** | Unix Nanoseconds | `int64_t` (Little-Endian) |
| `<>` | **Binary** | Raw Byte Stream | `uint32_t` (Len, LE) + Bytes |
| `&` | **Link** | Relative pointer | `int32_t` (Byte offset, LE) |
| `!` | **Macro** | Compile-time formula | (Resolved to base type) |
| `^` | **Null** | Explicit absent value | No payload (bitmask bit set, zero-width) |

> **Normative note — Keyword type (`$`, v1.2).** The Keyword sigil encodes a dictionary-interned symbol as a `uint16_t` index (little-endian) into the Schema Header's key-name dictionary (StringPool). This avoids repeating the full string for frequently repeated symbolic values. Implementation status:
> - **Binary encoding**: `uint16_t` dict index, valid range `[0, KeyCount)`.
> - **`get_keyword` is not implemented** in the current Rust reader; accessing a `$`-typed field in columnar or PAX layout returns `Err(NxsError::UnsupportedFieldType)`. Row-layout NYXO objects do not yet decode the index to a string name either.
> - **Full round-trip support** (compiler emission + reader materialisation to string) is planned for a future release.

### 3.2 String Literals and Escape Sequences
String values (sigil `"`) are enclosed in double-quote characters. The following escape sequences **MUST** be supported:

| Sequence | Meaning |
| :--- | :--- |
| `\\` | Literal backslash |
| `\"` | Literal double-quote |
| `\n` | Newline (U+000A) |
| `\r` | Carriage return (U+000D) |
| `\t` | Horizontal tab (U+0009) |
| `\0` | Null byte (U+0000) |
| `\uXXXX` | Unicode code point (4 hex digits) |
| `\UXXXXXXXX` | Unicode code point (8 hex digits) |

Any other character following `\` is a parse error. Parsers **MUST NOT** silently ignore unknown escape sequences.

### 3.3 Macro Expressions (`!`)
A Macro value is a compile-time expression resolved by the NXS compiler before binary output is produced. The expression language is a restricted arithmetic and string subset:

* **Literals:** Any base sigil value (e.g., `=10`, `~3.14`, `"hello"`).
* **Arithmetic:** `+`, `-`, `*`, `/`, `%` over numeric types.
* **String concatenation:** `+` over two String operands.
* **References:** `@key` dereferences another key in the same object scope.
* **Built-ins:** `now()` (current Unix nanoseconds as Int64), `len(@key)` (byte length of a String or Binary value).

**Example:**
```text
config {
    base_url: "https://api.example.com"
    version: =2
    endpoint: !"@base_url/v" + @version
}
```

Macros **MUST** be fully resolved at compile time. A Macro that cannot be statically resolved (e.g., references a runtime value) is a compile error. The resolved value is encoded using its resulting base type.

### 3.4 Structure
* **Objects:** Defined by `{}`. Keys do not require quotes unless they contain whitespace or the characters `{}[]:"`.
* **Lists:** Defined by `[]`. Elements are comma-separated and **MUST** be of a uniform sigil type within a single list.
* **Null:** The `^` sigil stands alone with no following value token.
* **Scope:** Objects can be nested indefinitely, subject to the recursion limit defined in Section 9.

---

## 4. The Binary Format (.nxb)
The binary representation is designed for memory mapping with **zero-copy access to aligned atomic values** (Int64, Float64, Time). Variable-length values (String, Binary) are length-prefixed; their payloads are not copied, but string interpretation (e.g. UTF-8 decoding) is a separate concern handled by the reader at materialization time. All multi-byte integer fields use **Little-Endian** byte order. NXS does not support Big-Endian byte order and provides no endianness negotiation mechanism. Readers on Big-Endian architectures MUST byte-swap multi-byte fields after reading.

### 4.1 Memory Alignment (The Rule of 8)
All atomic values (Int64, Float64, Temporal) **MUST** start at a file offset that satisfies the following condition:

$$Offset \equiv 0 \pmod{8}$$

The compiler **MUST** insert null bytes (`0x00`) as padding to maintain this alignment. Strings and Binary blobs are length-prefixed and **MUST** also be padded at their tail to ensure the *next* value is aligned.

Bool values are 1 byte; the compiler **MUST** insert 7 bytes of padding after each Bool.

### 4.2 File Layout
A `.nxb` file consists of four segments in order:

```
[Preamble 32B][Schema Header][Data Sector][Tail-Index]
```

#### 4.2.1 Preamble (exactly 32 bytes)

| Offset | Size | Field | Description |
| :--- | :--- | :--- | :--- |
| 0 | 4 | `Magic` | `0x4E595842` (`NYXB`) |
| 4 | 2 | `Version` | `0x0101` (major=1, minor=1) |
| 6 | 2 | `Flags` | Bit 0: Jumbo Offsets **or** `FLAG_COLUMNAR` (0x0001); Bit 1: Schema Embedded (0x0002); Bit 2: `FLAG_PAX` (0x0004); Bits 3–15: reserved. `FLAG_COLUMNAR` and `FLAG_PAX` are mutually exclusive and require Schema Embedded. See **Normative Annex A (OLAP.md)** for columnar/PAX footers and tail-index layouts. |

> **Normative note — bit 0 disambiguation (v1.2).** Bit 0 carries two distinct meanings that are resolved by inspecting bit 1:
>
> | Bit 1 (0x0002) | Bit 0 (0x0001) | Interpretation |
> | :--- | :--- | :--- |
> | 0 | 1 | **Jumbo Row** mode — the Offset Table inside each NYXO object uses `uint32_t` entries instead of `uint16_t`, extending the maximum object size from 64 KB to 4 GB. |
> | 1 | 1 | **Columnar layout** (`FLAG_COLUMNAR`) — the data sector and tail-index follow the columnar wire format defined in Normative Annex A (OLAP.md). Jumbo Offsets semantics **do not apply**; the bit simply identifies the columnar layout. |
> | 1 | 0 | Ordinary row layout with embedded schema and normal (`uint16_t`) offsets. |
> | 0 | 0 | Ordinary row layout, no embedded schema (external schema required). |
>
> A reader MUST NOT interpret bit 0 as Jumbo Offsets when bit 1 is also set. Conversely, a reader MUST NOT interpret bit 0 as `FLAG_COLUMNAR` when bit 1 is clear. Writers that set `FLAG_COLUMNAR` MUST also set bit 1 (`FLAG_SCHEMA_EMBEDDED`); the combination is validated by `ERR_INCOMPATIBLE_FLAGS` when `FLAG_COLUMNAR` appears without `FLAG_SCHEMA_EMBEDDED`.
| 8 | 8 | `DictHash` | 64-bit MurmurHash3 of the Schema Header bytes |
| 16 | 8 | `TailPtr` | Absolute byte offset to the Tail-Index; `0` means streamable v1.1 and the final footer carries the Tail-Index offset |
| 24 | 8 | `Reserved` | MUST be `0x00` |

#### 4.2.2 Schema Header
Present when `Flags` Bit 1 is set. Immediately follows the Preamble.

| Field | Type | Description |
| :--- | :--- | :--- |
| `KeyCount` | `u16` | Number of keys in the dictionary |
| `TypeManifest` | `u8[KeyCount]` | Sigil byte for each key, in dictionary order |
| `StringPool` | UTF-8 bytes | Null-terminated key name strings, concatenated |

The `StringPool` **MUST** be padded to an 8-byte boundary after the last null terminator.

#### 4.2.3 Compiler layout flags

The reference compiler (`nxs compile`) accepts:

```bash
nxs compile input.nxs                              # row-oriented (default)
nxs compile --layout columnar input.nxs            # FLAG_COLUMNAR
nxs compile --layout pax input.nxs                 # FLAG_PAX, default page size 4096
nxs compile --layout pax --page-size 1024 input.nxs
```

`--layout` and `--page-size` MAY also be set via `@layout` / `@page-size` pragmas in the `.nxs` source (pragma overrides CLI default; CLI overrides row default). Full columnar and PAX wire layouts are normative in **Normative Annex A (OLAP.md)**; this section records preamble interactions and streaming rules.

### 4.3 Columnar and PAX layouts (v1.2)

When `FLAG_COLUMNAR` (0x0001) or `FLAG_PAX` (0x0004) is set, the data sector and tail-index follow Normative Annex A (OLAP.md) instead of row-oriented NYXO objects. Both flags require `FLAG_SCHEMA_EMBEDDED` (0x0002). `FLAG_COLUMNAR` and `FLAG_PAX` are mutually exclusive.

| Flag | Value | Footer size (bytes) |
| :--- | :--- | :--- |
| Row (default) | — | 12 (`FooterTailPtr` + `MagicFooter`) |
| `FLAG_COLUMNAR` | 0x0001 | 20 (+ `RecordCount`) |
| `FLAG_PAX` | 0x0004 | 28 (+ `RecordCount`, `PageCount`, `PageSize`) |

**Columnar streaming.** Columnar files **MUST NOT** use Preamble `TailPtr == 0`. Writers and readers **MUST** reject `FLAG_COLUMNAR` with `TailPtr == 0` using `ERR_INCOMPATIBLE_FLAGS`.

**Optional page CRC.** `FLAG_PAGE_CRC` (0x0008, preamble bit 3) enables a 4-byte CRC32 per PAX page (see Normative Annex A §4.2 / OLAP.md §4.2). Writers **MUST** leave this bit clear unless per-page integrity is required. Readers **MAY** verify CRC when the bit is set.

### 4.4 Schema Evolution

NXS uses an **additive-only** schema evolution model within a single file. Writers may add new fields to the schema, but **adding fields changes the DictHash** because the hash covers the full Schema Header. A file compiled with an extended schema is therefore a **new file** with a new DictHash; it is not a drop-in replacement for the original file.

**Within-file semantics** (reader queries a field the file's schema does not know about):

When a reader accesses a field by key name or slot index, it checks the per-object bitmask to determine whether that field is present. If the bit for a requested slot is `0` (absent) or the slot index is beyond the bitmask range, the reader MUST return the language-appropriate "absent" sentinel rather than raising an error.

**Rules:**
1. Writers MAY add new keys to the end of the schema when producing a new file. Existing slot indices are unchanged in the new file.
2. Writers MUST NOT reorder or remove keys from an existing schema.
3. Readers MUST treat a missing bitmask bit (field not present in this record) as absent, not as an error.
4. A reader that knows about slots `0` through `M` can query any file whose schema has N ≥ M fields; fields beyond slot M will simply appear absent.
5. A reader caching an external schema MUST verify `DictHash` before using that schema. A hash mismatch means the file was compiled from a different (or evolved) schema; the reader MUST fall back to the embedded Schema Header or fail with `ERR_DICT_MISMATCH`.

**Example:** A file written with schema `["a", "b", "c"]` can be read by a reader that only queries `"a"` and `"b"`. Field `"c"` will be absent for that reader — no error is raised. A later file written with schema `["a", "b", "c", "d"]` has a different DictHash and is treated as a distinct file; cached schemas must not be applied to it without hash verification.

### 4.5 PAX streaming protocol

PAX layout (`FLAG_PAX`) supports **page-level streaming**: complete pages may be read while the file is still growing. This differs from row-oriented v1.1 streaming (§7), which polls complete NYXO objects.

#### 4.5.1 Writer (unsealed)

1. Write Preamble with `FLAG_PAX | FLAG_SCHEMA_EMBEDDED`, `TailPtr = 0`, and embedded Schema Header.
2. Accumulate records until the current page reaches `PAGE_SIZE` (from `--page-size` or `@page-size`, default **4096**), then emit one complete page: `PageMagic` (`NXSP`), column group, `PageLength`, 8-byte alignment padding.
3. Repeat step 2 for each full page.
4. On close: emit the final partial page (if any records remain), write the PAX tail-index (one entry per page), then the 28-byte PAX footer (`TailIndexOffset`, `RecordCount`, `PageCount`, `PageSize`, `MagicFooter`), and set Preamble `TailPtr` to the tail-index absolute offset (non-zero).

Until step 4 completes, the file is **unsealed**: `TailPtr == 0` and `MagicFooter` is absent.

#### 4.5.2 Reader (poll while unsealed)

1. If `FLAG_PAX` and Preamble `TailPtr == 0`, treat the file as a stream (no random-access tail-index yet).
2. Scan forward from the end of the Schema Header for `PageMagic` (`0x4E585350`, `NXSP`). A page is **complete** when `PageLength` bytes are available starting at that offset (length includes header through `PageLength` field per Normative Annex A §4.2 / OLAP.md §4.2).
3. Process complete pages in order; partial trailing bytes belong to an in-progress page.
4. When `MagicFooter` (`NXS!`) is present at EOF and Preamble `TailPtr` is non-zero (or the PAX footer’s `TailIndexOffset` resolves in-bounds), the file is **sealed** — use the PAX tail-index for cross-page record lookup and column scan.

**Batch open on unsealed files.** Drivers that require a sealed tail-index (e.g. `nxs_open` loading the full tail-index at open) **MUST** fail on unsealed PAX with `ERR_BAD_MAGIC` when `MagicFooter` is missing, or `ERR_OUT_OF_BOUNDS` when the buffer is shorter than `FOOTER_PAX` (28 bytes). Incremental page polling is the supported unsealed read path.

**Columnar contrast.** `FLAG_COLUMNAR` with `TailPtr == 0` remains invalid (`ERR_INCOMPATIBLE_FLAGS`). Only PAX permits `TailPtr == 0` among OLAP layouts.

#### 4.5.3 Seal invariant

A sealed PAX file **MUST** satisfy:

- Preamble `TailPtr` equals the absolute offset of the first tail-index entry (same value as `TailIndexOffset` in the PAX footer).
- Final 4 bytes are `MagicFooter` (`0x2153584E`).
- `PageCount` in the footer equals the number of tail-index page entries.

---

## 5. Object Anatomy
Objects are the primary data container. To support sparse data (missing fields) without wasting space, objects use a **Bitmask + Offset Table** approach.

### 5.1 Object Header

| Field | Size | Description |
| :--- | :--- | :--- |
| `Magic` | 4 bytes | `0x4E59584F` (NYXO) |
| `Length` | 4 bytes | Total byte length of this object including header |
| `Bitmask` | Variable | LEB128-encoded presence mask (see 5.2) |
| `OffsetTable` | Variable | Per-present-field offsets (see 5.3) |

### 5.2 Variable-Width Bitmask
To support more than 64 keys, the bitmask uses a continuation-bit encoding (LEB128):
* The 7 least significant bits of each byte are data bits.
* The Most Significant Bit (MSB) is the **Continuation Bit**.
* If MSB = 1, the next byte is part of the mask.
* The bitmask encodes one bit per dictionary key, in dictionary order (key 0 = LSB of first byte).

### 5.3 The Offset Table
Immediately following the bitmask is the Offset Table.
* Each bit set to `1` in the mask corresponds to one entry in the Offset Table, in dictionary key order.
* **Normal Mode** (Flags Bit 0 = 0): `uint16_t` offsets (max object size 64KB).
* **Jumbo Mode** (Flags Bit 0 = 1): `uint32_t` offsets (max object size 4GB).
* Offsets are **relative to the first byte of the object header** (i.e., the `Magic` field).

### 5.4 Null Fields
A `^` (Null) field has its bitmask bit set to `1` and an entry in the Offset Table. The Null field is **zero-width**: no payload bytes are emitted at the offset; the offset table entry is present solely to distinguish an explicitly-set Null from an absent field (whose bitmask bit is `0`). Parsers **MUST** distinguish a missing bitmask bit (field absent/unknown) from a Null entry (field explicitly set to null).

> **Conformance note — Null encoding.** An earlier draft of this section stated "the offset points to a single `0x00` byte". That description was incorrect. The reference implementation (`rust/src/compiler.rs` and `rust/src/writer.rs`) encodes Null as zero-width: the bitmask bit is set and an offset-table slot is allocated, but zero payload bytes are written at that slot. Readers MUST NOT attempt to dereference the Null offset as a data byte; they MUST check the TypeManifest sigil (`^`, `0x5E`) or the calling context and return a Null sentinel without reading any payload bytes.

---

## 6. List Encoding
Lists are encoded as a typed array immediately within the Data Sector or an enclosing Object's data region.

### 6.1 List Header

| Field | Size | Description |
| :--- | :--- | :--- |
| `Magic` | 4 bytes | `0x4E59584C` (NYXL) |
| `Length` | 4 bytes | Total byte length of this list including header |
| `ElemSigil` | 1 byte | Sigil byte of all elements (uniform type enforced) |
| `ElemCount` | 4 bytes | Number of elements (`uint32_t`) |
| `Padding` | 3 bytes | `0x00` (aligns data to 8-byte boundary from Magic) |

### 6.2 List Data
Immediately follows the header. Elements are laid out contiguously, each obeying the Rule of 8 (Section 4.1). For variable-length types (String, Binary), elements are length-prefixed and tail-padded individually.

---

## 7. The Tail-Index (O(1) Record Lookup)
The Tail-Index is located at the end of the file. In legacy v1.0 files its offset is given by non-zero `TailPtr` in the Preamble. In streamable v1.1 files, `TailPtr` is `0` while records are being streamed and the final footer stores the Tail-Index offset after the writer seals the file.

| Field | Size | Description |
| :--- | :--- | :--- |
| `EntryCount` | 4 bytes | `uint32_t` total number of indexed records |
| `RecordArray` | Variable | Pairs of `KeyID (u16)` and `AbsoluteOffset (u64)`, Little-Endian |
| `FooterTailPtr` | 8 bytes | `uint64_t` absolute byte offset to the start of the Tail-Index |
| `MagicFooter` | 4 bytes | `0x2153584E` (`NXS!`) |

The final 12 bytes of a v1.1 file are always `FooterTailPtr` + `MagicFooter`. A reader that sees Preamble `TailPtr == 0` MUST read `FooterTailPtr` from `EOF - 12`; a reader that sees non-zero `TailPtr` MAY use the preamble value directly for v1.0 compatibility.

**KeyID semantics.** In the Tail-Index, `KeyID` is a zero-based `uint16_t` ordinal for the top-level record slot as written by the compiler. It mirrors the record's position modulo the `uint16_t` range and carries no meaning outside the file in which it appears. Readers locate record `i` by reading the `i`th 10-byte tail entry; they do not need to binary-search by `KeyID`.

**Ordering invariant.** The `RecordArray` **MUST** be written in top-level record order. Readers **MAY** validate the `KeyID` sequence when the record count is within the `uint16_t` range and treat a non-sequential array as `ERR_MALFORMED_INDEX`.

**Field access.** Once a record's `AbsoluteOffset` is resolved from the Tail-Index, individual field access via the object's Offset Table is O(1).

---

## 8. Advanced Operations

### 8.1 Delta Patching
Because NXS uses fixed-width cells for atomic types and length-prefixes for blobs, clients **MAY** perform in-place updates. To update a value:
1. Locate the object via the Tail-Index.
2. Identify the value offset via the Object's Offset Table.
3. Overwrite the specific bytes.
4. **Note:** If a String/Binary update exceeds the original length, the entire object **MUST** be relocated to the end of the file and the Tail-Index updated.

### 8.2 Linking (`&`)
The Link sigil `&` stores a signed 32-bit Little-Endian integer. This value is a **relative byte offset from the first byte of the `&` field's own encoded value** to the first byte of the target object's Magic header. A positive value points forward in the file; a negative value points backward.

Circular links (a chain of `&` references that resolves back to its origin) **MUST** be detected and rejected by parsers. Parsers **SHOULD** limit link-chain depth to 16 hops.

### 8.3 Compaction advisory locks

Online repack or compaction of a sealed `.nxb` file (rewriting the data sector while preserving DictHash and tail-index semantics) **MUST** coordinate with concurrent readers and writers via a sidecar lock file.

**Lock file convention.** For a file at pathname `P` ending in `.nxb`, the lock file **MUST** be `P` with the suffix `.lock` appended (e.g. `records.nxb` → `records.nxb.lock`). The lock file **MAY** be empty; its presence and an **exclusive** advisory lock held on it denote an active compaction or ingestion critical section.

**Reader behavior.** A conformant reader that opens `P` for decode **MUST** detect `P.lock` (or failure to acquire a shared/non-blocking probe of the same advisory lock). While the lock is held by another process, the reader **MUST NOT** treat partially-written or in-rename bytes as valid NXB. The reader **MUST** either (a) wait with bounded backoff and retry, (b) return `ERR_COMPACTION_LOCK`, or (c) skip the file and surface an explicit warning — silent mis-read is forbidden.

**Compactor guarantees.** A compaction implementation **MUST** hold an exclusive advisory lock on `P.lock` for the entire interval from first read of `P` through atomic replacement of `P`. Repacked bytes **MUST** be written to a distinct pathname (e.g. `P.compacting`) and committed by a single atomic rename onto `P`. The lock **MUST** be released only after the rename succeeds or the operation is aborted without changing `P`. Readers therefore never observe a partially-renamed `P` without the lock being held.

---

## 9. Security and Constraints
1. **Recursion Limit:** Conformant parsers **MUST** support at least 64 levels of nesting but **SHOULD** reject files exceeding this limit to prevent stack exhaustion.
2. **Bounds Checking:** All offsets **MUST** be validated against the total buffer size before memory access. An out-of-bounds offset is a parse error; the parser **MUST NOT** attempt recovery.
3. **Dictionary Drift:** If the `DictHash` in the Preamble does not match the expected local schema, the parser **MUST** prioritize the **Embedded Schema Header** or fail with error `ERR_DICT_MISMATCH` if none is present.
4. **Integer Overflow:** Arithmetic in Macro expressions **MUST** be performed in 64-bit signed arithmetic. Overflow is a compile error.
5. **Circular Links:** See Section 8.2.

---

## 10. Appendix A: Error Codes

| Code | Meaning |
| :--- | :--- |
| `ERR_BAD_MAGIC` | Magic bytes do not match expected value |
| `ERR_UNKNOWN_SIGIL` | Unrecognized sigil byte encountered |
| `ERR_BAD_ESCAPE` | Invalid escape sequence in string literal |
| `ERR_OUT_OF_BOUNDS` | Offset points outside the buffer |
| `ERR_MALFORMED_INDEX` | Tail-Index `RecordArray` is not in top-level record order |
| `ERR_DICT_MISMATCH` | DictHash does not match and no embedded schema present |
| `ERR_CIRCULAR_LINK` | Link chain resolves back to its origin |
| `ERR_RECURSION_LIMIT` | Nesting depth exceeds conformance limit |
| `ERR_MACRO_UNRESOLVED` | Macro expression cannot be statically resolved |
| `ERR_LIST_TYPE_MISMATCH` | List contains elements of mixed sigil types |
| `ERR_OVERFLOW` | Arithmetic overflow in Macro expression |
| `ERR_INVALID_FLAGS` | Both `FLAG_COLUMNAR` and `FLAG_PAX` set |
| `ERR_INCOMPATIBLE_FLAGS` | Invalid flag combination (e.g. `FLAG_COLUMNAR` with Preamble `TailPtr == 0`) |
| `ERR_UNSUPPORTED_LAYOUT` | Reader does not implement the requested layout |
| `ERR_UNSUPPORTED_FIELD_TYPE` | Field type not supported in columnar/PAX initial release (see Normative Annex A §Q3 / OLAP.md §Q3) |
| `ERR_INVALID_PAGE_MAGIC` | Expected `NXSP` at page boundary |
| `ERR_PAGE_CRC_MISMATCH` | PAX page CRC32 does not match (only when `FLAG_PAGE_CRC` is set) |
| `ERR_COMPACTION_LOCK` | `.nxb.lock` held or compaction in progress; reader refused to decode |

---

## 11. Appendix B: Example Encoding
**Input (.nxs):**
```text
user {
    id: =1024
    active: ?true
    name: "Alex"
}
```

**Binary Structure (.nxb):**
1. **Preamble (32B):** Magic `NYXB`, Version `0x0101`, Flags `0x0002` (Schema Embedded), DictHash, TailPtr `0x0000000000000000` for streamable v1.1, Reserved.
2. **Schema Header:** KeyCount `0x0003`, TypeManifest `[0x3D, 0x3F, 0x22]` (`=`, `?`, `"`), StringPool `"id\0active\0name\0"` + padding.
3. **Object Header:** Magic `NYXO`, Length, Bitmask `0x07` (bits 0–2 set), OffsetTable `[0x10, 0x18, 0x20]`.
4. **Data Cell 0 (id):** `0x0000000000000400` (Int64 1024, LE).
5. **Data Cell 1 (active):** `0x01` + 7 bytes `0x00` padding.
6. **Data Cell 2 (name):** Length `0x00000004` + `Alex` + 4 bytes `0x00` padding.
7. **Tail-Index:** EntryCount `0x00000001`, Record `[KeyID=0x0000, Offset=<object start>]`, FooterTailPtr, Magic `NXS!`.

---

**End of Specification**

---

## Changelog

### v1.0.0 — 2026-04-30 (First Stable Release)

- Format frozen: preamble layout, schema header, object anatomy, list encoding, and tail-index structure are normative and backwards-incompatible changes are prohibited.
- DictHash validation (MurmurHash3-64 over the schema header) is now mandatory for all conforming readers.
- TypeManifest sigils in the schema header (`=`, `~`, `?`, `"`, `@`, `<`) are now emitted with per-slot type information by the reference writer.
- Conformance corpus (`conformance/`) added: 11 positive vectors and 3 negative vectors, validated by runners for Rust, JS, Python, Go, Ruby, PHP, C, Swift, Kotlin, and C#.
- Status promoted from "Final Specification" to "Stable (v1.0)" following successful cross-language conformance suite.

### v1.1.0 — 2026-05-18 (Streamable NXB)

- Writers may emit Preamble `TailPtr == 0` and append `FooterTailPtr u64` before `MagicFooter`.
- Readers MUST resolve `TailPtr == 0` by reading `FooterTailPtr` from `EOF - 12`.
- Schema and record bytes remain unchanged, allowing incremental parsers to emit records before the Tail-Index arrives.
- No binary format changes from the pre-release draft.

### v1.2.1 — 2026-05-22 (Compaction advisory locks)

- §8.3 defines `{path}.nxb.lock` sidecar convention, reader obligations, and compactor rename guarantees.
- Appendix A adds `ERR_COMPACTION_LOCK`.

### v1.2.0 — 2026-05-21 (Columnar & PAX)

- `FLAG_COLUMNAR` (0x0001) and `FLAG_PAX` (0x0004) layouts; mutual exclusion and schema-embedded requirement (§4.3).
- PAX page-level streaming with `TailPtr == 0` until seal (§4.5); columnar rejects streaming preamble.
- Compiler `--layout` and `--page-size`; optional `FLAG_PAGE_CRC` (0x0008) disabled by default.
- OLAP error codes `ERR_INVALID_FLAGS` through `ERR_PAGE_CRC_MISMATCH` (Appendix A).
- Conformance vectors for columnar/PAX positive decode and negative flag/page/stream cases.
