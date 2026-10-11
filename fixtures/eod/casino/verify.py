#!/usr/bin/env python3
"""Execute exact pinned original casino helpers, not complete casino handlers.

The complete original AbstractPlayingCardsDeck.cs is compiled unchanged. Two
byte slices from VMEODBlackjackPlugin.cs retain the original value dictionary,
BlackjackPlayer implementation and enums. Generated scaffolding supplies only
namespace/import/class containers and an unused opaque VMEODClient field type.
No VM, UI, account provider or original/native RNG equivalence is modeled.
"""

import argparse
import difflib
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def run(command, *, binary=False, timeout=30):
    result = subprocess.run(
        command, cwd=ROOT, capture_output=True, text=not binary, timeout=timeout
    )
    if result.returncode:
        output = result.stdout + result.stderr
        if binary:
            output = output.decode("utf-8", errors="replace")
        raise RuntimeError(f"command failed ({result.returncode}): {command[0]}\n{output}")
    return result.stdout


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def equal_trace(actual, expected):
    return actual == expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    manifest = json.loads((HERE / "sources.json").read_text())
    pin = manifest["original_commit"]
    original = {}
    for item in manifest["files"]:
        path = item["path"]
        data = (ROOT / path).read_bytes()
        if sha256(data) != item["sha256"]:
            raise RuntimeError(f"original file hash changed: {path}")
        pinned = run(["git", "show", f"{pin}:{path}"], binary=True)
        if data != pinned:
            raise RuntimeError(f"original file differs from pinned Git bytes: {path}")
        original[path] = data
    spec = manifest["helper_oracle"]
    slices = {}
    for item in spec["source_slices"]:
        data = original[item["path"]]
        start, end = item["start_byte"], item["end_byte_exclusive"]
        if not (0 <= start < end <= len(data)):
            raise RuntimeError(f"invalid source slice bounds: {item['name']}")
        extracted = data[start:end]
        start_line = data[:start].count(b"\n") + 1
        end_line = data[:end].count(b"\n") + 1
        if (sha256(extracted) != item["sha256"]
                or start_line != item["start_line"]
                or end_line != item["end_line"]):
            raise RuntimeError(f"original source slice changed: {item['name']}")
        slices[item["name"]] = extracted
    for command in ("mcs", "mono"):
        if shutil.which(command) is None:
            raise RuntimeError(f"required executable unavailable: {command}")
    scaffold = (
        b"using System;\nusing System.Collections.Generic;\n"
        b"using FSO.SimAntics.NetPlay.EODs.Utils;\n"
        b"namespace FSO.SimAntics.NetPlay.EODs { public sealed class VMEODClient {} }\n"
        b"namespace FSO.SimAntics.NetPlay.EODs.Handlers {\n"
        b"public static class VMEODBlackjackPlugin {\n"
        + slices["blackjack_values"] + b"\n}\n"
        + slices["blackjack_player_and_enums"] + b"\n}\n"
    )
    expected = (HERE / "expected-helper.txt").read_text()
    with tempfile.TemporaryDirectory(prefix="eod-casino-helper-") as work:
        generated = Path(work) / "OriginalBlackjackHelpers.cs"
        generated.write_bytes(scaffold)
        executable = Path(work) / "SourceHelperOracle.exe"
        run([
            "mcs", f"-out:{executable}", str(generated),
            str(HERE / "SourceHelperOracle.cs"),
            *[str(ROOT / path) for path in spec["compiled_whole_files"]],
        ], timeout=60)
        traces = [run(["mono", str(executable)]) for _ in range(2)]
    for actual in traces:
        if not equal_trace(actual, expected):
            difference = "".join(difflib.unified_diff(
                expected.splitlines(True), actual.splitlines(True),
                fromfile="checked-in literal helper trace", tofile="original C# helper trace"
            ))
            raise RuntimeError("original helper trace mismatch\n" + difference)
    mutations = {
        "circular_deck": ("deck.circular=104,52,True", "deck.circular=104,52,False"),
        "soft17": ("hand.soft17=17,True,2", "hand.soft17=17,False,2"),
        "ten_value_split": ("hand.ten_value_split=20,False,20", "hand.ten_value_split=20,False,2"),
        "odd_natural": ("payout.odd_natural=12", "payout.odd_natural=13"),
        "odd_insurance": ("payout.odd_insurance=5", "payout.odd_insurance=6"),
        "double_push": ("payout.double_push=20", "payout.double_push=10"),
        "split_settlement": ("split.payout=65", "split.payout=60"),
        "split_aces": ("split.aces_payout=45", "split.aces_payout=40"),
    }
    rejected = []
    for label, (before, after) in mutations.items():
        altered = expected.replace(before, after, 1)
        if altered == expected or equal_trace(traces[0], altered):
            raise RuntimeError(f"negative control failed: {label}")
        rejected.append(label)
    report = {
        "schema": 1,
        "source_pin": pin,
        "whole_file_and_pinned_git_bytes": "PASS",
        "verified_original_files": len(original),
        "compiled_whole_files": spec["compiled_whole_files"],
        "compiled_source_slices": spec["source_slices"],
        "scaffolding": spec["scaffolding"],
        "exact_repeated_runs": 2,
        "literal_trace_lines": len(expected.splitlines()),
        "trace_sha256": sha256(expected.encode()),
        "negative_controls_rejected": rejected,
        "result": "PASS",
        "scope": spec["scope"],
        "not_qualified": [
            "complete original handlers or graphical client",
            "production VM/account provider integration",
            "original versus native System.Random sequence equivalence",
            "original HoldemHand evaluator, roulette or slots execution",
        ],
    }
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
