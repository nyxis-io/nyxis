"""Shared field lists for Workload A (sparse) encoders."""

from __future__ import annotations

I64_FIELDS = [f"i{i:02d}" for i in range(1, 21)]
STR_FIELDS = [f"s{i:02d}" for i in range(21, 36)]
F64_FIELDS = [f"f{i:02d}" for i in range(36, 46)]
BOOL_FIELDS = [f"b{i:02d}" for i in range(46, 51)]
SELECTIVE_READ = ["i01", "s21", "f36", "b46", "i10"]


def cap_name(key: str) -> str:
    """i01 -> I01 for FlatBuffers AddI01."""
    return key[0].upper() + key[1:]


def _fb_add_scalar(SR, builder, key: str, val) -> None:
    getattr(SR, f"Add{cap_name(key)}")(builder, val)


def encode_sparse_flatbuffers(records: list[dict]) -> bytes:
    import flatbuffers
    from nyxis.bench import SparseFile as SF
    from nyxis.bench import SparseRecord as SR

    builder = flatbuffers.Builder(0)
    offsets = []
    for r in records:
        str_offs = {}
        for k in STR_FIELDS:
            if k in r:
                str_offs[k] = builder.CreateString(str(r[k]))
        SR.Start(builder)
        for k in I64_FIELDS:
            if k in r:
                _fb_add_scalar(SR, builder, k, int(r[k]))
        for k in STR_FIELDS:
            if k in r:
                _fb_add_scalar(SR, builder, k, str_offs[k])
        for k in F64_FIELDS:
            if k in r:
                _fb_add_scalar(SR, builder, k, float(r[k]))
        for k in BOOL_FIELDS:
            if k in r:
                _fb_add_scalar(SR, builder, k, bool(r[k]))
        offsets.append(SR.End(builder))
    SF.StartRecordsVector(builder, len(offsets))
    for off in reversed(offsets):
        builder.PrependUOffsetTRelative(off)
    vec = builder.EndVector()
    SF.Start(builder)
    SF.AddRecords(builder, vec)
    builder.Finish(SF.End(builder))
    return bytes(builder.Output())


def _capnp_meta(schema, d: dict | None):
    if not d:
        return None
    child_obj = None
    ch = d.get("child")
    if ch:
        gc = None
        g = ch.get("grandchild")
        if g:
            gc = schema.SparseGrandchild(
                gcI64=int(g.get("gc_i64", 0)),
                gcStr=str(g.get("gc_str", "")),
            )
        child_obj = schema.SparseChild(
            grandchild=gc,
            childF64=float(ch.get("child_f64", 0.0)),
        )
    return schema.SparseMeta(child=child_obj, metaFlag=bool(d.get("meta_flag", False)))


def _capnp_sparse_record(schema, r: dict):
    kw: dict = {}
    for k in I64_FIELDS:
        if k in r:
            kw[k] = int(r[k])
    for k in STR_FIELDS:
        if k in r:
            kw[k] = str(r[k])
    for k in F64_FIELDS:
        if k in r:
            kw[k] = float(r[k])
    for k in BOOL_FIELDS:
        if k in r:
            kw[k] = bool(r[k])
    if "meta" in r and r["meta"]:
        kw["meta"] = _capnp_meta(schema, r["meta"])
    return schema.SparseRecord(**kw)


def encode_sparse_capnp(records: list[dict], schema) -> bytes:
    msgs = [_capnp_sparse_record(schema, r) for r in records]
    return schema.SparseFile(records=msgs).to_bytes()
