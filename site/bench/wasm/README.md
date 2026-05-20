# NXS WASM Reducers

The `nxs_reducers.wasm` binary provides fast-path column scan reducers compiled from
`nxs_reducers.c` using Clang targeting WebAssembly.

## Building locally

Requires LLVM/Clang with the `wasm32-unknown-unknown` target:

```bash
# macOS
brew install llvm
bash site/bench/wasm/build.sh

# Ubuntu / CI
apt-get install -y clang lld
bash site/bench/wasm/build.sh
```

## Downloading from CI

Every push to `main` triggers the `build-wasm.yml` workflow, which builds
`nxs_reducers.wasm` and commits it back to `site/bench/wasm/`. You can download the
artifact directly from the GitHub Actions run if you need it before the commit
lands.

The pre-built binary in this directory is the output of that CI step and is
committed to the repository so consumers don't need a local LLVM toolchain.
