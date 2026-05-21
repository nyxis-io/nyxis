#!/usr/bin/env python3
"""Aggregate JSON-line harness output into summary.json and CSV."""

from __future__ import annotations

import argparse
import csv
import json
import re
import sys
from pathlib import Path

LINE_RE = re.compile(r"^\{.*\}$")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--results", type=Path, required=True)
    ap.add_argument("--raw", type=Path, nargs="*", default=[])
    args = ap.parse_args()

    rows: list[dict] = []
    for raw in args.raw:
        if not raw.exists():
            continue
        for line in raw.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if LINE_RE.match(line):
                rows.append(json.loads(line))

    args.results.mkdir(parents=True, exist_ok=True)
    summary = {"samples": len(rows), "measurements": rows}
    (args.results / "summary.json").write_text(
        json.dumps(summary, indent=2), encoding="utf-8"
    )

    if rows:
        csv_path = args.results / "raw" / "measurements.csv"
        csv_path.parent.mkdir(parents=True, exist_ok=True)
        keys = sorted({k for r in rows for k in r})
        with csv_path.open("w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=keys, extrasaction="ignore")
            w.writeheader()
            w.writerows(rows)
        print(f"wrote {csv_path} ({len(rows)} rows)", file=sys.stderr)

    print(f"wrote {args.results / 'summary.json'}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
