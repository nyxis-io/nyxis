# Workload A — Sparse hierarchical records (methodology template)

**Status:** Draft — freeze into `results/<date>_<host>/methodology.md` before publishing numbers.

## Hypothesis

Hypothesis: Nyxis wins on disk footprint and selective-read latency when few of 50 optional fields are populated. **Dev 10k (2026-05-21):** Protobuf was smallest on disk at all population rates; Cap'n Proto led selective-read P50; revisit at 1M+ before publication.

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
- **Selective (NXS harness):** default uses one warm reader for the whole sample window (C, Go, Rust, Python). Python accepts `--cold` to re-open on every sample (legacy path). Competitors: protobuf re-parses; capnp `from_bytes` per sample unless noted otherwise in the frozen run doc.
- Memory high-water during full-scan reducer over one numeric field

## Open definition (uniform)

“Open” completes when record 0’s first requested field is readable. For NXS: `nxs_open` + `nxs_record(0)` + field read. For FlatBuffers: buffer + root table + field. Document competitor-equivalent steps in the frozen run doc.

## Review checklist

- [ ] FlatBuffers uses generated accessors, not verifier on hot path
- [ ] Protobuf uses generated code, not reflection
- [ ] Cap'n Proto uses packed encoding where applicable
