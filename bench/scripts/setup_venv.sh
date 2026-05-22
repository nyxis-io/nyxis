#!/usr/bin/env bash
# Create bench venv with pinned deps (avoids broken Homebrew Python 3.14 pip).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VENV="${VENV:-$ROOT/.venv-bench}"
PY="${PYTHON:-}"

py_ok() {
  command -v "$1" >/dev/null 2>&1 \
    && "$1" -c 'import sys; exit(0 if (3, 11) <= sys.version_info[:2] <= (3, 13) else 1)' 2>/dev/null
}

if [ -z "$PY" ]; then
  for c in python3.12 python3.13 python3.11 python3; do
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
  echo "need python3.11, 3.12, or 3.13 (pyarrow>=18; 3.14+ not supported here)" >&2
  exit 1
}

"$PY" -m venv "$VENV"
# shellcheck disable=SC1091
source "$VENV/bin/activate"
pip install -U pip
pip install -r "$ROOT/bench/generators/requirements.txt"
bash "$ROOT/bench/generators/codegen.sh"
echo "venv ready: source $VENV/bin/activate"
