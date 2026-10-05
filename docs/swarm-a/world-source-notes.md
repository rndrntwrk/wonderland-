# W04 semantic architecture, placement, routing, and build state

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

This lane implements executable headless world mechanisms in `crates/sim-core/src/world/`. It does not claim full legacy object/catalog compatibility, a port of the rectangle/Bézier router, or a completed online purchase service. The tests use the actual Rust world providers, including their serialized continuations, and source-anchored invariants. They are not a differential C# trace corpus.

## Ownership and integration boundary

Owned files are `src/world/**`, `tests/world_*.rs`, and this note. The later runtime integration assignment also owns `src/runtime_routes.rs` and `tests/runtime_routes_*.rs`. The root runtime, manifests, shared contracts, VM/avatar modules, database adapter, network schema, renderer, and content importer remain in their respective lanes. These public Rust types are provisional sim-core integration interfaces for Swarm F to review; they are not a unilateral cross-swarm schema freeze.

All authoritative structs derive serde serialization and equality. Stable iteration uses ordered maps/sets or explicitly stable ordered vectors. Simulation RNG is passed in as `SimRng`; the world module never obtains a clock, OS randomness, renderer callback, network connection, or database handle.

### Primary APIs

| API | Responsibility |
|---|---|
| `LotModel::new(width, height, levels)` | Bounded lot, one-based levels, initial outside rooms and ground support. |
| `LotModel::{set_wall,set_floor,set_terrain_vertex,set_object_support,set_build_bounds,upsert_portal,remove_portal}` | Trusted semantic geometry edits, mirrored side data, room/support recomputation, revisions, exact dirty tile sets. User build edits must pass the build API instead. |
| `LotModel::{room_at,floor_at,portal_graph,revision,dirty,take_dirty,validate}` | Read projections and invariant validation. |
| `WorldState::new(lot)` | World object projection, slot state, and build transaction state. |
| `WorldState::{insert_object,replace_object,move_object,remove_object}` | Live-generation projection changes and object revision tracking. Prepared build locks reject conflicting changes. |
| `WorldQuery` | Immutable lot, object, slot, and revision access for alternative adapters. A callback yield retains copied handles/data, never a live mutable entity borrow. |
| `PlacementContinuation::{new,step,complete_callback,validate}` | Scriptable placement with saved callback cursor, generation/revision guards, stable source numeric errors. |
| `validate_placement(request, query, scripts)` | Synchronous convenience for adapters whose intersection checks are already bounded and isolated. |
| `WorldState::{define_slot,reserve_slot,occupy_slot,expire_slots}` and `SlotState::release` | Generation-aware capacity, idempotent reservation, expiration, consumption, and release. |
| `score_slots` | Bounded integer SLOT candidate enumeration and scoring; accepts an external candidate verification callback. |
| `RouteRequest::new(id, actor, start, goals)` | Route request with source-sized retry/wait defaults and explicit goal alternatives. |
| `RouteContinuation::{new,step,complete_callback_for,push_nested,cancel,validate,validate_against}` | Bounded search, movement, callback/nested continuation, replan, and failure handling. |
| `WorldState::{preview_build,begin_build_commit,complete_build_commit}` | Pure preview, guarded effect request, then geometry only after a matching durable server completion. |
| `BuildState::preview_undo` | Versioned compensating undo preview, requiring a new durable operation. |

`WorldObject::new(entity, position)` creates a semantic projection with a centered `[-4,4)` footprint, north facing, floor/terrain placement, and ground height permitted. Set `rules.is_avatar` for avatars and supply content-driven footprints, flags, entrypoints, and ownership. The runtime remains responsible for the corresponding VM entity and definition.

Coordinates use `LotPosition { x: i32, y: i32, level: u8 }`, with 16 units per tile. `TilePos::center()` adds 8 units on each axis. `LotPosition::OUT_OF_WORLD` is exactly `(-32768,-32768,1)`, anchored to `tso.world/Model/LotTilePos.cs`. The world object store accepts this sentinel and returns no footprint there. Route constructors reject out-of-world actors, starts, and goals. Contained entities can be represented by zero extent at their container's position; the runtime owns containment correctness.

