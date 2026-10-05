#!/usr/bin/env python3
"""Assemble the exact Swarm A Rust subtree with live B-owned bridge sources.

No original source is edited, no fake provider is generated, and no repository
submodule is cloned. The sibling dependency matches the eventual integrated tree.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile

SIM_REVISION = "8a0e251d19e222a0a6833d7408ca629f674e1729"


def git(repo: Path, *args: str) -> bytes:
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True).stdout


def real_path(path: Path, *, directory: bool) -> None:
    """Preflight every existing component, without resolving away symlinks."""
    for parent in reversed(path.parents):
        try:
            mode = parent.lstat().st_mode
        except FileNotFoundError:
            continue
        if not stat.S_ISDIR(mode):
            raise ValueError(f"assembly parent must be a real directory: {parent}")
    try:
        metadata = path.lstat()
        mode = metadata.st_mode
    except FileNotFoundError:
        return
    if not (stat.S_ISDIR(mode) if directory else stat.S_ISREG(mode)):
        raise ValueError(f"assembly path must be a real {'directory' if directory else 'file'}: {path}")
    if not directory and metadata.st_nlink != 1:
        raise ValueError(f"assembly file must have a single owned link: {path}")


def write_owned(path: Path, data: bytes, *, create: bool = False) -> None:
    """The preflight owns the layout; refuse a changed leaf symlink as well."""
    flags = os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW
    flags |= os.O_EXCL if create else os.O_TRUNC
    with os.fdopen(os.open(path, flags, 0o644), "wb") as output:
        output.write(data)


def verify_export(destination: Path, files: dict[str, bytes]) -> None:
    root = destination / "crates/sim-core"
    expected_files = {destination / name for name in files}
    expected_dirs = {root}
    for path in expected_files:
        expected_dirs.update(parent for parent in path.parents if parent.is_relative_to(root))
    actual_files = set()
    actual_dirs = {root}
    for directory, dirs, names in os.walk(root, followlinks=False):
        for name in dirs:
            path = Path(directory) / name
            real_path(path, directory=True)
            actual_dirs.add(path)
        for name in names:
            path = Path(directory) / name
            real_path(path, directory=False)
            actual_files.add(path)
    if actual_files != expected_files or actual_dirs != expected_dirs:
        raise ValueError("exported simulation subtree has missing or unexpected paths")
    for name, data in files.items():
        if (destination / name).read_bytes() != data:
            raise ValueError(f"exported simulation source drifted: {name}")


def assemble(repo: Path, destination: Path) -> Path:
    repo = repo.resolve()
    destination = destination.absolute()
    if ".." in destination.parts or destination.is_relative_to(repo):
        raise ValueError("use a dedicated assembly directory outside the source repository")
    real_path(destination, directory=True)
    commit = git(repo, "rev-parse", f"{SIM_REVISION}^{{commit}}").decode().strip()
    if commit != SIM_REVISION:
        raise ValueError("simulation revision mismatch")
    archive = git(repo, "archive", SIM_REVISION, "crates/sim-core")
    files = {}
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not member.name.startswith("crates/sim-core/") or ".." in Path(member.name).parts:
                raise ValueError("unexpected archive entry")
            data = tar.extractfile(member).read()
            files[member.name] = data
    identity = {"sim_revision": SIM_REVISION, "source_repository": str(repo),
                "files": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
    # The bridge directory itself is real, so ../sim-core resolves in this
    # assembly. Its Rust sources and test corpus remain the actual B-owned files.
    bridge = destination / "crates/content-runtime-bridge"
    links = {
        bridge / "src": repo / "crates/content-runtime-bridge/src",
        destination / "crates/content-ir": repo / "crates/content-ir",
        destination / "crates/legacy-formats": repo / "crates/legacy-formats",
        destination / "tools/creator": repo / "tools/creator",
        destination / "tests/integration/swarm_b_runtime": repo / "tests/integration/swarm_b_runtime",
        destination / "TSOClient": repo / "TSOClient",
    }
    stamp = destination / "assembly.json"
    manifest = bridge / "Cargo.toml"
    lock = bridge / "Cargo.lock"
    # Validate the complete owned layout before mkdir, symlink, or file refresh.
    # Only these exact B-source link leaves may be symlinks.
    for path in [stamp, manifest, lock, *(destination / name for name in files)]:
        real_path(path, directory=False)
    for target, source in links.items():
        real_path(target.parent, directory=True)
        if target.is_symlink() and target.resolve() == source:
            continue
        if target.exists() or target.is_symlink():
            raise ValueError(f"refusing to overwrite assembly path: {target}")
    existing = destination.exists()
    if existing:
        if not stamp.is_file() or json.loads(stamp.read_text()) != identity:
            raise ValueError("existing destination is not this exact bridge assembly; use a new directory")
        verify_export(destination, files)

    # Read source manifests before the first mutation, too.
    manifest_bytes = (repo / "crates/content-runtime-bridge/Cargo.toml").read_bytes()
    source_lock = repo / "crates/content-runtime-bridge/Cargo.lock"
    lock_bytes = source_lock.read_bytes() if source_lock.is_file() else None
    if not existing:
        destination.mkdir(parents=True)
        for name, data in files.items():
            path = destination / name
            path.parent.mkdir(parents=True, exist_ok=True)
            write_owned(path, data, create=True)
        write_owned(stamp, (json.dumps(identity, sort_keys=True, indent=2) + "\n").encode(), create=True)
    bridge.mkdir(parents=True, exist_ok=True)
    for target, source in links.items():
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.is_symlink() and target.resolve() == source:
            continue
        if target.exists() or target.is_symlink():
            raise ValueError(f"refusing to overwrite assembly path: {target}")
        target.symlink_to(source, target_is_directory=True)
    write_owned(manifest, manifest_bytes)
    if lock_bytes is not None:
        write_owned(lock, lock_bytes)
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"), help="Cargo executable (before destination)")
    parser.add_argument("destination", type=Path, help="dedicated scratch assembly directory")
    parser.add_argument("cargo_args", nargs=argparse.REMAINDER, help="optional Cargo command, e.g. test --locked --offline")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    manifest = assemble(repo, args.destination)
    print(f"A revision {SIM_REVISION}; manifest {manifest}", flush=True)
    if not args.cargo_args:
        return 0
    cargo_args = args.cargo_args
    if cargo_args[0] == "--":
        cargo_args = cargo_args[1:]
    if not cargo_args:
        return 0
    # Limit build storage; never reuse another task's target directory.
    env = dict(os.environ)
    env.setdefault("CARGO_TARGET_DIR", str(manifest.parents[2] / "target"))
    env.update(CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    env.setdefault("CARGO_BUILD_JOBS", "2")
    env["PATH"] = str(Path(args.cargo).absolute().parent) + os.pathsep + env.get("PATH", "") if os.path.isabs(args.cargo) else env.get("PATH", "")
    command = [args.cargo, cargo_args[0], "--manifest-path", str(manifest), *cargo_args[1:]]
    return subprocess.run(command, cwd=repo, env=env).returncode


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
