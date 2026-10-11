# FreeSO VM wire and v38 snapshot decoding

Source authority: original FreeSO revision
`4c6b3e8f5835b228723caea3c9f683c62f244f73`, retained under `TSOClient`.
This crate is a DOM-free, portable, bounded reader. It contains no invented game
content, fixtures, network destination policy, credential handling or simulation.

## Entry points

```rust
pub fn decode_tick_list(bytes: &[u8], limits: &DecodeLimits) -> Result<TickList>;
pub fn decode_direct_command(bytes: &[u8], limits: &DecodeLimits) -> Result<Command>;
pub fn decode_snapshot(bytes: &[u8], limits: &DecodeLimits) -> Result<Snapshot>;
```

Every entry point requires exact complete input. A successful command records its
wire `offset` and canonical `consumed` byte count. A failed tick list returns no
commands and performs no state mutation. EOD/StateSync byte patterns inside other
payloads have no special meaning. **VM commands have no per-command lengths.**
Every known layout is consumed field by field; unsupported commands, nested
variants and source versions abort the whole input without a guessed skip.

`VMNetTickList` is Boolean immediate mode, Int32 tick count, then ordered ticks.
Each `VMNetTick` is UInt32 tick ID, UInt64 random seed and Int32 command count.
`VMNetCommand` begins with its source byte enum (0–48). Most command bodies begin
with ActorUID; BlueprintRestore, StateSync and RequestResync omit that base field.
The reader validates every source command layout, including variable inventory,
architecture, tuning, persisted-avatar, adjacency and EOD payloads.

Focused command DTOs are `CommandBody::EodMessage(EodMessage)` and
`CommandBody::StateSync { snapshot, traces }`. Other source command bodies are
retained as `SourceFields { bytes }` after exact structural validation. These bytes
include the original actor prefix when the source class emits one; they exclude
the command enum byte. Parsing these records does not execute their effects.

EOD fields preserve ActorUID, PluginID, EventName and either .NET UTF-8 text or
UInt16-length binary data. Authentication, selected-avatar matching, tick replay,
plugin entry/leave incarnation and authority belong to the gateway session.
`eod_enter`/`eod_leave` are exact source event names, never scan markers. Gateway
code should update EOD permission only after an entire direct command/tick list
has decoded and the addressed actor matches the authenticated lot session.

## FSOv 38 source snapshot

`FSOv` magic, Int32 version 38 and Boolean compression precede the body. Compressed
snapshots have an explicit Int32 compressed length and one CRC-checked GZIP member.
Decoded bodies must consume completely, and GZIP trailing bytes/members are rejected.
All nested snapshots in one tick list share decompression, count and string budgets.
The reader rejects TS1 platform state and other FSOv versions explicitly.

The full version-38 TSO body is decoded in original order:

- TSO/TS1 flag; source clock; architecture, native Width×Height terrain heights and
  grass, per-story wall/floor records, roof, resource remap, fine build area and
  build/buy enable; ambience bits and source random seed.
- Ordered entities: source ObjectID/PersistID, complete object/person data and
  attributes, headline, GUID/master GUID, containers, relationships, dynamic
  masks, LotTilePos, lockout/light and platform ownership/permissions/budget.
- Object direction/disable; avatar animation/event state, carry animation,
  message, motive-change/decay/person/motive records, yaw, default/dynamic suits,
  decoration, bound appearances and exact original 64-bit outfit references.
- Corresponding threads: normal/routing/direct-control frames; nullable locals
  and args; source queued actions, callbacks, UID/result; temp/XL/exit state;
  all four async-state families; EOD thread state and bounded source EOD events;
  interruption, dialog cooldown and scheduler records.
- Multitile groups/prices/offsets, global state, original TSO lot platform records,
  surrounding terrain/roads/heights, roommates/build roommates, job UI, chat
  channels, neighborhood, next object ID and dynamic tuning.

`Snapshot.source_body` preserves the complete consumed uncompressed body; it is
omitted from normal JSON to avoid duplicating a large binary record. Routing and
async extensions retain validated source bytes even where no presentation needs
their individual fields. All UInt64 identities/masks/outfit keys/seeds and Int64
clock values serialize as decimal strings, preserving exact browser identities.

`Snapshot.semantics` is **RefreshOnly**. An original FSOv body cannot be handed to
Swarm A's unrelated snapshot reader. The original interpreter frames, RNG,
scheduler, queues and source quirks must be restored compatibly before applying
subsequent VM commands can establish deterministic live synchronization. Current
consumers can render/project the decoded source snapshot and route exact EOD
messages; successful decoding does not establish full game replica parity.

Original `VMTSOLotState.LotID` is a packed location in the lot container, not an
account Home owner, database lot ID or session incarnation. `ObjectID` is a signed
source instance number, distinct from content GUID and PersistID. Normalized world
code must supply its own explicit provenance and presentation ownership and retain
those distinctions.

## Bounds and compatibility

Limits bound input bytes, aggregate decompression, counts, aggregate entries,
string bytes/aggregate strings, ticks, commands, entities and architecture tiles.
Defaults are resource budgets that operators may configure; they are not game
feature/capacity limits. Counts are checked before allocation, arithmetic spans
are checked, UTF-8 and canonical BinaryWriter string lengths are validated, and
nonfinite projection floats are rejected.

Collection readers also check that the declared number of records can fit in the
remaining input using the smallest source wire representation, including empty
and nullable variants. This prevents a short malformed packet from reserving a
large collection before discovering truncation. Byte-counted collections use
the same general `max_count` budget as their Int16/Int32 counterparts. The bounds
tests observe actual allocator requests while still delegating to the normal
system allocator; they verify early rejection, not just an eventual error.

Source quirks handled explicitly:

- `VMActionCallbackMarshal.SerializeInto` writes Interaction as Int16, while the
  old source Deserialize reads Byte. This reader follows actual emitted wire and
  preserves Int16, including values above 255; it does not claim source interpreter
  restoration through this divergent reader.
- A null BlueprintRestore emits only zero length but its source reader expects
  further fields. This ambiguous variant is rejected rather than consuming the
  following command as metadata.
- Environment lists above the source reader's hard count 40 and EOD object-event
  data above its hard count 4 are rejected, avoiding original clipped-reader
  misalignment. Complete emitted data is never guessed or scanned past.
- SimJoin marker `0xFFEE`, persisted-avatar version 5 and DynamicTuning version 0
  are supported. Other versions reject transactionally.

## Source anchors

`NetPlay/Model/{VMNetTickList,VMNetTick,VMNetCommand,VMNetCommandBodyAbstract}.cs`,
`NetPlay/Model/Commands/*.cs`, `NetPlay/Model/VMNetAvatarPersistState.cs`,
`Marshals/{VMMarshal,VMContextMarshal,VMArchitectureMarshal,VMResourceIDMarshal,
VMEntityMarshal,VMGameObjectMarshal,VMAvatarMarshal,VMAnimationStateMarshal,
VMRuntimeHeadlineMarshal,VMMultitileGroupMarshal}.cs`, `Marshals/Threads/*.cs`,
`Model/VMArchitectureTerrain.cs`, `Model/TSOPlatform/*.cs`,
`Model/{VMOutfitReference,VMMotiveChange,VMInventoryItem,VMBudget,VMRoomInfo}.cs`,
`Entities/{VMAvatar,VMAvatarMotiveDecay}.cs`, `NetPlay/EODs/Model/*.cs`,
`Engine/Debug/VMSyncTrace.cs`, original `tso.world/Model/LotTilePos.cs` and
`tso.common/Model/DynamicTuning.cs`.
