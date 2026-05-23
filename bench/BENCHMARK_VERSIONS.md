# Benchmark dependency versions

Pinned toolchains for reproducible head-to-head runs. Re-run after bumps with explicit
`previous_version` / `current_version` columns in `results/`.

| Component | Pinned version | Notes |
| --- | --- | --- |
| Nyxis spec | v1.1 | `.nxb` wire format; compiler + drivers at release SHA |
| `protoc` / libprotobuf | 27.x / protobuf 7.x (Python) | Generated accessors only (no reflection on hot paths) |
| FlatBuffers | `google/flatbuffers` v24.3.25+ | Generated `*_reader.h`; verifier off hot path |
| Cap'n Proto | 1.0.x | Packed encoding where schema allows |
| Apache Arrow IPC | 18.x (3.11–3.13) / 22+ (3.14) | Workload C columnar comparator (`pyarrow` env markers in `pyproject.toml`) |
| Rust | 1.75+ | `--release`, LTO via `generators/transcode_rust/Cargo.toml` |
| Go | 1.22+ | Document `GOAMD64=v3` if used |
| C compiler | GCC 13+ or Clang 16+ | `-O3 -march=native -flto` primary; `-O2 -march=x86-64-v3` portable |

Lockfiles:

- `generators/pyproject.toml` + `generators/uv.lock` — Python transcoder (`uv sync`; optional `capnp` group for `pycapnp`)
- `generators/transcode_rust/Cargo.lock` — NXB writer from canonical JSON
- `harness/rust/Cargo.lock` — multi-format harness
