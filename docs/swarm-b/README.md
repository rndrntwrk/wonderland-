# Swarm B — content, objects, and creator tools

This branch provides a working Rust content pipeline, guarded offline creator
transactions and sprite editing, eight native EOD handlers, and source/cooked
content bridges to Swarm A's actual simulation runtime. It also records every
checked-in object and registered EOD in a reproducible coverage inventory, with
source-selected OBJf lifecycle references.

The source baseline is [`4c6b3e8f5835b228723caea3c9f683c62f244f73`](https://github.com/rndrntwrk/wonderland-/tree/4c6b3e8f5835b228723caea3c9f683c62f244f73).
The assigned packages are W01, W06, and W16 from the supplied parallel rewrite
plan. [IMPLEMENTATION.md](IMPLEMENTATION.md) records the initial ownership
decisions; [CONTINUATION.md](CONTINUATION.md) records the following implementation.
[BUILDOUT.md](BUILDOUT.md) and [BUILDOUT-DESIGN.md](BUILDOUT-DESIGN.md) describe the
cooperative-game, sprite-workflow and cooked-runtime increment.
[BUILDOUT-VERIFICATION.md](BUILDOUT-VERIFICATION.md) is the current verification
and review record. The earlier [44-gate continuation](CONTINUATION-VERIFICATION.md)
and [35-gate initial result](VERIFICATION.md) remain historical baselines.
The complete buildout passes **47 gates, 439 Rust tests and 48 Python tests**.
Draft PRs [#13](https://github.com/rndrntwrk/wonderland-/pull/13),
[#14](https://github.com/rndrntwrk/wonderland-/pull/14),
[#15](https://github.com/rndrntwrk/wonderland-/pull/15) and
[#16](https://github.com/rndrntwrk/wonderland-/pull/16) contain the four reviewed
feature increments; `swarm-b/buildout-verified` adds the final evidence and runner
integration. Each published feature tree was fetched and checked against the
reviewed local tree.

Full Swarm B completion still requires integrated interaction scheduling,
routing, presentation, transport, production persistence, and reference-runtime
qualification. The package table below distinguishes implemented functionality
from those gates. An isolated source routine running in the actual interpreter
does not qualify a complete original object as gameplay-complete.

## Run the delivered tools

Use Rust 1.90.0. Run these commands from the repository root. The commands use
package manifests because Swarm F owns the future root workspace and its lockfile.

```sh
cargo run --locked --manifest-path tools/asset-cooker/Cargo.toml -- demo /tmp/wonderland-demo
cargo run --locked --manifest-path tools/asset-cooker/Cargo.toml -- verify /tmp/wonderland-demo/release
cargo run --locked --manifest-path tools/creator/Cargo.toml -- --help
```

The demo destination must be new. It contains authored fixture source and a
verified release; see [cooker.md](cooker.md) for its precise output layout and
the `cook`, `verify`, `plan`, and `inspect` commands. The fixture has a BHAV,
BCON, effective tuning, and an unknown chunk. It demonstrates deterministic
packing and dependency selection. Its BHAV is not a playable object.

[creator.md](creator.md) documents resource/container inspection, exact untouched
round trips, SHA-256/version-guarded multi-operation transactions, typed palette
and OTF tuning edits, and semantic BMP city-map editing. The dedicated
[sprite workflow](creator-sprites.md) exports a source-bound editing package,
edits indexed pixels and palettes, applies explicit alpha policy, and imports
atomically through the original-aware IFF writer. The separate
[runtime bridge](runtime-bridge.md) supplies bounded routine queries, isolated
accepted-tick replay, watches, and frame-position inspection against a pinned
real VM. The [cooked-release workflow](cooked-runtime.md) prepares a binding,
plans selected packs, verifies and loads them, and replays an explicit scenario
without the original source files. Instruction stepping and a running-lot editor
remain separate integration work.

```sh
bash tools/swarm-b/verify.sh --with-parity --with-source-oracle --with-runtime-bridge
python3 tools/swarm-b/eod-census.py --check
python3 tools/swarm-b/object-census.py --check
```

The verification runner uses separate build directories, package locks, and
nonincremental compilation. It runs the native suites and lint checks, checks
browser-safe code for `wasm32-unknown-unknown`, and runs the authored cooker
workflow. The optional flags add native/WASI execution, original C#/C++ source
comparisons, and the actual pinned runtime bridge. Setup and the scope of those
checks are described in [BUILDOUT-VERIFICATION.md](BUILDOUT-VERIFICATION.md).
The EOD comparison executes five unchanged original C# handlers; the cooked
comparison executes the actual runtime from selected verified packs on both
native and WASI targets.

## Components and integration surfaces

| Component | Delivered behavior | Main consumer |
| --- | --- | --- |
| `crates/legacy-formats` | Bounded FAR1/FAR3/DBPF/IFF reads; original-aware resource-map rebuilding; PALT/SPR2 and OBJf encoding; RefPack/QFS; semantic resources; rigs, animations, mesh and audio metadata | Cooker, creator, content resolver |
| `crates/content-ir` | Ordered PIFF application, namespaces, tuning and localization; effective identities; immutable manifests, packs, tuning packs and load plans | Simulation and browser loader |
| `tools/asset-cooker` | Actual import → resolve → cook → verify; named scopes and tuning inputs; explicit codecs, provenance and demand groups | Build/import tooling |
| `crates/sim-core/src/interactions` | Source-derived offers, permissions, detached UI queries, validated intents, queue/cancel/start transitions | Swarm A's real VM |
| `crates/eod-runtime` | Native private host, eight handlers including three cooperative games, scoped sessions/controllers, checkpointed private state and persistence recovery | Swarm E's lot/transport/storage providers |
| `tools/creator` | Offline resource CLI, atomic guarded add/remove/edit/rekey transactions, guarded SPR2 packages, typed palette/BHAV/BCON/string/SLOT/OTF edits and exact BMP pixel editing | Creators and migration tools |
| `crates/content-runtime-bridge` | Validated source and hash-bound cooked-release conversion to actual `ContentSet`, real bounded queries, isolated snapshot/accepted-tick replay and creator inspection | Swarm A simulation and Swarm F composition |
| `tools/swarm-b-check` | Test-only manifests and cross-target fixtures compiling the actual owned modules | Swarm F integration |
| `tools/swarm-b/*census*` | Pinned source and registration inventories with concrete open leaves | Compatibility planning and qualification |

The portable pack codec lives in `content-ir::packs`; the cooker reexports that
same implementation. A browser consumer can verify a manifest selected by a
trusted release digest, calculate the dependency closure, fetch only selected
pack hashes, validate each pack and its membership, then decode the explicitly
tagged resource bytes. Presence in an unverified cache is not readiness.

An effective resource's metadata includes its byte hash, pack hash, decoder,
dependencies, criticality, locale, variants, source hash, ordered patch hashes,
tuning identity, origin, and recorded redistribution decision. Canonical
metadata and immutable pack bytes have deterministic SHA-256 identities. Public
distribution requires an explicit recorded license and permission; the tools do
not infer those rights from a filename or repository location.

## Package status and remaining acceptance gates

| Work package | Implemented and evidenced | Remaining acceptance |
| --- | --- | --- |
| W01.1 | Explicit container variants, bounded extraction/decompression, unknown-preserving IFF envelopes, original-aware v0/v1 resource-map rebuilding; original C# FAR3 and historical C++ IFF comparisons | W00 contract adoption; broader installation corpus, ambiguous maps and excluded variants |
| W01.2 | Source-cased PIFF matching and user suppression, deferred moves/removals/additions, namespace lookup, tuning precedence and localization, deterministic effective identities | Full frozen installation/patch manifest and original-runtime differential cases |
| W01.3 | CPU sprite color/alpha/depth, bounded PALT and SPR2 1000/1001 authoring, source C# encoder/reader comparisons, DGRP/SLOT, Vitaboy and audio/HIT metadata | Renderer/pose evaluation, compressed audio playback/HIT execution, additional legacy formats and visual reference comparisons |
| W01.4 | Working source import and resolver, immutable semantic/visual/audio/opaque packs, actual effective tuning, verified load closure, source-independent runtime binding and native/WASI cooked replay | Browser cache/network integration; complete dynamic BHAV/resource dependency declarations from the VM/content contract |
| W06.1 | Source-derived interaction module and provider traits; detached query and in-tick modes; guarded intents; queue/cancellation behavior; separate actual-interpreter source routine and accepted-tick replay proof | Complete check-tree/queue adapter, advertisement and query-state mapping, scheduler/routing/reservations and production persistence |
| W06.2 | Real chair/bed/appliance source descriptors and explicit scenario requirements | Content-driven walk/reserve/use/animate/need-change/exit with original traces and interrupted/concurrent cases |
| W06.3 | Eight native handlers, including PaperChase/PizzaMaker/Maze; private/public channels and controller capabilities; format 1/2/3 recovery; immutable persistence intents; unchanged-C# component comparisons for five handlers | Production transport, coherent VM/private checkpoint barrier, durable provider integration and original UI/runtime traces |
| W06.4 | Complete checked-in object/source inventory, exact source-selected OBJf lifecycle references, exact server/UI EOD registration census and per-entry open leaves | All enter/use/cancel/leave/save/reconnect scenarios; remaining 22 EOD implementations; W00's complete installation denominator |
| W16.1 | Safe offline inspection/import/export, strict bounded transaction JSON, atomic guarded resource creation/removal/rekey/edit and indexed repacking | Graphical resource browser, effective catalog/patch view and production tool privilege integration |
| W16.2 | BHAV branch/operand editing and CFG inspection; real isolated tick-step/query, bounded watches and frame-position inspection through the bridge | Instruction break/step/yield, actual executed trace and running-lot preview |
| W16.3 | Strings, BCON, SLOT, palettes and exact OTF editing; guarded indexed sprite/palette CLI workflow with explicit alpha policy; raw interchange and actual repacking | Graphical sprite editing, mesh/animation/upgrade authoring, OBJ/MTL/glTF/GLB workflows and contact/event authoring |
| W16.4 | Exact supported BMP city-map validation/editing and source tool disposition inventory | PNG/other map variants, graphical city/neighborhood editing, road reconstruction, server updates and optional-extension qualification |

## Source compatibility decisions

### Containers and payloads

FAR1a and FAR1b must be selected explicitly; their filename length widths differ.
FAR3 supports raw entries and the source reader's Persist/QFS framing. Its inner
length counts command bytes **after** the nine-byte QFS header. The regression
fixture was decoded by the unchanged pinned C# reader as well as Rust.

The normal DBPF entry point supports 1.0/index 7.0 with fixed TGI32 entries.
`dbpf::index_source_compatible` separately implements the pinned reader's unusual
1.1 and 2.0 header branches. Compact/64-bit indexes and nonzero hole tables are
excluded. DBPF compressed entries remain stored bytes unless an explicit codec
handles their contents.

IFF 2.0/2.5 envelopes preserve raw headers, flags, labels and unknown chunks.
`IffDocument` supports exact untouched indexed-file passthrough and validated
structural edits backed by the original resource map. It rebuilds supported
v0/v1 offsets, sizes, identifiers, labels and coverage after edits. Detached
indexed encoding and ambiguous original maps still reject; direct creator
`rsmp` edits are writer-managed. See [indexed-iff.md](indexed-iff.md).
Duplicate chunk identities remain rejected by production imports; the census
separately records the three source files with duplicate unknown `XXXX` records.

[sprite-authoring.md](sprite-authoring.md) documents PALT and SPR2 encoding,
explicit indexed colors, source-supported alpha/depth combinations and preflight
bounds. Exact alpha encoding is the default; source-style quantization is an
explicit option that reports the number of changed alpha values.

Behavior readers cover the supported source BHAV, OBJD, OBJf, TTAB, BCON,
STR#/CTSS/TTAs, GLOB, SLOT and PIFF layouts. [OBJf support](objf.md) preserves the
source table header, entries and tails; runtime admission retains the actual
simulation's narrower entry-count and representability limits. TTAB's TSBO layout has an explicit
decoder/pack tag. Unknown positive string layouts and the old TTAB 2/3 cases
remain explicit semantic exclusions; envelope preservation does not imply
semantic support. XML OTF processing preserves untouched source bytes and
rejects DTD/entity expansion and unsupported structural edits.

Standalone Vitaboy resources use big-endian integers and **little-endian f32
bits**, matching `IoBuffer.ReadFloat`. The named FreeSO coordinate policy
preserves negative zero and original animation property order. BCF/CMX/BMF,
derived skinning/pose evaluation and sprite postprocessing are remaining work.
WAVE PCM metadata, XA/UTK headers and HIT resource metadata have explicit tags;
encoded audio and HIT bytecode are not executed by these readers.

### Interaction semantics

UI offer checks receive detached state; temporary register arrays are reset
from the query baseline before **each** out-of-tick check. The separate in-tick
entry point retains legitimate accumulation. The real interpreter must spend
the supplied check budget and supply detached provider data.

Repeated gameplay interactions remain legal. Monotonic command sequences reject
transport retries/replays independently of gameplay duplication. The adapter
must bind the sequence and actor generation to authenticated authority.

Three bounded source deviations are documented in the interaction module and
verification record: visit the TS1 cancellation tail once when the source loop
could retain the same item indefinitely; reject a ParentIdle cancellation that
would delete an active descendant; and preserve the active prefix after a failed
immediate frame start. These require explicit scheduler fault/unwind or atomic
start handling, and do not claim eventual parity with a nonterminating source.

### EOD authority and recovery

EOD tickets bind host scope, epoch, instance and generation. The trusted
transport supplies connection identity; payload fields never establish the
sender's authority. Scoped identities are checked for commands, private output,
disconnect and rebind. Public object events and private UI/checkpoint data use
distinct types with redacted private Debug output.

Timer, DanceFloor, Signs, Scoreboard, PermissionDoor, PaperChase, PizzaMaker and
TwoPersonJobObjectMaze are native-enabled. The other 22 registrations remain
explicitly unsupported, with source-specific implementation/recovery/provider
leaves in [eod-coverage.md](eod-coverage.md). The [component oracle](eod-source-oracle.md)
executes the five original C# handlers in controlled scenarios. All 30 still
require qualification against the complete original application and production
providers.

The [cooperative host](cooperative-eod.md) keeps one bounded private group per
game, advances it once per authoritative tick, and admits controller callbacks
through a separate native capability. Its private checkpoint format 3 preserves
hands, maps, phase and random streams while retaining formats 1/2. Restored games
pause until the recorded controller and retained seats rebind. Invalid private
state and failed output admission cannot partially advance a game.

Checkpoint restore requires a newer host epoch and a trusted coherent VM/private
stamp. Persistent handlers checkpoint immutable writes before dispatch and
reconcile pending/provider state before rebind; the production provider must
supply durable atomic deduplication and fencing. Native policy differences and
private random behavior are explicit in the handler documentation.

## Corpus evidence

[content-corpus.json](../compat/content-corpus.json) records a read-only scan of
the pinned Git tree. Newly authored fixtures and later branch commits cannot
silently expand that denominator. It includes 745 IFF/PIFF paths, five XML OTF
files, and one `.otf` font explicitly excluded from object tuning.

The census retains the pinned source denominator and now includes all 471 OBJf
tables and their 14,685 entries. Generated lifecycle dependencies distinguish
OBJD selection from required OBJf tables and retain exact resource ordinals.
Missing or ambiguous required tables remain unresolved. The original-aware
indexed writer is qualified separately in
[indexed-iff-verification.json](indexed-iff-verification.json).

[object-matrix.json](../compat/object-matrix.json) and the normalized
[cohort index](../../fixtures/objects/cohorts/index.json) preserve object-local
identities and shared source evidence. Effective IDs remain null until a
specific ordered source/patch/tuning/dependency manifest is resolved. The
[object coverage guide](object-coverage.md) explains the real chair/bed/appliance
descriptors and how to expand one concrete leaf. All gameplay evidence remains
unverified.

The visual probe accepted all 411 checked-in standalone Avatar files: 50
animations, 121 meshes, 120 bindings and 120 appearances. It also accepted all
20,288 PALT, 2,448 DGRP and 402 SLOT resources encountered in its IFF scan, plus
2,700 SPR2 resources; 32 SPR2 resources lacked a same-file palette dependency.
These are reader-layout observations, with separate rendering and playback
gates.

## Handoff to the other swarms

| Owner | Concrete integration work |
| --- | --- |
| A — Simulation | Compose the pinned content/runtime bridge; finish `WorldProvider`, `CheckTreeProvider`, and `QueueRuntime` integration; map query side effects/advertisements; keep queue active-prefix and real frames coherent; handle routing/reservations/callbacks inside accepted ticks |
| C — Views/audio | Consume CPU resources without changing source f32 bits or event order; implement pose/skinning/render variants and audio/HIT execution; declare any additional semantic animation dependencies |
| D — Browser UX | Bind advisory offers to validated intents; verify the trusted manifest digest and pack members; fetch the requested closure; decode palette-dependent resources with their dependencies; connect real creator UI and cache/lifecycle policy |
| E — Online | Supply authenticated nonreusable connection identities, stable distinct host scopes, fencing, private transport/storage, coherent checkpoint barriers and durable effect providers |
| F — Integration | Adopt the standalone packages into the authoritative workspace; map local seams to W00 contracts and AcceptedTick; freeze the full content baseline; run actual VM/native/browser/reference-runtime qualification |

The original C# tree is unchanged. Shipping `sim-core` and root workspace files
remain with their owners; the interaction harness compiles the actual module
without introducing a substitute VM crate.
