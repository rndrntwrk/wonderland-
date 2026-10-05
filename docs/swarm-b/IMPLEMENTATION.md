# Swarm B implementation and integration plan

Source: `4c6b3e8f5835b228723caea3c9f683c62f244f73` in `rndrntwrk/wonderland-`.
Authority: the supplied FreeSO Rust Browser Rewrite Plan and Parallel Work Packages (W01, W06, W16).

## Constraints and integration decisions

- Keep authoritative simulation at 30 Hz. No renderer, browser, network or database dependency enters simulation rules.
- Read source implementations before translating a format or behavior. Unsupported versions fail explicitly; raw unknown IFF resources survive tool round trips.
- Bound imported bytes, counts, output sizes, offsets and decompression. Use no unsafe Rust.
- Source content, patch order, tuning, dependencies, locale, variants and rights affect effective identity. No original asset redistribution is assumed.
- UI checks operate on an isolated snapshot. EOD private state and recipient messages are separate from synchronized VM events and common snapshots.
- Durable effects are requested through provider interfaces, never executed by a client mirror.
- Do not mark an entire parent package or object/plugin cohort complete from foundation tests. Actual provider integration and original-runtime evidence remain separate gates.
- All existing C# source stays unchanged. New code is in owned paths. No root Cargo workspace, shared contracts, simulation root, lockfile or another swarm's modules are changed.
- Rust 1.90.0 is the verification toolchain; library packages are standalone until Swarm F adopts them. An interaction check harness compiles the actual `sim-core/src/interactions` files without claiming a functioning VM.
- Independent builders own disjoint files. The controller alone stages commits, integrates manifests and publishes the branch. This implements the user's requested parallel execution without shared-file edits.

## Shared parser API (local to W01; not a replacement for W00 contracts)

`wonderland-legacy-formats` imports as `wonderland_legacy_formats`.
It exports `Result<T>`, `Error`, `ErrorKind`, `Limits`, and `reader::Reader`.
`Error::new(kind: ErrorKind, offset: usize, context: impl Into<String>)`.
Error kinds: `Truncated`, `InvalidMagic`, `UnsupportedVersion`, `InvalidData`, `LimitExceeded`, `Overflow`, `Overlap`, `Duplicate`.
Error fields: `kind`, `offset`, `context`. Errors implement Display and std::error::Error.
Limits fields (usize): `max_input_bytes`, `max_entries`, `max_resource_bytes`, `max_total_decoded_bytes`, `max_string_bytes`, `max_pixels`, `max_vertices`, `max_frames`, `max_depth`.
`Limits::default()` is finite. `check_input(&self, bytes: &[u8]) -> Result<()>` and `check_count(&self, count: usize, limit: usize, offset: usize, context: &str) -> Result<()>`.
Reader methods: `new(&[u8])`, `position()`, `remaining()`, `read_bytes(usize) -> Result<&[u8]>`, `skip(usize) -> Result<()>`, `seek(usize) -> Result<()>`, `u8`, `i8`, `u16_le`, `u16_be`, `i16_le`, `i16_be`, `u32_le`, `u32_be`, `i32_le`, `i32_be`, `u64_le`, `u64_be`, `f32_le`, `f32_be` (numeric methods return Result), and bounded `c_string(max: usize) -> Result<String>` (UTF-8; legacy byte encodings are handled explicitly by format code).
All input-derived allocations require a limit before allocating. Reader never owns or mutates source bytes.

IFF interface: `iff::ChunkKey { kind: [u8;4], id: u16 }`; `iff::IffChunk { key, flags: u16, label: [u8;64], data: Vec<u8> }`; `iff::IffFile { header: [u8;64], chunks: Vec<IffChunk> }`; `iff::decode(bytes, &Limits) -> Result<IffFile>`; `iff::encode(&IffFile, &Limits) -> Result<Vec<u8>>`. Keep original header bytes and labels for untouched lossless output; unsupported reserved indexing metadata must be preserved or rejected on structural edits rather than silently corrupted.

## Task 1: Bounded containers and IFF envelopes (W01.1)

