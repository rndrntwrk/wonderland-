#!/usr/bin/env python3
"""Verify pinned original service components, repeated traces and negative controls.

VM/content/provider callbacks are authored test adapters, not a real service. The
native host tests independently consume selected exact original literals.
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

def run(argv, **kwargs):
    result = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, **kwargs)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {argv[0]}\n" + result.stdout + result.stderr)
    return result.stdout

def equal_trace(actual, expected):
    return actual == expected

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    manifest = json.loads((HERE / "sources.json").read_text())
    compiled = []
    for item in manifest["sources"]:
        path = ROOT / item["path"]
        raw = path.read_bytes()
        current_blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if current_blob != item["git_blob"]:
            raise RuntimeError(f"working bytes differ from pinned Git blob: {item['path']}")
        if hashlib.sha256(raw).hexdigest() != item["sha256"]:
            raise RuntimeError(f"original source changed: {item['path']}")
        blob = run(["git", "rev-parse", f"{manifest['source_pin']}:{item['path']}"]).strip()
        if blob != item["git_blob"]:
            raise RuntimeError(f"incorrect source pin: {item['path']}")
        if item["kind"] == "compile":
            compiled.append(str(path))
    if len(compiled) != 14:
        raise RuntimeError("incorrect original component set")
    for program in ("mcs", "mono"):
        if shutil.which(program) is None:
            raise RuntimeError(f"required executable unavailable: {program}")
    expected = (HERE / "expected.txt").read_text()
    with tempfile.TemporaryDirectory(prefix="eod-service-source-") as work:
        exe = Path(work) / "SourceOracle.exe"
        run(["mcs", f"-out:{exe}", str(HERE / "SourceAdapters.cs"), str(HERE / "SourceOracle.cs"), *compiled], timeout=60)
        runs = [run(["mono", str(exe)], timeout=30) for _ in range(2)]
    for actual in runs:
        if not equal_trace(actual, expected):
            diff = "".join(difflib.unified_diff(expected.splitlines(True), actual.splitlines(True), fromfile="checked-in original service trace", tofile="current original service trace"))
            raise RuntimeError("source trace mismatch\n" + diff)
    mutations = {
        "property_sign_extension": ("property.show=0080ffff", "property.show=00800100"),
        "animation_latest": ("bulletin.latest=2:;4:;3:", "bulletin.latest=2:;3:;3:"),
        "newspaper_float": ("0000a03f02000000", "0000803f02000000"),
        "draw_frequency_255": ("05466972737401075570646174656402055468697264ff", "0546697273740107557064617465640205546869726401"),
        "cooldown_account_category": ("cooldown.0.6=104:", "cooldown.0.6=103:"),
        "trunk_halloween_fallback": ("1,25,6000069312525", "1,25,123"),
        "trade_acceptance_delay": ("trade.before_delay=0", "trade.before_delay=1"),
        "source_property_bug": ("trade.source_duplicate_properties=2", "trade.source_duplicate_properties=1"),
    }
    rejected = []
    for label, (before, after) in mutations.items():
        altered = expected.replace(before, after, 1)
        if altered == expected or equal_trace(runs[0], altered):
            raise RuntimeError(f"negative control failed: {label}")
        rejected.append(label)
    report = {
        "schema": 1, "source_pin": manifest["source_pin"],
        "compiled_original_sources": len(compiled),
        "inspected_original_sources": sum(s["kind"] == "inspection" for s in manifest["sources"]),
        "unchanged_source_hashes": "PASS", "exact_repeated_runs": 2,
        "trace_sha256": hashlib.sha256(expected.encode()).hexdigest(),
        "negative_controls_rejected": rejected, "result": "PASS",
        "scope": "Original C# service component semantics with authored VM/content/provider boundaries; real native host recovery/admission is tested separately. No production provider or complete original-application equivalence claim.",
    }
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))

if __name__ == "__main__":
    main()
