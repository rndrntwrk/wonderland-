# Native EOD host, provider and private recovery boundary

`wonderland-eod-runtime` implements all thirty source registrations. The four
casino, eight social and ten service handlers use the `plugins` family interface
and private format 4; earlier handlers retain their format 1/2/3 behavior. The
[generated census](eod-coverage.md) records exact source registrations,
per-handler tests and the narrower original component/helper evidence.

The crate implements native state transitions, admission, private checkpoints,
provider request preparation and recovery. It contains no production database,
funds ledger, account directory, inventory service, transport authentication,
encryption store, renderer or complete VM. Real provider and VM-adapter
qualification remains separate from the native tests.

## Authoritative integration

1. Create a group with `NativeHost::connect_native(NativeCreateRequest)`. The VM
   supplies a nonzero object, trusted cluster, invoker and typed family config.
   Casino/service config object identities must equal the host object's ID. An
   object cannot have two groups for the same plugin. The host emits its native
   controller connection event even for original handlers whose source client
   entry was UI-only; that authority adapter event is not claimed as a complete
   original-server connection trace.
2. Admit a UI participant through `join_native` with `ConnectionAuthority` and
   `NativeJoinRequest`. The host derives the authenticated actor and allocates a
   scoped session generation. Role, registers, avatar object/UID, skills,
   owner authorization, gender and skin are trusted invocation values. Source
   game chairs remain separate from the host's private routing seat.
3. Feed UI bodies through the existing scoped `receive`/`receive_bytes` API.
   Event names and text/binary kinds are family-allowlisted. No UI body can set
   owner permission, seed, private provider receipt, actor identity, native VM
   observation or animation callback. Wrong scope/epoch/generation, recipient,
   sequence, rate or payload bounds fail before state admission.
4. Deliver native observations/callbacks through `deliver_native_event` with the
   current `NativeControllerTicket` and recorded invoker. Drive one host tick
   per accepted authoritative 30 Hz VM tick. Private random streams use bounded
   deterministic native algorithms; their complete state is checkpointed and
   cannot be seeded or read by a UI payload.
5. Apply `take_public_events()` and `take_native_commands()` through the accepted
   VM barrier. Commands are limited to typed graphics batches, forced
   interactions, outfit changes and conditional default outfit changes. Use
   `OutfitScope::source_value()` for original VM scope values. Private messages
   can be retrieved only for the authenticated scoped session through
   `take_private`; they cannot convert into public VM events.
6. Drain and deliver all accepted outputs and commit the matching VM checkpoint
   with `checkpoint_to`. Only then call `drive_native_provider` for prepared
   external work. Queue removal alone is not evidence the VM applied a command.
   The adapter must provide an actual shared commit/replay barrier.

Identity uniqueness covers live host invokers, authenticated actors, transport
incarnations and avatar object IDs across the old and new families. Native
persistent avatar UIDs are unique across their rosters. Dresser pending default
continuations additionally reserve the UID across native-group departure and
restore until the retained operation resolves.

## Immutable external operations

A `NativeOperationId` contains host scope, origin epoch, group instance and a
monotonic operation number. The private request also binds plugin, object,
callback and the complete typed operation. Those bytes remain immutable across
retry and restore. The current `HostIdentity` is passed separately to the
provider; a request originating in an older epoch does not authorize an old host
to keep writing.

`NativeProvider::execute` must authorize every account/object/avatar/catalog/
plugin namespace, atomically apply each requested effect, and durably deduplicate
**the complete request and exact terminal reply**. This includes read snapshots,
refusals, successful transactions and responses lost after commit. Reusing an
operation key with different bytes must fail. A new host must replay the same
origin ID and receive the same result, not reread a changed unversioned snapshot.

`NativeProviderReceipt` must match the current host and exact request ID, report
complete bounded bytes, and decode to the correct family reply. Financial and
inventory outcomes use typed terminal family replies. `NativeProviderFailure`
`Retryable` retains work; `Denied` is a host/authority failure and retains the
operation for trusted recovery; `Corrupt` rejects the reply. No failure becomes
a fabricated success or empty default record.

Requests first enter the host's private journal and become dispatchable only
after a successful private checkpoint. An unprepared or retryable predecessor
blocks later work in its group. Other independent groups may progress.
Acknowledged external effects commit individually, because a later provider
failure cannot undo an earlier acknowledged transaction.

