# Private native lot coordinator

This Worker exports a SQLite-backed `LotCoordinator` Durable Object and has only a 404 public fetch handler. Initialization and mutation require an explicitly provisioned private binding. The coordinator trusts the admitted Rust supervisor to supply simulation results; a matching digest is integrity, not proof of a correct simulation or caller authentication. No deploy configuration or credentials are included.

`commitBatch` checks the strict versioned binding/cursor contract and the Rust-compatible canonical payload SHA-256, then atomically stores its receipt, ordered 256 KiB journal chunks, effect/event outbox references and successor cursor. Retries return the original receipt. A changed payload under the same identity, a competing successor, or exhausted 4,096-batch/64 MiB journal capacity fails closed. No receipt eviction or compaction occurs.

`publishCheckpoint` requires the current committed snapshot hash, writes content-addressed R2 bytes, verifies readback and rechecks the cursor before publishing a SQLite manifest. Existing corrupt objects are refused, including a size check before reading their contents. An interrupted or superseded publication can leave an unadvertised object. Manifests and objects are not automatically garbage-collected.

## Fenced replacement

`beginRecovery(expectedIncarnation)` durably freezes fresh old-owner mutations and records one exact successor epoch/incarnation. Repeating that request returns the recorded grant across restarts, including after completion. Epoch/incarnation exhaustion fails without mutating state. Previously accepted batch receipts remain available for lookup; retrieving one never grants a new mutation.

The Rust replacement proves the old committed cursor, adopts the grant's exact successor epoch and emits a `recovery_candidate` marker with a pinned new snapshot. `finishRecovery(marker, bytes)` verifies the recorded grant, immutable R2 anchor/readback and unchanged predecessor, then commits the new binding/cursor, checkpoint manifest and completion record atomically. Only then may the private supervisor send `activate_recovery` with that exact marker. The host refuses prepare before activation. Logical effect IDs/issued epochs remain stable while dispatch epochs advance. Existing outboxes are retained and never silently acknowledged.

These grants are private protocol records, not authentication credentials. A deployment supervisor must securely control binding access and native process creation. Multiple consumers holding the same current private grant are not independently authenticated; admission and an actual provider orchestration loop remain subsequent work.

## Bounded recovery bundle

During a pending grant, `recoveryBundle(expectedIncarnation)` chooses a compatible retained checkpoint, verifies its R2 length and hash, validates stored accepted payload digests/receipts and selects the unique exact predecessor chain to the grant's old cursor. Gaps, forks, oversized tails and state changes during asynchronous reads fail closed. The checkpoint is returned as a binary Uint8Array; accepted packets retain their hex encoding. The 8 MiB checkpoint and 8 MiB binary tail limits keep the result below 25 MiB, within the private RPC profile. Receipt and payload memory remains bounded by the existing journal/proposal limits; this is not a measured maximum-RSS claim.

The native loader accepts a paired optional first tail and up to 64 total independently pinned tail files. Aggregate bytes and frames are bounded across all files. A checkpoint already at the committed cursor needs no fake empty packet. The actual process journey uses this production bundle selector rather than a hand-assembled journal. Deployment still needs to provision/run the supervisor which transports these bounded files and starts the host.

## Executable qualification

Build `runtime-bridge` and `wonderland-native-lot-host --all-targets`, then run `npm ci`, `npm run check`, `npm run build`, and `npm run e2e` here. The journey runs actual Rust processes and local Cloudflare workerd with persisted SQLite/R2. It covers accepted behavior/build/dialog effects, process restart and caller retry, exact retained journal/outbox payloads, genuine multi-chunk accepted ticks, capacity faults, corrupted R2 objects, a successor during checkpoint I/O, and a replacement Rust process restoring a checkpoint plus accepted tail to the exact committed state hash. The test-only driver introduces storage/capacity faults and an R2 pause; it is excluded from production output. Capacity fixtures are not successful gameplay transitions.

The original retry witness discards an already-returned response. A separate POSIX process journey now pauses the local HTTP driver before commit and after the real durable commit but before returning its HTTP response, then sends SIGKILL to a dedicated process group containing Miniflare and workerd. It restarts persisted SQLite, checks unknown versus exact committed receipts, retries the same real Rust proposal, and verifies a nonempty pending dialog outbox is retained without duplication. The native candidate remains unpublished until the recovered durable receipt is resolved. This exercises abrupt local process loss and an undelivered client reply; it does not model provider-wide power loss, filesystem corruption, or Cloudflare regional failure. Authenticated player admission, deployed supervisor orchestration, effect delivery/acknowledgment, safe compaction and live deployment remain open. Local old-owner fencing and same-grant retries are process-qualified. This is one Task 3 increment, not hosted multiplayer completion.

## Private account authority

`AccountAuthority` now provides a staged private account/appearance boundary.
Build the Rust kernel with `bash tools/build-account-crypto.sh` from this directory
(or its repository-root path), using the pinned Rust target and wasm-bindgen
0.2.129, before `npm run check` / `npm run build`. The build emits a static WASM
module beside the Worker; it is not downloaded or compiled from untrusted bytes.
Run `npm run e2e:accounts` for actual local workerd/SQLite/Rust-WASM journeys.
The public fetch handler remains 404. See `docs/security/NATIVE-ACCOUNT-BOUNDARY.md`
for source policy, trust boundaries and the open UI/recovery/gameplay gates.

## Staged controlled-mode guard (V2, no runtime admission yet)

Private `initializeControlledV2` initializes a **new** lot with the existing
content-bound cursor and an immutable controlled-mode marker. Neither initializer
can upgrade/downgrade an existing lot. An empty marker table preserves legacy V1
behavior; its controlled row is retained independently of lot cursor state.
`controlStateV2` returns a strictly versioned decimal revision, never an actor
grant. New exact-idempotent control tombstones increment this revision in their
SQLite transaction; changed identities and counter exhaustion roll back.

Controlled-mode lots reject V1 commit, checkpoint and recovery operations,
including a checkpoint racing initialization across its asynchronous hash. The
V2 mutation/recovery lane is deliberately unavailable: do not provision a live
lot in this mode until source-native admission, cleanup-progress fencing, leases,
registry-bound recovery and authenticated orchestration are implemented and
qualified. No existing lot is migrated. This guard adds no public route, actor,
control grant, claim release or playable-game acceptance.

Build `cargo build -p wonderland-native-host-contracts --example control-state-vectors
--locked` at the repository root, then run `npm run e2e:control-contract` here for
12 strict Rust/TypeScript golden vectors. `npm run e2e` additionally tests actual
SQLite controlled-mode initialization races, legacy mutation denial against a real
native proposal, concurrent tombstones, revision overflow rollback and persisted
workerd restart. Existing legacy durability/replacement/crash tests remain intact.
