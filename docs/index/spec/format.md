---
room: spec/format
source_paths: [SPEC.md, RFC.md]
file_count: 2
architectural_health: normal
security_tier: normal
hot_paths: [SPEC.md]
see_also: [implementations/rust.md]
---

# RFC.md

DOES: Formal RFC document for the Nyxis: design motivation, implementation guidance, security requirements, and comparison with JSON/Protobuf/FlatBuffers. Complements SPEC.md with rationale and edge-case discussion.
SYMBOLS:
- §1 Abstract
- §2 Status of This Document
- §3 Terminology — Sigil, Slot, Bitmask, Tail-Index, Uniform Schema, DictHash
- §4 Motivation and Design Goals
- §5 The Source Format (.nxs) — Sigil Table, Syntax, String Escapes, Macros
- §6 The Binary Format (.nxb) — File Layout, Preamble, Schema Header, Object Anatomy, List Encoding
- §7 Memory Alignment — The Rule of 8, Rationale
- §8 The Tail-Index — Structure, O(1) Access Protocol
- §9 Delta Patching — In-place Update Protocol
- §10 Advanced Features — Link Sigil, Macro Expressions, Null vs. Absent
- §11 Error Codes
- §12 Security Considerations — Bounds Checking, Recursion Limits, Circular Links, Overflow
- §13 Implementation Guidance — Parse Strategy, Uniform Schema Fast Path, SharedArrayBuffer, WASM
- §14 Comparison with Related Work
- §15 IANA Considerations
PATTERNS: binary-format-spec, zero-copy-reads, delta-patching, embedded-schema, security-constraints
USE WHEN: Reviewing design rationale, understanding security requirements, comparing NXS vs other formats, or clarifying ambiguous spec behaviour.

---

# SPEC.md

DOES: Ground-truth formal specification for the NXS binary wire format (v1.1): all sigils, encoding rules, binary layout, streamable TailPtr=0 sealed footer, alignment constraints, object/list headers, tail-index structure, error codes, and an annotated example encoding.
SYMBOLS:
- §3 Source Format — Sigil table (=~?$"@<>&!^), string escapes, macros, structure
- §4 Binary Format — Memory alignment (Rule of 8), file layout [Preamble][Schema][Data][Tail-Index]
- §4.2.1 Preamble — Magic 0x4E595842, Version, Flags, DictHash, TailPtr, Reserved
- §4.2.2 Schema Header — KeyCount, TypeManifest, StringPool
- §5 Object Anatomy — Magic 0x4E59584F, Length, LEB128 Bitmask, Offset Table, Null Fields
- §6 List Encoding — Magic 0x4E59584C, ElemSigil, ElemCount, uniform-type constraint
- §7 Tail-Index — EntryCount, (KeyID u16, AbsoluteOffset u64) pairs, FooterTailPtr, Magic 0x2153584E
- §8 Advanced — Delta Patching, Link sigil (&), circular-link detection
- §9 Security — Recursion limit (64), bounds checking, DictHash validation, integer overflow
- §10 Error Codes — ERR_BAD_MAGIC, ERR_OUT_OF_BOUNDS, ERR_DICT_MISMATCH, etc.
- §11 Example Encoding — Annotated user{id,active,name} binary walkthrough
PATTERNS: binary-format-spec, leb128-bitmask, rule-of-8-alignment, tail-index-lookup
USE WHEN: Implementing any NXS reader or writer; the authoritative reference for byte-level format decisions.
