#!/usr/bin/env bash
# Sync bench Python deps with uv (see bench/generators/pyproject.toml).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GEN="$ROOT/bench/generators"
VENV="${VENV:-$ROOT/.venv-bench}"
PY="${PYTHON:-}"
UV="${UV:-uv}"

if ! command -v "$UV" >/dev/null 2>&1; then
  echo "uv not found (install: https://docs.astral.sh/uv/getting-started/installation/)" >&2
  exit 1
fi

py_ok() {
  command -v "$1" >/dev/null 2>&1 \
    && "$1" -c 'import sys; exit(0 if (3, 11) <= sys.version_info[:2] <= (3, 14) else 1)' 2>/dev/null
}

if [ -z "$PY" ]; then
  for c in python3.12 python3.13 python3.14 python3.11 python3; do
    if py_ok "$c"; then
      PY="$c"
      break
    fi
  done
fi
if [ -n "$PY" ] && ! py_ok "$PY"; then
  PY=
fi
[ -n "$PY" ] || {
  echo "need python3.11–3.14 (pyarrow pins in bench/generators/pyproject.toml)" >&2
  exit 1
}

export UV_PROJECT_ENVIRONMENT="$VENV"
sync_args=(sync --directory "$GEN")
if [ -n "${BENCH_UV_FROZEN:-}" ]; then
  sync_args+=(--frozen)
fi
if [ -n "${BENCH_WITH_CAPNP:-}" ]; then
  sync_args+=(--group capnp)
fi
if [ -n "$PY" ]; then
  sync_args+=(--python "$PY")
fi
"$UV" "${sync_args[@]}"
bash "$GEN/codegen.sh"
ver="$("$VENV/bin/python" -c 'import sys; print(".".join(map(str, sys.version_info[:3])))')"
echo "venv ready: $VENV ($ver)"
if [ -z "${BENCH_WITH_CAPNP:-}" ] && ! "$VENV/bin/python" -c 'import capnp' 2>/dev/null; then
  echo "note: pycapnp skipped (fast sync). Cap'n Proto Python harness: PYTHON=python3.12 BENCH_WITH_CAPNP=1 $0" >&2
fi
