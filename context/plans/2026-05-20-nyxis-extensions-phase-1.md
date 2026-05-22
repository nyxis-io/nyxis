# Phase 1: Foundation & Commercial Alignment

> **Parent plan:** `2026-05-20-nyxis-extensions.md`  
> **Estimated time:** 2–3 days  
> **Prerequisite:** None  
> **Outcome:** `nyxis-extensions` is a buildable Cargo workspace with stub crates, contract tests, CI, aligned COMMERCIAL.md, and expanded README.

---

## Dependencies

| Phase | Depends on | Reason |
|-------|-----------|--------|
| **1** | None — this is the foundation phase | All later phases build on the workspace, SDK traits, and CI established here |

---

## Objective

Establish the private Rust workspace, shared SDK contracts, stub implementations, documentation alignment, and monorepo Makefile hook—so phases 2–5 can land independently without re-litigating structure.

---

## Key Context from Master Plan

**Relevant principles:**
- IP firewall: zero proprietary logic in public `nyxis/`
- Path-dep on `nxs` from `../nyxis/rust`
- TDD: contract tests before bodies

**Relevant architecture decisions:**
- Workspace layout: `crates/*`, `daemons/*`
- License: `NYXIS_LICENSE_PATH` env stub
- Integration: sidecar/daemon pattern

**Contracts this phase implements:**

```rust
// nyxis-ext-sdk/src/lib.rs (signatures only in phase 1)
pub struct ExtError { pub code: ErrorCode, pub message: String, pub correlation_id: Uuid }
pub enum ErrorCode { InvalidNxb, Io, Locked, License, Unavailable, Internal }

pub trait NyxisExtComponent {
    fn name(&self) -> &'static str;
    fn health_check(&self) -> Result<(), ExtError>;
}
```

Full contract catalog: `context/data/2026-05-20-nyxis-extensions-spec.yaml`

---

## Scope

### In Scope

- `nyxis-extensions/Cargo.toml` workspace matching monorepo layout (`crates/*`, `daemons/*`, `nxs` path = `../nyxis/rust`)
- Reserve `[workspace.dependencies]` entries: `nxs`, `fs2` (phase 2), `arrow` pin placeholder comment (phase 3 fills `54.2.0` + `ffi`)
- Crates: `nyxis-ext-sdk`, `nyxis-compactor`, `nyxis-arrow-bridge`, `nyxis-registry`, `nyxis-simd-guard` (stubs)
- Daemons: `nxs-compactd`, `nxs-registryd` (CLI parse + `unimplemented!` / exit 78)
- `dev-license.stub` sample + `.gitignore` for `target/`, `.env`
- `nyxis-extensions/.github/workflows/ci.yml`
- README expansion (layout, build, tier matrix)
- `nyxis/COMMERCIAL.md` Principal tier bullet additions
- Root `Makefile` `extensions-test` target
- Root `README.md` link to extensions build instructions

### Out of Scope

- Real compaction, Arrow, registry, SIMD logic (phases 2–5)
- Public `nxs` API changes unless `query::Reader` is already sufficient (it is)

---

## Implementation Steps

> Each checkbox = one git commit. Tests before implementation (TDD).

- [ ] **Initialize nyxis-extensions Cargo workspace with nxs path dependency**
  - Create `nyxis-extensions/Cargo.toml` workspace members
  - `[workspace.dependencies] nxs = { path = "../nyxis/rust" }`
  - **Tests:** none yet (workspace must parse)

- [ ] **Add nyxis-ext-sdk crate with ExtError and NyxisExtComponent trait**
  - `crates/nyxis-ext-sdk/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/license.rs` (stub `check_license()`)
  - **Tests:** `ext_sdk_contract_tests::error_display_includes_correlation_id` — asserts Display format

- [ ] **Write contract test suite for extension SDK**
  - `crates/nyxis-ext-sdk/tests/contract.rs`
  - **Tests:** `contract_license_missing_returns_license_error` — `NYXIS_LICENSE_PATH` unset → `ErrorCode::License`
  - **Tests:** `contract_health_check_default_ok` — stub component returns Ok

- [ ] **Scaffold feature crates with stub trait impls returning Unavailable**
  - `nyxis-compactor`, `nyxis-arrow-bridge`, `nyxis-registry`, `nyxis-simd-guard` each depend on `nyxis-ext-sdk` + `nxs`
  - `impl Compactor for StubCompactor` → `Err(Unavailable)`
  - **Tests:** `compactor_stub_returns_unavailable` per crate `tests/stub.rs`

- [ ] **Write acceptance test skeleton for extensions workspace**
  - `nyxis-extensions/tests/acceptance_extensions_e2e.rs`
  - **Tests:** `acceptance_full_pipeline_placeholder` — `assert!(false, "phase 5")` (RED)

- [ ] **Scaffold nxs-compactd and nxs-registryd CLIs**
  - `daemons/nxs-compactd/src/main.rs` — clap args per spec; calls stub; logs `correlation_id`
  - `daemons/nxs-registryd/src/main.rs` — `--listen` default `127.0.0.1:7946`; healthz stub
  - **Tests:** `daemon_compactd_help_succeeds` via `assert_cmd`

- [ ] **Add nyxis-extensions CI workflow and dev-license stub**
  - `.github/workflows/ci.yml`: `cargo test --workspace`, `cargo clippy -D warnings`
  - `dev-license.stub` + document in README
  - **Makes green:** all phase-1 contract + stub tests

- [ ] **Expand nyxis-extensions README with workspace layout and tier table**
  - Mirror GOVERNANCE component list with crate paths
  - Access section unchanged; add build/test commands

- [ ] **Align nyxis COMMERCIAL.md Principal tier with extensions README**
  - Add **Distributed Schema Registry & Proxy** and **Hardware-Accelerated SIMD Guard** under Principal
  - Cross-link `nyxis-extensions/README.md`

- [ ] **Add Makefile extensions-test target and root README pointer**
  - `Makefile`: `extensions-test` → cargo test in extensions
  - Root `README.md` quick start bullet

---

## Phase-Specific Risks

- **Risk:** `nxs` path breaks when extensions repo split to standalone private clone.  
  *Mitigation:* Document `git` dep override in README: `nxs = { git = "https://github.com/nyxis-io/nyxis", subdir = "rust" }`.

- **Risk:** Accidental commit of extensions code to public `nyxis` remote.  
  *Mitigation:* CI in extensions only; pre-push note in README; no `nyxis-extensions` code in `nyxis/.github`.

---

## Green Tests After This Phase

- ✅ `ext_sdk_contract_tests`
- ✅ per-crate `stub.rs` tests
- ✅ `daemon_compactd_help_succeeds`
- ❌ `acceptance_extensions_e2e` (intentionally RED)

---

## Files Created/Modified

| File | Action | Purpose |
|------|--------|---------|
| `nyxis-extensions/Cargo.toml` | Create | Workspace root |
| `nyxis-extensions/crates/nyxis-ext-sdk/**` | Create | Shared contracts |
| `nyxis-extensions/crates/nyxis-*/**` | Create | Stub libs |
| `nyxis-extensions/daemons/nxs-*/**` | Create | CLI stubs |
| `nyxis-extensions/.github/workflows/ci.yml` | Create | CI |
| `nyxis-extensions/README.md` | Modify | Dev guide |
| `nyxis/COMMERCIAL.md` | Modify | Tier alignment |
| `Makefile` | Modify | `extensions-test` |
| `README.md` | Modify | Cross-link |
| `.gitignore` | Modify | `context/` (done at plan time) |

---

**Next step:** `para-execute --phase=1`
