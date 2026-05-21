# Workload A — Sparse hierarchical records (methodology template)

**Status:** Draft — freeze into `results/<date>_<host>/methodology.md` before publishing numbers.

## Hypothesis

Hypothesis: Nyxis wins on disk footprint and selective-read latency when few of 50 optional fields are populated.

**Driver note:** Pre-2026-05-22 C harness runs used linear `strcmp` key lookup (no `key_index`, no per-record `present[]`/`rank[]` cache). Those selective-read numbers are not valid for wire-format claims. Publication runs require the optimized C driver (FNV hash table at `nxs_open`, rank cache on first field access — same strategy as Go/Python). File-size metrics are unchanged by the driver fix.

## Schema

- 50 optional scalar fields: 20× Int64, 15× String, 10× Float64, 5× Bool
- Optional 3-level nest: `meta.child.grandchild` (gc_i64, gc_str)
- Canonical source: `bench/data/json/workload_A_pop{10,25,50,90}_<N>.json`

## Population rates (required)

10%, 25%, 50%, 90% — same logical RNG seed (`SEED=0x4E595849`) across formats.

## Primary metric

File size (bytes) per format per population rate.

## Secondary metrics

- Read 5 populated fields from a random record (P50, P99 ns): `i01`, `s21`, `f36`, `b46`, `i10`
- **Selective (publication NXS path):** C harness (`bench/harness/c`) with optimized driver (key index + rank cache). Matrix also runs Python for cross-format comparison; Python `_nxs` uses the same C core when built.
- **Selective (competitors):** Cap'n Proto / FlatBuffers use warm mapped messages; Protobuf uses a **fully parsed** `ParseFromString` message held in memory for the sample window (attribute access on Python objects — not a re-parse per sample). This matches typical app usage after load but is faster than zero-copy field resolution in Python.
- Harness lines include `"driver": "c"|"python"|"rust"|"go"`. `report.py` dedupes by `(workload, format, metric, records, population)` and **prefers `driver=c`** over other drivers for the same key.
- **Publication display:** P50 at or below timer resolution on dev hardware → report as **`< 1 µs (below timer resolution)`**, not `0 ns`.
- If Protobuf selective is also `< 1 µs`, footnote that it is **pre-parsed attribute access**, not wire decode — not comparable to NXS C-driver zero-copy selective.
- Memory high-water during full-scan reducer over one numeric field

## Open definition (uniform)

“Open” completes when record 0’s first requested field is readable. For NXS: `nxs_open` + `nxs_record(0)` + field read. For FlatBuffers: buffer + root table + field. Document competitor-equivalent steps in the frozen run doc.

## Review checklist

- [ ] FlatBuffers uses generated accessors, not verifier on hot path
- [ ] Protobuf uses generated code, not reflection
- [ ] Cap'n Proto uses packed encoding where applicable
