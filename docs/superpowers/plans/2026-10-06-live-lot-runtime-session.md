# Live-lot runtime session integration

## Product goal

Keep the action-first Sim → city → lot UI from PR #21. Join the existing deterministic A/B runtime to the connected client's eventual continuous-play path, with source-defined interactions, accepted updates, and recoverable shared state. Preserve every original player capability; do not substitute an animated snapshot or hard-coded demo menu for simulation.

## Checkpoint implemented in PR #22

A DOM-free `LiveReplica` around `GameRuntime`, native checkpoint and accepted-tick handoff, connection/request fencing, deterministic validation, failure-atomic replay, read-only projections and freshly queried source interaction intents. This layer is useful to a browser transport but does not itself install that transport into the existing FreeSO gateway.

## Decisions

- **Ruling:** keep FSOv/VMNet and native A checkpoints explicitly separate. The repository's FSOv decoder is a presentation decoder, whereas A restoration requires complete validated `SimState`. Pretending they are interchangeable would create plausible but incorrect gameplay. This checkpoint does not claim original-server continuous playback.
- **Ruling:** consume accepted ticks only; prepare user intents without advancing the replica. Authority and durable side effects remain server-owned.
- **Ruling:** entire batches publish atomically. A bad later tick must not leave a visible or inspectable earlier prefix committed.
- **Ruling:** reconnect creates a fresh callback token and checkpoint request. A checkpoint older than the committed cursor is allowed only with a contiguous tail crossing the exact committed hash and catching up at least to it.
- **Ruling:** use the existing source BHAV routines in tests with their setup explicitly named as a harness. These tests qualify runtime execution, not an entire object family or production content set.

## Work and acceptance

1. Publish regression specifications first. Observe missing-module failure in hosted CI.
2. Implement lifecycle, checkpoint validation, accepted replay and live-only projection.
3. Verify atomic rollback, duplicate suppression, identity/request fencing, no backward recovery, bounded admission and closed-state disposal.
4. Exercise actual unchanged BHAV checks/actions through the session; preserve dynamic menu parameters and non-mutating intent preparation. Reject unauthorized/stale selections.
5. Run Rust formatting, strict native/WASM checks, complete native/audio suites and release build. Preserve failures as historical evidence, not successes.
6. Publish exact revisions, test evidence and an integration handoff. Leave the PR unmerged; gameplay not covered by these tests remains open.

## Remaining to reach the product acceptance path

- Full original FSOv-to-A state restoration and source command translation, or an explicitly configured native A service lane with original-source content supplied by the content pipeline.
- Actual bounded authenticated browser transport, source-scene/animation/picking and accepted-cue wiring.
- Connected object/person actions and cancellation in the UI, with authoritative admission and resynchronization.
- Real service/content acceptance across independent browsers, followed by physical mobile and cross-browser checks.
