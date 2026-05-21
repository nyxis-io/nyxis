#!/usr/bin/env bash
# Create bench venv with pinned deps (avoids broken Homebrew Python 3.14 pip).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VENV="${VENV:-$ROOT/.venv-bench}"
PY="${PYTHON:-}"

if [ -z "$PY" ]; then
  for c in python3.12 python3.11 python3; do
    if command -v "$c" >/dev/null && "$c" -c "import sys; exit(0 if sys.version_info < (3, 14) else 1)" 2>/dev/null; then
      PY="$c"
      break
    fi
  done
fi
[ -n "$PY" ] || { echo "need python3.11 or 3.12" >&2; exit 1; }

"$PY" -m venv "$VENV"
# shellcheck disable=SC1091
source "$VENV/bin/activate"
pip install -U pip
pip install -r "$ROOT/bench/generators/requirements.txt"
bash "$ROOT/bench/generators/codegen.sh"
echo "venv ready: source $VENV/bin/activate"
