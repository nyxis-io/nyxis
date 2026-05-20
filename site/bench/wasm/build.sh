#!/usr/bin/env bash
# Build nxs_reducers.wasm — freestanding, no libc, no Emscripten, no wasi-sdk.
# Requires: brew install llvm lld
set -euo pipefail
cd "$(dirname "$0")"

CLANG="${CLANG:-/opt/homebrew/opt/llvm@21/bin/clang}"
WASMLD="${WASMLD:-/opt/homebrew/opt/lld/bin/wasm-ld}"

if [[ ! -x "$CLANG" ]]; then
  CLANG=$(command -v clang)
fi

echo "Compiling nxs_reducers.c → nxs_reducers.wasm"
# Put wasm-ld on PATH so clang's default wasm driver picks it up.
export PATH="$(dirname "$WASMLD"):$PATH"

"$CLANG" \
  --target=wasm32 \
  -O3 \
  -nostdlib \
  -fno-builtin \
  -fvisibility=hidden \
  -Wl,--no-entry \
  -Wl,--export-dynamic \
  -Wl,--allow-undefined \
  -Wl,--import-memory \
  -o nxs_reducers.wasm \
  nxs_reducers.c

size=$(stat -f %z nxs_reducers.wasm 2>/dev/null || stat -c %s nxs_reducers.wasm)
echo "Built nxs_reducers.wasm  ($size bytes)"
