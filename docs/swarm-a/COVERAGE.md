# Swarm A implementation coverage and remaining acceptance gates

Source baseline: **FreeSO 4c6b3e8f5835b228723caea3c9f683c62f244f73**.
Scope: **W02.1–W05.4, SL.1, SL.4, and SL.9**, as defined in the supplied
FreeSO-Parallel-Work-Packages.md. This ledger maps all 19 requested tickets to
the implementation that exists. It does not mark the cross-swarm work-package
checkboxes complete.

The branch contains a working headless Rust simulation package and local
providers. Full acceptance also requires the predecessor contracts, imported
resources, deployed authority services, and the named reference/content cohorts.
Those dependencies are distinguished below. The package's internal interfaces
are provisional until F adopts or adapts them into the shared contracts.

Paths beginning src/ refer to crates/sim-core/; bare Rust test filenames refer
to crates/sim-core/tests/. Paths beginning tools/ are repository-relative.
C# anchors are relative to the pinned FreeSO repository. This document records implementation scope and executable
evidence locations; final command results, source revision/digests, and target
matrix results belong in the branch verification report. No aggregate test
count in another document should be interpreted as a complete-content count.

## How to read the evidence

- **Implemented mechanism:** executable Rust code exists and a named fixture
  exercises its state transitions. A handler using a host interface may still
  require a B/E provider for a real game object.
- **Source-derived fixture:** expected values were obtained by reading the
  pinned C# source. This is useful evidence, but does not mean the full original
  VM was executed.
- **Extracted-source reference:** the native Mono harness compiles the stated
  original classes/methods with disclosed shims. Its evidence stops at that
  extraction boundary.
- **Native/WASM replay:** the actual Rust runtime executes on both targets and
  its subsequent state is compared. Agreement between those Rust builds does
  not prove agreement with the complete C# engine.
- **Integration gate:** work still required before the package or shared slice
  meets the original acceptance statement. An explicit request or unsupported
  response is a boundary, not a completed service or approved retirement.

Detailed source ledgers are [numeric-source-notes.md](numeric-source-notes.md),
[vm-source-notes.md](vm-source-notes.md),
[runtime-memory.md](runtime-memory.md),
[world-source-notes.md](world-source-notes.md), and
[avatar-source-notes.md](avatar-source-notes.md). Snapshot and effect protocols
are described in [snapshot-contract.md](snapshot-contract.md) and
[effect-boundary.md](effect-boundary.md).

## W02 — Simulation substrate, interpreter, and continuations

