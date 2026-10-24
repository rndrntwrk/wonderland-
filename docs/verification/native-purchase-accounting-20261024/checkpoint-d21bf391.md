# Atomic native purchase accounting prerequisite — 24 October 2026

A private account binding can now consume a **committed single-object source purchase request** and atomically retain the existing game-currency debit, one owned persistent object identity and an exact durable financial decision. Successful rows remain **pending geometry**. **Buy/Build stays disabled; no rendered or complete native purchase is qualified.**

Product checkpoint `d21bf39163dbb398ad0b061035acdccfbef80567`, tree `e75ae6e84d91c5995219d608e53e0b9fb2dd8a92`, follows local document checkpoint `2d2bd5a7`. Public delivery overlays only this unit on draft #86 at `59ef8af8f2821ab8aa15a24ca01a4c8496591dd0`, tree `2d78449f74cbb960f02e09e475dd3db960a56f1f`. Later evidence/inventory documents do not change product inputs. [Sanitized evidence](checkpoint-d21bf391-evidence.json) records exact product hashes and private receipt hashes without publishing account stores, raw tokens, private source inputs or recordings.

## Existing source contracts and implementation

`crates/sim-core/src/world/build/mod.rs` already defines `DurableBuildRequest`, `DurablePurchase`, `ServerBuildConfirmation`, `DurableBuildStatus` and `NeedsReconciliation`. `services/native-lot-host/src/events.rs` retains `SupervisorEvent::BuildRequested` as fixed-width event tag 0. Original `VMNetBuyObjectCmd.cs` charges the caller to the system sink before forwarding verified geometry. This increment uses those obligations rather than introducing a browser-priced economy.

`services/native-edge/src/account/purchases.ts` accepts only lot/batch/digest/tick/ordinal identity. It reads through the bound Coordinator's existing `getBatch`/`readBatch`, requires a committed matching cursor and canonical digest, validates the controlled proposal and registry hash, and extracts the matching source actor/owner, immutable permission binding and one-object request. Unknown or mixed event types, malformed/truncated/trailing wire bytes, invalid IDs and oversized lanes reject; no event lane is retired. This narrow decoder is not a general event router.

Inside one SQLite transaction, the account store checks owned avatar initialization, current account/session/control generation, reservation/export identity, content release, permission and account revisions, lease and session expiry. It uses existing `avatar_balances`; a caller cannot supply cost, grant balance or choose an owner. The private binding remains absent from account HTTP and supervisor RPC allowlists. Existing accounts work without a Coordinator binding; purchases fail closed if it is missing.

A successful operation updates the existing saved balance, inserts one `native_owned_objects` row with owner, lot, content/catalog and entity identity, and inserts a canonical retained receipt. A rejected insufficient-funds request retains zero charge and no object. Receipt/sequence/object/debit changes are one transaction. A stable key of lot, avatar, control generation and source operation deduplicates retries; a changed request under that identity rejects. Re-emission under a later incarnation/authority epoch returns the original decision. The first response and retries deserialize the identical canonical retained JSON. Prior committed decisions can be redelivered after logout, while new operations still require current authority; there is no public receipt endpoint.

Allocation is **uninitialized by default**. A private, immutable empty-cohort intake must explicitly pin the allocator scope, source high-water and provenance-receipt hash. It rejects existing rows and changed intake, reserves avatar IDs and the system sink, and bounds IDs to the original persistent-object range. This pins a trusted operator assertion; it does not independently verify a global allocator or migrate existing world objects. No actual hosted/production intake was performed.

## Actual qualification and limits

