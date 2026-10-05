#!/usr/bin/env python3
"""Cook authored resources, remove sources, and compare actual native/WASI replay."""
from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

REVISION = "8a0e251d19e222a0a6833d7408ca629f674e1729"
EXPECTED_STATE = "d73de0a5492c86856824210438b57324e557f90c5bc3cc0ddda22697003cab56"
EXPECTED_SNAPSHOT = "621ce538f35e35a782fe2399702ba01fac12c7bb0c3dbee9af9b2d2f23e77813"


def validate(value: dict) -> None:
    query = value.get("query", {})
    replay = value.get("replay", {})
    if (value.get("schema") != "wonderland.cooked-replay.v1"
            or value.get("sim_revision") != REVISION
            or query.get("stop") != {"Completed": "ReturnTrue"}
            or query.get("instructions") != 3
            or query.get("temps") != [37, 41] + [0] * 18
            or query.get("temp_xl") != [0, 0] or query.get("diagnostics") != []
            or query.get("snapshot_unchanged") is not True
            or replay.get("snapshot_tick") != 2 or replay.get("tick") != 3
            or replay.get("instructions") != 3 or replay.get("attributes") != [41]
            or replay.get("stop") != {"Completed": "ReturnTrue"}
            or replay.get("effect_dispatches") != 0
            or replay.get("snapshots_match") is not True
            or replay.get("state_hash") != EXPECTED_STATE
            or replay.get("snapshot_sha256") != EXPECTED_SNAPSHOT):
        raise ValueError("authored cooked scenario outcome mismatch")
    for key in ("state_hash", "snapshot_sha256"):
        digest = replay.get(key, "")
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise ValueError("invalid runtime digest")


def compare(native: dict, wasm: dict) -> None:
    validate(native)
    validate(wasm)
    if native != wasm:
        raise ValueError("native and WASI cooked results differ")


def capture(command: list[str]) -> dict:
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode:
        raise ValueError(f"command failed ({result.returncode}): {result.stderr[-8000:]}")
    return json.loads(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assembly", required=True, type=Path)
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--node", default="node")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    assembly = args.assembly.absolute()
    target = args.target_dir.resolve() if args.target_dir else assembly / "target"
    runner = [sys.executable, str(root / "tools/swarm-b/runtime-bridge.py"),
              "--cargo", args.cargo, str(assembly)]
    if args.skip_build:
        subprocess.run(runner, check=True, cwd=root, stdout=sys.stderr)
    else:
        base = [*runner, "build", "--locked", "--offline", "--no-default-features",
                "--bin", "runtime-bridge", "--target-dir", str(target)]
        subprocess.run([*base, "--example", "cooked-fixture"], check=True, cwd=root, stdout=sys.stderr)
        subprocess.run([*base, "--target", "wasm32-wasip1"], check=True, cwd=root, stdout=sys.stderr)
    native = str(target / "debug/runtime-bridge")
    with tempfile.TemporaryDirectory(prefix="cooked-parity-", dir=assembly) as scratch:
        release = Path(scratch) / "release"
        subprocess.run([str(target / "debug/examples/cooked-fixture"), str(release)],
                       check=True, stdout=sys.stderr)
        manifest = json.loads((release / "manifest.json").read_bytes())
        if len(manifest["packs"]) <= 1 or len(list(release.glob("*.wlp"))) != 1:
            raise ValueError("fixture must omit unrelated release packs")
        prepared = capture([native, "release-prepare", str(release / "draft.json"),
                            "--release-dir", str(release), "--output", str(release / "binding.json")])
        sha = prepared["binding_sha256"]
        command = [native, "release-replay", str(release / "binding.json"),
                   "--binding-sha256", sha, "--release-dir", str(release),
                   "--scenario", str(release / "scenario.json")]
        left = capture(command)
        right = capture([args.node, "--no-warnings", str(root / "tools/swarm-b/runtime-bridge-cooked-wasi.mjs"),
                         str(target / "wasm32-wasip1/debug/runtime-bridge.wasm"), str(release), sha])
        compare(left, right)
        # Equal but wrong output must fail the literal assertions too.
        changed = copy.deepcopy(right)
        changed["replay"]["attributes"][0] = 42
        try:
            compare(changed, changed)
        except ValueError:
            pass
        else:
            raise ValueError("equal changed-output negative control was accepted")
        changed = copy.deepcopy(right)
        changed["replay"]["state_hash"] = "0" * 64
        try:
            compare(changed, changed)
        except ValueError:
            pass
        else:
            raise ValueError("changed-digest negative control was accepted")
        pack = next(release.glob("*.wlp"))
        data = bytearray(pack.read_bytes())
        data[-1] ^= 1
        pack.write_bytes(data)
        corrupt = subprocess.run(command, capture_output=True, text=True)
        if corrupt.returncode == 0 or "digest" not in corrupt.stderr:
            raise ValueError("corrupt selected pack was not rejected before execution")
    print(json.dumps({"native_wasi_match": True, "authored_expectations_pass": True,
                      "source_files_removed": True, "unrelated_packs_omitted": True,
                      "negative_controls_rejected": 3, "result": left}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