Owned: `crates/legacy-formats/{Cargo.toml,src/lib.rs,src/error.rs,src/limits.rs,src/reader.rs,src/far/,src/dbpf/,src/iff/,src/compression.rs,tests/containers.rs}`.
Read FAR1/FAR3, FAR3/Decompresser.cs, DBPFFile.cs, IffFile.cs at the pinned source. Implement safe FAR1a/1b, FAR3, DBPF baseline variants, RefPack/QFS decompression, raw IFF/PIFF envelope read/write and unknown resource preservation. Retain canonical resource keys without filesystem extraction. Reject truncation, overlap including index/header overlap, duplicate identities, invalid counts, malformed backreferences and output expansion before allocation. Do not guess unsupported layouts.
Provide a discoverable container index API and lazy bounded entry extraction; notify the controller of signatures early. Own core helper APIs above; coordinate additions instead of changing agreed methods. Declare modules `semantic`, `sprites`, `vitaboy`, `audio_meta` without editing their files.
Meaningful tests must use hand-built literal/source-derived vectors for valid endian/version cases and hostile inputs, every truncated prefix of representative input, overlapping ranges and decompression bounds. Read tests-first guidance; show at least the missing behavior failing before implementation. No network or proprietary fixture requirement. Run targeted tests then native/WASM checks. Report exact supported variants and exceptions.

## Task 2: Semantic resources, patch/tuning resolution (W01.2 and W01.1 chunk consumers)

Owned: `crates/legacy-formats/src/semantic.rs`, `crates/legacy-formats/tests/semantic.rs`, `crates/content-ir/{Cargo.toml,src/lib.rs,src/objects.rs,src/patches.rs,src/tuning.rs,src/strings.rs,tests/resolution.rs}`.
Read BHAV, OBJD, TTAB, BCON, STR/TTAs/GLOB, SLOT, PIFF and PIFFRegistry, WorldObjectProvider and tuning providers before implementation. Decode behavioral resource data with raw fields retained and explicit supported versions; do not execute BHAV. Implement bounded PIFF patch stream decoding and source-faithful remove/move/edit/add semantics, user-patch suppression of non-user patches for the same source, stable explicitly supplied intra-category order, and transactional rejection of invalid patch application. Expose resource lookup private/semiglobal/global, exact tuning precedence and localization fallback; effective identity must change with source/patch order/tuning and be deterministic. Use SHA-256 rather than unspecified Rust hashers.
Expose decoded content APIs for cooker and creator. Unknown raw chunks remain in IFF. No renderer types. Use the shared parser API; contact the controller for any missing surface. Test hand-checked versioned BHAV instruction branches/operands, patches/removals/moved IDs/multiple patches/user precedence, missing resources, localization and resource bounds. Record unsupported layouts and missing source corpus truthfully.

## Task 3: Visual, rig, animation and audio metadata (W01.3)

Owned: `crates/legacy-formats/src/{sprites.rs,vitaboy.rs,audio_meta.rs}`, `crates/legacy-formats/tests/{visual.rs,vitaboy.rs,audio_meta.rs}`.
Read PALT, SPR, SPR2, DGRP, SLOT, Vitaboy Animation/Skeleton/Mesh/Binding/Appearance/Outfit and audio resource metadata. Implement source-backed bounded color/alpha/depth sprite decoding, drawing group zoom/rotation/flags/transforms, skeleton binds and animation duration/time-property metadata with original ordering and xevt values retained. Implement mesh/appearance/binding resource metadata and audio metadata as feasible to source; reject unsupported variants explicitly. Preserve source coordinate conversion with a named policy and no GPU dependencies. Any simulation-critical float data must preserve source f32 bits/order, not introduce f64 reinterpretation. Avoid image generation or reconstructed guesses.
Use shared Limits/Error/Reader API. Types should derive serde serialization for pack metadata where useful, but byte codecs stay exact. Tests: hand-checked color/alpha/depth, RLE boundary/row overflow, known bind hierarchy, invalid cycles/indices, animation time-property order, finite numeric policy, count and truncated-input rejection. Report exact support and gaps; no claim of full visual parity without renderer and original assets.

## Task 4: Source-backed interaction offers and queues (W06.1 foundation)