## W04.1 — Lots, rooms, levels, and invalidation

### Source anchors

- `TSOClient/tso.simantics/VMArchitecture.cs`: `SetWall`, `SetFloor`, `OutsideClip`, `GetPreciseFloor`, `SetTerrainHeight`, `RegenerateSupported`, and `RegenRoomMap`.
- `TSOClient/tso.simantics/Model/VMRoomMap.cs`: split room words, diagonal spread restrictions, fence/floor adjacency, outside propagation, and pool classification.
- `TSOClient/tso.simantics/VMContext.cs`: `GetRoomAt`.
- `TSOClient/tso.world/Model/XmlHouse.cs`: `WallSegments` numeric side and diagonal layout.

### Implemented behavior

- Lots have explicit dimension, level, tile-count, and portal-count limits. Levels are independent; a tile on floor two cannot alias floor one.
- Wall sides preserve the source bit layout: west/TopLeft=1, north/TopRight=2, east/BottomRight=4, south/BottomLeft=8. Cardinal sides, room-separator bits, and occupied-wall bits are mirrored on the neighboring tile.
- Each tile has two room cells. Non-diagonal cells are connected internally. Vertical diagonal half zero is selected by local `x > y`; horizontal diagonal half one is selected by local `x+y > 15`, matching `VMContext.GetRoomAt`.
- Separate diagonal half-floor patterns use the same ordering as room cells. Pool and water use source floor patterns 65535 and 65534.
- Flood fill respects walls, diagonal connectivity, and floor category transitions. Fences partition room records while transmitting outside status through adjacency. Portal records bind explicit endpoints and a generation-aware object handle, including directed or bidirectional stairs between levels.
- Upstairs support follows the source algorithm: direct object support or an indoor room below; otherwise a neighboring upper floor enables a 5×5 lower-room check, with object support restricted to the inner 3×3.
- Architecture, room, terrain, portal, and support revisions are separate counters. No-op setters do not advance them. Dirty data is a set of exact tiles for walls, floors, terrain, rooms, support, routing, and build bounds. Disjoint edits do not become one large bounding rectangle. Changing a terrain vertex dirties only its at-most-four incident tiles.
- `LotModel::validate` checks dimensions, array sizes, reciprocal walls, portal shape, build masks, dirty coordinates, regenerated room state, and regenerated support state.

### Intentional model differences / remaining compatibility work

- The source room flood fill wraps lot edges and initially designates the first room outside. This model uses finite boundaries and outside connectivity. It does not silently wrap a route or room into the opposite lot edge.
- Terrain vertices use an explicit `(width+1)×(height+1)` bounded grid. A legacy importer must map the source terrain array and edge conventions.
- Room IDs are deterministic `u32` semantic IDs, not source packed `ushort` serialization. `area_half_tiles` counts a full tile as two and a diagonal half as one. The VM room-information adapter must map source `Area`, zero-based `Floor`, and the minimum adjacency-component light base room. Source `IsPool` means either water or pool; the semantic model exposes these separately.
- Light meshes, wall shadow lines, cutaways, ambient-room scoring, roofs, and source room marshal bytes are outside this module.
- Fine build masks also enforce the declared rectangle and maximum floor. This intentionally avoids the source fine-mask branch's incomplete floor-bound handling.
- Raw lot setters are trusted geometry projection methods, analogous to an architecture import/accepted edit. User affordability, object compatibility, and build permission checks live in the build layer.

Evidence: `tests/world_model.rs` covers split diagonals, floor-half lookup, room/fence behavior, multi-level portals, terrain and disjoint dirty regions, the support-spread invariant, build masks, bounded allocation, sentinel handling, deletion cleanup, and serialization round trips.

## W04.2 — Placement, intersection callbacks, and slots

