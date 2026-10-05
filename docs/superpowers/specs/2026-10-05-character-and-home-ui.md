# Wonderland: character and home authoring UI

## Outcome and authority

Continue the user's approved action-focused redesign and their instruction to proceed with implementation and PRs. This is the next W11 increment after PR #5: a character-stage creator, visual wardrobe, and a distinct Home scene with catalog selection, placement previews, room arrangement, and owned inventory. The game world and characters remain the main controls.

Use the existing isolated Wonderland worktree on `feat/character-and-home-ui`, starting at local `bd08bd1652bec851e42a9ad8348c4c3902a88745`. Its tree equals published PR #5 commit `b921c76fb2bea478a0685c9857e00e0fc309c097`. Publish a stacked PR against `feat/action-first-browser-ui`; do not merge another team's PR or change `master`.

The selected character/map/café references and existing app are the visual source. Extend their rounded blue chrome, green primary actions, warm scenic art, large character stage, and direct world actions. The user has already approved this direction and continuing work; normal implementation choices and asset production proceed without another permission round.

## Integration findings

Read-only audits inspected Swarm A PR #6 at `8a0e251d19e222a0a6833d7408ca629f674e1729` and Swarm B PRs #1–4, ending at `feaa91549acae3da2931f0b95ba768348f7d56f8`.

- Swarm A supplies deterministic simulation/build foundations and a native/WASM replay harness. It does not supply a production renderer, public render/pick interface, browser game ABI, or an integrated purchase/placement command.
- Swarm B supplies content readers, manifests/packs, interaction modules, and an offline IFF/BHAV editor. Its creator tool is not player character creation. Its object census is qualification metadata, not a catalog or owned inventory.
- Asset resource keys, simulation entity IDs and positions, interaction IDs, and this UI's presentation IDs are distinct. Do not cast or equate them. UI grid cells are preview coordinates; live adapters must map to canonical lot positions and authoritative rights/outcomes.
- The independent root workspaces and dependency pins still need integration. Do not import a test harness or filesystem creator CLI into the browser, or silently merge/repin another team's branch.

This increment therefore uses an explicit, replaceable authoring preview provider. It implements UI drafts, request/outcome handling, and a persisted local preview. It does not claim live account creation, authoritative spending, simulation integration, wall/roof construction, or production 3D. The provider is test/demo infrastructure, not a second production economy or placement engine.

## Character creation and wardrobe

- Keep a large full-body stage in view. Creation starts from the existing Create a Sim card. Select among the five existing visual identities: Maya, Jules, Nico, Amara, and Leo. These are appearance presets, not new accounts.
- Each identity has three coherent looks: Everyday (existing artwork), Smart, and Active. New looks use separately generated transparent full-body artwork that preserves identity and style. The UI selects whole valid looks; it does not imply arbitrary rig, hair, or body editing.
- Creation exposes one short name field as a nameplate next to the stage. Accept trimmed names of 1–32 Unicode scalar values, at most 128 UTF-8 bytes, without control characters. Preserve IME composition and ordinary Unicode input. Empty or invalid names leave Create disabled with a concise reason.
- Changing the identity or look updates the stage immediately as a draft. Create sends a typed request; the new profile appears only after its matching committed result. Show pending/rejection/retry states. A duplicate request/reply cannot create a second profile.
- The preview supports at most eight profiles. Keep the character grid at five profile cards plus the creation slot per page, adding simple page controls as needed. A created profile becomes selected and is immediately usable in the original Play → map → café/Home flow.
- Change outfit opens the same stage with the selected profile's three looks. Preserve identity and name. Save outfit submits a request; Cancel leaves the committed appearance unchanged. An accepted look is reflected in the character card, stage, café/Home sprite, and HUD portrait.
- Drafts are not persisted. Only validated, acknowledged profile/home snapshots are persisted. There is no profile deletion or account authentication in this increment.

## Home and room editing

- Enable the existing Home destination using a new, distinct, unfurnished coastal-room illustration. Café remains its existing scene; Park and Arcade retain their existing unavailable reasons. The Home title identifies the selected profile, and each preview profile has its own room and budget.
- Preserve Live / Build / Buy navigation. Live displays the room. Buy opens a compact visual catalog over the lower edge. Build selects existing furniture and exposes Move, Rotate where meaningful, and Store near the object. It is room arrangement; full architectural construction remains separate.
- Catalog cards contain an actual object thumbnail, short name, and price. Use category chips and optional compact search; avoid a permanent inspector or form. The authored starter catalog is six records, not a claim of full source-content coverage:

| ID | Name | Category | Price | Footprint |
| --- | --- | --- | ---: | --- |
| armchair | Harbor armchair | Living | 180 | 1 × 1 |
| coffee-table | Oak coffee table | Living | 120 | 2 × 1 |
| floor-lamp | Linen floor lamp | Lighting | 90 | 1 × 1 |
| fern | Potted fern | Decor | 45 | 1 × 1 |
| bookcase | Oak bookcase | Storage | 260 | 2 × 1 |
| woven-rug | Woven rug | Decor | 160 | 2 × 2 |