- **Nine real workerd/SQLite groups pass:** atomic debit/object/receipt; eight parallel exact retries with one debit; replay across incarnation/epoch; caller-price/digest/changed-operation rejection and stale frozen export refusal; durable insufficient-funds rejection; wrong source/owner/actor/revisions and invalid charge/catalog rejection; injected late SQL failure rolling back balance/object/sequence/receipt; real dedicated-process SIGKILL before commit and after commit before response followed by cold restart/exact retry; and revocation blocking new operations while preserving prior delivery. Four successful pending objects and five total decisions are retained in that disposable test store.
- **Six contract cases pass:** strict canonical references; exact source-format projection; every truncated prefix/trailing byte; mixed/unknown/order/count/byte bounds; missing bound journal; and both new methods denied by the actual supervisor dispatcher without invoking authority.
- **Six existing saved-assets SQLite groups and eleven account HTTP groups pass.** The same-origin unknown-route check includes both new purchase methods and returns 404. This preserves prior saved wardrobe, roster and creator behavior. The purchase runner's separate cross-origin local request returns 403 and is not misreported as a route-absence check.
- New-file formatting and diff whitespace checks pass. The type-check attempt still reports only the existing generated `account_crypto.js` declaration **TS7016**; no full type-check or repository-wide formatting pass is claimed. Final second-pass formatting changed whitespace only. Review is by the sole implementer; no independent worker/review is claimed under the no-extra-agent instruction.

The journal writer and funding/fault controls are **test-only**. Requests are hand-encoded from the reviewed Rust source layout, then seeded into the actual Coordinator's journal tables. Existing `getBatch`/`readBatch`, account authorization, crypto and SQLite transactions execute. This does **not** prove a fresh native runtime produced the quote or consumed the confirmation. The retained confirmation is a lossless JSON projection with exact decimal IDs, not an encoded authenticated Rust completion command.

The original missing-method failure, first-response canonical-key-order failure, and sandbox loopback EPERM attempt are preserved. Canonical first-response handling was corrected before the passing runs. The supported elevated loopback-only run passed; no browser security or network configuration was changed. No large build, dependency download or installation occurred.

## Reproduce the focused checks

From `services/native-edge`, with the already installed pinned dependencies and generated account crypto:

```sh
node tests/purchase-accounting.mjs
node --test tests/purchase-contract.test.mjs
node tests/account-assets-http.mjs
node tests/account-http.mjs
node node_modules/typescript/bin/tsc --noEmit --allowImportingTsExtensions
```

The final command currently fails with the recorded baseline TS7016. The purchase crash test binds only loopback, creates disposable local stores and kills only its own dedicated process group; no process inventory or real account is used. Source tests are public; private raw receipts are hash references, not accessible download links.

## Next source acceptance before activation

1. Execute the actual native producer and encode/authorize the Rust confirmation lane, including multi-event routing without dropping unrelated events. Current mixed lanes deliberately fail closed.
2. Connect completion to source geometry, retain `NeedsReconciliation`, prove crash/replay across the financial/geometry boundary, and implement correct reconciliation/refund before retiring requests. No acknowledgement is fabricated by this unit.
3. Reconcile source account/export revisions and budget after a debit. Today the prior frozen admission export deliberately fails closed; repeated live purchases or departure/save integration cannot silently reuse it.
4. Establish sole source allocator ownership, real high-water/intake evidence and any existing-world inventory migration; then qualify actual content/ownership inventory and rendered purchases before enabling Buy/Build. The private pending table is not a carried/placed/saleable inventory UI.

These are source integration tasks, not new access permissions. The broader game and real-object needs/content remain unfinished. [External prerequisites](../../integration/EXTERNAL-PREREQUISITES-20261024.md) retain the separate content, R2, startup identity/data, cost and Linux capacity blockers.

## Disk capacity

The current volume has about **3.8 GiB free**. Focused local tests completed; no native/WASM/browser rebuild or Linux builder installation was attempted. Candidate **task-generated reproducible compiler outputs**, requiring explicit cleanup approval, are `optimized-source/target/release/deps` (691,776 KiB), `optimized-source/target/wasm32-unknown-unknown/release/deps` (698,728 KiB), and `optimized-source/target/release/build` (37,880 KiB). Removing these would require later recompilation from retained inputs. No deletion occurred. Source, private assets, evidence, recordings, the original staged checkout, dependency registry, frozen native qualification binaries and browser dist are excluded.

No real money, provider transaction, new agents, grants/credentials, upload, install, spending, merge or deployment occurred. Prior source and private inputs remain intact.
