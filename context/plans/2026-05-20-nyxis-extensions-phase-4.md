# Phase 4: Distributed Schema Registry & Proxy

> **Parent plan:** `2026-05-20-nyxis-extensions.md`  
> **Estimated time:** 2–3 weeks  
> **Prerequisite:** Phase 3 merged  
> **Outcome:** `nxs-registryd` serves gRPC schema registration/lookup by `DictHash` and validates sample `.nxb` payloads; drift policy `additive_only` enforced.

---

## Dependencies

| Phase | Depends on | Reason |
|-------|-----------|--------|
| **4** | Phase 1 | Requires workspace structure, `nyxis-ext-sdk` traits (`NyxisExtComponent`, `ExtError`), and CI |
| **4** | Phase 2 | `ValidatePayload` reuses `nxs::query::Reader` validation logic validated in compactor contract tests |
| **4** | Phase 3 | Arrow bridge must be merged first to keep workspace dependency set stable before adding `tonic`/`prost` |

---

## Objective

Principal tier MVP: local schema registry with proxy-style validation API—enterprises register schema headers keyed by `DictHash`, consumers validate incoming `.nxb` preambles before ingest.

---

## Key Context from Master Plan

**gRPC package:** `nyxis.registry.v1` (see spec YAML)

**RPCs (MVP):**
- `RegisterSchema(dict_hash, schema_bytes)`
- `GetSchemaByHash(dict_hash)`
- `ValidatePayload(nxb_bytes)` → `{ valid, errors[] }`
- `ResolveDrift` → stub returns `UNAVAILABLE` in MVP (proxy rewrite phase 4.1 deferred)

**Storage:** SQLite `schemas(dict_hash BLOB PRIMARY KEY, schema_bytes BLOB, version INT)`

---

## Scope

### In Scope

- `nyxis-registry` library + `nxs-registryd` daemon
- HTTP `GET /healthz`
- gRPC via `tonic`
- Contract tests with in-memory SQLite

### Out of Scope

- Multi-region replication
- Postgres adapter
- Full drift auto-rewrite proxy

---

## Implementation Steps

- [ ] **Define protobuf and generate Rust types**
  - `crates/nyxis-registry/proto/registry.proto`
  - `build.rs` with `tonic-build`
  - **Tests:** compile-only `registry_proto_builds`

- [ ] **Write registry gRPC contract tests**
  - `tests/grpc_contract.rs` using `tonic::transport::Endpoint` + test server
  - **Tests:** `contract_register_and_get_roundtrip`
  - **Tests:** `contract_validate_payload_bad_magic_fails`
  - **Tests:** `contract_validate_payload_dict_hash_mismatch`

- [ ] **Implement SQLite store and RegistryService**
  - `src/store.rs`, `src/service.rs`
  - **Makes green:** register/get tests

- [ ] **Implement ValidatePayload using nxs preamble parse**
  - Compare `DictHash` + embedded schema bytes to registry row
  - **Makes green:** validate tests

- [ ] **Wire nxs-registryd with healthz and graceful shutdown**
  - SIGTERM drains connections; healthz `degraded` if DB unreadable
  - **Tests:** `daemon_registry_healthz_ok`

- [ ] **Document registry deployment and drift policies in README**
  - Default listen `127.0.0.1:7946`; env `NYXIS_REGISTRY_DB`

---

## Graceful Degradation (phase-specific)

| Failure | Behavior |
|---------|----------|
| SQLite locked | gRPC `ABORTED`, retry hint in message |
| Unknown DictHash on validate | `valid: false`, error `SCHEMA_NOT_FOUND` |
| DB file missing on start | Create if writable else exit 78 |

---

## Green Tests After This Phase

- ✅ phases 1–3 tests
- ✅ `registry grpc_contract.rs`
- ✅ `daemon_registry_healthz_ok`
- ❌ `acceptance_extensions_e2e`

---

**Next step:** `para-execute --phase=4`
