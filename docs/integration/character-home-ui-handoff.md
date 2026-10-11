# Wonderland UI integration handoff

## Decision and scope

Continue development on the Wonderland fork, with the action-focused UI as a separate stack. The inspected simulation and content branches contain useful foundations, but neither supplies the live character-creation, wardrobe, catalog, ownership and production-rendering interfaces required to replace the current illustrated preview. They should be integrated through explicit adapters after workspace and authority contracts agree.

This note records read-only source audits on 5 October 2026. It does not merge, build or qualify another branch. Links are pinned to the audited commits so later branch changes cannot silently alter this assessment. The UI baseline is PR #5, remote `b921c76fb2bea478a0685c9857e00e0fc309c097`, whose tree equals local `bd08bd1652bec851e42a9ad8348c4c3902a88745`.

## UI contract to preserve

The character/Home increment introduces a distinct bounded authoring snapshot and reducer alongside the existing game shell. A draft remains local until a request with operation identity and base revision receives a matching typed outcome plus a complete validated newer snapshot. Only then may the browser project profile, look, room and budget data into the compatible game shell. This ordering matters because `UiEvent::ProjectionUpdated` clears the old shell's pending work.

The preview provider and local-storage envelope demonstrate those behaviors with authored fixtures. They must not become production spending, permission, collision or ownership authorities. A live provider needs coordinated source projections, authenticated admission, stable identities, revision handling and durable outcomes. The presentation layer should keep its current action surfaces: character stage, tappable map, world-attached object actions, visual catalog, placement ghost and inventory drawer.

## Production renderer handoff (Swarm C)

No production renderer or render/pick adapter was found in the audited A/B heads. The current Home image, character cutouts and directional furniture sprites are fixed-camera presentation assets. Replacing them needs a render frame and inverse-picking contract that agrees with simulation and content identity.

| Required renderer output or input | UI use |
| --- | --- |
| Stable entity ID/incarnation plus source revision | Keep selected objects and menus attached to the correct live entity. |
| Canonical lot position, level, orientation and scene camera | Map clicks and placement drafts without treating normalized illustration coordinates as simulation tiles. |
| Screen projection, visibility, occlusion and hit result | Anchor world menus, reveal focused targets, avoid selecting objects hidden by walls/chrome. |
| Effective content/appearance identity and readiness | Show loading/unavailable states while meshes, skeletons, textures and dependencies become available. |
| Avatar rig, appearance and simulation animation timeline | Replace static stage/world sprites with real appearance and animation without inventing action completion. |
| Placement visualization from authoritative validity results | Draw footprint/ghost and reason while confirmation remains a service/simulation operation. |

Keep DOM focus, readable labels, keyboard actions and compact HUD/menu semantics when the world surface becomes canvas/WebGPU. Camera controls should move the real camera, and a rotation should select a real orientation; rotating the current flat background is not that implementation.


## Simulation and world integration (Swarm A)

