#!/usr/bin/env python3
"""Exercise the shipped ledger CLI with explicitly invalid admission inputs."""

import copy
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "tests/output/full-game-ledger"
OUT.mkdir(parents=True, exist_ok=True)
PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
base = json.loads((ROOT / "docs/compat/game-capabilities.json").read_text())
checks = []


def reject(name, data, needle, extra=()):
    p = OUT / (name + ".json")
    p.write_text(json.dumps(data))
    result = subprocess.run(
        [
            "python3",
            "tools/completion/verify-ledger.py",
            "--reference",
            PIN,
            "--mode",
            "inventory",
            "--ledger",
            str(p.relative_to(ROOT)),
            *extra,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    report = json.loads(result.stdout)
    assert result.returncode == 1 and report["status"] == "failed", (name, report)
    assert any(needle in error for error in report["errors"]), (name, report)
    assert "qualified" not in report and "intake_complete" not in report, (name, report)
    checks.append(name)


v = copy.deepcopy(base)
v["capabilities"][0]["source_paths"].append("TSOClient/absent-for-admission-case.cs")
v["capabilities"][0]["source_sha256"]["TSOClient/absent-for-admission-case.cs"] = (
    "0" * 64
)
reject("missing-source", v, "Missing or external path")
v = copy.deepcopy(base)
v["capabilities"].append(copy.deepcopy(v["capabilities"][0]))
reject("duplicate-id", v, "Duplicate capability ID")
v = copy.deepcopy(base)
v["capabilities"] = [
    c
    for c in v["capabilities"]
    if c["id"]
    != next(e["id"] for e in base["capabilities"] if e["id"].startswith("eod."))
]
reject("omitted-registry-member", v, "Omitted preservation member")
v = copy.deepcopy(base)
v["capabilities"][0]["status"] = "qualified"
reject("missing-evidence", v, "qualified without implementation/evidence")
for name, field, wrong in [
    ("wrong-source-evidence", "original_reference", "1" * 40),
    ("wrong-build-evidence", "implementation_commit", "2" * 40),
    ("wrong-content-evidence", "content_release", "wrong-release"),
]:
    v = copy.deepcopy(base)
    cap = v["capabilities"][0]
    cap["status"] = "qualified"
    cap["rust_paths"] = ["crates/game-runtime/src/lib.rs"]
    evidence = {
        "status": "passed",
        "original_reference": PIN,
        "implementation_commit": "3" * 40,
        "content_release": "declared-admission-fixture",
        "capability_ids": [cap["id"]],
    }
    evidence[field] = wrong
    p = OUT / (name + "-evidence.json")
    p.write_text(json.dumps(evidence))
    cap["evidence"] = [str(p.relative_to(ROOT))]
    reject(
        name,
        v,
        "incompatible evidence",
        (
            "--implementation-commit",
            "3" * 40,
            "--content-release",
            "declared-admission-fixture",
        ),
    )
v = copy.deepcopy(base)
v["capabilities"][0]["status"] = "qualified"
reject("unresolved-dependency", v, "unresolved dependency")
v = copy.deepcopy(base)
v["intake"][1]["state"] = "resolved"
reject("false-registration-closure", v, "Full registration closure is not implemented")
print(
    json.dumps(
        {
            "passed": True,
            "scope": "Real CLI rejection of invalid metadata, not gameplay or synthetic completion evidence",
            "checks": checks,
        },
        indent=2,
    )
)
