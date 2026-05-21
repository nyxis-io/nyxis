#!/usr/bin/env python3
"""Multi-format benchmark harness — same CLI contract as harness/c.

Measures: size, open, access, scan (sum), distinct (Workload C count_distinct).
Uses CLOCK_MONOTONIC via time.perf_counter_ns() (documented equivalent).
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
import time
from pathlib import Path

BENCH = Path(__file__).resolve().parents[2]
GEN = BENCH / "generators" / "generated"
sys.path.insert(0, str(GEN))
sys.path.insert(0, str(BENCH.parent.parent / "nyxis-drivers" / "py"))  # monorepo: nyxis-drivers/py
sys.path.insert(0, str(BENCH / "generators"))

from sparse_fields import SELECTIVE_READ  # noqa: E402

WARMUP_DEFAULT = 100
SAMPLES_DEFAULT = 1000
O_N_METRICS = frozenset({"scan", "distinct"})

EXT = {
    "nxs": "nxb",
    "proto": "pb",
    "fb": "bfbs",
    "capnp": "capnp",
    "arrow": "arrow",
}

# flatc output uses .bfbs in Makefile; allow .bin alias
EXT_ALT = {"fb": ("bfbs", "bin")}


def resolve_timing(records: int, metric: str) -> tuple[int, int]:
    """Scale warmup/samples down for O(n) metrics and large fixtures."""
    if os.environ.get("BENCH_WARMUP") and os.environ.get("BENCH_SAMPLES"):
        return int(os.environ["BENCH_WARMUP"]), int(os.environ["BENCH_SAMPLES"])
    fast = os.environ.get("BENCH_FAST", "")
    if metric in O_N_METRICS:
        if records >= 1_000_000 or fast:
            return 5, 20
        if records >= 100_000:
            return 10, 50
    if records >= 1_000_000 or fast:
        return 25, 100
    if records >= 100_000:
        return 50, 200
    return WARMUP_DEFAULT, SAMPLES_DEFAULT


def measure(fn, *, records: int, metric: str) -> tuple[int, int, int, int]:
    warmup, samples_n = resolve_timing(records, metric)
    for _ in range(warmup):
        fn()
    samples: list[int] = []
    for _ in range(samples_n):
        t0 = time.perf_counter_ns()
        fn()
        samples.append(time.perf_counter_ns() - t0)
    samples.sort()
    q1 = samples[len(samples) // 4]
    q3 = samples[(3 * len(samples)) // 4]
    trimmed = samples[len(samples) // 4 : (3 * len(samples)) // 4 + 1]
    p50 = int(statistics.median(trimmed))
    p99 = int(trimmed[int((len(trimmed) - 1) * 0.99)])
    return p50, p99, q3 - q1, samples_n


def default_path(data_dir: Path, workload: str, fmt: str, records: int, pop: float) -> Path:
    exts = EXT_ALT.get(fmt, (EXT[fmt],))
    stem = (
        f"workload_{workload}_{fmt}_{records}_pop{int(pop * 100 + 0.5):02d}"
        if workload == "A" and pop >= 0
        else f"workload_{workload}_{fmt}_{records}"
    )
    for ext in exts:
        p = data_dir / f"{stem}.{ext}"
        if p.exists():
            return p
    return data_dir / f"{stem}.{exts[0]}"


class Emitter:
    def __init__(self, base: dict):
        self.base = base

    def emit(self, **kwargs) -> None:
        print(json.dumps({**self.base, **kwargs}, separators=(",", ":")))


def field_for(workload: str) -> str:
    return "f36" if workload == "A" else "score"


def capnp_reader_limits(records: int) -> dict[str, int]:
    """pycapnp defaults are too low for multi-record benchmark blobs."""
    words = max((records + 1) * 4096, 64 * 1024 * 1024)
    return {
        "traversal_limit_in_words": min(words, (1 << 31) - 1),
        "nesting_limit": 128,
    }


def run_nxs(
    emit: Emitter,
    path: Path,
    workload: str,
    metric: str,
    field: str,
    records: int,
    *,
    cold: bool = False,
) -> None:
    from nxs import NxsReader  # type: ignore

    data = path.read_bytes()
    if metric == "size":
        emit.emit(bytes=path.stat().st_size)
        return

    def open_read():
        r = NxsReader(data)
        r.record(0).get_f64(field)

    reader = NxsReader(data)
    n = reader.record_count
    idx = 0

    def access():
        nonlocal idx
        idx = (idx * 997 + 1) % max(n, 1)
        reader.record(idx).get_f64(field)

    scan_reader = NxsReader(data)
    try:
        import _nxs as _nxs_mod  # type: ignore

        c_scan = _nxs_mod.Reader(data)
    except ImportError:
        c_scan = None

    def scan():
        if c_scan is not None:
            c_scan.sum_f64(field)
            return
        s = 0.0
        for rec in scan_reader.records():
            v = rec.get_f64(field)
            if v is not None:
                s += v
        return s

    if metric == "open":
        p50, p99, iqr, n = measure(open_read, records=records, metric=metric)
    elif metric == "access":
        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            access()
        p50, p99, iqr, n = measure(access, records=records, metric=metric)
    elif metric == "scan":
        p50, p99, iqr, n = measure(scan, records=records, metric=metric)
    elif metric == "selective":
        idx = 0

        def read_selective(rec) -> None:
            for f in SELECTIVE_READ:
                if f.startswith("i"):
                    rec.get_i64(f)
                elif f.startswith("s"):
                    rec.get_str(f)
                elif f.startswith("f"):
                    rec.get_f64(f)
                elif f.startswith("b"):
                    rec.get_bool(f)

        if cold:

            def selective():
                nonlocal idx
                r = NxsReader(data)
                n = r.record_count
                idx = (idx * 997 + 1) % max(n, 1)
                read_selective(r.record(idx))

        else:
            sel_reader = NxsReader(data)
            n_sel = sel_reader.record_count

            def selective():
                nonlocal idx
                idx = (idx * 997 + 1) % max(n_sel, 1)
                read_selective(sel_reader.record(idx))

        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            selective()
        p50, p99, iqr, n = measure(selective, records=records, metric=metric)
    else:
        raise ValueError(metric)
    emit.emit(p50_ns=p50, p99_ns=p99, iqr_ns=iqr, samples=n)


def run_proto(
    emit: Emitter, path: Path, workload: str, metric: str, field: str, records: int
) -> None:
    mod_name = {"B": "flat8_pb2", "C": "dense8_pb2", "A": "sparse_pb2"}[workload]
    mod = __import__(mod_name)
    blob = path.read_bytes()

    if metric == "size":
        emit.emit(bytes=path.stat().st_size)
        return

    file_cls = {"B": "Flat8File", "C": "Dense8File", "A": "SparseFile"}[workload]
    warm = getattr(mod, file_cls)()
    warm.ParseFromString(blob)

    def open_read():
        m = getattr(mod, file_cls)()
        m.ParseFromString(blob)
        rec = m.records[0]
        if workload == "B":
            _ = rec.score
        elif workload == "C":
            _ = rec.score
        else:
            _ = getattr(rec, field, 0.0)

    idx_acc = 0

    def access():
        nonlocal idx_acc
        n = len(warm.records)
        idx_acc = (idx_acc * 997 + 1) % max(n, 1)
        rec = warm.records[idx_acc]
        if workload == "C":
            _ = rec.score
        elif workload == "B":
            _ = rec.score
        else:
            _ = getattr(rec, field, 0.0)

    def scan():
        s = 0.0
        key = "score" if workload in ("B", "C") else field
        for rec in warm.records:
            s += float(getattr(rec, key, 0.0) or 0.0)
        return s

    if metric == "open":
        p50, p99, iqr, n = measure(open_read, records=records, metric=metric)
    elif metric == "access":
        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            access()
        p50, p99, iqr, n = measure(access, records=records, metric=metric)
    elif metric == "scan":
        p50, p99, iqr, n = measure(scan, records=records, metric=metric)
    elif metric == "selective" and workload == "A":
        idx = 0
        n = len(warm.records)

        def selective():
            nonlocal idx
            ri = (idx * 997 + 1) % max(n, 1)
            idx = ri
            rec = warm.records[ri]
            for f in SELECTIVE_READ:
                if rec.HasField(f):
                    getattr(rec, f)

        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            selective()
        p50, p99, iqr, n = measure(selective, records=records, metric=metric)
    else:
        raise ValueError(metric)
    emit.emit(p50_ns=p50, p99_ns=p99, iqr_ns=iqr, samples=n)


def run_flatbuffers(
    emit: Emitter, path: Path, workload: str, metric: str, field: str, records: int
) -> None:
    import flatbuffers

    blob = path.read_bytes()
    if metric == "size":
        emit.emit(bytes=path.stat().st_size)
        return

    if workload == "A":
        from nyxis.bench.SparseFile import SparseFile  # type: ignore

        if metric == "selective":
            idx = 0

            def selective():
                nonlocal idx
                t = SparseFile.GetRootAs(blob, 0)
                n = t.RecordsLength()
                ri = (idx * 997 + 1) % max(n, 1)
                idx = ri
                rec = t.Records(ri)
                for f in SELECTIVE_READ:
                    cap = f[0].upper() + f[1:]
                    getattr(rec, cap)()

            w, _ = resolve_timing(records, metric)
            for _ in range(w):
                selective()
            p50, p99, iqr, n = measure(selective, records=records, metric=metric)
            emit.emit(p50_ns=p50, p99_ns=p99, iqr_ns=iqr, samples=n)
            return
        raise NotImplementedError("FlatBuffers workload A metrics besides selective/size")

    from nyxis.bench.Flat8File import Flat8File  # type: ignore

    if metric == "scan" and records >= 100_000 and not os.environ.get("BENCH_FULL"):
        print(
            f"harness fb: skip scan at {records} records (Python loop); "
            "set BENCH_FULL=1 or use 10k for FB scan",
            file=sys.stderr,
        )
        return

    root_table = Flat8File.GetRootAs(blob, 0)
    idx_acc = 0

    def open_read():
        t = Flat8File.GetRootAs(blob, 0)
        t.Records(0).Score()

    def access():
        nonlocal idx_acc
        n = root_table.RecordsLength()
        idx_acc = (idx_acc * 997 + 1) % max(n, 1)
        root_table.Records(idx_acc).Score()

    def scan():
        s = 0.0
        for i in range(root_table.RecordsLength()):
            s += root_table.Records(i).Score()
        return s

    if metric == "open":
        p50, p99, iqr, n = measure(open_read, records=records, metric=metric)
    elif metric == "access":
        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            access()
        p50, p99, iqr, n = measure(access, records=records, metric=metric)
    elif metric == "scan":
        p50, p99, iqr, n = measure(scan, records=records, metric=metric)
    else:
        raise ValueError(metric)
    emit.emit(p50_ns=p50, p99_ns=p99, iqr_ns=iqr, samples=n)


def run_capnp(
    emit: Emitter, path: Path, workload: str, metric: str, field: str, records: int
) -> None:
    import capnp

    capnp.remove_import_hook()
    cap_file = {
        "B": BENCH / "schemas" / "flat8.capnp",
        "C": BENCH / "schemas" / "dense8.capnp",
        "A": BENCH / "schemas" / "sparse.capnp",
    }[workload]
    schema = capnp.load(str(cap_file))
    blob = path.read_bytes()

    if metric == "size":
        emit.emit(bytes=path.stat().st_size)
        return

    file_cls = {"B": "Flat8File", "C": "Dense8File", "A": "SparseFile"}[workload]
    capnp_cls = getattr(schema, file_cls)
    capnp_kw = capnp_reader_limits(records)
    n_recs = records
    idx_acc = 0

    def open_read():
        with capnp_cls.from_bytes(blob, **capnp_kw) as f:
            _ = f.records[0].score

    def access():
        nonlocal idx_acc
        idx_acc = (idx_acc * 997 + 1) % max(n_recs, 1)
        with capnp_cls.from_bytes(blob, **capnp_kw) as f:
            _ = f.records[idx_acc].score

    def scan():
        with capnp_cls.from_bytes(blob, **capnp_kw) as f:
            return sum(r.score for r in f.records)

    if metric == "open":
        p50, p99, iqr, n = measure(open_read, records=records, metric=metric)
    elif metric == "access":
        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            access()
        p50, p99, iqr, n = measure(access, records=records, metric=metric)
    elif metric == "scan":
        p50, p99, iqr, n = measure(scan, records=records, metric=metric)
    elif metric == "selective" and workload == "A":
        idx = 0

        def selective():
            nonlocal idx
            ri = (idx * 997 + 1) % max(n_recs, 1)
            idx = ri
            with capnp_cls.from_bytes(blob, **capnp_kw) as f:
                rec = f.records[ri]
                for f_name in SELECTIVE_READ:
                    getattr(rec, f_name)

        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            selective()
        p50, p99, iqr, n = measure(selective, records=records, metric=metric)
    else:
        raise ValueError(metric)
    emit.emit(p50_ns=p50, p99_ns=p99, iqr_ns=iqr, samples=n)


def run_arrow(emit: Emitter, path: Path, metric: str, records: int) -> None:
    import pyarrow as pa
    import pyarrow.compute as pc
    import pyarrow.ipc as ipc

    if metric == "size":
        emit.emit(bytes=path.stat().st_size)
        return

    warm_table = None

    def table():
        nonlocal warm_table
        if warm_table is None:
            with path.open("rb") as f:
                warm_table = ipc.open_file(f).read_all()
        return warm_table

    def open_read():
        with path.open("rb") as f:
            t = ipc.open_file(f).read_all()
        t.column("score")[0].as_py()

    def access():
        t = table()
        col = t.column("score")
        n = len(col)
        col[997 % n if n else 0].as_py()

    def scan():
        return pc.sum(table().column("score")).as_py()

    def distinct():
        return pc.count_distinct(table().column("category")).as_py()

    if metric == "open":
        p50, p99, iqr, n = measure(open_read, records=records, metric=metric)
    elif metric == "access":
        w, _ = resolve_timing(records, metric)
        for _ in range(w):
            access()
        p50, p99, iqr, n = measure(access, records=records, metric=metric)
    elif metric == "scan":
        p50, p99, iqr, n = measure(scan, records=records, metric=metric)
    elif metric == "distinct":
        p50, p99, iqr, n = measure(distinct, records=records, metric=metric)
    else:
        raise ValueError(metric)
    emit.emit(p50_ns=p50, p99_ns=p99, iqr_ns=iqr, samples=n)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--workload", required=True)
    ap.add_argument("--format", required=True, choices=list(EXT))
    ap.add_argument("--records", type=int, required=True)
    ap.add_argument("--metric", required=True)
    ap.add_argument("--population", type=float, default=-1.0)
    ap.add_argument("--data-dir", type=Path, default=BENCH / "data" / "bin")
    ap.add_argument("--path", type=Path, default=None)
    ap.add_argument(
        "--cold",
        action="store_true",
        help="NXS selective: re-open reader each sample (legacy cold path)",
    )
    args = ap.parse_args()

    path = args.path or default_path(
        args.data_dir, args.workload.upper(), args.format, args.records, args.population
    )
    if not path.exists():
        print(f"missing fixture: {path}", file=sys.stderr)
        return 1

    wl = args.workload.upper()
    field = field_for(wl)
    emit = Emitter(
        {
            "workload": wl,
            "format": args.format,
            "records": args.records,
            "metric": args.metric,
            "population": args.population,
        }
    )

    runners = {
        "nxs": lambda: run_nxs(
            emit, path, wl, args.metric, field, args.records, cold=args.cold
        ),
        "proto": lambda: run_proto(emit, path, wl, args.metric, field, args.records),
        "fb": lambda: run_flatbuffers(emit, path, wl, args.metric, field, args.records),
        "capnp": lambda: run_capnp(emit, path, wl, args.metric, field, args.records),
        "arrow": lambda: run_arrow(emit, path, args.metric, args.records),
    }
    try:
        runners[args.format]()
    except ImportError as e:
        print(f"harness {args.format}: {e}", file=sys.stderr)
        return 1
    except NotImplementedError as e:
        print(f"harness {args.format}: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
