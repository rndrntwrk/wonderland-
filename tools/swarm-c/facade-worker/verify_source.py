#!/usr/bin/env python3
"""Run the source-world worker and independently read its original FSOf bytes.

Usage: python3 verify_source.py PATH_TO_FACADE_WORKER NEW_OUTPUT_DIRECTORY
Uses only Python's standard library. The input is a declared synthetic source
world; the assertions cover topology/camera/container production, not licensed
content or MonoGame GPU pixel parity.
"""
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import zlib
from verify import decode_png, run


def read_fsof(path):
    encoded = path.read_bytes()
    assert len(encoded) <= 128 * 1024 * 1024
    assert encoded[:4] == b"FSOf"
    assert struct.unpack_from("<i", encoded, 4)[0] == 1
    assert encoded[8] in (0, 1)
    if encoded[8]:
        decoder = zlib.decompressobj(31)
        body = decoder.decompress(encoded[9:], 128 * 1024 * 1024 + 1)
        assert decoder.eof and not decoder.unused_data and not decoder.unconsumed_tail
        assert len(body) <= 128 * 1024 * 1024
    else:
        body = encoded[9:]
    offset = 0

    def read(fmt):
        nonlocal offset
        length = struct.calcsize(fmt)
        assert offset + length <= len(body)
        values = struct.unpack_from(fmt, body, offset)
        offset += length
        return values

    compression, fw, fh, ww, wh, night = read("<5iB")
    assert compression == 0, "worker emits original RGBA mode inside gzip"
    assert all(0 < value <= 4096 for value in (fw, fh, ww, wh))
    assert night in (0, 1)
    images = {}
    names = ["floor-day.png", "wall-day.png"]
    if night:
        names += ["floor-night.png", "wall-night.png"]
    for name in names:
        length, = read("<i")
        width, height = (fw, fh) if name.startswith("floor") else (ww, wh)
        assert length == width * height * 4 and offset + length <= len(body)
        images[name] = body[offset:offset + length]
        offset += length
    light = read("<4B") if night else None
    meshes = {}
    total_vertices, total_indices = 0, 0
    for name in ("floor", "wall"):
        count, = read("<i")
        total_vertices += count
        assert 0 <= count and total_vertices <= 2_000_000
        vertices = [read("<8f") for _ in range(count)]
        assert all(math.isfinite(value) for vertex in vertices for value in vertex)
        count, = read("<i")
        total_indices += count
        assert 0 <= count and count % 3 == 0 and total_indices <= 6_000_000
        indices = [read("<i")[0] for _ in range(count)]
        assert all(0 <= index < len(vertices) for index in indices)
        meshes[name] = (vertices, indices)
    assert offset == len(body), "original container has no trailing fields"
    return {"dimensions": [fw, fh, ww, wh], "night": bool(night), "light": light,
            "images": images, "meshes": meshes, "decoded_bytes": len(body)}


