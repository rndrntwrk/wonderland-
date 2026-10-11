# Native live wire: checkpoint and accepted-tick delivery

This increment is based on PR #22 at `9fe3a7cd404f4aca35cd3dd6db557fe555c5eb17`.
It consumes the real `GameRuntime` and `LiveReplica`; it does not create another
simulation model. PR #21's game UI, original `TSOClient`/`Other`, dependencies,
and concurrent legacy FIFO work are preserved.

**WLR1 is a native Rust replica protocol, not FreeSO FSOv/VMNet.** The existing
connected original-server lot remains a refreshed source snapshot. Original
snapshot/command translation, the authenticated native authority service,
and mounting this native transport in the player browser remain separate work.
There is no silent fallback from failed live admission to preview or source refresh.

## Implemented boundary

`wonderland_game_runtime::live_wire` owns one `LiveReplica` and its recovery
request correlation. The receiving application supplies an authenticated
socket, verified content, source/lot identity and a replica-role runtime.

| API | Contract |
| --- | --- |
| `NativeWire::new` | Requires a valid replica and wire limits; starts a checkpoint request before live state is usable. |
| `connection` | Capture this opaque token when creating each socket callback. Never obtain a newer token from inside an old callback. |
| `checkpoint_request` | The current request ID and last committed cursor, or none. IDs increase on this adapter across recovery/reconnect. Exhaustion closes it. |
| `encode_checkpoint_request` / `decode_checkpoint_request` | Client-only fixed-width request; decoding allocates nothing. A cursor is a hint, not permission or an instruction to trust client history. |
| `encode_checkpoint` / `encode_ticks` | Server encoders preserve exact native integers and command structures. Server tick hashes must come from the admitted authority transition. |
| `decode_packet` | Server-direction parser with borrowed checkpoint bytes and bounded owned tick records. It does not authenticate the peer or accept a checkpoint by itself. |
| `receive` | Parses and delivers directly through the real replica's checkpoint/hash/replay checks. It never executes a local unaccepted command. |
| `replica` | Read-only access for live projection, exact source menus and preparation of interaction/cancellation intents. Sending/admitting those intents is the host's responsibility. |
| `disconnect`, `reconnect`, `close` | Explicit lifecycle; stale socket tokens cannot suspend or modify a replacement stream. |

A valid accepted batch returns ordered `TickOutcome`s only after **every** tick
and post-state hash validate. A bad later tick restores the whole batch's
previous state, exposes no earlier outcome, and binds a fresh checkpoint request.
Checkpoint installation replays an accepted tail without emitting historical
presentation events or durable operations. An exact retained last tick returns
no duplicate outcomes. A replay beginning earlier than A's retained tick requires
recovery; unchecked history is not silently discarded.

Malformed current packets suspend the stream and invalidate the pending request;
the owner must close the affected socket and explicitly reconnect. Ticks received
before the requested checkpoint are rejected rather than lost. A well-formed late
checkpoint for an obsolete request is ignored without poisoning newer recovery.
Old callback tokens are rejected before parsing their bytes. No unknown user write
is automatically retried, and no socket write is represented as a receipt.

## WLR1 version 1

This version is tied to the pinned native `AcceptedTick`/`AcceptedCommand` schema.
An incompatible native serialization change needs a protocol-version change;
matching a magic value alone is not schema or account authentication. Native
checkpoint restoration additionally checks its actual content/configuration.

All numeric fields use fixed-width little-endian encoding. JavaScript must pass
binary data to Rust, not parse u64 fields through `Number`.

The 32-byte envelope contains: eight-byte `WLR1\r\n\x1a\n` magic, one-byte kind,
seven zero reserved bytes, u64 request ID, and u64 exact payload length. Unknown
kinds, reserved bits, trailing data and mismatched lengths reject.

| Kind | Payload |
| --- | --- |
| 1: checkpoint response | Nonzero request ID; u64 completed tick, 32-byte state hash, u32 checkpoint byte length, exact native checkpoint bytes, then a tick block. |
| 2: ordinary accepted ticks | Zero request ID; a tick block. |
| 3: client checkpoint request | Nonzero request ID; byte 0 for no cursor, or byte 1 followed by u64 lot, u64 authority epoch, u64 completed tick and 32-byte state hash. Total message length is exactly 33 or 89 bytes. |

Each tick block starts with a u32 count. Each record contains a 32-byte post-state
hash, u32 encoded length and an exact fixed-integer, little-endian bincode
`AcceptedTick`. Binary encoding retains native tuple-keyed maps that ordinary
JSON objects cannot represent. Client requests cannot enter the server parser,
and server packets cannot enter the request parser.

