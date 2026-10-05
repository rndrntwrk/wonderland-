# Durable effect request and response boundary

## Scope and source evidence

`crates/sim-core/src/effects.rs` implements a simulation-local continuation
contract. It records external intents, emits retry envelopes, validates scheduled
responses, cancels waiting operations, and serializes the state needed for replay.
It contains no database/network adapter, authentication implementation, balances,
inventory ownership records, or durable transaction executor. These interfaces
remain provisional until the shared-contract owner adopts or adapts them.

The pinned FreeSO baseline is
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. Relevant source anchors are:

| Source anchor | Observed behavior and relationship to this implementation |
| --- | --- |
| `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetAsyncResponseCmd.cs`, `AcceptFromClient`, `Verify`, `Execute` | The response command is server-only. A nonzero local object ID must exist and have the exact expected blocking-state type before the response replaces that state. Both client and server execute the accepted command in order. The new contract retains an explicit expected type and adds a durable operation ID, entity generation, application tick, and separate commit/delivery epochs. |
| `TSOClient/FSO.Server/Servers/Lot/Domain/LotServerGlobalLink.cs`, `PerformTransaction` | The server bridge executes a database transaction asynchronously and queues a `VMNetAsyncResponseCmd`. This module covers the request/resolution boundary; transaction execution remains external. |
| `TSOClient/FSO.Server.Database/DA/Avatars/SqlAvatars.cs`, `UpdateAvatarLotSave` | Avatar lot-save updates exclude the budget field. Restoring effect continuations must likewise create no authority to overwrite committed balances or ownership. |

The supplied rewrite plan, Section 9, requires stable logical IDs through lease
changes, independent fencing credentials, reconciliation of historical commits,
and snapshots coordinated with accepted-command and transaction/outbox positions.
Those requirements intentionally strengthen the legacy async-response contract.
They are not claims that the legacy implementation already had this journal or
that this isolated boundary establishes full transaction or gameplay parity.

## Contract

`EffectBook::new(namespace, limits)` creates a stream with a persisted nonce
high-water mark of zero. `issue(target, tick, epoch, response_kind, payload)`
records a pending operation and returns its `EffectRequest`. The request carries:

| Field | Meaning |
| --- | --- |
| `operation_id` | Sixteen bytes: big-endian namespace followed by a nonzero big-endian nonce. |
| `target` | Positive local object ID and nonzero generation of the exact waiting entity. |
| `issued_tick`, `issued_epoch` | Original issuance context, retained across retransmission. |
| `response_kind` | Expected `Bool`, `Int32`, or `Bytes` outcome. |
| `payload` | Bounded public VM intent, as `EffectPayload::Bool`, `Int32`, or `Bytes`. |

The nonce increases only when a new request is successfully issued. It never
decreases on response, cancellation, dedup eviction, or redispatch. A checked
increment fails at exhaustion; it never wraps to zero. Namespace uniqueness
across distinct persistent streams is an adapter/runtime responsibility. An
epoch change must not allocate a different namespace for the existing stream.

`request(id)` returns a pending request by reference. `pending_requests()` yields
pending requests in operation-ID order. `redispatch(id, current_epoch)` and
`redispatch_pending(current_epoch)` return bounded `EffectDispatch` envelopes
with an unchanged request and a separate `dispatch_epoch`. They reject epochs
older than issuance and never modify the book or execute an external effect.

## Resolution and takeover

The adapter delivers `EffectResolved { operation_id, target, apply_tick,
delivery_epoch, committed_epoch, value }` in an authenticated accepted tick.
`resolve(response, current_tick, current_epoch, live_target)` performs all checks
before mutation:

1. The book itself has valid bounds, IDs/counter, map relationships, terminal
   order, values, and temporal metadata.
2. The operation is pending or retained in the terminal cache. Foreign, zero,
   unissued, and evicted IDs cannot create a new operation.
3. The response target exactly matches the stored local ID **and generation**;
   its value type and byte count match the request contract.
4. `apply_tick == current_tick`, and the application does not precede issuance.
5. `delivery_epoch == current_epoch`, and
   `issued_epoch <= committed_epoch <= current_epoch`.
6. A pending request can resume only when `live_target == Some(request.target)`.
   The runtime obtains this value from its entity table. A response field cannot
   serve as its own liveness proof.

A pending success returns `EffectResolution::Applied { operation_id, target,
value }`, removes the pending request, and retains a bounded terminal record.
No result variant writes registers or changes ledger state. `Bool` and `Int32`
are explicit primitive outcomes. A `Bytes` outcome requires a bounded semantic
decoder; the runtime must validate its continuation, any register-bank/index
selection, and primitive-specific constraints before publishing the combined
runtime/effect transition. Using a cloned runtime or equivalent transaction
prevents an invalid decoded VM result from leaving the effect closed.

