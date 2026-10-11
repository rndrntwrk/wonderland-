# Full FreeSO Rust/browser rewrite implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The owner has selected direct implementation in the existing cloud task; parallel read-only review does not transfer implementation ownership.

**Goal:** Complete Wonderland as the full Rust/browser rewrite of FreeSO's expansive playable 3D multiplayer game, economy and creator systems, with the refined action-first UI and source-backed preservation beyond swarms A–F.

**Architecture:** Reuse the merged Rust simulation, content, renderer and client. Cloudflare Containers run the native Rust authority; per-lot and account Durable Objects provide fenced durable acceptance and ownership, with R2 immutable content/checkpoints and D1 directory projections. Small edge/platform adapters do not replace Rust game logic with JavaScript or leave C# as a production gameplay dependency.

**Tech Stack:** Existing Rust 1.99 workspace and WASM client; existing browser rendering/audio components; native Rust host; Cloudflare Workers, Containers, SQLite Durable Objects, R2 and D1; existing compiled-browser tooling extended for real-service E2E.

**Spec:** `docs/superpowers/specs/2026-10-11-playable-wonderland-design.md`. Read it with `docs/integration/FREESO-LINEAGE-20261011.json` and `docs/design/action-first/player-capability-map.md`.

## Global constraints

- Original reference: `riperiperi/FreeSO` at `4c6b3e8f5835b228723caea3c9f683c62f244f73`. Preserved original source is the behavior reference, not a feature-name inspiration.
- Keep the existing 30 Hz deterministic simulation boundary.
- Preserve WLB1/WLC1/WLA1/WLR1 byte meanings.
- All 64-bit identities remain exact decimal strings or native binary values, never JavaScript Numbers.
- First hosted profile: serialized checkpoint at most 16 MiB; entire WebSocket application message at most 20 MiB; SQL journal chunks at most 1 MiB. Verify encoded overhead before enabling.
- Keep the previous two verified checkpoints; unresolved effects and replay-deduplication records cannot be discarded.
- No browser-selected ownership, authority epoch, accepted command, receipt or success callback.
- Preserve original nonredeemable game-currency activities, including casino mechanics. Add no real-money payments, cashout, redeemable wagering or cryptocurrency.
- No silent scope reduction. Disabled/stubbed upstream behavior is documented separately from missing Wonderland implementation. Owner approval is required for any proposed omission.
- Ordinary first play loads a usable, distribution-qualified content release automatically. Illustrations, probes and format fixtures are not production characters or object packs.
- Refined UI remains action-first: character grids, interactive maps, direct world actions and complete contextual controls. Preserve all 21 player-capability rows and additional original-source findings.
- Actual browser journeys and real services are primary evidence. Add E2E/failure-injection coverage, not a new unit-test-count project. Preserve existing regression checks.
- Do not activate unbounded container spawning, new persistent grants or unapproved provider spending. Stage the code and identify exact operational approval needs.

## Review focus

1. A lost reply after a durable commit must recover the same result without repeating spending, spawning or reward effects (Tasks 3, 6 and 8).
2. Tab suspension, control transfer and cross-lot travel must not create two owners of one avatar or lose portable progression (Task 4).
3. A valid content pack can still lack a dependent rig, semiglobal, mesh override or sound; first play must fail recoverably and never switch into fixtures (Task 5).
4. A successful initial 3D frame can conceal broken depth picks, higher floors, occlusion, animation or context recovery; verify actual interaction and camera changes (Tasks 5 and 7).
5. Original registry entries can be stubs, alternate-mode features or operator-configured behavior; neither claim them complete from their names nor omit their dispositions (Tasks 1, 8 and 10).

## Delivery structure and boundaries

This is the overall implementation sequence with independently reviewable deliverables. The twenty concrete source obligations in `docs/compat/full-game-source-obligations.json` map to their owning task numbers and supply required acceptance journeys; none is optional. Complex domains expand into child plans from their specific original handlers before their code changes; the full-game requirement remains fixed. A child plan cannot remove a parent requirement. New code paths and interface names below are proposed, not existing implementations.

