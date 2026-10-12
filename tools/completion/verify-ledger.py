#!/usr/bin/env python3
"""Validate source intake and release evidence without converting inventory into parity."""

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path
import jsonschema

ROOT = Path(__file__).resolve().parents[2]
PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73"


def path(value):
    if Path(value).is_absolute() or ".." in Path(value).parts:
        raise ValueError("Repository-relative path required: " + value)
    candidate = (ROOT / value).resolve()
    if not candidate.is_relative_to(ROOT) or not candidate.is_file():
        raise ValueError("Missing or external path: " + value)
    return candidate


def load(value):
    return json.loads(path(value).read_text())


def expected_sources():
    domains = load("docs/compat/full-game-source-obligations.json")
    eods = load("fixtures/eod/registration-census.json")
    result = {"source." + e["id"]: set(e["source_paths"]) for e in domains["entries"]}
    for entry in eods["entries"]:
        paths = {
            entry["server"]["registration"]["path"],
            entry["server"]["source"]["path"],
        }
        if entry.get("ui"):
            paths.update(
                [entry["ui"]["registration"]["path"], entry["ui"]["source"]["path"]]
            )
        result["eod." + str(entry["id"])] = paths
    for line in (
        path("docs/design/action-first/player-capability-map.md")
        .read_text()
        .splitlines()
    ):
        if not re.match(r"\| \d+ \|", line):
            continue
        cells = line.split("|")
        paths = set()
        for anchor in re.findall(r"`([^`]+\.cs)`", cells[2]):
            candidate = (
                ROOT / anchor
                if anchor.startswith("TSOClient/")
                else ROOT / "TSOClient/tso.client" / anchor
            )
            if not candidate.is_file():
                candidates = list((ROOT / "TSOClient/tso.client").rglob(candidate.name))
                if len(candidates) != 1:
                    raise ValueError("Ambiguous player source anchor: " + anchor)
                candidate = candidates[0]
            paths.add(str(candidate.relative_to(ROOT)))
        result["player." + cells[1].strip().zfill(2)] = paths
    return result


