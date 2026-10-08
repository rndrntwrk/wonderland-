# Swarm E starting handoff from C — 7 October 2026

This is a handoff and ownership boundary, not a claim that E services have been
implemented or that C is complete. The approved program starts E with W13.1 and
W15.1 after W00.2/baseline contracts. C's complete renderer is not a predecessor.
C's SL.5 city-to-lot acceptance instead depends on E's SL.3 directory/admission.

## Read the actual interfaces before implementation

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
The published A core is PR6 at `8a0e251d19e222a0a6833d7408ca629f674e1729`.
The client baseline for this handoff is PR32 at
`544ceb2b5e0faf99f723ad84d3ac1b91eab8dd36`, stacked on PR28 and PR23.
These refs are integration inputs, not permission to overwrite their branches.
Refresh current refs before assigning work; preserve the integrator's merge queue.

In the inspected client tree:

- `crates/contracts/src/lib.rs` explicitly declares a **bounded presentation /
  preview boundary, not a multiplayer wire protocol**. Its availability and
  opaque presentation IDs are never sufficient server authorization.
- `services/browser-gateway` is an **original-service compatibility gateway**.
  It connects to original account HTTP and Aries TCP servers. It does not supply
  the new native account database, authoritative lot service or SQL claim store.
  Do not rename that bridge into the complete W13/W14/W15 implementation.
- `crates/game-services` holds original account/directory/session DTOs and parsers;
  `crates/vm-protocol` carries original wire decoding. These remain distinct from
  F's new native versioned accepted-command contract.
- `crates/sim-core` owns tick/RNG/VM/effect semantics. `crates/game-runtime` and its
  world projection are integration consumers, not renderer-derived authority.
- `crates/world-view::WorldDocument`, GPU generation IDs, lighting recipes and
  facade metadata are read-only presentation inputs or disposable derivatives.

The new native wire and database contracts therefore need an explicit F-owned
freeze/mapping review before E labels its integrated codec complete. Research,
fixtures, codec tests and schema/claim work can start now without waiting for C.
Do not infer a frozen multiplayer ABI from the presence of a Rust crate named
`contracts`.

## First two parallel E leaves

### W13.1 — bounded WSS codec and session routing

Own the assigned new-native `crates/net-protocol/src` and gateway session/limits
files; do not silently change the original compatibility gateway or C's modules.
Consume F's agreed versioned `ClientIntent`, `AcceptedTick`, control, IDs and
snapshot descriptors. The authenticated session supplies actor identity.
Specify framing, byte/message/queue quotas, heartbeat/deadline behavior, sequence
and replay policy, source/version mismatch and explicit overload/resync results.

Acceptance must reject malformed/oversized frames, unsupported versions, forged
actors, client-authored server messages, duplicate sequences, queue exhaustion
and heartbeat failure. A successful socket write is not an accepted game action.
Use real authenticated ingress for integration, not fixture-only authorization.

### W15.1 — database mapping and fenced claims

Own the assigned `crates/persistence` schema/accounts/avatars/lots/objects/claims
and migrations. Preserve the plan's initial SQLx + MySQL/MariaDB model; changing
storage engine is separate scope. Inventory source ID widths, collations, existing
constraints/triggers and ownership semantics before mapping them.

Implement atomic acquire, renew, transfer and release for lot/avatar claims with
monotonic fencing epochs. Both city allocation and lot actors consume this store.
Test concurrent acquisition, expiry/reacquisition, compare-and-swap transfer and
stale writer rejection against the real database adapter. In-memory races do not
establish SQL transaction correctness.

## Expand after these boundaries are ready

W13.2 adds native lot actors and mirrors using A's same 30 Hz core. W15.2 adds
atomic money/inventory/ownership operations with stable durable operation IDs and
an outcome/outbox record. W13.3 reconnects typed outcomes to the correct entity
and continuation. W15.3/W13.4 coordinate immutable snapshots and accepted tails:
state after tick N, tail begins N+1, one explicit capture boundary.

An effect's logical ID survives a lease change; its fencing credential does not.
A new owner reconciles a committed historical outcome rather than paying twice
or rejecting valid old-epoch truth. A stale snapshot never restores money or
ownership. Private EOD hands/decks/RNG/checkpoints stay out of public snapshots,
common state hashes, logs and generic browser projections.

SL.2/SL.3 then provide real account/owned-avatar/grant and at least two seeded
city destinations with capacity/admission and fenced lot identity. C consumes the
accepted destination and first world state, then verifies select/join/return to
the same city. A render loading failure cannot convert denial into admission or
commit a server effect.

## Disposable facade jobs

The new C worker accepts a normalized immutable world JSON and creates FSOf plus
a digest/provenance receipt in a new private directory. E may wrap it in a bounded
job queue after review. Key it by the declared source/version and verify actual
bytes; admit publication only if its source still matches the requested visual
revision. Do not trigger it blindly at 30 Hz or use its receipt as a lease,
checkpoint, transaction cursor or proof of multiplayer authority.

Only the supplied lighting state is rendered. Missing source assets remain
reported. Cancellation or failed generation must not disturb an existing valid
published derivative. Keep derivative cache/storage separate from immutable VM
snapshots, durable outcomes and private EOD state. The CLI's metadata-last file
pair is not a database checkpoint transaction.

## Exit evidence

Record exact commits, contract versions, test fixtures, database configuration,
commands, failures and reviewer. Independent review is required for authenticated
commands, persistence, money/ownership and shared protocols. No production
credentials, real-data migration, merge, deployment, billing change or assumed
live server is authorized by this handoff.
