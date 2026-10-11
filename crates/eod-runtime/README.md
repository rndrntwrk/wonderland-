# Native private EOD host

`wonderland-eod-runtime` is a dependency-free Rust 1.90 library. It rejects
WebAssembly and forbids unsafe code. It contains no VM, renderer, transport,
authentication service, database or encryption provider.

All thirty registered server handlers are translated from C# and dispatched
through `NativeHost`: the original five, three cooperative games, four casino
games, eight social handlers and ten service/clothing/trade handlers. Native host
behavior and private recovery are tested separately from bounded original C#
component/helper oracles. Complete original-runtime, UI-runtime and production
provider qualification remains open. Unknown IDs return `UnregisteredPlugin`;
handlers needing trusted typed invocation cannot use the timer-only entry.
There is no fallback stub.

The [native family boundary](../../docs/swarm-b/eod-native-boundary.md) describes
`connect_native`, authenticated `join_native`, epoch-fenced callbacks,
checkpoint-prepared external operations and private format 4. Family contracts
are documented for [casino](../../docs/swarm-b/eod-casino.md),
[social](../../docs/swarm-b/eod-social.md), and
[services](../../docs/swarm-b/eod-services.md).
See [generated coverage](../../docs/swarm-b/eod-coverage.md) and the
[registration census](../../fixtures/eod/registration-census.json) for source
anchors, exact server/UI pairings, source quirks, recovery policies and open
leaves.

## Invocation and delivery

1. The trusted transport implements `ConnectionAuthority`. Connection IDs name
   non-reusable authenticated transport incarnations. Actor identity is derived
   from the transport, never from UI data.
2. The trusted VM calls existing `connect` with `ConnectRequest` for Timer, or
   `connect_plugin` with `PluginConnectRequest` and typed `PluginInput` for the
   earlier simple handlers, and `connect_native`/`join_native` with typed family
   inputs for casino/social/service handlers. Cooperative games retain their
   `connect_game_controller`/`join_game` interface. Signs ownership/roommate status, Door Edit authorization,
   modes, avatar ObjectID and persistent-object ID come from the VM invocation.
   They cannot be changed by inbound frames. The old timer-only entry reports
   `PluginInputRequired` for translated plugins needing this additional data.
3. DanceFloor has a separate native `connect_dance_controller(object, invoker)`
   entry. Multiple authenticated players on that object route button codes to
   this controller with their recorded avatar ObjectID as temp0. Controller
   connections create no transport ticket or private UI. The trusted VM ends
   them through `disconnect_invoker`. One controller per object is allowed.
4. Deliver inbound UI frames through `receive_bytes(authority, connection,
   bytes)`. The typed `receive` entry performs the same version, bounds, event
   allowlist, payload kind, scope, epoch, session, actor, sequence and rate
   checks. Sequences start at one. Tickets are scoped routing identities, not
   authentication secrets.
5. Drain `take_public_events()` into the synchronized VM adapter. Its typed
   variants expose exact source event code/temp arguments through
   `source_event()`. Apply typed native-family commands from
   `take_native_commands()` at the same accepted VM barrier. Connect/disconnect
   are `-2`/`-1`. Only
   `take_private(authority, connection, ticket)` retrieves that recipient's UI
   outputs; private payloads cannot convert to public events and redact Debug.
6. Call `tick` once per authoritative 30 Hz tick. Only Timer consumes current
   `RegisterSource` values. Signs and Door initialize after provider data loads.
   Scoreboard sends show on connect and binary state on load completion.
   Revoked transport authentication disconnects on the next tick. Detached
   participants retain their bounded idle deadline.
7. Treat `QueueFull`, `PersistenceLimit` and `PersistencePending` as explicit
   admission failures. A rejected receive/tick leaves source state and message
   sequence unchanged. Source-malformed handler values are accepted no-ops;
   invalid envelope/authentication/authority is an explicit error. Signs writes
   before permission initialization are rejected with `PluginNotReady`.

Persistent handlers allow one active session/write stream per scoped plugin and
persistent object. This deliberate native rule serializes source patches and
avoids the C# asynchronous patch race. It is not evidence that every joinable
legacy invocation or multiplayer callback has been reproduced.

## Private plugin-data provider

The `persistence::PluginDataProvider` boundary has bounded `load` and immutable
`write` requests. `PluginDataKey` includes host scope, plugin and persistent
object. `PluginWriteId` additionally includes origin epoch, instance and
operation sequence; retries and restores retain the same identity. Writes carry
the authenticated actor and expected record revision. Request Debug is redacted.

New persistent sessions begin loading. Call `drive_persistence` to complete
provider work. A provider must report an explicitly absent record before the
source defaults are used. Malformed, denied or oversized reads do not silently
become successful loads. Incoming valid mutations create private write intents
and source VM/UI outputs. The provider does not receive an intent until
`checkpoint_to` successfully records it at the matching VM/event barrier.

A concrete adapter sequence is:

1. Invoke, load and initialize the source handler.
2. Receive an authenticated mutation and apply its public VM events.
3. Deliver the private UI outputs and commit the authoritative VM checkpoint at
   the same barrier as `checkpoint_to`.
4. Call `drive_persistence` to dispatch the checkpointed immutable request.
5. Retry a lost/ambiguous response using the retained ID and bytes. A later
   checkpoint records the acknowledged binding or remaining work.

