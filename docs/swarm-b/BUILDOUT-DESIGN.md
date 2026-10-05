# Swarm B games, sprite workflow, and cooked runtime consumption

## Intent and scope

The user asked Swarm B to build the Wonderland fork exhaustively and, after the
published continuation, explicitly asked to proceed. This increment advances
the remaining W01, W06 and W16 work using the existing architecture. It produces
usable native handlers and creator/content workflows, executable comparisons,
and reviewable follow-up PRs. The supplied `FreeSO-Parallel-Work-Packages.md`
and the package table in [README.md](README.md) remain the scope authority.

The implementation starts at `04f0a407dd81acf0685453e7367764bf75b5c092`.
Original source and assets remain at baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`; the actual simulation dependency
remains `8a0e251d19e222a0a6833d7408ca629f674e1729`.

## Cooperative EOD games

Add PaperChase (`0xCA418206`), PizzaMaker (`0xEA47AE39`) and the two-person
job maze (`0x4A245A22`) to the existing native host. Their original handlers
share controller/participant lifetimes, bounded roles and 30 Hz tick timing.
A logical game owns source phase/counters, private hands/maps/choices and one
recorded no-avatar controller. Each authenticated participant retains the
existing scoped transport ticket and one fixed role. A game advances once per
host tick regardless of its participant count.

Controller creation, participant roles, invocation registers and Pizza's
SimAntics callbacks are trusted native inputs. UI messages cannot set those
values. A controller capability binds host scope, epoch, group and invoker.
Public events contain only the source VM-visible outputs; role-specific UI,
private state and random state remain outside public projections.

Use a documented native deterministic random stream with bounded draws and
private checkpointed state. This is not a claim of C# `System.Random` sequence
parity. Preserve source state transitions, iteration order and emitted event
values. All host admission and output-budget failures are atomic, including
random draws and card/map changes. These handlers have no direct durable
provider calls; a Pizza payout event is still an event for the VM to handle.

Private checkpoint format 3 stores each group once with participant references.
Existing format 1 and 2 behavior remains compatible. Restore detaches controllers
and transports; games pause until their retained controller and seats rebind.
Rebind restores role UI without rerunning initial connection/dealing logic.
Controller teardown closes all its seats. Validate group/seat identities,
state-machine fields, map/path bounds, private RNG state and references before
restoring. WarGame's wall-clock callback contract is a later separate change.

## Sprite authoring workflow

Expose the existing source-tested PALT/SPR2 codecs through a strict, single-file
creator package. JSON contains whole-IFF and resource guards, exact sprite
identity, ordered frame metadata, compact hexadecimal index/alpha/depth planes,
and required local palettes with editable RGB bytes. All planes are row-major;
RGB derives from the explicit palette without color quantization.

Creators can export, change a pixel or palette color, choose exact or explicit
source alpha quantization, import atomically, and reopen the resulting IFF.
Version 1 retains sprite identity, frame count/order and palette identity/count.
Frame dimensions, position and supported channel flags may change with matching
bounded planes. A missing/external palette dependency rejects typed authoring.
Only required palettes are decoded, so unrelated resources do not become an
implicit editing prerequisite.

The package is capped at 16 MiB and the caller's stricter limits. Map-only
deserialization rejects unknown/duplicate fields before reading their values,
positional arrays, wrong scalar types, trailing input and malformed hex. Count
all retained package strings, decoded planes/palettes, source/candidate copies
and encoding buffers before allocating. Apply aggregate pixel and frame limits.

Import verifies every guard and immutable identity against the same source,
validates the complete candidate, encodes through original-aware IFF rebuilding,
reopens it, then publishes once. Semantic no-op retains original bytes, including
noncanonical SPR2 command streams and tails. A palette-only change preserves the
SPR2 payload. Invalid late edits leave both the document and existing output
unchanged. The existing stable-workspace path contract remains in force.

## Verified cooked runtime binding

Add a separately hash-selected `CookedRuntimeBindingV1` to describe how selected
verified pack members form runtime object scopes. It binds the exact manifest
hash, simulation revision, ordered member IDs/source names, object IDs/GUIDs,
semiglobal owners, resolved tuning, locale/dialect and explicitly normalized
runtime metadata. The existing manifest/pack schemas remain unchanged.

Verify binding and manifest hashes, calculate the required dependency closure,
verify selected pack hashes and exact manifest membership, then decode selected
members with their declared codecs. Reject noncritical semantic inputs, wrong
codecs, conflicting scope identities, missing calls and inconsistent tuning.
Do not trust the unbound import report, infer source order from sorted IDs, or
manufacture original whole-IFF/resolver provenance from one-chunk derivatives.
Source import and cooked import have separate provenance reports while sharing
the validated actual `ContentSet` conversion. A's descriptor binds the converted
runtime content and tuning. The separate pinned A revision and snapshot decoder
provide revision and state-schema checks.

Promote source-backed OBJf tables into the semantic format dispatcher so the
existing cooker can place them in simulation-critical semantic packs. Preserve
their header/entries/tails in the format API; the runtime retains its narrower
representability checks. This removes an otherwise unnecessary cooked-object
restriction without changing manifest codecs.

The acceptance workflow cooks authored IFF/patch/tuning inputs, removes those
source inputs, loads only selected packs, executes a real BHAV, and restores and
resumes a snapshot. Native and WASI runs must agree. This demonstrates pack-only
runtime consumption, not full game routing, interaction scheduling or rendering.

## Original-handler comparisons

Compile unchanged Timer, DanceFloor, Signs, Scoreboard and PermissionDoor C#
handlers with their real relevant source support classes under Mono. Supply
explicit test adapters for the missing VM/transport/storage and a controlled
scheduler for asynchronous source continuations. Drive equivalent scenarios
through the native Rust host and compare normalized per-channel ordered outputs
at named operation boundaries. Compare VM events, private payloads, persistence
and disconnect behavior; independently prove the comparator rejects mutations.

Pin every original input and include the actual bytes in aggregate verification
identity. Document adapter projections and intentional native policy differences.
This provides original-handler execution evidence; full FreeSO application, UI,
transport and production-provider qualification remain separate gates.

## Ownership and completion

Independent implementers own disjoint EOD, creator, bridge and oracle paths.
The coordinator owns OBJf support, aggregate verification, shared status docs,
staging, commits and publication. Original C#, original assets, A-owned runtime,
shared contracts and root workspace are outside mutation scope.

Completion requires task review, fixed actionable findings, relevant native and
portable checks, complete frozen-input aggregate results and fetched GitHub trees
matching the tested trees. Publish draft PRs above the existing stack. Merging
remains an integrator decision.