| Ticket | Actual mechanism and evidence | Remaining acceptance gate |
|---|---|---|
| **W02.1 — Typed state, IDs, and arithmetic** | src/state.rs defines SimState, immutable ContentSet, content/tuning descriptors, and limits. src/ids.rs separates signed ObjectId, PersistentId, and generation-aware EntityRef; allocation chooses the smallest free positive ID. src/numeric.rs implements wrapping, narrowing, masked shifts, C# division/remainder/modulo policies, rounding, and pinned floating conversion behavior. src/rng.rs implements the source xorshift stream and entity-count mix. Tests: numeric_reference.rs, runtime_tick.rs, and allocator/snapshot adversarial cases in snapshot_contract.rs. The extracted expression/RNG probe is tools/swarm-a/reference-numeric.cs. | F must adopt the local types into W00.2 contracts and run the full cross-target numeric/reference corpus. Exhaustion, generations, unordered collections, and platform-dependent casts use the explicit policies below. Local arithmetic coverage is not a claim that every source floating-point path is equal on all platforms. |
| **W02.2 — BHAV control flow and memory** | src/vm/bytecode.rs, frames.rs, interpreter.rs, and memory.rs decode bounded BHAV bodies, retain the eight operand bytes, resolve global/private/semiglobal calls, preserve caller/callee/code owner/raw stack ID/cached generation, and execute branches, nested calls, waits, and resumes. src/runtime_memory.rs supplies real entity/world/avatar/clock/tuning storage and execution-thread views. Tests: vm_behavior.rs, vm_memory.rs, runtime_memory_reference.rs, primitives_basic.rs, runtime_review_regressions.rs. | B must import actual W01 BHAV/OBJf/OBJD/tuning/string/neighborhood resources into the normalized content interface. Some host memory families remain explicit unavailable providers; a typed address is not imported source data. Full interpreter differential traces and the shipped object corpus remain F/B gates. |
| **W02.3 — Scheduling and lifecycle** | src/clock.rs and scheduler.rs provide 30 Hz semantic ticks, mode-specific game minutes, sorted object dispatch, interrupts, future wakes, entity-count RNG mixing, and deferred deletion. src/runtime.rs owns accepted tick validation, generation checks, initialization/main/reset, VM dispatch, avatar timing, budgets, atomic rejected ticks, and replica effect suppression. Tests: clock_reference.rs, scheduler_reference.rs, runtime_tick.rs, runtime_semantics.rs, runtime_review_regressions.rs. The original-source lifecycle/scheduler harness and exact vectors are linked below. | This runtime deliberately changes single-wake scheduling, initialization/check execution, runaway/fault handling, and calendar reconstruction. B/F must test real init/main/reset/exit and deletion hooks against the reference corpus. E must supply authenticated, ordered accepted commands; constructing AcceptedTick does not authenticate an actor. |
| **W02.4 — Snapshot codecs and restore** | src/snapshot.rs encodes a versioned post-tick envelope with lot/epoch/tick/content/tuning identities, lengths, canonical payload, and checksum. It bounds decoding and validates allocator/entity/thread/world/avatar/containment/continuation relationships before atomic restore. Tests: snapshot_contract.rs, runtime_tick.rs, VM mid-call/sleep cases, runtime_routes_source.rs, runtime_avatar_integration.rs, and the replay harness. | **Legacy FSOv conversion is not implemented in this branch.** B/F must add the legacy-format conversion and compare the subsequent execution tail. E/F must coordinate snapshot publication with accepted-command/outbox/journal positions, lease changes, private-state exclusion, and recovery. SL.9's real durable-provider and full cross-target continuation gate remains separate. |

## W03 — Primitive implementations

| Ticket | Actual mechanism and evidence | Remaining acceptance gate |
|---|---|---|
| **W03.1 — Arithmetic, memory, and flow** | src/primitives/arithmetic.rs, flow.rs, and the elementary cases in entities.rs implement source operators, list aliases, sleep, random, type/distance/direction behavior, and sentinel/control cases through the shared VM ABI. src/vm/memory.rs and src/runtime_memory.rs provide scope access. Tests: primitives_basic.rs, vm_memory.rs, vm_behavior.rs, runtime_memory_reference.rs, numeric_reference.rs. Cases include promoted increment/decrement comparison after a narrowed write, mutation-before-RHS reads, normalized modulo, one-based bit masks, and mode-dependent list/bit operators. | W00.3/F must run the full original VM trace corpus, including imported read/write/test modes and room/level compatibility cases. The small C# arithmetic probe is not the complete VM oracle. |
| **W03.2 — Object, list, and entity primitives** | src/primitives/entities.rs, behavior.rs, relationships.rs, legacy.rs, and fire.rs implement ordered entity lookup/iteration, local and persistent identity rules, slot/drop/create/delete requests, functional/named calls, interaction predicates, relationship storage, source fire propagation, and TS1 budget/inventory cases. src/runtime.rs and runtime_memory.rs apply implemented requests to the shared state. Tests: primitives_entities.rs, primitives_behavior.rs, primitives_relationships.rs, primitives_fire.rs, primitives_legacy.rs, and runtime_review_regressions.rs. | B owns real interaction queues/active actions and content-specific creation, callbacks, neighbor, and persistence paths. Unsupported Create/SetToNext cases remain explicit. TS1 neighborhood import/save and special character creation are not supplied by the normalized in-memory projections. Real multipart object cohorts and lifecycle traces remain required. |
| **W03.3 — Architecture and route-facing primitives** | src/primitives/external.rs preserves routing operand/check guards; src/runtime_routes.rs executes FindLocation, LookTowards, GotoRelativePosition, GotoRoutingSlot, Snap, and Reach against the actual W04/W05 providers. It stages faults atomically, runs bounded isolated entrypoint-5 queries, and records resumable route requests. src/runtime.rs applies movement and callback completion to VM/world state. Tests: runtime_routes_source.rs, primitives_external.rs, primitives_entities.rs, world_placement.rs, world_routing.rs. | The integer router, heading model, and source timing differences are explicit replacements. B must dispatch real portal/sit/stand/shoo/failure BHAVs, bind their source arguments, and return matching callback identities. Runtime-level nested routing through those actual BHAVs, custom SLOT metadata, cross-thread async actor ownership, and the source object corpus remain gates. Direct-control LookTowards and Snap's queued shoo case remain unsupported. |
| **W03.4 — Animation, external, and remaining groups** | src/primitives/external.rs and presentation.rs decode animation, motive, transfer, suit/string/headline, and external operands. src/runtime_avatars.rs and avatars/timeline.rs provide real animation/resource/motive behavior. src/effects.rs and runtime.rs provide typed, serializable pending requests and validated one-time completion. src/primitives/registry.rs reports each source registration's actual status. Tests: primitives_external.rs, primitives_presentation.rs, primitives_autonomy.rs, runtime_avatar_integration.rs, avatar_events.rs, effect_boundary.rs, and registry census in primitives_basic.rs. | **Registry closure is not complete.** RequestOnly entries transport intent but do not implement their handler internals; Partial entries have explicit unsupported cases. B/E/C must supply dialog, generic-call, inventory, job, EOD, sound/effect, and transaction handlers as appropriate. Every declared baseline registration/case still needs actual reachability/lifecycle evidence or an approved retirement. |

