# Authoring preview contract v1

> **Scope correction — 5 October 2026:** This document describes the existing limited preview protocol. Fixed identity/look membership, exact catalog records and preview capacity/geometry limits must not be adopted as production game policy. See [preservation scope](../design/action-first/preservation-scope.md) for verified missing capabilities and required contract generalization.

This surface adds character creation, whole-look wardrobe changes, and per-profile room arrangement to the UI preview. It preserves the original `UiProjection`, `UiIntent`, `UiRequest`, `UiEvent`, and `ShellState` APIs. The new data lives in `wonderland_contracts::authoring`; the pure reducer, fixture, and explicitly named demo provider live in `wonderland_client_app::authoring`.

The provider is fixture/demo infrastructure. It does not create accounts, authenticate actors, spend server money, drive simulation, or assign canonical lot coordinates. Presentation IDs, asset keys, and simulation entity IDs remain separate. A live integration must supply coordinated authoritative projections and rights; this preview's exact-delta rules and fixed catalog/budget are not a production economy or placement implementation.

## Entry points and read-only state

```rust
use wonderland_client_app::authoring::{
    AuthoringState, PreviewAuthoringProvider, MAX_PREVIEW_OPERATIONS,
    preview_authoring_projection, preview_project_authoring_to_ui,
};
use wonderland_contracts::authoring::*;

AuthoringState::new(projection: AuthoringProjection) -> AuthoringState;
AuthoringState::dispatch(&mut self, intent: AuthoringIntent)
    -> Result<Vec<AuthoringRequest>, AuthoringError>;
AuthoringState::receive(&mut self, event: AuthoringEvent)
    -> Result<(), AuthoringError>;

PreviewAuthoringProvider::new(projection: AuthoringProjection)
    -> Result<PreviewAuthoringProvider, AuthoringError>;
PreviewAuthoringProvider::handle(&mut self, request: &AuthoringRequest)
    -> AuthoringEvent;
PreviewAuthoringProvider::snapshot(&self) -> &AuthoringProjection;

preview_authoring_projection() -> AuthoringProjection;
preview_project_authoring_to_ui(
    current: &wonderland_contracts::UiProjection,
    authoring: &AuthoringProjection,
) -> Result<wonderland_contracts::UiProjection, AuthoringError>;
```

`AuthoringState` derives `Clone` and `Debug`. Every field is private. Consumers read through these getters and change state through `dispatch` and `receive`:

| Getter | Return type | Meaning |
| --- | --- | --- |
| `projection()` | `&AuthoringProjection` | Current committed data |
| `selected_profile()` | `Option<&CharacterId>` | Acting profile context |
| `selected_home()` | `Option<&CharacterId>` | Explicit room owner context |
| `selected_instance()` | `Option<&OwnedInstanceId>` | Selected placed object |
| `draft()` | `Option<&AuthoringDraft>` | Current uncommitted editor |
| `pending()` | `Option<&AuthoringRequest>` | At most one in-flight mutation |
| `last_error()` | `Option<&AuthoringError>` | Latest dispatch or matching rejection feedback |
| `last_commit()` | `Option<&AuthoringCommit>` | Most recent accepted receipt metadata |
| `draft_validity()` | `Result<(), AuthoringError>` | Advisory validity for the current editor's primary action |

Invalid initial data remains readable with an `InvalidProjection` error and disabled dispatch. Validate stored/external data before exposing it in a view. A valid newer replacement can recover a reducer. `PreviewAuthoringProvider::new` instead refuses invalid data immediately.

Reducer operation IDs are `OperationId` values such as `authoring-1`. The counter uses checked arithmetic and survives explicit projection replacement. IDs are scoped to one reducer/provider session; cloned reducers must not be used as concurrent independent actors against the same provider. Replacing a browser session must dispose of that session's callbacks and provider together.

## Snapshot types

