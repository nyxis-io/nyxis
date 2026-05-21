#!/usr/bin/env python3
"""Render bench/results/*/summary.json as markdown tables for BENCHMARK.md."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


def ns_us(v: int) -> str:
    if v >= 1_000_000:
        return f"{v / 1e6:.2f} ms"
    if v >= 1_000:
        return f"{v / 1e3:.1f} µs"
    return f"{v} ns"


def render_verdicts(outcomes_path: Path) -> None:
    data = json.loads(outcomes_path.read_text())
    verdicts = data.get("verdicts", [])
    if not verdicts:
        return

    counts = Counter(v["verdict"] for v in verdicts)
    wins = counts.get("win", 0)
    ties = counts.get("tie", 0)
    losses = counts.get("loss", 0)

    print("\n### NXS verdicts\n")
    print(f"**{wins}** wins · **{ties}** ties · **{losses}** losses")
    print(f"_(from `{outcomes_path.parent.name}/outcomes.json`, within 5% = tie)_\n")

    by_wl: dict[str, list[dict]] = {}
    for v in verdicts:
        by_wl.setdefault(v["workload"], []).append(v)

    print("| Workload | Wins | Ties | Losses | Primary winner (losses) |")
    print("| --- | ---: | ---: | ---: | --- |")
    for wl in ("A", "B", "C"):
        items = by_wl.get(wl)
        if not items:
            continue
        wc = Counter(x["verdict"] for x in items)
        loss_rows = [x for x in items if x["verdict"] == "loss"]
        winners: Counter[str] = Counter()
        for row in loss_rows:
            winners[row["best_format"]] += 1
        if winners:
            primary = ", ".join(
                f"{fmt} ({n})" for fmt, n in winners.most_common()
            )
        else:
            primary = "—"
        print(
            f"| {wl} | {wc.get('win', 0)} | {wc.get('tie', 0)} | "
            f"{wc.get('loss', 0)} | {primary} |"
        )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("summary", type=Path)
    args = ap.parse_args()
    data = json.loads(args.summary.read_text())
    rows = data.get("measurements", [])

    by_wl: dict[str, list] = {}
    for r in rows:
        by_wl.setdefault(r["workload"], []).append(r)

    for wl in ("B", "C", "A"):
        if wl not in by_wl:
            continue
        print(f"\n### Workload {wl} (from `{args.summary.parent.name}`)\n")
        items = by_wl[wl]
        if wl == "A":
            pops = sorted({r.get("population", -1) for r in items if r.get("population", -1) >= 0})
            fmts = sorted({r["format"] for r in items})
            for met in ("size", "selective"):
                sub = [r for r in items if r["metric"] == met]
                if not sub:
                    continue
                label = "File size" if met == "size" else "Selective read P50"
                print(f"\n**{label}**\n")
                hdr = "| Pop | " + " | ".join(fmts) + " |"
                print(hdr)
                print("| --- | " + " | ".join(["---"] * len(fmts)) + " |")
                for pop in pops:
                    cells = [f"{int(pop * 100)}%"]
                    for fmt in fmts:
                        hit = next(
                            (
                                r
                                for r in sub
                                if r["format"] == fmt and abs(r.get("population", -2) - pop) < 0.001
                            ),
                            None,
                        )
                        if not hit:
                            cells.append("—")
                        elif met == "size":
                            cells.append(f"{hit['bytes']/1e6:.2f} MB")
                        else:
                            cells.append(ns_us(hit["p50_ns"]))
                    print("| " + " | ".join(cells) + " |")
            continue
        metrics = sorted({r["metric"] for r in items})
        fmts = sorted({r["format"] for r in items})
        hdr = "| Format | " + " | ".join(metrics) + " |"
        sep = "| --- | " + " | ".join(["---"] * len(metrics)) + " |"
        print(hdr)
        print(sep)
        for fmt in fmts:
            cells = [fmt]
            for met in metrics:
                hit = next((r for r in items if r["format"] == fmt and r["metric"] == met), None)
                if not hit:
                    cells.append("—")
                elif "bytes" in hit:
                    cells.append(f"{hit['bytes']/1e6:.2f} MB")
                else:
                    cells.append(ns_us(hit["p50_ns"]))
            print("| " + " | ".join(cells) + " |")

    outcomes = args.summary.parent / "outcomes.json"
    if outcomes.is_file():
        render_verdicts(outcomes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