### Registry denominator and status limits

The pinned source has **56 registration statements**: 48 common, six TS1, and
two TSO. Overrides produce **50 populated slots in a fresh TSO registry** and
**53 in a fresh TS1 registry**. These are registry counts, not passing-test counts
or implemented gameplay cohorts. The actual source anchors and opcode-by-opcode
ledger are in vm-source-notes.md; the implementation and census test are
src/primitives/registry.rs and
fresh_mode_registries_exhaustively_match_source_inventory in
tests/primitives_basic.rs.

| Fresh mode | Implemented | SourceNoOp | HostAdapter | Partial | RequestOnly | Unsupported |
|---|---:|---:|---:|---:|---:|---:|
| TSO | 6 | 2 | 23 | 9 | 10 | 0 |
| TS1 | 6 | 1 | 27 | 8 | 10 | 1 |

Implemented means the handler algorithm is present; HostAdapter additionally
depends on typed providers and can still return HostUnsupported in the current
runtime. SourceNoOp is restricted to actual source no-ops: BreakPoint and TSO
SysLog. RequestOnly currently includes opcode 1's mode-specific generic call,
PlaySound, SpecialEffect, the three dialog registrations, OnlineJobsCall,
StopAllSounds, InvokePlugin, and TSO InventoryOperations. TS1
MakeNewCharacter is explicitly Unsupported. A complete registry census does
not turn those statuses into completed behavior.

## W04 — Lot architecture, placement, routing, and build

