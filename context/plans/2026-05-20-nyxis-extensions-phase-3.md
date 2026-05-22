# Phase 3: Apache Arrow Zero-Copy Memory Bridge

> **Parent plan:** `2026-05-20-nyxis-extensions.md`  
> **Estimated time:** 1–2 weeks  
> **Prerequisite:** Phase 2 merged  
> **Outcome:** `nyxis-arrow-bridge` exports `arrow::RecordBatch` with workspace-pinned Arrow + `ffi` (C Data Interface) for handoff to Polars, DataFusion, and Python without extra serialization.

---

## Dependencies

| Phase | Depends on | Reason |
|-------|-----------|--------|
| **3** | Phase 1 | Requires workspace structure, `nyxis-ext-sdk` traits (`ArrowBridge`, `ExtError`), and CI |
| **3** | Phase 2 | Compactor must be merged first so the workspace `Cargo.lock` is stable before adding the `arrow` workspace dependency pin |

---

## Objective

Principal tier MVP: map streaming `.nxb` records into Apache Arrow columnar memory with minimal copying—using `nxs::query::Reader` for O(1) record access, explicit copy fallback for jumbo/nested paths, and **C Data Interface** export for downstream analytical stacks.

---

## Key Context from Master Plan

**Contracts:**

```rust
pub struct ArrowFieldMap {
    pub columns: Vec<(String, ArrowDataType)>,
}

pub trait ArrowBridge {
    fn export_record_batch<'a>(...) -> Result<arrow::record_batch::RecordBatch, ExtError>;
    /// Principal: zero-copy handoff to Polars/DataFusion/Python via Arrow C Data Interface
    fn export_ffi_arrays<'a>(...) -> Result<arrow::ffi::FFI_ArrowArray, ExtError>;
}
```

**Workspace dependency (single pin for entire extensions workspace):**

```toml
# nyxis-extensions/Cargo.toml
[workspace.dependencies]
arrow = { version = "54.2.0", default-features = false, features = ["ffi"] }

# crates/nyxis-arrow-bridge/Cargo.toml
[features]
default = []
arrow = ["dep:arrow"]

[dependencies]
arrow = { workspace = true, optional = true }
```

- **Pin:** Exact version in `[workspace.dependencies]`; commit `Cargo.lock` in `nyxis-extensions`
- **`ffi` feature:** Required for `FFI_ArrowArray` / `FFI_ArrowSchema` cross-boundary transfer without serialize round-trip
- **CI:** `cargo test -p nyxis-arrow-bridge --features arrow` uses workspace pin only (no floating crate version)

---

## Scope

### In Scope

- Primitive sigils: `= ~ ? " @` → Arrow types
- `export_record_batch` + `export_ffi_arrays` (or equivalent using `arrow::ffi` from enabled feature set)
- CLI `nxs-arrow-export` with optional `--ffi` smoke path
- Contract tests: batch shape, zero-copy primitive column, FFI struct validity
- README: document pinned Arrow version + consumer compatibility (Polars/DataFusion/Python pyarrow)

### Out of Scope

- Snowflake/Tableau native connectors
- Full nested struct Arrow struct (JSON Utf8 fallback remains)
- Building Polars/DataFusion in CI (FFI struct layout tests only)

---

## Implementation Steps

- [ ] **Pin arrow in workspace Cargo.toml with ffi feature**
  - `[workspace.dependencies] arrow = { version = "54.2.0", default-features = false, features = ["ffi"] }`
  - Wire `nyxis-arrow-bridge` optional dep from workspace
  - **Tests:** `cargo tree -p nyxis-arrow-bridge --features arrow` shows single arrow 54.2.0

- [ ] **Write arrow bridge contract tests (feature arrow)**
  - `crates/nyxis-arrow-bridge/tests/contract.rs`
  - **Tests:** `contract_export_minimal_batch_len`
  - **Tests:** `contract_export_i64_column_zero_copy`
  - **Tests:** `contract_nested_field_fallback_string`

- [ ] **Write FFI / C Data Interface contract test**
  - `crates/nyxis-arrow-bridge/tests/contract_ffi.rs`
  - **Tests:** `contract_ffi_schema_and_array_non_null` — export `FFI_ArrowArray` + schema; `release` callbacks valid
  - **Tests:** `contract_ffi_roundtrip_import` — `arrow::ffi` import exported arrays → `RecordBatch` len unchanged (in-process roundtrip)

- [ ] **Implement ArrowFieldMap builder from nxb schema keys/sigils**
  - `src/schema_map.rs`
  - **Makes green:** schema mapping unit tests

- [ ] **Implement export_record_batch with column builders**
  - `src/export.rs` — primitive zero-copy via `Reader::get_*`
  - **Makes green:** batch contract tests

- [ ] **Implement export_ffi_arrays for C Data Interface handoff**
  - `src/ffi_export.rs` — use `arrow::ffi` with `ffi` feature enabled at workspace level
  - Document memory lifetime: caller must not free until Nyxis buffer outlives import
  - **Makes green:** `contract_ffi_*`

- [ ] **Add nxs-arrow-export CLI and README Principal section**
  - Flags: `--nxb`, `--rows`, `--ffi` (print/dump FFI metadata for smoke)
  - **Tests:** `assert_cmd` exit 0 on `minimal.nxb`
  - README table: Arrow **54.2.0**, features `ffi`, compatible consumers

- [ ] **Enable arrow+ffi in CI matrix job**
  - `.github/workflows/ci.yml` job `arrow-bridge`: `cargo test -p nyxis-arrow-bridge --features arrow --locked`
  - **Makes green:** all arrow + ffi contract tests

---

## Phase-Specific Risks

- **Risk:** Arrow patch mismatch with customer pyarrow.  
  *Mitigation:* Document pinned version; export `arrow::datatypes::Metadata` version string in CLI `--version`.

- **Risk:** FFI memory lifetime bugs (use-after-free).  
  *Mitigation:* `Arc` holder for backing `.nxb` bytes tied to `FFI_ArrowArray` private_data + release callback tests.

- **Risk:** Misaligned buffers on big-endian.  
  *Mitigation:* `compile_error` on big-endian targets for arrow-bridge crate.

---

## Green Tests After This Phase

- ✅ phases 1–2 tests
- ✅ `arrow_bridge contract.rs` + `contract_ffi.rs` (`--features arrow --locked`)
- ❌ `acceptance_extensions_e2e`

---

**Next step:** `para-execute --phase=3`
