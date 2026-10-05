# Swarm A: simulation implementation and handoff

This branch delivers the standalone `crates/sim-core` Rust package for W02–W05
and the simulation portions of SL.1, SL.4, and SL.9. It also includes source
reference probes, adversarial integration tests, and an actual-runtime
native/WebAssembly replay harness. It is an implementation and integration
handoff; complete legacy content parity remains a separate acceptance gate.

Repository: `rndrntwrk/wonderland-`

Branch: `feat/swarm-a-simulation`

Source baseline: `4c6b3e8f5835b228723caea3c9f683c62f244f73`

## Start here

| Document | What it answers |
|---|---|
| [Package README](../../crates/sim-core/README.md) | How to build, run the headless example, and locate the public APIs. |
| [Coverage](COVERAGE.md) | What each work package implements, what evidence exists, and which acceptance gates remain open. |
| [Handoff](HANDOFF.md) | The concrete data and adapter contracts needed by the other swarms. |
| [Review](REVIEW.md) | The independent findings, corrections, and permanent regression evidence. |
| [Verification](VERIFICATION.md) | Exact final commands, results, source identity, and limits of the evidence. |
| [Implementation plan](IMPLEMENTATION.md) | Ownership, sequencing, and provisional cross-swarm interfaces. |
| [Native/WASM replay](../../tools/swarm-a/replay/README.md) | Four actual-runtime scenarios, safe WASM ABI, byte-level comparison, and repeatable commands. |

## What is implemented

The VM executes bounded immutable BHAV instructions with distinct caller,
callee, stack-object, and code-owner identities. It retains frames, arguments,
locals, short/XL registers, wait state, and continuations through snapshots.
Primitive groups include arithmetic/flow, object and list operations,
relationships, TS1 projections, behavior lookup, presentation requests,
animation/motive changes, routing requests, and typed external effects.
The source-derived registry makes incomplete providers visible to callers.

The runtime applies admitted commands to a staged state, executes deterministic
dispatch, advances avatar behavior, records deferred deletions, validates the
result, and publishes one post-tick state/hash. Duplicate input is an exact
no-op; conflicting input, wrong epochs, wrong content/RNG, invalid state, and
aggregate instruction exhaustion reject the transaction. Individual runaway
threads are quarantined under an explicit bounded policy.

World state includes floors, walls and diagonals, terrain, levels, rooms,
support, invalidation, footprints, placement constraints, generation/revision
guards, resumable paths and portal/chair/shoo callbacks, and slot reservations.
Build preview and completion form a staged state machine with durable receipts,
duplicate handling, reconciliation states, and versioned undo. Its geometry
transaction still needs coordinated VM entity mutation and E's service adapter.

Avatar state includes source-width person data, needs/skills, motives and
fractional decay, relationships, outfits and legacy suit inputs, head seek,
disconnect/reset/leave state, social rendezvous, and deterministic autonomous
selection over supplied offers. Animation events advance from simulation time;
event order, normal/hurried/reverse playback, completion, and event synthesis
are independent of a renderer. Resource lookup uses supplied STR/animation
metadata with explicit missing-resource and case-alias rules.

Snapshots validate the complete graph before accepting a restore. They bind
schema, lot/epoch/tick, content, and tuning; reject malformed lengths and
noncanonical bytes; and verify relationships among entities, IDs, frames,
effects, routes, containment, world projections, and saved primitive operands.
The checksum detects corruption. Authentication and durable recovery positions
belong to the service boundary.

## Source evidence

| Area | Notes and reference anchors |
|---|---|
| Numeric widths, rounding, RNG, IDs, clock/scheduler policies | [Numeric source notes](numeric-source-notes.md) |
| Initialization, main restart, reset, notifications, dispatch | [Lifecycle source vectors](lifecycle-source-vectors.md) |
| VM/primitive census and source mapping | [VM source notes](vm-source-notes.md) |
| Runtime memory scopes and projections | [Memory adapter](runtime-memory.md) |
| World, placement, routes, build receipts, source deviations | [World source notes](world-source-notes.md) |
| Avatar values, animation event order, motives, suits, autonomy | [Avatar source notes](avatar-source-notes.md) |
| Durable request/resolution boundary | [Effect boundary](effect-boundary.md) |
| Snapshot layout, validation, restore contract | [Snapshot contract](snapshot-contract.md) |

The C# probes verify identified source expressions or extracted source methods.
The replay harness verifies that the native and WASM builds of this Rust
runtime agree on the supplied scenarios. Neither substitutes for the complete
legacy reference trace/content corpus. Each document distinguishes direct
source behavior, intentional replacements, and unfinished integration.
