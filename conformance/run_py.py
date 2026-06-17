#!/usr/bin/env python3
"""NXS conformance runner for Python.

Usage: python conformance/run_py.py conformance/

Row vectors work with pure ``nxs.NxsReader``. Columnar and PAX vectors require
the C extension (links ``nyxis-drivers/c/nxs.c``):

    cd nyxis-drivers/py && bash build_ext.sh
"""
import json
import math
import os
import struct
import sys

_drv = os.environ.get("DRV") or os.path.join(
    os.path.dirname(__file__), "..", "..", "nyxis-drivers"
)
_py_drv = os.path.join(os.path.abspath(_drv), "py")
sys.path.insert(0, _py_drv)

_HAS_CEXT = False
try:
    import _nxs  # noqa: E402

    _HAS_CEXT = True
except ImportError:
    _nxs = None  # type: ignore

from nxs import NxsReader, NxsError  # noqa: E402

MAGIC_LIST = 0x4E59584C  # NYXL


def is_layout_vector(name: str) -> bool:
    return name.startswith("columnar_") or name.startswith("pax_")


def parse_cext_error(exc: BaseException) -> str:
    msg = str(exc)
    for token in (
        "ERR_INVALID_PAGE_MAGIC",
        "ERR_INCOMPATIBLE_FLAGS",
        "ERR_INVALID_FLAGS",
        "ERR_UNSUPPORTED_FIELD_TYPE",
        "ERR_UNSUPPORTED",
        "ERR_DICT_MISMATCH",
        "ERR_KEY_NOT_FOUND",
        "ERR_OUT_OF_BOUNDS",
        "ERR_BAD_MAGIC",
    ):
        if token in msg:
            return token
    return msg.split(":", 1)[0].strip()


class CextNxsError(Exception):
    def __init__(self, code: str, message: str = "") -> None:
        super().__init__(f"{code}: {message}" if message else code)
        self.code = code


def open_reader(data: bytes):
    """Return (reader, backend) where backend is 'cext' or 'pure'."""
    if _HAS_CEXT:
        try:
            return _nxs.Reader(data), "cext"
        except ValueError as e:
            raise CextNxsError(parse_cext_error(e), str(e)) from e
    return NxsReader(data), "pure"


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


SIGIL_LIST = 0x4C  # L


def get_field_value(obj, reader, key, backend: str):
    """Return the decoded value of a field."""
    slot = reader.key_index.get(key)
    if slot is None:
        return None

    sigil = reader.key_sigils[slot] if slot < len(reader.key_sigils) else 0

    if sigil == SIGIL_LIST:
        if backend == "cext":
            off = obj.field_offset(key)
        else:
            off = obj._field_offset(slot)
        if off is None:
            return None
        mv = reader.buffer if backend == "cext" else reader.mv
        return read_list(mv, off)

    if sigil == 0x3D:  # = int
        return obj.get_i64(key)
    if sigil == 0x7E:  # ~ float
        return obj.get_f64(key)
    if sigil == 0x3F:  # ? bool
        return obj.get_bool(key)
    if sigil == 0x22:  # " str
        return obj.get_str(key)
    if sigil == 0x40:  # @ time
        return obj.get_i64(key)
    if sigil == 0x5E:  # ^ null
        return None
    return obj.get_i64(key)


def run_positive(conformance_dir: str, name: str, expected: dict, backend: str) -> None:
    nxb_path = os.path.join(conformance_dir, f"{name}.nxb")
    with open(nxb_path, "rb") as f:
        data = f.read()

    reader, _ = open_reader(data)

    if reader.record_count != expected["record_count"]:
        raise AssertionError(
            f"record_count: expected {expected['record_count']}, got {reader.record_count}"
        )

    keys = list(reader.keys)
    for i, exp_key in enumerate(expected["keys"]):
        if i >= len(keys):
            raise AssertionError(f"key[{i}] missing (expected {exp_key!r})")
        if keys[i] != exp_key:
            raise AssertionError(f"key[{i}]: expected {exp_key!r}, got {keys[i]!r}")

    for ri, exp_rec in enumerate(expected["records"]):
        obj = reader.record(ri)
        for key, exp_val in exp_rec.items():
            actual = get_field_value(obj, reader, key, backend)
            if exp_val is None:
                if actual is not None and actual not in (0, False, ""):
                    raise AssertionError(
                        f"rec[{ri}].{key}: expected null, got {actual!r}"
                    )
            elif not values_match(actual, exp_val):
                raise AssertionError(
                    f"rec[{ri}].{key}: expected {exp_val!r}, got {actual!r}"
                )


def run_negative(conformance_dir: str, name: str, expected_code: str) -> None:
    nxb_path = os.path.join(conformance_dir, f"{name}.nxb")
    with open(nxb_path, "rb") as f:
        data = f.read()

    try:
        open_reader(data)
        raise AssertionError(f"expected error {expected_code!r} but reader succeeded")
    except (NxsError, CextNxsError) as e:
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
    skipped = 0

    for name in entries:
        if is_layout_vector(name) and not _HAS_CEXT:
            print(
                f"  SKIP  {name} (columnar/PAX requires C extension; "
                f"build: cd {_py_drv} && bash build_ext.sh)",
                file=sys.stderr,
            )
            skipped += 1
            continue

        json_path = os.path.join(conformance_dir, f"{name}.expected.json")
        with open(json_path) as f:
            expected = json.load(f)

        if expected.get("forward_stream") is True:
            print(
                f"  SKIP  {name} (forward_stream requires StreamReader; not implemented)",
                file=sys.stderr,
            )
            skipped += 1
            continue

        is_negative = "error" in expected
        backend = "cext" if _HAS_CEXT else "pure"
        try:
            if is_negative:
                run_negative(conformance_dir, name, expected["error"])
            else:
                run_positive(conformance_dir, name, expected, backend)
            print(f"  PASS  {name}")
            passed += 1
        except Exception as e:
            print(f"  FAIL  {name} — {e}", file=sys.stderr)
            failed += 1

    print(f"\n{passed} passed, {failed} failed", end="")
    if skipped:
        print(f", {skipped} skipped (no C extension)", end="")
    print()
    sys.exit(1 if failed > 0 else 0)


if __name__ == "__main__":
    main()
