#!/usr/bin/env bash
# Run benchmark matrix: Workload B, C, A, D (set BENCH_D=0 to skip D).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
if [ -x "$ROOT/.venv-bench/bin/python" ]; then
  PY="$ROOT/.venv-bench/bin/python"
elif [ -f "$ROOT/.venv-bench/bin/activate" ]; then
  # shellcheck disable=SC1091
  source "$ROOT/.venv-bench/bin/activate"
  PY="${PY:-python3}"
else
  PY="${PY:-python3}"
fi
export PY

maybe_drop_caches() {
  local met="$1"
  [ "${BENCH_COLD:-0}" = "1" ] || return 0
  [ "$met" = "open" ] || return 0
  bash "$ROOT/bench/scripts/drop_caches.sh" || true
}

BENCH_RECORDS="${BENCH_RECORDS:-10000}"
export BENCH_RECORDS
RESULT_TAG="${RESULT_TAG:-$(date +%Y-%m-%d)_$(hostname -s)}"
OUT="${OUT:-bench/results/${RESULT_TAG}}"
mkdir -p "$OUT/raw"
LOG="$OUT/raw/run.log"
: >"$LOG"

echo "Results → $OUT (BENCH_RECORDS=$BENCH_RECORDS)" | tee -a "$LOG"

# Single tee per command — avoid duplicate lines when stdout is also redirected.
run() {
  {
    echo "=== $* ==="
    "$@" || true
  } 2>&1 | tee -a "$LOG"
}

run_open() {
  maybe_drop_caches open
  run "$@"
}

HARNESS_PY=("$PY" bench/harness/python/harness.py)
HC=bench/harness/c/harness
HR=bench/harness/rust/target/release/bench-harness

py_harness() {
  local wl="$1" fmt="$2" met="$3"
  shift 3
  local extra=("$@")
  if [ "$fmt" = nxs ] && [ "$met" = scan ] && [ "$BENCH_RECORDS" -ge 100000 ] && [ -x "$HR" ]; then
    echo "=== $HR --workload $wl --format nxs --records $BENCH_RECORDS --metric scan ... ===" | tee -a "$LOG"
    {
      "$HR" --workload "$wl" --format nxs --records "$BENCH_RECORDS" --metric scan \
        --data-dir bench/data/bin "${extra[@]}" || true
    } 2>&1 | tee -a "$LOG"
    return
  fi
  if [ "$fmt" = fb ] && [ "$met" = scan ] && [ "$BENCH_RECORDS" -ge 100000 ] && [ "${BENCH_FULL:-0}" != 1 ]; then
    echo "=== skip fb scan at $BENCH_RECORDS (set BENCH_FULL=1 for Python FB scan) ===" | tee -a "$LOG"
    return
  fi
  if [ "$met" = open ]; then
    run_open "${HARNESS_PY[@]}" --workload "$wl" --format "$fmt" --records "$BENCH_RECORDS" \
      --metric "$met" --data-dir bench/data/bin "${extra[@]}"
  else
    run "${HARNESS_PY[@]}" --workload "$wl" --format "$fmt" --records "$BENCH_RECORDS" \
      --metric "$met" --data-dir bench/data/bin "${extra[@]}"
  fi
}

if [ "${SKIP_SETUP:-0}" != 1 ]; then
  make -C bench gen BENCH_RECORDS="$BENCH_RECORDS" | tee -a "$LOG"
  make -C bench transcode BENCH_RECORDS="$BENCH_RECORDS" | tee -a "$LOG"
  make -C bench harness | tee -a "$LOG"
else
  echo "SKIP_SETUP=1 (fixtures/harness assumed ready)" | tee -a "$LOG"
fi

# Workload B — cold-open suite
for fmt in nxs proto fb capnp; do
  for met in size open access scan; do
    py_harness B "$fmt" "$met"
  done
done

# Workload C — dense reducer (+ Arrow)
for fmt in nxs proto capnp arrow; do
  for met in size open scan; do
    py_harness C "$fmt" "$met"
  done
done
run "${HARNESS_PY[@]}" --workload C --format arrow --records "$BENCH_RECORDS" \
  --metric distinct --data-dir bench/data/bin

# NXS cross-language check (Workload B)
for met in open access scan; do
  if [ "$met" = scan ] && [ "$BENCH_RECORDS" -ge 100000 ] && [ -x "$HR" ]; then
    run "$HR" --workload B --format nxs --records "$BENCH_RECORDS" \
      --metric scan --data-dir bench/data/bin
    continue
  fi
  if [ "$met" = open ]; then
    run_open "$HC" --workload B --format nxs --records "$BENCH_RECORDS" \
      --metric "$met" --data-dir bench/data/bin
  else
    run "$HC" --workload B --format nxs --records "$BENCH_RECORDS" \
      --metric "$met" --data-dir bench/data/bin
  fi
done
if [ -x "$HR" ]; then
  for met in open access scan; do
    if [ "$met" = open ]; then
      run_open "$HR" --workload B --format nxs --records "$BENCH_RECORDS" \
        --metric "$met" --data-dir bench/data/bin
    else
      run "$HR" --workload B --format nxs --records "$BENCH_RECORDS" \
        --metric "$met" --data-dir bench/data/bin
    fi
  done
fi
if [ -d ../nyxis-drivers/go ]; then
  for met in open access scan; do
    if [ "$met" = open ]; then
      run_open bash -c "cd ../nyxis-drivers/go && go run ../../nyxis/bench/harness/go/main.go \
        --workload B --format nxs --records $BENCH_RECORDS --metric $met \
        --data-dir $ROOT/bench/data/bin"
    else
      run bash -c "cd ../nyxis-drivers/go && go run ../../nyxis/bench/harness/go/main.go \
        --workload B --format nxs --records $BENCH_RECORDS --metric $met \
        --data-dir $ROOT/bench/data/bin"
    fi
  done
fi

# Workload A — sparse sizes + selective read at four population rates
for pop in 0.10 0.25 0.50 0.90; do
  for fmt in nxs proto fb capnp; do
    py_harness A "$fmt" size --population "$pop"
    py_harness A "$fmt" selective --population "$pop"
  done
  if [ -x "$HC" ]; then
    run "$HC" --workload A --format nxs --records "$BENCH_RECORDS" \
      --population "$pop" --metric selective --data-dir bench/data/bin
  fi
done

# Workload D — streaming TTFR (Rust stream_d; ~1–3 min at 10k rows)
if [ "${BENCH_D:-1}" != "0" ]; then
  BENCH_RECORDS_D="${BENCH_RECORDS_D:-$BENCH_RECORDS}"
  echo "Workload D → run-d-smoke (BENCH_RECORDS_D=$BENCH_RECORDS_D)" | tee -a "$LOG"
  run make -C bench run-d-smoke BENCH_RECORDS_D="$BENCH_RECORDS_D"
  echo "Workload D → run-d-ttfr (n=1000, batched flush, publication TTFR+seal)" | tee -a "$LOG"
  run make -C bench run-d-ttfr BENCH_RECORDS_D="$BENCH_RECORDS_D"
  echo "Workload D → run-d-throughput (batched flush, publication throughput)" | tee -a "$LOG"
  run make -C bench run-d-throughput BENCH_RECORDS_D="$BENCH_RECORDS_D"
fi

if [ -z "${SKIP_REPORT:-}" ]; then
  "$PY" bench/scripts/report.py --results "$OUT" --raw "$LOG" || true
  cat bench/methodology/workload_*.md >"$OUT/methodology.md" 2>/dev/null || true
fi
echo "Done → $OUT"
