#!/usr/bin/env bash
# Linux only: drop page cache before cold-open runs (requires root).
set -euo pipefail
if [[ "$(uname -s)" != Linux ]]; then
  echo "drop_caches: skipped (not Linux)" >&2
  exit 0
fi
sync
echo 3 > /proc/sys/vm/drop_caches
echo "page cache dropped"