| Ticket | Actual mechanism and evidence | Remaining acceptance gate |
|---|---|---|
| **W04.1 — Lots, rooms, levels, and invalidation** | src/world/lot.rs, rooms.rs, tiles.rs, and invalidation.rs implement bounded levels, finite room connectivity, walls/diagonals, floors, terrain/water/support, portal projections, revisions, and dirty regions independent of meshes. Tests: world_model.rs plus geometry cases in world_placement.rs and world_routing.rs. | B must import actual architecture/object footprints and portal semantics; C consumes the semantic model for rendering. Finite lot boundaries, explicit terrain vertices, wider room IDs, and support behavior are declared replacements. Full reference lots and exact legacy room/terrain compatibility remain gates. |
| **W04.2 — Placement legality and slots** | src/world/placement.rs, footprints.rs, and slots.rs implement stable placement reason codes, floor/wall/height/support/diagonal/pool/slope/bounds predicates, ordered colliders, both intersection approvals, slot capacity, generations, reservations, expiration, and deterministic scoring. src/runtime_routes.rs runs real encoded intersection BHAVs in isolated queries. Tests: world_placement.rs, runtime_routes_source.rs, world_build.rs, and snapshot_contract.rs. | B must provide actual SLOT/OBJD/footprint/script definitions and prove the declared object families. Full source radian/score equivalence, nonzero support-slot semantics beyond the documented first-slot path, custom reach OffsetZ, and full SLOT flags remain open. Reservation tokens and content-level containment must be coordinated by the integrating owner. |
| **W04.3 — Resumable routes and portal behavior** | src/world/routing/mod.rs implements bounded search, deterministic ties, alternatives, retries/waits, collision/shoo callbacks, portal revision fencing, chair/stand outcomes, nested provider routes, source numeric failure values, and saved callback/search cursors. src/runtime.rs binds callbacks to continuation/route/token and applies movement. Tests: world_routing.rs, its internal restore regressions, runtime_routes_source.rs, and standalone-route validation in snapshot_contract.rs. | This is a working integer grid replacement for VMRectRouter/room routing, not the original rectangle/Bézier/turn-animation system. B must execute actual content callbacks and update moved portal endpoint/revision projections. Real nested BHAV routing, full shoo policy, and all stairs/chair/door content cohorts remain gates. The final four-scenario native/WASM matrix includes a synthetic opcode-45 portal route and suspended callback. |
| **W04.4 — Multiplayer build/buy commits** | src/world/build/mod.rs implements pure preview, permission/balance/catalog projections, expected object/architecture revisions, stable operation/digest binding, prepared geometry locks, server-receipt matching, rejected outcomes, duplicate/conflict handling, reconciliation, and versioned compensating undo. Tests: world_build.rs and internal build restore regressions; snapshots retain pending build state. | **No online purchase service or database transaction is implemented here.** E must supply authenticated authority, account/ownership revisions, durable operation/outbox storage, commit fencing, persistent IDs, recovery/refunds, and history compaction. B/runtime must atomically pair committed PlaceObject geometry with VM allocation/init, and coordinate moves/deletes. Actual prices/discounts/refunds/catalogs and live multiplayer acceptance remain open. |

The crate includes an ignore exception for src/world/build so the legacy
repository-wide build-directory rule cannot omit this Rust source. Its preview digest is an
integrity binding, not authentication. A matching locally constructed receipt
is not proof that money or ownership changed in a durable service.

## W05 — Avatar state, animation, autonomy, and social lifecycle

