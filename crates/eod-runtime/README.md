# Native private EOD host

`wonderland-eod-runtime` is a standalone, dependency-free Rust 1.90 library. It
rejects WebAssembly builds and forbids unsafe code. It does not contain a VM,
renderer, browser, transport, authentication implementation or database provider.

The exact 30 source server registrations and 28 UI registrations are in
`src/registry.rs`. Only `0xAA65FE9E` (`VMEODTimerPlugin`) can be instantiated.
Its behavior is translated from the source; all original-runtime verification
flags remain false. The other 29 registered plugins return `UnverifiedPlugin`.
Unknown IDs return `UnregisteredPlugin`, without a stub fallback.

See `../../docs/swarm-b/eod-coverage.md` and
`../../fixtures/eod/registration-census.json` for every ID, exact server/UI pair,
source anchor, recovery policy and open leaf requirement.

## Integration contract

1. The authoritative transport implements `ConnectionAuthority`. A connection ID
   identifies one non-reusable transport incarnation. The lookup derives the
   actor independently of UI message bytes. The authoritative VM implements
   `RegisterSource`, returning a read-only snapshot of the timer invoker's first
   four temporary registers.
2. An authoritative Invoke Plugin operation calls `NativeHost::connect` with
   `ConnectRequest`. This is a trusted VM entry, not a network operation. The
   current implementation is a single-participant timer instance; it does not
   claim support for joinable lobbies, controller-only connections or arbitrary
   SimAntics callbacks.
3. Give the returned `SessionTicket` only to that connection. Inbound UI frames go
   through `receive_bytes(authority, connection, bytes)`. The typed `receive`
   entry enforces the same version, size, allowlist, authentication, host scope, epoch,
   plugin, session, ordering and rate checks. Tickets are routing identifiers,
   not authentication secrets. Each ticket includes the authoritative host scope,
   so equal per-scope epochs and counters cannot alias after a still-authenticated
   connection moves to another host. Sequences start at one. Replays are rejected.
4. Drain `take_public_events()` into the synchronized VM command adapter. Its
   typed variants expose the exact source event code/temp arguments via
   `source_event()`. Source connection/disconnection events are `-2` and `-1`.
   Deliver `take_private(authority, connection, ticket)` only on that same
   connection's private UI channel. The type cannot convert into a public VM
   event; its Debug is redacted. The private UI adapter must retain plugin and
   ticket checks when consuming deliveries.
5. Call `tick` once per authoritative 30 Hz tick with a stable VM register and
   authentication snapshot. Timer does not advance the VM's time registers; it
   reads them and emits the same control/update events as the source. Timeout
   order is deterministic by instance ID. The default idle timeout is 1,800
   ticks; accepted UI activity renews it. Detached restored instances wait for
   rebind while the idle deadline keeps advancing.
6. Handle `QueueFull` as backpressure: drain/admit outputs and retry the same
   operation. The host uses bounded clone-and-commit transactions, so a rejected
   operation does not change plugin state, counters or sequence. Missing VM
   registers reject a complete tick atomically. This foundation has no measured
   30 Hz performance claim; queue handling and VM commit ordering remain adapter
   responsibilities.

## Protocol version 1

The inbound envelope is intentionally separate from the legacy C# network wire
format. Unreleased version 1 uses a 57-byte scoped header; the earlier unscoped
draft is unsupported. All integers are little endian, and the whole frame is bounded before
parsing. Text must be UTF-8; event names are bounded ASCII. Parsing borrows the
frame and allocates no input-derived buffer.

| Byte offset | Field |
| --- | --- |
| 0 | `EODI` magic, 4 bytes |
| 4 | protocol version, `u16` = 1 |
| 6 | host scope, `u64` |
| 14 | host epoch, `u64` |
| 22 | instance ID, `u64` |
| 30 | session generation, `u64` |
| 38 | plugin ID, `u32` |
| 42 | sequence, `u64` |
| 50 | payload kind, `u8`: 0 text, 1 binary |
| 51 | event-name byte length, `u16` |
| 53 | payload byte length, `u32` |
| 57 | event-name bytes followed by payload bytes |

