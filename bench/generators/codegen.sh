#!/usr/bin/env bash
# Generate language bindings from bench/schemas. Skips tools not on PATH.
set -euo pipefail
BENCH="$(cd "$(dirname "$0")/.." && pwd)"
SCHEMAS="$BENCH/schemas"
GEN="$BENCH/generators/generated"
mkdir -p "$GEN"

if command -v protoc >/dev/null; then
  protoc -I"$SCHEMAS" --python_out="$GEN" \
    "$SCHEMAS/flat8.proto" \
    "$SCHEMAS/sparse.proto" \
    "$SCHEMAS/dense8.proto"
  echo "protobuf: ok → $GEN"
else
  echo "protobuf: skip (protoc not found)" >&2
fi

if command -v flatc >/dev/null; then
  flatc --python -o "$GEN" \
    "$SCHEMAS/flat8.fbs" \
    "$SCHEMAS/sparse.fbs" \
    "$SCHEMAS/dense8.fbs"
  echo "flatbuffers: ok → $GEN"
else
  echo "flatbuffers: skip (flatc not found)" >&2
fi

if command -v capnp >/dev/null; then
  if capnp compile -o"$GEN" -I"$SCHEMAS" \
    "$SCHEMAS/flat8.capnp" \
    "$SCHEMAS/sparse.capnp" \
    "$SCHEMAS/dense8.capnp" 2>/dev/null; then
    echo "capnp: ok → $GEN"
  else
    echo "capnp: compile skipped (pycapnp loads schemas/ at runtime)" >&2
  fi
else
  echo "capnp: skip (capnp not found; pycapnp can still load .capnp at runtime)" >&2
fi

touch "$GEN/.codegen_stamp"