The decoder compares a streaming canonical reserialization against each tick's
original bytes without constructing a second full encoded copy. Duplicate or
noncanonical map/set entries cannot silently collapse into a different admitted
command. Original data is not normalized into a guessed legacy command.

## Resource admission

Defaults are 64 ticks, 4,096 total commands and an 8 MiB tick block. A single
encoded tick is limited to 1 MiB. The checkpoint cap follows the native snapshot
payload limit plus its actual header/checksum. `max_packet_bytes()` supplies the
checked total checkpoint-packet ceiling for a transport's pre-buffer message cap.

The owner **must enforce message size before WebSocket buffering/allocation**.
This module operates on an already borrowed message; it cannot undo allocation
performed by the socket or JavaScript beforehand. Compressed/aggregate transport
limits, rate limiting and authentication remain host responsibilities.

Nested decode work is bounded across the whole block: 262,144 visited items,
32 MiB conservative allocation-admission credit, and depth 64 by default. Hard
ceilings reject invalid configurations. Collection length hints are checked and
then hidden from Serde container visitors, so they cannot reserve arbitrary
capacity before individual values are admitted. Strings/owned byte buffers use
a slice-backed path and are charged before their owned output is constructed.
Top-level records and encoded packets use fallible exact reservations.

Allocation credit is deliberately conservative and based on the target's native
type sizes plus collection overhead; it is **not** an RSS bound, a replacement
allocator, a guarantee that OOM cannot occur, or an equality claim for admission
thresholds at every target boundary. The runtime retains its independent batch,
checkpoint and simulation limits after decoding. Fatal allocation/host failures
are not presented as recoverable successful commands.

## Reproduction

Use the repository's pinned Rust toolchain and Node. No operator endpoint, account
or external copyrighted content pack is needed for these declared source fixtures.

```sh
cargo test -p wonderland-game-runtime --test live_wire --lib --locked
cargo run -p wonderland-game-runtime --example live_wire_probe --locked > /tmp/wire-native.json
cargo build -p wonderland-game-runtime --example live_wire_probe \
  --target wasm32-unknown-unknown --release --locked
node crates/game-runtime/scripts/verify-live-wire.mjs \
  /tmp/wire-native.json \
  target/wasm32-unknown-unknown/release/examples/live_wire_probe.wasm \
  /tmp/wire-wasm.json
```

The comparator creates its output with create-new semantics; use a new output
path for a rerun. Add `--offline` to Cargo commands when dependencies are cached.
The scoped `Native live wire` workflow reproduces the package checks and both
executions. The existing `Browser UI` workflow checks the complete workspace and
builds the unchanged player application separately.

## Verification and fixture scope

The test fixtures import **unchanged checked-in BHAV bytes** through the original
content loader. Metadata, four-attribute object definitions, permissions, source
identity and sessions are explicit test harness values. This does not qualify the
entire casino object, production prices/ownership, authentication or a deployed
multiplayer server.

The wire probe exercises two replicas over 60 consecutive source-runtime ticks,
original action completion and attributes `[0, 0, 30, 0]`, read-only dynamic source
menus/parameters, original needs changes, checkpoint-tail recovery, last-tick
suppression and a bad second tick with complete rollback. The same bytes run
natively and in ordinary browser-target WASM with zero host imports and non-shared
memory. Complete records include snapshot bytes/hashes, packet hashes, ordered
events, queues and needs. The comparator rejects equal-but-wrong expected values,
wrong request precision, altered snapshots, and differing event records.

The adjacent machine record contains the exact commands, counts, output identities
and local verification scope. Hosted CI status belongs to the published commit,
not to this locally verified record. No rendered-browser, real WebSocket server,
physical-device or complete original-game acceptance is inferred from the probe.

## Still required for the player experience

Connect an authenticated native authority and its verified content admission,
correlated request replies, command admission/results and bounded socket delivery
into the actual browser scene. Feed returned accepted outcomes to the real renderer,
needs/queues and gameplay audio exactly once. Preserve UI state across recovery,
without reviving unconfirmed actions. Separately implement original FSOv/VMNet
translation or retain that path explicitly as refresh-only.

Full catalog/inventory/Build/property and remaining social/object-dialog integration,
original resource completeness, real multiplayer/reconnect/persistence testing,
merge review and deployment remain outside this increment. This code does not
rename those remaining requirements as complete.