All contract types below implement `Clone`, `Debug`, `PartialEq`, `Eq`, and Serde serialization/deserialization. Small enums and geometry values additionally implement `Copy`. `LookId`, `CatalogId`, and `OwnedInstanceId` are distinct public tuple newtypes around `String`, with `From<&str>`, `From<String>`, `AsRef<str>`, `Display`, ordering, and hashing. Existing `CharacterId`, `Character`, `Availability`, and `OperationId` are reused unchanged.

| Type | Public fields |
| --- | --- |
| `AuthoringProjection` | `version: u32`, `revision: u64`, `profiles: Vec<AuthoringProfile>`, `catalog: Vec<CatalogItem>` |
| `AuthoringProfile` | `character: Character`, `identity: VisualIdentity`, `look_id: LookId`, `home: Home` |
| `Home` | `owner_id: CharacterId`, `permissions: HomePermissions`, `instances: Vec<OwnedInstance>` |
| `HomePermissions` | `purchase: Availability`, `arrange: Availability` |
| `OwnedInstance` | `id: OwnedInstanceId`, `catalog_id: CatalogId`, `placement: Option<GridPose>` |
| `CatalogItem` | `id: CatalogId`, `name: String`, `category: CatalogCategory`, `price: i64`, `footprint: Footprint`, `availability: Availability` |
| `GridCell` | `x: i16`, `y: i16` |
| `GridPose` | `cell: GridCell`, `direction: Direction` |
| `Footprint` | `width: u8`, `depth: u8` |

One Home belongs to each profile, and `home.owner_id` must equal `character.id`. Instances live in that owner's Home, with globally unique instance IDs across the snapshot. `placement: None` means stored inventory. Catalog and instance IDs are never interchangeable. `purchase` permits the combined buy-and-place operation; `arrange` permits Move, Store, and Place again. Actor ownership is always checked in addition to the capability's `Availability` value.

The snapshot's utility methods are:

```rust
AuthoringProjection::validate(&self) -> Result<(), AuthoringError>;
AuthoringProjection::profile(&self, id: &CharacterId) -> Option<&AuthoringProfile>;
AuthoringProjection::home(&self, owner_id: &CharacterId) -> Option<&Home>;
AuthoringProjection::catalog_item(&self, id: &CatalogId) -> Option<&CatalogItem>;
Home::instance(&self, id: &OwnedInstanceId) -> Option<&OwnedInstance>;
CatalogItem::can_rotate(&self) -> bool;
```

### Visual identities and looks

`VisualIdentity` has exactly `Maya`, `Jules`, `Nico`, `Amara`, and `Leo`. `LookStyle` has exactly `Everyday`, `Smart`, and `Active`. Each exports `ALL`, `as_str(self) -> &'static str`, and `label(self) -> &'static str`. `VisualIdentity::look_id(self, style: LookStyle) -> LookId` creates one of the 15 valid IDs. `LookId::style_for(&self, identity: VisualIdentity) -> Option<LookStyle>` rejects another identity's look and invented suffixes.

The IDs are `maya-everyday`, `maya-smart`, `maya-active`, and the corresponding three suffixes for `jules`, `nico`, `amara`, and `leo`. They refer to appearance presets, including when a created profile has a new `CharacterId` such as `preview-profile-2`. Multiple profiles can share an appearance preset; they retain separate names, budgets, and Homes. Changing creation identity retains the currently chosen style using the new identity's valid look ID. Outfit editing cannot alter identity or name.

`normalize_profile_name(value: &str) -> Result<String, AuthoringError>` trims Unicode whitespace, accepts 1–32 Unicode scalar values and at most 128 UTF-8 bytes after trimming, and rejects control characters anywhere in the input. It does not normalize Unicode or truncate input. Persisted names and submitted request names must already be trimmed. The draft retains raw input so ordinary editing and IME composition are not rewritten by the reducer.

### Catalog and geometry

