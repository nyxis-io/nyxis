```
RFC NXS-001
Title: The Nyxis (NXS) Serialization Format
Date: 2026-04-30
Status: Stable v1.1
Editors: Micael Malta
```

---

# RFC NXS-001: The Nyxis (NXS) Serialization Format

**Date:** 2026-04-30
**Status:** Stable v1.1
**Editors:** Micael Malta
**MIME Types:** `application/nxb` (binary), `application/nxs` (text source)
**File Extensions:** `.nxb`, `.nxs`

---

## Table of Contents

1. [Abstract](#1-abstract)
2. [Status of This Document](#2-status-of-this-document)
3. [Terminology](#3-terminology)
4. [Motivation and Design Goals](#4-motivation-and-design-goals)
5. [The Source Format (.nxs)](#5-the-source-format-nxs)
6. [The Binary Format (.nxb)](#6-the-binary-format-nxb)
7. [Memory Alignment](#7-memory-alignment)
8. [The Tail-Index](#8-the-tail-index)
9. [Delta Patching](#9-delta-patching)
10. [Advanced Features](#10-advanced-features)
11. [Error Codes](#11-error-codes)
12. [Security Considerations](#12-security-considerations)
13. [Implementation Guidance](#13-implementation-guidance)
14. [Comparison with Related Work](#14-comparison-with-related-work)
15. [IANA Considerations](#15-iana-considerations)
16. [References](#16-references)
17. [Acknowledgements](#17-acknowledgements)

---

## 1. Abstract

The Nyxis (NXS) is a high-performance, bi-modal data serialization format designed for environments where parse latency, memory overhead, and random-access performance are first-class concerns. NXS exists in two complementary representations: a human-readable UTF-8 source format (`.nxs`) that is authored and reviewed by developers, and a compiled binary format (`.nxb`) that is consumed at runtime without full deserialization.

NXS addresses a gap in the existing serialization landscape. Human-readable formats such as JSON impose an O(n) parsing tax before any data can be accessed and carry no mechanism for random record lookup. Schema-driven binary formats such as Protocol Buffers and FlatBuffers deliver excellent performance but require generated code, external schema registries, and non-trivial build tooling. NXS combines the authoring ergonomics of a text format with the runtime characteristics of a memory-mapped binary format: records are located in O(1) time via a Tail-Index positional lookup, individual fields within a located record are O(1) via the Offset Table, aligned atomic values support zero-copy access via 8-byte alignment, and the embedded schema header eliminates out-of-band schema management for most use cases.

This document specifies the NXS source syntax, the `.nxb` binary wire format, the Tail-Index structure, the delta-patching protocol, and the security requirements that conformant implementations must satisfy. It also provides implementation guidance and a comparison with related serialization formats.

---

## 2. Status of This Document

This document defines the **Stable v1.1** specification for the NXS format. The v1.1 revision preserves the 32-byte preamble and adds a streamable sealed-file footer so readers can parse schema and records before the Tail-Index is known.

The specification has been validated by a cross-language conformance suite (11 positive vectors, 3 negative vectors) across ten reference implementations: Rust, JavaScript, Python, Go, Ruby, PHP, C, Swift, Kotlin, and C#.

Readers are encouraged to report ambiguities or implementation difficulties. Clarifications that do not alter wire format may be incorporated as v1.x errata without a version bump.

---

## 3. Terminology

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in **RFC 2119**.

The following NXS-specific terms are used throughout this document:

- **Sigil**: A single character (or two-character token) that immediately precedes a value in the `.nxs` source format and unambiguously declares its machine type. The sigil determines both how the compiler encodes the value and how the parser interprets the corresponding binary cell.

- **Slot**: A single typed value cell within the data region of a binary object, addressable via the object's Offset Table.

- **Bitmask**: The variable-width LEB128-encoded presence mask that appears in every object header. A set bit indicates that the corresponding dictionary key is present in this object instance; a clear bit indicates absence. The Bitmask enables sparse objects without wasted space.

- **Tail-Index**: The index structure located at the end of every `.nxb` file that maps record positions to their absolute byte offsets, enabling indexed record lookup (O(1) positional access) without scanning the Data Sector.

- **Uniform Schema**: A schema in which every record in the Data Sector carries the same set of keys with the same sigil types. The Uniform Schema fast path allows implementations to skip per-object schema resolution after reading the first record.

- **Normal Mode**: The default object encoding mode, selected when Preamble Flags Bit 0 is clear. Offset Table entries are `uint16_t` (2 bytes), constraining individual objects to a maximum encoded size of 64 KB.

- **Jumbo Mode**: An extended object encoding mode, selected when Preamble Flags Bit 0 is set. Offset Table entries are `uint32_t` (4 bytes), raising the per-object size limit to 4 GB.

- **DictHash**: The 64-bit MurmurHash3 digest of the Schema Header bytes, stored in the Preamble. Used to detect schema drift when a reader applies an externally cached schema to a file.

---

## 4. Motivation and Design Goals

### 4.1 Limitations of Existing Formats

**JSON** is ubiquitous and human-readable but imposes significant runtime costs. The entire document must be parsed before the first value can be accessed. String-based representation carries no type information beyond what can be inferred heuristically. Browser implementations impose a practical string length ceiling of approximately 512 MB. There is no mechanism for random record access; locating the n-th record requires reading all preceding records.

**Protocol Buffers and FlatBuffers** offer excellent binary performance and strong typing. However, both require an external schema file, a code generation step, and integration of a non-trivial toolchain into the build pipeline. The schema is not embedded in the data stream; schema and data files can drift out of sync. FlatBuffers provides O(1) field access within a single object but does not index across records in a multi-record file.

**CSV** is minimal and widely supported but provides no type information, relies on brittle delimiter escaping, and supports neither nested structures nor random access. Column semantics are defined entirely by convention, with no enforcement mechanism.

**MessagePack** eliminates the text parsing overhead of JSON and adds a compact type system, but retains the sequential parse model: a reader must scan from the beginning of the file to reach a specific record. There is no tail-index or equivalent structure.

### 4.2 NXS Design Goals

The following goals are listed in priority order:

1. **O(1) record lookup via Tail-Index.** A reader MUST be able to locate any top-level record by seeking to `EOF - 12` and following the Tail-Index without scanning the Data Sector after the file is sealed. Field access within a located record is O(1) via the Offset Table.

2. **Zero-copy access for aligned atomic values via 8-byte alignment.** Atomic values (Int64, Float64, Time) are stored at file offsets satisfying `offset mod 8 = 0`. A memory-mapped view of a `.nxb` file can be cast to typed pointers for these values without copying.

3. **Human-readable source format.** The `.nxs` source format is UTF-8 text authored by humans and compiled to `.nxb` by a NXS compiler. The source format is the authoritative representation for version control and code review.

4. **Schema-optional via embedded header.** The Schema Header is embedded in the `.nxb` file when Preamble Flags Bit 1 is set. Readers do not require an out-of-band schema file for the common case.

5. **In-place delta patching for live data.** Fixed-width atomic slots can be overwritten in place. Variable-length slots that grow beyond their original allocation trigger object relocation and Tail-Index update, but no full file rewrite is required.

6. **Sparse data support via Bitmask.** Absent fields consume no space in the Data Sector. The LEB128-encoded Bitmask scales to schemas with more than 64 keys without a fixed-size header.

---

## 5. The Source Format (.nxs)

### 5.1 Sigil Table

Every value in the `.nxs` format **MUST** be prefixed with a Sigil that declares its machine type. The full sigil table is as follows:

| Sigil | Type | Description | Binary Encoding |
| :--- | :--- | :--- | :--- |
| `=` | **Int64** | Signed 64-bit integer | `int64_t` (Little-Endian) |
| `~` | **Float64** | 64-bit floating point | `double` (IEEE 754, Little-Endian) |
| `?` | **Bool** | Truth value | `uint8_t` (`0x01` or `0x00`) |
| `$` | **Keyword** | Dictionary-interned key | `uint16_t` (Dict Index, Little-Endian) |
| `"` | **String** | UTF-8 text | `uint32_t` (length, LE) + bytes |
| `@` | **Time** | Unix nanoseconds | `int64_t` (Little-Endian) |
| `<>` | **Binary** | Raw byte stream | `uint32_t` (length, LE) + bytes |
| `&` | **Link** | Relative pointer | `int32_t` (byte offset, LE) |
| `!` | **Macro** | Compile-time formula | Resolved to base type at compile time |
| `^` | **Null** | Explicit absent value | No payload; bitmask bit set, zero-width |

### 5.2 Syntax Structure

**Objects** are delimited by braces (`{}`). Object keys do not require quoting unless they contain whitespace or any of the characters `{}[]:"`. Key-value pairs are separated by whitespace; the colon between key and value is required.

**Lists** are delimited by brackets (`[]`). Elements are comma-separated. All elements within a single list **MUST** carry the same sigil type; mixed-type lists are a compile error (`ERR_LIST_TYPE_MISMATCH`).

**Null** is expressed as the `^` sigil standing alone, with no following value token.

**Nesting** is unrestricted in the source format, but implementations **MUST** enforce the recursion limit specified in Section 12.

### 5.3 String Escape Sequences

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

Any character following `\` that is not listed above is a parse error (`ERR_BAD_ESCAPE`). Parsers **MUST NOT** silently ignore unknown escape sequences.

### 5.4 Macro Expressions

A Macro value (sigil `!`) is a compile-time expression resolved by the NXS compiler before binary output is produced. The expression language supports:

- **Literals:** Any base sigil value (e.g., `=10`, `~3.14`, `"hello"`).
- **Arithmetic:** `+`, `-`, `*`, `/`, `%` over numeric types.
- **String concatenation:** `+` over two String operands.
- **References:** `@key` dereferences another key in the same object scope.
- **Built-in functions:** `now()` (current Unix nanoseconds as Int64), `len(@key)` (byte length of a String or Binary value).

Macros **MUST** be fully resolved at compile time. A Macro that cannot be statically resolved — for example, one that references a value computed at runtime — is a compile error (`ERR_MACRO_UNRESOLVED`). The resolved value is encoded using its resulting base sigil type. Arithmetic overflow in Macro expressions is a compile error (`ERR_OVERFLOW`); overflow detection **MUST** use 64-bit signed arithmetic.

**Example:**

```text
config {
    base_url: "https://api.example.com"
    version: =2
    endpoint: !"@base_url/v" + @version
}
```

---

## 6. The Binary Format (.nxb)

The `.nxb` binary format is designed for memory mapping with zero-copy access to aligned atomic values (Int64, Float64, Time). Variable-length values (String, Binary) require a length-prefixed read but no copy of the payload; decoded string interpretation (e.g. `TextDecoder`) is a separate concern handled by the reader when materializing strings. All multi-byte integer fields use **Little-Endian** byte order. NXS does not support Big-Endian byte order and provides no endianness negotiation mechanism. Readers on Big-Endian architectures MUST byte-swap multi-byte fields after reading.

### 6.1 File Layout

A `.nxb` file consists of four segments in the following order:

```
[Preamble 32 bytes][Schema Header][Data Sector][Tail-Index]
```

The Preamble is exactly 32 bytes. The Schema Header is present only when Preamble Flags Bit 1 is set. The Data Sector contains one or more encoded objects and lists. The Tail-Index follows the Data Sector and is located by the reader via a non-zero `TailPtr` field in the Preamble or, for streamable v1.1 files, via the final footer tail pointer (see Section 8).

### 6.2 Preamble (Exactly 32 Bytes)

| Offset | Size | Field | Description |
| :--- | :--- | :--- | :--- |
| 0 | 4 | `Magic` | `0x4E595842` (`NYXB`) |
| 4 | 2 | `Version` | `0x0101` (major=1, minor=1) |
| 6 | 2 | `Flags` | Bit 0: Jumbo Offsets; Bit 1: Schema Embedded; Bits 2–15: reserved (MUST be `0x00`) |
| 8 | 8 | `DictHash` | 64-bit MurmurHash3 of the Schema Header bytes |
| 16 | 8 | `TailPtr` | Absolute byte offset to the Tail-Index; `0` means streamable v1.1 and the sealed footer carries the Tail-Index offset |
| 24 | 8 | `Reserved` | MUST be `0x00` |

Readers **MUST** verify that `Magic` equals `0x4E595842` before processing any other field. A mismatch is `ERR_BAD_MAGIC`. Reserved bits in `Flags` **MUST** be zero; a reader encountering a non-zero reserved bit **SHOULD** reject the file as it may indicate a format extension the reader does not support.

### 6.3 Schema Header

The Schema Header is present immediately after the Preamble when Flags Bit 1 is set.

| Field | Type | Description |
| :--- | :--- | :--- |
| `KeyCount` | `uint16_t` | Number of keys in the dictionary |
| `TypeManifest` | `uint8_t[KeyCount]` | Sigil byte for each key, in dictionary order |
| `StringPool` | UTF-8 bytes | Null-terminated key name strings, concatenated |

The `StringPool` **MUST** be padded with zero bytes to the next 8-byte boundary after the last null terminator. The `DictHash` in the Preamble is the 64-bit MurmurHash3 of the complete Schema Header bytes, including padding.

### 6.4 Object Anatomy

Objects are the primary data container in the Data Sector. Each object begins with a fixed-structure header followed by variable-width Bitmask and Offset Table fields.

#### 6.4.1 Object Header

| Field | Size | Description |
| :--- | :--- | :--- |
| `Magic` | 4 bytes | `0x4E59584F` (`NYXO`) |
| `Length` | 4 bytes | Total byte length of this object including header (`uint32_t`) |
| `Bitmask` | Variable | LEB128-encoded presence mask (see Section 6.4.2) |
| `OffsetTable` | Variable | Per-present-field offsets (see Section 6.4.3) |

#### 6.4.2 Variable-Width Bitmask

The Bitmask uses LEB128 continuation-bit encoding to support schemas with more than 64 keys:

- The 7 least-significant bits of each byte are data bits.
- The most-significant bit (MSB) is the continuation bit.
- If MSB = 1, the following byte is part of the mask.
- If MSB = 0, the current byte is the last byte of the mask.
- Bit 0 of the first byte corresponds to dictionary key 0 (the LSB-first convention).

A set bit indicates that the corresponding dictionary key is present in this object instance and has an entry in the Offset Table. A clear bit indicates the key is absent.

#### 6.4.3 The Offset Table

The Offset Table immediately follows the Bitmask. It contains one entry for each bit that is set to `1` in the Bitmask, in dictionary key order.

- **Normal Mode** (Flags Bit 0 = 0): each entry is a `uint16_t` (2 bytes). Maximum addressable object size is 64 KB.
- **Jumbo Mode** (Flags Bit 0 = 1): each entry is a `uint32_t` (4 bytes). Maximum addressable object size is 4 GB.

All offsets are relative to the first byte of the object's `Magic` field. Readers **MUST** validate that every offset lies within the object's declared `Length` before dereferencing.

#### 6.4.4 Null Fields

A field carrying the `^` (Null) sigil has its Bitmask bit set to `1` and possesses an entry in the Offset Table. The entry points to a single `0x00` byte. Parsers **MUST** distinguish a missing bit (key is absent; the field was never written) from a Null entry (key is present; the field was explicitly set to null). These are semantically distinct states.

### 6.5 List Encoding

Lists are encoded as typed arrays within the Data Sector or within an enclosing object's data region.

#### 6.5.1 List Header

| Field | Size | Description |
| :--- | :--- | :--- |
| `Magic` | 4 bytes | `0x4E59584C` (`NYXL`) |
| `Length` | 4 bytes | Total byte length of this list including header (`uint32_t`) |
| `ElemSigil` | 1 byte | Sigil byte of all elements (uniform type enforced) |
| `ElemCount` | 4 bytes | Number of elements (`uint32_t`) |
| `Padding` | 3 bytes | `0x00` (aligns list data to an 8-byte boundary from `Magic`) |

#### 6.5.2 List Data

List elements immediately follow the header. Elements are laid out contiguously, each satisfying the Rule of 8 (Section 7). For variable-length types (String, Binary), each element is individually length-prefixed and tail-padded to the next 8-byte boundary.

---

## 7. Memory Alignment

### 7.1 The Rule of 8

All atomic value cells (Int64, Float64, Time) **MUST** begin at a file offset satisfying:

```
offset mod 8 = 0
```

The NXS compiler **MUST** insert zero bytes (`0x00`) as padding ahead of each such value to enforce this invariant. String and Binary values are length-prefixed; after the data bytes, the compiler **MUST** insert zero bytes to pad the total encoded size (4-byte length prefix + data bytes) to the next 8-byte boundary, ensuring that the value beginning immediately after is also aligned.

Bool values occupy 1 byte of payload. The compiler **MUST** insert 7 bytes of zero padding after each Bool cell.

### 7.2 Rationale

Processors and SIMD instruction sets (SSE2, AVX2, NEON) perform their most efficient loads when the source address is a multiple of the native word width. A memory-mapped `.nxb` file whose atomic values are 8-byte-aligned can be read with direct pointer casts on any conformant architecture. This eliminates the copy-to-aligned-buffer step that would otherwise be required before arithmetic operations, and enables auto-vectorized loops over list elements. The 7-byte Bool padding wastes space for individual Bool fields but preserves the alignment invariant uniformly across all types.

---

## 8. The Tail-Index

### 8.1 Structure

The Tail-Index is located at the absolute byte offset given by non-zero `TailPtr` in the Preamble, or by `FooterTailPtr` in streamable v1.1 files where the preamble `TailPtr` is `0`. It consists of the following fields:

| Field | Size | Description |
| :--- | :--- | :--- |
| `EntryCount` | 4 bytes | `uint32_t` total number of indexed records |
| `RecordArray` | Variable | Pairs of `KeyID` (`uint16_t`) and `AbsoluteOffset` (`uint64_t`), Little-Endian |
| `FooterTailPtr` | 8 bytes | `uint64_t` absolute byte offset to the start of the Tail-Index |
| `MagicFooter` | 4 bytes | `0x2153584E` (`NXS!`) |

The final 12 bytes of a valid v1.1 `.nxb` file are always `FooterTailPtr` followed by `MagicFooter`. A reader **MUST** use `FooterTailPtr` when the Preamble `TailPtr` is `0`, which allows records to be parsed while bytes are still downloading and random-access indexing to be enabled after the footer arrives.

**KeyID semantics.** Each Tail-Index `KeyID` is a zero-based `uint16_t` ordinal for the top-level record slot in the order it was written by the compiler. It mirrors the record position modulo the `uint16_t` range and is not stable across files. Readers locate record `i` by reading the `i`th 10-byte Tail-Index entry and using its `AbsoluteOffset`; they do not need to binary-search by `KeyID`.

**RecordArray ordering.** The `RecordArray` **MUST** be written in top-level record order. Readers **MAY** validate the `KeyID` sequence when the record count is within the `uint16_t` range and treat a non-sequential array as `ERR_MALFORMED_INDEX`.

### 8.2 Record Access Protocol

To access record `i`:

1. Seek to `EOF - 12`. Verify the final `MagicFooter` equals `0x2153584E`.
2. Read `FooterTailPtr` and seek to that absolute offset to reach the start of the Tail-Index.
3. Read `EntryCount`, then seek directly to Tail-Index entry `i` at `TailPtr + 4 + i * 10`.
4. Follow `AbsoluteOffset` directly to the object's `Magic` field (`NYXO`) in the Data Sector.

Steps 1–3 touch only the Tail-Index region regardless of Data Sector size; step 4 is a single seek. No portion of the Data Sector is read until the target record is accessed. The cost of locating any record is O(1).

---

## 9. Delta Patching

Because NXS uses fixed-width cells for atomic types and explicit length prefixes for variable-length types, clients **MAY** perform in-place updates without rewriting the full file.

**In-place update protocol:**

1. Locate the target object via the Tail-Index (Section 8.2).
2. Verify the object `Magic` (`NYXO`) and validate `Length`.
3. Read the Bitmask to confirm the target key is present.
4. Look up the key's Offset Table entry to obtain the slot's byte offset within the object.
5. For atomic types (Int64, Float64, Bool, Time, Keyword, Link): overwrite the slot bytes in place. The slot width is fixed and no padding adjustment is required.
6. For variable-length types (String, Binary): compare the new byte length with the existing length prefix.
   - If the new length is less than or equal to the existing length, overwrite the data bytes and length prefix in place. Tail padding may be adjusted, but the total encoded cell size must remain the same to preserve alignment of subsequent slots.
   - If the new length exceeds the existing length, the object **MUST** be relocated: append the entire updated object at the end of the file (before the Tail-Index, which must be rewritten), and update the corresponding `AbsoluteOffset` entry in the Tail-Index.

Relocating an object does not invalidate other objects. The original object bytes **SHOULD** be zeroed to avoid stale data being misinterpreted if the file is inspected without the Tail-Index.

---

## 10. Advanced Features

### 10.1 Link Sigil (`&`)

The Link sigil encodes a signed 32-bit Little-Endian integer that is a **relative byte offset**. The offset is measured from the first byte of the Link field's own encoded value (i.e., the byte at the slot's offset-table address) to the first byte of the target object's `Magic` field. A positive value points forward in the file; a negative value points backward.

Links enable shared sub-structures and cross-references within a `.nxb` file without duplicating data. Parsers that dereference Links **MUST** detect circular chains (a chain that resolves back to its own origin) and reject them with `ERR_CIRCULAR_LINK`. Parsers **SHOULD** limit link-chain depth to 16 hops as a defense against deeply nested but non-circular chains that could cause excessive recursion.

### 10.2 Macro Expressions (`!`)

Macros are a compile-time feature of the `.nxs` source format. They are fully resolved by the NXS compiler and do not appear in the `.nxb` binary; the binary contains only the resolved value encoded in its base type. Refer to Section 5.4 for the full Macro expression language.

### 10.3 Null vs. Absent

NXS distinguishes two distinct states for a key that has no value:

- **Absent** (Bitmask bit = 0, no Offset Table entry): The key was never written to this object instance. A reader querying this key **SHOULD** treat it as unknown or default, not as null.
- **Null** (Bitmask bit = 1, Offset Table entry pointing to `0x00`): The key was explicitly set to the null value `^`. A reader **MUST** return a typed null, not a default.

Implementations that conflate these two states are non-conformant.

---

## 11. Error Codes

Conformant implementations **MUST** surface errors through a mechanism that communicates at minimum the error code. The following codes are defined:

| Code | Meaning |
| :--- | :--- |
| `ERR_BAD_MAGIC` | Magic bytes do not match the expected value |
| `ERR_UNKNOWN_SIGIL` | An unrecognized sigil byte was encountered during parsing |
| `ERR_BAD_ESCAPE` | An invalid escape sequence was encountered in a string literal |
| `ERR_OUT_OF_BOUNDS` | An offset points outside the bounds of the buffer |
| `ERR_MALFORMED_INDEX` | The Tail-Index `RecordArray` is not in top-level record order |
| `ERR_DICT_MISMATCH` | The `DictHash` does not match and no embedded Schema Header is present |
| `ERR_CIRCULAR_LINK` | A Link chain resolves back to its own origin |
| `ERR_RECURSION_LIMIT` | Nesting depth exceeds the conformance limit (64 levels) |
| `ERR_MACRO_UNRESOLVED` | A Macro expression cannot be statically resolved at compile time |
| `ERR_LIST_TYPE_MISMATCH` | A List contains elements with differing sigil types |
| `ERR_OVERFLOW` | Arithmetic overflow occurred in a Macro expression |

---

## 12. Security Considerations

Implementations that process untrusted `.nxb` files **MUST** apply the following mitigations.

**Bounds checking.** Every offset read from an Offset Table, the Tail-Index, or any other in-file pointer structure **MUST** be validated against the total buffer size before the corresponding memory access is performed. An out-of-bounds offset is `ERR_OUT_OF_BOUNDS`; the parser **MUST NOT** attempt recovery or partial reads.

**Recursion limits.** NXS supports arbitrarily nested objects. A conformant parser **MUST** support at least 64 levels of nesting and **MUST** reject files that exceed this depth, raising `ERR_RECURSION_LIMIT`. This prevents stack exhaustion attacks via pathologically deep structures.

**Circular link detection.** Because Links use relative offsets, a malicious file may construct a cycle. Parsers **MUST** track the set of byte offsets visited during Link resolution and raise `ERR_CIRCULAR_LINK` if any offset is visited twice. The recommended link-chain hop limit of 16 provides an additional low-cost guard.

**Integer overflow in Macro expressions.** The NXS compiler processes Macro arithmetic in 64-bit signed arithmetic. Any operation that would produce a value outside the range `[−2^63, 2^63 − 1]` **MUST** be treated as a compile error (`ERR_OVERFLOW`), not silently truncated.

**Malformed LEB128.** The LEB128-encoded Bitmask may contain a continuation-bit sequence that never terminates. Parsers **MUST** impose an upper bound on the number of LEB128 continuation bytes accepted, consistent with the maximum schema key count supported by the implementation. An unterminated sequence is `ERR_OUT_OF_BOUNDS`.

**Dictionary drift attack.** If a reader applies an externally cached schema to a `.nxb` file whose `DictHash` does not match that schema's hash, the reader will silently misinterpret field offsets. Parsers **MUST** verify `DictHash` against the cached schema before using it. If the hash does not match, the parser **MUST** use the embedded Schema Header (if present) or fail with `ERR_DICT_MISMATCH`.

---

## 13. Implementation Guidance

### 13.1 Recommended Parse Strategy

The recommended read path for a `.nxb` consumer is:

1. Memory-map (or read into a buffer) the entire file.
2. Validate the Preamble Magic, Version, and Flags.
3. If Flags Bit 1 is set, parse and cache the Schema Header. Verify `DictHash`.
4. Seek to the Tail-Index via non-zero `TailPtr` or, for streamable v1.1 files, via `FooterTailPtr` at `EOF - 12`.
5. Access records on demand by reading the desired 10-byte Tail-Index entry and following its `AbsoluteOffset` directly to the object layout.

This strategy reads only the Preamble, Schema Header, and requested Tail-Index entries before touching record data. The Data Sector is accessed lazily, at the granularity of individual records. For a file with N records, the cost of record lookup is O(1), independent of record data size.

### 13.2 Uniform Schema Fast Path

When all objects in the Data Sector share the same key set (Uniform Schema), the Bitmask and Offset Table for every record after the first are identical. Implementations **MAY** detect this condition by comparing each object's Bitmask to the first object's Bitmask, then cache the Offset Table and skip per-record Bitmask/Offset parsing entirely. This fast path reduces per-record overhead to a single bounds-checked pointer arithmetic step.

### 13.3 SharedArrayBuffer Compatibility

In browser environments, `TextDecoder` rejects `SharedArrayBuffer`-backed `TypedArray` views. Implementations that use `SharedArrayBuffer` to share a `.nxb` memory-mapped view across workers **MUST** copy the Schema Header bytes into a private `ArrayBuffer` before passing them to `TextDecoder` for key-name decoding. The Data Sector itself does not require `TextDecoder` for atomic types; only String and Binary values require copying out of the shared buffer for safe processing.

### 13.4 WASM Reducers for Aggregate Queries

For aggregate operations (sum, count, filter) over large Data Sectors, implementations **SHOULD** consider compiling tight inner loops to WebAssembly. Because NXS atomic values are 8-byte-aligned, a WASM module can perform SIMD-vectorized passes over contiguous list elements or repeated fields without any data realignment. This is particularly effective for the uniform-schema case where field offsets are constant across records.

---

## 14. Comparison with Related Work

| Property | NXS | JSON | Protobuf | FlatBuffers | CSV |
|---|---|---|---|---|---|
| Human-readable source | Yes | Yes | No | No | Yes |
| Zero-copy read | Yes | No | No | Yes | No |
| Indexed record access | O(1) via Tail-Index | O(N) | O(N) | No standard index† | O(N) |
| O(1) field access (within record) | Yes | No | No | Yes | No |
| Schema required | Optional | No | Yes | Yes | No |
| In-place patching | Yes | No | No | No | No |
| Browser-safe above 512 MB | Yes | No | Yes | Yes | No |
| Typed values | Yes | Partial | Yes | Yes | No |

† FlatBuffers provides O(1) field access within a single object but does not define a cross-record index in the format itself.

### 14.1 NXS vs. Columnar Formats (Arrow, Parquet)

The two most common points of comparison in the high-performance data space are Apache Arrow (in-memory columnar) and Apache Parquet (disk-oriented columnar). NXS occupies a different position from both.

**Apache Arrow** is an in-memory columnar format designed for analytical workloads: vectorized aggregates, cross-process data sharing via IPC, and zero-copy interchange between analytical engines. Its columnar layout makes column scans extremely fast but row (record) access requires reconstructing a row from N column buffers. Arrow also requires a runtime library; there is no stable on-disk format in the base Arrow spec (Arrow IPC is not a general-purpose file format).

**Apache Parquet** is a disk-oriented columnar format with rich encoding schemes (dictionary, RLE, delta) tuned for compressed analytical reads. Like Arrow, it favors column scans over row access. Parquet is optimized for write-once, read-many analytics pipelines where schema is known up front and a query planner is available.

**NXS** is row-oriented. It is designed for workloads where the unit of access is a record (or a field within a record), not a column over all records. Its key differentiators relative to Arrow and Parquet are:

- **Human-authored source.** `.nxs` files are written by hand and committed to version control; Arrow and Parquet have no equivalent authoring format.
- **Random row access without a query engine.** The tail-index gives any reader O(1) access to any single record with no columnar reconstruction step and no engine dependency.
- **In-place mutation.** Fixed-width atomic slots can be overwritten without rewriting the file; Arrow and Parquet are append-only.
- **Multi-language, zero-dependency readers.** Each NXS reader is a single-file library with no schema registry or code generation requirement.

The trade-off is that NXS is slower than Arrow or Parquet for full-column aggregates over large datasets, because values are interleaved by row rather than laid out contiguously by column. NXS is the better choice when records are the natural unit of work — configuration, event logs, entity stores — and the better choice in constrained environments (browser, embedded, CLI tools) where pulling in a full analytical engine is not acceptable.

---

## 15. IANA Considerations

This document requests registration of the following media types:

**`application/nxb`**
- Type name: application
- Subtype name: nxb
- Required parameters: none
- Optional parameters: none
- Encoding considerations: binary
- Security considerations: see Section 12 of this document
- File extension: `.nxb`

**`application/nxs`**
- Type name: application
- Subtype name: nxs
- Required parameters: none
- Optional parameters: none
- Encoding considerations: UTF-8 text
- Security considerations: macro expressions are resolved at compile time; `.nxs` files are not directly executable
- File extension: `.nxs`

---

## 16. References

**[RFC2119]** Bradner, S., "Key words for use in RFCs to Indicate Requirement Levels", BCP 14, RFC 2119, March 1997.

**[IEEE754]** IEEE, "IEEE Standard for Floating-Point Arithmetic", IEEE Std 754-2019, July 2019.

**[LEB128]** DWARF Debugging Information Format, Version 4, Section 7.6: "Variable Length Data". Free Standards Group, 2010.

**[MURMUR3]** Appleby, A., "MurmurHash3", 2011. Available at: https://github.com/aappleby/smhasher

---

## 17. Acknowledgements

The NXS format specification was developed as a proof-of-concept exploration of the design space between human-authored text formats and high-performance binary formats. The design draws on lessons from Protocol Buffers, FlatBuffers, MessagePack, and Cap'n Proto. The sigil-based type system is an original contribution of this work.

---

*End of RFC NXS-001*
