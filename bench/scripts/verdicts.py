#!/usr/bin/env python3
"""Derive win/tie/loss verdicts for NXS vs competitors from summary.json."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

TIE_RATIO = 1.05  # within 5% counts as tie


def value(row: dict) -> float:
    if "bytes" in row:
        return float(row["bytes"])
    return float(row["p50_ns"])


def verdict(nxs: float, best_other: float, lower_is_better: bool) -> str:
    if best_other <= 0:
        return "win"
    if lower_is_better:
        if nxs <= best_other:
            return "win"
        if nxs <= best_other * TIE_RATIO:
            return "tie"
        return "loss"
    if nxs >= best_other:
        return "win"
    if nxs >= best_other / TIE_RATIO:
        return "tie"
    return "loss"


def build(rows: list[dict]) -> list[dict]:
    groups: dict[tuple, list[dict]] = {}
    for r in rows:
        if r.get("format") == "arrow" and r.get("workload") != "C":
            continue
        key = (
            r["workload"],
            r["metric"],
            r.get("population", -1),
            r.get("records"),
        )
        groups.setdefault(key, []).append(r)

    out: list[dict] = []
    for (wl, met, pop, rec), items in sorted(groups.items()):
        nxs_rows = [x for x in items if x["format"] == "nxs"]
        others = [x for x in items if x["format"] != "nxs"]
        if not nxs_rows or not others:
            continue
        nxs_row = nxs_rows[0]
        nxs_v = value(nxs_row)
        lower = met in ("size", "open", "access", "selective", "scan", "distinct")
        best = min(others, key=value)
        best_v = value(best)
        v = verdict(nxs_v, best_v, lower)
        out.append(
            {
                "workload": wl,
                "metric": met,
                "population": pop,
                "records": rec,
                "nxs": nxs_row["format"],
                "nxs_value": nxs_v,
                "best_format": best["format"],
                "best_value": best_v,
                "verdict": v,
            }
        )
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("summary", type=Path)
    ap.add_argument("--write", type=Path, help="also write outcomes.json next to summary")
    args = ap.parse_args()
    data = json.loads(args.summary.read_text())
    rows = data.get("measurements", [])
    outcomes = build(rows)
    payload = {"verdicts": outcomes, "losses": sum(1 for o in outcomes if o["verdict"] == "loss")}
    print(json.dumps(payload, indent=2))
    if args.write:
        args.write.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        print(f"wrote {args.write}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
