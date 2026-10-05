# Simulation snapshot contract

## Scope and capture boundary

`crates/sim-core/src/snapshot.rs` encodes and restores the replicated
`SimState` after completed tick **N**. The accepted command tail begins at
**N+1**. The snapshot contains the simulation's clock/RNG, allocator generations,
entities, VM frames, scheduler, world/route state, avatar state, relationship
matrices, and pending/terminal effect records. The runtime's authority/replica
role is outside `SimState` and is not restored from the payload.

This is a new simulation-local format, provisionally versioned until Swarm F
adopts or adapts the shared contract. It is not an importer for the legacy lot
save format, and round-trip tests do not establish full source/content parity.
Interaction queue ownership remains with its integrating subsystem; this module
serializes and checks the fields currently represented by `SimState`.

The source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
The following local source anchors inform the boundary:

| Source anchor | Observed behavior and relevance |
| --- | --- |
| `TSOClient/tso.simantics/VM.cs`, `Save` | Saves entities and parallel thread records, multitile groups, context, global state, object-ID state, mode, and tuning. A positions-only snapshot cannot retain running VM state. |
| `TSOClient/tso.simantics/Marshals/Threads/VMThreadMarshal.cs`, `SerializeInto` | Saves stack-frame variants, queue state, 20 temporary registers, two extended temporary registers, blocking state, and scheduling state. The new format preserves its own VM/route/effect continuations and does not claim to reproduce every legacy field. |
| `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetAsyncResponseCmd.cs`, `Verify` and `Execute` | Accepted async responses require the expected blocking-state type. The new effect boundary adds stable operation IDs, generations, and explicit tick/epoch checks. |
| `TSOClient/tso.simantics/Primitives/VMGenericTSOCall.cs`, `ClearRelationships` | Clearing relationship matrices can leave reverse/dirty bookkeeping populated. Snapshot validation permits these historical supersets while checking their identities and bounds. |
| `TSOClient/FSO.Server.Database/DA/Avatars/SqlAvatars.cs`, `UpdateAvatarLotSave` | Lot-save updates exclude budget. Restored public VM mirrors do not authorize writes to balances or committed ownership. |

The supplied rewrite plan's shared `SimSnapshot` contract specifies the
post-completed-tick boundary. Section 9 additionally requires coordinated
snapshot/journal/outbox positions, epoch fencing, and recovery by stable logical
operation ID. This module supplies the bounded state representation and semantic
validation needed by that coordination; storage and transport adapters own the
coordination itself.

## Public API

```rust
pub fn encode(
    state: &SimState,
    content: &ContentSet,
) -> Result<Vec<u8>, SnapshotError>;

pub fn decode(
    bytes: &[u8],
    content: &ContentSet,
    expectation: SnapshotExpectation,
) -> Result<SimState, SnapshotError>;

pub fn validate_state(
    state: &SimState,
    content: &ContentSet,
) -> Result<(), SnapshotError>;

pub struct SnapshotExpectation {
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub limits: SnapshotLimits,
}

pub struct SnapshotLimits {
    pub max_payload_bytes: u64,
    pub max_entities: u32,
    pub max_continuations: u32,
}
```

`SnapshotExpectation::new(lot_id, authority_epoch)` selects the default limits:
64 MiB of payload, 32,767 entities, and 32,767 runtime continuations. These are
also hard ceilings; callers may lower them. Every limit and the expected lot and
epoch must be nonzero. `SnapshotExpectation::validate()` checks these settings
before decoding reads or allocates payload state. The state's own
`SimulationLimits` are checked separately and can impose lower count limits.

`encode` and `validate_state` borrow their inputs immutably. `decode` returns a
new candidate only after all checks succeed. An error cannot partially change
the caller's live state. The caller installs the successful candidate as one
runtime transition; it must not restore individual fields before validation
finishes.

## Wire format

All integer fields in the header use fixed-width little-endian encoding.

