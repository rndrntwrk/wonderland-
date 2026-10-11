# Authoring contract v2

This documents the implemented `AUTHORING_VERSION = 2` contract, superseding [v1](authoring-v1.md). The [contracts](../../crates/contracts/src/authoring/mod.rs) describe acknowledged account, content, wardrobe and lot data. The [client provider](../../crates/client-app/src/authoring/provider.rs) is an explicit local simulation: it does not authenticate accounts, spend live money, execute construction or render a world.

## Source identities and independent appearance

`AuthoringProjection` carries a positive global revision, account and appearance capabilities, catalog source/revision, catalog categories, profiles, catalog records and optional `shared_homes`. Profiles retain name, description, shard, appearance, optional historical portrait, wardrobe and Home.

`ContentKey`, `ContentSourceId`, `AccountId`, `ShardId`, `CatalogId`, `CatalogCategoryId`, `OwnedInstanceId`, `WardrobeCategoryId`, `OwnedOutfitId` and `BuildToolId` are distinct opaque string types. Source content keys, owned identities and operation IDs are not interchangeable. Valid identity syntax is 1–64 ASCII letters/digits or `-_.:`; syntax alone grants no authority.

[`AppearanceSelection`](../../crates/contracts/src/authoring/appearance.rs) stores independent optional head, body, skin-tone and gender keys, plus decorations keyed by wardrobe category. `AppearanceContent` supplies option lists, required fields, source/revision and separate rendering availability. Head/body options carry gender and skin membership; empty compatibility lists mean unrestricted. Creation validates supplied membership, availability and compatibility without rewriting the other selected parts.

Saved appearance references receive structural validation even when their content is unavailable. Creation decorations are not accepted through unowned content selection. `PortraitReference` preserves historical artwork and its original identity/look strings; it is not a game outfit or proof of the current rendered appearance. Decoded meshes and texture pixels are outside the saved projection.

## Account and request policy

[`AccountCapabilities`](../../crates/contracts/src/authoring/capabilities.rs) supplies creation availability, field rules, shard options/default and `profile_capacity`:

| Capacity policy | Addition behavior |
| --- | --- |
| `Unknown` | Creation is unavailable because the service has not supplied capacity. |
| `Limited { maximum }` | Allow an addition only while the existing count is below the supplied maximum. |
| `Unlimited` | No policy count ceiling; client resource-safety bounds still apply. |

[`legacy_account_capabilities()`](../../crates/client-app/src/authoring/mod.rs) describes the original three-character policy, names of 3–24 ASCII letters/spaces, and descriptions of at most 499 characters. It does not authenticate an account. The [preview fixture](../../fixtures/ui/authoring-v2.json) explicitly supplies `Unlimited` simulation policy. The provider's 1,250 starting balance is a new-preview-profile default, not a balance ceiling.

Creation field rules apply to new requests. Saved profiles above a supplied capacity, saved Unicode names and existing nonnegative balances remain readable within structural safety bounds. Creation retains the selected supplied shard ID; unavailable or missing required shard selections cannot submit.

## Catalog, lots and scoped permissions

Catalog IDs, category IDs, source keys, names, nonnegative prices, footprints, thumbnails and availability come from supplied records. Rotation follows each item's supplied orientation list. The current `Direction` representation is North/East/South/West; rectangular occupancy is not a complete source-world geometry or direction model.

[`LotGeometry`](../../crates/contracts/src/authoring/geometry.rs) supplies source/revision, an origin and width/depth, allowed levels and reserved cells. `GridPose` includes cell, level and direction. Bounds, reserved cells and overlap are checked on the applicable level. An indexed reserved-cell set is reused across footprint queries.

Each profile's embedded Home owner matches that profile. `shared_homes` adds target lots whose owners need not be in the playable roster; owner IDs are unique across all Homes. `home()` and `homes()` search both collections. `owned_instance()` finds an instance and its containing Home; `inventory_for(owner)` yields stored instances with that explicit owner.

`Home.permissions` is lot-wide availability, not actor authorization. A matching `LotGrant.actor_id` must supply the relevant purchase, arrangement or construction permission. No grant means denial, including for the lot owner. `inventory_owners` explicitly scopes which owners' stored objects the actor may place. These grants belong to the supplied lot snapshot/revision; actor IDs in requests remain UI context.

`PurchaseGrant` specifies availability, `payer_id`, `object_owner_id` and category scope. `categories: None` permits every supplied category; `Some([])` permits none. The preview requires the payer's profile/balance, charges that payer and records the supplied object owner. `Home.instance_capacity` governs additions to its collection; unknown capacity does not imply unlimited space, and existing overflow does not invalidate saved data.

`OwnedInstance.owner_id` is separate from its containing Home; `None` is unknown ownership and grants no inventory access. Move/Store retain instance ID and owner. Store clears placement without refund. PlaceOwned may transfer a permitted stored instance between Home collections without charging again. In existing lot outcome fields, `owner_id` identifies the target Home; the instance's own `owner_id` identifies the possession's owner.

[`BuildCapabilities` and `ArchitectureRequest`](../../crates/contracts/src/authoring/capabilities.rs) describe tools, view modes and preview/commit requests. Preflight validates revisions, actor grant, availability and endpoints. `PreviewAuthoringProvider` has no architecture execution or successful construction receipt path.

## Owned wardrobe actions

Wardrobe categories supply labels and Head/Body/Decoration slots. Each owned outfit has a stable owned ID, content key, category, default flag and available action offers. There is no fixed three-look or five-identity membership.

