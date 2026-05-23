#!/usr/bin/env python3
"""Transcode canonical JSON → binary formats for the benchmark suite.

Requires: bench/generators/codegen.sh (protoc) for protobuf; optional flatc, pyarrow, pycapnp.
Python deps: uv sync in bench/generators (see pyproject.toml).

Usage:
  python3 transcode.py --workload B --format proto --json ../data/json/workload_B_1000.json \\
      --out ../data/bin/workload_B_proto_1000.pb
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

BENCH = Path(__file__).resolve().parent.parent
SCHEMAS = BENCH / "schemas"
GEN = Path(__file__).resolve().parent / "generated"
sys.path.insert(0, str(GEN))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from sparse_fields import (  # noqa: E402
    BOOL_FIELDS,
    F64_FIELDS,
    I64_FIELDS,
    STR_FIELDS,
    encode_sparse_capnp,
    encode_sparse_flatbuffers,
)
# flatc --python emits Flat8/, Dense8/ under GEN


def load_json(path: Path) -> list[dict]:
    return json.loads(path.read_text(encoding="utf-8"))


def transcode_proto(workload: str, records: list[dict], out: Path) -> None:
    mod_name = {"B": "flat8_pb2", "C": "dense8_pb2", "A": "sparse_pb2"}[workload]
    mod = __import__(mod_name)

    if workload == "B":
        msg = mod.Flat8File()
        for r in records:
            rec = msg.records.add()
            rec.id = int(r["id"])
            rec.username = r["username"]
            rec.email = r["email"]
            rec.age = int(r["age"])
            rec.balance = float(r["balance"])
            rec.active = bool(r["active"])
            rec.score = float(r["score"])
            rec.created_at = int(r["created_at"])
    elif workload == "C":
        msg = mod.Dense8File()
        for r in records:
            rec = msg.records.add()
            rec.id = int(r["id"])
            rec.bucket = int(r["bucket"])
            rec.quantity = int(r["quantity"])
            rec.amount = float(r["amount"])
            rec.rate = float(r["rate"])
            rec.score = float(r["score"])
            rec.category = int(r["category"])
            rec.active = bool(r["active"])
    else:
        msg = mod.SparseFile()
        for r in records:
            rec = msg.records.add()
            for k in I64_FIELDS:
                if k in r:
                    setattr(rec, k, int(r[k]))
            for k in STR_FIELDS:
                if k in r:
                    setattr(rec, k, str(r[k]))
            for k in F64_FIELDS:
                if k in r:
                    setattr(rec, k, float(r[k]))
            for k in BOOL_FIELDS:
                if k in r:
                    setattr(rec, k, bool(r[k]))
            if "meta" in r and r["meta"]:
                _fill_sparse_meta(rec.meta, r["meta"])

    out.write_bytes(msg.SerializeToString())


def _fill_sparse_meta(meta, d: dict) -> None:
    if "child" in d and d["child"]:
        ch = d["child"]
        if "grandchild" in ch and ch["grandchild"]:
            gc = ch["grandchild"]
            if "gc_i64" in gc:
                meta.child.grandchild.gc_i64 = int(gc["gc_i64"])
            if "gc_str" in gc:
                meta.child.grandchild.gc_str = str(gc["gc_str"])
        if "child_f64" in ch:
            meta.child.child_f64 = float(ch["child_f64"])
    if "meta_flag" in d:
        meta.meta_flag = bool(d["meta_flag"])


def transcode_flatbuffers(workload: str, records: list[dict], out: Path) -> None:
    if workload == "B":
        import flatbuffers
        from nyxis.bench import Flat8File as FF
        from nyxis.bench import Flat8Record as FR

        builder = flatbuffers.Builder(0)
        record_offsets = []
        for r in records:
            uname = builder.CreateString(r["username"])
            email = builder.CreateString(r["email"])
            FR.Start(builder)
            FR.AddId(builder, int(r["id"]))
            FR.AddUsername(builder, uname)
            FR.AddEmail(builder, email)
            FR.AddAge(builder, int(r["age"]))
            FR.AddBalance(builder, float(r["balance"]))
            FR.AddActive(builder, bool(r["active"]))
            FR.AddScore(builder, float(r["score"]))
            FR.AddCreatedAt(builder, int(r["created_at"]))
            record_offsets.append(FR.End(builder))
        FF.StartRecordsVector(builder, len(record_offsets))
        for off in reversed(record_offsets):
            builder.PrependUOffsetTRelative(off)
        vec = builder.EndVector()
        FF.Start(builder)
        FF.AddRecords(builder, vec)
        builder.Finish(FF.End(builder))
        out.write_bytes(bytes(builder.Output()))
    elif workload == "C":
        import flatbuffers
        from nyxis.bench import Dense8File as DF
        from nyxis.bench import Dense8Record as DR

        builder = flatbuffers.Builder(0)
        record_offsets = []
        for r in records:
            DR.Start(builder)
            DR.AddId(builder, int(r["id"]))
            DR.AddBucket(builder, int(r["bucket"]))
            DR.AddQuantity(builder, int(r["quantity"]))
            DR.AddAmount(builder, float(r["amount"]))
            DR.AddRate(builder, float(r["rate"]))
            DR.AddScore(builder, float(r["score"]))
            DR.AddCategory(builder, int(r["category"]))
            DR.AddActive(builder, bool(r["active"]))
            record_offsets.append(DR.End(builder))
        DF.StartRecordsVector(builder, len(record_offsets))
        for off in reversed(record_offsets):
            builder.PrependUOffsetTRelative(off)
        vec = builder.EndVector()
        DF.Start(builder)
        DF.AddRecords(builder, vec)
        builder.Finish(DF.End(builder))
        out.write_bytes(bytes(builder.Output()))
    elif workload == "A":
        out.write_bytes(encode_sparse_flatbuffers(records))
    else:
        raise NotImplementedError(f"FlatBuffers workload {workload}")


def transcode_capnp(workload: str, records: list[dict], out: Path) -> None:
    import capnp

    capnp.remove_import_hook()
    cap_file = {
        "B": SCHEMAS / "flat8.capnp",
        "C": SCHEMAS / "dense8.capnp",
        "A": SCHEMAS / "sparse.capnp",
    }[workload]
    schema = capnp.load(str(cap_file))

    if workload == "B":
        msgs = []
        for r in records:
            msgs.append(
                schema.Flat8Record(
                    id=int(r["id"]),
                    username=r["username"],
                    email=r["email"],
                    age=int(r["age"]),
                    balance=float(r["balance"]),
                    active=bool(r["active"]),
                    score=float(r["score"]),
                    createdAt=int(r["created_at"]),
                )
            )
        out.write_bytes(schema.Flat8File(records=msgs).to_bytes())
    elif workload == "C":
        msgs = []
        for r in records:
            msgs.append(
                schema.Dense8Record(
                    id=int(r["id"]),
                    bucket=int(r["bucket"]),
                    quantity=int(r["quantity"]),
                    amount=float(r["amount"]),
                    rate=float(r["rate"]),
                    score=float(r["score"]),
                    category=int(r["category"]),
                    active=bool(r["active"]),
                )
            )
        out.write_bytes(schema.Dense8File(records=msgs).to_bytes())
    elif workload == "A":
        out.write_bytes(encode_sparse_capnp(records, schema))
    else:
        raise NotImplementedError(f"Cap'n Proto workload {workload}")


def transcode_arrow(workload: str, records: list[dict], out: Path) -> None:
    import pyarrow as pa
    import pyarrow.ipc as ipc

    if workload != "C":
        raise ValueError("Arrow IPC is only used for Workload C in this suite")
    cols = {
        "id": pa.array([int(r["id"]) for r in records], type=pa.int64()),
        "bucket": pa.array([int(r["bucket"]) for r in records], type=pa.int64()),
        "quantity": pa.array([int(r["quantity"]) for r in records], type=pa.int64()),
        "amount": pa.array([float(r["amount"]) for r in records], type=pa.float64()),
        "rate": pa.array([float(r["rate"]) for r in records], type=pa.float64()),
        "score": pa.array([float(r["score"]) for r in records], type=pa.float64()),
        "category": pa.array([int(r["category"]) for r in records], type=pa.int64()),
        "active": pa.array([bool(r["active"]) for r in records], type=pa.bool_()),
    }
    table = pa.table(cols)
    with out.open("wb") as f:
        with ipc.new_file(f, table.schema) as writer:
            writer.write_table(table)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--workload", required=True, choices=("A", "B", "C"))
    ap.add_argument("--format", required=True, choices=("proto", "fb", "capnp", "arrow", "nxs"))
    ap.add_argument("--json", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()

    if args.format == "nxs":
        print("use bench-transcode (Rust) for nxs", file=sys.stderr)
        return 1

    records = load_json(args.json)
    args.out.parent.mkdir(parents=True, exist_ok=True)

    try:
        if args.format == "proto":
            transcode_proto(args.workload, records, args.out)
        elif args.format == "fb":
            transcode_flatbuffers(args.workload, records, args.out)
        elif args.format == "capnp":
            transcode_capnp(args.workload, records, args.out)
        elif args.format == "arrow":
            transcode_arrow(args.workload, records, args.out)
    except ImportError as e:
        print(f"transcode {args.format}: missing dependency ({e})", file=sys.stderr)
        return 1
    except NotImplementedError as e:
        print(f"transcode {args.format}: {e}", file=sys.stderr)
        return 1

    print(f"wrote {args.out} ({args.out.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