| Byte offset | Width | Field |
| --- | --- | --- |
| 0 | 8 | Magic bytes `WLDSNAP\0` |
| 8 | 2 | Snapshot format version, currently 1 |
| 10 | 2 | Simulation schema, currently 1 |
| 12 | 8 | Lot ID |
| 20 | 8 | Authority epoch |
| 28 | 8 | Completed tick N |
| 36 | 32 | Immutable content hash |
| 68 | 32 | Tuning hash |
| 100 | 8 | Payload length in bytes |
| 108 | Payload length | Canonical serialized `SimState` |
| 108 + payload length | 32 | SHA-256 of the complete header followed by payload |

The fixed header is 108 bytes; the checksum is 32 bytes. The accepted total
length is exactly `108 + payload_length + 32`. The payload uses bincode 1.3.3
with fixed integer encoding, little-endian order, an explicit byte limit, and
trailing-byte rejection. Its repeated schema, lot, epoch, tick, and content
descriptor must agree with the header.

The content descriptor is calculated from the caller-supplied immutable
`ContentSet`. A snapshot contains dynamic state and content/tuning hashes; the
caller supplies the actual compatible routines, object definitions, animation
metadata, routing slots, and other normalized content. Embedded runtime copies
of animation metadata and motive tuning are also compared against that content.
Changing a header hash alone cannot substitute different embedded metadata.

SHA-256 provides **integrity checking, not authentication**. Anyone able to
replace a payload can calculate a new checksum. The transport must establish
the snapshot's authorized source and the expected lot/epoch, and must protect
the coordinated capture boundary and accepted tail.

## Decode order and bounds

The decoder applies the checks in this order:

1. Validate caller expectations and limits, read the fixed header, reject
   unsupported versions, and verify the declared payload limit and exact frame
   length. A huge declared size cannot cause a payload allocation.
2. Check the expected lot/epoch and SHA-256 over header plus payload.
3. Validate the supplied content and compare its content/tuning descriptor.
4. Deserialize through bincode's byte limit and a guarded Serde traversal.
   The guard limits nesting to 128 levels and each generic collection/string
   hint to 1,048,576 entries/bytes before forwarding it to an allocating
   visitor. Module-specific limits are usually much smaller.
5. Reserialize the candidate with the canonical options and require byte-for-
   byte equality with the original payload. Duplicate map/set entries and
   noncanonical ordering cannot disappear silently during deserialization.
6. Compare repeated header fields, enforce both caller and stored count limits,
   and validate the complete simulation graph.

The nesting guard matters because a malicious recursive route payload could
otherwise overflow the native stack before its route-depth validator runs.
The collection guard matters because a forged size hint can request excessive
capacity before bincode reaches the missing entries. These guards complement
the outer 64 MiB byte ceiling and semantic validators; they are not a promise
that decoded in-memory state occupies only 64 MiB.

## Semantic state checks

### Completed boundary and identity

The clock must validate, produce a representable source-compatible UTC value,
and report exactly completed tick N. The scheduler must be between ticks with
no executing cursor or deferred deletion work, and scheduled entities must be
live generations. Tick zero has no last accepted-tick digest; a positive
completed tick has one. N must permit a checked N+1.

The allocator, entity map, thread map, and world object map have a one-to-one
relationship. Keys, local IDs, and generations must agree. Local IDs are
positive, generation zero is invalid, and completed state cannot retain a dead
or exited entity. Suspended or faulted entities remain representable. Nonzero
persistent IDs can be shared only by members of the same multitile group.

### Entity/world/avatar graph

Object definitions must exist. The validator checks the 38 globals, 80 object
data fields, 128 sprite flags, attributes retaining the definition's initial
count with source-compatible zero-filled growth up to 4,096 slots, bounded lists and
per-type/tuning/advertisement maps, and at most three pending lifecycle entries
from the source init/multitile/main sequence. Raw main-parameter and main-stack
IDs can remain null or historical values. Queued-user references must be live.

Entity position, facing, and avatar flags must agree with the world projection.
Group membership/base-object relationships are reciprocal and unique; the
world's multitile projection agrees with them. A normal singleton group has no
world multitile-group marker. Containment requires reciprocal parent/slot
references, equal positions, zero physical extent for the contained object,
and an acyclic container graph. Avatar behavior slots are exactly three;
other objects use their definition's slot count. Behavior containment slots
and physical routing reservations are separate structures.

