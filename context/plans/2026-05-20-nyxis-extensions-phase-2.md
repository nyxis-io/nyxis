# Phase 2: In-Memory Compactor Daemon

> **Parent plan:** `2026-05-20-nyxis-extensions.md`  
> **Estimated time:** 1–2 weeks  
> **Prerequisite:** Phase 1 merged  
> **Outcome:** `nxs-compactd` repacks sealed `.nxb` files reducing data-sector slack ≥10% on fragmented fixtures (dry-run and apply modes), with cross-process safety via advisory file locks.

---

## Dependencies

| Phase | Depends on | Reason |
|-------|-----------|--------|
| **2** | Phase 1 | Requires the Cargo workspace, `nyxis-ext-sdk` traits (`Compactor`, `ExtError`), stub crates, and CI pipeline established in Phase 1 |

---

## Objective

Deliver Enterprise Core tier MVP: online data-sector defragmentation for **sealed** `.nxb` files using `nxs::query::Reader` for validation and atomic rewrite preserving `DictHash`, `record_count`, and tail-index O(1 semantics—**without** racing active mmap/ingestion workers.

---

## Key Context from Master Plan

**Contracts:**

```rust
pub struct CompactOptions {
    pub dry_run: bool,
    pub min_savings_ratio: f64, // default 0.05
    pub lock_backoff: BackoffPolicy, // default: 3 attempts, 100/400/1600 ms
}

pub trait Compactor {
    fn compact_file(&self, path: &Path, opts: CompactOptions) -> Result<CompactorStats, ExtError>;
}
```

**Lock protocol (mandatory before read or stage):**

1. Derive lock path: `{nxb_path}.lock` (e.g. `data.nxb.lock`)
2. Open/create lock file; attempt **exclusive** advisory lock via `fs2::FileExt::lock_exclusive()` (non-blocking or try-lock loop with backoff)
3. On lock failure → `ExtError` with code `LOCKED` (or `Io` + context) → caller applies exponential backoff
4. Hold lock for entire read → repack → `rename` window; release on success, error, or panic guard (`Drop`)

**Why:** Atomic `rename` alone is insufficient when another process has the original path mmap’d (Linux retains inode; Windows may return `ERROR_ACCESS_DENIED` / `EBUSY` on replace).

**Cross-repo convention (documented, not implemented in MIT drivers v1):** Ingestion workers SHOULD acquire the same `{path}.lock` shared/exclusive policy before opening `.nxb` for write. Extensions enforce exclusive compaction lock; shared locks for readers are a future driver enhancement.

**Algorithm sketch (MVP):**

0. Acquire exclusive `{path}.lock` (backoff on contention)
1. `Reader::new(mmap/file)` — validate NYXB
2. Walk tail-index offsets; copy each record blob sequentially into new data sector (8-byte aligned)
3. Rebuild tail-index + preamble `TailPtr`; verify `DictHash` unchanged
4. Write `{path}.compacting` → `rename` → `{path}` (release lock after cleanup)

**Out of scope:** `.nxsw` WAL compaction; implementing locks inside `nyxis-drivers` (docs only).

---

## Scope

### In Scope

- `nyxis-compactor` real `Compactor` impl + `src/lock.rs` (`fs2` crate)
- `nxs-compactd` scan loop over `--data-dir`
- Observability: `tracing` with `correlation_id`, stats struct, `compactor_lock_contention_total`
- Integration + lock contention tests

### Out of Scope

- 20–30% savings guarantee on all workloads (benchmark only)
- Distributed / cluster compactor
- Driver-side lock acquisition (deferred to drivers roadmap)

---

## Implementation Steps

- [ ] **Add fs2 workspace dependency and lock helper module**
  - `nyxis-extensions/Cargo.toml`: `fs2 = "0.4"` under `[workspace.dependencies]`
  - `crates/nyxis-compactor/src/lock.rs` — `struct NxbLockGuard` with `Drop` release
  - **Tests:** `lock_guard_releases_on_drop` — second process can lock after drop

- [ ] **Write compactor contract tests against conformance minimal.nxb**
  - `crates/nyxis-compactor/tests/contract.rs`
  - **Tests:** `contract_compact_preserves_dict_hash`
  - **Tests:** `contract_compact_preserves_record_count`
  - **Tests:** `contract_compact_reader_query_equivalent`

- [ ] **Write lock contention contract test**
  - `crates/nyxis-compactor/tests/lock_contract.rs`
  - Spawn thread holding `{fixture}.lock`; compactor returns `LOCKED`/skips within backoff window
  - **Tests:** `contract_compact_skips_when_lock_held` — no `.compacting` artifact left behind

- [ ] **Write compactd integration test skeleton**
  - `daemons/nxs-compactd/tests/integration.rs`
  - **Tests:** `integration_dry_run_no_rename` — mtime unchanged
  - **Tests:** `integration_respects_external_lock` — external lock file blocks compaction

- [ ] **Implement sector repack engine in nyxis-compactor**
  - `src/repack.rs`, `src/stats.rs`
  - Entry: `compact_file` acquires lock first, then repack
  - **Makes green:** `contract_compact_preserves_*`

- [ ] **Implement atomic write path with lock-held rename and EBUSY cleanup**
  - `src/io.rs` — temp suffix `.compacting`; on rename failure release lock + backoff
  - Remove orphan `.compacting` on abort
  - **Tests:** `contract_compact_atomic_rename`
  - **Makes green:** lock + io contract tests

- [ ] **Wire nxs-compactd scan loop with backoff policy and structured logging**
  - Default backoff: 3 attempts, 100ms / 400ms / 1600ms jitter optional
  - Log: `{ correlation_id, path, lock_wait_ms, bytes_before, bytes_after }`
  - **Makes green:** integration tests

- [ ] **Document lock protocol in nyxis-extensions README compactor section**
  - Operator guide: ingestion services must cooperate on `{path}.lock`
  - Windows note: exclusive lock before rename

- [ ] **Add compactor bench harness against large.nxb fixture**
  - `benches/compactor.rs`
  - **Tests:** `bench_smoke_runs_under_30s`

---

## Phase-Specific Risks

- **Risk:** Repack breaks nested objects / lists.  
  *Mitigation:* Contract tests include `nested` conformance vector; round-trip field compare.

- **Risk:** Writers ignore `.lock` convention.  
  *Mitigation:* Exclusive lock still serializes compactor vs compactor; document driver obligation; metric `compactor_skip_total` for ops visibility.

- **Risk:** Stale lock after crash.  
  *Mitigation:* OS releases advisory lock on process exit; document manual delete only if lock file orphaned without holder (rare on Unix).

---

## Green Tests After This Phase

- ✅ all phase-1 tests
- ✅ `compactor contract.rs` + `lock_contract.rs`
- ✅ `compactd integration.rs`
- ❌ `acceptance_extensions_e2e`

---

## Files Created/Modified

| File | Action |
|------|--------|
| `crates/nyxis-compactor/src/lock.rs` | Create |
| `crates/nyxis-compactor/tests/lock_contract.rs` | Create |
| `crates/nyxis-compactor/src/repack.rs` | Create |
| `crates/nyxis-compactor/tests/contract.rs` | Create |
| `daemons/nxs-compactd/src/main.rs` | Modify |
| `daemons/nxs-compactd/tests/integration.rs` | Create |
| `nyxis-extensions/README.md` | Modify |
| `nyxis-extensions/Cargo.toml` | Modify — `fs2` workspace dep |

---

**Next step:** `para-execute --phase=2`