### Source anchors

- `TSOClient/tso.simantics/Entities/VMEntity.cs`: `PositionValid`, `WallChangeValid`, `FloorChangeValid`, `SlopeValid`, wall rotation, placement and entity flags.
- `TSOClient/tso.simantics/VMContext.cs`: `GetObjPlace` and `GetAvatarPlace`, including ordered colliders and slot-zero support checks.
- `TSOClient/tso.simantics/Model/VMPlacementError.cs`: complete numeric mapping `Success=-1`, errors 0 through 50.
- `TSOClient/tso.simantics/Engine/VMSlotParser.cs`: candidate enumeration order, `NextRandom(1024)`, stable score ordering, failure priority, chair/standing weighting, congestion penalty, and directional masks.

### Implemented behavior

Placement validates bounds, requested build limits, center/corner requirements, wall/diagonal requirements, exclusive wall occupancy, inside/outside requirements, floor holes, floor/terrain/water/pool permissions, first-floor restrictions, upstairs support, slope permission, footprint wall crossing, and ordered object intersection.

Each intersection is resolved in ascending `(ObjectId,generation)` order. The proposed object and the other object each receive entrypoint 5, with the other handle, ghost-preview flag, proposed position, and proposed facing copied into the typed request. Results allow intersection if either tree allows it. The continuation can stop between the first and second script, serialize, resume, and reject a stale architecture/object/slot revision or a mismatched callback token.

**Requirement-driven callback difference:** the C# expression in `VMContext` uses short-circuit `target.ExecuteEntryPoint(5) || other.ExecuteEntryPoint(5)`. The W04 lane instruction explicitly required BOTH objects' callbacks, including stable ordering. The Rust implementation therefore invokes the second present script even if the first allowed intersection. That is deliberately tested and is not exact source short-circuit parity. Content whose second check mutates state or RNG needs an explicit compatibility decision before source-parity closure.

Object support checks source slot zero, one-based slot height, allowed height flags, strict `weight < support_strength`, source counter/end-table error mapping, maximum supported size, sale state, and available capacity. Avatars cannot be placed into surface slots.

Independent source review also covered less common predicate cases: `AllowIntersection` bypasses the object-height gate after geometry succeeds; the surface-support `allowedHeights > 1` comparison uses a signed short; the target avatar's `AllowPersonIntersection` ignores its own disallow bit as in `GetAvatarPlace`; and wall attachment uses the source cardinal-only `DirectionToWallOff` fallback for every diagonal. Regression fixtures preserve those cases without conflating them with the deliberate full-footprint policy.

Reservations contain owner and actor generations, slot index, logical operation, and monotonically increasing token sequence. A repeated operation returns its existing token. Conflicting actors cannot exceed capacity. Occupying a token checks liveness and expiration; duplicate occupation is idempotent. Deletion cleans affected reservations and slots. A reused local ID cannot consume or release an earlier generation's token. Call `expire_slots(accepted_tick)` before tick consumers use capacity; slot state has no wall clock.

SLOT scoring enumerates the source eight circle points or x-outer/y-inner range order. It consumes `NextRandom(1024)` after proximity, room, and direction eligibility but before final geometry/occupancy validation, unless equal-proximity or exact-point behavior suppresses the draw. Stable ties retain enumeration order. Standing/chair weights and the `/100000` congestion penalty are represented. Search sectors use source whole-tile quantization and overlapping 89.8-degree sectors; the angle test uses a fixed tangent comparison instead of platform `atan2`.

### Remaining compatibility work