`Change` updates the category's appearance slot; `SetDefault` changes default flags in that category. `Delete` removes the owned record. Deleting a default requires an `OutfitSaved.replacement_default` that names a surviving outfit in the same category; no survivor means the request is unavailable. Other outcomes must not supply a replacement. The preview chooses the first survivor; the reducer accepts another valid source-selected survivor with the exact matching snapshot. See [transition rules](../../crates/client-app/src/authoring/transition.rs).

## Requests, accepted receipts and provider refresh

[`AuthoringRequest`](../../crates/contracts/src/authoring/messages.rs) carries `operation_id`, `base_revision`, `expected_sources` and a typed kind: CreateProfile, SetOutfit, BuyAndPlace, MoveInstance, StoreInstance or PlaceOwned. Expected source revisions include account, appearance and catalog, plus the relevant wardrobe or lot revision. The provider repeats validation against its current snapshot.

[`AuthoringState`](../../crates/client-app/src/authoring/state.rs) keeps one pending mutation. A committed reply must match its operation and current base, contain a strictly newer valid projection, and equal the exact permitted delta. Validation precedes the state swap. For creation, the requested name, description, shard and appearance must match; the accepting service supplies the new profile's initial balance, needs and lot data. Unrelated record changes are rejected.

Acceptance clears the draft and pending request. A matching rejection clears pending work but preserves draft intent for a fresh-operation retry. Unknown operations, wrong bases, duplicate replies and nonnewer receipts are no-ops. Invalid matching receipts leave committed data, draft and pending request intact.

`PreviewAuthoringProvider::handle()` works on a candidate copy and commits only after validation and replay-budget checks. Exact repeated requests replay their original event; reusing an operation ID with different request data rejects as `InvalidOperation`. Uncached stale global/source revisions reject as `StaleRevision`.

`replace_projection(projection) -> Result<(), AuthoringError>` accepts only valid, strictly newer snapshots and retains cached replies, operation identities and budgets. Reducer `ProjectionReplaced` separately clears drafts/pending work while retaining its operation counter; equal/older valid replacements are ignored. The [browser bridge](../../apps/web-shell/src/authoring_bridge.rs) guards content installation while requests/saves are busy or the session is disposed, and explicitly restores retained editor choices after replacement.

## Persistence, migration and conflict protection

[`persistence.rs`](../../apps/web-shell/src/persistence.rs) writes a version-2 envelope under the unchanged localStorage key **`wonderland.authoring.v1`**. It accepts version-1 envelopes through explicit migration and version-2 envelopes through normal validation. Unsupported, corrupt or oversized data is not silently replaced.

[`migrate_v1_projection()`](../../crates/contracts/src/authoring/legacy.rs) retains structurally valid saved profiles, balances, owned IDs/catalog references, stored/placed state and portrait references, including more than three or eight profiles and more than 64 possessions. Legacy poses acquire level 0 and the known preview geometry becomes supplied metadata. Historical identity/look strings become portrait references; no real head/body outfit IDs are fabricated. Generic migration supplies unknown account capacity and unavailable creation.

`resume_preview_projection()` explicitly opts recognized preview saves into fixture simulation policy. For earlier v2 fixture saves lacking grants/ownership fields, only this recognized preview adapter restores the historical owner-only facts. Generic v2 decoding leaves missing grants empty and missing ownership unknown; unrelated live/source projections are unchanged.

Only validated acknowledged snapshots cross the save boundary; drafts, pending operations and decoded rendering resources are excluded. The browser applies the accepted reducer receipt before mirroring the shell and attempting persistence. A failed write does not undo an accepted session change or falsely report it as saved.

`SaveSession` retains the exact raw envelope observed at load. Cooperating tabs serialize synchronous localStorage compare-and-set through one IndexedDB readwrite transaction. A byte mismatch disables writes for that session and preserves the other tab's save; a transient read/write failure remains retryable. The stable key and exact-byte comparison also detect intervening saves across v1/v2 tabs. Disposed sessions and unavailable coordination cannot bypass the guard.

## Client resource-safety bounds

These are allocation/input/replay limits, separate from account or game policy. Additional small field/list checks are defined in [validation](../../crates/contracts/src/authoring/validation.rs).

| Resource | Current bound |
| --- | --- |
| Profile records / total embedded and shared Homes | 1,024 profiles; 1,024 total Homes |
| Catalog records / categories / combined appearance options / total lot grants | 65,536 per listed collection |
| Owned instance IDs / owned outfit IDs | 65,536 each, across the projection |
| Name / description safety | 128 UTF-8 bytes / 16 KiB; request policy may be narrower |
| Lot levels / reserved cells / one footprint | 256 levels / 65,536 cells / 65,536 cells |
| Aggregate placed-footprint validation work | 1,048,576 cells, checked before occupancy allocation |
| Encoded or decoded saved JSON | 16 MiB |
| Preview replay retention | 256 cached operations and 32 MiB of serialized request/event bytes |

## Regression evidence

- [Capability preservation](../../crates/contracts/tests/capability_preservation.rs) and [provider capabilities](../../crates/client-app/tests/capability_provider.rs): custom catalog records, large balances, saved overflow, independent fields and supplied geometry/policy.
- [Authoring flow](../../crates/client-app/tests/authoring_flow.rs) and [provider behavior](../../crates/client-app/tests/authoring_provider.rs): exact receipts, failed drafts, identity preservation and repeated-operation safety.
- [Review regressions](../../crates/client-app/tests/review_regressions.rs): default replacement, scoped roommate rights, payer/object ownership, shared lots and old v2 preview resumption.
- [Provider refresh](../../crates/client-app/tests/provider_refresh.rs), [geometry safety](../../crates/contracts/tests/geometry_safety.rs) and [persistence](../../apps/web-shell/tests/authoring_persistence.rs): retained replay budgets, bounded occupancy, lossless v1 migration and exact-byte save conflicts.
