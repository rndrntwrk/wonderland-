#!/usr/bin/env python3
"""Exercise real source-format authoring, guards, and atomic CLI publication."""
import hashlib
import json
import pathlib
import struct
import subprocess
import sys
import tempfile

BIN = pathlib.Path(sys.argv[1]).resolve()


def digest(data):
    return hashlib.sha256(data).hexdigest()


with tempfile.TemporaryDirectory(prefix="wonderland-creator-interchange-") as directory:
    root = pathlib.Path(directory)

    def call(*args, success=True):
        result = subprocess.run([str(BIN), "--root", directory, *args], capture_output=True)
        assert (result.returncode == 0) == success, (args, result.returncode, result.stdout, result.stderr)
        if not success:
            assert result.returncode == 2
        return result.stdout

    # Literal source OBJ grammar: a custom texture, shared corners, UV reversal.
    obj = ("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\n"
           "vn 0 0 1\no 0_TEX_7\nf 1/1/1 2/2/1 3/3/1\n").encode()
    (root / "mesh.obj").write_bytes(obj)
    call("mesh-from-obj", "mesh.obj", "mesh.fsom", "authored")
    original = (root / "mesh.fsom").read_bytes()
    info = json.loads(call("asset-inspect", "fsom", "mesh.fsom"))
    assert info["data"]["groups"][0][0]["pixel_sprite"] == 7
    call("mesh-obj-export", "mesh.fsom", "export.obj")
    call("mesh-mtl-export", "mesh.fsom", "export.mtl")
    assert b"map_Kd 0_TEX_7.png" in (root / "export.mtl").read_bytes()
    call("mesh-obj-import", "mesh.fsom", "noop.fsom", digest(original), "export.obj")
    assert (root / "noop.fsom").read_bytes() == original
    call("mesh-gltf-export", "mesh.fsom", "mesh.glb", "glb")
    call("mesh-gltf-import", "mesh.fsom", "noop-glb.fsom", digest(original).upper(), "mesh.glb")
    assert (root / "noop-glb.fsom").read_bytes() == original
    glb = bytearray((root / "mesh.glb").read_bytes())
    json_length = struct.unpack_from("<I", glb, 12)[0]
    struct.pack_into("<f", glb, 28 + json_length, 4.0)
    (root / "edit.glb").write_bytes(glb)
    call("mesh-gltf-import", "mesh.fsom", "changed.fsom", digest(original), "edit.glb")
    changed = json.loads(call("asset-inspect", "fsom", "changed.fsom"))
    assert changed["data"]["groups"][0][0]["vertices"][0]["position"][0] == 0x40800000
    (root / "protected.fsom").write_bytes(b"keep-existing")
    call("mesh-gltf-import", "mesh.fsom", "protected.fsom", "0" * 64, "edit.glb", success=False)
    assert (root / "protected.fsom").read_bytes() == b"keep-existing"
    (root / "bad.glb").write_bytes(glb[:-1])
    call("mesh-gltf-import", "mesh.fsom", "protected.fsom", digest(original), "bad.glb", success=False)
    assert (root / "protected.fsom").read_bytes() == b"keep-existing"

    upgrade = b'{"Version":2,"Files":[{"Name":"x.iff","Subs":[],"Groups":[],"Upgrades":[],"Config":[],"future":17}]}'
    (root / "upgrades.json").write_bytes(upgrade)
    (root / "edits.json").write_text(json.dumps([{"path": ["Files", "0", "future"], "remove": False, "value": 99}]))
    call("upgrades-edit", "upgrades.json", "changed.json", digest(upgrade), "edits.json")
    assert json.loads((root / "changed.json").read_bytes())["Files"][0]["future"] == 99
    (root / "edits.json").write_text(json.dumps([{"path": ["Files", "0", "Name"], "remove": True, "value": None}]))
    call("upgrades-edit", "upgrades.json", "changed.json", digest(upgrade), "edits.json", success=False)
    assert json.loads((root / "changed.json").read_bytes())["Files"][0]["future"] == 99

    neighborhood = b'[{"GUID":"one","Name":"First","Location":{"X":1,"Y":1}},{"GUID":"two","Name":"Second","Location":{"X":3,"Y":1},"DistanceMul":100}]'
    (root / "hoods.json").write_bytes(neighborhood)
    assert json.loads(call("neighborhood-nearest", "hoods.json", "2", "1")) == 0

    # Baseline indexed BMP input converts to lossless PNG pixel channels.
    bmp = bytearray(14 + 40 + 8 + 4)
    bmp[:2] = b"BM"
    struct.pack_into("<I", bmp, 2, len(bmp))
    struct.pack_into("<I", bmp, 10, 62)
    struct.pack_into("<IiiHHII", bmp, 14, 40, 2, 1, 1, 8, 0, 4)
    struct.pack_into("<I", bmp, 46, 2)
    bmp[54:62] = bytes([3, 2, 1, 0, 6, 5, 4, 0])
    bmp[62:66] = bytes([0, 1, 0, 0])
    (root / "city.bmp").write_bytes(bmp)
    call("city-convert", "city.bmp", "city.png", "png")
    inspected = json.loads(call("city-image-inspect", "city.png"))
    assert (inspected["width"], inspected["height"], inspected["first_rgba"]) == (2, 1, [1, 2, 3, 255])
    assert not list(root.glob("*.tmp"))
    print("source authoring/interchange CLI: OBJ, MTL, FSOm, GLB, JSON, indexed BMP/PNG and failed publication verified")
