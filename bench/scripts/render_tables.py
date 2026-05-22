#!/usr/bin/env python3
"""Render bench/results/*/summary.json as markdown tables for BENCHMARK.md."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

ZERO_COPY_FORMATS = ("nxs", "capnp", "fb")
WORKLOAD_C_FORMATS = ("arrow", "nxs", "capnp")
PROTO_FORMAT = "proto"
SUB_TIMER = "< 1 µs (below timer resolution)"
SCAN_PYTHON_REF = "† Python harness"
PROTO_FOOTNOTE = (
    "† Protobuf **access** and **scan** are measured on a **pre-parsed Python object graph** "
    "(not wire decode in the timed region). **Open** is full `ParseFromString` per sample. "
    "Not comparable to zero-copy access/scan for NXS, FlatBuffers, or Cap'n Proto."
)
PROTO_SELECTIVE_FOOTNOTE = (
    "† Protobuf **selective** uses attribute access on a **pre-parsed** message (same warm-object "
    "model as Workload B access). NXS selective uses the **C driver** zero-copy path (FNV key index "
    "+ per-record rank cache). Both may show `< 1 µs` on this hardware; the mechanisms are not comparable."
)
B_SCAN_REF_FOOTNOTE = (
    "† **Cap'n Proto / FlatBuffers scan** is measured with the **Python harness** (warm accessor loop). "
    "These numbers reflect Python overhead, not wire-format scan limits. "
    "Publication NXS scan uses the **C driver** (`nxs_sum_f64` / `scan_offset_bulk`)."
)
B_NXS_SCAN_NOTE = (
    "_NXS scan (`driver=c`): C `nxs_sum_f64` on flat-8 schema (~25 µs at 10k dev macOS). "
    "Earlier ~8.9 ms matrix rows were Python harness overhead._"
)
WORKLOAD_C_PROSE = (
    "_Arrow wins columnar scan by orders of magnitude — the expected architectural result. "
    "Route dense analytics to Arrow (NXS Arrow bridge for ingest). "
    "NXS and Cap'n Proto are row-oriented; scan reflects per-record traversal, not batch column ops._"
)
D_TTFR_NOTE = (
    "_Publication TTFR: **n=1000** trials, **flush_every=100** (batched flush), D2 file-on-disk, poll 50 µs. "
    "Smoke TTFR (20 trials, flush_every=1) is not shown — P50 can differ (e.g. Protobuf may lead on smoke). "
    "Earlier n=1000 **per-record flush** run showed Cap'n Proto winning P99 (252 µs vs 321 µs); "
    "this **batched flush** run shows NXS ahead at P99 (437 µs vs 583 µs). Flush policy affects tail behavior; "
    "do not claim NXS wins P99 until **Linux + inotify** confirms which result is stable under push notification._"
)
B_WARM_CACHE_NOTE = (
    "_NXS **open** and **access** at `< 1 µs` reflect **warm page cache** on this file size (~1.3 MB at 10k records) "
    "after the C harness has touched the file — not cold-open from disk. "
    "Cold-open latency at larger files is documented separately; on this hardware, initial header + tail-index "
    "mapping for a ~1.5 GB file is ~25 µs. Cap'n Proto / FlatBuffers open in the table above are Python harness "
    "samples on the same warm-cache conditions._"
)
PUBLICATION_SUMMARY = (
    "> These benchmarks cover four workloads: sparse record density and selective access (A), "
    "zero-copy warm access and scan (B), dense columnar analytics (C), and streaming ingest "
    "time-to-first-record (D). All results are macOS dev runs; Linux + inotify results are pending. "
    "NXS leads zero-copy peers on warm selective access (sub-microsecond with C driver vs Cap'n Proto ~3 µs "
    "and FlatBuffers ~8 µs), TTFR at P50/P95 in the batched streaming configuration, and file size at 50%+ "
    "field population. NXS loses on cold open vs Cap'n Proto and FlatBuffers, on file size at low population "
    "rates vs FlatBuffers, and on columnar analytics vs Arrow by orders of magnitude — the Arrow bridge is the "
    "correct tool for that workload. Protobuf results are shown as a post-parse reference; their access and scan "
    "times are not comparable to zero-copy measurements.\n"
)
D_THROUGHPUT_NOTE = (
    "_**throughput**: sustained rec/s from first complete record to last while the writer appends "
    "(flush_every=100). Smoke throughput (~200 rec/s) is omitted from publication._"
)
D_FB_NOTE = (
    "† FlatBuffers has no native file-level streaming (root offset at buffer start). "
    "With external per-record framing, TTFR is expected to match Cap'n Proto framed streaming."
)

DRIVER_PRIORITY = ("c", "rust", "go", "python", "stream_d")
PUBLICATION_METRICS = frozenset(
    {"size", "open", "access", "scan", "selective", "distinct", "ttfr", "seal", "throughput"}
)


def format_p50_ns(v: int) -> str:
    if v <= 0 or v < 1000:
        return SUB_TIMER
    return ns_us(v)


def ns_us(v: int) -> str:
    if v >= 1_000_000:
        return f"{v / 1e6:.2f} ms"
    if v >= 1_000:
        return f"{v / 1e3:.1f} µs"
    return f"{v} ns"


def pick_row(rows: list[dict], *, metric: str | None = None) -> dict | None:
    if not rows:
        return None
    if metric:
        rows = [r for r in rows if r.get("metric") == metric]
    by_drv = {r.get("driver", "python"): r for r in rows}
    for drv in DRIVER_PRIORITY:
        if drv in by_drv:
            return by_drv[drv]
    return rows[0]


def cell_value(hit: dict | None, met: str) -> str:
    if not hit:
        return "—"
    if met == "size" and "bytes" in hit:
        return f"{hit['bytes'] / 1e6:.2f} MB"
    if met in ("throughput", "throughput_smoke") and "p50_rec_per_s" in hit:
        return f"{hit['p50_rec_per_s']:.0f} rec/s"
    if met in ("ttfr", "ttfr_smoke", "seal", "seal_smoke") and "p50_us" in hit:
        return f"{hit['p50_us']} µs"
    if "p50_ns" in hit:
        return format_p50_ns(int(hit["p50_ns"]))
    return "—"


def render_metric_table(
    items: list[dict],
    fmts: tuple[str, ...],
    metrics: tuple[str, ...],
    title: str,
    *,
    cell_fn=None,
) -> None:
    print(f"\n**{title}**\n")
    hdr = "| Format | " + " | ".join(metrics) + " |"
    print(hdr)
    print("| --- | " + " | ".join(["---"] * len(metrics)) + " |")
    for fmt in fmts:
        cells = [fmt]
        for met in metrics:
            hits = [r for r in items if r["format"] == fmt and r.get("metric") == met]
            hit = pick_row(hits, metric=met)
            if cell_fn:
                cells.append(cell_fn(fmt, met, hit))
            else:
                cells.append(cell_value(hit, met))
        print("| " + " | ".join(cells) + " |")


def render_workload_b(items: list[dict]) -> None:
    zc = [f for f in ZERO_COPY_FORMATS if any(r["format"] == f for r in items)]
    if zc:
        render_metric_table(
            items,
            tuple(zc),
            ("open", "access", "size"),
            "Workload B — zero-copy warm access (open, access, size)",
        )
        print(f"\n{B_WARM_CACHE_NOTE}\n")
        nxs_scan = pick_row(
            [r for r in items if r["format"] == "nxs" and r.get("metric") == "scan"]
        )
        if nxs_scan:
            render_metric_table(
                items,
                ("nxs",),
                ("scan",),
                "Workload B — NXS scan (C driver, publication)",
            )
            print(f"\n{B_NXS_SCAN_NOTE}\n")
        capnp_fb = [f for f in ("capnp", "fb") if f in zc]
        if capnp_fb:

            def scan_ref_cell(fmt: str, met: str, hit: dict | None) -> str:
                if hit and "p50_ns" in hit:
                    return f"{format_p50_ns(int(hit['p50_ns']))} {SCAN_PYTHON_REF}"
                return "—"

            render_metric_table(
                items,
                tuple(capnp_fb),
                ("scan",),
                "Workload B — scan reference (Python harness — not wire-format limits)",
                cell_fn=scan_ref_cell,
            )
            print(f"\n{B_SCAN_REF_FOOTNOTE}\n")
    proto_rows = [r for r in items if r["format"] == PROTO_FORMAT]
    if proto_rows:
        render_metric_table(
            proto_rows,
            (PROTO_FORMAT,),
            ("open", "access", "scan", "size"),
            "Workload B — Protobuf (post-parse reference)",
        )
        print(f"\n{PROTO_FOOTNOTE}\n")


def render_workload_c(items: list[dict]) -> None:
    fmts = [f for f in WORKLOAD_C_FORMATS if any(r["format"] == f for r in items)]
    if fmts:
        render_metric_table(
            items,
            tuple(fmts),
            ("open", "scan", "size"),
            "Workload C — columnar vs row-oriented (Arrow required)",
        )
        print(f"\n{WORKLOAD_C_PROSE}\n")
    proto_rows = [r for r in items if r["format"] == PROTO_FORMAT]
    if proto_rows:
        render_metric_table(
            proto_rows,
            (PROTO_FORMAT,),
            ("open", "scan", "size"),
            "Workload C — Protobuf (post-parse reference)",
        )
        print(f"\n{PROTO_FOOTNOTE}\n")


def ttfr_cell(hit: dict | None, col: str) -> str:
    if not hit:
        return "—"
    key = f"{col}_us"
    if key in hit:
        return f"{hit[key]} µs"
    return "—"


def render_workload_d(items: list[dict], *, publication: bool) -> None:
    fmts = [f for f in ("nxs", "proto", "capnp", "fb") if any(r["format"] == f for r in items)]
    pub_ttfr = [r for r in items if r.get("metric") == "ttfr"]
    if pub_ttfr:
        print("\n**Workload D — TTFR (publication: n=1000, flush_every=100)**\n")
        print("| Format | P50 | P95 | P99 |")
        print("| --- | --- | --- | --- |")
        for fmt in fmts:
            hit = pick_row([r for r in pub_ttfr if r["format"] == fmt])
            if fmt == "fb" and not hit:
                print(f"| fb | n/a † | n/a † | n/a † |")
                continue
            if not hit:
                continue
            print(
                f"| {fmt} | {ttfr_cell(hit, 'p50')} | {ttfr_cell(hit, 'p95')} | "
                f"{ttfr_cell(hit, 'p99')} |"
            )
        print(f"\n{D_TTFR_NOTE}\n")
        print(f"\n{D_FB_NOTE}\n")
    elif not publication:
        smoke = [r for r in items if r.get("metric") in ("ttfr", "ttfr_smoke")]
        if smoke:
            render_metric_table(
                smoke,
                tuple(fmts),
                ("ttfr",),
                "Workload D — TTFR (smoke only — not for publication)",
            )

    seal_rows = [r for r in items if r.get("metric") == "seal"]
    if seal_rows:
        render_metric_table(
            seal_rows,
            tuple(f for f in fmts if f == "nxs"),
            ("seal",),
            "Workload D — seal latency (NXS, full dataset)",
        )

    tput_rows = [r for r in items if r.get("metric") == "throughput"]
    if tput_rows:
        render_metric_table(
            tput_rows,
            tuple(f for f in fmts if f != "fb"),
            ("throughput",),
            "Workload D — sustained throughput (batched flush)",
        )
        print(f"\n{D_THROUGHPUT_NOTE}\n")

    if not publication:
        for label, met in (
            ("smoke TTFR", "ttfr_smoke"),
            ("smoke throughput", "throughput_smoke"),
            ("smoke seal", "seal_smoke"),
        ):
            sub = [r for r in items if r.get("metric") == met]
            if sub:
                render_metric_table(
                    sub,
                    tuple(sorted({r["format"] for r in sub})),
                    (met,),
                    f"Workload D — {label} (internal)",
                )


def render_workload_e(items: list[dict]) -> None:
    layouts = [l for l in ("row", "columnar", "pax") if any(r.get("layout") == l for r in items)]
    if not layouts:
        return
    print("\n**Workload E — PAX mixed access + column scan (row / columnar / PAX)**\n")
    print("| Layout | access P50 | col_scan P50 | mixed_total P50 | file size |")
    print("| --- | --- | --- | --- | --- |")
    for layout in layouts:
        sub = [r for r in items if r.get("layout") == layout]
        def _p50(metric: str) -> str:
            hit = next((r for r in sub if r.get("metric") == metric), None)
            if not hit:
                return "—"
            v = hit.get("p50_us")
            if v is None:
                return "—"
            if v <= 0:
                return SUB_TIMER
            return f"{v} µs"
        size_hit = next((r for r in sub if r.get("file_bytes")), None)
        size_str = f"{size_hit['file_bytes'] / 1e6:.2f} MB" if size_hit else "—"
        print(f"| {layout} | {_p50('access')} | {_p50('col_scan')} | {_p50('mixed_total')} | {size_str} |")
    print(
        "\n_PAX col_scan is faster than row col_scan (column locality); "
        "PAX random access within ~2× of row (OLAP §4.5 criterion). "
        "Columnar access and scan at `< 1 µs` reflect warm page cache on this file size._\n"
    )


def render_workload_a(items: list[dict]) -> None:
    pops = sorted({r.get("population", -1) for r in items if r.get("population", -1) >= 0})
    fmts = sorted({r["format"] for r in items})
    for met in ("size", "selective"):
        sub = [r for r in items if r["metric"] == met]
        if not sub:
            continue
        label = (
            "File size"
            if met == "size"
            else "Selective read P50 (NXS: C driver; `< 1 µs` = below timer resolution)"
        )
        print(f"\n**{label}**\n")
        hdr = "| Pop | " + " | ".join(fmts) + " |"
        print(hdr)
        print("| --- | " + " | ".join(["---"] * len(fmts)) + " |")
        for pop in pops:
            cells = [f"{int(pop * 100)}%"]
            for fmt in fmts:
                hits = [
                    r
                    for r in sub
                    if r["format"] == fmt and abs(r.get("population", -2) - pop) < 0.001
                ]
                cells.append(cell_value(pick_row(hits), met))
            print("| " + " | ".join(cells) + " |")
        if met == "selective":
            print(f"\n{PROTO_SELECTIVE_FOOTNOTE}\n")


def render_verdicts(outcomes_path: Path) -> None:
    data = json.loads(outcomes_path.read_text())
    verdicts = [
        v
        for v in data.get("verdicts", [])
        if v.get("metric") in PUBLICATION_METRICS
    ]
    if not verdicts:
        return

    counts = Counter(v["verdict"] for v in verdicts)
    wins = counts.get("win", 0)
    ties = counts.get("tie", 0)
    losses = counts.get("loss", 0)

    print("\n### Appendix: aggregate verdicts (not a product scorecard)\n")
    print(f"**{wins}** wins · **{ties}** ties · **{losses}** losses")
    print(
        f"_(from `{outcomes_path.parent.name}/outcomes.json`; publication metrics only — "
        "use workload tables above for positioning)_\n"
    )

    by_wl: dict[str, list[dict]] = {}
    for v in verdicts:
        by_wl.setdefault(v["workload"], []).append(v)

    print("| Workload | Wins | Ties | Losses | Primary winner (losses) |")
    print("| --- | ---: | ---: | ---: | --- |")
    for wl in ("A", "B", "C", "D"):
        wl_items = by_wl.get(wl)
        if not wl_items:
            continue
        wc = Counter(x["verdict"] for x in wl_items)
        loss_rows = [x for x in wl_items if x["verdict"] == "loss"]
        winners: Counter[str] = Counter()
        for row in loss_rows:
            winners[row["best_format"]] += 1
        primary = (
            ", ".join(f"{fmt} ({n})" for fmt, n in winners.most_common())
            if winners
            else "—"
        )
        print(
            f"| {wl} | {wc.get('win', 0)} | {wc.get('tie', 0)} | "
            f"{wc.get('loss', 0)} | {primary} |"
        )


def render_publication_header(result_dir: str) -> None:
    print(PUBLICATION_SUMMARY)
    print("## Workload comparison suite (macOS dev — frozen)\n")
    print(
        f"**Run:** `bench/results/{result_dir}/` · **Records:** 10,000 · "
        "**Platform:** Apple Silicon (arm64), macOS · "
        "**Status:** macOS dev dataset frozen; **Linux bare-metal + inotify pending**.\n"
    )
    print("### Methodology\n")
    print(
        "Per-workload definitions: `bench/methodology/workload_{A,B,C,D}.md`. "
        "Version pins: `bench/BENCHMARK_VERSIONS.md`. "
        f"Frozen copy: `bench/results/{result_dir}/methodology.md`.\n"
    )


def render_positioning() -> None:
    print("### Honest positioning (macOS dev, 10k records)\n")
    print("**Supported by this dataset:**\n")
    print(
        "- NXS warm random **access** is fastest among zero-copy formats at this record size\n"
        "- NXS selective read (C driver) is sub-microsecond; competitive with Cap'n Proto\n"
        "- NXS file size is competitive with FlatBuffers at 50%+ population; FlatBuffers leads at 10–25%\n"
        "- NXS streaming **TTFR** leads at P50/P95 on this frozen batched run (n=1000, flush_every=100)\n"
        "- P99 TTFR vs Cap'n Proto **conflicts across flush policies** on macOS — do not claim a P99 win until Linux Q1\n"
        "- NXS sustained streaming throughput is in the same band as Protobuf and Cap'n Proto (~25k rec/s)\n"
        "- **Arrow** wins columnar scan by orders of magnitude — use the Arrow bridge for analytics\n"
        "- NXS is the only format here with native file-level streaming **and** post-seal O(1) random access\n"
    )
    print("\n**Not supported:**\n")
    print(
        "- NXS file size wins at low population (FlatBuffers leads at 10–25%)\n"
        "- NXS cold open vs Cap'n Proto / FlatBuffers at small files\n"
        "- NXS scan vs Arrow without noting row-oriented vs columnar design\n"
        "- Any NXS vs Protobuf claim on access/scan/selective without the post-parse footnote\n"
    )
    print("\n**Linux run (three questions):**")
    print(
        " **Q1** — Under inotify, does Cap'n Proto P99 TTFR beat NXS, or does NXS hold the lead? "
        "(Two macOS n=1000 runs disagreed on P99 ordering.)\n"
        " **Q2** — Does NXS Workload A selective produce a real ns number (expect ~30–80 ns)?\n"
        " **Q3** — Does NXS Workload B C scan hold at ~25 µs on Linux?\n"
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("summary", type=Path)
    ap.add_argument("-o", "--output", type=Path, help="write markdown to file")
    ap.add_argument(
        "--publication",
        action="store_true",
        help="publication tables only (no smoke D metrics, appendix verdicts optional)",
    )
    ap.add_argument("--verdicts", action="store_true", help="include appendix verdict table")
    args = ap.parse_args()

    if args.output:
        sys.stdout = args.output.open("w", encoding="utf-8")

    data = json.loads(args.summary.read_text())
    rows = data.get("measurements", [])
    result_dir = args.summary.parent.name

    if args.publication:
        render_publication_header(result_dir)

    by_wl: dict[str, list] = {}
    for r in rows:
        by_wl.setdefault(r["workload"], []).append(r)

    for wl, title in (
        ("A", "Workload A: Sparse records"),
        ("B", "Workload B: Cold-open random access"),
        ("C", "Workload C: Dense analytical reducer"),
        ("D", "Workload D: Streaming ingest"),
        ("E", "Workload E: PAX mixed access + column scan"),
    ):
        if wl not in by_wl:
            continue
        print(f"\n### {title}\n")
        items = by_wl[wl]
        if wl == "D":
            render_workload_d(items, publication=args.publication)
        elif wl == "C":
            render_workload_c(items)
        elif wl == "B":
            render_workload_b(items)
        elif wl == "A":
            render_workload_a(items)
        elif wl == "E":
            render_workload_e(items)

    if args.publication:
        render_positioning()
        print("\n### Reproducing this run\n")
        print("```bash")
        print("cd nyxis && bash bench/scripts/setup_venv.sh")
        print("make -C bench matrix BENCH_RECORDS=10000")
        print(
            f"make -C bench freeze-benchmark RESULT_DIR=bench/results/{result_dir}"
        )
        print("```\n")

    outcomes = args.summary.parent / "outcomes.json"
    if args.verdicts and outcomes.is_file():
        render_verdicts(outcomes)

    if args.output:
        sys.stdout.close()
        print(f"wrote {args.output}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
