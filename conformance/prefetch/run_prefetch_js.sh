#!/usr/bin/env bash
# Prefetch conformance runner (JavaScript) — stub until nyxis-drivers/js prefetch lands.
#
# Usage: conformance/prefetch/run_prefetch_js.sh
# Env:   DRV — path to nyxis-drivers (default: ../../nyxis-drivers from repo root)
#
# When implemented, this script will invoke Node with a fetch recorder and assert:
#   - prefetch_viewport_basic on prefetch_sparse_50.nxb
#   - prefetch_range_coalescing (≤3 range fetches for viewport 0..49)
#   - prefetch_deduplication, prefetch_memory_eviction
#   - prefetch_sequential_upgrade on prefetch_sequential_upgrade.nxb (phase 2:
#     150 sequential record() calls → strategy eager, pattern sequential)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CONF="${ROOT}/conformance/prefetch"
DRV="${DRV:-${ROOT}/../nyxis-drivers}"
FIXTURE="${CONF}/prefetch_sparse_50.nxb"
SEQ_FIXTURE="${CONF}/prefetch_sequential_upgrade.nxb"

if [[ ! -f "${FIXTURE}" ]]; then
  echo "missing fixture: ${FIXTURE} (run: make conformance-generate)" >&2
  exit 1
fi

if [[ ! -f "${SEQ_FIXTURE}" ]]; then
  echo "missing phase-2 fixture: ${SEQ_FIXTURE} (run: make conformance-generate)" >&2
  exit 1
fi

if [[ -f "${DRV}/js/prefetch.js" ]] && [[ -f "${DRV}/js/test/prefetch.test.js" ]]; then
  echo "prefetch JS driver detected — running prefetch tests..."
  (cd "${DRV}/js" && node --test test/prefetch.test.js)
  # TODO: wire conformance vectors + fetch recorder once NxsReader.open() prefetch path exists
  echo "prefetch conformance vector runner: not yet wired (driver unit tests only)"
  echo "  phase-2 fixture ready: ${SEQ_FIXTURE##*/} ($(wc -c <"${SEQ_FIXTURE}" | tr -d ' ') bytes)"
else
  echo "prefetch JS runner: stub OK (fixture ${FIXTURE##*/}, $(wc -c <"${FIXTURE}" | tr -d ' ') bytes)"
  echo "  phase-2 fixture: ${SEQ_FIXTURE##*/} ($(wc -c <"${SEQ_FIXTURE}" | tr -d ' ') bytes)"
  echo "  DRV=${DRV} — awaiting prefetch.js + NxsReader.open() integration"
fi

exit 0