def verify_source(binary, directory):
    directory.mkdir()
    first, repeat = directory / "first", directory / "repeat"
    a = run(binary, ["source-fixture", first])
    b = run(binary, ["source-render", first / "source-request.wlcsrc", repeat])
    assert json.loads(a.stdout) == json.loads(b.stdout)
    files = sorted(path.name for path in first.iterdir())
    assert len(files) == 9 and files == sorted(path.name for path in repeat.iterdir())
    assert all((first / name).read_bytes() == (repeat / name).read_bytes() for name in files)
    metadata = json.loads((first / "metadata.json").read_text())
    fsof = read_fsof(first / "facade.fsof")
    assert fsof["dimensions"] == [384, 256, 512, 72]
    assert fsof["light"] == (60, 70, 110, 255)
    assert hashlib.sha256((first / "facade.fsof").read_bytes()).hexdigest() == metadata["fsof_sha256"]
    decoded = {}
    for image in metadata["images"]:
        path = first / image["file"]
        dimensions, rgba = decode_png(path)
        assert dimensions == (image["width"], image["height"])
        assert hashlib.sha256(path.read_bytes()).hexdigest() == image["png_sha256"]
        assert hashlib.sha256(rgba).hexdigest() == image["rgba_sha256"]
        assert any(rgba[3::4]), image["file"]
        if image["file"].startswith("thumbnail"):
            assert dimensions == (576, 576), "original TSO thumbnail size"
        else:
            assert rgba == fsof["images"][image["file"]], "container and PNG use the same produced pixels"
        decoded[image["file"]] = rgba
    for kind in ("floor", "wall", "thumbnail"):
        assert decoded[f"{kind}-day.png"] != decoded[f"{kind}-night.png"]
    floor_vertices, floor_indices = fsof["meshes"]["floor"]
    wall_vertices, wall_indices = fsof["meshes"]["wall"]
    assert len(wall_vertices) == 8 * 4 and len(wall_indices) == 8 * 6, "extracted exterior lines on two stories"
    assert len(floor_vertices) > 3 * 25 and len(floor_indices) > 3 * 96, "source subdivided floors, overlay and roof"
    assert min(vertex[0] for vertex in floor_vertices) == 6.5, "(77 - 64) / 2 source base position"
    assert max(vertex[0] for vertex in floor_vertices) == 70.5
    assert max(vertex[2] for vertex in floor_vertices) == 70.5
    assert any(abs(vertex[1] - 1.475) < 0.0001 for vertex in floor_vertices), "raised object overlay at half a 2.95-tile story"
    assert any(abs(vertex[1] - 5.9) < 0.0001 for vertex in floor_vertices), "source roof converted from graphics to tile units"
    valid = (first / "source-request.wlcsrc").read_bytes()
    assert valid[-5:] == bytes([1, 60, 70, 110, 255]), "bincode Option night field"
    day_request = directory / "day-source.wlcsrc"
    day_request.write_bytes(valid[:-5] + bytes([0]))
    day = directory / "day"
    run(binary, ["source-render", day_request, day])
    day_metadata = json.loads((day / "metadata.json").read_text())
    assert len(day_metadata["images"]) == 3
    assert [phase["phase"] for phase in day_metadata["lighting_passes"]] == ["day"]
    assert not read_fsof(day / "facade.fsof")["night"]
    assert not any(day.glob("*-night.png"))
    impossible = bytearray(valid)
    impossible[72:80] = struct.pack("<Q", 2**64 - 1)
    for name, data in (("bad-magic", b"incorrect"), ("truncated", valid[:-1]), ("trailing", valid + b"!"), ("impossible-count", impossible)):
        request, destination = directory / f"{name}.wlcsrc", directory / f"{name}-output"
        request.write_bytes(data)
        run(binary, ["source-render", request, destination], success=False)
        assert not destination.exists()
    oversized = directory / "oversized.wlcsrc"
    with oversized.open("wb") as stream:
        stream.truncate(64 * 1024 * 1024 + 9)
    run(binary, ["source-render", oversized, directory / "oversized-output"], success=False)
    assert not (directory / "oversized-output").exists()
    oversized.unlink()
    run(binary, ["source-fixture", first], success=False)
    assert json.loads((first / "metadata.json").read_text()) == metadata
    report = {
        "fixture": "synthetic source-world topology and materials; not licensed-content evidence",
        "algorithm_version": metadata["algorithm_version"],
        "images": 6, "thumbnail_dimensions": [576, 576], "repeat_files_identical": files,
        "source_request_and_original_fsof": True,
        "container_textures_equal_decoded_png": True,
        "source_ground_floor_overlay_roof_and_walls": True,
        "geometry": {name: {"vertices": len(vertices), "indices": len(indices)} for name, (vertices, indices) in fsof["meshes"].items()},
        "no_night_source_emits_three_images_and_no_night_container": True,
        "malformed_requests_rejected": 5, "existing_directory_preserved": True,
        "artifact_sha256": metadata["artifact_sha256"], "fsof_sha256": metadata["fsof_sha256"],
        "effective_input_sha256": metadata["effective_input_sha256"],
        "work_units": metadata["work_units"], "reservation_bytes": metadata["reservation_bytes"], "resident_bytes": metadata["resident_bytes"],
        "png_sha256": {image["file"]: image["png_sha256"] for image in metadata["images"]},
        "decoder": "independent Python struct/zlib FSOf and CRC-checked PNG decoders"
    }
    (directory / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, sort_keys=True))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    verify_source(Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve())
