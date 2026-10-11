# Test-only source-wire fixtures

These are **synthetic golden wire fixtures**, not original game saves, assets or
observed live server responses. Production code does not reference or bundle them.
They reproduce the original FreeSO version38 `SerializeInto` field order at
`4c6b3e8f5835b228723caea3c9f683c62f244f73` and the `fsov(false)` test builder in
`../snapshot_wire.rs`. The corresponding test compares the checked-in bytes with
that independent builder byte for byte.

- `source-v38-object.fsov`: uncompressed `FSOv` header, version38, TSO body.
- `source-v38-object-compressed.fsov`: same body, explicit length and one GZIP member.
- `source-v38-state-sync.bin`: command12, compressed FSOv, no trace list. The source
  StateSync command has no ActorUID prefix.
- `source-v38-state-sync-tick.bin`: immediate=true, one tick42/seed99, one StateSync.

Body fields include a 2×2 one-story architecture with native signed heights
`[0,16,-16,32]`, grass `[1,2,3,4]`, four source wall records, floorIDs `[9,0,0,9]`,
roof16/pitch0.66, source resource-map strings; one object ObjectID7/PersistID11,
GUID0x12345678, owner42, position(16,32,level1), one corresponding thread and queued
Drink action UID5; one multitile group and source platform records. Platform
`VMTSOLotState.LotID=55` is the **packed location**, not a database lot ID.
The wire names/patterns/GUID are fixture values and do not imply installed content.

Source anchors: `Marshals/{VMMarshal,VMContextMarshal,VMArchitectureMarshal,
VMEntityMarshal,VMGameObjectMarshal,VMMultitileGroupMarshal}.cs`,
`Marshals/Threads/{VMThreadMarshal,VMQueuedActionMarshal}.cs`,
`Model/TSOPlatform/{VMTSOObjectState,VMTSOLotState,VMTSOSurroundingTerrain}.cs`,
`NetPlay/Model/{VMNetTickList,VMNetTick,VMNetCommand}.cs` and
`NetPlay/Model/Commands/VMStateSyncCmd.cs`.
