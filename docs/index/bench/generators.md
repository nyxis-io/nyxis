---
room: generators
subdomain: bench
source_paths: [bench/generators/, bench/generators/generated/]
see_also: ["harness.md", "scripts.md"]
hot_paths: [bench/generators/gen.py, bench/generators/transcode.py]
architectural_health: normal
security_tier: normal
---

# bench/generators/ — Workload Data Pipeline

Subdomain: bench/
Source paths: bench/generators/, bench/generators/generated/

## TASK → LOAD

| Task | Load |
|------|------|
| Regenerate canonical JSON for workloads A/B/C | generators.md |
| Transcode JSON to competitor binary formats | generators.md |
| Refresh FlatBuffers / protobuf Python stubs | generators.md |

---

# bench/generators/codegen.sh

DOES: Shell driver to regenerate flatbuffer/capnp/protobuf artifacts consumed by transcode and harnesses.
SYMBOLS:
- invokes protoc, flatc, and internal codegen steps

---

# bench/generators/gen.py

DOES: Canonical JSON generator for workloads A (dense scalars), B (flat 8-field), and C (sparse nested); fixed seed NYXI for reproducibility.
SYMBOLS:
- main()
- _rng(seed) → Random
- workload A/B/C record builders
- SELECTIVE_READ_FIELDS constant set
CONFIG: --out, --records, --workload CLI
PATTERNS: deterministic-seed

---

# bench/generators/sparse_fields.py

DOES: Defines sparse workload C field tree (meta, child, grandchild) used by gen.py and transcode.
SYMBOLS:
- (+field name lists and nesting metadata)

---

# bench/generators/transcode.py

DOES: Transcodes canonical JSON into NXB, Parquet, Cap'n Proto, FlatBuffers, protobuf, and other competitor formats for harness inputs.
SYMBOLS:
- main()
- per-format encode functions
DEPENDS: bench/generators/generated stubs
PATTERNS: two-phase-pipeline

---

# bench/generators/transcode_rust/src/main.rs

DOES: Rust transcode utility for high-throughput conversion steps in the benchmark data pipeline.
SYMBOLS:
- fn main()
- (+format writers reading JSON inputs)

---

# bench/generators/generated/dense8_pb2.py

DOES: Auto-generated protobuf Python module for dense 8-field workload messages.
SYMBOLS:
- (+protobuf message classes)
USE WHEN: Python transcode needs protobuf bindings; regenerate via codegen.sh

---

# bench/generators/generated/flat8_pb2.py

DOES: Auto-generated protobuf module for flat-8 workload records.
SYMBOLS:
- (+protobuf classes)

---

# bench/generators/generated/sparse_pb2.py

DOES: Auto-generated protobuf module for sparse nested workload C.
SYMBOLS:
- (+protobuf classes)

---

# bench/generators/generated/nyxis/__init__.py

DOES: Package marker for generated FlatBuffers Python bindings under nyxis namespace.
SYMBOLS:
- (package init)

---

# bench/generators/generated/nyxis/bench/Dense8File.py

DOES: Generated FlatBuffers table type for dense-8 file root.
SYMBOLS:
- Dense8File FlatBuffers accessors

---

# bench/generators/generated/nyxis/bench/Dense8Record.py

DOES: Generated FlatBuffers struct for a single dense-8 record.
SYMBOLS:
- Dense8Record accessors

---

# bench/generators/generated/nyxis/bench/Flat8File.py

DOES: Generated FlatBuffers root for workload B flat file layout.
SYMBOLS:
- Flat8File accessors

---

# bench/generators/generated/nyxis/bench/Flat8Record.py

DOES: Generated FlatBuffers record type for 8-field flat rows.
SYMBOLS:
- Flat8Record accessors

---

# bench/generators/generated/nyxis/bench/SparseChild.py

DOES: Generated FlatBuffers nested child table for sparse workload.
SYMBOLS:
- SparseChild accessors

---

# bench/generators/generated/nyxis/bench/SparseFile.py

DOES: Generated FlatBuffers sparse file root with nested records.
SYMBOLS:
- SparseFile accessors

---

# bench/generators/generated/nyxis/bench/SparseGrandchild.py

DOES: Generated FlatBuffers grandchild table in sparse hierarchy.
SYMBOLS:
- SparseGrandchild accessors

---

# bench/generators/generated/nyxis/bench/SparseMeta.py

DOES: Generated FlatBuffers meta header for sparse records.
SYMBOLS:
- SparseMeta accessors

---

# bench/generators/generated/nyxis/bench/SparseRecord.py

DOES: Generated FlatBuffers top-level sparse record combining meta/child trees.
SYMBOLS:
- SparseRecord accessors

---

# bench/generators/generated/nyxis/bench/__init__.py

DOES: Subpackage init for generated nyxis.bench FlatBuffers modules.
SYMBOLS:
- (package init)
