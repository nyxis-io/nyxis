# Nyxis Extensions — Full Roadmap

> **Master plan.** Phase-specific implementation steps are in sub-plan files. Load only the phase you are executing.

**Date:** 2026-05-20  
**Status:** In Review

---

## Objective

Turn `nyxis-extensions/` from a README-only private placeholder into a **production-grade Rust workspace** that implements all four commercial components as **private crates** depending on the public `nxs` library via path/git dependency—without adding extension hooks or trade-secret code to `nyxis-io/nyxis`. Align [nyxis/COMMERCIAL.md](../../nyxis/COMMERCIAL.md) with [nyxis-extensions/README.md](../../nyxis-extensions/README.md) and [GOVERNANCE.md](../../GOVERNANCE.md).

---

## Core Principles

1. **IP firewall.** All compactor, Arrow, registry, and SIMD logic stays in `nyxis-extensions`; public `nyxis` only gains *stable read APIs* if truly necessary—never proprietary algorithms.
2. **Path-dep on `nxs`, not fork.** Extensions consume `../nyxis/rust` (`package name: nyxis`, `lib name: nxs`) so conformance and magic-byte rules stay single-sourced.
3. **Sidecar-first delivery.** Daemons and libraries operate on `.nxb` files and optional local sockets; no required in-process plugin loading in v1.
4. **TDD per boundary.** Contract tests for each crate trait before implementation; one acceptance test skeleton on day 1 (foundation phase).
5. **Tier-gated features.** Build flags / license file check stub in foundation; real license verification deferred to commercial ops integration.
6. **Defer perfection.** Each phase ships an MVP with measurable success criteria; hardening (HA registry, distributed compactor) is explicitly out of v1 scope.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  nyxis/ (PUBLIC BSL) — nxs crate: compiler, query::Reader, WAL │
└────────────────────────────┬────────────────────────────────────┘
                             │ path dependency (Cargo)
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│  nyxis-extensions/ (PRIVATE EULA)                               │
│  ┌──────────────┐  ┌─────────────┐  ┌──────────┐  ┌───────────┐ │
│  │ ext-sdk      │  │ compactor   │  │ arrow    │  │ registry  │ │
│  │ (traits/err) │  │ + compactd  │  │ bridge   │  │ + registryd│ │
│  └──────────────┘  └─────────────┘  └──────────┘  └───────────┘ │
│                              ┌──────────────┐                   │
│                              │ simd-guard   │                   │
│                              └──────────────┘                   │
└─────────────────────────────────────────────────────────────────┘
```

### Monorepo Layout (local dev)

```
nyxis-workspace/                 # monorepo root
├── Makefile                     # extensions-test
├── README.md
├── nyxis/                       # PUBLIC BSL
│   └── COMMERCIAL.md
├── nyxis-drivers/               # PUBLIC MIT
└── nyxis-extensions/            # PRIVATE EULA workspace
    ├── Cargo.toml               # workspace root; nxs path = "../nyxis/rust"
    ├── dev-license.stub
    ├── crates/
    │   ├── nyxis-ext-sdk/
    │   ├── nyxis-compactor/
    │   ├── nyxis-arrow-bridge/
    │   ├── nyxis-registry/
    │   └── nyxis-simd-guard/
    └── daemons/
        ├── nxs-compactd/
        └── nxs-registryd/
```

### Data/Event Flow

```
.nxb file ──► nxs::query::Reader (validate NYXB/NYXO)
     │
     ├─► nxs-compactd ──► acquire {path}.lock (exclusive)
     │       └─► repacked .nxb (temp + atomic rename) ──► release lock
     ├─► nxs-arrow-bridge ──► Arrow RecordBatch + C Data Interface (ffi)
     ├─► nxs-registryd ──► schema by DictHash / drift policy
     └─► nxs-simd-guard ──► accelerated batch ops on raw sectors