The exact source out-of-world sentinel `(-32768, -32768, 1)` is accepted by the
world validator. Arbitrary out-of-bounds coordinates are not substituted for
it. World validation also checks its owned terrain/room/portal/placement/slot
graphs and reconstruction invariants.

An avatar's entity ID, generation, persistent ID, object GUID, mode/platform,
thread flag, and always-tick behavior must agree with its owner. Avatar-owned
validators check person data, motives, animation state, and other serializable
fields. Motive limits and embedded TSO decay tuning must match content.
Animation metadata must be an exact known content record for the resource.

Headlines are simulation state because their lifetime influences every-frame
scheduling. Source-scaled duration bounds are `-491520..=491505`; the animation
counter is a wrapping signed 32-bit value. Optional balloon icon references
must be structurally valid but may identify a historical generation.

### Relationship projections

`SimState.relationships` is the shared VM relationship book. It validates
identity-kind combinations, matrices of at most 256 columns, and bounded
matrix/changed/reverse collections. Entity owners, local targets, and reverse
bookkeeping references must identify live generations. Persistent dirty
targets must be nonzero.

Each avatar's relationship values and persistent-dirty set must exactly equal
the shared book's rows and dirty entries for that avatar's entity owner.
Dirty entries and local reverse edges may exist without a matrix: the source
can clear a matrix without clearing its bookkeeping, or mark bookkeeping
before a later operation fails. This does not permit a divergent avatar copy
or a recycled local identity.

### TS1 neighborhood projections

The optional TS1 family budget and the TS1 inventory book are simulation
projections. TSO snapshots must contain no TS1 family budget and an empty
inventory map. Every snapshot runs the book's own validator, which caps the
total number of token items at 65,536. A single neighbor cannot bypass that
aggregate bound.

TS1 budgets retain their full signed 32-bit range, including negative values
produced by the source's wrapping subtraction. Neighbor IDs are raw signed
short identifiers from the neighborhood and do not require a live lot entity.
Token order, duplicate entries, unsigned 16-bit counts, and empty recorded
inventories remain part of the source projection. Import, durable saving, and
authority for these values belong to the neighborhood/persistence adapter.

### VM frames and continuations

`VmThread::validate` checks routine availability, instruction/frame bounds,
register sizes, request context, continuation depth, and stopped/resolved
status. A saved host request must also match the current immutable instruction's
request family, opcode, and static operand fields. Captured route parameters and
locals must equal the suspended frame's banks, and captured external parameters
must equal its arguments. Replacing both a VM request and its effect payload
with a matching forgery does not bypass these checks. Shared Temp/XL values may
change during a wait; captured request values from those banks and external
world memory remain latched instead of being reread during restore.
The owning thread's entity must be live and match its map key.
Cached caller/callee/stack-object references have a different lifetime: they
may survive deletion until execution rejects or resets the access. Their local
IDs and generations must be structurally valid, and a cached stack reference
must match its raw stack-object ID. Restore retains that identity; it never
rebinds the cache to a recycled object ID.

The persisted `next_continuation` is the next unused nonzero ID. Every retained
runtime or VM continuation ID is smaller, and distinct VM continuations cannot
reuse an ID even after resolution. Each unresolved waiting VM has
exactly one matching runtime operation, and each `resumes_vm` operation owns
that waiting frame. A resolved ready/budget-exhausted VM continuation may remain
after its runtime operation has been removed. A standalone route has
`resumes_vm == false`, a live idle owner with no executing BHAV, and a future
schedule.

Pending effects pass `EffectBook::validate_context(N, epoch)` and must match
one live owning continuation each. The stored request is `Bytes` containing
the exact canonical public `ExternalRequest`; the expected outcome kind is
also `Bytes`. The effect book itself never interprets these bytes as an
instruction to execute a transaction.

