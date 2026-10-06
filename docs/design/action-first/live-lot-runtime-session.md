# Live-lot native replica session

PR #22, based on PR #21 (`dbc08f1f0ce0c65a6cad9ef9f7fd34fef3c2c522`).

This checkpoint implements the **native runtime replay and interaction boundary** for continuous play. It wraps the existing A/B `GameRuntime`; it does not introduce another simulation or silently change the original connected client's protocol. **The existing browser lot is not made continuously playable by this PR alone.** No UI, original C#/resources, authority implementation or other swarm branch is replaced.

## Implemented contract

`wonderland_game_runtime::live_session::LiveReplica` starts in `AwaitingCheckpoint`. It is constructed only from a replica-role runtime and a fresh, nonzero browser/source/lot identity. Its live projection and interaction methods remain unavailable until checkpoint handoff succeeds.

| Operation | Behavior |
| --- | --- |
| `connection()` | A local opaque callback token. Capture it when creating a socket; never substitute the newest token in an old callback. |
| `checkpoint_ticket()` / `request_checkpoint(token)` | One correlated request generation. A replaced request cannot consume or invalidate its successor. |
| `install_checkpoint(ticket, checkpoint, tail)` | Validate native bytes, content identity, lot/authority epoch, mode, simulation/effect limits, effect namespace, envelope tick and hash. Restore a detached candidate, replay its tail, and publish only the complete result. |
| `apply_batch(token, frames)` | Apply actual accepted A ticks and verify their post-state hashes. Publish outcomes only after the entire batch succeeds. A later failure restores the complete pre-batch state and enters recovery. |
| `offers(...)` | Query the actual detached source check tree. Source keys, dynamic labels and `Param0` values remain distinct. |
| `prepare_interaction(...)` | Re-query the selected exact key/parameter against current state; preserve current entity generations, world and queue revisions. Return an **unaccepted** intent without changing the replica. |
| `prepare_cancel(...)` | Validate the exact visible queue action ID and current actor grant/version. Return an **unaccepted** cancellation. It does not optimistically remove a queue item. |
| `suspend(token)` / `reconnect()` | Stop exposing live state; invalidate old callback and checkpoint generations. Reconnect requires a new checkpoint. |
| `close()` | Terminal disposal of the runtime, pending checkpoint and cursor. |

Preparation rejects zero command sequences and any sequence already accepted by the restored queue. The transport must also allocate sequences monotonically across **pending** intents and reconcile uncertain writes; this library does not persist a client outbox, reserve pending sequence numbers or automatically retry commands. The authority still validates every incoming intent. A local `Authority` role, callback token or hash is not authentication.

`runtime()` is deliberately read-only and exposes the last committed state for diagnostics during recovery. It is not a substitute for the live-only `projection()`. No mutable runtime accessor is provided.

## Recovery and publication invariants

An A checkpoint at completed tick **N** is followed by accepted tick **N+1**. This is different from the legacy FreeSO pre-tick snapshot boundary. The original `FSOv` and `VMNetTickList` handling from PRs #20/#21 is untouched.

A checkpoint at the already committed tick must have the same hash. An older checkpoint is accepted only with a contiguous tail that crosses the committed hash and catches up at least to the committed tick. A newer checkpoint is trusted input from the authenticated authority; hashes/checksums here do not prove its earlier history or authenticate its sender.

Recovery returns a cursor, **not** historical outcomes. Thus the caller cannot accidentally replay the recovered tail's transient sounds, headlines or UI notifications through this return value. Normal exact latest-tick duplicates also produce no new outcome. Older/conflicting duplicates and gaps request recovery rather than being guessed away.

Replica transitions must emit no durable effect dispatches. Checkpoint candidates and failed batches never publish partial state or presentation events. Authority admission, effect execution and persistence remain outside this client library.

## Input and memory boundaries

Default admission limits are 64 ticks, 4,096 commands and 8 MiB of native binary data per batch, and the existing 64 MiB native snapshot payload plus its envelope. Callers can lower these limits; constructors reject zero or above-ceiling values.

Nested commands are measured through a bounded binary sink using the **already pinned bincode 1.3.3** with fixed integers and little-endian encoding. This supports native tuple-keyed maps, which JSON cannot represent. The lockfile only adds the existing bincode dependency to `wonderland-game-runtime`; no dependency version changes.

These are bounds for **already decoded input**, not a network decoder or a total browser-heap guarantee. The future transport must bound frames before decoding, and account for retained content, a rollback state, checkpoint candidates and returned outcomes. The successful batch path clones simulation state but does not clone immutable content. No physical-device performance or memory benchmark is claimed.

