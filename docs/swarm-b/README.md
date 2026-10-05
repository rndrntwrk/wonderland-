# Swarm B — content, objects, and creator tools

This branch adds a working Rust content pipeline, an offline creator CLI, and
source-backed interaction and private EOD foundations. It also records every
checked-in object and registered EOD in a reproducible coverage inventory.

The source baseline is [`4c6b3e8f5835b228723caea3c9f683c62f244f73`](https://github.com/rndrntwrk/wonderland-/tree/4c6b3e8f5835b228723caea3c9f683c62f244f73).
The assigned packages are W01, W06, and W16 from the supplied parallel rewrite
plan. [IMPLEMENTATION.md](IMPLEMENTATION.md) records the ownership decisions;
[VERIFICATION.md](VERIFICATION.md) records the final checks and review outcomes.

Full Swarm B completion still requires the actual simulation, routing,
presentation, transport, persistence, and reference-runtime providers. The
package table below distinguishes implemented functionality from those gates.
No original object is counted as gameplay-complete by parsing its resources.

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
round trips, SHA-256/version-guarded edits, OTF tuning edits, and semantic BMP
city-map editing. Its live debugger capability report explicitly remains
unsupported until a real simulation adapter is supplied.

```sh
bash tools/swarm-b/verify.sh
python3 tools/swarm-b/eod-census.py --check
python3 tools/swarm-b/object-census.py --check
```

The verification runner uses separate build directories, package locks, and
nonincremental compilation. It runs the native suites and lint checks, checks
browser-safe code for `wasm32-unknown-unknown`, and runs the authored cooker
workflow. The optional execution parity and original-reader checks are described
in [VERIFICATION.md](VERIFICATION.md).

## Components and integration surfaces

| Component | Delivered behavior | Main consumer |
| --- | --- | --- |
| `crates/legacy-formats` | Bounded FAR1/FAR3/DBPF/IFF reads; RefPack/QFS; semantic resources; sprites, rigs, animations, mesh and audio metadata | Cooker, creator, content resolver |
| `crates/content-ir` | Ordered PIFF application, namespaces, tuning and localization; effective identities; immutable manifests, packs, tuning packs and load plans | Simulation and browser loader |
| `tools/asset-cooker` | Actual import → resolve → cook → verify; named scopes and tuning inputs; explicit codecs, provenance and demand groups | Build/import tooling |
| `crates/sim-core/src/interactions` | Source-derived offers, permissions, detached UI queries, validated intents, queue/cancel/start transitions | Swarm A's real VM |
| `crates/eod-runtime` | Native private host, scoped authenticated sessions, recipient delivery, checkpoints, Timer translation, durable-effect handoff | Swarm E's lot/transport/storage providers |
| `tools/creator` | Usable offline resource CLI, guarded BHAV/BCON/string/SLOT/OTF edits and exact BMP pixel editing | Creators and migration tools |
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
| W01.1 | Explicit container variants, bounded extraction/decompression, unknown-preserving IFF envelopes, endian/version/overlap/truncation tests; focused original C# FAR3 comparison | W00 contract adoption; broader installation corpus and excluded variants |
| W01.2 | Source-cased PIFF matching and user suppression, deferred moves/removals/additions, namespace lookup, tuning precedence and localization, deterministic effective identities | Full frozen installation/patch manifest and original-runtime differential cases |
| W01.3 | CPU sprite color/alpha/depth, DGRP and SLOT, standalone Vitaboy metadata with source float bits/order, audio/HIT metadata | Renderer/pose evaluation, compressed audio playback/HIT execution, additional legacy formats and visual reference comparisons |
| W01.4 | Working source import and resolver, immutable semantic/visual/audio/opaque packs, actual effective tuning, verified load closure and authored rebuild demo | Browser cache/network integration; complete dynamic BHAV/resource dependency declarations from the VM/content contract |
| W06.1 | Real source-derived interaction module and provider traits; detached query and in-tick modes; guarded intents; queue and cancellation behavior | Real interpreter, scheduler, routing/reservations, accepted-tick mapping and persistence |
| W06.2 | Real chair/bed/appliance source descriptors and explicit scenario requirements | Content-driven walk/reserve/use/animate/need-change/exit with original traces and interrupted/concurrent cases |
| W06.3 | Native host, source-translated Timer, typed private/public channels, scoped tickets, bounded checkpoints and effect requests | Production transport, coherent VM/private checkpoint barrier, durable outbox/provider integration and Timer UI/runtime reference traces |
| W06.4 | Complete checked-in object/source inventory and exact server/UI EOD registration census; per-entry open leaves | All enter/use/cancel/leave/save/reconnect scenarios; remaining 29 EOD implementations; W00's complete installation denominator |
| W16.1 | Safe offline inspect/list/validate/import/export/extract and metadata; unknown preservation and precise guarded edits | Graphical resource browser, effective catalog/patch view and production tool privilege integration |
| W16.2 | BHAV branch/operand editing and CFG inspection; explicit isolated debugger interface | Real VM break/step/yield/watch/trace and running-lot preview |
| W16.3 | Strings, BCON, SLOT and exact OTF editing; raw resource interchange and actual repacking | Sprite/mesh/animation/upgrade authoring; OBJ/MTL/glTF/GLB workflows and contact/event authoring |
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
`IffDocument` supports exact untouched indexed-file passthrough. Structural
indexed edits are rejected until resource-map rebuilding exists. Duplicate
chunk identities are rejected by production imports; the census separately
records the three source files whose duplicates are unknown `XXXX` records.

Behavior readers cover the supported source BHAV, OBJD, TTAB, BCON,
STR#/CTSS/TTAs, GLOB, SLOT and PIFF layouts. TTAB's TSBO layout has an explicit
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

Only Timer is runtime-enabled. Every other registered plugin remains explicitly
unsupported, with source-specific implementation/recovery/provider leaves in
[eod-coverage.md](eod-coverage.md). Checkpoint restore requires a newer host
epoch and a trusted coherent VM/private checkpoint stamp. Effect retries use
stable identities, but the production provider must supply durable atomic
deduplication and reconciliation.

## Corpus evidence

[content-corpus.json](../compat/content-corpus.json) records a read-only scan of
the pinned Git tree. Newly authored fixtures and later branch commits cannot
silently expand that denominator. It includes 745 IFF/PIFF paths, five XML OTF
files, and one `.otf` font explicitly excluded from object tuning.

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
| A — Simulation | Supply `WorldProvider`, `CheckTreeProvider`, and `QueueRuntime`; map full routine/interaction identities from effective content; isolate UI checks; keep queue active-prefix and real frames coherent; handle routing/reservations/callbacks inside accepted ticks |
| C — Views/audio | Consume CPU resources without changing source f32 bits or event order; implement pose/skinning/render variants and audio/HIT execution; declare any additional semantic animation dependencies |
| D — Browser UX | Bind advisory offers to validated intents; verify the trusted manifest digest and pack members; fetch the requested closure; decode palette-dependent resources with their dependencies; connect real creator UI and cache/lifecycle policy |
| E — Online | Supply authenticated nonreusable connection identities, stable distinct host scopes, fencing, private transport/storage, coherent checkpoint barriers and durable effect providers |
| F — Integration | Adopt the standalone packages into the authoritative workspace; map local seams to W00 contracts and AcceptedTick; freeze the full content baseline; run actual VM/native/browser/reference-runtime qualification |

The original C# tree is unchanged. Shipping `sim-core` and root workspace files
remain with their owners; the interaction harness compiles the actual module
without introducing a substitute VM crate.