Stored external `VmResolution` values must select one of the four explicit
true/false branches (`GotoTrue`, `GotoFalse`, or their next-tick variants).
At most 1,024 typed register writes are allowed, with indexes checked against
the actual temporary, extended temporary, local, parameter, or stack-ID bank.
Pending responses and the synchronous-only `CompleteWithWrites` response are
invalid stored resolutions. An animation event can resume only an animation
request. Runtime delivery applies the same external branch/index checks before
publishing a completed effect transition.

Routes pass their owned recursive structural and world validation. Active
actors, targets, callback references, and required chair/slot generations must
be live where the route phase requires them. Geometry/search/path bounds,
root ownership, route IDs, and nested-route structure must agree. A changed
target position/revision can intentionally remain cached until replanning.
Failed/completed route memory can retain historical references; the validator
does not turn these caches into a new live target. Active runtime routes must
have a future scheduled owner tick.

## Authority and recovery responsibilities

The expected epoch is an exact decode expectation. A coordinated takeover can
restore an authenticated snapshot from its recorded epoch and then use the
runtime's takeover transition to advance the epoch. Snapshot decoding does not
silently rewrite epoch fields or mint fresh logical effect IDs. A pending
effect keeps its namespace/nonce ID and original issuance context; the new
owner redispatches it with separate fencing metadata.

There is no database client, ledger executor, authentication credential, GPU
handle, or private EOD checkpoint in this format. Public VM data can include
mirrors needed by simulation, but those values cannot overwrite authoritative
balances, inventories, property, or committed ownership. Producers of opaque
effect bytes are responsible for including public VM data only. The native
EOD owner coordinates private recovery independently with the public VM wait.

Structural validation and a checksum cannot prove that a valid old snapshot
is the newest authorized state. Publishing and restoring snapshots therefore
requires the external accepted-command journal, operation/outcome journal,
transactional outbox positions, and lot epoch to describe one capture boundary.
After installation, the runtime consumes the authenticated accepted tail from
N+1. Pending durable outcomes are reconciled by operation ID; arbitrary trace
entries are never executed as side effects.

## Verification and remaining integration gates

`crates/sim-core/tests/snapshot_contract.rs` uses synthetic minimal states and
adversarially rehashed payloads. It exercises the fixed header and canonical
round trip, checksum/identity/version/tuning mismatches, framing/limit failures,
duplicate map keys, huge allocation hints, excessive recursive nesting,
entity/world/thread/allocator inconsistencies, stored count limits, incomplete
tick boundaries, stale cached references, pending/resolved effect ownership,
response kinds and register destinations, embedded animation metadata,
standalone route restore, containment cycles, the source out-of-world sentinel,
relationship projections/bookkeeping, and headline bounds.
It also checks TS1 budget and inventory round trips, TSO mode separation, and
single-neighbor and aggregate inventory item limits.

Targeted command from the worktree:

```sh
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo \
CARGO_TARGET_DIR=/workspace/scratch/cb8e2814717b/toolchain/target-runtime-review \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 \
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --offline --test snapshot_contract
```

Final independent result: **26 passed, 0 failed**. The target ran in 0.53 seconds
as part of the final 108-test snapshot/query/VM replay, with no warnings. That
run included the final content fields, relationship/headline schema, lazy
attribute growth, TS1 projection bounds, and five paired VM/effect request
forgeries: changed opcode, kind, operand, captured parameter, or literal amount.
A positive wait with changed shared Temp/XL values also restored successfully.
The two new negative TS1 tests were observed failing before the TSO mode and
inventory-bound guards were added. The runtime reviewer separately reran all
26 snapshot tests in its final 59-test runtime/memory/query/snapshot gate, also
with no failures. The owned Rust implementation and fixtures pass
`rustfmt --edition 2021 --check`; the scoped `git diff --check` passed as well.

The tests establish the new format's local round-trip and rejection behavior.
Native/WASM full-runtime replay, real imported content cohorts, interaction
queue integration, authenticated transfer, coordinated publication/recovery,
and shared-contract adoption remain their owners' integration gates. No test
here establishes closure of those broader gates or legacy save compatibility.
