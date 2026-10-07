"""Version-1 numeric interpreter projection. No field filtering or tolerances."""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re

FIELDS = [
    "case", "ts1", "tick", "clock_ticks", "rng", "entities", "object_id",
    "attribute_0", "attribute_1", "attribute_2", "attribute_3",
    "stack_count", "routine", "pc", "arg0", "local0",
]
CASES = ("source4110", "authored")
MODES = (1, 0)
TICKS = 32
MAX_BYTES = 1024 * 1024
NUMBER = re.compile(r"(?:0|[1-9][0-9]*|-[1-9][0-9]*)\Z")
BOUNDS = [
    None, (0, 1), (1, TICKS), (0, 2**64-1), (0, 2**64-1),
    (0, 32767), (1, 32767),
    *[(-32768, 32767)]*4, (0, 8), (-1, 65535), (-1, 255),
    (-32768, 32767), (-32768, 32767),
]

def parse_trace(data: bytes) -> list[list[str | int]]:
    if not isinstance(data, bytes) or not 0 < len(data) <= MAX_BYTES:
        raise ValueError("trace byte limit")
    try:
        text = data.decode("ascii")
    except UnicodeDecodeError as error:
        raise ValueError("trace must be ASCII without BOM") from error
    if "\r" in text or not text.endswith("\n") or text.endswith("\n\n"):
        raise ValueError("trace requires one LF after every record")
    lines = text[:-1].split("\n")
    if lines[0] != "\t".join(FIELDS):
        raise ValueError("interpreter trace schema mismatch")
    if len(lines) != 1 + len(CASES)*len(MODES)*TICKS:
        raise ValueError("incomplete or extra interpreter rows")
    result = []
    for n, line in enumerate(lines[1:]):
        parts = line.split("\t")
        if len(parts) != len(FIELDS):
            raise ValueError(f"wrong column count at row {n+1}")
        row: list[str | int] = [parts[0]]
        for name, cell, bounds in zip(FIELDS[1:], parts[1:], BOUNDS[1:]):
            if len(cell) > 21 or not NUMBER.fullmatch(cell):
                raise ValueError(f"noncanonical integer {name} at row {n+1}")
            value = int(cell)
            if not bounds[0] <= value <= bounds[1]:
                raise ValueError(f"out-of-range {name} at row {n+1}")
            row.append(value)
        expected_key = [CASES[n // (2*TICKS)], MODES[(n // TICKS) % 2], n % TICKS + 1]
        if row[:3] != expected_key:
            raise ValueError(f"unexpected, duplicate, or missing row key at row {n+1}")
        if row[11] == 0 and row[12:] != [-1]*4:
            raise ValueError("empty stack must have absent frame sentinels")
        if row[11] > 0 and (row[12] < 256 or row[13] < 0):
            raise ValueError("active stack must identify a routine and instruction")
        result.append(row)
    return result

def _reference_coverage(rows: list[list[str | int]]) -> None:
    for case in CASES:
        for mode in MODES:
            cohort = [r for r in rows if r[:2] == [case, mode]]
            if any(r[3] != r[2] or r[5:7] != [1, 1] for r in cohort):
                raise ValueError("reference did not advance its one-object VM")
            if case == "source4110":
                if any(r[7:12] != [0, 0, 30, 0, 0] for r in cohort):
                    raise ValueError("original source behavior contract not observed")
                if any(b[4] != a[4]+1 for a,b in zip(cohort,cohort[1:])):
                    raise ValueError("reference source scheduler RNG mixing not observed")
            else:
                if not {11, -11}.issubset({r[8] for r in cohort}):
                    raise ValueError("reference did not exercise both branch paths")
                if not {0, 2}.issubset({r[11] for r in cohort}):
                    raise ValueError("reference did not yield in a nested call and return")
                if max(r[9] for r in cohort) <= 30 or max(r[10] for r in cohort) <= 40:
                    raise ValueError("reference call/continuation mutations not observed")

def compare(reference: bytes, candidate: bytes) -> dict:
    expected = parse_trace(reference)
    actual = parse_trace(candidate)
    _reference_coverage(expected)
    result = {
        "schema": 1, "scope": "interpreter-observable-projection-v1",
        "full_vm_state_parity": False, "rows": len(expected),
        "fields": FIELDS, "passed": True, "first_difference": None,
        "reference_sha256": hashlib.sha256(reference).hexdigest(),
        "candidate_sha256": hashlib.sha256(candidate).hexdigest(),
    }
    for left, right in zip(expected, actual):
        for i, (a, b) in enumerate(zip(left, right)):
            if a != b:
                result["passed"] = False
                result["first_difference"] = {
                    "case": left[0], "ts1": left[1], "tick": left[2],
                    "field": FIELDS[i], "expected": a, "actual": b,
                }
                return result
    return result

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=pathlib.Path)
    parser.add_argument("candidate", type=pathlib.Path)
    args = parser.parse_args()
    try:
        for path in (args.reference,args.candidate):
            if not path.is_file() or path.stat().st_size > MAX_BYTES:
                raise ValueError("trace path/size invalid")
        result = compare(args.reference.read_bytes(), args.candidate.read_bytes())
    except (OSError, ValueError) as error:
        print(json.dumps({"schema": 1, "passed": False, "error": str(error)}))
        return 2
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1

if __name__ == "__main__":
    raise SystemExit(main())