For example, a request issued in epoch 2 can commit in epoch 2, survive takeover,
and be delivered at the scheduled tick in epoch 5. That delivery is valid. A
message still claiming delivery epoch 2 is rejected by the epoch-5 runtime.
Changing the delivery credential does not change the operation's logical ID.

A retained resolved operation returns `Duplicate` only when the target,
committed epoch, and typed value match the stored durable outcome. Delivery
epoch and scheduled tick may differ when an adapter redelivers the outcome, but
the new envelope must still match the current accepted tick/epoch. A message
scheduled for an earlier tick is rejected. Conflicting duplicate values or
commit epochs are errors. A duplicate never yields another resume value and
does not update terminal-cache order. It remains a no-op if the original entity
has since been deleted, because it cannot resume an entity.

## Cancellation, bounds, and snapshots

`cancel(id, target, tick, epoch)` closes a matching pending continuation.
`cancel_target(target, tick, epoch)` atomically closes all operations for exactly
one entity generation and returns their IDs in deterministic order. Both reject
invalid identity or a context preceding issuance. Repeating a retained matching
cancellation returns `AlreadyCancelled`; cancelling a resolved operation fails.
A cancelled operation rejects subsequent responses and redispatch.

Cancellation closes a simulation wait. It is not a reversal of a committed
database action. The adapter must reconcile any in-flight or already committed
durable operation according to its own transactional rules.

The default limits are 256 pending operations, 1,024 retained terminal
operations, 16,384 request-value bytes, and 16,384 response-value bytes. Absolute
ceilings are 4,096 pending, 4,096 terminal, and 65,536 bytes per value. Pending
and terminal capacities must be nonzero; byte limits may be zero. A Boolean
counts as one value byte, an `Int32` as four, and opaque bytes as their actual
length. Container metadata is separately bounded by the operation count.

Terminal records retain original request metadata and the typed outcome or
cancellation marker. Completed request payloads are discarded. Eviction follows
completion/cancellation insertion order, which is explicitly serialized. An
evicted allocated ID is `RetiredOperation`, never a new request or a successful
resume. Long-term transaction deduplication remains the durable journal's job;
the local terminal cache is intentionally bounded.

`EffectBook` and all request/response/value types are serializable. Every
fallible mutator validates before its first mutation; failed calls preserve the
book byte-for-byte. `validate()` checks intrinsic snapshot invariants including:

- Configured limits and collection/value bounds.
- Operation namespaces, nonzero nonces, and the persisted high-water mark.
- Request/map ID agreement and absence of pending/terminal overlap.
- Valid object IDs/generations and terminal response types/epochs/ticks.
- Complete, unique terminal-order entries matching the terminal map.

`validate_context(snapshot_tick, snapshot_epoch)` additionally rejects records
from the snapshot's future. The runtime checks live pending targets against its
entity/generation table. Decoding untrusted bytes also requires a bounded outer
snapshot decoder before validation; the derived Serde decoder is not itself an
allocation limit or an authenticity check.

A structurally valid snapshot cannot prove that its nonce is newer than every
previously published snapshot. The runtime/platform must coordinate snapshot
publication with accepted commands, operation/outcome journal positions, and
epoch fencing. Replaying the accepted tail re-creates the same logical IDs from
the same namespace and nonce sequence. Restoring an old snapshot without the
coordinated tail is outside this boundary's recovery guarantee.

Opaque payloads must contain public VM data only. This module cannot infer
whether arbitrary bytes contain private EOD data. Producers and runtime codecs
must exclude private EOD state and must not interpret saved outcomes as current
balances, inventory ownership, or an instruction to execute a durable action.

## Verification and integration limits

The adversarial suite is `crates/sim-core/tests/effect_boundary.rs`. It covers
epoch-independent IDs and snapshot replay, current-epoch delivery of historical
commits, stale/future epochs, deleted/recycled targets, wrong response types,
application-tick mismatch, conflicting duplicates, payload/count bounds,
generation-zero rejection, failure-atomic group cancellation, terminal eviction,
unissued/foreign IDs, nonce exhaustion, malformed snapshot bounds/counters, and
restored dedup/cancellation state. Snapshot corruption cases also include
request/map ID disagreement and duplicate terminal-order entries.

Targeted command from the worktree:

```sh
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo cargo test --manifest-path crates/sim-core/Cargo.toml --offline --test effect_boundary
```

Observed result: **19 passed, 0 failed**. `rustfmt --edition 2021 --check` for
the implementation/test files and the owned-file `git diff --check` also passed.

This verifies a pure simulation mechanism using synthetic fixtures. Adapter
authentication, database atomicity, outbox recovery, privacy classification,
shared-contract adoption, VM-specific byte decoding, and full native/WASM
runtime replay require their owning integration layers. This document makes no
claim that those gates or any complete source/content cohort are closed here.
