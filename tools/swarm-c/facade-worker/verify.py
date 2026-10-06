#!/usr/bin/env python3
"""Independently decode actual PNGs with Python/zlib and compare CLI reruns.

Usage: python3 verify.py PATH_TO_FACADE_WORKER NEW_OUTPUT_DIRECTORY
No third-party Python dependencies, network, provider assets, or graphics device.
"""
import binascii
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import zlib


def decode_png(path):
    data = path.read_bytes()
    assert len(data) <= 80 * 1024 * 1024, "encoded PNG budget"
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    offset, compressed, dimensions, seen_end = 8, bytearray(), None, False
    while offset < len(data):
        assert offset + 12 <= len(data)
        length = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        assert length <= 65_540 and offset + 12 + length <= len(data)
        payload = data[offset + 8:offset + 8 + length]
        crc = struct.unpack_from(">I", data, offset + 8 + length)[0]
        assert crc == binascii.crc32(kind + payload) & 0xffffffff
        offset += 12 + length
        if kind == b"IHDR":
            assert dimensions is None and length == 13
            width, height, depth, color, compression, filters, interlace = struct.unpack(">IIBBBBB", payload)
            assert 0 < width <= 4096 and 0 < height <= 4096 and width * height <= 16_777_216
            assert (depth, color, compression, filters, interlace) == (8, 6, 0, 0, 0)
            dimensions = (width, height)
        elif kind == b"IDAT":
            assert dimensions is not None and not seen_end
            compressed.extend(payload)
        elif kind == b"IEND":
            assert not payload and offset == len(data)
            seen_end = True
        else:
            raise AssertionError(f"unexpected PNG chunk {kind!r}")
    assert dimensions is not None and seen_end
    width, height = dimensions
    expected = height * (width * 4 + 1)
    decoder = zlib.decompressobj()
    raw = decoder.decompress(compressed, expected + 1)
    assert len(raw) == expected and decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail
    rgba = bytearray()
    for row in range(height):
        start = row * (width * 4 + 1)
        assert raw[start] == 0
        rgba.extend(raw[start + 1:start + 1 + width * 4])
    return dimensions, rgba


def run(binary, args, *, success=True):
    result = subprocess.run([str(binary), *map(str, args)], check=False, capture_output=True, text=True, timeout=30)
    assert (result.returncode == 0) == success, (result.returncode, result.stdout, result.stderr)
    if not success:
        assert result.returncode == 1 and result.stderr.startswith("facade-worker: "), (result.returncode, result.stderr)
    return result


def verify(binary, directory):
    directory.mkdir()
    first, repeat = directory / "first", directory / "repeat"
    a = run(binary, ["fixture", first])
    b = run(binary, ["render", first / "request.wlcdr", repeat])
    assert json.loads(a.stdout) == json.loads(b.stdout)
    files = sorted(path.name for path in first.iterdir())
    assert files == sorted(path.name for path in repeat.iterdir())
    assert len(files) == 8
    for name in files:
        assert (first / name).read_bytes() == (repeat / name).read_bytes(), name
    metadata = json.loads((first / "metadata.json").read_text())
    assert len(metadata["images"]) == 6
    decoded = {}
    for image in metadata["images"]:
        path = first / image["file"]
        dimensions, rgba = decode_png(path)
        assert dimensions == (image["width"], image["height"])
        assert hashlib.sha256(path.read_bytes()).hexdigest() == image["png_sha256"]
        assert hashlib.sha256(rgba).hexdigest() == image["rgba_sha256"]
        assert any(rgba[3::4]), image["file"]
        decoded[image["file"]] = rgba
    for kind in ("floor", "wall", "thumbnail"):
        assert decoded[f"{kind}-day.png"] != decoded[f"{kind}-night.png"]
    assert metadata["lighting_passes"][0]["time_of_day"] == 0.5
    assert metadata["lighting_passes"][1]["time_of_day"] == 0.0
    # Malformed envelopes, impossible frame count, and trailing bytes never
    # create an output artifact or a completion marker.
    valid = (first / "request.wlcdr").read_bytes()
    impossible_count = bytearray(valid)
    impossible_count[72:80] = struct.pack("<Q", 2**64 - 1)
    for name, data in (("bad-magic", b"incorrect"), ("truncated", valid[:-1]), ("trailing", valid + b"!"), ("impossible-count", impossible_count)):
        request = directory / f"{name}.wlcdr"
        request.write_bytes(data)
        destination = directory / f"{name}-output"
        run(binary, ["render", request, destination], success=False)
        assert not destination.exists()
    oversized = directory / "oversized.wlcdr"
    with oversized.open("wb") as stream:
        stream.truncate(64 * 1024 * 1024 + 9)
    run(binary, ["render", oversized, directory / "oversized-output"], success=False)
    assert not (directory / "oversized-output").exists()
    oversized.unlink()
    # Existing artifact sets cannot be silently replaced.
    run(binary, ["fixture", first], success=False)
    assert json.loads((first / "metadata.json").read_text()) == metadata
    report = {
        "fixture": "synthetic normalized geometry; not licensed-content evidence",
        "images": 6,
        "repeat_files_identical": files,
        "png_decoder": "Python standard-library zlib with CRC, RGBA, size and EOF checks",
        "malformed_requests_rejected": 5,
        "existing_directory_preserved": True,
        "artifact_sha256": metadata["artifact_sha256"],
        "effective_input_sha256": metadata["effective_input_sha256"],
        "work_units": metadata["work_units"],
        "reservation_bytes": metadata["reservation_bytes"],
        "resident_bytes": metadata["resident_bytes"],
        "png_sha256": {image["file"]: image["png_sha256"] for image in metadata["images"]},
    }
    (directory / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    verify(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