Create focused modules rather than another monolithic gateway:

- `crates/native-host-contracts/`: bounded versioned Rust host/coordinator messages and receipt types.
- `services/native-lot-host/`: native executable, content-bound runtime, tentative batching and restore.
- `services/native-edge/src/lot/`: coordinator acceptance, journal, checkpoint and outbox.
- `services/native-edge/src/account/`: sessions, owned avatars, location claims and game-currency ledger.
- `services/native-edge/src/directory/`: versioned D1 projections; never spending authority.
- `services/native-edge/src/activities/`: private durable activity-provider boundary; game rules remain Rust.
- `tools/hosted-game/`: E2E runner, real-service lifecycle/fault driver and evidence collection.
- `docs/compat/game-capabilities.json`: source-to-implementation-to-browser-evidence ledger.

Existing Rust crates and browser components are extended, not forked into a new demo. Native and original-server compatibility modes are explicit. Preserve device-local preview saves.

### Task 1: Establish the full source completion ledger

**Files:** Create `docs/compat/game-capabilities.json`, `docs/compat/game-capabilities.schema.json`, `tools/completion/verify-ledger.py`; extend `docs/compat/dispositions.md` and the existing player capability map only after source inspection.

**Interfaces:** Every `Capability` has a stable `id`, original `source_pin`, `source_paths`, `upstream_status`, `product_behavior`, `rust_paths`, `provider`, `dependencies`, `journey`, `evidence`, `status` and optional explicit `owner_disposition`. Statuses are `unassessed`, `missing`, `partial`, `integrated`, `qualified`; only `qualified` permits completion credit. An unresolved dependency prevents qualification.

- [ ] Extract all active screen, primitive, network command, EOD, service, database/task and creator-editor registrations from the pinned source. Reconcile SDK-default source files and submodule obligations with the existing census; retain unresolved intake rows.
- [ ] Seed every current player-capability row, all 30 registered EOD IDs and each concrete 3D/economic/job subsystem. Preserve source distinctions, including unavailable job branches and TS1 compatibility.
- [ ] Add ledger validation cases: missing source path, duplicate ID, omitted registry member, missing evidence, evidence from the wrong source/content release and unresolved dependency all fail qualification.
- [ ] Run `python3 tools/completion/verify-ledger.py --reference 4c6b3e8f5835b228723caea3c9f683c62f244f73 --mode inventory`. Expected: complete ingestion accounting; missing implementation stays visible. `--mode release` must currently fail with named unmet capabilities.
- [ ] Commit the census and coverage ledger. Do not publish a percent-complete number while intake is unresolved.

### Task 2: Define and exercise the native host boundary

**Files:** Create `crates/native-host-contracts/{Cargo.toml,src/lib.rs,src/batch.rs,src/limits.rs}`, `services/native-lot-host/{Cargo.toml,src/main.rs,src/runtime.rs}`, `tools/hosted-game/{package.json,verify.mjs,driver.mjs}`; register crates in `Cargo.toml` without changing existing wire encodings.

**Interfaces:** `BatchProposalV1` binds lot, epoch, incarnation, batch ID, predecessor cursor/hash, accepted ticks, resulting cursor/hash and stable effect IDs. `CommitResultV1` is `Committed` with exact digest/cursor/hash, `Rejected` with reason, or `Unknown` requiring lookup. `CheckpointManifestV1` binds source/content/protocol, epoch, cursor, object hash/length and outbox watermark. Platform JSON uses decimal-string identities; bounded binary payloads preserve existing accepted-tick bytes.