- Each profile starts with a preview budget of 1,250. The room uses an 8 × 6 integer-cell presentation grid and no more than 64 owned instances, including stored objects. The far corner cell (0,0) is reserved and visibly explained as the entrance. Preview footprint occupancy is a UI aid; the provider rechecks every request against its own snapshot.
- Select a catalog item, then tap a floor cell to move its translucent placement preview. Drag/pan and zoom continue to use the shared camera. The preview's image and footprint share the same coordinate transform. A visible reason accompanies invalid cells, insufficient budget, permission denial, or a full inventory; color is supplemental.
- Rotate updates the footprint and uses directional object artwork. Never rotate a flat room image as a 3D camera. Directional sprites can use front/back art with horizontal mirroring for fixed-camera quarter turns; symmetric lamp/plant/rug assets need no false visual rotation control.
- Buy and place submits one request with actor, catalog identity, snapshot revision, cell, and direction. Money and committed objects change only on the matching accepted snapshot/result. Keep the placement preview on rejection so the user can adjust or retry. Repeated input must not duplicate an object or cost.
- Move stages an owned object at another cell without changing committed placement until accepted. The candidate ignores its own previous footprint when testing overlap. Cancel restores the committed position. Store moves the instance into inventory without refunding money. Place again reuses that owned instance without charging a second purchase.
- Inventory uses the same visual drawer and shows only the selected profile's stored instances. Empty inventory has a clear return to the catalog. Ownership/revision, entry/out-of-bounds, rotated footprint, overlap, budget, capacity, and permissions are checked at the preview provider boundary.

## State and provider boundary

- Keep the original `UiProjection`, `UiIntent`, `UiRequest`, `UiEvent`, and `ShellState` behavior compatible. Add a distinct versioned authoring projection/intents/requests/events surface in `wonderland-contracts::authoring` and pure state in `wonderland-client-app::authoring`.
- Export `AuthoringState::new(projection: AuthoringProjection) -> Self`, `dispatch(intent: AuthoringIntent) -> Result<Vec<AuthoringRequest>, AuthoringError>`, and `receive(event: AuthoringEvent) -> Result<(), AuthoringError>`. Public presentation fields are read-only to consumers; mutation goes through these methods. The implementer documents every exported variant/field for the UI task.
- The authoring snapshot owns profile character data, selected valid look IDs, per-profile Home instances/permissions, and the authored catalog. Reuse the existing typed `CharacterId`, `Character`, `Availability`, and operation identity conventions. Asset keys remain separate from gameplay entity IDs.
- Requests preserve unique operation identity for the state lifetime and their base revision. At most one authoring mutation is in flight. A committed reply includes the matching operation/base revision, a newer complete validated authoring snapshot, and a typed outcome identifying the created/updated profile or instance. Bare success cannot invent a new identity.
- Validate the entire reply before changing state. Unknown, duplicate, stale, wrong-operation, or wrong-base replies cannot close another editor, charge another profile, or replace a newer snapshot. Invalid snapshots leave the prior valid state intact. Explicit snapshot replacement invalidates in-flight drafts/requests safely and never reuses operation IDs.
- The UI's separate original shell receives a newer `ProjectionUpdated` generated by an explicitly named preview projector only after a validated authoring commit. This mirrors profile names/money and enables Home; it is not a production server adapter. Do not mutate `ShellState` fields directly, and do not deliver an unrelated projection update before matching the authoring receipt. A live provider must instead supply coordinated authoritative projections.
- The pure `PreviewAuthoringProvider` validates operations against its own snapshot and emits typed committed/rejected results. The browser schedules its replies after 850 ms, cancels/disposes owned timers, and delivers only to the originating current authoring session. Keep navigation/close unavailable during that short mutation so it does not falsely promise cancellation of a submitted operation; leaving the page disposes the preview session. Draft Cancel remains immediate before submission.
- The existing `UI preview` label remains visible. Detailed limitations belong in the README/PR; local saves must not be described as account or server saves.

## Persistence and accessible interaction

- Persist the acknowledged authoring snapshot under `wonderland.authoring.v1` in localStorage, with a versioned JSON envelope and a 256 KiB input/output limit. Read and validate before exposing it. Never persist pending operations, secrets, session grants, or source assets.
- Storage failures leave the current tab usable and explain that changes cannot be saved. Invalid, unsupported, or oversized stored data is preserved and is not overwritten automatically; continue with a temporary preview and a clear notice. Do not add a destructive reset action.
- Full fixture scenarios bypass persisted state so they are reproducible. Add `reject-authoring`, `read-only-home`, `poor-home`, and `empty-catalog` cases while preserving existing entry/travel/action fixtures. One-shot rejection permits retry.
- Use semantic controls, visible focus, named dialogs/regions, accessible selected and busy states, and concise live announcements. Keyboard users can choose looks/catalog items, move a placement candidate with arrows, rotate with R, confirm with Enter, and cancel with Escape. Avoid swallowing typing/IME keys inside the name/search inputs. Pointer targets work through ordinary buttons/cells as well as scene selection.
- At desktop and 390 × 844, keep the full-body preview or room visible with the relevant primary action. Use a compact lower drawer on narrow screens. Menus/dialogs stay within the usable viewport; restoring focus reveals the target through camera movement, without independent native scene scrolling.
- Honor reduced motion. Do not animate a static sprite and claim contact/locomotion animation. Raster content and sprite poses remain explicit preview assets.

## Validation and handoff

Native tests cover actual state transitions and serialized snapshots, including commit ordering, duplicate/stale replies, invalid data, draft cancellation, same-instance inventory placement, and per-profile ownership/budget isolation. Camera tests cover screen-to-cell round trips and rotated footprint geometry. Preserve the original 29 tests.

Build the actual release WASM application. Browser-check creation, pagination, outfit Save/Cancel, refreshed local persistence, Home entry, category/search, placement pending/rejection/success, move/cancel/store/place-again, and keyboard/narrow layout. Read current console errors and compare rendered views against the approved visual family. Attempt supported screenshot export again; report any remaining evidence limitation honestly without withholding the reviewable code.

Publish a stacked PR with a clear delta from PR #5, tests/build evidence, asset provenance, reproduced flows, and precise 3D/service integration boundaries. Retain the existing UI PR. Neither the full W11 screen matrix nor production renderer/server completion is claimed by this increment.
