#!/usr/bin/env bash
# Create bench venv with pinned deps (avoids broken Homebrew Python 3.14 pip).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VENV="${VENV:-$ROOT/.venv-bench}"
PY="${PYTHON:-}"

if [ -z "$PY" ]; then
  for c in python3.12 python3.11; do
    if command -v "$c" >/dev/null 2>&1; then
      PY="$c"
      break
    fi
  done
fi
if [ -n "$PY" ]; then
  "$PY" -c "import sys; assert (3, 11) <= sys.version_info[:2] <= (3, 12)" 2>/dev/null || PY=
fi
[ -n "$PY" ] || {
  echo "need python3.11 or 3.12 (pyarrow has no cp313 wheel under pyarrow<18)" >&2
  echo "  Ubuntu: sudo apt install python3.12 python3.12-venv" >&2
  echo "  then: rm -rf .venv-bench && PYTHON=python3.12 bash bench/scripts/setup_venv.sh" >&2
  exit 1
}

"$PY" -m venv "$VENV"
# shellcheck disable=SC1091
source "$VENV/bin/activate"
pip install -U pip
pip install -r "$ROOT/bench/generators/requirements.txt"
bash "$ROOT/bench/generators/codegen.sh"
echo "venv ready: source $VENV/bin/activate"
