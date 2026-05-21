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

# Prefer explicit driver tags when deduping cross-language reruns (c > rust > go > python).
DRIVER_PRIORITY = ("c", "rust", "go", "python", "stream_d")


def _driver_rank(row: dict) -> int:
    drv = row.get("driver", "python")
    try:
        return DRIVER_PRIORITY.index(drv)
    except ValueError:
        return len(DRIVER_PRIORITY)


def _dedupe_key(row: dict) -> tuple:
    return (
        row.get("workload"),
        row.get("format"),
        row.get("metric"),
        row.get("records"),
        row.get("population", -1),
    )


def normalize_row(row: dict) -> dict:
    """Map legacy JSONL to current metric names."""
    row = dict(row)
    fe = row.get("flush_every")
    samples = int(row.get("samples", 0) or 0)
    met = row.get("metric")
    rec = int(row.get("records", 0) or 0)
    seal_n = int(row.get("seal_records", 0) or 0)

    if met == "throughput" and fe is not None and int(fe) < 100:
        row["metric"] = "throughput_smoke"
    if met == "ttfr" and (
        (fe is not None and int(fe) < 100) or (samples > 0 and samples < 1000)
    ):
        row["metric"] = "ttfr_smoke"
    if met == "seal" and (
        (fe is not None and int(fe) < 100)
        or (rec > 0 and seal_n > 0 and seal_n < rec * 9 // 10)
    ):
        row["metric"] = "seal_smoke"
    return row


def dedupe_rows(rows: list[dict]) -> list[dict]:
    deduped: dict[tuple, dict] = {}
    for r in rows:
        key = _dedupe_key(r)
        prev = deduped.get(key)
        if prev is None or _driver_rank(r) < _driver_rank(prev):
            deduped[key] = r
    return list(deduped.values())


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
                rows.append(normalize_row(json.loads(line)))

    rows = dedupe_rows(rows)

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