Audited [PR #6](https://github.com/rndrntwrk/wonderland-/pull/6), commit `8a0e251d19e222a0a6833d7408ca629f674e1729`. These are source findings at that commit, not a claim that its separate branch has been merged into the UI workspace.

### Runtime seam

[`SimRuntime`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/runtime.rs) owns `SimState`, content and runtime role. Its constructor accepts `ContentSet`, `LotModel`, `RuntimeConfig` and `RuntimeRole`; it exposes read-only state/content access, `step(&AcceptedTick)`, behavior queries, snapshots and restore. This is the simulation foundation. It does not expose the UI's `UiProjection`, a production render frame, picking or a browser game ABI. The exported WASM replay functions are acceptance-harness entry points, not a running game renderer.

Runtime admission uses lot/authority epoch/tick/content/RNG context and applies a cloned candidate atomically. Preserve that boundary when adding UI adapters: the browser sends an intent to admission and renders projected outcomes, rather than directly editing simulation fields. `RuntimeHost` remains private and construction does not inject external interaction/content/service providers. The B interaction helpers therefore still need deliberate host integration.

### Build, spending and inventory

The actual reusable world API is [`WorldState::{preview_build,begin_build_commit,complete_build_commit}`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/world/build/mod.rs). `BuildIntent` carries an operation, actor, architecture revision and semantic edits. `BuildAuthority` supplies rights and versioned account/catalog/permission information. Placement edits contain full `WorldObject` data and a catalog ID: this is an internal semantic structure, not a safe browser purchase DTO. Content and admission must construct and verify it.

Preview does not charge or place. Begin returns a durable effect or the already-completed outcome; completion checks the authenticated server confirmation against the operation, actor, owner, preview hash, exact cost and durable identities. Keep all three outcomes visible in the live adapter:

| Source outcome | Required player behavior |
| --- | --- |
| `Committed` | Display the accepted geometry, instance IDs and account projection. |
| `Rejected` | Retain a useful draft and explain the source reason. |
| `NeedsReconciliation` | Cost has already committed but geometry could not be applied. Preserve the receipt and show reconciliation status; do not label this as an ordinary rejection or invent a refund. |

The audited `AcceptedCommand` surface has no integrated Build command. Joining world geometry with VM creation/initialization/move/delete lifecycle remains an integration task. An accepted geometry edit alone does not establish that both systems changed atomically. There is one pending build; no public cancellation/timeout path is exposed. The live UI must not promise cancellation after submission unless the authority provides it. Undo is a separately guarded operation and deletes are not generally undoable.

[`validate_placement`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/world/placement.rs) supplies placement status/blocker/support-slot and resumable script checks. Project those results instead of recreating production validity rules in Leptos. The current 8×6 authoring provider is expressly a local preview and will be replaced.

TS1 neighbor inventory and the TSO external inventory request seam are not a player-owned TSO inventory service. The authoring preview's stored instances do not imply that service is implemented. Live ownership, stable item identity, wallet revision and receipts belong in the service adapter.

### Characters, identity and spatial mapping

[`AvatarState`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/avatars/state.rs) includes name, motives, animation timeline, outfits, lifecycle and permissions. Its `budget_mirror` is explicitly not spending authority. [`OutfitReference` and outfit helpers](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/avatars/outfits.rs) support ID, name and legacy references, resolution and appearance state. They do not provide admitted player profile creation, an owned wardrobe service or browser avatar assembly. A live creator needs those operations and a coordinated character projection.

| UI presentation data | Source data | Adapter requirement |
| --- | --- | --- |
| Opaque string entity/profile IDs and UI generation | [`ObjectId(i16)`, `EntityRef` generation `u32`, `PersistentId(u32)`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/ids.rs) | Checked, session/lot/epoch-aware mapping; never unchecked casts or asset-key reuse. |
| Normalized screen anchor / authoring integer cell | Canonical lot x/y in sixteenths of a tile and level | Renderer owns projection, inverse picking, visibility, camera and floor selection. |
| Eight labeled 0–100 needs | Sixteen signed `i16` motive slots | Explicit index and value conversion; preserve source range and unavailable state. |
| Catalog card and displayed price | Effective content identity, rights, versioned authoritative quote | Display a projection; submit identity/revision, not client-authored semantic object data. |

The eight UI motive indices are Energy **5**, Comfort **6**, Hunger **7**, Hygiene **8**, Bladder **9**, Room **13**, Social **14**, Fun **15**, from [`avatars/motives.rs`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/src/avatars/motives.rs). The signed source values and limits must be converted explicitly; copying raw slots into percentage bars is incorrect.

### Workspace requirements

[`sim-core/Cargo.toml`](https://github.com/rndrntwrk/wonderland-/blob/8a0e251d19e222a0a6833d7408ca629f674e1729/crates/sim-core/Cargo.toml) specifies edition 2021, Rust 1.75 and exact `serde=1.0.195`, `bincode=1.3.3`, `sha2=0.10.8`. The UI workspace uses newer serde/serde_json pins. Integrate workspace membership/exclusions and a coherent lockfile intentionally; do not drop an incompatible standalone manifest into the root workspace. Carry `overflow-checks=true` into the effective root release profile, because member profiles are not the workspace's active release profile.

## Content and interactions integration (Swarm B)

### Audit basis

Read-only source inspection; no new build, test, merge, or runtime qualification was performed. “Implemented” below means present in the audited source, not integrated gameplay.

| Stack | Exact audited head |
| --- | --- |
| PR #1 — content | `e41792fc368b167b9c14c37a21b20c1939e0eb14` |
| PR #2 — interactions | `1c9e1af8da802a74b13c152c44a8ed931f1bf14e` |
| PR #3 — creator | `f032ccbc80f7768b71b2da603524f50555c6efcb` |
| PR #4 — qualification inventories | `feaa91549acae3da2931f0b95ba768348f7d56f8` |

Current UI comparison: `bd08bd1652bec851e42a9ad8348c4c3902a88745`, with `wonderland-contracts` / `wonderland-client-app` 0.1.0 and Rust 1.99 / edition 2024. Swarm B source baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

### Implemented content and appearance foundations

- [`crates/legacy-formats/Cargo.toml`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/legacy-formats/Cargo.toml): `wonderland-legacy-formats` 0.1.0, Rust 1.90, edition 2021.
- [`crates/content-ir/Cargo.toml`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/content-ir/Cargo.toml): `wonderland-content-ir` 0.1.0, Rust 1.90, edition 2021.
- [`vitaboy.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/legacy-formats/src/vitaboy.rs): `decode_outfit`, `decode_appearance`, `decode_binding`, `decode_mesh`, and `decode_skeleton` each accept `(&[u8], &Limits)` and return `Result<T>`.
- `Outfit` references light/medium/dark appearance `FileKey { file_id: u32, type_id: u32 }`; `Appearance` contains thumbnail/binding keys; `Binding` contains bone and optional mesh/texture `ResourceKey { group_id, file_id, type_id }`.
- These are bounded decoders, not wardrobe enumeration, ownership checks, character assembly, browser asset URLs, or outfit application. [`content-ir/src/visual.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/content-ir/src/visual.rs) only re-exports visual metadata.
- [`objects.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/content-ir/src/objects.rs): `resolve_content(&ResolveRequest, &Limits) -> Result<ResolvedContent>` consumes source/global/semiglobal IFFs, ordered patches, tuning, locale, variant, rights, and dependencies; returns effective resources, semantic entries, tuning, hashes, and provenance.
- [`semantic.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/legacy-formats/src/semantic.rs): `Objd::{guid, master_id, sub_index, catalog_strings_id, field}` exposes source catalog ingredients; `field("Price") -> Option<u16>` is supported. This is not an authoritative price quote or purchase operation.
- [`strings.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/content-ir/src/strings.rs): `lookup_string(&Strings, usize, LocaleSelection) -> Option<LocalizedString<'_>>` supplies source-compatible locale lookup.
- [`manifest.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/content-ir/src/manifest.rs): `AssetManifest::from_json_verified` verifies an expected manifest digest; `load_plan(roots, phase, variant, cached, limits)` returns dependency-ordered resources, pack digests, and download bytes.
- [`packs.rs`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/crates/content-ir/src/packs.rs): `verify_pack<'a>(&'a [u8], &Digest, &PackLimits) -> ManifestResult<PackIndex<'a>>`; `PackIndex::get(&self, &str) -> ManifestResult<&'a [u8]>`.
- [`docs/swarm-b/cooker.md`](https://github.com/rndrntwrk/wonderland-/blob/e41792fc368b167b9c14c37a21b20c1939e0eb14/docs/swarm-b/cooker.md): the local cooker preserves explicit codecs/raw source bytes; rendering conversion, dynamic dependency completeness, HTTP/cache providers, and gameplay scenes remain outside its scope.

### Implemented interaction boundary

- [`query.rs`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/crates/sim-core/src/interactions/query.rs): `query_offers<W: WorldProvider, C: CheckTreeProvider>(&W, &C, OfferQuery, &InteractionLimits) -> Result<OfferBatch>` and `validate_intent<W: WorldProvider, C: CheckTreeProvider>(&W, &C, &ActionQueue, InteractionIntent, &InteractionLimits) -> Result<ValidatedIntent>` are real implementations.
- `OfferBatch { seen: ViewStamp, offers: Vec<InteractionOffer> }` returns interaction keys, `param0`, labels, and advertisements. Unavailable/hidden/failed-check offers are filtered out; there is no disabled-with-reason `Availability` projection.
- [`queue.rs`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/crates/sim-core/src/interactions/queue.rs): `ActionQueue::{enqueue_validated, cancel_intent, attempt_push, finish_active}` return `QueueTransition<T> { result, queue_revision, events }`. Cancellation can return `Retained`; a click does not always remove the entry.
- [`adapters/mod.rs`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/crates/sim-core/src/interactions/adapters/mod.rs): `WorldProvider`, `CheckTreeProvider`, and `QueueRuntime` require real authorization, detached snapshots/check execution, target liveness, and action start/check implementations. Fixture providers do not qualify those integrations.
- [`tools/swarm-b-check/interactions/Cargo.toml`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/tools/swarm-b-check/interactions/Cargo.toml) compiles the real module as `wonderland-interactions-check` 0.0.0, `publish = false`. No shipping `crates/sim-core/Cargo.toml` exists at this B head; the UI must not depend on that harness.

### Creator and census are separate from player creation/inventory

- [`tools/creator/Cargo.toml`](https://github.com/rndrntwrk/wonderland-/blob/f032ccbc80f7768b71b2da603524f50555c6efcb/tools/creator/Cargo.toml): `wonderland-creator` 0.1.0, Rust 1.90, edition 2021, is an offline library/CLI.
- [`resources.rs`](https://github.com/rndrntwrk/wonderland-/blob/f032ccbc80f7768b71b2da603524f50555c6efcb/tools/creator/src/resources.rs): `ResourceDocument::{import, guard, edit, export}` performs guarded IFF edits. `EditGuard` carries source/resource hashes and format version; `Edit` covers BHAV, strings, tuning, SLOT offsets, and unknown bytes.
- [`creator.md`](https://github.com/rndrntwrk/wonderland-/blob/f032ccbc80f7768b71b2da603524f50555c6efcb/docs/swarm-b/creator.md) excludes graphical previews, effective catalog resolution, live VM execution, and server publication. The inspected API exposes no wardrobe/character creation service; filesystem `Workspace` and BMP city editing are developer-tool capabilities.
- [`object-census.py`](https://github.com/rndrntwrk/wonderland-/blob/feaa91549acae3da2931f0b95ba768348f7d56f8/tools/swarm-b/object-census.py): `build_inventory(corpus, registry)` generates qualification rows/tickets; `expand_ticket(matrix, row)` expands a source leaf. These are not owned-item records.
- Rows expose source/hash/cohort/leaf IDs, GUIDs, raw IFF labels, TTAB/SLOT references, and entrypoints. They set `effective_content_id: null`, `verified_rows: 0`, scenarios to `unverified`, and named gameplay tests to `not_implemented`.
- [`content-census.rs`](https://github.com/rndrntwrk/wonderland-/blob/feaa91549acae3da2931f0b95ba768348f7d56f8/tools/swarm-b/content-census.rs#L302-L328) emits raw OBJD flags and `catalog_strings_id`, but no price or resolved catalog taxonomy/title. No census row provides owner, quantity, persistent owned-item ID, or wallet balance.
- [`eod-runtime/src/effects.rs`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/crates/eod-runtime/src/effects.rs) explicitly has no database, funds transfer, inventory mutation, or production provider. Effect request types are not working purchases.

### Required mappings and workspace integration

| Existing identity | Required distinction |
| --- | --- |
| B `EntityKey { slot: u32, generation: u32 }`, `EntityVersion { key, revision: u64 }` | Map deliberately to UI `EntityRef { id: ObjectId(String), generation: u64 }`; retain incarnation and revision separately. |
| B `InteractionKey { tta_index, scope: Local / Global }` | Maps to a UI offered action; do not use TTAB vector position or visible label as identity. |
| B `ActionId(u64)` | Queue-entry identity; UI `ActionId(String)` instead identifies an offer. Keep a separate operation/queue correlation map. |
| B `ViewStamp` / `InteractionIntent` | Preserve world, actor, target, and queue revisions plus authenticated principal and command sequence. UI character selection is not authority. |
| Manifest logical resource IDs | May contain `/` and exceed UI's 64-byte ASCII `-_.:` ID alphabet; do not put asset paths into UI entity IDs. |
| UI `Anchor` | Normalized illustration coordinate, not an authoritative lot tile/level/direction. Placement needs explicit coordinate mapping. |

Identity sources: [`interactions/mod.rs`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/crates/sim-core/src/interactions/mod.rs), [`offers.rs`](https://github.com/rndrntwrk/wonderland-/blob/1c9e1af8da802a74b13c152c44a8ed931f1bf14e/crates/sim-core/src/interactions/offers.rs), and UI `crates/contracts/src/lib.rs` / `docs/contracts/ui-projection-v1.md` at the compared UI commit.

B's manifests/lockfiles are standalone; shared root workspace membership and the effective lockfile were intentionally left to integration. Preserve actual package names, explicitly add/exclude packages, reconcile dependency versions, and wire the interaction module through the shipping simulation root. Rust 1.90 package minima do not require downgrading UI Rust 1.99. Do not import creator CLI/filesystem functionality into the browser game provider.

### Planned UI work; not claimed as implemented providers

Keep the character stage and lot as the direct-action surfaces. The separate authoring increment supplies preview state/contracts for character drafts and outfit selection; catalog filtering/selection; Live/Build/Buy mode; placement ghost position/rotation/validity; and owned-item projections with pending/accepted/rejected results.

A catalog adapter must combine effective OBJD/CTSS metadata, thumbnails, category mapping, content readiness, player eligibility, and authoritative pricing. Creation, outfit application, buying, placement, inventory return/sell, and balance changes require explicit provider outcomes; none is supplied by these B heads.

Current UI `UiEvent::ProjectionUpdated` clears all pending requests, while bare `Accepted` identifies no newly created character/object or inventory delta. The authoring contract uses a matching typed outcome and validated full snapshot before projecting accepted data into the original shell. Preserve that ordering and bounded schema when replacing fixtures. An authored fixture provider can demonstrate these flows, but must remain explicitly a UI preview rather than a live transaction or gameplay-parity claim.
