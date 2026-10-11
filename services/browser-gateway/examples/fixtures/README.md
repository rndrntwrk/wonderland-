# Test-only connected browser fixture

`test-only-synthetic-avatar-v38.fsov` is deterministic synthetic state for the
loopback `controlled_replay` example. It is not an observed game save, a deployed
server response, or continuous game simulation. Production startup does not use
this example or its fixtures.

The source-shaped version-38 snapshot contains an 8 × 8 lot at packed location
16777472 (city tile 256, 256), one avatar with object ID 7 and persistent ID 42 at lot tile (4, 4), an empty
queue, and eight distinct original motive values. The original protocol decoder
and example tests verify its structure. Unlike the parser's separate 2 × 2
golden fixture, its entity lies inside the lot, so it can exercise the browser
world projection without weakening position validation. The packed city location
is also inside the source city map's renderer diamond, so the same directory
entry can be selected through its actual terrain pin.

The outfit references are the original `mab000_leathers.oft` body
(`0x0000024a0000000d`) and `mah001_robin.oft` head (`0x000003a10000000d`), with Light
skin. No proprietary mesh, texture, skeleton, or outfit bytes are embedded. To
verify rendering, provide the matching original game files separately through
the existing Game content loader. No animation or carry layers are serialized;
this fixture therefore tests the first restored resource-skeleton pose.

The example additionally sends an original version-5 SimJoin for a synthetic
sender, Controlled Bob, followed by an original lot-chat command. These bytes
travel through the native source observer and gateway before reaching the
browser. Player chat sent back to the example receives no fabricated acceptance
or server echo. Refresh requests return the same synthetic world state with a
new monotonic transport tick.

Regenerate the payload and metadata:

```sh
python3 services/browser-gateway/examples/fixtures/generate_synthetic_avatar_fsov38.py
```

Verify the complete tick and incoming chat projection:

```sh
cargo test -p wonderland-browser-gateway --example controlled_replay --locked
```

Wire authority is the unchanged original FreeSO source at commit
`4c6b3e8f5835b228723caea3c9f683c62f244f73`: `VMMarshal`,
`VMContextMarshal`, `VMArchitectureMarshal`, `VMEntityMarshal`, `VMAvatarMarshal`,
`VMThreadMarshal`, `VMTSOLotState`, `VMNetTickList`, `VMStateSyncCmd`,
`VMNetSimJoinCmd`, and `VMNetChatCmd`. The adjacent metadata records dimensions,
identities, source references, needs, and the fixture hash.