```

---

## Architecture Decisions

| Decision | Choice | Rationale | Alternatives Rejected |
|----------|--------|-----------|----------------------|
| Core integration | Private crates + `path = "../nyxis/rust"` | Preserves trade-secret boundary; reuses `query::Reader` | Dynamic plugins in public repo (IP leak risk) |
| Compactor I/O | Temp file + `rename` in-place | Crash-safe; mmap’d readers retain old inode until reopen | In-place sector shuffle (corruption risk) |
| Compactor concurrency | Advisory lock `{path}.lock` via `fs2` before read/repack | Prevents rename races with mmap/ingestion workers; cross-platform (incl. Windows `ERROR_ACCESS_DENIED`) | Blind rename + EBUSY-only retry |
| Registry storage v1 | SQLite embedded | Single-binary deploy for evals | Postgres day-1 (ops burden) |
| Arrow dep | Workspace-pinned `arrow` with `ffi` feature; crate feature `arrow` | C Data Interface for Polars/DataFusion/Python zero-copy handoff; single `Cargo.lock` pin | Per-crate floating version; arrow without `ffi` |
| SIMD dispatch | `cfg` + runtime CPUID fallback | AVX-512 vs NEON without separate binaries | Separate per-arch releases |
| License gate | Stub `NYXIS_LICENSE_PATH` env check | Unblocks engineering; legal integrates later | Hard-coded keys in repo |

---

## Responsibility Split

| Responsibility | Owner |
|----------------|-------|
| `.nxb` format truth | `nyxis` / `SPEC.md` |
| Extension traits & `ExtError` | `nyxis-ext-sdk` |
| Online compaction | `nyxis-compactor` + `nxs-compactd` |
| Arrow export | `nyxis-arrow-bridge` |
| Schema storage & drift API | `nyxis-registry` + `nxs-registryd` |
| Vectorized guards | `nyxis-simd-guard` |
| Commercial tier docs | `nyxis/COMMERCIAL.md` + `nyxis-extensions/README.md` |
| Workspace CI | `nyxis-extensions/.github/workflows/` |

---

## Graceful Degradation

| Failure Scenario | Expected Behavior |
|-----------------|-------------------|
| `nxs::query::Reader::new` fails (bad magic, truncated file) | Return `ExtError::INVALID_NXB`; log with `correlation_id`; daemon skips file, continues scan |
| `{path}.lock` held by writer/peer compactor | Exponential backoff (3×: 100ms → 400ms → 1600ms); then skip + `compactor_skip_total` |
| Lock acquired but `rename` fails (EBUSY / Windows access denied) | Release lock; same backoff; never leave stale `.compacting` without cleanup |
| `NYXIS_LICENSE_PATH` missing or invalid | Daemons exit code 78; libraries return `ExtError::LICENSE` on `init()` |
| Arrow feature disabled at compile time | `export_record_batch` returns `UNAVAILABLE` with clear message |
| Registry DB locked/corrupt | `registryd` returns 503 on gRPC; healthz reports `degraded` |
| CPU lacks AVX-512/NEON | `simd-guard` transparent scalar fallback; log once at `debug` |

---

## Phase Overview

| Phase | Title | Scope | Est. Time | Status |
|-------|-------|-------|-----------|--------|
| **1** | Foundation & commercial alignment | Workspace, SDK traits, stubs, CI, README, COMMERCIAL sync | 2–3 days | Not started |
| **2** | In-Memory Compactor Daemon | MVP online sector repack + `nxs-compactd` | 1–2 weeks | Not started |
| **3** | Apache Arrow Zero-Copy Bridge | RecordBatch export + CLI smoke | 1–2 weeks | Not started |
| **4** | Distributed Schema Registry & Proxy | `registryd` + SQLite + gRPC MVP | 2–3 weeks | Not started |
| **5** | Hardware-Accelerated SIMD Guard | CPUID dispatch + batch ops + tests | 1 week | Not started |

### Phase Dependency Graph

Phases must be executed in order. Each phase depends on all prior phases being merged before starting.

```
Phase 1 (Foundation)
    │
    ▼
Phase 2 (Compactor)
    │
    ▼
Phase 3 (Arrow Bridge)
    │
    ▼
Phase 4 (Schema Registry)
    │
    ▼