| Ticket | Actual mechanism and evidence | Remaining acceptance gate |
|---|---|---|
| **W05.1 — Identity, needs, skills, and outfits** | src/avatars/state.rs, motives.rs, skills.rs, and outfits.rs preserve source widths, person/motive indices, fractional change gates, TSO/TS1 decay, skill policy/locks, persistent-state mappings, outfit references, pet/worker fields, and leave/reset state. src/runtime_memory.rs and runtime_avatars.rs integrate script writes and resource resolution. Tests: avatar_state.rs, avatar_motives.rs, avatar_source_reference.rs, runtime_memory_reference.rs, runtime_avatar_integration.rs. | B must provide actual curve/tuning/animation/outfit/string/job/neighbor resources and incoming avatar persistence. Missing TSO tuning is an explicit error, not an invented decay table. C must render actual outfits/rigs. Join/load/save and all special-avatar cohorts remain broader acceptance work. |
| **W05.2 — Simulation-driven animation** | src/avatars/timeline.rs and events.rs advance authoritative f32 frames from simulation ticks, preserve source property encounter order and blend forwarding, loop/reverse/hurry, carry behavior, hands/appearance cues, synthetic expected events, and resumable queues. src/runtime_avatars.rs and runtime_routes.rs provide AnimateSim/Reach behavior. Tests: avatar_events.rs, avatar_source_reference.rs, runtime_avatar_integration.rs, runtime_routes_source.rs, and replay scenario avatar-animation-motives. | B supplies the actual animation properties and asset tables; C consumes cues and poses without controlling behavior. Renderer-rate independence for real contact/rig content, licensed animation cohorts, and complete native/WASM/source comparisons remain gates. The saved headless timeline alone does not certify rendered chair/bed/stair contact. |
| **W05.3 — Advertisements and autonomy** | src/avatars/advertisements.rs and autonomy.rs implement eligibility inputs, source scoring/attenuation and f32 operation order, motive-order normalization, stable ties, duplicate first variants, byte-narrowed joins, queued/AutoFirst fast paths, and explicit RNG selection. src/primitives/behavior.rs delegates to the shared scorer and typed enqueue request. Tests: avatar_autonomy.rs, primitives_autonomy.rs, and source-reference score vectors. | **The current RuntimeHost does not implement the B offer/queue provider:** autonomy_context, autonomy_offers, and enqueue_autonomy retain explicit HostUnsupported defaults. B must check TTAB conditions, permissions/carry/repair/availability, revalidate and enqueue the selected live interaction, and test full object cohorts. Pure ranking is not an autonomous gameplay loop by itself. |
| **W05.4 — Multi-avatar and social lifecycle** | src/avatars/social.rs supplies directed relationship state and a serializable participant/barrier protocol with generation/session checks, contention, cancellation, and exact supplied reservation-token release. lifecycle.rs and state.rs preserve source leave/reset/deadline/person/NPC state. Runtime integrates relationships and avatar disconnect/reconnect/leave requests. Tests: avatar_social.rs, avatar_state.rs, primitives_relationships.rs, runtime_semantics.rs. | SocialCoordinator is a provider protocol, not a stored/ticked social registry in SimState. B must bind actual action IDs, queues/BHAVs, participant readiness, death/service-NPC behavior, EOD disconnect, and W04 reservation validity. Full synchronized chair/bed/social action, participant departure/reconnect, and baseline special-avatar cohorts remain open. |

## Shared slices

| Slice | Implemented simulation portion | Remaining slice acceptance |
|---|---|---|
| **SL.1 — Minimal animation/effect primitives** | Real AnimateSim/Reach/motive timelines and VM branch/resume behavior exist. EffectBook and the runtime can issue, save, retry, cancel, fence, deduplicate, and resume typed external requests. Relevant tests: primitives_external.rs, runtime_avatar_integration.rs, runtime_routes_source.rs, effect_boundary.rs; replay scenarios avatar-animation-motives and effect-fenced-retry. | B must freeze the first chair/appliance/purchase corpus and prove every reachable primitive case against actual resources. E must execute the durable effect and return authenticated typed results. A synthetic TransferFunds completion establishes continuation behavior, not a completed purchase. |
| **SL.4 — Purchase and place one object** | W04.4 previews, contention, rejection, prepared locks, receipt matching, duplicate geometry suppression, and committed-but-stale reconciliation are exercised by world_build.rs. | A real native lot-worker, transactional funds/ownership ledger, exactly-once recovery/outbox, real catalog entry, and atomic VM/world creation are absent from this simulation slice. The required insufficient-funds/concurrent-retry/reconnect test must traverse those actual services before SL.4 can close. |
| **SL.9 — Suspended continuation integration** | The actual runtime and providers save/restore calls/sleep, walking and portal callbacks, animation frame/xevt queues, and pending/resolved effects. Tests: snapshot_contract.rs, runtime_routes_source.rs, runtime_avatar_integration.rs, runtime_review_regressions.rs, world_build.rs, plus tools/swarm-a/replay. The replay harness compares accepted-input/state hashes, RNG, instruction counts, events, and raw snapshots against uninterrupted/restored Rust execution. | The final cross-target matrix covers BHAV/sleep/RNG, avatar animation/motives, typed fenced effects, and an actual opcode-45 portal route with a held callback. Its native/WASM comparison passes across four seeds, six portal snapshots, and five malformed/stale callback rejection cases. This closes the synthetic portal target-consistency gap, not real callback-content or source-engine parity. E must add a real committed-but-undelivered durable outcome and coordinated snapshot/journal recovery. Full FSOv conversion and actual content/cohort equivalence remain separate gates. |

## Consolidated compatibility choices

The following choices are explicit and must remain visible when comparing a
source trace or admitting a content cohort.