- [ ] Make `node tools/hosted-game/verify.mjs --suite host-boundary` fail clearly when the native executable and coordinator do not exist; never substitute `tools/native-browser/peer.mjs`.
- [ ] Implement executable startup from a real cooked release and existing `GameRuntime`. Separate committed state from tentative batches; expose no speculative projection.
- [ ] Exercise genuine accepted runtime ticks, changed content identity, oversize input, malformed decimal IDs and same-ID/different-digest batches through the service boundary. Assert no accepted output for denied inputs.
- [ ] Run the boundary suite and existing relevant native-wire regression jobs. Expected: exact source/content identity, unchanged legacy wire vectors, bounded allocations and labelled local-service evidence.
- [ ] Commit only after this deliverable works with the real runtime. Provider deployment remains separate.

### Task 3: Persist accepted lots and prove replacement recovery

**Files:** Create `services/native-edge/src/lot/{coordinator.ts,journal.ts,checkpoints.ts,outbox.ts}`, `services/native-lot-host/src/{coordinator.rs,recovery.rs}`, `tools/hosted-game/suites/durability.mjs`; modify `crates/game-runtime/src/lib.rs` through a narrow authority method.

**Interfaces:** Coordinator methods `commitBatch(proposal)`, `getBatch(batchId)`, `beginRecovery(expectedIncarnation)`, `publishCheckpoint(manifest)` and `finishRecovery(grant, marker)` return versioned contract records. Proposed Rust `GameRuntime::adopt_authority_epoch(new_epoch: u64) -> Result<(), GameRuntimeError>` delegates to validated `SimRuntime::adopt_epoch` without exposing mutable runtime internals. `TakeoverMarkerV1` links old and new epochs/cursors/hashes and the fenced incarnation.

- [ ] Build a real Worker/SQLite/R2 plus native-process durability suite. Fault controls live in the isolated test driver; do not add public production failure endpoints.
- [ ] Implement atomic predecessor/incarnation checks, journal chunks, operation results, outbox and committed cursor in one SQLite transaction.
- [ ] Implement immutable R2 upload/readback before manifest publication, recovery of old-epoch checkpoint/tail and a linked new-epoch anchor. Retry recorded recovery rather than allocating a new epoch on each attempt.
- [ ] Interrupt before commit, after commit before reply, before/after checkpoint publication and during takeover with the old host alive. Assert exactly one committed successor, no premature broadcast/effect, preserved pending IDs, and rejection of stale writes.
- [ ] Run `node tools/hosted-game/verify.mjs --suite durability`. Include corrupt/missing/oversize checkpoint, orphan upload, two retained checkpoints and unresolved-outbox compaction cases. Expected: consistent state and explicit blocked recovery on corruption, never partial success.
- [ ] Commit the qualified boundary and document measured storage/transport overhead.

### Task 4: Connect real accounts, avatars, directory and travel

**Files:** Create `services/native-edge/src/account/{sessions.ts,avatars.ts,location.ts}`, `services/native-edge/src/directory/index.ts`, `apps/web-shell/src/native_account.rs`, `tools/hosted-game/suites/identity-travel.mjs`; modify `apps/web-shell/src/{startup.rs,connected.rs,native_lot.rs}`, existing avatar/city screens and `crates/game-services/src/` contracts as required.

**Interfaces:** Native session grants bind authenticated account, owned avatar, grant version, expiry and content/protocol release. `AvatarLocationClaimV1` binds avatar, claim epoch, active lot and `Active|TransferPending|Detached|Admitted` state. `TravelResultV1` carries a stable transfer ID and resumable status. Native entry never waits for original Aries LotReady.

