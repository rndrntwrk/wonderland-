#!/usr/bin/env python3
"""Run the pinned original FAR3 C# reader on the regression's authored bytes.

Requires Mono (mcs and mono). No installation assets or Rust reference emulator
are used. This is one focused reader check, not the complete W00 oracle harness.
"""
import pathlib
import shutil
import struct
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
BASE = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SOURCES = [f"TSOClient/tso.files/FAR3/{name}.cs" for name in
           ("FAR3Archive", "Far3Entry", "FAR3Exception", "Decompresser")]


def main():
    for program in ("mcs", "mono"):
        if not shutil.which(program):
            raise SystemExit(f"{program} is required for the original C# reader check")
    for source in SOURCES:
        pinned = subprocess.check_output(["git", "show", f"{BASE}:{source}"], cwd=ROOT)
        if (ROOT / source).read_bytes() != pinned:
            raise SystemExit(f"source differs from pinned reader: {source}")
    u32 = lambda n: struct.pack("<I", n)
    commands = b"\xe0abcd\xffefg"
    entry = b"\x01\x07\0\0\0" + u32(len(commands))
    entry += u32(len(commands)) + b"\x10\xfb\0\0\x07" + commands
    archive = b"FAR!byAZ" + u32(3) + u32(16 + len(entry)) + entry + u32(1)
    archive += u32(7) + len(entry).to_bytes(3, "little") + b"\x80"
    archive += u32(16) + bytes([1, 0, 2, 0])
    archive += u32(0x01020304) + u32(0xA1B2C3D4) + b"f3"
    assert len(archive) == 73
    with tempfile.TemporaryDirectory(prefix="wonderland-far3-oracle-") as directory:
        root = pathlib.Path(directory)
        fixture = root / "body-length.far"
        fixture.write_bytes(archive)
        harness = root / "Probe.cs"
        harness.write_text('''using System; using System.Text; using FSO.Files.FAR3;
class Probe { static void Main(string[] args) {
using (var archive = new FAR3Archive(args[0])) {
Console.Write(Encoding.ASCII.GetString(archive.GetEntry(archive.GetAllFAR3Entries()[0])));
} } }''')
        exe = root / "probe.exe"
        subprocess.run(["mcs", f"-out:{exe}", str(harness), *SOURCES], cwd=ROOT, check=True)
        output = subprocess.check_output(["mono", str(exe), str(fixture)], cwd=ROOT)
        if output != b"abcdefg":
            raise SystemExit(f"pinned C# reader mismatch: {output!r}")
        print("Pinned C# FAR3 reader: 73-byte authored vector -> abcdefg PASS")


if __name__ == "__main__":
    main()
