#!/usr/bin/env python3
"""Compare actual native/WASI source replay and enforce literal source outcomes."""
from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys

REVISION = "8a0e251d19e222a0a6833d7408ca629f674e1729"
SOURCE_HASH = "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865"
EXPECTED_STATE = "d52e0091425b12e7d820dcb7f1b883fb36606059b7a121bb5cf9218cc272f62b"
EXPECTED_SNAPSHOT = "49094c9adc9b1e3df73264ec221c323ce0027ec231af4f3e38b2b006fe0ca43d"


def validate(value: dict) -> None:
    query = value.get("query", {})
    replay = value.get("replay", {})
    if (value.get("schema") != "wonderland.runtime-source-probe.v1"
            or value.get("sim_revision") != REVISION
            or value.get("source_sha256") != SOURCE_HASH
            or value.get("source_bhav") != 4110
            or query.get("stop") != "ReturnTrue" or query.get("instructions") != 3
            or query.get("snapshot_unchanged") is not True
            or query.get("temps") != [0] * 20 or query.get("temp_xl") != [0, 0]
            or replay.get("snapshot_tick") != 1 or replay.get("completed_tick") != 2
            or replay.get("instructions") != 3 or replay.get("attributes") != [0, 0, 30, 0]
            or replay.get("durable_dispatches") != 0
            or replay.get("authority_matches") is not True
            or replay.get("snapshot_roundtrip") is not True
            or replay.get("state_hash") != EXPECTED_STATE
            or replay.get("snapshot_sha256") != EXPECTED_SNAPSHOT):
        raise ValueError("source-derived outcome mismatch")


def compare(native: dict, wasm: dict) -> None:
    validate(native)
    validate(wasm)
    if native != wasm:
        raise ValueError("native and WASI results differ")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assembly", required=True, type=Path)
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--node", default="node")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    assembly = args.assembly.absolute()
    target = args.target_dir.resolve() if args.target_dir else assembly / "target"
    runner = [sys.executable, str(root / "tools/swarm-b/runtime-bridge.py"), "--cargo", args.cargo, str(assembly)]
    if not args.skip_build:
        for platform in ([], ["--target", "wasm32-wasip1"]):
            command = [*runner, "build", "--locked", "--offline", "--no-default-features", "--example", "source-replay",
                       "--target-dir", str(target), *platform]
            subprocess.run(command, check=True, cwd=root, stdout=sys.stderr)
    else:
        # Skipping compilation still verifies the complete pinned A subtree.
        subprocess.run(runner, check=True, cwd=root, stdout=sys.stderr)
    source = root / "TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"
    native = subprocess.run([str(target / "debug/examples/source-replay"), str(source)],
                            check=True, capture_output=True, text=True)
    wasm = subprocess.run([args.node, "--no-warnings", str(root / "tools/swarm-b/runtime-bridge-wasi.mjs"),
                           str(target / "wasm32-wasip1/debug/examples/source-replay.wasm"), str(source)],
                          check=True, capture_output=True, text=True)
    left, right = json.loads(native.stdout), json.loads(wasm.stdout)
    compare(left, right)
    changed = copy.deepcopy(right)
    changed["replay"]["attributes"][0] = 1
    try:
        compare(left, changed)
    except ValueError:
        pass
    else:
        raise ValueError("changed-output negative control was accepted")
    changed_digest = copy.deepcopy(right)
    changed_digest["replay"]["state_hash"] = "0" * 64
    try:
        compare(left, changed_digest)
    except ValueError:
        pass
    else:
        raise ValueError("changed-digest negative control was accepted")
    print(json.dumps({"native_wasi_match": True, "source_expectations_pass": True,
                      "negative_controls_rejected": 2, "result": left}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