Reply application clones host state, applies the handler transition, checks
pending-operation equality and reachable-state invariants, admits every public,
private and peer output, and only then removes the journal record. Full output
queues or invalid replies retain the same immutable operation for exact replay.
Each family exports its complete pending-operation map; a missing, changed,
extra or duplicate callback/operation fails validation before dispatch.

## Format 4 and restore

Format 4 starts with `EODP`, little-endian version 4 and schema 1. It contains an
exact length-delimited legacy format 2/3 inner record, native member identities,
group metadata/rosters, each bounded private family kernel, and the operation
journal. Manual family codecs preserve source-specific original UI/storage
bodies separately from this private host format. Existing format 1/2/3 bytes and
entry points remain supported.

`PrivateCheckpointStore` must atomically store integrity-protected private
bytes and maintain a trusted latest `CheckpointStamp`. `restore_from` requires
that exact stamp and a strictly newer host epoch. No old transport connection is
restored. All recorded members and controllers start detached; every session
rebind requires fresh authentication for the recorded actor and rotates the
session generation. Controller rebind requires its recorded address/invoker and
emits no second source connection event.

Validation bounds counts before allocation and checks exact group/plugin/object
identity, member/seat/roster agreement, cross-family identity uniqueness, source
phase reachability, timers, counters, hidden hands/RNG, cache ownership, private
payloads, callback domains and equality between the kernel's expected pending
operations and the journal. Trade's final operation must equal both retained
accepted offers. Wardrobe pending stages, outfit namespaces and retained writer
identity must match the exact operation. Native private types expose no generic
public serialization or unredacted Debug conversion.

## Required peer recovery

Social families export their actual retained dependencies: BuzzerPlayer's owner
host, BuzzerHost's occupied slots, nightclub controller's floor, floor's
controller, and DJ/platform controller links. War, Band, service and casino
handlers have no cross-group dependency list. The host checks the transitive
closure with a bounded cycle-safe visited set before ordinary messages, ticks,
native callbacks and live provider progression. Every required controller and
recorded member must be bound, even for an output-silent transition. Independent
groups can progress in the same cluster.

Rebind is display reconstruction, not a gameplay transition. Trusted teardown
must be able to remove the detached dependency that is pausing progression:
peer actions from a closing source require the recipient's own bindings and may
clear its retained link. Ordinary peer actions still require the full dependency
closure. Private peer identity/type/cluster validation runs on the resulting
state. No unbound recipient silently drops a peer action.

Closing groups retain unresolved financial and storage obligations. They may
finish bookkeeping while detached. A controller event or typed VM command,
however, requires the recorded controller to rebind before the provider result
can be consumed. A dresser conditional default command carries the frozen
original asset and replacement; the VM applies it only if the current default
still equals that original asset. Outside inventory writers and authoritative
replacement ownership must be coordinated by the real provider/VM integration.

## Named native boundary policies

| Policy | Behavior and evidence |
| --- | --- |
| `NATIVE-AUTHORITATIVE-INVOCATION` | Typed trusted config/member/controller input; scope/epoch/actor/invoker and object/account mismatches rejected. |
| `NATIVE-PRIVATE-STATE` | Native-only build; private handlers, decks, RNG, requests and checkpoints; public projection excludes these values. |
| `NATIVE-PREPARED-IMMUTABLE-OPERATION` | Checkpoint before external dispatch; exact whole-request/result replay; current-host receipt fence and per-group order. |
| `NATIVE-ATOMIC-OUTPUT-ADMISSION` | Reject a transition before source state, sequence or RNG mutation commits if any output cannot be admitted. |
| `NATIVE-RETAINED-DEPENDENCY-READINESS` | Freeze silent countdown/RNG and ordinary input while required peers remain detached; allow independent groups. |
| `NATIVE-TRUSTED-DEPENDENCY-TEARDOWN` | A closing source may clear its detached link from a locally bound peer without enabling ordinary gameplay. |
| `NATIVE-VM-CONTINUATION-REBIND` | Retain prepared provider work when its VM continuation has no attached controller; replay after rebind. |
| `NATIVE-RESTORE-UI` | Reconstruct recipient-specific saved views without initialization, random draw, payout or phase transition. |

The host tests are in `service_handlers.rs`, `casino_handlers.rs`,
`social_handlers.rs` and `native_recovery.rs`. The last suite directly covers
silent nightclub RNG, buzzer countdown subframes, transitive DJ dependencies,
unrelated same-cluster progress and teardown in both floor/controller
directions. Older scoped wire, persistence, game, queue, recipient, timer and
checkpoint regressions remain part of the full crate gate. The original
component/helper oracles remain a separate evidence family.
