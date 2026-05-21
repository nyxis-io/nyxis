#!/usr/bin/env bash
# Run all NXS benchmarks one at a time (2s cooldown between runs).
# Parallel benches contend for CPU/cache and skew results.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DRV="${DRV:-$(cd "$ROOT/../nyxis-drivers" 2>/dev/null && pwd)}"
FIX="${FIX:-$ROOT/site/bench/fixtures}"
LOG="${LOG:-$ROOT/bench-sequential.log}"

if [[ ! -d "$DRV" ]]; then
  echo "nyxis-drivers not found at $DRV (set DRV=…)" >&2
  exit 1
fi

if [[ ! -f "$FIX/records_1000000.nxb" ]]; then
  echo "Generating 1M fixtures in $FIX …"
  make -C "$ROOT" fixtures FIXTURE_COUNT=1000000
fi

run() {
  echo ""
  echo "══════════════════════════════════════════════════════════════"
  echo ">>> $1"
  echo "══════════════════════════════════════════════════════════════"
  shift
  "$@" || echo "WARNING: $1 exited with status $?"
  sleep 2
}

: >"$LOG"
exec > >(tee -a "$LOG") 2>&1

echo "Sequential benchmark run — $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo "FIX=$FIX  DRV=$DRV  LOG=$LOG"

# Native extensions (idempotent)
[[ -f "$DRV/py"/_nxs*.so ]] || (cd "$DRV/py" && bash build_ext.sh)
[[ -f "$DRV/ruby/ext/nxs/nxs_ext.bundle" || -f "$DRV/ruby/ext/nxs/nxs_ext.so" ]] \
  || (cd "$DRV/ruby" && bash ext/build.sh)
[[ -f "$DRV/php/nxs_ext/modules/nxs.so" ]] || (cd "$DRV/php" && bash nxs_ext/build.sh)

run "1/11 Rust" bash -c "cd '$ROOT/rust' && cargo run --release --bin bench"
run "2/11 C" bash -c "cd '$DRV/c' && make bench -s && ./bench '$FIX'"
run "3/11 Go" bash -c "cd '$DRV/go' && go run ./cmd/bench '$FIX'"
run "4/11 Python" bash -c "cd '$DRV/py' && python3 bench_c.py '$FIX'"
run "5/11 Ruby" bash -c "ruby '$DRV/ruby/bench_c.rb' '$FIX'"
run "6/11 PHP" bash -c "php -d extension='$DRV/php/nxs_ext/modules/nxs.so' -d memory_limit=2G '$DRV/php/bench_c.php' '$FIX'"
run "7/11 Swift" bash -c "cd '$DRV/swift' && swift run -c release nxs-bench '$FIX'"
run "8/11 Kotlin" bash -c "cd '$DRV/kotlin' && ./gradlew bench -q"

# C# smoke tests expect records_1000.*; symlink 1M files for --bench only
ln -sf records_1000000.nxb "$FIX/records_1000.nxb"
ln -sf records_1000000.json "$FIX/records_1000.json"
run "9/11 C#" bash -c "cd '$DRV/csharp' && dotnet run -c Release -- '$FIX' --bench"
rm -f "$FIX/records_1000.nxb" "$FIX/records_1000.json"

run "10/11 Node.js" bash -c "cd '$ROOT' && node site/bench/bench.js '$FIX'"
run "11a/11 WAL C" bash -c "cd '$DRV/c' && cc -O2 -std=c99 bench_wal.c nxs_writer.c -o bench_wal && ./bench_wal"
run "11b/11 WAL Go" bash -c "cd '$DRV/go' && go test -run TestWalBench -v -count=1"
run "11c/11 WAL Python" bash -c "cd '$DRV/py' && python3 bench_wal.py"
run "11d/11 WAL Ruby" bash -c "ruby '$DRV/ruby/bench_wal.rb'"

echo ""
echo "Done. Full log: $LOG"
