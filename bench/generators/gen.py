#!/usr/bin/env python3
"""Canonical JSON generator for benchmark workloads A, B, and C.

All binary formats are transcoded from these JSON files so every competitor
sees identical logical data.

Usage:
  python3 gen.py --out ../data/json [--records 1000000] [--workload A|B|C|all]
"""

from __future__ import annotations

import argparse
import json
import random
import sys
import zlib
from pathlib import Path
from typing import Any

SEED = 0x4E595849  # "NYXI"

# Workload A field layout
I64_FIELDS = [f"i{i:02d}" for i in range(1, 21)]
STR_FIELDS = [f"s{i:02d}" for i in range(21, 36)]
F64_FIELDS = [f"f{i:02d}" for i in range(36, 46)]
BOOL_FIELDS = [f"b{i:02d}" for i in range(46, 51)]
ALL_SCALAR = I64_FIELDS + STR_FIELDS + F64_FIELDS + BOOL_FIELDS

# Five fields read in selective-read benchmark (Workload A secondary metric)
SELECTIVE_READ_FIELDS = ["i01", "s21", "f36", "b46", "i10"]

POPULATION_RATES = (0.10, 0.25, 0.50, 0.90)
B_RECORD_COUNTS = (1_000_000, 10_000_000, 100_000_000)
C_RECORD_COUNTS = (1_000_000, 10_000_000)


def _rng(seed: int) -> random.Random:
    return random.Random(seed)


def _maybe(rng: random.Random, rate: float) -> bool:
    return rng.random() < rate


def _field_salt(name: str) -> int:
    """Stable per-field offset (Python hash() is salted per process)."""
    return zlib.crc32(name.encode()) % 1000


def write_json_array(path: Path, records) -> None:
    """Write a JSON array without materializing one giant string."""
    with path.open("w", encoding="utf-8") as f:
        f.write("[")
        for i, rec in enumerate(records):
            if i:
                f.write(",")
            json.dump(rec, f, separators=(",", ":"))
        f.write("]")


def _nested_block(rng: random.Random, i: int) -> dict[str, Any] | None:
    if not _maybe(rng, 0.15):
        return None
    return {
        "child": {
            "grandchild": {
                "gc_i64": i * 7,
                "gc_str": f"gc_{i:08d}",
            },
            "child_f64": float(i) * 0.25,
        },
        "meta_flag": i % 2 == 0,
    }


def gen_sparse_record(rng: random.Random, i: int, population: float) -> dict[str, Any]:
    rec: dict[str, Any] = {}
    for name in I64_FIELDS:
        if _maybe(rng, population):
            rec[name] = i + _field_salt(name)
    for name in STR_FIELDS:
        if _maybe(rng, population):
            rec[name] = f"{name}_{i:08d}"
    for name in F64_FIELDS:
        if _maybe(rng, population):
            rec[name] = (i * 1.37 + len(name)) % 10000.0
    for name in BOOL_FIELDS:
        if _maybe(rng, population):
            rec[name] = (i + len(name)) % 3 != 0
    meta = _nested_block(rng, i)
    if meta is not None:
        rec["meta"] = meta
    # Selective-read benchmark always touches these five fields (see workload_A.md).
    for name in SELECTIVE_READ_FIELDS:
        if name in rec:
            continue
        if name.startswith("i"):
            rec[name] = i + _field_salt(name)
        elif name.startswith("s"):
            rec[name] = f"{name}_{i:08d}"
        elif name.startswith("f"):
            rec[name] = (i * 1.37 + len(name)) % 10000.0
        else:
            rec[name] = (i + len(name)) % 3 != 0
    return rec


def gen_workload_a(out: Path, records: int, population: float | None) -> list[Path]:
    written: list[Path] = []
    rates = POPULATION_RATES if population is None else (population,)
    for rate in rates:
        rng = _rng(SEED + int(rate * 100))
        data = [gen_sparse_record(rng, i, rate) for i in range(records)]
        pct = int(rate * 100)
        path = out / f"workload_A_pop{pct:02d}_{records}.json"
        write_json_array(path, data)
        print(f"  {path.name}  ({len(data)} records, pop={rate:.0%})")
        written.append(path)
    meta = out / "workload_A_selective_fields.json"
    meta.write_text(json.dumps(SELECTIVE_READ_FIELDS), encoding="utf-8")
    written.append(meta)
    return written


def gen_flat8_record(i: int) -> dict[str, Any]:
    return {
        "id": i,
        "username": f"user_{i:07d}",
        "email": f"user{i}@example.com",
        "age": 20 + (i % 50),
        "balance": round(100.0 + i * 1.37, 2),
        "active": i % 3 != 0,
        "score": round((i % 100) / 10.0, 1),
        "created_at": 1_777_593_600_000_000_000,
    }


def gen_workload_b(out: Path, records: int) -> list[Path]:
    written: list[Path] = []
    counts = sorted(set([c for c in B_RECORD_COUNTS if c <= records] + [records]))
    for n_eff in counts:
        data = [gen_flat8_record(i) for i in range(n_eff)]
        path = out / f"workload_B_{n_eff}.json"
        write_json_array(path, data)
        print(f"  {path.name}  ({len(data)} records)")
        written.append(path)
    return written


def gen_dense8_record(i: int) -> dict[str, Any]:
    return {
        "id": i,
        "bucket": i % 256,
        "quantity": (i % 97) + 1,
        "amount": round(i * 1.11, 4),
        "rate": round(0.01 + (i % 50) * 0.001, 6),
        "score": round((i % 1000) / 10.0, 2),
        "category": i % 64,
        "active": i % 5 != 0,
    }


def gen_workload_c(out: Path, records: int) -> list[Path]:
    written: list[Path] = []
    counts = sorted(set([c for c in C_RECORD_COUNTS if c <= records] + [records]))
    for n_eff in counts:
        data = [gen_dense8_record(i) for i in range(n_eff)]
        path = out / f"workload_C_{n_eff}.json"
        write_json_array(path, data)
        print(f"  {path.name}  ({len(data)} records)")
        written.append(path)
    return written


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, default=Path(__file__).resolve().parent.parent / "data" / "json")
    ap.add_argument("--records", type=int, default=1_000_000, help="Max records per generated file")
    ap.add_argument("--workload", choices=("A", "B", "C", "all"), default="all")
    ap.add_argument("--population", type=float, default=None, help="Workload A only: single rate 0–1")
    args = ap.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    print(f"Generating JSON under {args.out} (max records={args.records:,})")

    if args.workload in ("A", "all"):
        print("Workload A (sparse):")
        gen_workload_a(args.out, args.records, args.population)
    if args.workload in ("B", "all"):
        print("Workload B (flat8):")
        gen_workload_b(args.out, args.records)
    if args.workload in ("C", "all"):
        print("Workload C (dense8):")
        gen_workload_c(args.out, args.records)

    return 0


if __name__ == "__main__":
    sys.exit(main())
