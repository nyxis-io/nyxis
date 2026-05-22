# Phase 5: Hardware-Accelerated SIMD Guard

> **Parent plan:** `2026-05-20-nyxis-extensions.md`  
> **Estimated time:** 1 week  
> **Prerequisite:** Phase 4 merged  
> **Outcome:** `nyxis-simd-guard` provides AVX-512/NEON/scalar batch ops with CPUID dispatch; workspace acceptance test goes GREEN.

---

## Dependencies

| Phase | Depends on | Reason |
|-------|-----------|--------|
| **5** | Phase 1 | Requires workspace, `SimdGuard` trait in `nyxis-ext-sdk`, and CI |
| **5** | Phase 2 | `acceptance_extensions_e2e` pipeline wires compactor dry-run; compactor must be implemented |
| **5** | Phase 3 | E2E pipeline includes arrow export step; bridge must be implemented |
| **5** | Phase 4 | E2E pipeline includes registry validate step; registry must be implemented |

---

## Objective

Principal tier MVP: extensions-only SIMD helpers for batch field calculations on raw buffers extracted via `nxs::query::Reader`—never exported from public `nxs` crate.

---

## Key Context from Master Plan

```rust
pub trait SimdGuard {
    fn batch_sum_i64(&self, values: &[i64]) -> i64;
    fn batch_count_present(&self, bitmask: &[u8], slot: u16) -> u32;
}
```

**Dispatch:** `std::is_x86_feature_detected!("avx512f")` / `std::arch::is_aarch64_feature_detected!("neon")` → scalar fallback.

---

## Scope

### In Scope

- `nyxis-simd-guard` implementations in `src/x86.rs`, `src/neon.rs`, `src/scalar.rs`
- `SimdGuard` trait impl on `AutoGuard`
- Property tests vs scalar reference
- Flip `acceptance_extensions_e2e` to GREEN (minimal pipeline: license → compact dry-run → arrow export 1 row → registry validate → simd sum)

### Out of Scope

- Wiring SIMD into public CLI
- GPU backends

---

## Implementation Steps

- [ ] **Write simd guard contract tests with scalar oracle**
  - `tests/contract.rs`
  - **Tests:** `contract_batch_sum_matches_scalar_oracle` — random len 0..10_000
  - **Tests:** `contract_count_present_matches_bit_scan`

- [ ] **Implement scalar reference in simd-guard**
  - `src/scalar.rs`
  - **Makes green:** oracle tests when forced `ScalarGuard`

- [ ] **Implement NEON and AVX-512 kernels behind cfg**
  - `src/neon.rs`, `src/x86.rs`, `src/dispatch.rs`
  - **Makes green:** contract tests on CI runners (scalar always; simd cfg on mac/linux)

- [ ] **Expose AutoGuard and document CPU requirements in README**
  - Log once at debug when fallback used

- [ ] **Complete acceptance_extensions_e2e pipeline test**
  - Generate temp dir with `minimal.nxb`; run stubs wired to real impls
  - **Tests:** `acceptance_full_pipeline_placeholder` → rename to `acceptance_full_pipeline` assert true
  - **Makes green:** E2E acceptance

- [ ] **Finalize extensions README tier matrix and runbook**
  - All four components marked MVP/shipped status

---

## Green Tests After This Phase

- ✅ all prior phase tests
- ✅ `simd_guard contract.rs`
- ✅ `acceptance_extensions_e2e` **GREEN**

---

**Next step:** `para-execute --phase=5`, then `para-archive`
