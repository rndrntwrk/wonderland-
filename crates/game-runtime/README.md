# Source simulation integration

`wonderland-game-runtime` assembles the actual deterministic A runtime and B content
conversion code. It is independent of browser, socket and DOM APIs. A local
`RuntimeRole::Authority` admits deterministic commands; it does not authenticate
an account, infer server permission or decode a FreeSO VM network packet.

## Public boundary

- `GameRuntime::new(ContentSet, LotModel, RuntimeConfig, RuntimeRole)` accepts
  validated source runtime metadata and geometry.
- `from_imported` / `from_cooked` consume B's original effective-content or
  verified cooked-release result and attach source-order local/global TTABs,
  TTAs names, routine bindings and motive-advertisement metadata.
- `advance(Vec<AcceptedCommand>)` admits a local authoritative tick;
  `apply_accepted(&AcceptedTick)` replays an already admitted A tick.
- `offers(principal, actor, target, QueryOptions)` is detached. Generation,
  world, entity and authority stamps are checked against live state. The returned
  menu is a view; `InteractionIntent` is revalidated before queue insertion.
- `invoke(InteractionIntent)` and `cancel(CancelIntent)` enter the real B queue.
  Checks run on the real interpreter. Actions push real VM frames; completion
  follows frame return, cancellation wakes the source idle primitive, and
  unsupported source behavior faults explicitly.
- `preview_build(&BuildIntent, &BuildAuthority)` runs pure A geometry/quote checks.
  `AcceptedCommand::BeginBuild` emits `RuntimeEvent::BuildRequested`. Only a
  matching accepted `CompleteBuild(ServerBuildConfirmation)` can change world
  geometry. A socket write or queued effect is not a durable confirmation.
- `projection` returns live entity generations, poses, signed motives, display
  needs, queue entries, architecture revision and the complete semantic world.
- `world_document(&WorldDocument)` combines the live semantic world with
  explicitly supplied source appearance for the shared world renderer. It emits
  live generations and source positions, preserves source asset bindings, and
  reports missing models, Vitaboy appearance or wall styles.
- `snapshot` / `restore` use the bounded validated A snapshot protocol.

`GameRuntime::sim()` exposes read-only source state and content for richer
integrations. `sim_core` and `content_bridge` are reexports of the real crates.

## Original-content requirements

The adapter needs the effective local resource, its explicitly identified
semiglobal resource when GLOB requires one, and the needed global resource.
Missing referenced BHAVs are rejected by B's closure validation. Runtime
footprints, placement rules, normalized routing slots, animation metadata,
catalog identity-to-GUID bindings and non-BCON tuning are explicit loader inputs.
An object's filename is not metadata.

TSO avatar creation requires original `TsoMotiveTuning`; absent tuning is an
atomic error. There is no generated motive-decay matrix. The display projection
maps signed motive values to `clamp((raw+100)/2, 0, 100)`. Room is `None` because
the inherited A environment score has no connected source provider; the adapter
does not label that value as a live room need.

Source `VMContext` chooses the first global TTAB and first global TTAs
independently, in source enumeration order. Local absence is distinct from an
empty local table. Routine owner is the target object's code owner even for a
global/semiglobal BHAV, matching `VMEntity.GetRoutineWithOwner`. Raw TTAB motive
ads, attenuation float bits, autonomy threshold and joining index are preserved;
check-time ad changes and every dynamic action-string/Param0 variant remain
separate source results.

## Deterministic semantics

Detached queries clone writable entity, register, scheduler, effect and RNG state;
no query result is installed into the live lot. B intentionally preserves check
writes/RNG between definitions inside the detached query while restoring the
caller's original temporary arrays for each out-of-tick check. In-tick queue
rechecks apply source writes and temporary/RNG changes to the accepted state.
Intent validation's source occupancy exception does not leak into in-tick checks.

Instruction accounting includes original check instructions and action
instructions. Queue completion uses an explicit per-instruction return callback,
not bounded diagnostic history, elapsed time or an optimistic UI reply. The
callback executes before a parent RunImmediately frame continues. Missing
opcodes/routines cannot produce successful connected actions. Legacy standalone
primitive traces retain A's diagnostic behavior outside this strict boundary.

Snapshot schema 2 appends queue and authority state. A validated schema-1 snapshot
with matching old content identity migrates to empty queues and no grants; prior
entities, RNG, scheduling and effects remain unchanged. Content with no added
interaction/catalog metadata retains the pinned A descriptor. Interaction,
catalog and advertisement additions participate in the integrated descriptor.
Cooked B reports retain their sealed B descriptor; `GameRuntime` extends that
identity when binding its source interaction data.

## Build durability

`ContentSet::with_build_catalog` binds source catalog identities to GUIDs. A
purchase preview takes its ownership/footprint/placement data from authoritative
inputs, reserves a generation-aware local identity and validates real geometry.
Independent single-object moves, deletion, architecture changes and purchases
are supported. Multi-tile/contained group editing requires a complete source
group contract and is rejected by this adapter until supplied.

A confirmed purchase creates a real entity/thread and runs initialization.
Confirmed movement updates simulation memory and world pose together. Exact
receipt duplicates are idempotent; mismatches fail atomically. If initialization
fails after an otherwise valid durable receipt, projected geometry is rolled
back and an explicit `NeedsReconciliation` result retains that receipt. The
server's account/ownership provider must reconcile it. This crate never debits a
balance or treats a receipt number alone as authentication.

## Deliberately unclaimed source behavior

This adapter does not translate `VMNetTickList`, `VMNetCommand` or `FSOv` into A
state. These are different protocols with different execution contracts. The
network/snapshot workers own those boundaries. An arbitrary source snapshot is
not passed off as an A checkpoint or an accepted command stream.

Avatar autonomy scoring curves, room environment, cross-avatar immediate result
chooser callbacks, remaining primitives, EOD handlers and external effect
completions need their original providers. Unsupported operations remain typed
errors or explicit requested events. Projecting a menu or frame is not evidence
that every original object can execute. A real object with missing dependency
closure or an unsupported primitive remains unavailable.

## Source pins and verification

- A simulation: `8a0e251d19e222a0a6833d7408ca629f674e1729`.
- B content, interaction and tools: `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7`.
- Original FreeSO source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

`tests/source_interactions.rs` executes unchanged original BHAV bytes from
`Casino_2-Tile_Bar_CC.iff` (4110), `fso_christmas_flag.iff` (4108), and
`cursebook_set_permission.iff` (4107). Their declared object/lot harnesses do not
claim complete playable copies of those objects. Additional tests exercise the
real idle primitive, cancellation, generation fences, instruction accounting,
unsupported behavior, snapshot migration and 130 repeated original actions.

The Rust 1.99 lint maintenance preserves the imported behavior and public data
layouts; sim-core's README records its equivalent style changes. The B release
CLI now reads validated option pairs with fixed-size slice chunks, retaining
the same missing-argument and duplicate-option rejection.

The imported A tests and B interaction/content/tool tests remain part of their
normal Cargo packages. See the task report for commands, actual results and
remaining source-provider gaps.