- Content B must supply actual OBJ/SLOT geometry, flags, custom type-0 slot heights, multi-object average SLOT positions, and entrypoint implementations. The runtime adapter below supplies standing/chair checks, normalized facing, and snap-to-container dispatch.
- `score_slots` deliberately accepts a verifier rather than embedding VM script execution. The verifier must check wall rays, actual chair facing, occupancy, and reservations from the relevant immutable snapshot.
- Scores and rotations use bounded integer arithmetic. Circle diagonal rotation uses 46341/65536, and distance scores use 1/1024 units. These are deterministic approximations to C# floating calculations; near-boundary legacy score ordering requires reference fixtures.
- Footprints are bounded rectangle collections; eight-way rotation produces integer axis-aligned boxes. This is not arbitrary rotated-polygon collision parity.
- The model performs full-footprint bounds/support/wall checks and explicit corner requirements. Source `PositionValid` has some center/corner TODOs and less complete footprint checks. These additional constraints must not be mistaken for observed parity in untested content.

Evidence: `tests/world_placement.rs` covers all numeric code round trips, both script orders even after first-tree success, snapshot between scripts, stale and wrong-token rejection, wall/diagonal/exclusive flags, terrain/pool/slope/support/build bounds, strict weight and slot height mapping, reservation contention/idempotence/expiry/reuse, and scoring draw/tie/sector invariants.

## W04.3 — Resumable routing and behavior boundaries

### Source anchors

- `TSOClient/tso.simantics/Engine/VMRoutingFrame.cs`: `Execute` line 516 `NextRandom(1)`, room portal identity, ignored failed portals, goal alternatives, entrypoints 15/26/27, wait/retry logic, shoo tree 4107, and failure tree 398.
- `TSOClient/tso.simantics/Model/Routing/VMRouteFailCode.cs`: complete numeric mapping 0 through 13.
- `TSOClient/tso.simantics/Engine/Routing/VMRectRouter.cs`: stable sorted open-set insertion and source rectangle/free-region path optimization. Read as a source anchor; its rectangle and Bézier implementation is not ported here.
- `TSOClient/tso.simantics/Marshals/Threads/VMRoutingFrameMarshal.cs`: continuation requirements for current portal, choices, ignored portals, movement state, waits, retries, and target handles.

### Implemented behavior

- `step(query, rng, budget)` performs at most the requested node expansions, capped at 2048 per dispatch and the request's total search-node limit. Search open queue, costs, parents, closed set, tie sequence, path cursor, alternatives, waits, retries, and callback state serialize.
- Exactly one `NextRandom(1)` is consumed by each active route dispatch, including zero-budget dispatches. While a nested route executes, only that child's dispatch consumes the cycle.
- The navigation graph uses a four-subtile grid, exact start/goal connectors, stable eight-way neighbors, full footprint collision checks, wall centerline intersection, diagonal barriers, and explicit portal edges. Every emitted ordinary movement checks current geometry and occupants again.
- Portals are distinct traversal identities, so a failed door does not exclude a different door into the same room. A failed portal is remembered and the search tries another traversal; architecture changes clear that history and replan.
- Portal entrypoint 15 is an explicit callback. On success, the returned/default destination, recorded entry/exit, traversal direction, enabled state, object identity, and portal revision must still match. The host applies the emitted `Progress` change. An endpoint or cost/revision change while the script is suspended causes replanning instead of accepting the old crossing.
- Sit 26, stand 27, shoo 4107, and optional top-level failure 398 are typed callback requests. The `kind` identifies whether the numeric value is an entrypoint or a routine request; the content host resolves the correct code owner and routine. A later `CantStand`, exhausted search, or other ordinary route failure emits its enabled failure callback before returning a terminal failure, just as initial SLOT rejection does. The callback has its own bounded wait even if search exhausted the dispatch allowance; completion or timeout cannot invoke that failure tree a second time.
- `RouteCallbackKind::Failure { code, blocker }` carries the arguments required by source `HardFail`: B binds `[code.code(), blockerObjectIdOrZero, 0, 0]` to routine 398 in the actor. The blocker keeps its generation even if already deleted. While this callback is pending, the original route's target/chair/slot references are historical, but its callback actor and target must remain live. Missing/out-of-world actors interrupt directly without issuing a callback to an unavailable actor.
- Chair capacity failures, sit failures, and ordinary route failure can advance through supplied goal alternatives. Sitting/standing scripts may own their own nested route and return a validated position.
- Moving avatars cause waits; repeated or stationary blockers are considered during replanning. A stationary avatar that leaves no path can produce a shoo callback followed by a bounded wait and retry. Already shooed entities are remembered. Default retry/wait parameters use five retries, 300 timeout dispatches, 30 moving-avatar waits, and 60 shoo waits. A separate total dispatch limit prevents unbounded continuations.
- A moved target updates goals marked `follows_target`, then replans. Generation reuse/deletion fails a still-active target. Actor displacement and architecture revision changes also replan.
- `push_nested` requires a pending parent callback, same actor, an ID distinct from every ancestor, matching start, and bounded depth. The child owns its search/callback cursor. `NestedFinished` returns to the parent script; it does not automatically declare the parent callback successful.
- Restore validation checks bounded queues and insertion counters, sorted open ordering, positive-cost acyclic parent relations, coordinate bounds, callback kind/entrypoint/target correspondence, live active handles, and nested ownership/depth/ID uniqueness. Immutable lot dimensions also bound positions at callback admission. Historical avoided avatars and failed portal references may remain as historical memory; stale live target revisions are valid inputs to the next replan. An actor moved to the exact out-of-world sentinel fails the active route before replanning.

