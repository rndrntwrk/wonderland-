#!/usr/bin/env python3
"""Compare authored SPR2 command vectors with the unchanged source encoder.

Only the Color/frame containers and byte-writing plumbing are shimmed. The
entire original SPR2FrameEncoder.cs is compiled unchanged. This verifies the
command stream and all nontrivial alpha inputs, not rendering or the complete
C# resource system. Requires Mono (mcs and mono).
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import struct
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
BASE = "4c6b3e8f5835b228723caea3c9f683c62f244f73"
SOURCE = "TSOClient/tso.files/Formats/IFF/Chunks/SPR2FrameEncoder.cs"

HARNESS = r"""
using System;
using System.IO;
using FSO.Files.Formats.IFF.Chunks;
using FSO.Files.Utils;
using Microsoft.Xna.Framework;
namespace Microsoft.Xna.Framework {
    public struct Color { public byte A; public Color(byte a) { A = a; } }
}
namespace FSO.Files.Utils {
    public enum ByteOrder { LITTLE_ENDIAN }
    public class IoWriter {
        private Stream stream;
        public static IoWriter FromStream(Stream s, ByteOrder order) {
            return new IoWriter { stream = s };
        }
        public void WriteUInt16(ushort n) { stream.WriteByte((byte)n); stream.WriteByte((byte)(n >> 8)); }
        public void WriteBytes(byte[] bytes) { stream.Write(bytes, 0, bytes.Length); }
    }
}
namespace FSO.Files.Formats.IFF.Chunks {
    public class SPR2Frame {
        public int Width, Height;
        public byte[] PalData, ZBufferData;
        public Color[] PixelData;
    }
}
class Probe {
    static void Emit(int width, int height, byte[] pixels) {
        int n = pixels.Length / 3;
        var f = new SPR2Frame { Width = width, Height = height,
            PalData = new byte[n], ZBufferData = new byte[n], PixelData = new Color[n] };
        for (int i = 0; i < n; i++) {
            f.PalData[i] = pixels[i * 3];
            f.PixelData[i] = new Color(pixels[i * 3 + 1]);
            f.ZBufferData[i] = pixels[i * 3 + 2];
        }
        var stream = new MemoryStream();
        SPR2FrameEncoder.WriteFrame(f, IoWriter.FromStream(stream, ByteOrder.LITTLE_ENDIAN));
        Console.WriteLine(BitConverter.ToString(stream.ToArray()).Replace("-", "").ToLowerInvariant());
    }
    static void Main() {
        Emit(4, 2, new byte[] { 1,255,0, 2,123,19, 1,255,9, 0,0,255,
            0,0,255, 0,0,255, 0,0,255, 0,0,255 });
        Emit(1, 2, new byte[] { 0,0,255, 0,0,255 });
        Emit(3, 1, new byte[] { 1,255,0, 2,255,0, 1,255,0 });
        for (int a = 1; a <= 254; a++) Emit(1, 1, new byte[] { 1, (byte)a, 20 });
    }
}
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=pathlib.Path, help="optional JSON evidence output")
    args = parser.parse_args()
    for program in ("mcs", "mono"):
        if not shutil.which(program):
            raise SystemExit(f"{program} is required for the original C# encoder check")
    pinned = subprocess.check_output(["git", "show", f"{BASE}:{SOURCE}"], cwd=ROOT)
    if (ROOT / SOURCE).read_bytes() != pinned:
        raise SystemExit("SPR2FrameEncoder.cs differs from the pinned source")
    expected = [
        "120001c00100014013020f00012009010160018000a0",
        "028000a0",
        "080003c00102010000a0",
    ]
    for alpha in range(1, 255):
        quantized = (alpha * 31 + 254) // 255
        expected.append((struct.pack("<HH", 8, 0x4001)
                         + bytes([20, 1, quantized, 0]) + b"\x00\xa0").hex())
    with tempfile.TemporaryDirectory(prefix="wonderland-sprite-oracle-") as directory:
        temp = pathlib.Path(directory)
        harness = temp / "Probe.cs"
        harness.write_text(HARNESS, encoding="utf-8")
        exe = temp / "probe.exe"
        subprocess.run(["mcs", f"-out:{exe}", str(harness), str(ROOT / SOURCE)], check=True)
        actual = subprocess.check_output(["mono", str(exe)], text=True).splitlines()
    if len(actual) != len(expected):
        raise SystemExit(f"source emitted {len(actual)} vectors; expected {len(expected)}")
    for index, (got, want) in enumerate(zip(actual, expected)):
        if got != want:
            raise SystemExit(f"source vector {index} differs: expected={want}, actual={got}")
    report = {
        "source_commit": BASE,
        "source_path": SOURCE,
        "source_sha256": hashlib.sha256(pinned).hexdigest(),
        "vectors": len(expected),
        "alpha_values": 254,
        "command_stream_sha256": hashlib.sha256(bytes.fromhex("".join(actual))).hexdigest(),
        "result": "pass",
        "scope": "unchanged source SPR2FrameEncoder commands; authored inputs; container and renderer not executed",
    }
    if args.output:
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    main()