| Area | Pinned source behavior | Branch policy and practical consequence |
|---|---|---|
| **Scheduler single wake** | Engine/VMScheduler.cs ScheduleTick adds an entity to a requested bucket without deleting earlier buckets. Scheduling the same entity at ticks 2 and 5 leaves both entries; ScheduleIdleEnd records 5. | Scheduler::schedule retains one wake per entity and replaces the earlier entry. Suppressed extra dispatches can suppress primitive RNG draws or change entity counts at the next count mix. The scheduler call itself draws no RNG. scheduler_reference.rs tests the new policy; the lifecycle source harness records the unequal legacy calendar/RNG vector. |
| **Saved calendar and clock width** | Source load schedules restored entities at tick 1 and resynchronizes that bucket; saves do not preserve an exact future calendar. Scheduler tick arithmetic is u32. | Snapshots retain the exact new calendar and resume at N+1 using wider accepted ticks and checked exhaustion. This is deterministic recovery for the new schema, not a literal FSOv decoder. |
| **Deferred deletion order** | VMScheduler.RunTick mixes entity count before iterating an unsorted pending-deletion HashSet. | Count-before-delete is preserved; pending deletions use deterministic sorted order. Cross-object deletion hooks and resulting RNG require cohort comparison rather than an assumption about C# HashSet order. |
| **Initialization and yields** | VMEntity.Init:485–522 runs entry 0 and entry 8 immediately through EvaluateCheck, with IsCheck=true; the check evaluator repeatedly executes at the current simulation time. Main is then pushed. | run_initialization currently pushes these entries on the ordinary entity thread and can retain yielded work across accepted ticks. This changes both timing and IsCheck-guarded primitive behavior. Entry order and one-time main inputs have tests; exact legacy initialization traces remain an explicit W02.3/F gate. |
| **Runaway and exception policy** | VMThread's source exception/reset/dialog path has legacy dispatch limits and can reset or delete repeatedly faulting objects. | Configured entity/tick/query/depth limits, typed VmFault errors, quarantine, and failure-atomic accepted ticks bound work. The new fault policy is not a port of the original UI exception recovery loop. |
| **Source runtime numeric behavior** | Unchecked integer widths are source-defined; out-of-range/non-finite float-to-integer results and DateTime.AddSeconds precision depend on the original runtime/compiler target. | Integer helpers preserve explicit wrapping and narrowing. Floating cast helpers pin the observed Mono 6.8 amd64 policy. SimClock pins legacy millisecond rounding: the first 30 Hz ticks produce 33, 67, and 100 ms, using integer arithmetic and an explicitly supplied UTC origin. Game months remain 30 days; standard-time fields use Gregorian UTC. Other .NET runtime behavior is not silently treated as equivalent. See numeric_reference.rs and clock_reference.rs. |
| **ID safety and fresh mode registry** | Legacy local IDs are signed shorts without generation fencing; static registry reinitialization can retain earlier-mode slots. | Positive allocation, explicit exhaustion, stale/double-release rejection, generation retirement, and per-mode fresh registries are intentional safety policies. Raw signed stack IDs and stale cached references remain representable without rebinding. |
| **Rooms, path geometry, and scores** | VMRoomMap/VMRoutingFrame/VMRectRouter and VMSlotParser use legacy edge, room, rectangle, floating score, turn, and Bézier behavior. | Bounded finite rooms, integer footprint/rotation/heading/SLOT scoring, and resumable subtile graph search are declared replacements. Portal search uses a safe zero heuristic where needed. Exact route shape, duration, steering, and floating score parity are not claimed. |
| **BOTH intersection approvals** | VMContext.cs:1217–1218 and 1269–1270 use short-circuit OR of entrypoint-5 checks. | The supplied W04.2 requirement explicitly requires both objects' scripts. Rust runs both present checks in stable order even when the first approves. A second fault can reject the operation; the difference is tested and remains material to source compatibility. |
| **Preview/query isolation** | Some legacy synchronous checks share VM state and RNG during active ticks. | Placement/build/UI query paths use staged state; query writes, effects, scheduling, and RNG do not enter the authoritative state. Script depth and cumulative work are bounded. Ordinary in-tick functional checks retain their separate source sharing rules. Content relying on placement-query side effects needs an explicit compatibility decision. |
| **Route dispatch and callback seams** | Source routing can discover SLOT choices and run nested callback trees at different points in the same VM dispatch. | SLOT discovery occurs in the requesting host call; route progression begins at its next scheduled step. Callback identity and copied data cross a B boundary, and continuous headings/turn timing are replaced. Per-dispatch RNG is explicit, but whole-VM traces must account for this schedule. |
| **Bounds and atomic restore** | Source readers and collections often assume trusted resources or allow larger/unbounded data. | Finite BHAV/frame/string/map/animation/route/slot/build/effect limits and cross-graph validation reject malformed or excessive state before publication. Non-finite continuation floats are rejected. Extra saved transient avatar/calendar fields and the new snapshot envelope are intentional schema differences. |
| **Durable effects and build history** | Legacy async responses are server-originated but do not provide this branch's namespace/nonce/generation/commit-and-delivery-epoch contract. | Effect IDs survive takeover; typed outcomes apply once to the matching generation. Prepared build receipts, locks, reconciliation, and compensating undo are new mechanisms. E must supply authentication, durable journals, compaction watermarks, and real ledger semantics. Saved mirrors never authorize restoring balances. |