### Deliberate replacement / unimplemented legacy behaviors

- The geometry is an actual usable resumable grid router, not a synthetic test standing in for `VMRectRouter`. It nevertheless remains a replacement. It does not reproduce source free-rectangle expansion, smoothing, Bézier lines, acceleration, turn animation frames, exact steering, wall thickness, or source path frame counts.
- With portals present, search uses a safe zero heuristic; without portals, it uses a lower-bound planar heuristic. Large or congested lots can exhaust configured node budgets and return a typed failure rather than blocking indefinitely.
- Ordinary geometric search uses semantic footprint/person-intersection flags. It does not speculatively run arbitrary entrypoint-5 BHAVs while exploring candidate nodes. The placement provider offers those callbacks; route-specific content passability still requires integration fixtures.
- Room routing and within-room planning are combined into the semantic graph instead of reproducing the source's floating room-portal A* followed by rectangle routing. Avatar job-lot intersection hacks, exact source turn handling, and the full shoo interaction-queue policy remain unported.
- The host owns accepted-tick scheduling, VM frame call/return, animations, shoo target routines, slot reservation timing, cancellation, and actual entity position writes. The caller must apply a `Progress` atomically; if a prepared build lock rejects the movement, restore/replan the route rather than publishing an unapplied position.

Evidence: `tests/world_routing.rs` runs the actual provider through zero-budget RNG cycles, suspended search/walk snapshots, diagonal wall avoidance, failed first portal/alternate stairs, mid-portal save/resume, nested routes, moving/deleted/reused targets, chair capacity, sit/stand failures, a blocked corridor requiring shoo, and timeout/out-of-world failure. Continuous and restored runs compare each subsequent dispatch and full state/RNG.

## W04.4 / SL.4 — Build preview and durable completion

### Source anchors

- `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetArchitectureCmd.cs`: permission verification and serialized global-link architecture ordering; `Verified` is internal authority.
- `TSOClient/tso.simantics/NetPlay/Model/Commands/VMNetBuyObjectCmd.cs`: discard network price, obtain catalog price/discounts, perform transaction, and requeue only after success.
- `VMArchitecture` and entity placement/floor/wall checks above supply geometry legality.

### Implemented state machine

