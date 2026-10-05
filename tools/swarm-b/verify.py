#!/usr/bin/env python3
"""Reproduce Swarm B gates without adopting or modifying the root workspace."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PACKAGES = [
    ("formats", "crates/legacy-formats/Cargo.toml", True),
    ("content", "crates/content-ir/Cargo.toml", True),
    ("manifest", "tools/swarm-b-check/manifest/Cargo.toml", False),
    ("interactions", "tools/swarm-b-check/interactions/Cargo.toml", True),
    ("eod", "crates/eod-runtime/Cargo.toml", False),
    ("creator", "tools/creator/Cargo.toml", False),
    ("cooker", "tools/asset-cooker/Cargo.toml", False),
]
SIM_REVISION = "8a0e251d19e222a0a6833d7408ca629f674e1729"
ORIGINAL_SOURCE_FILES = (
    "Other/tools/Iffinator/Iffinator/srcs.zip",
    "TSOClient/tso.files/Formats/IFF/Chunks/SPR2.cs",
    "TSOClient/tso.files/Formats/IFF/Chunks/SPR2FrameEncoder.cs",
    "TSOClient/tso.files/FAR3/FAR3Archive.cs",
    "TSOClient/tso.files/FAR3/Far3Entry.cs",
    "TSOClient/tso.files/FAR3/FAR3Exception.cs",
    "TSOClient/tso.files/FAR3/Decompresser.cs",
)


def rust_test_counts(text):
    counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)
    return sum(int(c[0]) for c in counts), sum(int(c[2]) for c in counts)


def source_digest():
    """Bind code, locks, fixtures, metadata and the actual source-oracle inputs."""
    digest = hashlib.sha256()
    files = []
    for prefix in ("crates", "tools", "tests", "fixtures", "docs/compat"):
        for directory, children, names in os.walk(ROOT / prefix):
            children[:] = sorted(c for c in children if c not in
                                 ("target", "__pycache__", ".pytest_cache", ".git"))
            for name in sorted(names):
                path = Path(directory) / name
                if path.suffix == ".pyc" or path.is_symlink():
                    continue
                files.append(path)
    # The source oracles read these working-tree inputs, whereas the ordinary
    # census reads immutable Git objects. Include original input additions,
    # deletions and edits so a later change cannot retain an earlier PASS.
    objects = ROOT / "TSOClient/FSO.Content.TSO/Content/Objects"
    files.extend(path for path in objects.glob("*.iff") if path.is_file())
    files.extend(ROOT / name for name in ORIGINAL_SOURCE_FILES if (ROOT / name).is_file())
    for path in sorted(files):
        name = path.relative_to(ROOT).as_posix().encode()
        data = path.read_bytes()
        digest.update(len(name).to_bytes(8, "little"))
        digest.update(name)
        digest.update(len(data).to_bytes(8, "little"))
        digest.update(data)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--logs-dir", type=Path)
    parser.add_argument("--cargo", default=os.environ.get("B_CARGO_BIN") or
                        shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo"))
    parser.add_argument("--with-parity", action="store_true",
                        help="Execute the authored native/WASI probe using Node")
    parser.add_argument("--with-source-oracle", action="store_true",
                        help="Run original FAR3, indexed IFF and sprite source comparisons (Mono/C++)")
    parser.add_argument("--with-runtime-bridge", action="store_true",
                        help="Test the pinned Swarm A bridge and execute actual native/WASI source replay")
    args = parser.parse_args()
    logs = args.logs_dir or Path(tempfile.mkdtemp(prefix="wonderland-b-verification-"))
    logs = logs.resolve()
    logs.mkdir(parents=True, exist_ok=True)
    results = []
    initial_digest = source_digest()
    env = os.environ.copy()
    env.update(CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_TEST_DEBUG="0", PYTHONDONTWRITEBYTECODE="1",
               RUSTUP_TOOLCHAIN="1.90.0")
    if os.path.isabs(args.cargo):
        env["PATH"] = str(Path(args.cargo).parent) + os.pathsep + env.get("PATH", "")
    cargo = [args.cargo]

    def run(label, command, run_env=None, expected_failure=None):
        log = logs / f"{len(results)+1:02d}-{label}.log"
        with log.open("w") as output:
            completed = subprocess.run(command, cwd=ROOT, env=run_env or env,
                                       stdout=output, stderr=subprocess.STDOUT)
        text = log.read_text(errors="replace")
        passed = (completed.returncode == 0) if expected_failure is None else (
            completed.returncode != 0 and expected_failure in text)
        passed_tests, ignored_tests = rust_test_counts(text)
        python_counts = re.findall(r"Ran (\d+) tests? in ", text)
        result = {"gate": label, "command": command, "status": "pass" if passed else "fail",
                  "exit_code": completed.returncode, "log": log.name,
                  "tests_passed": passed_tests, "tests_ignored": ignored_tests,
                  "python_tests_passed": sum(map(int, python_counts)) if passed else 0}
        if expected_failure:
            result["expected_rejection"] = expected_failure
        results.append(result)
        print(f"{label}: {result['status'].upper()}", flush=True)
        if not passed:
            print(text[-16000:], file=sys.stderr)
            raise RuntimeError(f"verification failed: {label}; log: {log}")
        return text

    try:
        version = run("toolchain", cargo + ["--version"])
        if not version.startswith("cargo 1.90.0 "):
            raise RuntimeError("verification requires Cargo/Rust toolchain 1.90.0")
        for name, manifest, browser_safe in PACKAGES:
            # A unique directory per package avoids shared incremental artifacts;
            # deleting only this runner-owned directory bounds disk consumption.
            with tempfile.TemporaryDirectory(prefix=f"wonderland-b-{name}-") as build:
                package_env = dict(env, CARGO_TARGET_DIR=build)
                run(f"{name}-tests", cargo + ["test", "--locked", "--manifest-path", manifest], package_env)
                run(f"{name}-fmt", cargo + ["fmt", "--manifest-path", manifest, "--", "--check"], package_env)
                run(f"{name}-clippy", cargo + ["clippy", "--locked", "--manifest-path", manifest,
                                              "--all-targets", "--no-deps", "--", "-D", "warnings"], package_env)
                if browser_safe:
                    run(f"{name}-wasm32", cargo + ["check", "--locked", "--manifest-path", manifest,
                                                  "--lib", "--target", "wasm32-unknown-unknown"], package_env)
                if name == "formats" and args.with_source_oracle:
                    oracle_logs = logs / "indexed-iff"
                    run("original-indexed-iff-reader", [sys.executable,
                        "tools/swarm-b/indexed-iff-oracle.py", "--cargo", args.cargo,
                        "--target-dir", build, "--logs-dir", str(oracle_logs)], package_env)
                    # This source oracle explicitly runs the two corpus tests
                    # omitted by the ordinary native suite. Count their real log.
                    counts = rust_test_counts((oracle_logs / "bounded-rust-corpus-edits.log").read_text())
                    results[-1].update(tests_passed=counts[0], tests_ignored=counts[1],
                                       details="indexed-iff/indexed-iff-oracle.json")
                    run("original-sprite-writer", [sys.executable,
                        "tools/swarm-b/sprite-oracle.py", "--output", str(logs / "sprite-oracle.json")], package_env)
                    run("original-sprite-reader", [sys.executable,
                        "tools/swarm-b/sprite-reader-oracle.py", "--cargo", args.cargo], package_env)
                if name == "eod":
                    run("eod-native-boundary", cargo + ["check", "--locked", "--manifest-path", manifest,
                                                       "--lib", "--target", "wasm32-unknown-unknown"],
                        package_env, expected_failure="authoritative private EOD state is native-only")
                if name == "content":
                    with tempfile.TemporaryDirectory(prefix="wonderland-b-census-") as output:
                        generated = Path(output) / "corpus.json"
                        run("pinned-content-census", cargo + ["run", "--locked", "--manifest-path", manifest,
                            "--bin", "content-census", "--", str(ROOT), str(generated)], package_env)
                        if generated.read_bytes() != (ROOT / "docs/compat/content-corpus.json").read_bytes():
                            raise RuntimeError("pinned content census differs from committed metadata")
                if name == "cooker":
                    with tempfile.TemporaryDirectory(prefix="wonderland-b-demo-") as output:
                        first, second = Path(output) / "first", Path(output) / "second"
                        prefix = cargo + ["run", "--locked", "--manifest-path", manifest, "--"]
                        run("cooker-demo", prefix + ["demo", str(first)], package_env)
                        run("cooker-rebuild", prefix + ["cook", str(first / "demo.json"), str(second), "--public"], package_env)
                        release = first / "release"
                        expected = hashlib.sha256((release / "manifest.json").read_bytes()).hexdigest()
                        run("cooker-verify", prefix + ["verify", str(second), "--manifest-sha256", expected], package_env)
                        for filename in ["manifest.json"] + sorted(p.name for p in release.glob("*.wlp")):
                            if (release / filename).read_bytes() != (second / filename).read_bytes():
                                raise RuntimeError(f"nondeterministic cooker output: {filename}")
        run("eod-census", [sys.executable, "tools/swarm-b/eod-census.py", "--check"])
        run("object-census", [sys.executable, "tools/swarm-b/object-census.py", "--check"])
        run("catalog-tests", [sys.executable, "-m", "unittest", "discover", "-s",
                              "tests/compat/catalog", "-p", "test_*.py"])
        if args.with_parity:
            with tempfile.TemporaryDirectory(prefix="wonderland-b-parity-") as build:
                parity_env = dict(env, CARGO=args.cargo)
                run("native-wasm-execution", [sys.executable, "tools/swarm-b-check/parity/check.py",
                                              "--target-dir", build], parity_env)
        if args.with_source_oracle:
            run("original-far3-reader", [sys.executable, "tools/swarm-b/far3-oracle.py"])
        if args.with_runtime_bridge:
            run("runtime-assembly-path-tests", [sys.executable, "-m", "unittest", "discover",
                "-s", "tests/integration/swarm_b_runtime", "-p", "test_*.py"])
            with tempfile.TemporaryDirectory(prefix="wonderland-b-runtime-") as workspace:
                assembly = Path(workspace) / "assembly"
                build = Path(workspace) / "target"
                runtime_env = dict(env, CARGO_TARGET_DIR=str(build))
                runner = [sys.executable, "tools/swarm-b/runtime-bridge.py", "--cargo",
                          args.cargo, str(assembly)]
                run("runtime-bridge-tests", runner + ["test", "--locked", "--offline"], runtime_env)
                run("runtime-bridge-fmt", runner + ["fmt", "--", "--check"], runtime_env)
                run("runtime-bridge-clippy", runner + ["clippy", "--locked", "--offline",
                    "--all-targets", "--no-deps", "--", "-D", "warnings"], runtime_env)
                run("runtime-bridge-wasm32", runner + ["check", "--locked", "--offline",
                    "--no-default-features", "--lib", "--target", "wasm32-unknown-unknown"], runtime_env)
                run("runtime-source-native-wasi", [sys.executable,
                    "tools/swarm-b/runtime-bridge-parity.py", "--assembly", str(assembly),
                    "--cargo", args.cargo, "--target-dir", str(build)], runtime_env)
        if source_digest() != initial_digest:
            raise RuntimeError("verification inputs changed during the run; rerun on stable sources")
        status = "pass"
    except (OSError, RuntimeError) as error:
        status = "fail"
        print(str(error), file=sys.stderr)
    summary = {"schema_version": 1, "status": status, "toolchain": "1.90.0",
               "source_baseline": "4c6b3e8f5835b228723caea3c9f683c62f244f73",
               "verification_inputs_sha256": initial_digest, "gates": results,
               "rust_tests_passed": sum(r["tests_passed"] for r in results),
               "rust_tests_ignored": sum(r["tests_ignored"] for r in results),
               "python_tests_passed": sum(r["python_tests_passed"] for r in results),
               "requested_optional_checks": {
                   "authored_native_wasi_parity": args.with_parity,
                   "original_source_comparisons": args.with_source_oracle,
                   "pinned_runtime_bridge": args.with_runtime_bridge,
               }}
    if args.with_runtime_bridge:
        summary["simulation_revision"] = SIM_REVISION
    (logs / "verification.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Verification {status.upper()}; logs: {logs}")
    return 0 if status == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
