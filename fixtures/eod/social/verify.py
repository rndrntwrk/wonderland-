#!/usr/bin/env python3
"""Run unchanged FreeSO social handlers against explicit VM/transport adapters.

The checked-in literal output is compared twice, never regenerated. This is a
source semantic oracle, not an implementation of the VM, a payment provider, or
a claim that Mono's System.Random is the native replay stream.
"""

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def run(command, **kwargs):
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {command[0]}\n"
            + result.stdout + result.stderr
        )
    return result.stdout


def equal_trace(actual, expected):
    return actual == expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    manifest = json.loads((HERE / "sources.json").read_text())
    compiled = []
    reference = None
    for item in manifest["sources"]:
        path = ROOT / item["path"]
        raw = path.read_bytes()
        current_blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if current_blob != item["git_blob"]:
            raise RuntimeError(f"working bytes differ from pinned Git blob: {item['path']}")
        digest = hashlib.sha256(raw).hexdigest()
        if digest != item["sha256"]:
            raise RuntimeError(f"original source changed: {item['path']}")
        blob = run(["git", "rev-parse", f"{manifest['source_pin']}:{item['path']}"]).strip()
        if blob != item["git_blob"]:
            raise RuntimeError(f"incorrect source pin: {item['path']}")
        if item["kind"] == "compile":
            compiled.append(str(path))
        elif item["kind"] == "reference":
            reference = path
    if not compiled or reference is None:
        raise RuntimeError("missing source/reference inputs")
    for program in ("mcs", "mono"):
        if shutil.which(program) is None:
            raise RuntimeError(f"required executable unavailable: {program}")
    expected = (HERE / "expected.txt").read_text()
    with tempfile.TemporaryDirectory(prefix="eod-social-source-") as work:
        exe = Path(work) / "SourceOracle.exe"
        run([
            "mcs", f"-r:{reference}", f"-out:{exe}",
            str(HERE / "SourceAdapters.cs"), str(HERE / "SourceOracle.cs"),
            *compiled,
        ], timeout=60)
        env = os.environ.copy()
        env["MONO_PATH"] = str(reference.parent)
        runs = [run(["mono", str(exe)], env=env, timeout=30) for _ in range(2)]
    for actual in runs:
        if not equal_trace(actual, expected):
            difference = "".join(difflib.unified_diff(
                expected.splitlines(True), actual.splitlines(True),
                fromfile="checked-in source trace", tofile="current source trace",
            ))
            raise RuntimeError("source trace mismatch\n" + difference)
    # Exercise the exact comparator with changed graph, payout, raster and timing
    # results. Acceptance must be sensitive to each independently obtained value.
    mutations = {
        "combat_graph": ("war.pairings=-1,1", "war.pairings=-1,0"),
        "last_piece_stall": ("source_outputs=0", "source_outputs=1"),
        "payout": ("1454,2684", "1455,2684"),
        "freshness": ("c98f943e", "c88f943e"),
        "raster": ("floor.rainbow1=07", "floor.rainbow1=06"),
        "buzzer_window": ("buzzer.state29=Engaged", "buzzer.state29=Locked"),
    }
    controls = []
    for label, (before, after) in mutations.items():
        altered = expected.replace(before, after, 1)
        if altered == expected or equal_trace(runs[0], altered):
            raise RuntimeError(f"negative control failed: {label}")
        controls.append(label)
    report = {
        "schema": 1,
        "source_pin": manifest["source_pin"],
        "compiled_original_sources": len(compiled),
        "inspected_original_sources": sum(s["kind"] == "inspection" for s in manifest["sources"]),
        "original_mono_game_reference": reference.relative_to(ROOT).as_posix(),
        "unchanged_source_hashes": "PASS",
        "exact_repeated_runs": 2,
        "trace_sha256": hashlib.sha256(expected.encode()).hexdigest(),
        "negative_controls_rejected": controls,
        "result": "PASS",
        "scope": "Original C# handler semantics with boundary adapters; native integration is tested separately through NativeHost.",
    }
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
