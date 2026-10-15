#!/usr/bin/env python3
"""Lightweight, read-only source-provenance/dependency check. No compiler or VM.

Uses retained Rust-decoder census metadata, verifies selected actual files against
their SHA-256 and original Git blobs, and reports missing scope references. This
is not a new IFF codec, effective-content resolver or runtime qualification.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
PREFIX = "TSOClient/FSO.Content.TSO/Content/Objects/"
NAMES = (
    "fso_decor_2-in-1_Racing_Banner.iff",
    "fso_event_tally.iff",
    "fso-summer-candy-border.iff",
    "painting_s1_jackpotPoster.iff",
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def inspect(root: Path) -> dict:
    census_path = root / "docs/compat/content-corpus.json"
    require(census_path.stat().st_size <= 32 * 1024 * 1024, "Census byte bound")
    census_bytes = census_path.read_bytes()
    census = json.loads(census_bytes)
    require(census["source_baseline"] == PIN, "Wrong source baseline")
    files = {row["path"]: row for row in census["files"]}
    candidates = []
    for name in NAMES:
        path = PREFIX + name
        source = files[path]
        actual_path = root / path
        require(not actual_path.is_symlink() and actual_path.stat().st_size <= 8 * 1024 * 1024,
                "Unsafe or oversized source")
        actual = actual_path.read_bytes()
        require(digest(actual) == source["source_sha256"], "Source differs from retained census")
        original = subprocess.check_output(["git", "show", f"{PIN}:{path}"], cwd=root, timeout=15)
        require(actual == original, "Source differs from original Git blob")
        refs = {ref["id"] for obj in source["objects"] for ref in obj["bhav_refs"]}
        for table in source["interactions"]:
            for entry in table["entries"]:
                refs.update([entry["action_function"], entry["test_function"]])
        for routine in source["behaviors"]:
            refs.update(routine["direct_calls"])
        refs.discard(0)
        globals_missing = sorted(value for value in refs if value < 4096)
        semiglobals_missing = sorted(value for value in refs if value >= 8192)
        locals_present = {routine["chunk_id"] for routine in source["behaviors"]}
        private_missing = sorted(value for value in refs if 4096 <= value < 8192 and value not in locals_present)
        candidates.append({
            "path": path, "source_sha256": digest(actual), "source_git_blob": source["source_git_blob"],
            "source_bytes": len(actual), "source_matches_pin": True,
            "object_definitions": source["objects"], "interaction_tables": source["interactions"],
            "behavior_summaries": source["behaviors"], "original_chunk_counts": source["resource_counts"],
            "global_bhav_required": globals_missing, "semiglobal_bhav_required": semiglobals_missing,
            "named_semiglobals": [g["name"] for g in source["globals"]],
            "missing_private_bhav": private_missing,
            "zero_instruction_local_routines": sorted(r["chunk_id"] for r in source["behaviors"]
                                                       if r["instruction_count"] == 0),
            "whole_object_runtime_qualified": False,
        })
    require(candidates[0]["global_bhav_required"] == [271, 280], "Banner dependency expectation changed")
    require(candidates[0]["missing_private_bhav"] == [], "Banner private closure changed")
    require(candidates[0]["named_semiglobals"] == [], "Banner scope changed")
    require(candidates[1]["global_bhav_required"] == [271], "Tally dependency expectation changed")
    require(candidates[3]["global_bhav_required"] == [265], "Poster global expectation changed")
    require(candidates[3]["semiglobal_bhav_required"] == [8199, 8201, 8203], "Poster semiglobal expectation changed")
    # Evidence from retained census, not an assertion about unprovided installations.
    providers = {str(id): [f["path"] for f in census["files"]
                 if any(b["chunk_id"] == id for b in f.get("behaviors", []))] for id in [271, 280]}
    require(providers == {"271": [], "280": []}, "A retained base provider needs review")
    patches = [{"path": f["path"], "source_sha256": f["source_sha256"],
                "targets": p["source_name"]}
               for f in census["files"] for p in f.get("piffs", [])
               if p["source_name"].lower() in ["global.iff", "personglobals.iff", "templateperson.iff"]]
    return {"schema": 1, "source_baseline": PIN, "census_sha256": digest(census_bytes),
            "scope": "selected-original-file-hash-verification-and-retained-decoder-metadata",
            "selected_source_hash_checks_passed": True, "candidate_count": len(candidates),
            "preferred_candidate": NAMES[0], "preferred_guid": "6378e9e6",
            "smallest_immediate_base_input": "objectdata/globals/global.iff",
            "transitive_closure_known": False, "playable_release_qualified": False,
            "runtime_executed": False, "original_files_modified": 0,
            "retained_census_global_providers": providers, "available_matching_patches_unapplied": patches,
            "candidates": candidates}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    require(not args.output.exists(), "Output already exists")
    report = inspect(Path(__file__).resolve().parents[2])
    args.output.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print("4 exact source files verified; banner blocked on authentic global BHAV271/280; runtime unqualified")