`CatalogCategory` has `Living`, `Lighting`, `Decor`, and `Storage`, plus `ALL` and `label(self) -> &'static str`.

| Catalog ID | Name | Category | Price | Width × depth | Meaningful rotation |
| --- | --- | --- | ---: | --- | --- |
| `armchair` | Harbor armchair | Living | 180 | 1 × 1 | Yes |
| `coffee-table` | Oak coffee table | Living | 120 | 2 × 1 | Yes |
| `floor-lamp` | Linen floor lamp | Lighting | 90 | 1 × 1 | No |
| `fern` | Potted fern | Decor | 45 | 1 × 1 | No |
| `bookcase` | Oak bookcase | Storage | 260 | 2 × 1 | Yes |
| `woven-rug` | Woven rug | Decor | 160 | 2 × 2 | No |

The validator permits subsets, including an empty catalog, for fixture scenarios. Each present row must retain its exact ID, name, category, price, and footprint; availability may change. Owned instances must still reference present catalog rows. `CatalogItem::can_rotate()` enables only the three directional objects above.

`Direction` has `North`, `East`, `South`, and `West`. `Direction::clockwise(self) -> Direction` advances one quarter turn. `Footprint::rotated(self, direction: Direction) -> Footprint` exchanges width and depth for East/West. `Footprint::cells(self, pose: GridPose) -> Result<Vec<GridCell>, AuthoringError>` returns bounded occupied cells in row order. It rejects zero dimensions, negative/outside cells, rotated overflow, and the reserved entrance `(0, 0)`. A `2 × 1` item becomes `1 × 2` after a quarter turn. The provider checks overlap against its own snapshot and ignores only the moving instance's old footprint.

These cells are a presentation aid for the shared camera transform. A live adapter must map them to canonical lot coordinates; this module has no projection/picking or scene rendering code.

### Exported bounds

| Constant | Type | Value |
| --- | --- | ---: |
| `AUTHORING_VERSION` | `u32` | `1` |
| `MAX_PROFILES` | `usize` | `8` |
| `MAX_OWNED_INSTANCES` | `usize` | `64` per Home, stored and placed together |
| `PREVIEW_STARTING_BUDGET` | `i64` | `1_250` |
| `MAX_AUTHORING_JSON_BYTES` | `usize` | `262_144` |
| `ROOM_WIDTH` | `i16` | `8` |
| `ROOM_DEPTH` | `i16` | `6` |
| `MAX_PREVIEW_OPERATIONS` (client module) | `usize` | `256` per provider session |

Snapshot validation requires version 1 and a positive revision. It reuses the original `Character` validation for IDs, availability, and all eight need values, then enforces the stricter authoring name/look/home rules. Preview money is bounded to `0..=1_250`. IDs are 1–64 ASCII bytes containing letters, digits, `-`, `_`, `.`, or `:`. Unavailable reasons are nonblank, contain no controls, and occupy at most 256 UTF-8 bytes. Profile IDs, catalog IDs, and global owned-instance IDs must be unique in their respective domains. Duplicate/unknown instances, missing catalog references, overlap, entrance occupancy, and outside placements invalidate the whole snapshot.

`is_valid_authoring_id(value: &str) -> bool` is the contracts-owned shared rule for authoring snapshot instance IDs, request IDs, and receipt outcome IDs. It applies the exact ID format above; passing it does not establish ownership or authority. The reducer/provider still perform their typed identity, reference, operation, and permission checks.

## Editors and intents

| Draft type | Public fields or variants |
| --- | --- |
| `CharacterDraft` | `name: String`, `identity: VisualIdentity`, `look_id: LookId` |
| `OutfitDraft` | `character_id: CharacterId`, `look_id: LookId` |
| `PlacementDraft` | `actor_id: CharacterId`, `home_owner_id: CharacterId`, `source: PlacementSource`, `pose: GridPose` |
| `PlacementSource` | `Catalog(CatalogId)`, `Move(OwnedInstanceId)`, `Inventory(OwnedInstanceId)` |
| `AuthoringDraft` | `Creation(CharacterDraft)`, `Outfit(OutfitDraft)`, `Placement(PlacementDraft)` |

