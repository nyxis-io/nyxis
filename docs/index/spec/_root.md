# Spec

Subdomain: spec/
Source paths: SPEC.md, RFC.md

## TASK → LOAD

| Task | Load |
|------|------|
| Understand the binary format byte-by-byte | format.md |
| Check the exact preamble or schema header layout | format.md |
| Understand what LEB128 bitmask encoding means | format.md |
| Understand the tail-index structure for O(1) access | format.md |
| Review security constraints (recursion limit, bounds checking) | format.md |
| Understand the Rule of 8 (8-byte alignment) | format.md |
| Understand design rationale or compare NXS to Protobuf/FlatBuffers | format.md |
| Check delta-patching or link sigil semantics | format.md |
| Review error codes | format.md |

## Rooms

| Room | Source paths | Files |
|------|-------------|-------|
| format.md | SPEC.md, RFC.md | 2 |
