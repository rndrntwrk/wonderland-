#!/usr/bin/env python3
"""TEST ONLY: deterministic synthetic FSOv38 avatar refresh payload.

This writes invented test state, not a save from a running game. No proprietary
asset bytes are copied. The packed references below were verified against the
user-supplied original-avatar-small-pack SOURCE_MANIFEST.json and OFT payloads.

Wire authority: wonderland-ui/crates/vm-protocol/tests/snapshot_wire.rs and
wonderland-ui/crates/vm-protocol/src/snapshot.rs, FSOv version 38 (TSO only).
The payload has an FSOv header; native VM update/tick wrapping belongs to the
test harness. It describes the first restored resource-skeleton pose, with no
animation, carry, marker replay, or simulated gameplay.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import struct


OUT_DIR = Path(__file__).resolve().parent
NAME = "TEST ONLY - Synthetic Avatar FSOv38"
OBJECT_ID = 7
PERSIST_ID = 42
LOT_LOCATION = (256 << 16) | 256  # source map tile (256, 256); controlled_replay.rs
GUID = 0x7FD96B54
BODY = 0x0000024A0000000D  # mab000_leathers.oft, male adult
HEAD = 0x000003A10000000D  # mah001_robin.oft, male adult
WIDTH = HEIGHT = 8
STORIES = 1


class Wire:
    def __init__(self):
        self.data = bytearray()

    def put(self, fmt, value):
        self.data.extend(struct.pack("<" + fmt, value))

    def u8(self, value):
        self.put("B", value)

    def i8(self, value):
        self.put("b", value)

    def i16(self, value):
        self.put("h", value)

    def u16(self, value):
        self.put("H", value)

    def i32(self, value):
        self.put("i", value)

    def u32(self, value):
        self.put("I", value)

    def i64(self, value):
        self.put("q", value)

    def u64(self, value):
        self.put("Q", value)

    def f32(self, value):
        self.put("f", value)

    def f64(self, value):
        self.put("d", value)

    def boolean(self, value):
        self.u8(int(value))

    def text(self, value):
        encoded = value.encode("utf-8")
        count = len(encoded)
        while count >= 128:
            self.u8((count & 127) | 128)
            count >>= 7
        self.u8(count)
        self.data.extend(encoded)

    def shorts(self, values):
        self.i32(len(values))
        for value in values:
            self.i16(value)

    def position(self, x, y, level):
        self.i16(x)
        self.i16(y)
        self.i8(level)


def build():
    w = Wire()
    w.data.extend(b"FSOv")
    w.i32(38)
    w.boolean(False)  # uncompressed
    w.boolean(False)  # TSO, not TS1

    # Context.Clock, serialized fields in source order.
    w.i64(123)
    for value in [0, 30, 5, 12, 1, 6, 1997, 20000]:
        w.i32(value)
    w.i64(999)

    # Architecture: 8 x 8, one story, zero corner heights/walls/floors.
    for value in [WIDTH, HEIGHT, STORIES]:
        w.i32(value)
    w.u8(0)  # terrain light
    w.u8(0)  # terrain dark
    cells = WIDTH * HEIGHT
    w.i32(cells * 2)  # terrain height BYTE count
    w.data.extend(bytes(cells * 2))
    w.i32(cells)  # grass BYTE count
    w.data.extend(bytes([255]) * cells)
    w.data.extend(bytes(cells * STORIES * 13))  # 13-byte wall cells
    w.data.extend(bytes(cells * STORIES * 2))  # UInt16 floor cells
    w.boolean(False)  # walls dirty
    w.boolean(False)  # floors dirty
    w.u32(16)  # source fixture roof style (no roofs exist here)
    w.f32(0.66)
    w.boolean(False)  # resource ID map absent
    w.boolean(False)  # fine buildable absent
    w.boolean(True)
    w.u64(0)  # context ambience bits
    w.u64(7)  # synthetic RNG state, never advanced

    # Exactly one VMAvatar entity and its Avatar platform state.
    w.i32(1)
    w.u8(1)
    w.i16(OBJECT_ID)
    w.u32(PERSIST_ID)
    w.u32(1000)  # budget
    w.u8(0)  # permissions
    w.i32(0)  # ignored avatars
    w.i32(0)  # jobs
    w.u32(0)  # platform flags
    w.data.extend(bytes([255, 255, 255]))
    w.i8(0)  # chat pitch
    w.u8(0)  # chat channel
    object_data = [0] * 80
    object_data[11] = OBJECT_ID
    object_data[34] = 0  # Hidden is independent of dynamic sprite masks
    w.shorts(object_data)
    w.shorts([])  # MyList
    w.boolean(False)  # headline absent
    w.u32(GUID)
    w.u32(0)  # master GUID
    w.i16(0)  # main parameter
    w.i16(0)  # main stack object
    w.shorts([0, 0, 0])  # empty hand/head/pelvis slots
    w.i16(0)  # no container
    w.i16(-1)  # no container slot
    w.shorts([])  # attributes
    w.i32(0)  # object relationships
    w.i32(0)  # persistent relationships
    w.u64(0)
    w.u64(0)
    w.position(64, 64, 1)  # (4, 4) tiles, strictly inside 8 x 8
    w.u32(0)  # timestamp lockout
    w.u32(0xFFFFFFFF)  # light color

    # VMAvatar extension: no layers, no carry, no message.
    w.i32(0)
    w.boolean(False)
    w.text("")
    w.i32(0)
    w.i32(16)  # source has sixteen motive changes
    for motive in range(16):
        w.i16(0)  # per hour
        w.i16(100)  # maximum
        w.u8(motive)
        w.f64(0.0)
    w.i32(5)  # motive decay last minute
    for _ in range(7):
        w.i16(0)
    person_data = [0] * 101
    person_data[58] = 30  # adult age
    person_data[63] = 100  # FSO scale 1.0
    person_data[65] = 0  # male
    w.shorts(person_data)
    motives = [0] * 16
    named_motives = {
        "hunger": (7, 80), "comfort": (6, 60), "hygiene": (8, 40),
        "bladder": (9, 20), "energy": (5, 0), "fun": (15, -20),
        "social": (14, -40), "room": (13, -60),
    }
    for index, value in named_motives.values():
        motives[index] = value
    w.shorts(motives)
    w.i16(0)  # old hand object
    w.f32(0.0)  # source yaw, mesh transform uses pi - yaw
    w.i32(-1)  # kill timeout disabled
    for _ in range(3):
        w.u64(BODY)  # default day/swim/sleep suits
    for _ in range(4):
        w.u64(BODY)  # dynamic day/swim/sleep/costume suits
    for _ in range(4):
        w.u64(0)  # head/back/shoes/tail decorations absent
    w.i32(0)  # bound appearances
    w.u64(BODY)
    w.u64(HEAD)
    w.u8(0)  # Light skin

    # Exactly one corresponding empty v38 VMThread (71 bytes).
    w.i32(1)
    thread_start = len(w.data)
    w.i32(0)  # stack
    w.i32(0)  # queue
    w.i8(-1)  # no active queue block
    w.data.extend(bytes(40))  # twenty Int16 temp registers
    w.data.extend(bytes(8))  # two Int32 temp XL registers
    w.u8(0)  # last exit
    w.boolean(False)  # no blocking state
    w.boolean(False)  # no EOD connection
    w.boolean(False)  # interrupt
    w.u16(0)  # action UID
    w.i32(0)  # dialog cooldown
    w.u32(0)  # schedule idle start
    assert len(w.data) - thread_start == 71

    # A source-shaped singleton group supplies an explicitly synthetic name.
    w.i32(1)
    w.boolean(False)
    w.text("TEST ONLY - Synthetic Avatar 42")
    w.i32(0)
    w.i32(-1)
    w.shorts([OBJECT_ID])
    w.position(0, 0, 0)
    w.shorts([0] * 38)  # inert synthetic global state

    # VMTSOLotState. lot_id stores the packed city location, not database ID 1.
    w.text(NAME)
    w.u32(LOT_LOCATION)
    w.data.extend(bytes(36 + 9 + 16))
    w.u8(7)  # residential fixture category
    w.i32(1)
    w.u32(PERSIST_ID)  # owner
    w.i16(1)
    w.u32(PERSIST_ID)  # roommate
    w.i16(0)  # build roommates
    w.boolean(False)  # no job UI
    w.u8(0)  # skill mode
    w.u8(0)  # chat channel count
    w.u32(99)  # controlled neighborhood
    w.i16(OBJECT_ID + 1)
    w.boolean(False)  # no tuning
    return bytes(w.data), named_motives


def main():
    payload, motives = build()
    target = OUT_DIR / "test-only-synthetic-avatar-v38.fsov"
    target.write_bytes(payload)
    metadata = {
        "test_only": True,
        "synthetic_state": True,
        "proprietary_asset_bytes_in_payload": False,
        "label": NAME,
        "file": target.name,
        "bytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "version": 38,
        "compressed": False,
        "semantics": "First restored zero-time source resource-skeleton pose",
        "lot_location": LOT_LOCATION,
        "object_id": OBJECT_ID,
        "persist_id": PERSIST_ID,
        "guid": f"0x{GUID:08x}",
        "dimensions": [WIDTH, HEIGHT, STORIES],
        "position": [64, 64, 1],
        "yaw": 0.0,
        "scale": 1.0,
        "skin": "Light",
        "body": {"key": f"0x{BODY:016x}", "name": "mab000_leathers.oft"},
        "head": {"key": f"0x{HEAD:016x}", "name": "mah001_robin.oft"},
        "clock_ticks": 123,
        "animation_count": 0,
        "carry": False,
        "bound_appearance_count": 0,
        "decorations": [0, 0, 0, 0],
        "motives": {name: {"source_index": index, "raw": raw, "percent": (raw + 100) / 2}
                    for name, (index, raw) in motives.items()},
    }
    (OUT_DIR / "test-only-synthetic-avatar-v38.metadata.json").write_text(
        json.dumps(metadata, indent=2, sort_keys=True) + "\n")
    print(json.dumps(metadata, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
