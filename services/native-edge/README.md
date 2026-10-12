# Private native lot coordinator

This Worker exports a SQLite-backed `LotCoordinator` Durable Object and has only a 404 public fetch handler. Initialization and mutation require an explicitly provisioned private binding. The coordinator trusts the admitted Rust supervisor to supply simulation results; a matching digest is integrity, not proof of a correct simulation or caller authentication. No deploy configuration or credentials are included.

`commitBatch` checks the strict versioned binding/cursor contract and the Rust-compatible canonical payload SHA-256, then atomically stores its receipt, ordered 256 KiB journal chunks, effect/event outbox references and successor cursor. Retries return the original receipt. A changed payload under the same identity, a competing successor, or exhausted 4,096-batch/64 MiB journal capacity fails closed. No receipt eviction or compaction occurs.

`publishCheckpoint` requires the current committed snapshot hash, writes content-addressed R2 bytes, verifies readback and rechecks the cursor before publishing a SQLite manifest. Existing corrupt objects are refused, including a size check before reading their contents. An interrupted or superseded publication can leave an unadvertised object. Manifests and objects are not automatically garbage-collected.

## Executable qualification

Build `runtime-bridge` and `wonderland-native-lot-host --all-targets`, then run `npm ci`, `npm run check`, `npm run build`, and `npm run e2e` here. The journey runs actual Rust processes and local Cloudflare workerd with persisted SQLite/R2. It covers accepted behavior/build/dialog effects, process restart and caller retry, exact retained journal/outbox payloads, genuine multi-chunk accepted ticks, capacity faults, corrupted R2 objects, a successor during checkpoint I/O, and a replacement Rust process restoring a checkpoint plus accepted tail to the exact committed state hash. The test-only driver introduces storage/capacity faults and an R2 pause; it is excluded from production output. Capacity fixtures are not successful gameplay transitions.

The retry witness discards an already-returned response and restarts workerd; it does not inject a crash between SQLite commit and network delivery. Authenticated player admission, epoch/owner fencing, automatic supervisor orchestration, multi-batch recovery assembly, effect delivery/acknowledgment, safe compaction and live deployment remain open. This is one Task 3 increment, not hosted multiplayer completion.
