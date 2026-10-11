# Task 3 — Actual A/B runtime integration

Status: implemented and ready for the root-owned fresh native/WASM gates. This
report distinguishes the working local accepted-tick adapter from the separate
original network/snapshot translation and unavailable source providers.

## Ownership and imports

Work was performed on `feat/playable-world-integration` in the shared checkout.
No branch switch, push, subagent, root manifest replacement or root lock rewrite
was performed. Root registered the crates and resolved the shared lock.

Imported source pins:

- A simulation `8a0e251d19e222a0a6833d7408ca629f674e1729`:
  `crates/sim-core` and its tests; source compatibility documentation under
  `docs/swarm-a`.
- B `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7`:
  `crates/content-ir`, `crates/content-runtime-bridge`, its separate interaction
  module integrated under the actual `sim-core`, creator/asset-cooker tools,
  normal Cargo interaction/content/tool integration tests, fixture pack data,
  `docs/compat/tools.json` and source bridge/OBJf notes.
- Existing hardened `legacy-formats` retained. The semantic root has only the
  nine-line OBJf registration/export/dispatch addition; the B OBJf implementation
  and tests were added without overwriting current reader hardening.

Shared serde/serde_json/sha2 pins were made compatible with the root workspace;
bincode 1.3.3 and the existing fixed-width little-endian snapshot encoding were
retained. Nested bridge workspace isolation was removed. Source test harnesses
now use the actual imported interaction module, not a parallel production stub.

New crate: `wonderland-game-runtime` (`crates/game-runtime`). Dependencies are the
real sim-core, content IR/bridge, hardened legacy reader, shared world-view and
render-core plus serde/serde_json. The bridge's creator-debug feature is disabled
for the runtime dependency.

## Exact public adapter

```rust
GameRuntime::new(ContentSet, LotModel, RuntimeConfig, RuntimeRole)
    -> Result<GameRuntime, GameRuntimeError>
GameRuntime::from_imported(ImportedContent, LotModel, RuntimeConfig, RuntimeRole)
    -> Result<GameRuntime, GameRuntimeError>
GameRuntime::from_cooked(LoadedRelease, LotModel, RuntimeConfig, RuntimeRole)
    -> Result<GameRuntime, GameRuntimeError>
GameRuntime::sim(&self) -> &SimRuntime
GameRuntime::advance(&mut self, Vec<AcceptedCommand>)
    -> Result<TickOutcome, GameRuntimeError>
GameRuntime::apply_accepted(&mut self, &AcceptedTick)
    -> Result<TickOutcome, GameRuntimeError>
GameRuntime::offers(&self, PrincipalKey, EntityRef, EntityRef, QueryOptions)
    -> Result<OfferBatch, GameRuntimeError>
GameRuntime::invoke(&mut self, InteractionIntent)
    -> Result<TickOutcome, GameRuntimeError>
GameRuntime::cancel(&mut self, CancelIntent)
    -> Result<TickOutcome, GameRuntimeError>
GameRuntime::preview_build(&self, &BuildIntent, &BuildAuthority)
    -> Result<BuildPreview, GameRuntimeError>
GameRuntime::projection(&self) -> RuntimeProjection
GameRuntime::world_document(&self, &WorldDocument)
    -> Result<WorldDocument, GameRuntimeError>
GameRuntime::snapshot(&self) -> Result<Vec<u8>, GameRuntimeError>
GameRuntime::restore(&mut self, &[u8]) -> Result<(), GameRuntimeError>
```

`sim_core`, `content_bridge` and `world_view` are real-crate reexports.

Additional actual A seams:

- `SimRuntime::query_behavior_state(entity, routine, context, args, budget)`
  returns detached source `SimState`, `VmThread` and instruction count.
- `SimRuntime::interaction_offers` and `SimRuntime::preview_build` are real A
  methods used by the adapter.
- `AcceptedCommand::{SetInteractionAuthority, QueueInteraction,
  CancelInteraction, BeginBuild, CompleteBuild}` are appended command variants.
- `RuntimeEvent::{Interaction, BuildRequested, BuildCompleted,
  BuildRuntimeFault}` report actual transitions and durable-boundary state.
- `ContentSet::{with_interaction_tables, with_interaction_advertisements,
  with_build_catalog}` bind immutable source data into the integrated descriptor.

## Source behavior that now executes

The real A `RuntimeHost` implements B's owned `WorldProvider`, `CheckTreeProvider`
and `QueueRuntime` requirements. There is no timed fixture reply or alternative
simulation loop.