- [ ] Specify registration/password verification/session rotation/revocation/recovery against the existing account UI before exposing public signup. Pin reviewed password hashing and rate-limiting configuration in this subsystem's security plan; no placeholder auth provider.
- [ ] Exercise distinct isolated accounts creating avatars with independent head/body/skin/gender/name/description and reloading their own persisted roster. Assert another account cannot read private state or control them.
- [ ] Implement server-derived actor admission and fenced source detach before destination ownership. Preserve pending logical operations and committed portable state during travel.
- [ ] Run `node tools/hosted-game/verify.mjs --suite identity-travel`: duplicate same-avatar sessions, stale grant, logout/reconnect, interrupted travel at each durable boundary, return home/city and legacy/preview isolation. Expected: one authoritative controller, resumable transfer and no cloned inventory.
- [ ] Commit the integrated flow after visual review of desktop and narrow touch layouts. New credentials/provider grants are separately approved when actually required.

### Task 5: Deliver automatic content and the genuine playable 3D world

**Files:** Extend `tools/asset-cooker/src/{manifest,packs}`, `crates/render-3d/src/{reconstruction.rs,reconstruction,objects.rs,lot,city}`, `crates/world-view/src/{scene,renderer,lighting.rs,materials.rs}`, `apps/web-shell/src/{avatar_content.rs,native_avatar.rs,world_renderer.rs,source_city_renderer.rs}`; create `tools/hosted-game/suites/content-3d.mjs` and a versioned content provenance manifest.

**Interfaces:** A `PlayableReleaseManifestV1` binds each resource hash, license/permission, dependencies, catalog and runtime compatibility to one release ID. Browser and authority resolve the same cooked identity. Follow the existing cooker/resource contracts rather than introducing an unrelated scene format.

- [ ] Inventory and qualify actual meshes, adult/custom rig requirements, independent appearances, locomotion/contact animations, textures, object behaviors, globals/semiglobals/tuning, routing slots and sound. Resolve public distribution permissions explicitly; do not infer permission from upstream availability.
- [ ] Implement missing sprite-depth reconstruction, FSOM/replacement precedence, cache invalidation, multi-level architecture, room/lighting/cutaway/pool/roof rendering, camera and depth-picking behavior by comparison with the original source. Retain functional hybrid/isometric and full-3D presentations.
- [ ] Auto-load the qualified release during first play. Corrupt/missing dependency or GPU context loss must recover visibly without fixture fallback or data reset.
- [ ] Run `node tools/hosted-game/verify.mjs --suite content-3d`: create avatar, rotate/zoom, change floors/walls, select partly occluded objects, walk and complete an interaction with animation/contact/audio; reload with warm/cold cache and interrupted download.
- [ ] Visually inspect the captured actual game at desktop and mobile widths and on a genuine GPU-supported device. Software rendering evidence is labelled. Commit only the qualified capabilities; remaining original renderer features stay open in the ledger.

### Task 6: Qualify shared-lot actions and durable building/economy

**Files:** Create `services/native-edge/src/account/{ledger.ts,reservations.ts,reconciliation.ts}`, `services/native-lot-host/src/intents.rs`, `tools/hosted-game/suites/play-build.mjs`; extend `apps/web-shell/src/{native_lot.rs,native_build.rs,native_build_browser.rs}` and existing `crates/game-runtime` interaction/build interfaces.

**Interfaces:** Separate `MovementIntentV1` carries operation ID, target/level and observed world revision; identity is server-derived. `EconomicOperationV1` binds immutable consent/digest, account/actor/lot/content/catalog, amount, quantity, persistent IDs and `Requested|Reserved|Accepted|Captured|Installed|NeedsReconciliation|Released|Compensated`. Capture and compensation have stable independent IDs and queried receipts.

