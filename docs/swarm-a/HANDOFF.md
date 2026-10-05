# Swarm A integration contracts

These interfaces are local to `sim-core` version 0.1.0. Swarm F owns final
workspace and shared-schema adoption. Do not expose derived Rust/Serde types as
a network protocol without that review. The implementation keeps the C# source
tree intact and does not change another swarm's workspace, service, UI, or
content-import paths.

## Swarm B: normalized content and interactions

Construct a validated immutable `ContentSet` from effective, patched legacy
resources. The core consumes bounded `RoutineStore` instructions, object
definitions and entrypoint/condition bindings, semiglobal ownership, string
tables, named routines, normalized SLOT data, animation metadata/time
properties, suit resources, legacy job uniforms, and tuning. The content and
tuning descriptors include their canonical data; all replicas must load the
same descriptor before accepting a tick or snapshot.

Preserve source order in animation time-property records. Import the actual
STR/body/walk tables, child/pet identifiers, and TSO motive tuning. Missing
tables are not filled with invented behavior. Animation resource lookup folds
ASCII case and preserves whitespace; conflicting folded aliases reject.
Non-ASCII normalization needs an explicit content policy before expansion.

The runtime currently spawns one entity for one `SpawnSpec`. Multipart groups
are supported in validated state, placement, and deletion, but complete
normalized multi-tile construction and legacy FSOv conversion are still open.
Creating a semantic build object is also insufficient to create its VM state:
the integration must publish both stores atomically with the same allocated
handle and immutable definition.

### Interaction providers

`VmHost` exposes typed seams for offers, queues, autonomy enqueue, interaction
state, named/functional behaviors, idle input, interaction icons, and dynamic
dialog text. The VM implementation decodes and validates operands; absent
runtime providers return explicit `HostUnsupported` faults. B supplies actual
offers and permissions, content-driven action queues, EOD state machines,
private/public data separation, and complete object-family behaviors.

`AcceptedCommand::SetInteractionProjection` carries the admitted active queue
prefix and advertisement projection used by memory/autonomy. It does not
replace B's queue lifecycle. An entity's in-use status is computed from
Occupied flags and real avatar callee frames, including frames temporarily
removed while executing. A temporary condition stack must not erase this view.

Condition checks during scheduler dispatch share the selected entity's short
and XL register banks. Outside dispatch they clone those banks. Named
`RunInMyStack` addresses either the executing thread or the stack object's real
thread according to its destination operand. Same entity identity alone does
not prove these are the same thread. Preserve that distinction when connecting
offer/menu and EOD providers.

### Route callbacks

`RuntimeEvent::RouteScript` carries the continuation ID plus the typed callback
with route ID and token. Run the appropriate content behavior, then submit
`AcceptedCommand::RouteCallback` with the exact IDs and a validated outcome.
The callback wait has a bounded timeout; cancellation or target invalidation
can also end the wait. An earlier event does not prove the callback is still
pending. Late, duplicate, wrong-route, wrong-token, and structurally invalid
outcomes must follow the provider's explicit validation path.

For a failure tree, bind source arguments as `[failure_code, blocker_id_or_0,
0, 0]`. PersonData 62 is written before the failure callback is exposed. Route
failure does not write Temp0. A successful Stand clears sitting posture 1 to 0
and detaches the avatar; other postures are retained. Nested routes remain
owned by the callback's continuation and must not complete the parent script
implicitly.

Exact source rectangle/Bézier movement, per-frame turning, all content-specific
route passability, blocked-avatar Snap shoo behavior, direct-control routing,
and custom Reach height offsets remain covered by the gates in the world note.

## Swarm C: rendering and audio

Read immutable post-tick state and consume deterministic presentation events.
The simulation clock drives animation frames, ordered xevts, synthesized
completion events, hand/head/carry changes, motive updates, and message/headline
expiry. Renderer interpolation, playback completion, visibility, browser
refresh rate, and audio duration must not feed back into authoritative state.

Sound and special-effect primitives emit typed presentation requests. C owns
HIT execution/playback and graphical resources. Avatar outfit and animation
metadata identify the requested appearance; this branch provides no rig,
texture, mesh, or browser rendering implementation.

World dirty tile sets and revisions expose semantic invalidation. Converting
them into sprite depth, geometry, lighting, or picking structures is C's work.
Moving a world object does not infer transformed portal endpoints; B/C/E must
coordinate explicitly imported endpoints and the corresponding revision.