1. `preview_build` validates the connected live actor, permission/owner projection, expected architecture revision, catalog price table, affected object revisions, placement legality, and balance sufficiency. It applies edits to a clone. The live world and the account projection remain unchanged.
2. The preview stores edits, exact affected tiles/objects/allocated IDs, expected live generations/revisions, script decisions, accepted cost and permission/account/catalog revisions, undo metadata, and a SHA-256 digest of its canonical bincode form.
3. `begin_build_commit` validates those guards again. One prepared build reserves the geometry boundary. It returns an idempotent `DurableBuildRequest`. New builds conflict with a prepared operation; normal projection changes cannot move through its reserved geometry, change guarded objects, or reuse its proposed local IDs. Unrelated avatar movement continues.
4. The E adapter performs the durable permission/funds/object-ownership transaction and submits `ServerBuildConfirmation` through the authenticated completion lane. Public Rust data alone does not authenticate a server; clients must never be admitted to this completion API.
5. A matching committed receipt binds operation, actor generation, persistent owner, preview digest, exact charged cost, and each created object to a nonzero unique persistent ID. Only then are the staged edits reapplied to the current world and published atomically. Previously checked intersection decisions are replayed and verified, not rerun against the live VM.
6. Rejected durable transactions release the pending lock without geometry. Exact duplicate confirmations return the same recorded result without reapplying geometry. Conflicting duplicate payloads are rejected.
7. If a trusted raw architecture import/change bypasses the lock after the durable cost committed, completion records `NeedsReconciliation` with the durable receipt. It does not overwrite newer geometry, invent a refund, or forget the already committed cost. E must reconcile the durable receipt.

Replacing an existing portal requires permission over both the old and new associated objects. Its preview guards and locks include the old endpoints and owner as well as the replacement. Restore checks also bind every terminal receipt to the matching outcome status, nonnegative charged cost, valid owner/actor identities, created-object mapping, and durable receipt. Pending inverse edits and undo guard maps have explicit bounds and structural validation.

Version-one undo is a new compensating transaction, restricted to the latest compatible history entry with matching affected-object/architecture revisions. Architecture changes, moves, portal edits, and purchase deletion can have inverses. Deletion of an existing persistent object has no fabricated restore operation: its full durable restore semantics require B/E. Costs for deletions/removals currently remain zero in the supplied simple price table; no source depreciation/refund/discount policy is claimed. Unknown undo versions are rejected.

The build outcome cache is bounded at 4096 operations and refuses new operations when full. E/F must provide a durable replay-watermark/compaction policy before long-lived production operation; silently evicting records and allowing old operations to execute again would be incorrect.

### Integration gates that remain open

- **B/content:** actual catalog definitions, multi-tile groups, discounts/upgrades/donations, placement scripts, portal/chair code owners, slot bindings, exact object initialization/deletion hooks, and representative object-family trace cohorts.
- **Runtime/SL.1:** a committed `PlaceObject` creates the semantic world projection. The runtime must create the corresponding VM entity from the staged immutable definition and allocated handle at the same accepted completion tick. Existing-object moves/deletes likewise need coordinated VM/world mutation. This lane does not write the VM store.
- **E/W13.3/W15.2:** authenticated admission, native lot authority/epoch fencing, permission and account revisions, real durable money/ownership transactions, exactly-once operation ledger, reconnect reconciliation/refunds, persistent object IDs, and outcome compaction.
- **F/W02.4/SL.9:** final shared schema adoption and full runtime/native/WASM snapshot-plus-tail integration. The world provider's continuation round trips are implemented; this alone does not close cross-service or catalog parity.

Evidence: `tests/world_build.rs` covers pure preview, insufficient funds/permissions/disconnect, competing placement, stale architecture and moved objects, build locks and unrelated movement, forged/mismatched receipts, rejected durable outcomes, committed-but-undelivered snapshot/retry, duplicate completion, out-of-band reconciliation, versioned undo, owner/generation checks, and saved intersection decisions.

`BuildAuthority` and `BuildPreview` are inputs from the trusted preview/admission path. A SHA-256 preview digest is an integrity binding, not authentication. E must recheck catalog pricing and permissions from trusted data rather than accepting a client-created authority or digest. B/runtime must gate interaction and sale-status changes against a pending build lock; ordinary `in_use`/`for_sale` projection changes advance object revisions and cannot silently bypass a guarded preview. If E elects to permit such changes, it needs an explicit reconciliation policy. Portal endpoints are similarly explicit content data: moving a `WorldObject` alone does not infer transformed portal endpoints. The corresponding B/runtime movement adapter must update those endpoints and portal revision when appropriate.