The decoder rejects trailing bytes and unsupported payload kinds. There is no
sender, recipient or Verified field to forge. The only accepted timer events
are binary `Timer_State_Change`, `Timer_IsRunning_Change`, `Timer_Set`, and text
`Timer_Close`. Source-invalid handler values are accepted as no-ops, consume a
sequence/rate slot, and produce no plugin event, as documented in the fixtures.

## Private checkpoints

`checkpoint_to` writes only through `PrivateCheckpointStore`, at a quiescent
barrier where both output queues are empty. The store receives private bytes;
there is no public snapshot or generic Debug path to retrieve them. The
provider must use private, integrity-protected storage and atomic writes, and
retain the latest trusted `CheckpointStamp`. No such provider is supplied.

The VM must commit its synchronized events and checkpoint at the same barrier.
Draining an event from the host is not confirmation that the VM applied it. A
provider must not publish a checkpoint stamp until its write has committed.

`restore_from` checks the exact expected scope, epoch and revision, private
format 1, timer schema 1, bounds, unique participants/invokers/generations, source
plugin ID and all serialized fields. It rejects timeout-policy changes without
an explicit migration. A strictly newer host epoch is mandatory; choose it from
authoritative durable state, fencing any earlier host before resuming effects.

No network connection ID is serialized. `rebind` requires an `InstanceAddress`
with the host scope and instance ID, obtainable from `ticket.instance_address()`.
It rejects a different scope and verifies fresh authentication for the recorded
actor, rotates the session generation, resets inbound sequence
to one and reconstructs the UI from supplied authoritative registers. Preserved
private timer state is not reset. The restored VM connection already exists, so
rebind does not emit another VM connect event.

Every unimplemented registration has an explicit abort/reconcile policy: its
source abort behavior and any provider-confirmed reservation/refund must be
implemented and verified before enabling its recovery. The host rejects those
plugins and their checkpoints today. It never estimates a refund or simulates a
successful transfer.

## Durable effects

`effects::EffectOutbox` requests private-data writes, fund reservations,
settlements and refunds through `DurableEffectProvider`. It performs no effect.
Keys are stable across host epochs and must be allocated by an authoritative
durable caller. Immutable requests and terminal receipts are retained; duplicate
keys with a changed operation fail. A retry reuses the exact original request.

The provider must authorize all accounts, ownership and reservation references,
and atomically persist deduplication and effect outcome. It must also enforce
that a reservation cannot be settled or refunded twice even under different
operation keys. Unknown or mismatched provider receipts remain pending. No
production provider or durable outbox recovery is implemented. The outbox is an
in-memory handoff foundation; future effectful plugins must persist their
requests/receipts in private recovery state before being enabled. Timer does
not request durable effects or use RNG.

## Validation

```sh
CARGO_INCREMENTAL=0 /root/.cargo/bin/cargo test --manifest-path crates/eod-runtime/Cargo.toml --offline
python3 tools/swarm-b/eod-census.py --check
/root/.cargo/bin/cargo fmt --manifest-path crates/eod-runtime/Cargo.toml --check
CARGO_INCREMENTAL=0 /root/.cargo/bin/cargo clippy --manifest-path crates/eod-runtime/Cargo.toml --offline --all-targets -- -D warnings
```

The tests are source-derived native fixtures and adversarial host/provider
tests. They are not original-runtime conformance, a browser UI implementation,
database integration, object-content validation or performance evidence.

## License and source attribution

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain
one at http://mozilla.org/MPL/2.0/.

The source timer behavior and registration dictionaries are derived from FreeSO
at commit `4c6b3e8f5835b228723caea3c9f683c62f244f73`, by the original FreeSO
contributors, under the repository's `../../LICENSE.md`. Existing C# source and
its notices are unchanged. No original game assets are included.