`AuthoringIntent` contains every variant below. Local editing intents return an empty request vector. A valid submission returns one request. No draft is persisted by this crate.

| Intent | Payload | Behavior |
| --- | --- | --- |
| `SelectProfile` | `CharacterId` | Select an existing actor context; clear room, object, and draft context |
| `OpenCreate` | None | Open a blank-name Maya/Everyday draft; refuse when eight profiles exist |
| `UpdateName` | `String` | Retain raw name input in creation only |
| `SelectIdentity` | `VisualIdentity` | Change creation identity, retaining a valid style |
| `SelectLook` | `LookId` | Change creation/outfit appearance after checking that identity's look |
| `SubmitCreate` | None | Normalize and validate creation; emit `CreateProfile` |
| `OpenOutfit` | None | Open the selected available profile's committed look |
| `SaveOutfit` | None | Emit `SetOutfit` for that profile |
| `OpenHome` | `CharacterId` | Open an explicit owner room with the existing selected actor; clear object/draft |
| `SelectCatalog` | `CatalogId` | Start a purchase candidate at `(1, 1)`, North |
| `SelectOwned` | `OwnedInstanceId` | Select a placed object in the current Home; clear any draft |
| `BeginMove` | None | Stage the selected instance at its committed pose |
| `BeginPlace` | `OwnedInstanceId` | Stage a stored instance at `(1, 1)`, North |
| `SetCell` | `GridCell` | Set even an invalid candidate cell, so the UI can show its reason |
| `MoveCandidate` | `{ dx: i16, dy: i16 }` | Checked integer movement for keyboard/camera controls |
| `RotateCandidate` | None | Rotate the directional candidate; symmetric items expose an unavailable reason |
| `ConfirmPlacement` | None | Validate and emit Buy, Move, or Place again for the draft source |
| `StoreSelected` | None | Emit Store for the selected placed object |
| `Cancel` | None | Discard the draft; committed appearance/placement remains intact |
| `Close` | None | Discard the draft and clear Home/object selection; retain profile selection |

Selection of an item or room does not grant permissions. A poor, full, or denied room can still show a candidate and `draft_validity()` explains why it cannot commit. Another profile's stored instance cannot be found through the current Home. Submissions repeat the provider-facing checks before being emitted.

At most one authoring mutation is pending. While pending, submission intents emit no extra request, and every other intent returns `Busy`. The browser should disable navigation and Close/Cancel for that short interval, rather than imply cancellation of an already submitted mutation. Before submission, Cancel is immediate.

## Requests, outcomes, and events

`AuthoringRequest` has `operation_id: OperationId`, `base_revision: u64`, and `kind: AuthoringRequestKind`.

| `AuthoringRequestKind` variant | Fields |
| --- | --- |
| `CreateProfile` | `name: String`, `identity: VisualIdentity`, `look_id: LookId` |
| `SetOutfit` | `actor_id: CharacterId`, `look_id: LookId` |
| `BuyAndPlace` | `actor_id: CharacterId`, `home_owner_id: CharacterId`, `catalog_id: CatalogId`, `pose: GridPose` |
| `MoveInstance` | `actor_id: CharacterId`, `home_owner_id: CharacterId`, `instance_id: OwnedInstanceId`, `pose: GridPose` |
| `StoreInstance` | `actor_id: CharacterId`, `home_owner_id: CharacterId`, `instance_id: OwnedInstanceId` |
| `PlaceOwned` | `actor_id: CharacterId`, `home_owner_id: CharacterId`, `instance_id: OwnedInstanceId`, `pose: GridPose` |