## Runtime routing primitive integration

The root runtime delegates routing requests to `src/runtime_routes.rs`. This adapter owns no second VM or clock: it reads the shared `SimState`, immutable `ContentSet`, explicit RNG, world projection, and imported animation metadata. Its `handle_route(state, content, owner, request, budget)` stages each provider call and commits the staged state only after a valid response. A `VmFault` leaves no partial movement, detach, script write, container change, or RNG mutation. The shared `move_checked(state, content, entity, position, facing, budget)` helper is also used by root EntityOperation ChangePosition/Create. Both take a shared `ScriptBudget` outside the staged state: script work remains charged when a probe or provider fails. Candidate scoring, relocation revalidation, FindLocation attempts, and recursively reentered placement retain that instruction allowance and the caller's bounded query depth.

| Source handler | Implemented runtime behavior |
| --- | --- |
| `Primitives/VMGotoRoutingSlot.cs`, `Engine/VMMemory.cs:GetSlot`, `Engine/Scopes/VMSlotScope.cs` | Full 16-bit data/scope decoding; parameter, literal, and global normalized SLOT lookup; source default max/optimal proximity normalization; live target and avatar checks; finite route continuation and typed failure-script continuation when no candidate survives. |
| `Primitives/VMGotoRelativePosition.cs` | Exact-point “OnTop” with no scoring draw, AnywhereNear 16–32, directional 16–24 searches, source 16-unit resolution, any/toward/relative-facing operands, and the byte-six NoFailureTrees bit. |
| `Primitives/VMSnap.cs` | Full 16-bit mode, parameter/literal/global/InFront/BeContained modes, occupied-container rejection, exact-point SnapToDirection before range enumeration with no scoring draw and source rounded-log2 direction-mask semantics, slot-target placement even without valid location candidates, ordinary checked relocation and container detachment, and source PrimitiveResult/PrimitiveResultID failure fields. The exact out-of-world caller sentinel may snap into a valid lot. |
| `Primitives/VMLookTowards.cs` | Head-seek person data 41–45 with source `[target,1,0,1,0]`, headless camera no-op, body toward/away and multitile-average positions with short narrowing after each source addition, check-tree branching, and asynchronous eight-notch body facing. |
| `Primitives/VMFindLocationFor.cs` | Modes 0–5: exact reference then source ring/direction order, inverted PreferNonEmpty flag and deferred occupied-tile order, local reference, out-of-world relocation, smoke midpoint minus half a tile, source along/lateral vectors including repeated zero-distance candidates, and at most 100 random attempts consuming X then Y. |
| `Primitives/VMReach.cs`, `tso.files/Formats/IFF/Chunks/SLOT.cs:HeightOffsets` | Reach stack/parameter-selected slot, source height-to-animation choice, animation speed one, idle hand gestures, `EndReached` before queued events, xevt zero grab/drop, occupied-slot behavior, and serialized animation cursor/queue. A completed pickup does not restart the earlier reach animation when the object's apparent height changes. |

The SLOT verifier checks integer wall line-of-sight, source north-facing placement, both present entrypoint-5 BHAVs, avatar congestion, chair entrypoint 26 and facing tolerance. The route continuation checks defined chair-slot capacity before sitting. Equal-position destinations from other active routes receive the congestion penalty. `score_slots_with_rng` exposes the RNG position after each scoring draw to the isolated query snapshot. The root check helper binds `[otherObjectId, ghost, 0, 0]` and proposed pose before running the real encoded behavior tree. Query mutations, effects, and query-only RNG draws never enter the authoritative state; an incomplete/faulting query is propagated explicitly. SLOT verification has an 8192-probe host ceiling in addition to world candidate bounds and the shared instruction/depth limits.

