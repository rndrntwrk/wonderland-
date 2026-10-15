#!/usr/bin/env python3
"""Qualify original-source prerequisites and privately cook the banner unchanged.

No downloads, replacement routines, gameplay metadata, source writes, public
release or runtime admission. Success means the bounded blocker was reproduced.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

from run_content_preflight import LIBS, Runner, encoded, read, require, sha

PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SOURCE = "TSOClient/FSO.Content.TSO/Content/Objects/fso_decor_2-in-1_Racing_Banner.iff"
DIGEST = "260a9db1ed017eacf2604beaf92dbf0cafae6c651b1f0296605eab2429503ab7"
PROBE = "tools/replay/native_playable_object_preflight.rs"


def run(root: Path, out: Path) -> None:
    require(not out.exists(), "Output must be new")
    require(not out.is_relative_to(root), "Output must remain outside source tree")
    out.mkdir()
    (out / "work").mkdir()
    (out / "evidence").mkdir()
    record = {"schema": 1, "verification_passed": False, "commands": [],
              "executables": {}, "playable_release_qualified": False,
              "private_cook_only": True, "source_baseline": PIN}
    try:
        original = read(root / SOURCE)
        require(sha(original) == DIGEST, "Banner source bytes differ from pin")
        pinned = subprocess.check_output(["git", "show", f"{PIN}:{SOURCE}"], cwd=root, timeout=15)
        require(original == pinned, "Banner does not match original Git object")
        record["banner_source"] = {"path": SOURCE, "sha256": DIGEST, "bytes": len(original)}
        record["checkout_head_at_start"] = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True, timeout=15).strip()
        record["probe_sha256"] = sha(read(root / PROBE))
        runner = Runner(root, out, record)
        runner.env["CARGO_BUILD_JOBS"] = "1"
        runner.run("rust-version", ["rustc", "-vV"])
        runner.run("format", ["rustfmt", "--edition", "2024", "--check", root / PROBE])
        # Match the workspace's already qualified default feature set rather
        # than instantiate a second, unnecessary sim-core feature build.
        build = runner.run("build-native", ["cargo", "build", "--locked", "--offline", "-j1",
            "--lib", "-p", "wonderland-content-runtime-bridge", "--message-format=json"], timeout=300)
        libraries = {}
        for line in build.read_text().splitlines():
            message = json.loads(line)
            if message.get("reason") == "compiler-artifact" and message["target"]["name"] in LIBS:
                name = message["target"]["name"]
                artifacts = [Path(p) for p in message["filenames"] if p.endswith(".rlib")]
                require(len(artifacts) == 1 and name not in libraries, "Ambiguous compiler artifact")
                libraries[name] = artifacts[0]
        require(set(libraries) == set(LIBS), "Incomplete compiler dependency output")
        tests = runner.compile("candidate-tests", PROBE, libraries, test=True)
        runner.run("candidate-tests-run", [tests])
        binary = runner.compile("candidate-probe", PROBE, libraries)
        first = runner.run("candidate-report", [binary])
        second = runner.run("candidate-report-repeat", [binary])
        require(read(first) == read(second), "Candidate report is not reproducible")
        document = json.loads(read(first))
        require(document["playable_release_qualified"] is False, "Unexpected qualification")
        runner.run("require-playable-denied", [binary, "--require-playable"], expected=1)

        # Stage exact existing bytes for the cooker's safe relative-path importer.
        # A release here is only an inspection archive: no resource is asserted
        # simulation-critical and no external effective tuning is invented.
        staged = out / "work" / "source"
        staged.mkdir()
        name = Path(SOURCE).name
        (staged / name).write_bytes(original)
        spec = {"schema_version": 1, "fixture_only": False, "source_baseline": PIN,
                "tuning_version": sha(b"private inspection archive; gameplay tuning unresolved"),
                "sources": [{"id": "racing-banner", "path": name, "format": "iff",
                             "source_name": name, "pack_group": "original-banner-inspection",
                             "provenance": {"origin": "original", "license": None,
                                            "redistribution": "unknown"},
                             "patches": [], "semiglobal": None, "global": None,
                             "tuning": {"otf": None, "otf_rewrite": None, "upgrades": [],
                                        "dynamic_private": [], "dynamic_semiglobal": []},
                             "resolver_variant": "", "locale_selection": {
                                 "requested": 0, "default_language": 1}}], "overrides": []}
        (staged / "private-inspection.json").write_bytes(encoded(spec))
        command = ["cargo", "run", "--locked", "--offline", "-j1", "-p",
                   "wonderland-asset-cooker", "--bin", "wonderland-asset-cooker", "--"]
        release = out / "work" / "private-inspection-release"
        runner.run("private-cook", command + ["cook", staged / "private-inspection.json", release], timeout=300)
        manifest_bytes = read(release / "manifest.json")
        manifest_digest = sha(manifest_bytes)
        manifest = json.loads(manifest_bytes)
        require(all(not r["simulation_critical"] for r in manifest["resources"].values()),
                "Inspection cook cannot assert gameplay dependency closure")
        require(all(r["provenance"]["redistribution"] == "unknown"
                    for r in manifest["resources"].values()), "Rights claim was invented")
        runner.run("private-cook-verify", command + ["verify", release, "--manifest-sha256", manifest_digest])
        runner.run("public-cook-denied", command + ["cook", staged / "private-inspection.json",
                   out / "work" / "forbidden-public-release", "--public"], expected=1)
        require(not (out / "work" / "forbidden-public-release").exists(), "Public output escaped refusal")
        record["private_inspection_cook"] = {"manifest_sha256": manifest_digest,
            "resources": len(manifest["resources"]), "packs": len(manifest["packs"]),
            "simulation_critical_resources": 0, "rights": "unknown",
            "path": str(release), "gameplay_tuning_resolved": False}
        record["candidate_report_sha256"] = sha(read(first))
        record["verification_passed"] = True
    finally:
        (out / "evidence" / "verification.json").write_bytes(encoded(record))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    run(Path(__file__).resolve().parents[2], args.output.resolve())
