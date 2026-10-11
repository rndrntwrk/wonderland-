#!/usr/bin/env python3
"""Execute and compare every original chair/bed/appliance private BHAV on native/WASI."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

REVISION = "8a0e251d19e222a0a6833d7408ca629f674e1729"
SOURCES = {
    "chair": (21, "ab9975d947e64ea19ac194679dee6080f99a5b4c0a4c89f03f719d75113d57a7"),
    "bed": (72, "c8289640be973e53a79a7a0d4430809f8e306cc7e6b38bec0c7eaa187a5808bb"),
    "appliance": (51, "7a2eb22e4958dc12bce6b470a263e87ecfa0cd57019deafd3efb24270a41a145"),
}
MAX_REPORT_BYTES = 16 * 1024 * 1024


def execution(family: dict, routine: int, context: str) -> dict:
    matches = [item for item in family["executions"]
               if item["routine"]["id"] == routine and item["context"] == context]
    if len(matches) != 1:
        raise ValueError("missing or duplicate source routine/context")
    return matches[0]


def validate(value: dict) -> None:
    if (value.get("schema") != "wonderland.runtime-source-families.v1"
            or value.get("sim_revision") != REVISION
            or value.get("source_revision") != "4c6b3e8f5835b228723caea3c9f683c62f244f73"):
        raise ValueError("source-family schema/revision mismatch")
    families = value.get("families", [])
    if len(families) != 6:
        raise ValueError("expected every source in both dialects")
    seen = set()
    for family in families:
        identity = (family["name"], family["mode"])
        if identity in seen or identity[0] not in SOURCES or identity[1] not in ("Ts1", "Tso"):
            raise ValueError("duplicate or invalid source dialect")
        seen.add(identity)
        count, source_hash = SOURCES[identity[0]]
        if (family["routine_count"] != count or family["source_sha256"] != source_hash
                or family["complete_gameplay"] is not False
                or family["rejected_routines"] or not family["unresolved_calls"]
                or len(family["executions"]) != count * 2):
            raise ValueError("source routine inventory or missing-content scope changed")
        keys = set()
        for item in family["executions"]:
            key = (item["routine"]["id"], item["context"])
            if key in keys or key[1] not in ("object_self", "avatar_to_object"):
                raise ValueError("duplicate execution or invalid source context")
            keys.add(key)
            if (item["query"]["snapshot_unchanged"] is not True
                    or item["accepted"]["authority_matches"] is not True
                    or item["accepted"]["replica_external_dispatches"] != 0
                    or item["accepted"]["snapshot_roundtrip"] is not True):
                raise ValueError("source isolation, accepted replay or round-trip mismatch")
        main = execution(family, 4096, "object_self")
        if (main["query"]["stop"] != {"Completed": "Error"}
                or not any(item.get("MissingRoutine", {}).get("id") == 280
                           for item in main["query"]["diagnostics"])):
            raise ValueError("missing original global 280 was hidden")
        if identity[0] == "chair":
            helper = execution(family, 4122, "object_self")
            if (helper["query"]["instructions"] != 2
                    or helper["accepted"]["object"]["object_data"][7] != 30):
                raise ValueError("source chair room-impact outcome changed")
        elif identity[0] == "bed":
            helper = execution(family, 4114, "avatar_to_object")
            if (helper["query"]["instructions"] != 3
                    or helper["accepted"]["actor"]["motives"][11] != -40
                    or helper["accepted"]["actor"]["person_data"][27] != 1):
                raise ValueError("source bed sleep/TickCounter outcome changed")
        else:
            helper = execution(family, 4102, "object_self")
            if (helper["query"]["temps"][0] != 2
                    or helper["query"]["instructions"] != 3
                    or helper["accepted"]["rng_after"] != 4_194_304_100
                    or helper["accepted"]["object"]["attributes"][1] != 0
                    or not any(item["kind"] == "STR#" and item["id"] == 300
                               for item in family["rejected_resources"])):
                raise ValueError("source appliance RNG or rejected resource evidence changed")


def compare(native: dict, wasi: dict) -> None:
    validate(native)
    validate(wasi)
    if native != wasi:
        raise ValueError("native and WASI source-family results differ")


def inventory(report: dict, raw: bytes) -> dict:
    """Keep every observed outcome/call site while omitting repeated raw memory."""
    result = {
        "schema": "wonderland.runtime-source-family-inventory.v1",
        "sim_revision": report["sim_revision"],
        "source_revision": report["source_revision"],
        "scope": report["scope"],
        "verified_report_sha256": hashlib.sha256(raw).hexdigest(),
        "verified_report_bytes": len(raw),
        "native_wasi_match": True,
        "negative_controls_rejected": 2,
        "families": [],
    }
    for family in report["families"]:
        item = {key: family[key] for key in (
            "name", "mode", "source", "source_sha256", "guid", "object_chunk_id",
            "object_version", "source_multitile_master", "source_subindex",
            "routine_count", "harness", "complete_gameplay", "primitive_requirements",
            "rejected_resources", "rejected_routines", "unresolved_calls")}
        routines, counts = {}, {}
        groups: dict[str, dict] = {"query": {}, "accepted": {}}
        for case in family["executions"]:
            routine = case["routine"]
            routines[routine["id"]] = {**routine, "read_only_proof": case["read_only_proof"]}
            for stage in groups:
                state = case[stage]
                outcome = {key: state[key] for key in ("stop", "error", "diagnostics") if key in state}
                key = json.dumps(outcome, sort_keys=True, separators=(",", ":"))
                group = groups[stage].setdefault(key, {"outcome": outcome, "cases": []})
                entry = {"routine": routine["id"], "context": case["context"]}
                if stage == "query":
                    entry["instructions"] = state.get("instructions")
                else:
                    entry["instructions"] = sum(tick["instructions"] for tick in state["ticks"])
                    entry["state_hash"] = state["state_hash"]
                group["cases"].append(entry)
            stop = case["accepted"].get("stop")
            if case["accepted"].get("error"):
                kind = "Rejected"
            elif isinstance(stop, dict):
                kind = "Completed " + stop["Completed"] if "Completed" in stop else next(iter(stop))
            else:
                kind = str(stop)
            counts[kind] = counts.get(kind, 0) + 1
        item["routines"] = [routines[key] for key in sorted(routines)]
        item["execution_count"] = len(family["executions"])
        item["accepted_outcome_counts"] = dict(sorted(counts.items()))
        item["outcome_groups"] = {stage: [groups[stage][key] for key in sorted(groups[stage])]
                                  for stage in groups}
        result["families"].append(item)
    return result


def run_report(command: list[str], root: Path) -> tuple[dict, bytes]:
    # The tested programs are local build artifacts. Retain stdout in a temporary
    # file and bound it before reading/JSON allocation, including --skip-build.
    with tempfile.TemporaryFile() as output:
        subprocess.run(command, check=True, cwd=root, stdout=output, timeout=180)
        size = output.tell()
        if size > MAX_REPORT_BYTES:
            raise ValueError("source-family report byte limit")
        output.seek(0)
        raw = output.read(MAX_REPORT_BYTES + 1)
    return json.loads(raw), raw


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assembly", required=True, type=Path)
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--node", default="node")
    parser.add_argument("--report", type=Path, help="optional full verified native JSON report")
    parser.add_argument("--inventory", type=Path, help="optional verified outcome and unresolved-call inventory")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    assembly = args.assembly.absolute()
    target = args.target_dir.resolve() if args.target_dir else assembly / "target"
    runner = [sys.executable, str(root / "tools/swarm-b/runtime-bridge.py"), "--cargo", args.cargo, str(assembly)]
    if args.skip_build:
        subprocess.run(runner, check=True, cwd=root, stdout=sys.stderr)
    else:
        for platform in ([], ["--target", "wasm32-wasip1"]):
            subprocess.run([*runner, "build", "--locked", "--offline", "--no-default-features",
                            "--example", "source-families", "--target-dir", str(target), *platform],
                           check=True, cwd=root, stdout=sys.stderr)
    native, raw = run_report([str(target / "debug/examples/source-families"), str(root)], root)
    wasi, _ = run_report([args.node, "--no-warnings", str(root / "tools/swarm-b/runtime-source-families-wasi.mjs"),
                          str(target / "wasm32-wasip1/debug/examples/source-families.wasm"), str(root)], root)
    compare(native, wasi)
    for change in ("state_hash", "source_sha256"):
        bad = copy.deepcopy(wasi)
        if change == "state_hash":
            bad["families"][0]["executions"][0]["accepted"]["state_hash"] = "0" * 64
        else:
            bad["families"][0][change] = "0" * 64
        try:
            compare(native, bad)
        except ValueError:
            pass
        else:
            raise ValueError("changed source/accepted-state negative control was accepted")
    if args.report:
        # Only publish a complete report after every comparison has succeeded.
        args.report.write_bytes(raw)
    if args.inventory:
        args.inventory.write_text(json.dumps(inventory(native, raw), sort_keys=True, indent=2) + "\n")
    summaries = []
    for family in native["families"]:
        stops = {}
        for item in family["executions"]:
            stop = item["accepted"].get("stop")
            kind = next(iter(stop)) if isinstance(stop, dict) else str(stop)
            stops[kind] = stops.get(kind, 0) + 1
        summaries.append({"name": family["name"], "mode": family["mode"],
                          "routines": family["routine_count"], "executions": len(family["executions"]),
                          "unresolved_calls": len(family["unresolved_calls"]), "accepted_stops": stops,
                          "rejected_resources": family["rejected_resources"]})
    print(json.dumps({"native_wasi_match": True, "source_expectations_pass": True,
                      "negative_controls_rejected": 2, "report_sha256": hashlib.sha256(raw).hexdigest(),
                      "report_bytes": len(raw), "families": summaries}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error)) from error