Root runtime integration binds a route continuation to the caller's generation and owning thread, applies emitted movement atomically to VM/world descendants, records source RouteResult in person data 62 before dispatching a failure behavior, preserves the existing Temp0 on false route completion, and detaches a seated avatar after a validated successful Stand callback while changing posture 1 to 0. All portal/sit/stand/shoo/failure scripts remain typed requests which B must dispatch and return with continuation ID, route ID, and callback token. A snapshot can retain a pending portal, failure behavior, or animation xevt and resume it through accepted ticks on authority and replica.

### Explicit compatibility boundaries

- Heading, wall rays, slot scores, footprints, path geometry, and body-turn timing use the bounded integer world model. Continuous source radian orientation, turn animations, rectangle free-region routing, Bézier motion, and full original dispatch timing are not claimed. SLOT discovery currently occurs in the requesting host call; the route's first search dispatch occurs at the runtime's next scheduled route step. Whole-VM RNG traces must account for that scheduling boundary, while each scoring and route-dispatch draw is explicit and ordered.
- Cross-thread calls whose route actor differs from the owner fail explicitly until F/B adopt separate actor/owner continuation binding. Synchronous Snap and FindLocation can act on other objects without binding an asynchronous route to the wrong thread.
- LookTowards mode 255 requires accepted direct-control input and its dedicated continuation. Snap's Shoo flag requires B's queued goto-object interaction 3 adapter when a blocking avatar is actually encountered. Both produce explicit unsupported cases rather than fabricated completion.
- Reach custom height 5 requires normalized type-0 SLOT `OffsetZ`; unsupported height records produce an explicit provider error. Other source height-table entries are mapped, including midpoint-even rounding of 2.5 to 2. Source reach-to-mouth remains unsupported as it is in C#.
- The normalized routing SLOT contract does not yet encode every legacy flag, such as UseAverageObjectLocation for SLOT searches or all entry direction hints used by sit animations. Body LookTowards average mode is implemented separately. The congestion proxy uses supplied alternative goal positions rather than a source rectangle-avoidance destination object, and architectural-door scoring metadata still needs the B importer.
- Source synchronous placement checks can mutate script/RNG state in some legacy call paths. This project's pure-query requirement intentionally isolates each check; BOTH script evaluation remains the earlier requirement-driven difference. Imported content that depends on those side effects needs an explicit compatibility decision.
- `PlaceObject` build completion and coordinated persistent VM creation/deletion still require B/E integration; the checked runtime Create helper does not by itself connect a durable build receipt to VM allocation.

Evidence: `tests/runtime_routes_source.rs` drives actual `SimRuntime` accepted ticks and real encoded BHAV instructions for all six provider families. It covers scope/mode decoding, bound script arguments and isolated writes, second-script failure atomicity, recursive placement and shared budget exhaustion, container rules, local/vector/random search ordering, source failure data, Reach events, and authority/replica snapshots across walking, portal callbacks, and animation queues. `world::routing::restore_regressions` and `world::build::restore_regressions` additionally reject forged search counters, ancestor route-ID aliases, and contradictory terminal receipts.

## Validation commands and scope of evidence

Run from the branch root with the provided Rust 1.75 offline toolchain:

```sh
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo cargo test --manifest-path crates/sim-core/Cargo.toml --offline --test world_model --test world_placement --test world_routing --test world_build
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo cargo test --manifest-path crates/sim-core/Cargo.toml --offline --lib world::
CARGO_HOME=/workspace/scratch/cb8e2814717b/toolchain/cargo cargo test --manifest-path crates/sim-core/Cargo.toml --offline --test runtime_routes_source
```

The world-owned source and tests are formatted with Rust 2021 rustfmt. Debug and release commands/results, independent review findings, and final runtime/native/WASM integration status should be recorded in the branch-wide verification report. Do not convert these invariant fixtures into a claim that every W04 source branch, primitive, object family, or external persistence path is implemented.
