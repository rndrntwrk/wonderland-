#!/usr/bin/env python3
"""Compare preserved files with the pinned original Git tree, without executing them."""

import argparse
import hashlib
import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PIN = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
# Fork build/runtime outputs require this metadata change; source/assets do not.
ALLOWED_CHANGED = {".gitignore"}


def verify(reference):
    if reference != PIN:
        raise ValueError("Unexpected original reference")
    commit = subprocess.check_output(
        ["git", "--no-replace-objects", "cat-file", "commit", reference], cwd=ROOT
    )
    if (
        hashlib.sha1(
            b"commit " + str(len(commit)).encode() + b"\0" + commit
        ).hexdigest()
        != reference
    ):
        raise ValueError("Original commit object digest mismatch")
    tree = subprocess.check_output(
        ["git", "--no-replace-objects", "rev-parse", reference + "^{tree}"],
        cwd=ROOT,
        text=True,
    ).strip()
    raw = subprocess.check_output(
        ["git", "--no-replace-objects", "ls-tree", "-rz", reference], cwd=ROOT
    )
    report = {
        "source_repository": "https://github.com/riperiperi/FreeSO",
        "source_commit": reference,
        "source_tree": tree,
        "matched_blobs": 0,
        "allowed_metadata_changes": [],
        "missing": [],
        "changed": [],
        "gitlinks": [],
    }
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        mode, kind, expected = metadata.decode().split()
        name = name.decode()
        p = ROOT / name
        if kind == "commit":
            report["gitlinks"].append(
                {
                    "path": name,
                    "commit": expected,
                    "qualification": "unresolved submodule obligation",
                }
            )
            continue
        if not p.exists() and not p.is_symlink():
            report["missing"].append(name)
            continue
        h = hashlib.sha1()
        if mode == "120000":
            if not p.is_symlink():
                report["changed"].append(
                    {"path": name, "reason": "expected symbolic link"}
                )
                continue
            data = os.readlink(p).encode()
            h.update(b"blob " + str(len(data)).encode() + b"\0" + data)
        else:
            if (
                p.is_symlink()
                or not p.resolve().is_relative_to(ROOT)
                or not p.is_file()
            ):
                report["changed"].append(
                    {"path": name, "reason": "unexpected external path or file type"}
                )
                continue
            h.update(b"blob " + str(p.stat().st_size).encode() + b"\0")
            with p.open("rb") as f:
                for data in iter(lambda: f.read(1024 * 1024), b""):
                    h.update(data)
        actual = h.hexdigest()
        if actual == expected:
            report["matched_blobs"] += 1
        else:
            row = {"path": name, "original_blob": expected, "current_blob": actual}
            report[
                "allowed_metadata_changes" if name in ALLOWED_CHANGED else "changed"
            ].append(row)
    report["status"] = (
        "passed" if not report["missing"] and not report["changed"] else "failed"
    )
    report["qualification"] = (
        "Original pinned blob preservation only; submodule intake and Rust gameplay parity remain separate."
    )
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True)
    try:
        result = verify(parser.parse_args().reference)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        result = {"status": "failed", "error": str(error)}
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["status"] == "passed" else 1)
