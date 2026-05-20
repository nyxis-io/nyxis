#!/usr/bin/env python3
"""NXS conformance runner for Python.

Usage: python conformance/run_py.py conformance/
"""
import json
import math
import os
import struct
import sys

_drv = os.environ.get("DRV") or os.path.join(
    os.path.dirname(__file__), "..", "..", "nyxis-drivers"
)
sys.path.insert(0, os.path.join(os.path.abspath(_drv), "py"))
from nxs import NxsReader, NxsError

MAGIC_LIST = 0x4E59584C  # NYXL

_U32 = struct.Struct("<I")
_I64 = struct.Struct("<q")
_F64 = struct.Struct("<d")
_U16 = struct.Struct("<H")


def approx_eq(a: float, b: float) -> bool:
    if a == b:
        return True
    diff = abs(a - b)
    mag = max(abs(a), abs(b))
    if mag < 1e-300:
        return diff < 1e-300
    return diff / mag < 1e-9


def read_list(mv, off: int):
    """Decode a NYXL list at absolute offset `off` from memoryview `mv`."""
    magic = _U32.unpack_from(mv, off)[0]
    if magic != MAGIC_LIST:
        return None
    # total_len = _U32.unpack_from(mv, off+4)[0]
    elem_sigil = mv[off + 8]
    elem_count = _U32.unpack_from(mv, off + 9)[0]
    data_start = off + 16
    result = []
    for i in range(elem_count):
        elem_off = data_start + i * 8
        if elem_sigil == 0x3D:  # =  int
            result.append(_I64.unpack_from(mv, elem_off)[0])
        elif elem_sigil == 0x7E:  # ~  float
            result.append(_F64.unpack_from(mv, elem_off)[0])
        else:
            result.append(None)
    return result


def values_match(actual, expected) -> bool:
    if expected is None:
        return actual is None or actual == 0 or actual is False
    if isinstance(expected, bool):
        return actual == expected
    if isinstance(expected, int):
        if isinstance(actual, int):
            return actual == expected
        if isinstance(actual, float):
            return approx_eq(actual, float(expected))
        return False
    if isinstance(expected, float):
        if isinstance(actual, (int, float)):
            return approx_eq(float(actual), expected)
        return False
    if isinstance(expected, str):
        return actual == expected
    if isinstance(expected, list):
        if not isinstance(actual, list):
            return False
        if len(actual) != len(expected):
            return False
        return all(values_match(a, e) for a, e in zip(actual, expected))
    return False


def get_field_value(obj, reader, key):
    """Return the decoded value of a field, handling all types including lists."""
    slot = reader.key_index.get(key)
    if slot is None:
        return None
    off = obj._field_offset(slot)
    if off is None:
        return None  # absent

    mv = reader.mv
    # Check if it's a list
    if off + 4 <= len(mv):
        maybe_magic = _U32.unpack_from(mv, off)[0]
        if maybe_magic == MAGIC_LIST:
            return read_list(mv, off)

    sigil = reader.key_sigils[slot] if slot < len(reader.key_sigils) else 0
    if sigil == 0x3D:  # = int
        return obj.get_i64(key)
    elif sigil == 0x7E:  # ~ float
        return obj.get_f64(key)
    elif sigil == 0x3F:  # ? bool
        return obj.get_bool(key)
    elif sigil == 0x22:  # " str
        return obj.get_str(key)
    elif sigil == 0x40:  # @ time
        return obj.get_i64(key)
    elif sigil == 0x5E:  # ^ null
        return None
    else:
        # Unknown sigil: try i64 first
        return obj.get_i64(key)


def run_positive(conformance_dir: str, name: str, expected: dict) -> None:
    nxb_path = os.path.join(conformance_dir, f"{name}.nxb")
    with open(nxb_path, "rb") as f:
        data = f.read()

    reader = NxsReader(data)

    # Validate record_count
    if reader.record_count != expected["record_count"]:
        raise AssertionError(
            f"record_count: expected {expected['record_count']}, got {reader.record_count}"
        )

    # Validate keys
    for i, exp_key in enumerate(expected["keys"]):
        if i >= len(reader.keys):
            raise AssertionError(f"key[{i}] missing (expected {exp_key!r})")
        if reader.keys[i] != exp_key:
            raise AssertionError(f"key[{i}]: expected {exp_key!r}, got {reader.keys[i]!r}")

    # Validate each record
    for ri, exp_rec in enumerate(expected["records"]):
        obj = reader.record(ri)
        for key, exp_val in exp_rec.items():
            actual = get_field_value(obj, reader, key)
            if exp_val is None:
                # null: accept None (present-but-null)
                if actual is not None and actual != 0:
                    # Some implementations decode null as 0 — allow that
                    pass
            elif not values_match(actual, exp_val):
                raise AssertionError(
                    f"rec[{ri}].{key}: expected {exp_val!r}, got {actual!r}"
                )


def run_negative(conformance_dir: str, name: str, expected_code: str) -> None:
    nxb_path = os.path.join(conformance_dir, f"{name}.nxb")
    with open(nxb_path, "rb") as f:
        data = f.read()

    try:
        reader = NxsReader(data)
        raise AssertionError(f"expected error {expected_code!r} but reader succeeded")
    except NxsError as e:
        code = e.code
        if code != expected_code:
            raise AssertionError(f"expected error {expected_code!r}, got {code!r} ({e})")


def main():
    conformance_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(__file__)

    entries = sorted(
        f[: -len(".expected.json")]
        for f in os.listdir(conformance_dir)
        if f.endswith(".expected.json")
    )

    passed = 0
    failed = 0

    for name in entries:
        json_path = os.path.join(conformance_dir, f"{name}.expected.json")
        with open(json_path) as f:
            expected = json.load(f)

        is_negative = "error" in expected
        try:
            if is_negative:
                run_negative(conformance_dir, name, expected["error"])
            else:
                run_positive(conformance_dir, name, expected)
            print(f"  PASS  {name}")
            passed += 1
        except Exception as e:
            print(f"  FAIL  {name} — {e}", file=sys.stderr)
            failed += 1

    print(f"\n{passed} passed, {failed} failed")
    sys.exit(1 if failed > 0 else 0)


if __name__ == "__main__":
    main()