## Verification

Fresh local checks passed on the recorded code:

| Gate | Result |
| --- | --- |
| Full locked native workspace | **1,350 passed; zero failed; five pre-existing optional tests ignored** |
| New live-session suite, included above | **31 passed; zero failed or ignored** |
| Browser audio Node suite | **59 passed; zero failed or skipped** |
| Formatting and strict native all-target Clippy | Passed |
| Strict web-shell WASM Clippy | Passed |
| Optimized ordinary WASM conformance probe | Built and executed successfully in Node's WebAssembly engine |
| Native / ordinary WASM complete record | **56,785 bytes exactly equal**; zero host imports; non-shared memory |
| Comparator negative controls | Six equal-but-wrong semantic records and one changed ordered-event record rejected |
| Original source/resources and existing UI | No changes against PR #21 |

The shared record contains complete final checkpoint bytes, entity/queue projections, ordered event diagnostics and a 61-tick action trace. It verifies two independent replicas, source action completion, dynamic menu parameters, need changes, duplicate suppression, stale callbacks, recovery through tick 64 and atomic rejection/recovery through tick 66. It is **not** a two-player limit, an actual network test, a browser UI interaction run or a claim of full original-object behavior.

Three selected unchanged BHAVs are embedded from the existing repository: casino bar 4110 (literal resulting attributes `[0, 0, 30, 0]`), Christmas flag 4108 (three original dynamic menu variants), and cursebook 4107 (energy/hunger/hygiene refill). Object definitions, grants, lot and interaction-table setup are explicitly declared harness metadata. A separate authored idle harness verifies actual source-primitive cancellation and frame completion. The seven displayed needs remain runtime-driven; the unprovided room/environment value remains `None`.

The initial absent-module test failure was observed in hosted CI before implementation. Subsequent behavioral regressions were observed failing and then passing for native tuple-key command admission, already accepted command-sequence reuse, and changed effect-stream configuration. Earlier hosted formatting failures remain historical failures. The unchanged pinned `proc-macro-error2` dependency retains its future-Rust compatibility notice.

[Machine-readable verification](live-lot-runtime-session-verification.json) records command results, code/input hashes and the common-output digest. Hosted checks on the published head are linked from PR #22 and must be checked separately; an older successful run is not evidence for a new head.

## Reproduction

Use the repository's pinned Rust toolchain, its `wasm32-unknown-unknown` target, and a supported Node installation. Run from the repository root, with a new output directory:

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
node --test crates/audio-runtime/browser/*.test.mjs
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown \
  --lib --bin wonderland-web-shell --locked -- -D warnings

mkdir -p /tmp/wonderland-live-check
cargo run -p wonderland-game-runtime --example live_session_probe --locked \
  > /tmp/wonderland-live-check/native.json
cargo build -p wonderland-game-runtime --example live_session_probe \
  --target wasm32-unknown-unknown --release --locked
node crates/game-runtime/scripts/verify-live-session.mjs \
  /tmp/wonderland-live-check/native.json \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/examples/live_session_probe.wasm" \
  /tmp/wonderland-live-check/wasm.json
```

The output writer refuses to overwrite an existing WASM result. The focused `Live lot session` workflow runs the new tests and complete native/WASM comparison and retains the resulting records. The existing `Browser UI` workflow separately covers all native/audio tests, strict lint and the actual optimized Trunk application build. Neither workflow is represented as browser interaction QA.

## Remaining client integration

1. **Select and implement the service protocol explicitly.** Either complete original FSOv-to-A state/command conversion, or provide an explicitly configured native A service with matching effective source-content identity. A decoded source presentation snapshot is insufficient to resume all native threads, queues, routes and effects.
2. **Bind the authenticated browser session to this lifecycle.** Add a bounded decoder, checkpoint/tail handoff and retained connection tokens in the actual transport. Bind principal and selected actor to server admission, not values asserted by the browser. Preserve the existing legacy lane and uncertain-write behavior.
3. **Drive the scene, interactions and presentation from accepted state.** Feed live semantic world/poses into the existing source renderer, wire source resource/animation/contact providers, consume new accepted cues once, and attach prepared action/cancellation intents to the actual scene controls.
4. **Verify the complete player path.** Sim selection → map admission → lot → source action → animation/queue/needs across independent browser sessions → disconnect/recovery, followed by original-content, real-service, physical mobile and cross-browser qualification.

All 21 original player surfaces remain in scope. This checkpoint does not mark those remaining integrations complete, merge the other swarms, or deploy a server.