Detached checks validate live generations, clone the writable simulation state,
execute original BHAVs with the original interpreter, retain resulting entity,
register, RNG, action-string and advertisement state in the detached result,
and do not dispatch external effects. B's distinct out-of-tick register restore
semantics are retained. In-tick queue rechecks install legitimate source writes,
register/RNG changes, and count real instructions against the same tick budget.
Intent verification's occupancy exception is kept out of in-tick checks.

Offer authorization is an explicit principal-to-live-actor grant. Hidden access,
world revision, entity revision, generations and queue revision are checked.
Actor facts come from actual avatar/person data and world ownership. A menu is
not an executable token: accepted invocation runs the B verifier again.

Source local-table absence differs from an empty table. Global interaction
definitions are now imported from the first global TTAB and the first global
TTAs independently, as `VMContext` does. They are appended after locals. Full
TTAIndex, labels, flags, permissions, routine bindings, every action-string
variant/Param0 and original base motive ads are retained. Float attenuation bits,
autonomy threshold and joining index are preserved. `GetRoutineWithOwner` retains
the object's code owner for every BHAV scope; global/semiglobal selection is
carried by the resolved routine rather than a guessed replacement owner.

Accepted actions push real interaction frames. IdleForInput, AttemptPush,
GosubFoundAction's active-callee view, PushInteraction and ChangeInteractionIcon
reach the B queue. Active cancellation marks source state, notifies the original
idle primitive and wakes the scheduler. A pending cancellation removes the queued
entry without running its action. Source motive-change clearing preserves A's
fractional accumulator semantics.

Frame completion now has an explicit per-instruction host callback. It executes
before a RunImmediately parent can continue and is independent of the bounded
128-entry diagnostic history. Strict connected checks/actions cannot turn an
unknown opcode or missing routine into success. Legacy standalone trace behavior
outside that boundary is preserved.

## Needs, world and build projection

The runtime projection includes actual generations, source GUIDs, persistent
IDs, poses, lifecycle/thread state, signed motives, visible queue entries and
semantic architecture. Needs use `clamp((raw + 100) / 2, 0, 100)`. Room is `None`:
the inherited A room-score constant has no connected original environment
provider and is not presented as a live room need. TSO avatar spawning atomically
rejects absent original `TsoMotiveTuning`; no replacement tuning matrix is made.

`world_document` consumes an already validated appearance document. It projects
source terrain, floors, wall sides/diagonals, room/support state, dynamic flags,
lot/epoch/tick/content/architecture revisions and live instance generations.
Game object origins are `(rawX / 16 - .5, rawY / 16 - .5, (level-1)*2.95)`;
avatars keep `(rawX/16, rawY/16, (level-1)*2.95)`. These follow the actual source
VMEntity/VMGameObject distinction. Existing model/material bindings are supplied
by the content owner; absent models, avatar appearance and wall styles remain
explicit diagnostics. An original snapshot record never acquires a fake live
generation through this conversion.

Build previews use A's real geometry, quote, permissions, stale-object guards
and intersection BHAV checks on detached state. Catalog identities map to GUIDs
through source content. Purchase ownership/geometry comes from authoritative
source metadata, not arbitrary client fields. Begin emits the durable request
without changing visible geometry. Matching accepted confirmation commits the
world, creates real purchase entities/threads and initialization, updates moved
entity/world poses together, and removes deleted entities. Receipt duplicates
are idempotent; mismatches fail atomically. A reserved ID can complete even when
an unrelated lower free slot appears meanwhile. If source initialization fails
after a valid durable receipt, rollback preserves an explicit reconciliation
outcome and receipt rather than pretending the charge never happened.

Build permissions/prices and confirmations are trusted server-adapter inputs.
This crate does not authenticate receipt numbers, debit balances or equate
socket writes with durable commits. Multi-tile/contained build edits are rejected
until the complete group lifecycle/placement contract is supplied.

## Snapshot and source compatibility

Simulation schema 2 appends queue/access maps and validates queue ownership,
entry identities, active-frame correspondence, routine bindings and grants.
Validated schema-1 snapshots migrate to empty queues/no grants while preserving
the original entity/thread/RNG/scheduler/effect state. VM instruction events are
transient and do not change the serialized thread layout. Old content without
interaction/catalog additions keeps A's descriptor; integrated interaction,
advertisement and catalog metadata participates in the extended descriptor.

The content bridge's guarded validation workspace was adjusted by exactly 24
bytes for the three added empty ContentSet maps. Its byte-for-byte size test
found this regression in the broader run and passes after the fix.

Original B cooked-release sealing is still checked by B. `GameRuntime` then
extends the sealed source content identity when attaching its runtime interaction
metadata. This change does not reinterpret an old B descriptor as an original
FreeSO network ABI.

## Verification performed

The test-driven sequence recorded behavior failures before implementation:
detached original checks, dynamic variants, accepted queue action/cancellation,
idle dispatch, build admission, 130-action history overflow, original TTAB
advertisement/global import, and the normalized world projection.

