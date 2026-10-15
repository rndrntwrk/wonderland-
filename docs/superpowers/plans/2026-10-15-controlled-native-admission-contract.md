# Controlled native admission and commit contract — review draft

Status: proposed implementation contract, 15 October 2026. **Review before changing authority semantics.** This document adds no implementation, protocol capability, public endpoint, or deployment. It refines Task 4 of the approved full-original-game design and Increment 2/3 of the existing native-avatar-custody plan. The complete Rust/browser FreeSO scope remains unchanged.

## 1. Baseline and overlap reconciliation

Local baseline: `e9bf14be21bd2d34045b60298e904e4459e9b8b7` (account interruption fix). Its parent tree matches published PR75. Do not publish this plan as part of that UI fix without separate review.

Read-backed GitHub open-PR inventory on 15 October 2026:

- [PR74](https://github.com/rndrntwrk/wonderland-/pull/74): exclusive session-bound avatar reservations, pending-fence discovery, retained lot tombstones, placement and named-entrypoint replay helpers. Its description explicitly leaves controlled commits, detach and portable actor admission unfinished. Reuse these implementations.
- [PR75](https://github.com/rndrntwrk/wonderland-/pull/75), head `6dc9dbe1a67e0d0aede1ad77a54fa307660fcfa3` at inspection: native account-ready HTTP/UI only. Preserve it; no legacy LotReady fabrication.
- [PR54](https://github.com/rndrntwrk/wonderland-/pull/54): separate MySQL/MariaDB ownership-claim adapter, qualified real database tests. Its same-transaction fencing invariant is relevant, but it is not the Cloudflare coordinator and must not be installed as a second authority for the same avatar.
- [PR51–53](https://github.com/rndrntwrk/wonderland-/pull/53): existing quote/consent/BeginBuild/CompleteBuild and stable outcome contracts. Reuse them; a new actor grant must not reset pending financial operation IDs or claim construction is durable merely because admission succeeded.
- [PR56](https://github.com/rndrntwrk/wonderland-/pull/56): integrated GPU/native player/Buy-Build/recovery. Its controlled authentication and RAM accounting evidence does not supply this account-authorized host lane. Preserve rendering, poses, terrain, audio and construction clients.
- [PR60](https://github.com/rndrntwrk/wonderland-/pull/60): full original-game design. Its scope and source obligations remain the completion contract.

The open-PR search found no separate published controlled-host implementation. This does not recover the previously unavailable later continuation; compare any recovered source by hash/diff before implementing overlapping code.

Ownership: Swarm E owns account/location/coordinator persistence; Swarm A owns deterministic simulation and portable avatar behavior; Swarm F reviews shared contracts, source parity and qualification; the client owner integrates actual native bootstrap only after these contracts pass. Record this cross-lane change before simultaneous shared-interface edits. Existing `docs/swarm-f/TAKEOVER.md`, `docs/swarm-e/OWNERSHIP_CLAIMS.md`, `docs/contracts/boundary-map.json` remain authoritative maps, not evidence that the missing lane already exists.

## 2. Existing code to extend

- `services/native-edge/src/account/authority.ts`: `reserveAvatarControl`, `observeControl`, session revocation and pending-fence sweep. Reservation is deliberately not an admitted actor.
- `services/native-edge/src/account/control.ts`: exact decimal avatar/lot IDs, releases, generation and reserved/fence_pending states.
- `services/native-edge/src/lot/{control,coordinator,contracts,recovery,journal}.ts`: tombstones, transactional accepted journal/outboxes, checkpoint/recovery. `commitBatch` currently does **not** consult control tombstones.
- `crates/native-host-contracts/src/lib.rs`: V1 Inspect/Prepare/Resolve/Lookup/ActivateRecovery only; no actor admission or control revision.
- `services/native-lot-host/src/runtime.rs`: tentative cloned runtime, ambiguous-result hold, exact resolution and recovery. Preserve these mechanics.
- `crates/game-runtime/src/live_wire/player/admission.rs`: `revalidate_action` assumes an authenticated principal and live exact actor; it cannot create either.
- `crates/sim-core/src/avatars/state.rs`: `AvatarPersistState`, `persistent_state`, `apply_persistent_state`. Reuse original outfit resets and portable data, not generic Spawn defaults.

## 3. Authority and identities

All calls described below are private service-binding/supervisor calls. A public browser may request an owned avatar and directory-selected destination through a later bounded adapter; it may never supply credentials for these private calls, arbitrary commands, actor/principal, incarnation, lease deadline, permission, portable state or a successful receipt.

Define `AdmissionKeyV2 = (authority_id, avatar_id, control_generation, admission_operation)` and an immutable canonical request digest. `authority_id` is a configured account authority/realm identity. Avatar IDs remain 1..16,777,215; other native u64 values remain canonical decimal strings in JSON. No truncation or JavaScript Number conversion.

The account reservation's private export binds:

- key, account ID, immutable reservation digest and target lot;
- opaque server session reference and account/session revision; never the raw bearer token;
- content release, host protocol release, portable-state schema/version and exact bytes hash;
- owned-assets initialization version, wardrobe/ledger/progression source revisions;
- authorized permissions and their source revision, not a browser-provided role;
- bounded reservation expiry from the account authority's clock and private provenance.

A lot allocates a unique monotonic `PrincipalKey(u64)` for each controller generation from its own durable counter. It must not truncate a hash or reuse account/avatar ID as a principal. Exhaustion fails closed. The exact `(principal, EntityRef {object_id,generation})` is recorded only after Rust computes and durably commits admission. Runtime actor-ID allocation stays deterministic and source-native. Old principal/actor generations are never resurrected by reconnect.

The reservation export must be revalidated by AccountAuthority before lot installation. This is a cross-object authorization handshake, **not** a cross-object atomic transaction. A simultaneous account revocation is resolved by the lot-side durable fence ordering below. Account-side expiry/revocation never releases custody by itself.

## 4. Durable lot mode and data

Introduce a persisted `controlled_v2` lot mode, selected only on a verified initialization/migration boundary. It survives checkpoints and host replacement. V1 `commitBatch` must refuse all new mutations for a controlled lot; otherwise an old or generic trusted path could omit fencing. Historical V1 receipt lookup remains read-only. Do not silently upgrade existing lots or permit downgrade after recovery.

New bounded tables/records:

- `lot_control_state`: monotonic control_revision, principal counter, mode and registry digest;
- `actor_controls`: key/digest, session binding, source revisions, principal, expiry, state (`installed`, `active`, `fence_pending`, `detached`), exact actor and committed admission receipt when present;
- `admission_operations`: immutable key/digest and durable terminal receipt/rejection; identities retained across host replacement;
- `control_detach_receipts`: exact fence key, committed cursor, actor generation, portable bytes hash, unresolved operation references and state version;
- existing `control_fences` retained and enforced. Terminal tombstones or equivalent high-water marks cannot be evicted while old requests can replay.

Use explicit row/count/byte caps before insertion. Keep the existing 8 MiB host checkpoint, 1 MiB supervisor input and journal proposal/chunk limits; choose admission payload bounds inside them, with tests for actual serialization overhead. Oversize portable state is an explicit blocked admission, never partial/truncated data. No unbounded full-table sweep. Review retention/compaction separately; reaching capacity blocks new work safely.

Every install, renewal, tombstone and detach changes control_revision. Renewal is private, binds the same session/key and current revision, cannot revive expired/fenced state, and cannot overtake a tombstone. Coordinator wall-clock expiry is checked within the commit transaction; include clock discipline in provider qualification. Use a short bounded lease derived from authoritative remaining lifetime; never make it longer by rounding or trusting client time. A network partition expires control rather than granting a new owner. Cross-authority clock skew policy must be explicitly reviewed, not assumed away.

## 5. Versioned host messages and proposal

Keep V1 wire meanings unchanged. Introduce explicit V2 envelopes and exact canonical Rust/TypeScript golden vectors. Append/version deterministic accepted transitions without reordering old command tags. Any changed snapshot format needs an explicit new version and old-version migration/refusal tests.

Private host operations:

1. `PrepareAdmissionV2`: binding, predecessor, expected control_revision, installed reservation key/digest, exact portable-state bytes/hash and server-derived principal/permission source.
2. `PrepareControlledTickV2`: binding, predecessor, expected control_revision and bounded server-resolved intents; no unvalidated browser `AcceptedCommand` list.
3. `PrepareFenceDrainV2`: exact pending fence set/revision and affected registry identities. Rust applies source detach/freeze semantics and emits portable state plus unresolved-operation references.
4. V2 Resolve/Lookup retains V1's Committed/Rejected/Unknown semantics and exact input/digest checks. Unknown remains pending; there is no speculative publication.

Each proposal binds the runtime journal payload, exact predecessor/result, control_revision, actor-registry-before/after digests, touched admission/fence keys, resulting actor/portable receipts, and effects/events. The coordinator verifies metadata and private provenance; Rust remains the simulation authority. A digest proves integrity, not correct source execution. Unknown fields, unsupported versions or conflicting metadata fail before mutation.

The authoritative controller registry must be deterministic checkpoint/replay state, not RAM-only host annotations. Its schema and digest are bound to the checkpoint manifest and journal; recovery compares its exact result with the coordinator registry. It may be added to a versioned host-state envelope only if restore also verifies that its actor/principal bindings exactly match restored simulation interaction_access and live entity generations. Prefer one deterministic runtime representation rather than two mutable sources of truth.

## 6. Admission sequence and concurrency

1. Authenticate in AccountAuthority; reserve existing owned avatar using the existing exact operation/digest path. Distinct sessions/changed-body retries cannot reserve it twice.
2. Export authoritative portable state, owned-asset/source revisions and current session binding. A missing ownership migration or incompatible content fails without fallback/default spawn.
3. Lot coordinator atomically installs the matching reservation unless any retained fence covers its generation, mode is wrong, recovery is pending, capacity is exhausted, or another actor already owns that avatar/key. Allocate principal and advance control_revision once. Exact retries return the same installed record.
4. Trusted supervisor requests Rust's source-faithful admission against the exact committed predecessor and control_revision. Preparation produces no browser grant, event broadcast or durable effect delivery.
5. `commitControlledBatchV2` checks current mode/binding/predecessor/control_revision, reservation identity/digest/state/expiry, actor uniqueness and absence of covering tombstones **inside the same SQLite transaction** that writes admission receipt, registry delta, journal, outboxes and successor cursor. Async hash preparation happens before this final transaction; no cached authorization substitutes for it.
6. Exact already-committed retries return their historical receipt before new-admission checks, but a historical receipt does not authorize a new socket or revive a fenced controller.
7. Resolve the Rust candidate using that exact durable receipt. Only then expose a native bootstrap grant after confirming the account session still authorizes this generation and the lot registry remains active. Socket command checks continue on every message and commit; a post-check revocation is still serialized by fences. Record `admitted` in the account claim with the exact receipt through an idempotent private reconciliation path. If this recording or response is lost, query the same key; do not spawn again.
8. Two concurrent different avatars may prepare against one predecessor; only a matching predecessor/control_revision commits. Losing candidates recompute from committed state using the same logical operation, or remain explicit retryable conflicts. Never transplant an old actor allocation into a new predecessor.

## 7. Revocation linearization and all-tick fencing

The lot's tombstone transaction is the gameplay revocation linearization point. Account logout/rotation invalidates the login credential immediately, marks its claims fence_pending and durably queues fence delivery. It must not report completed gameplay revocation while a lot fence is undelivered. The later native UI distinguishes account signed-out from gameplay detach pending.

Recording a fence advances control_revision and marks its actor pending detach in the same transaction. No subsequent ordinary controlled batch may commit until pending fences have been drained. This intentionally pauses the lot briefly instead of allowing autonomous queued actions from the revoked avatar to progress. Checking only submitted player intents would be insufficient: scheduled interpreter work and delayed callbacks can also mutate state.

Every controlled batch, including no-input simulation ticks and callback/effect completion, must match the current control_revision. A fence installed after preparation makes that candidate stale. The host resolves it rejected and rolls back tentative work; it cannot publish it or "refresh" its revision. A fence-drain proposal may commit only against the exact current pending set/revision; another concurrent fence invalidates and recomputes the drain.

Fence-drain first revokes interaction access, then advances bounded replayable cleanup-progress batches using the source avatar leave lifecycle. The existing request_leave/tick path can require up to 1,800 ticks before forced source cleanup; waiting for one terminal batch would deadlock. Permit only cleanup work and the explicitly modeled dependent scheduling needed for that lifecycle while unrelated ordinary player progression remains paused. Each progress batch has the same durable prepare/commit/resolve and control_revision rules. It preserves the actor while cleanup is pending; only actual terminal removal can produce the portable-state/detach receipt. Do not shortcut this sequence to generic deletion. Terminal fence-drain preserves committed portable state and unresolved logical operations, and records the exact detach receipt atomically with its accepted journal. A raw fence receipt alone remains insufficient for custody release. If source detach cannot finish, retain custody and block affected progression; no guessed success or refund.

Batches committed before the fence remain durable and their logical effects must be reconciled once. Fencing prevents new stale authority mutations; it does not erase history. An already committed outbox effect can complete by its original logical ID, with authorization appropriate to committed-effect settlement, without reactivating the actor or accepting a stale host callback as a new tick.

## 8. Retry, crash, replacement and travel

- Lost prepare reply: query same host operation; no second logical admission.
- Lost commit reply: query durable admission/batch key; Unknown freezes the candidate. Exact committed receipt resolves once; exact absence permits retry under the same operation; conflicting digest halts.
- Crash after commit before grant/account reconciliation: recover the same actor and receipt; fresh credentials may observe status only through explicit ownership checks. Do not mint a second controller.
- Logout during install/admission: whichever lot transaction commits first defines order. A fence-before-install rejects install. An admission-before-fence is durably real but immediately pending detach; the socket grant is not proof that control remains valid. Account custody remains held in both cases.
- Expiry with lost fence delivery: in-transaction lot lease check denies further ordinary ticks and records/queues the equivalent fence-drain obligation. No new ownership generation until durable detach proof. If expiry is discovered during commit, persist its fence/revision obligation and return a successful transaction result containing a typed denial; throwing inside transactionSync would roll back the fence. Reject the candidate outside that transaction, after the durable obligation is established.
- Host replacement: use existing beginRecovery/recoveryBundle/finishRecovery/activate sequence. Rebuild exact runtime and registry, preserve operation IDs, tombstones and principal counter, and drain any newer pending fences before ready/admission. Bind activation to current control_revision; a fence during recovery invalidates an older ready candidate. Recovery receipt lookup alone never reopens control.
- Source-to-destination travel: reserve a stable transfer operation; source commits detach and portable state plus unresolved IDs; account verifies the exact receipt before advancing retained generation/target; destination consumes that transfer once. No destination admission from an account timeout, reservation expiry or fence-recorded-only receipt. Resume the recorded phase after every crash; no rollback by cloning inventory or resetting budget.

## 9. Source-faithful deterministic transition gate

Pinned source is `4c6b3e8f5835b228723caea3c9f683c62f244f73`. Read and differentially execute:

- `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetSimJoinCmd.cs`: server-only join; template/custom GUID; mailbox-relative placement/default fallback; source permissions/community and roommate reconciliation; Apply portable state; register persistent identity; chat/join event; `CT - FSO Player Joined` callbacks and their source ordering.
- `TSOClient/FSO.Server/Servers/Lot/Domain/LotContainer.cs` plus avatar persist serialization: source save/load, progression, relationships, jobs, motives and logout behavior.
- Source registration/owned outfit grant rules already implemented by account authority. Preserve zero starting balance and original owned suits; the runtime budget remains a mirror of ledger truth.

Use existing mode-0 placement, named-entrypoint and `AvatarPersistState` helpers. Do not substitute generic Spawn or JavaScript snapshot patches. Source exceptional/special actors and TS1/Simitone paths stay explicitly inventoried; ordinary account admission must reject privileged identities rather than pretending the entire original feature is absent.

PR74 documents an unresolved callback exception difference: original non-THROW_SIMANTICS check execution retains earlier mutations and returns false, whereas current native host faults reject the staged tick. Resolve this by a separately reviewed source-native transition, or qualify exactly which supported content cohort cannot hit it. A full source-faithful admission claim remains blocked until the complete required source behavior is covered. Do not change global fault semantics incidentally while adding control fields.

## 10. Ordered implementation and acceptance

Each increment begins with failing behavioral tests and is independently reviewed; do not enable public entry between them.

1. **Contract/compatibility:** V2 Rust+TS types, canonical vectors, mode guard, strict bounds and denied V1 mutation for controlled lots. Golden V1/WLB1/WLR1 vectors unchanged. Pure tests are contract evidence only.
2. **Source-native admission/detach:** deterministic transition with portable state, exact actor/principal registry and accepted replay. Real Rust authority/replica/state-hash/snapshot recovery and original-source differential cases. Test callback failure and identity exhaustion explicitly.
3. **Transactional control lane:** install/renew/commit/fence-drain/receipt lookup using actual workerd SQLite and real native proposals. Race a fence between prepare and commit; test autonomous ticks and stale effect callbacks, not only player messages. No fabricated successful admitted rows.
4. **Crash/recovery:** SIGKILL before/after each durable boundary; lost admission response; crash before account acknowledgement; two host replacements; fence during recovery; confirm exactly one actor, one principal generation and unchanged ownership/outbox logical IDs. Wrong registry/checkpoint/release digest must block ready.
5. **Authenticated provider:** private supervisor orchestration and then explicit native browser adapter. Two independent accounts select owned avatars and enter a genuinely shared authority; duplicate-controller refusal, logout/rotation/expiry, reconnect, interrupted travel, and actual actions after restart. Desktop and narrow-touch rendered evidence, compiled release hash and real service identities required. Do not use legacy LotReady, fixed controlled peer actors or RAM wallets as success substitutes.

Keep existing account, durability/crash, host contract/runtime, native player/construction, source-preservation and browser tests. Add tests for changed operation body, reordered delivery/renewal, stale generation, response replay after detach, bounded scans/capacity, cross-account/realm access, source revision changes, and interrupted build settlement during detach. Inventory/release verification continues to expose remaining full-game gates.

### Decisions for contract review before implementation

- Confirm lot-wide pause-until-fence-drain as the first safe model; a more concurrent per-actor model requires proven complete dependency tracking.
- Approve explicit V2 mode/migration and deterministic registry checkpoint representation, including no generic V1 mutation bypass.
- Pin source detach semantics and callback-exception qualification, portable-state byte/count caps, operation/tombstone retention and lease clock-skew policy from measured/current implementation constraints.
- Confirm cross-lane ownership and compare any recovered later source before issuing code assignments.

No production credentials, provider spend, public signup, deployment, merge or new persistent access is requested by this plan.

## Reviewed first implementation slice

Start with controlled-mode initialization, immutable no-downgrade mode state, monotonic fence revision and denial of all V1 mutation/recovery paths for controlled lots. Preserve legacy behavior and historical read-only receipts. Add strict Rust/TypeScript control-state golden vectors and actual SQLite restart/race/overflow checks. Do not add lease installation, simulated expiry, actor admission, grants or public endpoints to this first slice. Expiry-as-durable-denial and cleanup-progress tests belong to the first slice that implements those respective transitions; do not fabricate admitted records to claim them early. This resolves the immediate review findings without changing existing authority behavior.
