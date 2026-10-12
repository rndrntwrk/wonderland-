# Private native lot coordinator

This Worker exports a SQLite-backed `LotCoordinator` Durable Object and has only a 404 public fetch handler. Initialization and mutation require an explicitly provisioned private binding. The coordinator trusts the admitted Rust supervisor to supply simulation results; a matching digest is integrity, not proof of a correct simulation or caller authentication. No deploy configuration or credentials are included.

`commitBatch` checks the strict versioned binding/cursor contract and the Rust-compatible canonical payload SHA-256, then atomically stores its receipt, ordered 256 KiB journal chunks, effect/event outbox references and successor cursor. Retries return the original receipt. A changed payload under the same identity, a competing successor, or exhausted 4,096-batch/64 MiB journal capacity fails closed. No receipt eviction or compaction occurs.

`publishCheckpoint` requires the current committed snapshot hash, writes content-addressed R2 bytes, verifies readback and rechecks the cursor before publishing a SQLite manifest. Existing corrupt objects are refused, including a size check before reading their contents. An interrupted or superseded publication can leave an unadvertised object. Manifests and objects are not automatically garbage-collected.

## Fenced replacement

`beginRecovery(expectedIncarnation)` durably freezes fresh old-owner mutations and records one exact successor epoch/incarnation. Repeating that request returns the recorded grant across restarts, including after completion. Epoch/incarnation exhaustion fails without mutating state. Previously accepted batch receipts remain available for lookup; retrieving one never grants a new mutation.

The Rust replacement proves the old committed cursor, adopts the grant's exact successor epoch and emits a `recovery_candidate` marker with a pinned new snapshot. `finishRecovery(marker, bytes)` verifies the recorded grant, immutable R2 anchor/readback and unchanged predecessor, then commits the new binding/cursor, checkpoint manifest and completion record atomically. Only then may the private supervisor send `activate_recovery` with that exact marker. The host refuses prepare before activation. Logical effect IDs/issued epochs remain stable while dispatch epochs advance. Existing outboxes are retained and never silently acknowledged.

These grants are private protocol records, not authentication credentials. A deployment supervisor must securely control binding access and native process creation. Multiple consumers holding the same current private grant are not independently authenticated; admission and an actual provider orchestration loop remain subsequent work.

## Executable qualification

Build `runtime-bridge` and `wonderland-native-lot-host --all-targets`, then run `npm ci`, `npm run check`, `npm run build`, and `npm run e2e` here. The journey runs actual Rust processes and local Cloudflare workerd with persisted SQLite/R2. It covers accepted behavior/build/dialog effects, process restart and caller retry, exact retained journal/outbox payloads, genuine multi-chunk accepted ticks, capacity faults, corrupted R2 objects, a successor during checkpoint I/O, and a replacement Rust process restoring a checkpoint plus accepted tail to the exact committed state hash. The test-only driver introduces storage/capacity faults and an R2 pause; it is excluded from production output. Capacity fixtures are not successful gameplay transitions.

The retry witness discards an already-returned response and restarts workerd; it does not inject a crash between SQLite commit and network delivery. Authenticated player admission, deployed supervisor orchestration, automatic multi-batch recovery assembly, effect delivery/acknowledgment, safe compaction and live deployment remain open. Local old-owner fencing and same-grant retries are process-qualified. This is one Task 3 increment, not hosted multiplayer completion.