Owned: `crates/sim-core/src/interactions/{mod.rs,offers.rs,query.rs,queue.rs,adapters/}`, `tests/interactions/`, `tools/swarm-b-check/interactions/`.
Read VMEntity interaction/check tree code, VMQueuedAction, VMThread queue/cancellation behavior, TTAB flags. Port stable queue flags/priorities, duplicate/cancel/head-tail/must-run/parent-exit rules and isolated UI offer query behavior. Keep all gameplay policy sourced and deterministic. Use trait adapters for unavailable VM/check-tree/route provider, explicit validated intent versus mere UI offers, stale entity generation/revision handling, bounded queue growth, and unambiguous ordered events. Do not manufacture a simulation engine, copy global contract IDs, or hardcode chair/bed gameplay. The check harness builds actual interaction module source using `[lib] path` and is not a shipping sim-core crate. Use test fixtures to prove isolation of RNG/temps/check advertisements from UI queries and legitimate in-tick mutations when requested, queue/cancel/failure semantics and rejection of stale/unauthorized requests. Document the concrete integration functions Swarm A/F must provide. All synthetic behavior is marked fixture-only.

## Task 5: Native EOD host, registry and cohort evidence (W06.3/4 foundations)

Owned: `crates/eod-runtime/`, `fixtures/eod/`, `docs/swarm-b/eod-coverage.md`, `tools/swarm-b/eod-census.py`.
Read actual VMEODServer and UIEODController registration dictionaries and representative plugin source. Generate exact registered plugin IDs and server/UI source anchors; do not invent registrations. Create a typed native-only private host/protocol with authenticated recipient/connection routing, per-plugin event allowlists, message/participant/timer limits, disconnect/timeout handling and schema/version checked private checkpoints. Public VM events and private UI outputs use distinct types; private state/RNG cannot enter shared state by accident. Explicit restore or abort/refund policy per registration; unspecified plugins are unsupported at runtime, never fake-completed. Define durable-effect provider handoff without charging/transferring funds in this crate. Implement source-backed simple plugins only where exact behavior can be verified; record every remaining registered plugin as unverified with concrete leaf IDs and prerequisites. Test forged messages/recipient spoofing, size/epoch/version rejects, stale sessions, private redaction, deterministic timeout, restoration and effect request idempotency under retry. Never return private checkpoint data through generic Debug or public projections.

## Task 6: Creator inspection, safe editing and authoring tools (W16 foundations)

Owned: `tools/creator/`, `tests/tools/`, `docs/swarm-b/creator.md`, `docs/compat/tools.json` (new B-owned tool disposition only).
Use real legacy/container/content APIs from Tasks 1/2/3. Build a CLI/library resource workspace with inspect/list/validate, bounded unknown-preserving import/export, precise resource mutations and version/hash guarded edits with atomic output. Expose practical BHAV branch/operand editing and CFG validation, string/tuning/slot edits and metadata export. Provide trace/watch/debugger interfaces with an explicit isolated provider boundary; live VM stepping must be unsupported until a real provider exists. Do not pretend headless output is a graphical Volcanic replacement. Include city data-map validation/editing and tools inventory if source permits; never use lossy transformation for semantic map pixels. Implement legitimate interchange where possible and list exact unsupported capabilities. Test import-inspect-edit-export-reopen with no unrelated byte changes, bad edit atomicity, unknown chunk retention, path traversal/symlink boundaries, branch validation and source hash conflicts. Document commands that can run without external assets.

## Task 7: Immutable asset manifests, cooker and demand-load verification (W01.4)

Owned by controller: `crates/content-ir/src/{manifest.rs,visual.rs}`, `tools/asset-cooker/`, `fixtures/packs/`, `tests/content/`, `tools/swarm-b/verify.sh`, `docs/swarm-b/README.md`.
Build real import-resolve-cook-verify pipeline consuming Tasks 1/2/3. Packs and manifests are deterministic, versioned, SHA-256 addressed and bounded; original/derived/imported/redistributable provenance is explicit. Every resource lists semantic dependencies and simulation-critical readiness; closure planning fetches required packs without an entire installation. Reject corrupt/mixed-version/cyclic/missing dependencies and unsafe publish rights. Changing content/patch/tuning invalidates derived cache identity. Provide a small redistributable synthetic fixture with source-independent bytes and deterministic command-line reproduction. Test at least two repeat builds, corruption, dependency closure, unresolved critical dependency, patch invalidation and permissions.

## Task 8: Independent review, integration and handoff

Obtain parsing/authority-focused independent review. Run native tests, rustfmt/clippy, wasm32-unknown-unknown builds for browser-safe libraries and CLI end-to-end fixture commands. Track test commands and results in `docs/swarm-b/VERIFICATION.md`, and map each parent work package to implemented code, evidence and remaining provider/corpus requirements. Publish dedicated branch PRs after validation; do not merge or claim full declared baseline parity. Preserve a useful release handoff for Swarms A/C/D/E/F.
