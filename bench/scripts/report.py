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


def _extract_workload_e(text: str) -> list[dict]:
    """Parse bench_pax_mixed multi-line JSON blocks from raw log text."""
    rows = []
    # Find all top-level {...} blocks that span multiple lines
    depth = 0
    start = None
    for i, ch in enumerate(text):
        if ch == "{":
            if depth == 0:
                start = i
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0 and start is not None:
                chunk = text[start : i + 1]
                try:
                    obj = json.loads(chunk)
                except json.JSONDecodeError:
                    start = None
                    continue
                if obj.get("workload") == "E" and "layouts" in obj:
                    driver = obj.get("driver", "rust")
                    records = obj.get("records")
                    for layout in obj["layouts"]:
                        for metric, key in (
                            ("access", "access_us"),
                            ("col_scan", "col_scan_us"),
                            ("mixed_total", "mixed_total_us"),
                        ):
                            block = layout.get(key)
                            if not isinstance(block, dict):
                                continue
                            rows.append({
                                "workload": "E",
                                "format": "nxs",
                                "layout": layout.get("layout"),
                                "records": records or layout.get("records"),
                                "metric": metric,
                                "population": -1.0,
                                "driver": driver,
                                "p50_us": block.get("p50"),
                                "p95_us": block.get("p95"),
                                "p99_us": block.get("p99"),
                                "samples": block.get("samples"),
                                "file_bytes": layout.get("file_bytes"),
                                "col_sum_checksum": obj.get("col_sum_checksum") or layout.get("col_sum_checksum"),
                            })
                start = None
    return rows


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
        row.get("layout"),
        row.get("scenario"),
        row.get("mode"),
    )


def normalize_row(row: dict) -> dict:
    """Map legacy JSONL to current metric names."""
    row = dict(row)
    if row.get("workload") == "F":
        row.setdefault("format", "nxs")
        row.setdefault("population", -1)
        val = row.get("value")
        unit = row.get("unit", "")
        if val is not None:
            if unit == "ms":
                row["value_ms"] = val
                row["p50_us"] = round(float(val) * 1000, 3)
            elif unit == "s":
                row["value_s"] = val
                row["p50_us"] = round(float(val) * 1e6, 1)
            elif unit == "MB":
                row["peak_sys_mb"] = val
        return row

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
        text = raw.read_text(encoding="utf-8")
        for line in text.splitlines():
            line = line.strip()
            if LINE_RE.match(line):
                rows.append(normalize_row(json.loads(line)))
        rows.extend(_extract_workload_e(text))

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
