# Cross-Repository Conformance

MIT client drivers (`nyxis-drivers`) must read the same `.nxb` layout that the BSL core (`nyxis`) compiles. Nyxis uses zero-copy offsets and strict alignment; a single bitmask or magic-byte drift in any driver corrupts downstream data.

This document describes **what is implemented today** in this monorepo and **how to wire CI** when `nyxis` and `nyxis-drivers` are separate GitHub repositories.

Canonical vector format and runner contract: [`nyxis/conformance/README.md`](nyxis/conformance/README.md).

---

## 1. Golden matrix (implemented)

Core owns the corpus; drivers only **read** generated binaries and compare to JSON expectations.

```text
  nyxis/conformance/
    generate.rs          # invoked by gen_conformance (Rust)
    <name>.nxb           # reference binaries (NYXB / NYXO / NYXL magic)
    <name>.expected.json # decoded field expectations
    run_<lang>.*         # per-language runners (import code from nyxis-drivers)

  Flow:
    1. Rust gen_conformance writes .nxb + .expected.json
    2. Each driver runner opens .nxb and asserts parity with .expected.json
    3. Negative vectors assert ERR_BAD_MAGIC, ERR_DICT_MISMATCH, ERR_OUT_OF_BOUNDS
```

**14 vectors** today: `minimal`, `all_sigils`, `null_vs_absent`, `sparse`, `nested`, `list_i64`, `list_f64`, `unicode_strings`, `large`, `max_keys`, `jumbo_string`, `bad_magic`, `bad_dict_hash`, `truncated`.

Drivers covered by runners: **Rust** (core), **Go, Python, JavaScript, Ruby, PHP, C, Swift, Kotlin, C#**.

---

## 2. Monorepo commands (this workspace)

From the repo root:

```bash
make conformance
```

Equivalent:

```bash
make -C nyxis conformance-generate   # cargo run --bin gen_conformance
make -C nyxis conformance-run        # all 10 language runners
```

Single runner:

```bash
make -C nyxis conformance-run-go
make -C nyxis conformance-run-js
# … see nyxis/Makefile conformance-run-* targets
```

Drivers live in `nyxis-drivers/`; runners in `nyxis/conformance/` import SDKs via `../../nyxis-drivers/<lang>/`.

---

## 3. Split-repo CI (recommended when published)

### Core: `nyxis-io/nyxis`

Keep [`.github/workflows/conformance.yml`](nyxis/.github/workflows/conformance.yml):

1. **Job `generate-vectors`** — `make conformance-generate`, upload `conformance/` artifact.
2. **Jobs `run-*`** — download artifact, run `make conformance-run-<lang>` (core checkout must include a sibling `nyxis-drivers` checkout or use a composite action).

On spec/compiler changes, optionally **dispatch** driver CI:

```yaml
# .github/workflows/conformance-dispatch.yml (not yet committed — add when repos split)
name: Core Conformance Dispatch
on:
  push:
    branches: [main]
    paths:
      - 'rust/**'
      - 'conformance/**'
      - 'SPEC.md'

jobs:
  trigger-drivers:
    runs-on: ubuntu-latest
    steps:
      - uses: peter-evans/repository-dispatch@v3
        with:
          token: ${{ secrets.NYXIS_CI_AUTOMATION_TOKEN }}
          repository: nyxis-io/nyxis-drivers
          event-type: core-spec-updated
          client-payload: '{"sha": "${{ github.sha }}"}'
```

### Drivers: `nyxis-io/nyxis-drivers`

Driver workflow must **check out both repos** and point at core vectors:

```yaml
# .github/workflows/run-conformance.yml (template — align paths when splitting)
name: Driver Conformance Matrix
on:
  repository_dispatch:
    types: [core-spec-updated]
  push:
    branches: [main]

jobs:
  conformance:
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        runner: [rust, js, py, go, ruby, php, c, swift, kotlin, csharp]
    steps:
      - uses: actions/checkout@v4
        with:
          path: nyxis-drivers

      - uses: actions/checkout@v4
        with:
          repository: nyxis-io/nyxis
          path: nyxis

      - uses: dtolnay/rust-toolchain@stable

      - name: Generate vectors
        working-directory: nyxis
        run: make conformance-generate

      - name: Run ${{ matrix.runner }}
        working-directory: nyxis
        run: make conformance-run-${{ matrix.runner }}
        env:
          DRV: ${{ github.workspace }}/nyxis-drivers
```

**Note:** Today’s `nyxis/.github/workflows/conformance.yml` still lists paths like `js/**`, `py/**` as if drivers were inside the core tree. After splitting, narrow core `paths` to `rust/**`, `conformance/**`, `SPEC.md` and run driver jobs from `nyxis-drivers` with the dual-checkout layout above.

---

## 4. Enforcement rules

### Implemented (corpus + runners)

| Rule | Coverage |
|------|----------|
| **Read parity** | Every runner decodes `.nxb` and matches `.expected.json` field-by-field |
| **Sparse bitmask** | `sparse` vector — 100 records, random field subsets |
| **Negative handling** | `bad_magic`, `bad_dict_hash`, `truncated` |
| **NYXB magic** | Vectors generated with `0x4E595842` file magic (not legacy `NXSB`) |

### Not yet in conformance (roadmap)

| Rule | Status |
|------|--------|
| **Round-trip (driver write → core read)** | Use `nxs-import` / `nxs-inspect` manually; not gated in CI matrix |
| **Delta-patch offset validation** | No vector or runner |
| **Writer symmetry per driver** | Writers tested via language smoke tests (`make -C nyxis-drivers test`), not shared corpus |

Add these as new vectors + runner steps before claiming “bulletproof” cross-repo alignment.

---

## 5. When to regenerate vectors

Regenerate after any change to:

- `SPEC.md` / binary layout
- `gen_conformance` or Rust writer/compiler magic bytes
- Schema sigils or tail-index layout

```bash
make -C nyxis conformance-generate
git add nyxis/conformance/*.nxb nyxis/conformance/*.expected.json
```

Commit regenerated `.nxb` binaries with the spec change so driver CI consumes a fixed artifact (or rely on `generate-vectors` in CI only — current workflow uploads fresh artifacts per SHA).

---

## 6. Related docs

- [GOVERNANCE.md](GOVERNANCE.md) — repo split and licensing
- [nyxis/conformance/README.md](nyxis/conformance/README.md) — JSON schemas and vector table
- [nyxis/COMMERCIAL.md](nyxis/COMMERCIAL.md) — production licensing thresholds
