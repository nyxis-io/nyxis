#!/usr/bin/env python3
"""Splice publication tables from summary.json into nyxis/BENCHMARK.md."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

MARKER_START = "<!-- BENCH-SUITE-FROZEN:START -->"
MARKER_END = "<!-- BENCH-SUITE-FROZEN:END -->"
SUMMARY_MARKER = "<!-- BENCH-PUBLICATION-SUMMARY -->"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--results", type=Path, required=True, help="bench/results/<tag>")
    ap.add_argument(
        "--benchmark",
        type=Path,
        required=True,
        help="path to BENCHMARK.md (nyxis/BENCHMARK.md)",
    )
    ap.add_argument("--py", default=sys.executable)
    args = ap.parse_args()

    bench = args.results.parent.parent
    render = bench / "scripts" / "render_tables.py"
    summary = args.results / "summary.json"
    if not summary.is_file():
        print(f"missing {summary}", file=sys.stderr)
        return 1

    tables_path = args.results / "publication_tables.md"
    subprocess.run(
        [
            args.py,
            str(render),
            str(summary),
            "--publication",
            "-o",
            str(tables_path),
        ],
        check=True,
    )

    block = tables_path.read_text(encoding="utf-8").strip()
    wrapped = f"{MARKER_START}\n\n{block}\n\n{MARKER_END}"

    # Publication summary blockquote at top of BENCHMARK.md (after hardware line).
    summary_lines = [
        ln
        for ln in block.splitlines()
        if ln.startswith("> These benchmarks cover")
    ]
    summary_block = (
        (summary_lines[0] + "\n") if summary_lines else ""
    )

    text = args.benchmark.read_text(encoding="utf-8")
    if summary_block:
        if SUMMARY_MARKER in text:
            parts = text.split(SUMMARY_MARKER)
            text = (
                parts[0]
                + SUMMARY_MARKER
                + "\n\n"
                + summary_block
                + "\n"
                + SUMMARY_MARKER
                + parts[-1]
            )
        else:
            anchor = (
                "Hardware: Apple M-series (arm64), macOS. "
                "All runs against locally-served fixtures.\n"
            )
            if anchor in text:
                text = text.replace(
                    anchor,
                    anchor
                    + "\n"
                    + SUMMARY_MARKER
                    + "\n\n"
                    + summary_block
                    + SUMMARY_MARKER
                    + "\n",
                    1,
                )
    if MARKER_START in text and MARKER_END in text:
        pre, rest = text.split(MARKER_START, 1)
        _, post = rest.split(MARKER_END, 1)
        text = pre + wrapped + post
    else:
        anchor = "## Workload comparison suite"
        if anchor not in text:
            print(f"anchor {anchor!r} not found in {args.benchmark}", file=sys.stderr)
            return 1
        pre, rest = text.split(anchor, 1)
        end_anchor = "\n## Running the Benchmarks"
        if end_anchor in rest:
            _, tail = rest.split(end_anchor, 1)
            text = pre + wrapped + "\n\n---\n\n" + end_anchor + tail
        else:
            text = pre + wrapped + "\n\n" + rest

    args.benchmark.write_text(text, encoding="utf-8")
    print(f"frozen → {args.benchmark}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