## Swarm D: UI and command producers

Read state and use bounded `query_behavior` for previews. The query operates on
isolated state/RNG and does not return executable external effects. Do not
submit arbitrary serialized `SimState` or treat `next_tick` as authentication.
Submit user intent to E/B admission, which produces the ordered accepted input.

Build previews are pure proposals containing exact geometry, identities,
revisions, cost and script decisions. A preview digest establishes an integrity
binding, not user permission or proof of payment. Display provisional versus
committed state according to the build state machine and eventual E outcome.

## Swarm E: admission, authority, persistence, and durable effects

Only trusted admission may create accepted commands. Validate the session,
lot/lease epoch, permissions, actor/target generations, transaction authority,
and command-specific content before calling `step`. The core then checks the
lot, epoch, content/tuning, tick order, prior RNG, structural invariants, and
execution bounds again. Replicas apply the same admitted input and emit no
durable effect dispatches.

`EffectBook` records a logical operation ID independently of delivery fencing.
Persist its namespace and nonce with the accepted stream. Retries and epoch
takeover retain the same operation ID, target, original issuance context, and
payload. Dispatch under the current granted epoch; reconcile a historical
committed result and deliver it in an explicitly scheduled accepted tick.

`EffectResolved` binds operation, target generation, apply tick, delivery epoch,
committed epoch, and typed result. For VM effects, the bytes decode into the
bounded `VmResolution`; it must select an allowed branch and valid register
targets. The pending continuation additionally binds its opcode, operands,
frame position, captured parameters/locals, and request payload to immutable
BHAV content. Save data cannot substitute a different operation under the same
wait identifier.

Authentication, database money/inventory/ownership transactions, operation
journals, outboxes, exactly-once durable effects, refunds, and private EOD
state are E/B responsibilities. Cancelling a simulation wait does not undo a
database commit. The bounded in-memory terminal cache is not a permanent
deduplication ledger. The core contains optional TS1 family-budget/inventory
projections; these are local legacy VM state, not TSO financial authority.

### Build completion

Validate permissions, account/catalog revisions, exact cost, proposed local
handles, and geometry guards through `WorldState::preview_build` and
`WorldState::begin_build_commit`. Execute the durable transaction once, then
deliver a matching `ServerBuildConfirmation` through trusted admission to
`WorldState::complete_build_commit`. That completion checks receipt bindings
and requires every purchased local handle exactly once, with nonzero unique
persistent IDs. A stale post-commit world produces `NeedsReconciliation`,
retaining the receipt; it does not invent a refund.

Integrate the semantic world commit with VM creation, movement, deletion, and
source lifecycle hooks in the same accepted transaction. That adapter is still
required. Pure preview generation holds no geometry lock. After
`begin_build_commit` prepares an operation, interaction-driven `in_use`/
`for_sale` changes advance guarded object revisions and can reject the update.
Admission must gate those conflicts or implement an explicit reconciliation
policy.
The build outcome history also needs a durable replay-watermark/compaction
policy before continuous production operation.

### Restore and takeover

Snapshots describe the validated state after tick N. Recover the coordinated
accepted-command and durable journal/outbox positions, restore the snapshot,
grant a newer fencing epoch through the external lease protocol, and replay
the admitted tail beginning at N+1. `adopt_epoch` records that granted epoch;
it is not a lease implementation. Existing pending effects retain their
logical IDs and are redispatched with the new delivery credential.

SHA-256 detects corruption and binds canonical state/content. It does not
authenticate a snapshot or prove that its nonce/journal position is current.
Do not restore saved balances/ownership over newer committed service state.

## Swarm F: composition and acceptance

Adopt or adapt the simulation-local IDs, numeric policy, descriptors,
continuations, events, limits, and snapshot schema into the shared contracts.
The local ID is signed i16 and positive when live; zero is null. `EntityRef`
adds a nonzero generation. Persistent IDs and effect IDs serve different
purposes and cannot replace a live-generation check.

Run the pinned reference probes, both package profiles, and the actual
native/WASM replay matrix. Review every intentional source deviation in
[COVERAGE.md](COVERAGE.md) and freeze the accepted capability/content denominator.
Then add full imported-content cohorts, source differential traces, combined
service/VM/build recovery, physical-browser tests, performance gates, and the
32-client shared-lot scenario. None of those gates is closed by a synthetic
fixture count or an opcode-dispatch table alone.