def verify(args):
    data = load(args.ledger)
    jsonschema.Draft202012Validator(
        load("docs/compat/game-capabilities.schema.json")
    ).validate(data)
    errors = []
    if args.reference != PIN or data["reference_commit"] != args.reference:
        errors.append("Original reference mismatch")
    if args.implementation_commit is not None and not re.fullmatch(
        r"[0-9a-f]{40}", args.implementation_commit
    ):
        errors.append("Implementation commit must be a full lowercase 40-hex commit ID")
    caps = data["capabilities"]
    ids = [c["id"] for c in caps]
    if len(ids) != len(set(ids)):
        errors.append("Duplicate capability ID")
    expected = expected_sources()
    missing = set(expected) - set(ids)
    errors.extend("Omitted preservation member: " + id for id in sorted(missing))
    intake = {i["id"]: i for i in data["intake"]}
    if len(intake) != len(data["intake"]):
        errors.append("Duplicate intake ID")
    for required in ["intake.original-pin", "intake.registration-closure"]:
        if required not in intake:
            errors.append("Missing intake obligation: " + required)
    if intake.get("intake.registration-closure", {}).get("state") == "resolved":
        errors.append(
            "Full registration closure is not implemented in this initial ledger; expand the audited census before resolving it"
        )
    if intake.get("intake.original-pin", {}).get("state") == "resolved":
        check = subprocess.run(
            ["git", "cat-file", "-e", args.reference + "^{commit}"],
            cwd=ROOT,
            capture_output=True,
        )
        if check.returncode:
            errors.append("Original pin is unavailable despite resolved intake")
        else:
            proof = subprocess.run(
                [
                    "python3",
                    "tools/completion/verify-source-pin.py",
                    "--reference",
                    args.reference,
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=120,
            )
            if proof.returncode:
                errors.append(
                    "Original pinned source preservation failed: "
                    + proof.stdout[-1000:]
                )

    if set(ids) & set(intake):
        errors.append("Capability/intake ID collision")
    byid = {c["id"]: c for c in caps}
    visiting = set()
    visited = set()

    def visit(id):
        if id in visiting:
            errors.append("Capability dependency cycle: " + id)
            return
        if id in visited:
            return
        visiting.add(id)
        for dep in byid[id]["dependencies"]:
            if dep in byid:
                visit(dep)
        visiting.remove(id)
        visited.add(id)

    for id in byid:
        visit(id)
    for c in caps:
        id = c["id"]
        if not expected.get(id, set()).issubset(c["source_paths"]):
            errors.append(
                id + ": required original source anchors omitted or substituted"
            )
        if not {"intake.original-pin", "intake.registration-closure"}.issubset(
            c["dependencies"]
        ):
            errors.append(id + ": mandatory intake dependencies omitted")
        if c["source_pin"] != args.reference:
            errors.append(id + ": source pin mismatch")
        if set(c["source_paths"]) != set(c["source_sha256"]):
            errors.append(id + ": source hash coverage mismatch")
        for p in c["source_paths"]:
            try:
                if hashlib.sha256(path(p).read_bytes()).hexdigest() != c[
                    "source_sha256"
                ].get(p):
                    errors.append(id + ": preserved source drift: " + p)
            except ValueError as e:
                errors.append(id + ": " + str(e))
        for p in c["rust_paths"]:
            try:
                path(p)
            except ValueError as e:
                errors.append(id + ": " + str(e))
        for dep in c["dependencies"]:
            if dep not in intake and dep not in byid:
                errors.append(id + ": unknown dependency: " + dep)
        if c["status"] == "qualified":
            if not c["rust_paths"] or not c["evidence"]:
                errors.append(id + ": qualified without implementation/evidence")
            if not args.implementation_commit or not args.content_release:
                errors.append(id + ": exact implementation/content release required")
            for dep in c["dependencies"]:
                done = (
                    intake.get(dep, {}).get("state") == "resolved"
                    or byid.get(dep, {}).get("status") == "qualified"
                )
                if not done:
                    errors.append(id + ": unresolved dependency: " + dep)
            for p in c["source_paths"]:
                original = subprocess.run(
                    ["git", "show", args.reference + ":" + p],
                    cwd=ROOT,
                    capture_output=True,
                )
                if original.returncode or hashlib.sha256(
                    original.stdout
                ).hexdigest() != c["source_sha256"].get(p):
                    errors.append(id + ": original pinned blob not verified: " + p)
            for evidence in c["evidence"]:
                try:
                    record = load(evidence)
                    jsonschema.Draft202012Validator(
                        load("docs/compat/game-capability-evidence.schema.json")
                    ).validate(record)
                    if (
                        record.get("status") != "passed"
                        or record.get("original_reference") != args.reference
                        or record.get("implementation_commit")
                        != args.implementation_commit
                        or record.get("content_release") != args.content_release
                        or id not in record.get("capability_ids", [])
                    ):
                        errors.append(id + ": incompatible evidence: " + evidence)
                except (
                    ValueError,
                    TypeError,
                    AttributeError,
                    jsonschema.ValidationError,
                ) as e:
                    errors.append(id + ": invalid evidence: " + str(e))
    # Existing source census checks compare the actual IDToHandler registries and hashes.
    census = subprocess.run(
        ["python3", "tools/swarm-b/eod-census.py", "--check"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if census.returncode:
        errors.append(
            "Existing exact EOD source census failed: " + census.stderr[-1000:]
        )
    unresolved = [i["id"] for i in data["intake"] if i["state"] != "resolved"]
    incomplete = [c["id"] for c in caps if c["status"] != "qualified"]
    if args.mode == "release":
        errors.extend("Unresolved intake: " + id for id in unresolved)
        errors.extend("Unqualified capability: " + id for id in incomplete)
    return {
        "status": "failed" if errors else "passed",
        "mode": args.mode,
        "capabilities": len(caps),
        "declared_qualified": len(caps) - len(incomplete),
        "declared_intake_complete": not unresolved,
        "unresolved_intake": unresolved,
        "errors": errors,
        "qualification": "Inventory acceptance is structural/source preservation only, not complete ingestion or gameplay acceptance.",
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--reference", required=True)
    p.add_argument("--mode", choices=["inventory", "release"], required=True)
    p.add_argument("--ledger", default="docs/compat/game-capabilities.json")
    p.add_argument("--implementation-commit")
    p.add_argument("--content-release")
    try:
        result = verify(p.parse_args())
    except (
        ValueError,
        KeyError,
        TypeError,
        jsonschema.ValidationError,
        subprocess.TimeoutExpired,
    ) as e:
        result = {"status": "failed", "errors": [str(e)[:2000]]}
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["status"] == "passed" else 1)