Phase 5 (SIMD Guard + E2E acceptance GREEN)
```

**Dependency summary:**

| Phase | Direct dependencies |
|-------|-------------------|
| 1 | None (foundation) |
| 2 | Phase 1 |
| 3 | Phases 1, 2 |
| 4 | Phases 1, 2, 3 |
| 5 | Phases 1, 2, 3, 4 |

### Progressive Regression Rule

```
Phase 1 → ext_sdk_contract_tests GREEN; acceptance skeleton compiles (RED)
Phase 2 → + compactor_contract_tests GREEN; compactd_integration GREEN
Phase 3 → + arrow_contract_tests GREEN (feature arrow)
Phase 4 → + registry_grpc_contract_tests GREEN
Phase 5 → + simd_guard_contract_tests GREEN; acceptance_extensions_e2e GREEN
```

---

## Execution Plan

1. Review master + all sub-plans
2. `para-execute --phase=1` on branch `para/nyxis-extensions-phase-1`
3. PR review → merge to `main` (extensions repo + monorepo doc commits)
4. Repeat phases 2–5; each branch rebased on updated `main`
5. Final: run `make -C nyxis test` + `cargo test -p nyxis-ext-workspace` from extensions root
6. `para-archive` when complete

### Branch Strategy

- `para/nyxis-extensions-phase-1` … `para/nyxis-extensions-phase-5`
- Commits land in **`nyxis-extensions/`** subtree; `nyxis/COMMERCIAL.md` edits in phase 1 only

---

## New Components

| Component | Location | Purpose |
|-----------|----------|---------|
| Workspace root | `nyxis-extensions/Cargo.toml` | Members, shared deps, `nxs` path |
| SDK | `nyxis-extensions/crates/nyxis-ext-sdk/` | Traits, `ExtError`, license stub |
| Compactor | `nyxis-extensions/crates/nyxis-compactor/` | Repack logic |
| Daemon | `nyxis-extensions/daemons/nxs-compactd/` | Background scanner |
| Arrow | `nyxis-extensions/crates/nyxis-arrow-bridge/` | Arrow export |
| Registry | `nyxis-extensions/crates/nyxis-registry/` | gRPC service lib |
| Registry daemon | `nyxis-extensions/daemons/nxs-registryd/` | HTTP/gRPC server |
| SIMD | `nyxis-extensions/crates/nyxis-simd-guard/` | Vectorized guards |
| Spec | `context/data/2026-05-20-nyxis-extensions-spec.yaml` | Contracts |
| CI | `nyxis-extensions/.github/workflows/ci.yml` | `cargo test --workspace` |

---

## Security Model Summary

- **Repo access:** Private GitHub org; no mirroring to public remotes (per README).
- **Build artifacts:** Release binaries require commercial license file; stub checks env in v1.
- **Registry:** Local bind `127.0.0.1` default; TLS termination deferred to deployment guide.
- **Secrets:** Never commit license keys; `.env` gitignored.

---

## Local Dev Setup

```bash
# From monorepo root (after phase 1)
cd nyxis-extensions
export NYXIS_LICENSE_PATH=./dev-license.stub   # phase-1 stub file
cargo test --workspace
cargo run -p nxs-compactd -- --data-dir ../nyxis/bench/fixtures --dry-run

# Optional Principal features (workspace-pinned arrow + ffi)
cargo test -p nyxis-arrow-bridge --features arrow
cargo test -p nyxis-arrow-bridge --features arrow -- contract_ffi_roundtrip
```

Root `Makefile` target (phase 1): `make extensions-test` → `cargo test --manifest-path nyxis-extensions/Cargo.toml`.

---

## Sub-Plans

- [2026-05-20-nyxis-extensions-phase-1.md](./2026-05-20-nyxis-extensions-phase-1.md) — Foundation & commercial alignment
- [2026-05-20-nyxis-extensions-phase-2.md](./2026-05-20-nyxis-extensions-phase-2.md) — In-Memory Compactor Daemon
- [2026-05-20-nyxis-extensions-phase-3.md](./2026-05-20-nyxis-extensions-phase-3.md) — Apache Arrow Zero-Copy Bridge
- [2026-05-20-nyxis-extensions-phase-4.md](./2026-05-20-nyxis-extensions-phase-4.md) — Schema Registry & Proxy
- [2026-05-20-nyxis-extensions-phase-5.md](./2026-05-20-nyxis-extensions-phase-5.md) — SIMD Guard

---

## What We Are NOT Building (v1)

- Public `nxs` plugin ABI or `dlopen` hooks
- Multi-node registry HA or Postgres backend
- Live WAL compaction while writer holds exclusive lock (phase 2: sealed `.nxb` only)
- Mandatory lock acquisition inside MIT drivers (documented convention only; driver PRs deferred)
- Snowflake/Tableau connectors (only Arrow batch export)
- Automated license server / billing integration

---

**Next step:** Review sub-plans. Then `para-review --plan` or `para-execute --phase=1`.