Original unchanged execution bytes used by the adapter tests:

| Source resource | BHAV | Verified behavior |
| --- | --- | --- |
| `Casino_2-Tile_Bar_CC.iff` | 4110 | Three source attribute writes; detached purity; real queue completion; 130 repeated actions |
| `fso_christmas_flag.iff` | 4108 | Three source menu strings with Param0 0, 1 and 2 |
| `cursebook_set_permission.iff` | 4107 | Source Random/MotiveChange helper changes real needs |

The object/lot scaffolds for those routines are explicitly authored harnesses,
not claims that those whole objects have complete original dependency closure.
Other tests use source primitive harnesses for idle and unsupported-opcode paths.

Commands executed using the shared low-debug target/toolchain:

- `cargo test --locked -p wonderland-game-runtime --test source_interactions`:
  16/16 passed, including the 130-action regression after its observed RED failure.
- `cargo test --locked -p wonderland-game-runtime --test build_admission`:
  4/4 passed.
- `cargo test --locked -p wonderland-game-runtime --test world_projection`:
  1/1 passed after missing-method RED.
- `cargo test --locked -p wonderland-content-runtime-bridge --test content_import original_table_advertisements`:
  1/1 passed after missing-field RED.
- `cargo test --locked -p sim-core -p wonderland-game-runtime -p wonderland-content-runtime-bridge -p wonderland-content-ir --no-fail-fast`:
  61 test targets; 522 passed, one failed (the exact allocation-buffer accounting
  regression), one ignored. Every other target passed. The failed allocation
  test was fixed and its focused re-run passed. This is not reported as a full
  green run.
- Owned crate formatting and a final `game-runtime` package run are recorded
  below before the owned commit.

Root explicitly took ownership of fresh whole-native/WASM gates and requested
no further broad recompilation due shared disk pressure. This agent did not claim
an unperformed WASM pass. Root also owns final whole-checkout review and any
force-rebuild needed for the shared compilation cache.

## Remaining original-provider gaps

1. `AcceptedTick` is A's deterministic protocol, not original `VMNetTickList` or
   `VMNetCommand`. Original wire parsing/FSOv projection and replay admission are
   independently owned. Original snapshot restore into a complete A live state
   requires original content, metadata and precise generation/thread mapping;
   this adapter does not fabricate those fields.
2. Many original objects require installation-owned global/semiglobal content,
   animation/slot metadata and non-BCON tuning not present in a single checked-in
   object IFF. Missing closure remains a loader error.
3. Autonomy score curves and a live room-environment provider are not manufactured.
   A's autonomy host remains explicitly unsupported until those complete source
   providers are integrated. Raw TTAB scoring metadata is preserved now.
4. Cross-avatar immediate result chooser callbacks remain explicitly unsupported.
   Detached PushInteraction requires a source check-local queue contract and is
   explicitly rejected. EOD disconnect, result callback and lifecycle requests
   are surfaced events; receiving an event does not claim the provider executed.
5. Account/ownership durability, original GotoObject menu variants, Vitaboy
   appearance and unsupported primitive handlers are independent source seams.
   In particular source `UIPieMenu` sends the selected GotoObject action.ID and
   Param0; coordinate-only WalkTo must not invent Interaction=0/Param0=0.

## Final handoff record

- Source implementation commit: `460f5f9` — `Integrate original content simulation and accepted runtime actions`.
- Final `cargo test --locked -p wonderland-game-runtime`: **21 passed, zero failed**
  across the three behavior targets (16 source/queue, four build, one world), plus
  clean crate/doc targets; exit zero. Log: `/workspace/scratch/1a823ef7b5bc/runtime-game-final-tests.log`.
- `cargo fmt --check -p sim-core -p wonderland-content-ir -p wonderland-content-runtime-bridge -p wonderland-game-runtime -p wonderland-creator -p wonderland-asset-cooker`:
  exit zero.
- Staged owned `git diff --check`: clean. A trailing blank line in the imported
  bridge manifest was removed before commit.
- Two historical Python entry points (`test_assembly.py`,
  `test_verification_inputs.py`) were removed from the imported test folder in
  this handoff commit. They depend on B's separate pinned-A scratch-assembly and
  aggregate-oracle tools, which do not apply to the now-integrated runtime and
  were not imported. All normal Cargo test targets and actual current runtime
  regressions remain. The historical scripts remain available in the pinned B
  reference; the root owns fresh integrated workspace verification.
- No compiler process remains running from this agent. Root requested a cache
  cleanup window and owns the remaining full gates. Root reported the combined
  web WASM gate already passed; this report does not present that as an
  independently executed agent check.
