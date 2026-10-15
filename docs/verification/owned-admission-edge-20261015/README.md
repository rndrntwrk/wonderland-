# Owned-avatar admission: edge and native host integration

Date: 15 October 2026. Worktree baseline: `d9760b6f`.

## Result and qualification boundary

The actual local workerd/SQLite/R2 AccountAuthority and LotCoordinator now cross the production Rust Host admission and recovery contract. Two separately authenticated accounts export their real initialized owned-avatar rows; principals `1` and `2` control distinct persistent avatars `901` and `902` and distinct runtime entity references. Native proposals, rather than manufactured successful receipts, drive the durable admission transaction. A process kill plus workerd restart recovers the same actors from a persisted checkpoint and three accepted journal batches, completes takeover, and commits another tick.

The bounded `owned-boundary-fixture` executable supplies explicitly authored test content to the production Host. This is contract evidence, **not original-game content, rendered gameplay, source object-interaction, or full product qualification**. The source assets required for the original playable lot remain unavailable. The normal browser runner must receive a separately validated content/lot configuration and must not select this fixture as a product fallback.

## Executed checks

From `services/native-edge`:

- `npm run build`
- `npm run check`
- `node tests/owned-export.mjs`
- `node tests/owned-control.mjs`
- `OWNED_BOUNDARY_FIXTURE=/workspace/scratch/efdbaac59410/wonderland-browser-entry-20261014/target/debug/examples/owned-boundary-fixture node tests/owned-runtime.mjs`
- `node tests/accounts.mjs`
- `node tests/account-crash.mjs`
- `node tests/account-http.mjs`
- `node --test tests/account-provider-interruption.mjs`
- `npm run e2e:owned`
- `npm run e2e:control-contract` (12 exact Rust/TypeScript vectors)
- `npm run e2e` (legacy durability/recovery plus real process-kill receipt tests)

Captured outputs are adjacent `.log` files. The npm `e2e:owned` script runs all three owned-admission suites; the runtime binary can be selected with `OWNED_BOUNDARY_FIXTURE`, otherwise it uses `../../target/debug/examples/owned-boundary-fixture`. Generated account password WASM is required, as for the existing account suites. These tests use actual local workerd durable objects, not a JavaScript storage mock. Local `node_modules` and ignored target/generated artifacts were reused from the previous authorized cloud worktree; they are not delivered in the commit.

The runtime test verifies:

1. Exact TypeScript export / Rust DTO and canonical JSON/hash agreement for two distinct owned avatars.
2. Sequential installation, monotonic lot principal allocation, source admission, atomic receipt/registry/journal commit, and exact historical retries.
3. A lease renewal advances the durable control revision; an autonomous candidate prepared against the previous revision cannot commit.
4. Host SIGKILL, workerd restart, authenticated retrieval of the immutable saved export, checkpoint plus three real accepted batches, exact restored registry/permission checks, and durable takeover.
5. A subsequent real tick succeeds; a fence arriving after preparation rejects the next tick without advancing the cursor. A changed-control wrapper cannot masquerade as a historical recovery receipt.

Separate edge tests prove exact decimal outfit IDs, source mutation invalidation, missing legacy initialization denial, account/session isolation, restart, conflicting retries, durable expiry tombstones and typed lost-response denial lookup, no journal/outbox/cursor mutation on expiry, no V1 mutation bypass, no late lease revival, and no renewal overtaking a covering fence.

Presence renewal is enforced independently by AccountAuthority: `renewAvatarControlV2(token, export, operation, "presence")` uses non-touch session validation and caps renewal at the existing idle and absolute deadlines. The test performs 59 presence renewals across the original 30-minute idle interval and proves that the idle deadline never moves. A logical renewal operation cannot change between `activity` and `presence` on retry. Server autonomous ticks have no renewal capability.

## Original source provenance

Source pin: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

- `TSOClient/FSO.Server/Servers/City/Handlers/RegistrationHandler.cs`: original CAS creation, zero initial budget, selected appearance and default suit outfit ownership.
- `TSOClient/FSO.Server.Database/DA/Avatars/SqlAvatars.cs`: omitted database-default fields on creation.
- `TSOClient/FSO.Server.Database/DatabaseScripts/fso_avatars.sql`: initial motive/person/job values. The binary motive literal starts with ASCII `0`, which the big-endian signed-short load interprets as `12288`; this intentionally preserves the source quirk instead of substituting generic Spawn motives.
- `TSOClient/FSO.Server/Servers/Lot/Domain/LotContainer.cs`, `StateFromDB`: Epoch-second age/skill-lock conversion, portable appearance/motives/person state, membership/moderation/mayor inputs.
- `TSOClient/tso.simantics/NetPlay/Model/VMNetAvatarPersistState.cs`: source portable person-data field mapping.

New account/avatar creation persists explicit source facts. Admission rejects a legacy row with no source initialization; it never invents empty portable state at export time. Runtime derives role and flags from these inputs plus the exact committed lot metadata. Browser input cannot choose role, actor, principal, portable state, permission revision or private commands.

## Durable boundaries and remaining limits

- Every ordinary controlled commit checks current mode, predecessor, control revision, permission binding, full actor registry, leases and covering tombstones in the same SQLite transaction as cursor, admission receipt, journal and outbox updates. A single captured transaction timestamp prevents crossing an expiry deadline from rolling back the required tombstone.
- Account-to-lot validation is a private cross-object handshake, not a cross-object transaction. The loopback supervisor must drain account fences before grants/ticks and before acknowledging logout; the lot tombstone transaction is the gameplay revocation boundary.
- A lease lasts at most 60 seconds and is capped by account idle/absolute expiry. Presence is browser-originated and does not extend account idle lifetime. The local clock profile rejects authority time over one second ahead and validation older than five seconds; this is not multi-region/provider clock qualification.
- Registry/install scans are bounded to 256 entries; portable state is capped at 64 KiB. The existing journal is capped at 4,096 batches or 64 MiB, with an at-most-64-batch recovery tail and 8 MiB checkpoint. At one accepted batch per 30 Hz tick, 4,096 batches is about 136 seconds. Checkpoint publication does not compact historical receipt/journal retention. Capacity fails closed; sustained gameplay is not claimed.
- Source fence-drain, terminal portable detach, custody release and later new-session/new-lot admission remain unfinished. Expiry/logout preserves custody and pauses ordinary lot mutation; it does not pretend the avatar was detached. Same live session reconnect recovers the existing committed actor without replaying source join.
- V1 initialization/commit/recovery publication cannot mutate a controlled V2 lot. Private V2 recovery wrappers bind the persisted registry, permission metadata and control revision; the normal native takeover marker is activated only after that durable finish.