## Reference and replay artifacts

| Artifact | What it executes or checks | Evidence boundary |
|---|---|---|
| tools/swarm-a/reference-numeric.cs | Extracted C# arithmetic/RNG expressions and runtime casts. | Small native probe, not the original VM loop. |
| tools/swarm-a/reference-lifecycle.py, companion .cs and .expected.tsv | Hash-pinned original scheduler/Notify files and extracted entity/frame/check/main-restart methods. Includes source null main stack, consumed creation inputs, reset temp sharing, wake ordering, and count-before-delete. | The fixture BHAV executor and content/renderer/sandbox storage are disclosed shims. Exact compiled methods are listed in [lifecycle-source-vectors.md](lifecycle-source-vectors.md). |
| tests/avatar_source_reference.rs | Rust bit/vector assertions and an explicitly ignored-by-default Mono execution of extracted original motive, animation, and curve source. | The Mono test must actually be selected; an ordinary cargo test run alone does not execute it. It is not the full avatar/content engine. |
| tools/swarm-a/replay/ | Actual SimRuntime on native Rust and wasm32-unknown-unknown, including live replicas and restored authorities, with semantic assertions and raw snapshot/hash comparison. | Synthetic supplied resources, no host imports or real services. The scenarios and reproducible commands are in its [README](../../tools/swarm-a/replay/README.md). Final success requires a fresh run on the final source digest. |

## What remains before cross-swarm acceptance

1. **F — Contracts and reference gates:** adopt provisional local schemas;
   preserve the reviewed source-deviation ledger; complete the original-runtime
   trace oracle and FSOv conversion; extend the verified four-scenario native/WASM
   matrix to actual content and further failures, with versioned artifact evidence.
2. **B — Actual content and interaction providers:** import bounded authorized
   BHAV/OBJf/OBJD/SLOT/animation/STR/tuning/neighborhood data; provide TTAB
   offers/queues, portal/chair/social callbacks, source resource scopes, and a
   named reachable primitive/content cohort. RequestOnly, Partial, and missing
   host cases must stay visible until implemented or explicitly retired.
3. **E — Authority, money, and recovery:** admit commands and completions through
   authenticated native services; enforce lot/actor/epoch ownership; commit real
   accounts/objects with stable operations; coordinate outbox and snapshot tails;
   reconcile historical receipts and define bounded-history compaction.
4. **C/D and integration owners — Real client path:** consume semantic poses,
   animation cues, authoritative placement outcomes, queue state, and pending
   effects without letting renderer/UI callbacks control simulation. Physical
   browser behavior, accessibility, performance, and full multi-user slice
   evidence are outside this crate's tests.

These are concrete acceptance dependencies, not hidden implementations behind
the local APIs. Their completion must be evidenced by the owning lanes before
claiming the shared purchase/interaction/continuation slices or full FreeSO
cohort parity.