- [ ] Implement genuine source-aware routing: existing eligible idle `BeginRoute`, qualified Goto/menu route for active Sims, visible rejection when unsupported; never teleport or fake route completion.
- [ ] Exercise source object menus, queue/cancel, needs, autonomy and other-client projections using two different accounts, then larger measured cohorts. Two clients are only the minimum concurrency witness.
- [ ] Implement reservation → admitted BeginBuild → durable capture/ID allocation → admitted CompleteBuild → committed geometry or reconciliation. Deny state growth beyond the checkpoint profile before economic commit.
- [ ] Run `node tools/hosted-game/verify.mjs --suite play-build`: purchase/place/move, floor edit, visitor denial, host crash and reload, lost capture reply, changed-body replay and post-capture source initialization failure. Assert exact balances/ownership/geometry and no timeout refunds.
- [ ] Qualify sale, donation, upgrade, inventory placement, secure trade, property purchase/move/resize and roommate permissions as separate source-backed journeys before their controls claim success. Each new ledger mutation requires a crash/replay case.
- [ ] Commit a connected preview milestone with an explicit open capability list. This does not complete the game.

### Task 7: Complete simulation, object and world breadth

**Files:** Extend `crates/{sim-core,content-runtime-bridge,avatar-content,avatar-view,audio-content,audio-runtime,render-3d,world-view}`, existing `tools/reference-csharp`, `tools/swarm-a`, `tools/swarm-b`, `tools/swarm-c`; create `tools/hosted-game/suites/full-lot-life.mjs`.

**Interfaces:** Preserve admitted runtime/content contracts; every unsupported source primitive, resource dependency and presentation cohort gets a ledger entry and original behavior oracle. Do not broaden arbitrary callbacks to force content to run.

- [ ] Expand child plans per unresolved source primitive/object/render cohort, using actual original call paths and expected state transitions. Keep differential evidence from a genuine original runtime, not compile-only stubs.
- [ ] Complete motives/autonomy/skills/relationships, containment/multipart objects, room/routing/support behavior, animation/audio and source time semantics across representative complete household/activity object chains.
- [ ] Run `node tools/hosted-game/verify.mjs --suite full-lot-life` for a continuous play session: maintain needs, earn progression, change clothes, navigate floors, use multiple object families, alter architecture and recover after restart.
- [ ] Compare original behavior and browser-visible outcomes; record permitted platform differences explicitly. Commit reviewable cohorts until no required source behavior is left merely decoded or unsupported.

### Task 8: Restore jobs, activities and the complete game-currency economy

**Files:** Extend `crates/eod-runtime` and native Rust activity providers; `services/native-edge/src/activities/`; ledger/progression providers; corresponding browser EOD panels. Create `tools/hosted-game/suites/jobs-activities.mjs`.

**Interfaces:** `PrivateActivityRecordV1` binds session/participants/content, private checkpoint, stable effects and recipient projections. `ScheduledEffectV1` binds source rule, event period, beneficiary and idempotent effect ID; wall-clock policy is separate from 30 Hz simulation. Hidden hands/decks/RNG never enter public lot payloads/hashes.

- [ ] Map every registered EOD handler, public/private message and durable effect to a child plan. Preserve upstream unavailable branches honestly. Original job matchmaking is not generic lot joining.
- [ ] Implement grade/role-matched robot-factory, restaurant and nightclub instances, matching restrictions, career progression, single-object payouts, dynamic tuning, visitor/property bonuses and configured event/gift tasks from source rules.
- [ ] Qualify each activity's real gameplay, participants, cancellation/disconnect, hidden information and durable outcomes, including original nonredeemable casino bankroll/bet/owner flows and secure trade. No real-money conversion.
- [ ] Run `node tools/hosted-game/verify.mjs --suite jobs-activities`: separate private sessions, restart mid-activity, repeated scheduled period, lost payout receipt, unchanged game currency conservation and correct job entry/return travel.
- [ ] Commit cohorts with all remaining registry members visible. A stub handler never qualifies because it opens a panel.

### Task 9: Complete social, city, property and refined UI journeys

**Files:** Extend native account/directory/social providers, `crates/game-services`, `apps/web-shell/src/{screens,components,connected_adapter,source_city_renderer.rs}` and existing styles; create `tools/hosted-game/suites/social-city-ui.mjs`.