| `AuthoringOutcome` variant | Fields |
| --- | --- |
| `ProfileCreated` | `character_id: CharacterId` |
| `OutfitSaved` | `character_id: CharacterId` |
| `Purchased` | `owner_id: CharacterId`, `instance_id: OwnedInstanceId` |
| `Moved` | `owner_id: CharacterId`, `instance_id: OwnedInstanceId` |
| `Stored` | `owner_id: CharacterId`, `instance_id: OwnedInstanceId` |
| `Placed` | `owner_id: CharacterId`, `instance_id: OwnedInstanceId` |

| `AuthoringEvent` variant | Fields |
| --- | --- |
| `Committed` | `operation_id: OperationId`, `base_revision: u64`, `projection: AuthoringProjection`, `outcome: AuthoringOutcome` |
| `Rejected` | `operation_id: OperationId`, `base_revision: u64`, `error: AuthoringError` |
| `ProjectionReplaced` | `projection: AuthoringProjection` |

`AuthoringCommit` has `operation_id: OperationId`, `base_revision: u64`, `revision: u64`, and `outcome: AuthoringOutcome`. It is receipt metadata exposed by `last_commit()`; it is not persisted and is not an alternate submission path.

### Atomic receipt rules

A committed reply must match the one pending operation and its base revision, and that base must still be the reducer's current revision. Its snapshot must be strictly newer, fully valid, and equal to the exact requested change with the typed outcome's identity. Validation happens before any state mutation. A valid snapshot with an unrelated money, profile, catalog, or placement change is rejected as `InvalidOutcome`; simply supplying a plausible new ID is insufficient.

Creation adds exactly one fresh profile using the chosen identity/look, normalized name, 1,250 preview budget, fixture needs, and an empty owned Home. The returned ID becomes selected. Outfit changes only the target's look. Buy adds one fresh instance and subtracts its exact catalog price from its owner. Move changes only that instance's pose. Store sets that instance's placement to `None` without refund; Place again sets its placement without charging. Successful commits close the draft. Purchase/Move/Place select the returned placed instance; Store clears the placed selection.

Unknown operations, duplicate replies, wrong bases, and nonnewer receipt snapshots are no-ops, even while another editor/request is open. Invalid matching snapshots or outcomes return an error and preserve the prior projection, draft, selection, and pending operation. A matching rejection clears pending work, retains the draft for retry, and exposes its error. Free-form rejection text is stripped of controls and bounded to 256 UTF-8 bytes. Retrying creates a fresh operation ID.

A valid strictly newer `ProjectionReplaced` invalidates pending work, all drafts, selected object, and last-commit metadata. Surviving profile/Home context may remain selected. The operation counter is never reset. Invalid replacements fail atomically; valid equal/older replacements are no-ops.

### Provider behavior and bounds

The provider validates against its own current snapshot, constructs a candidate copy, validates that complete copy, and then commits. It increments the revision by exactly one with checked arithmetic. The reducer accepts any strictly newer revision carrying the exact permitted delta.

The demo generates opaque `preview-profile-{number}` and `preview-instance-{number}` identities, skipping existing IDs. The UI must use the returned typed outcome, not predict these values. Exact bounded requests replay their original complete receipt, even after subsequent changes. Reusing a cached operation ID with a different payload or base rejects as `InvalidOperation`. An uncached request with an old base rejects as `StaleRevision`.

At most 256 bounded request receipts, including ordinary rejections, are cached per provider lifetime. New operations after that bound reject as `OperationLimit`; existing cached receipts remain replayable. Malformed/oversized request fields are rejected without retaining their payloads. This bound belongs to the disposable demo session and is not a production idempotency policy. There is no hidden mutation, timer, storage access, or automatic UI delivery in `handle`.

## Browser adapter ordering and persistence

At initial bootstrap, validate the fixture or accepted stored snapshot and derive a compatible initial shell projection. For each new authoring mutation, the required order is:

1. Dispatch in the current `AuthoringState`, obtaining a single request.
2. Schedule the preview provider's reply after the browser's 850 ms delay, tied to the originating current session. A one-shot rejection fixture should construct a matching `Rejected` event without first calling `handle`, so no hidden provider mutation occurs.
3. Save the current authoring revision, deliver the event through `AuthoringState::receive`, and check that the revision advanced and `last_commit()` matches the expected operation/base. `Ok(())` alone is insufficient because ignored events also return it.
4. Only after that verified commit, pass the accepted snapshot to `preview_project_authoring_to_ui(&shell.projection, state.projection())`. It validates both inputs, replaces shell character data, enables the existing `home` destination, preserves other places/objects, and returns a validated shell revision exactly one higher than the current shell revision.
5. Deliver that result through `ShellState::receive(UiEvent::ProjectionUpdated { projection })`. Use ordinary shell intents to select a newly created character. Never mutate `ShellState` fields directly or send an unrelated shell update before the matching authoring receipt.
6. Persist only the accepted authoring snapshot using the versioned local preview envelope/key. Dispose of owned timers and callbacks when the browser session ends.

The projector refuses an invalid shell/authoring snapshot, a shell without the existing `home` destination, or shell revision overflow. It does not execute simulation requests or mutate either input.

The browser owns storage under `wonderland.authoring.v1`. Enforce `MAX_AUTHORING_JSON_BYTES` before decoding and after encoding, validate version/data before exposure, and persist no drafts, pending operations, receipt cache, secrets, grants, or assets. Invalid/unsupported/oversized stored data must be preserved rather than overwritten; use a temporary preview and visible notice. Storage failures must leave the tab usable. Complete fixture scenarios bypass persisted data. Construct `read-only-home` by making purchase and arrange unavailable, `poor-home` by reducing the selected profile's money, and `empty-catalog` from a fresh empty-home fixture with an empty catalog. Detailed browser storage and timer behavior belongs to the browser adapter.

## Errors and serialization

`AuthoringError` implements Serde, equality, `Display`, and `std::error::Error`. Its complete variants are:

```rust
InvalidProjection(String), InvalidName(String), InvalidLook,
UnknownProfile, UnknownCatalogItem, UnknownInstance,
NoSelection, WrongEditor, Busy, Unavailable(String), PermissionDenied,
InsufficientFunds, ProfileLimit, InventoryLimit,
OutOfBounds, EntranceReserved, Occupied, WrongPlacementState,
StaleRevision, InvalidOperation, InvalidOutcome, OperationLimit,
Rejected(String)
```

Simple enums serialize as snake_case strings. `AuthoringIntent` uses `intent` plus `data`; `PlacementSource` uses `source` plus `id`; `AuthoringDraft` uses `editor` plus `draft`. Request kinds use an internal `kind` tag, outcomes use `outcome`, events use `event`, and errors use `error` plus `detail`, all snake_case. Existing `Availability` retains its `status` representation and existing need keys keep their names. Unknown fields are accepted by Serde for forward extension; unknown variants are rejected. Validation is a separate required boundary after decoding.

## Native coverage and source layout

`crates/contracts/tests/authoring_fixture.rs` covers the serialized fixture, all 15 identity/look combinations and cross-identity rejection, Unicode names, malformed snapshot references/bounds, exact catalog data, and integer rotated footprints. `crates/client-app/tests/authoring_flow.rs` covers draft/commit ordering, typed identity selection, cancellation, pending/rejection/retry behavior, stale/invalid receipt isolation, replacements without operation reuse, and shell mirroring. `crates/client-app/tests/authoring_provider.rs` covers duplicate requests, exact spending, ownership and capability checks, capacity including stored items, overlap/entrance/bounds, own-footprint movement, and Store/Place again without a second charge.

Implementation is split into focused `authoring/` modules in the two crates. The original shell reducer, API types, fixture, tests, dependency versions, and manifests remain unchanged apart from exporting the new modules.