The provider must fence the current `HostIdentity`, authorize actor/object/plugin
access, atomically compare-and-set the expected revision and durably deduplicate
the complete request. Repeating a previously applied ID must return its original
receipt even if the response was lost. A conflicting reuse must never apply
another write. No provider satisfying that production contract is supplied.

Provider work commits one operation at a time; an error in a later operation
cannot undo a prior durable acknowledgement. Pending, conflicted and orphaned
intents count against record and byte limits. A UI disconnect does not discard
ambiguous work. `abort_conflicted_writes` drops only provider-confirmed
not-applied requests and disconnects their sessions; it cannot drop retry work.

## Private checkpoints and recovery

`PrivateCheckpointStore` receives private bytes only. It must provide private,
integrity-protected, atomic storage and an authoritative latest
`CheckpointStamp`. Both UI and VM output queues must be drained before saving.
Draining an event does not itself prove the VM applied it: the VM and EOD
checkpoints must share an actual commit barrier.

Timer-only snapshots retain format/schema 1. Earlier mixed snapshots use format
2; cooperative game snapshots use format 3; native-family snapshots use format
4 with an exact legacy inner record. Per-handler schemas remain 1. Format 2 stores loading/initialization/source state,
participant identities, native controllers, persistence bindings and immutable
write intents. Parsers bound lengths before allocations and check identities,
uniqueness, schemas, counters, deadlines, canonical intent bytes and relevant
source-state/persistence consistency. Unsupported state fails closed.

`restore_from` requires an exact trusted stamp and a strictly newer host epoch.
It discards all old transport bindings and detaches native controllers. Replayed
write intents retain their original ID while the provider separately fences the
new host epoch. Before persisted UI rebind, `drive_persistence` must resolve
pending work and confirm the provider revision, record existence and private
bytes match the expected binding. Divergence returns `PersistenceDiverged`
and remains blocked. The trusted adapter can end an unreconciled detached
participant through `abort_unreconciled` once it has no unresolved intent, then
invoke afresh to reload authoritative data.

`rebind` requires a scoped `InstanceAddress` and fresh authentication for the
recorded actor. It rotates the generation and resets inbound sequence to one,
reconstructs UI without resetting source state and does not emit a second VM
connect event. Timer rebind retains its current register input. Controller
rebind uses `rebind_dance_controller(address, recorded_invoker)` and produces no
player button effects before it succeeds.

Door Edit intentionally retains the source's cached original code after
`set_code`; a new invocation reloads the saved value. Door View/CodeInput never
receive `door_code`. Signs read redaction and the original write-mode override
are retained. A Signs UTF-16 truncation that splits a surrogate is rejected
atomically, instead of reproducing the source encoder exception and partial
mutation.

## Protocol version 1

The native envelope is independent of the legacy C# wire format. Its 57-byte
header is little endian. Event names are bounded ASCII and text bodies are
UTF-8. The parser borrows the input after enforcing the full frame bound.

| Offset | Field |
| --- | --- |
| 0 | `EODI` magic, four bytes |
| 4 | version `u16` = 1 |
| 6 | host scope `u64` |
| 14 | host epoch `u64` |
| 22 | instance ID `u64` |
| 30 | generation `u64` |
| 38 | plugin ID `u32` |
| 42 | sequence `u64` |
| 50 | kind `u8`: text 0, binary 1 |
| 51 | event length `u16` |
| 53 | body length `u32` |
| 57 | event bytes then body bytes |

The earlier unscoped draft and extra sender/recipient/Verified fields are
rejected. Every implemented handler has its own event/kind allowlist.

## Native family effects and remaining production integrations

The new `NativeProvider` boundary carries typed atomic purchases, stock/debit,
clothing mutations, cooldown reservations, whole-offer trade, casino transfers,
revisioned data/name storage and bounded read snapshots. The actual handlers
retain unresolved operations, refunds and private continuations across departure
and restore. `effects::EffectOutbox` remains a separate general foundation; it
is not the native-family journal. The production ledger, inventory service,
durable provider, private checkpoint store and VM adapter remain unimplemented
outside their declared native integration contracts.

## Validation and evidence

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml --offline
python3 tools/swarm-b/eod-census.py --check
/root/.cargo/bin/cargo fmt --manifest-path crates/eod-runtime/Cargo.toml --check
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 /root/.cargo/bin/cargo clippy --manifest-path crates/eod-runtime/Cargo.toml --offline --all-targets -- -D warnings
```

Tests use source-derived expectations and actual native host/provider boundary
adapters. Separate original C# existing-five, social and service component
oracles and a bounded casino helper oracle execute pinned source under their
documented boundaries. They do not execute the complete FreeSO application/UI,
establish a production durable provider, validate object content or measure
30 Hz performance.

## License and attribution

This Source Code Form is subject to the Mozilla Public License, v. 2.0. If a
copy of the MPL was not distributed with this file, You can obtain one at
http://mozilla.org/MPL/2.0/.

The translated handlers and registration dictionaries derive from FreeSO at
commit `4c6b3e8f5835b228723caea3c9f683c62f244f73`, by its original contributors,
under the repository's [LICENSE.md](../../LICENSE.md). Existing C# source and
notices are unchanged. No original game assets are added.
