# Preserve game capabilities in the browser redesign

**User authority:** `docs/design/action-first/preservation-scope.md`, followed by the user's explicit “so fix that” on 5 October 2026. This is a correction to the authorized implementation, not a new product scope or a request for another approval.

**Goal:** remove the prototype restrictions from game-facing contracts and controls, restore independent character composition and source-backed world editing, and retain a visible, honest path to every original player capability. Keep the character stage, roster grid, interactive map and object-attached actions. Preserve existing saved data and accepted-request safeguards.

**Architecture:** versioned content/account/lot projections supply identifiers, choices, policy, availability and renderer resources. Fixture content remains an adapter. A legacy adapter reports the original three-avatar account policy and original creation fields; unknown policy is not unlimited. Existing preview saves migrate without deleting profiles, possessions, money or appearance references. Swarm C's newly published `render-core` and `avatar-view` crates provide the appearance-composition seam; actual asset readiness remains explicit. No invented successful network, construction or renderer operation.

## Task 1: Generalize authoring contracts and state

Replace the five-value identity model, three-look coupling, exact six-record catalog validation, eight-profile rule, 1,250 budget ceiling, 64-object game limit and 8-by-6 room rule with versioned data. Keep byte/count safety limits separate from game policy. Preserve independent head/body/skin/gender, name/description, account/shard capability, owned wardrobe identity/categories, catalog category and rotation metadata, lot bounds/levels and build capabilities. Carry original content keys and service revisions through requests. Retain operation/revision matching, atomic accepted receipts, rejection drafts and conflict-safe persistence. Provide explicit v1 migration; never silently map an illustrated legacy portrait to a fabricated game outfit. Adapt dependent browser code as necessary to compile. Add behavioral regressions for non-fixture data, independent choices, service policy, variable geometry and lossless migration.

## Task 2: Restore the character-centered creator and renderer boundary

Replace identity and three-look buttons with content-driven head/body grids, skin and gender choices, name and description, source account availability, and owned wardrobe categories/actions. Retain the dominant character stage and visual selection. Integrate the actual Swarm C composition types and preview interface with source content input. The repository lacks the original adult skeleton, base outfit collections and normal head/body payloads; sample or diagnostic imagery must not claim full textured game-avatar rendering. Preserve saved portrait references until actual content is available. Expose content readiness and errors as actionable states, and keep creation drafts on rejection.

## Task 3: Restore world-editing controls and player-surface coverage

Drive Buy categories, catalog entries, prices, inventory and rotation from projection data. Use lot-supplied bounds and levels. Expose source Build tools (terrain, water, walls, wallpaper, stairs, fireplaces, plants, floors, doors, windows, roof and hand), view modes and architecture preview/commit requests with permissions and explicit unavailable states where the runtime is not connected. Preserve object movement/storage and add source-backed action contracts instead of pretending furniture arrangement is full Build. Record each original primary player surface against its new control, provider/renderer dependency and verified status. Do not relabel missing features as completed.

## Task 4: Verify and publish the corrected implementation

Review the complete correction against the user requirement and original source. Run necessary Rust tests, formatting/clippy and a WASM release build; inspect the actual browser creator, roster, Home/catalog/build and saved-data recovery. Take real screenshots where the preview is available. Publish the exact reviewed tree to the existing authorized draft PR without merging or deploying. Explain what now works and any remaining concrete content/service integration gaps.

## Evidence and constraints

- Original creator: `TSOClient/tso.client/UI/Screens/PersonSelectionEdit.cs` and its controller; independent head/body collections, three skin appearances, gender, name and description, rotating `UISim`.
- Original account policy: `PersonSelection.cs` and `0001_3_max_avatars.sql`; three avatars, server authoritative. Existing preview saves above three must remain readable.
- Original wardrobe: `UIDresserEOD.cs`; Day/Sleep/Swim plus decoration categories, owned outfit identity, change/default/delete.
- Original Buy/Build: `UIBuyMode.cs`, `UICatalog.cs`, `UIObjectHolder.cs`, `UIBuildMode.cs`, `VMArchitectureCommand.cs`, `UIUCP.cs`; content collections, lot dimensions/levels, rights and architecture actions.
- No legacy game source deletion or changes are required. Draft PR #10 is the existing UI correction target; #5 remains its base.
- New policy must not be inferred from demo convenience. Rendered fixture primitives are not evidence of original content parity.

## Verification ownership

Implementation agents own targeted regression evidence. The controller owns browser checks, publication and one final integration gate. A fresh reviewer checks concrete diffs; repeat only for a changed risk or required gate.