**Interfaces:** Commands bind authenticated actor, target and revision; recipient-specific projections separate public profile data from private mail, blocks and permissions. Source ranks, neighborhoods, elections and bulletins use durable period/ownership records, not client-authored counters.

- [ ] Complete all 21 preserved UI rows and newly discovered source surfaces: roster/retirement, wardrobe/racks, maps/search/rankings, owned property, roommate/admit/ban, profiles/relationships, chat/mail/ignore, neighborhood governance and settings.
- [ ] Refine contextual controls without losing actions: direct selection, readable hierarchy, consistent sizing/alignment, touch hit regions, keyboard/focus, reduced motion, dismissal/Back behavior and truthful pending/rejected states.
- [ ] Run `node tools/hosted-game/verify.mjs --suite social-city-ui` across desktop/touch viewports, long text/localization, multiple returned pages, slow replies, repeated clicks, switching avatar and reconnect. Assert private data never appears in another account.
- [ ] Visually inspect every inventoried screen/state. Update the ledger with screenshot/trace links and actual accepted provider receipts, not screenshots alone. Commit independently reviewable journeys.

### Task 10: Complete creator tools, operations and final release acceptance

**Files:** Extend `tools/creator`, `tools/creator-web`, `tools/asset-cooker`, native edge deployment configuration and CI; create `tools/hosted-game/suites/{creator,release-recovery}.mjs` and `docs/releases/full-game-acceptance.md`.

**Interfaces:** Creator edits produce content-addressed cooked releases with provenance and dependency closure. Publishing changes the admitted release only through reviewed promotion. Operations receipts bind source, browser/WASM, native image, Worker, schema and content versions together.

- [ ] Map Volcanic resource editors, BHAV graph/operand editing, live breakpoints/step/watch inspection, mutation/import/export and undo/redo to real browser creator journeys. Developer privileges remain separate from ordinary player authority.
- [ ] Complete moderation/admin tasks, limits, observability, scheduled-service recovery, backup/restore, migration and rollback policies. No browser-accessible admin secrets or unrestricted host spawns.
- [ ] Run creator edit → cook → isolated live behavior → reload/undo/export round-trip and malformed/unlicensed dependency rejection.
- [ ] Run `node tools/hosted-game/verify.mjs --suite release-recovery` against coherent hosted versions: rolling replacement, interrupted schema/content promotion, restore into an isolated environment, stale clients, sleep/wake and preservation of newer committed items/currency.
- [ ] Measure cold load, frame/input response, tick backlog, RAM, bandwidth, recovery time and supported concurrency on stated devices. Publish measured operating limits; resolve failures rather than redefine completion.
- [ ] Run the existing repository CI plus `python3 tools/completion/verify-ledger.py --mode release`. Final acceptance requires every retained capability qualified, all intake/dispositions resolved and the full UI/3D/E2E evidence reviewed. Draft PRs, green builds and the first lot cannot close this task.

## Self-review and execution

- Coverage: Tasks 1/7 close source scope; 2/3 host/durability; 4 identity/travel; 5 content/3D; 6 ownership/build; 8 jobs/EOD/economy; 9 full refined player UI/social; 10 creator/operations/release.
- The five review-focus risks each have an explicit E2E owner above. The separate source audit contributes concrete 3D modes, shop/property trading, community governance, object wear, payout scheduling, private activity and live-editor cases to their owning tasks.
- Detailed auth policy, unresolved original cohorts and schema-specific protocols require their source-informed child plans before implementation; this document does not invent completed designs for uninspected systems.
- Execute directly in the existing cloud task, maintaining one integration owner. Parallel read-only audits and independent project work are permitted; no competing Wonderland deployment writers.
- Commit and publish each reviewed increment through Git, retaining source and evidence durably. Reconcile the current base before changes. Keep the complete-game ledger open through every intermediate release.
- Owner review of this implementation sequence is required before new-host code begins. The owner has already set the full-game outcome and cloud execution method; do not ask them to choose those again.
