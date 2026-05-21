#!/usr/bin/env bash
# Run full matrix and write bench/results/<date>_<host>/{raw,summary.json,methodology.md}
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
export BENCH_RECORDS="${BENCH_RECORDS:-10000}"
export RESULT_TAG="${RESULT_TAG:-$(date +%Y-%m-%d)_$(hostname -s)}"
export OUT="${OUT:-bench/results/${RESULT_TAG}}"

if [ -x "$ROOT/.venv-bench/bin/python" ]; then
  export PY="$ROOT/.venv-bench/bin/python"
elif [ -f "$ROOT/.venv-bench/bin/activate" ]; then
  # shellcheck disable=SC1091
  source "$ROOT/.venv-bench/bin/activate"
fi
export PY="${PY:-python3}"

# Setup once here; run_all only runs harness when SKIP_SETUP=1.
make -C bench gen BENCH_RECORDS="$BENCH_RECORDS"
make -C bench transcode BENCH_RECORDS="$BENCH_RECORDS"
make -C bench harness

export SKIP_SETUP=1
export SKIP_REPORT=1
bash bench/scripts/run_all.sh

"$PY" bench/scripts/report.py --results "$OUT" --raw "$OUT/raw/run.log" || true
"$PY" bench/scripts/verdicts.py "$OUT/summary.json" --write "$OUT/outcomes.json" || true
cat bench/methodology/workload_*.md >"$OUT/methodology.md" 2>/dev/null || true
echo "saved → $OUT/summary.json (+ outcomes.json)"
echo "tables: make -C bench render-all RESULT_DIR=$OUT"
