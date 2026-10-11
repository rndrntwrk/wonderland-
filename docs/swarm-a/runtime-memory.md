# Runtime memory adapter

Source baseline: FreeSO `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
Source paths below are relative to `TSOClient/tso.simantics/`.
Implementation: `crates/sim-core/src/runtime_memory.rs`.
Regressions: `crates/sim-core/tests/runtime_memory_reference.rs`.

## Public API and ownership

The adapter implements:

```rust
read_memory(&SimState, &ContentSet, &MemoryAddress) -> Result<i16, VmFault>
read_memory_with_active(&SimState, &ContentSet, &MemoryAddress, Option<&VmThread>)
    -> Result<i16, VmFault>
write_memory(&mut SimState, &ContentSet, &MemoryAddress, i16)
    -> Result<MemoryWrite, VmFault>
write_memory_with_active(&mut SimState, &ContentSet, &MemoryAddress, i16, Option<&VmThread>)
    -> Result<MemoryWrite, VmFault>
sync_projection(&mut SimState, EntityRef) -> Result<bool, VmFault>
sync_projection_with_active(&mut SimState, EntityRef, Option<&VmThread>)
    -> Result<bool, VmFault>
is_in_use(&SimState, EntityRef, Option<&VmThread>) -> Result<bool, VmFault>
read_memory_with_threads(&SimState, &ContentSet, &MemoryAddress, ThreadView<'_>)
    -> Result<i16, VmFault>
write_memory_with_threads(&mut SimState, &ContentSet, &MemoryAddress, i16, ThreadView<'_>)
    -> Result<MemoryWrite, VmFault>
sync_projection_with_threads(&mut SimState, EntityRef, ThreadView<'_>)
    -> Result<bool, VmFault>
is_in_use_with_threads(&SimState, EntityRef, ThreadView<'_>) -> Result<bool, VmFault>
```

`vm/memory.rs` retains scope decoding, indirect indexes, current-thread
registers, local/parameter memory, TempXL, list operations, raw versus cached
stack-object identity, and source read/write order. The runtime host delegates
resolved addresses to this module. `with_active` accepts the current
instruction's thread view when the runtime has removed that thread from the
state map to dispatch it. The view must be refreshed before each instruction.
It supplies current frame callees, thread presence, and advertisement context;
it is not a second mutable copy of the executing VM registers.

Nested runtime dispatch can remove several real entity threads at once, and
source `EvaluateCheck` uses a temporary evaluation thread without replacing
`Entity.Thread.Stack`. The additive overlay API preserves that distinction:

```rust
pub struct ThreadView<'a> {
    pub current: Option<&'a VmThread>,
    pub entity_threads: Option<&'a BTreeMap<ObjectId, VmThread>>,
}
```

With `entity_threads: Some(...)`, entity-thread lookup prefers the overlay's
local-ID entry and then `SimState.threads`; the current evaluation stack never
stands in for a missing real entity-thread entry. A mismatched overlay generation
is an error, including when valid stored state exists. `current` still supplies
the executing advertisement/register context. The old single-active APIs wrap
`ThreadView::active` and keep their existing signatures. These are borrowed
views; individual memory reads do not clone a map or the simulation state.

The runtime maintains overlay lifetimes and source semantics: ordinary dispatch
and `RunInMyStack` expose their active entity stacks, while `EvaluateCheck`
retains the real entity-thread stacks. The immutable view cannot commit a Temp
write to an overlay bank. Such a write returns explicit `HostUnsupported`; the
runtime must intercept it and mutate its owned overlay. The VM still owns writes
to its current thread's temporaries, and ordinary stored other-thread Temp
writes remain in this adapter.

`MemoryWrite.written` is the **source setter's Boolean return value**. It does
not mean that a stored byte changed. Several source setters intentionally
return true without changing their raw slot. `MemorySignal` carries the
additional work that the runtime must apply:

| Signal | Meaning |
|---|---|
| `MoneyHeadline { entity, amount }` | Present the source money headline; no balance or transaction changes. |
| `QueueDirty { entity }` | Notify the queue owner that a source priority write dirtied its queue. |
| `OutfitRequest { entity, suit }` | Resolve the TS1 person suit using imported outfit content. |
| `ProjectionChanged { entity }` | The semantic or presentation projection changed; consumers read its updated state. |
| `ResetRequested { entity }` | Apply the source entity/thread/queue reset at the instruction boundary. |

Emitting a reset or outfit request is not evidence that its downstream content,
queue, EOD, or complete lifecycle behavior has been implemented. Those owners
must apply the request or expose an unsupported/failure result. Money signals
never mutate `budget_mirror` or produce spending authority.

## Entity fields

All entity addresses validate the local ID and exact live generation against
both `IdAllocator` and `EntityState.info.reference`. Dead, missing, or stale
entities cannot be read or mutated through this adapter.

| `EntityField` | Read | Write |
|---|---|---|
| `Attribute` | Existing signed short, or zero for an absent unsigned-16 index. | Source-style zero-filled growth, up to the explicit 4096-slot port bound. |
| `ObjectData` | Exactly 80 source slots, with computed exceptions below. | Store a short and apply the source special cases below. |
| `Temp` | Twenty-register other-thread bank, with the active view available for reads. | Validate index and ownership, then write the stored other thread. The VM owns writes to its removed current thread. |
| `Motive` | Avatar motive index 0–15. | Source clamp and overfill behavior using content motive limits. |
| `PersonData` | Avatar index 0–100, including computed avatar getters. | Delegate data/effects to `AvatarState`, preserving the original setter Boolean. |
| `Slot` | Occupant ID or zero; game objects tolerate missing indexes, avatars have exactly three bounded slots. | Explicit unsupported result, matching the source memory setter; use the slot operation API. |
| `Definition` | Preflattened script-visible OBJD variables from immutable content. | Returns false. |
| `MasterDefinition` | Explicit master vector, then master object definition when present, otherwise the object's own vector. | Returns false. |
| `DynamicSpriteFlag` | One of the two source 64-bit flag words, with C# masked shift indexing. | Set only for `value > 0`; retain the same aliasing. |
| `Function` | Action routine ID or zero for an empty in-range entry; bounds use imported `entry_point_count`. | Returns false. |
| `TypeAttribute` | TSO returns zero; TS1 returns `HostUnsupported` until neighborhood TATT is provided. | TSO returns true without mutation; TS1 returns `HostUnsupported`. |

Attribute behavior comes from `Entities/VMEntity.cs:77-88`: reads beyond the
current list return zero and writes extend the list. Scope decoding casts the
script index to `ushort`, so a negative source short can become a large
attribute index; that read still returns zero. The 4096-slot write ceiling is
an explicit bounded-state safety change. It agrees with the package's current
entity/content bounds and is tested at the boundary; it is not a claim that
the original list had this ceiling.

`Entities/VMEntity.cs:710-740` selects the second sprite word whenever the
unsigned index exceeds 63. Its 64-bit shift then masks the count to six bits.
The normalized index is:

```text
index < 64: index
otherwise: 64 + ((index - 64) & 63)
```

Consequently, 64, 128, and 192 refer to the same bit in the second word; 127,
191, and 65535 also alias. They never alias the first word. The state stores a
fixed 128-flag vector, and the adapter rejects malformed vector lengths.

### Computed ObjectData

`Model/VMStackObjectVariable.cs` defines all 80 indexes.
`Entities/VMEntity.cs:786-817` defines the computed reads; other indexes read
their stored signed short.

| Index | Variable | Runtime read behavior |
|---:|---|---|
| 1 | Direction | Canonical direction notch 0–7 from entity metadata. |
| 2, 26 | Container/parent ID | Live containing entity ID, or zero. |
| 3 | Container slot | Stored container slot narrowed to short, or -1 when uncontained. |
| 11 | Object ID | The actual allocated ID, independent of the raw ObjectData slot. |
| 25 | Lockout count | Stored count minus elapsed scheduler ticks, clamped at zero; a threadless entity returns the stored count. |
| 29 | Room | -5 at the exact out-of-world sentinel; otherwise the stored room short. |
| 41 | Current value | Shared multitile initial price, read through the group's base object and narrowed to short. |
| 62 | Use count | Number of distinct live avatars in the group's active-queue-prefix projection. |
| 67 | Slot count | Three for an avatar, or the game object's contained-slot count. |

The original direction getter rounds `RadianDirection` to a notch. This
package's `EntityInfo.direction` already stores that discrete result; importers
and route/animation consumers must perform the source conversion when they
produce the metadata. This adapter does not claim to reconstruct a continuous
angle from the notch.

Writes to computed ID, parent, count, slot, and similar indexes still update
their raw short, because source `SetValue` generally permits this. Subsequent
reads use the computed value. This separation is tested explicitly.

Special setters in `Entities/VMEntity.cs:819-867` are implemented as follows:

- Direction writes normalize the signed short to 0–7 and update both entity
  direction and world facing.
- Flags retain every raw bit. Zero extent, person intersection, and occupied
  state update the semantic projection. A change in an avatar's Burning bit
  emits `ResetRequested`, including a transition that clears the bit.
- Lockout writes record the current scheduler tick when a thread exists.
- Category writes update both its raw short and `EntityInfo.category`.
- Current-value writes update the shared price in every group member, while
  writing the raw slot only on the addressed entity, as in the source.
- Engine query 1 computes whether deletion is safe and stores the result in
  ObjectData 79. Other query values are stored unchanged.

The source repairs a future lockout timestamp as a side effect of a getter.
The adapter's immutable read API instead rejects that malformed state with
`InvalidContent`; it does not return a temporary normalized value that would
silently diverge once simulation time catches up. Valid writes cannot create
future timestamps. This is an explicit corruption-handling deviation.

### Use counts and deletion queries are different source predicates

`VMEntity.GetUsers` at `Entities/VMEntity.cs:1130-1164` reads each avatar queue
through `ActiveQueueBlock`, unions users over the target multitile group, and
skips the tile itself. The runtime state supplies
`EntityState.queued_users: BTreeSet<EntityRef>` as that **active prefix**
projection. It is not the entire future queue. Missing, stale, or non-avatar
entries cannot inflate the returned live-user count.

`VMEntity.IsInUse` at `:1172-1194` instead examines Occupied flags and avatar
frame **callees**. Engine query 1 also rejects an object contained by an
avatar. Its source call uses `IsInUse(context, true, true)`, but the recursive
multitile path calls the two-argument overload and drops `stackObjSafety`.
The adapter preserves that behavior: a stack-object-only reference does not
make this query fail. An executing thread's current frames are included through
the active view, even while that thread is absent from `SimState.threads`.

The accepted interaction-projection producer must maintain the active-prefix
contract; this module does not invent a queue engine or infer active actions
from arbitrary BHAV frames. World `in_use` projections should be refreshed
after frame changes and before build checks that consume them. An occupied-bit
write also affects every member of its multitile group; this adapter refreshes
the written member, and the runtime must refresh its siblings before consuming
their cached `WorldObject.in_use` values. Engine queries compute directly from
current group flags and frame callees.

## Avatar semantics

The adapter delegates motive/person data to the avatar module rather than
duplicating its source policies. Relevant source anchors are
`Entities/VMAvatar.cs:751-784`, `:842-946`, and `:964-978`.

Motive writes check the full unsigned index before narrowing to the avatar
API's `u8`, and clamp to `[-100, max(old_value, content_limit)]`. A threadless
avatar uses the source default limit of 100. This preserves allowed overfill.
The current thread view supplies thread presence for both motive writes and
computed person-data skill-lock reads.

`PersonWrite.written` in the avatar module describes mutation, while the
source setter reports success for several no-ops. The adapter explicitly
preserves success for skill-policy-blocked writes, ignored PD70 writes,
display-only PD74 writes, and writes to job fields when the job entry is
missing. An online job ID greater than 5 returns false atomically. Other
invalid data indexes return explicit faults.

Money headline PD1, priority PD33, and TS1 outfit PD8 produce the corresponding
signals. The -32768 money sentinel writes its raw value without emitting a
headline. PD74 updates the avatar display-flags state while leaving its raw
person-data slot unchanged. PD68 is the avatar's visual ghost/dead state; it
does not change the world's `GhostImage` build-preview flag.

## Globals, tuning, clocks, rooms, and advertisements

### Globals and clocks

`VM.cs:589-626` bounds globals by the actual 38-short state array. Reads of
globals 0, 1, 4, 5, 6, 7, and 8 return clock hours, day, time-of-day, minutes,
seconds, month, and year. Writes still update the raw global slot and do not
change the clock. Other indexes are ordinary stored shorts.

`Engine/VMMemory.cs:175-219` maps CityTime and GameTime to the game calendar,
including the source's constant-zero TimeOfDay. TSOStandardTime uses
`SimClock.standard_component` for the Gregorian UTC calendar. Its unknown
component indexes return zero, while unknown game-clock indexes fault. UTC
conversion/range policy belongs to `clock.rs`; memory does not read host time.

### Tuning and definitions supplied by content

`Engine/VMMemory.cs:364-420` first checks the **callee's** tuning replacement,
then selects the code-owner resource cache. The adapter uses
`EntityState.tuning_overrides[(table, key)]` for that first stage. The producer
must resolve upgrade and dynamic tuning precedence described by
`Entities/VMEntityTuning.cs`; the adapter does not synthesize missing upgrade
content.

Content tuning keys are `(resource_owner, table_id, key_id)`:

| Resource mode | Owner key |
|---|---|
| Private | Code-owner resource ID. |
| Semiglobal | `RoutineStore.semiglobal(code_owner)`, falling back to the code owner when it has no semiglobal. |
| Global | Zero. |

Scope decoding has already applied the source 4096/8192/256 table offsets and
the seven-bit key mask. A missing cache entry returns zero, as in the source;
a missing callee or invalid mode is a fault. Tuning and definition writes
return false.

`ObjectDefinition.definition` and `master_definition` contain preflattened
**script-visible** values, not untagged OBJD raw bytes. The importer must place
`(short)(Version % 0xFFFF)` at index 0, `(short)(Version >> 16)` at index 1,
and the original raw data at index 2 onward, matching
`Engine/VMMemory.cs:447-459`. Modulo 65535 at index 0 must not be replaced with
an ordinary low-16-bit mask. `entry_point_count` preserves the source OBJf
array length, including trailing empty entries; generated tables default to
33 slots. These remain provisional content interfaces owned for shared
adoption by B/F.

### Rooms

`Engine/VMMemory.cs:94-105` clamps the requested room ID, follows
`LightBaseRoom` for outside/floor/area, and uses the selected room for IsPool.
The semantic world supplies a room adjacency graph. The adapter chooses the
minimum room in the connected fence/floor-adjacency component as its light
base, maps one-based semantic levels to the source zero-based floor, and
counts a full non-diagonal tile once and each diagonal side once for Area.
This avoids treating the world's two half-cells per full tile as two source
area units. Pool reads include both pool and water floor categories, matching
source `VMRoomMap`'s `ExpectedTile > 65533` condition. Room zero is the source
default/dummy record, and ambient light remains the source constant 100.

These are memory views over the world's documented semantic room model;
they do not claim full original room-generation or renderer-lighting parity.

### Advertisement and external data boundaries

`Engine/VMMemory.cs:752-790` first reads current-thread advertisement
overrides, then the current action's TTAB motive entries. The active thread's
own override map takes precedence. `EntityState.active_advertisements` holds
the accepted active-action TTAB projection: `None` means no active action;
an absent `(kind, index)` entry returns zero. Kinds are source minimum, range,
and personality entries 0–2. The producer is responsible for changing this
projection as its active action changes.

TS1 neighborhood, neighbor, career, and type-wide TATT data remain
`HostUnsupported`. The placeholder per-entity `type_attributes` map cannot
correctly implement neighborhood/type-wide identity and is deliberately not
used to manufacture a successful result. Direct host advertisement writes
also fail explicitly; VM scope writes belong to the executing thread.

## Projection consistency and atomicity

Projection writes copy only the affected entity candidate, prepare all
fallible changes and revision increments, then call the world's atomic
`replace_object` before committing entity state. A missing projection,
`BuildLocked`, invalid geometry, or revision exhaustion cannot leave an
updated ObjectData short paired with old geometry. Group-price writes prepare
all member candidates before committing any member. Simple global/register
writes validate their complete address before mutation. These local
guarantees do not rely on the runtime's outer cloned-tick transaction.

The projection contains direction, position, placement/wall/height flags,
weight, exclusive-wall state, person intersection policy, zero extent,
multitile identity, frame/flag-derived in-use state, and the separate disabled
ForSale flag. Unknown raw placement bits remain stored in ObjectData and are
masked out only in the world's known-bit semantic projection. Opaque raw bits
do not cause a partially applied write.

Contained objects take their outer container's position and have zero extent;
cyclic or stale containment is rejected. The exact out-of-world sentinel is
`(-32768, -32768, 1)`. `sync_projection` refreshes room metadata after explicit
position changes; ordinary raw room writes retain their source readback until
such a position refresh. Callers must synchronize affected descendants too.

`GhostImage` preview state and obstacle avoidance motion are preserved.
Avatar PD68 is not preview ghost state; an object's FSODynamicFootprint bit is
not an avatar motion mode. The world module reads current object rectangles
rather than maintaining the source static-footprint cache, and documents that
geometry/routing replacement separately.

## Validation evidence

The targeted reference suite covers computed/raw memory separation, sprite
aliases, bounded attribute growth, world/revision/build-lock failure atomicity,
shared group price, lockout timing and active thread presence, complete global
bounds, tuning owner/override precedence, source avatar success/no-op behavior,
money/queue/outfit/reset signals, motive overfill, active queue versus frame
use predicates, advertisement precedence, room base-area semantics, explicit
function-table lengths, OOW/containment, stale generations, and unsupported
external state.

Run from the repository root:

```sh
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/target-memory cargo test --manifest-path crates/sim-core/Cargo.toml --offline --test runtime_memory_reference
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/target-memory cargo test --manifest-path crates/sim-core/Cargo.toml --offline --release --test runtime_memory_reference
```

| Profile | Result |
| --- | --- |
| Debug | 16 passed, 0 failed |
| Release, with `overflow-checks = true` | 16 passed, 0 failed |

These are source-anchored synthetic regressions, not a full original-runtime
or content-corpus differential oracle. Both runs completed without compiler
warnings. The fixtures include the runtime's optional headline state; headline
duration and reset consumption remain runtime responsibilities.
