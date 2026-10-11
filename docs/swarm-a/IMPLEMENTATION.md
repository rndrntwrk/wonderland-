# Swarm A implementation and ownership

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
Target: `rndrntwrk/wonderland-`, branch `feat/swarm-a-simulation`.
Requirements: supplied FreeSO Rust Browser Rewrite Plan and Parallel Work Packages,
W02–W05, SL.1, SL.4 (simulation portion), and SL.9 (simulation portion).

## Boundaries

This is a standalone `crates/sim-core` package until Swarm F integrates workspace
membership and the shared contracts. No root workspace, `crates/contracts`,
`content-ir`, `legacy-formats`, network service, or interaction/EOD lane is owned
by this branch. Simulation-local types are explicitly provisional integration
interfaces, not a unilateral freeze of the cross-swarm protocol. Swarm F can
adapt/re-export them after reviewing the compatibility fixtures.

The native/WASM rules are one implementation, 30 ticks per second. Ordered maps
and sets define iteration. All persistent state is serializable, including VM,
route, animation and effect continuations. External effects are requests only;
the core never executes database or network operations. Replica snapshots exclude
private EOD state and do not create authority for persistent money/ownership.

## Task 1: Numeric and identity substrate (W02.1)

Owned: `src/ids.rs`, `src/numeric.rs`, `src/rng.rs`, numeric/ID reference tests.
Port source integer overflow, narrowing, shifts, divide/modulo, ties-to-even,
xorshift RNG, smallest-positive local ID allocation and generation rejection.
Vectors cover zero/one RNG bounds, narrowing-before-compare and exhaustion.

Shared within this package: `ObjectId(pub i16)`,
`EntityRef { object_id: ObjectId, generation: u32 }`,
`PersistentId(pub u32)`, `SimRng { state: u64 }` with
`SimRng::new(u64)`, `state()`, `next(u64) -> u64`, `mix_entity_count(usize)`.
All derive Clone/Debug/PartialEq/Eq and serde Serialize/Deserialize; IDs also
Copy/Ord/PartialOrd/Hash. Other modules validate nonzero positive IDs and live
generations at authority boundaries.

## Task 2: Interpreter, memory and primitives (W02.2, W03)

Owned: `src/vm/`, `src/primitives/`, VM/primitive tests and coverage census.
Consume immutable, bounded BHAV instructions from typed routine stores; expose
a host interface for memory/entity/world/animation/effect operations. Keep
caller/callee/stack-object/code-owner distinct. Port branches, call/return,
temps/locals/parameters, yields, explicit missing-opcode behavior, arithmetic,
list and object primitives. Unsupported external cases have explicit statuses;
they cannot masquerade as implemented entries. Provide early host API to root.

## Task 3: Architecture, placement and routing (W04, SL.4)

Owned: `src/world/`, world tests, world source/coverage notes.
Implement semantic multi-level lots, walls/diagonals/rooms/terrain, precise dirty
regions, deterministic placement with BOTH scripted intersection checks, bounded
resumable routes, portal callback continuations, slot reservations and revision
checks. Expose transactional build preview/commit interfaces without creating a
database adapter or spending money inside the mirror. Stable source failure codes
are required. Serialize continuation and reservation state.

## Task 4: Avatar behavior (W05)

Owned: `src/avatars/`, avatar tests and avatar source/coverage notes.
Implement source-width identity/needs/skills/outfits; headless animation timeline
including original event-order behavior, synthetic completion markers and
loop/reverse/hurry; deterministic autonomy scoring over supplied offers; participant
lifecycle/social cancellation. Consume an explicit RNG, never OS time or renderer
callbacks. Autonomous selection does not mutate UI queries. Expose serializable
state and a tick API for root integration.

## Task 5: Runtime, scheduler, effects, snapshots (W02.3–W02.4, SL.1, SL.9)

Owned: `src/state.rs`, `src/runtime.rs`, `src/scheduler.rs`, `src/effects.rs`,
`src/snapshot/`, integration tests and replay example.
Integrate module APIs into `SimRuntime::step`; enforce epoch/tick order,
generation-aware targets, deterministic scheduling/deletion and instruction
budgets. Capture post-tick snapshots; validate version/content/hash/bounds and
restore atomically before replaying N+1. Validate typed effect completion against
durable operation IDs and generations. Tests compare continuous vs interrupted
execution during calls/sleep/routes/animation/effects and native/WASM outputs.

## Task 6: Independent review and handoff

Owned: package manifest/lockfile, `tools/swarm-a/`, `docs/swarm-a/`, PR publication.
Run targeted suites, whole-package debug/release suites, formatting, native/WASM
replay, reference vectors and independent review. Record exact source coverage,
unmet upstream dependencies, commands/results and review findings. Final status
distinguishes implemented mechanisms, source-reference evidence and full-content
parity; no example can close an entire primitive or object cohort.

## Preflight resolutions

| Shared pair | Producer / consumer agreement | Resolution |
|---|---|---|
| Numeric → all | Signed local IDs, generation refs, explicit RNG | The types above are local to sim-core; F owns cross-swarm adoption. |
| VM → runtime | Serializable thread + host interface | VM owner publishes host API before root implements the adapter. |
| World → runtime / primitives | Pure semantic model and typed callback/route state | Root invokes API; world owns all module implementation. |
| Avatars → runtime / primitives | Serializable state and simulation-time events | No renderer callbacks control behavior. |
| Runtime → snapshots | Full deterministic state at completed tick | Versioned bounded encoding and replay tests guard the boundary. |
| Shared workspace → F | No Rust workspace or contracts exist on master | Keep this crate standalone; do not edit F-owned global schema paths. |
| W04.4 / W03.4 → E | Durable writes require online platform | Implement core requests/resolution and concurrency tests; E supplies transactional authority. |
| W02.4 → B/F | Legacy save import and reference corpus are separate owners | Provide continuation schema; full FSOv conversion remains an explicit integration gate. |
| W05.3–4 → B | Offers and content-driven interactions are B-owned | Consume typed candidates and participant state; no per-object hardcoded gameplay. |

Each task's tests match its owned code and named source anchors. High-risk
intentional deviations (e.g. rejecting ID exhaustion instead of allocating ID 0)
must be recorded with the test that proves the new behavior.
