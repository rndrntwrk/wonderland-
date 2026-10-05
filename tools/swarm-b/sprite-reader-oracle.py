#!/usr/bin/env python3
"""Read Rust-authored SPR2 boundary vectors with unchanged source C# methods.

The probe extracts four pinned methods and supplies CPU-only data and I/O
containers. No rendering or resource lookup implementation is claimed here.
"""
import argparse
import hashlib
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BASE = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SOURCE = "TSOClient/tso.files/Formats/IFF/Chunks/SPR2.cs"
METHODS_SHA256 = "0d5a24fe4449de91f4413dba38208d5ee3a134ed614beb10bdbb4a0ae97479ba"


def method(source, signature):
    start = source.index(signature)
    brace = source.index("{", start)
    depth, end = 1, brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default=os.environ.get("CARGO") or shutil.which("cargo"))
    args = parser.parse_args()
    if not args.cargo:
        raise SystemExit("Cargo/Rust 1.90 is required")
    for program in ("mcs", "mono"):
        if not shutil.which(program):
            raise SystemExit(f"{program} is required")
    pinned = subprocess.check_output(["git", "show", f"{BASE}:{SOURCE}"], cwd=ROOT)
    if (ROOT / SOURCE).read_bytes() != pinned:
        raise SystemExit("SPR2.cs differs from the pinned source")
    source = pinned.decode("utf-8-sig")
    signatures = [
        "public override void Read(IffFile iff, Stream stream)",
        "public void Read(uint version, IoBuffer io, uint guessedSize)",
        "public void ReadDeferred(uint version, IoBuffer io)",
        "private void Decode(IoBuffer io)",
    ]
    methods = [method(source, signature) for signature in signatures]
    digest = hashlib.sha256("\n".join(methods).encode()).hexdigest()
    if digest != METHODS_SHA256:
        raise SystemExit("extracted method identity mismatch")
    harness = runpy.run_path(str(ROOT / "tools/swarm-b/sprite-reader-harness.py"),
                             init_globals={"methods": methods})["source"]
    with tempfile.TemporaryDirectory(prefix="wonderland-sprite-reader-") as directory:
        temp = Path(directory)
        vectors = temp / "vectors.bin"
        env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0",
                   CARGO_PROFILE_TEST_DEBUG="0")
        env.setdefault("CARGO_TARGET_DIR", str(temp / "target"))
        subprocess.run([args.cargo, "run", "--quiet", "--locked", "--manifest-path",
                        str(ROOT / "crates/legacy-formats/Cargo.toml"), "--example",
                        "sprite-source-vectors", "--", str(vectors)],
                       cwd=ROOT, env=env, check=True, timeout=180)
        cs = temp / "Reader.cs"
        cs.write_text(harness, encoding="utf-8")
        exe = temp / "reader.exe"
        subprocess.run(["mcs", f"-out:{exe}", str(cs)], check=True, timeout=30)
        output = subprocess.check_output(["mono", str(exe), str(vectors)],
                                         text=True, timeout=30).strip()
        expected = "PASS: 18 cases, 34 frames, 294034 pixels decoded with source SPR2 reader methods"
        if output != expected:
            raise SystemExit(f"unexpected reader result: {output}")
        print(output)
        print(f"Extracted source methods SHA-256: {digest}")


if __name__ == "__main__":
    main()
