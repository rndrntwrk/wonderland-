# Character and Home UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an action-focused character creator, wardrobe, and directly editable Home scene to the existing Wonderland browser UI.

**Architecture:** Preserve the existing game shell boundary. A separate pure authoring reducer consumes bounded snapshots and typed commit outcomes from an explicit preview provider; a browser adapter persists acknowledged snapshots and mirrors them into the original shell through its event API. Leptos renders the character stage, catalog, inventory, placement previews, and object-attached editing controls.

**Tech Stack:** Rust 1.99.0, edition 2024, Leptos 0.8.21 CSR, serde, web-sys, Trunk 0.21.14, bundled PNG art/Nunito/Tabler assets, HTML/CSS.

**Spec:** `docs/superpowers/specs/2026-10-05-character-and-home-ui.md`

## Global Constraints

- Work only in `rndrntwrk/wonderland-`, branch `feat/character-and-home-ui`, local base `bd08bd1652bec851e42a9ad8348c4c3902a88745`; publish stacked against `feat/action-first-browser-ui` at remote parent `b921c76fb2bea478a0685c9857e00e0fc309c097`.
- Keep Rust 1.99.0 / edition 2024 / Leptos 0.8.21 / Trunk 0.21.14. No framework replacement or unrelated dependency upgrades.
- Preserve the original character/map/café flow and its compatible public contracts. DOM consumers never mutate reducer presentation fields directly.
- This is explicit authoring preview infrastructure, with no live account, economic, multiplayer, simulation, or production 3D claim. Do not merge or modify Swarm A/B branches or import their test harnesses.
- Preserve the approved scenic game design, blue/green controls, full-body stage, and direct world actions. No form/dashboard or full-mockup-image substitutes.
- Authoring mutations allow at most one in-flight request; accepted data comes from a matching typed result plus a validated newer snapshot. Money/instances do not change optimistically.
- Profile cap 8; name length 1–32 Unicode scalars and at most 128 UTF-8 bytes after trimming, no controls; initial money 1,250 per profile; room 8 × 6; reserved cell (0,0); owned-instance cap 64 per profile; local snapshot byte cap 256 KiB.
- Runtime code is Rust. JS/Node/Python may be development/test tooling and generated bindings. Use real separately generated art and licensed bundled icons/fonts; maintain provenance.
- Integrator owns spec/plan, asset imports/manifests, design evidence, and PR publication. Product code and fixes go through the task implementer.

## Review Focus

- Delayed or duplicate authoring replies after another snapshot/session must not select the wrong character, duplicate ownership/cost, or close the wrong editor — Task 1 behavioral tests and Task 2 timer guards.
- A rotated wide footprint at an edge, entry, or another object's cells must remain invalid; moving an object may reuse its own cells — Task 1 preview geometry/provider tests and Task 2 screen mapping tests.
- Different profiles must not share money or expose each other's stored instances/room mutations — Task 1 isolation tests, Task 2 profile switching browser checks.
- Corrupt/oversized/unsupported stored state and unavailable storage must preserve data and usable in-memory UI — Task 2 codec tests and persistence adapter checks.
- IME input and keyboard/touch placement must preserve typing, focus, camera alignment, and reachable primary actions at 390 × 844 — Task 2 browser verification with scoped native geometry tests.

## Asset inputs

The integrator supplies `apps/web-shell/public/assets/authoring/` and an authoring manifest. Existing `<profile>.png` files remain Everyday looks. Smart and Active files are `<profile>-smart.png` and `<profile>-active.png` for `maya`, `jules`, `nico`, `amara`, `leo`.

Home uses `home-scene.png`. Catalog directional files: `armchair-front.png`, `armchair-back.png`, `coffee-table-front.png`, `coffee-table-back.png`, `bookcase-front.png`, `bookcase-back.png`. Symmetric files: `floor-lamp.png`, `fern.png`, `woven-rug.png`. Directional mapping must use whole sprites and horizontal mirroring only; no flat image camera rotation. Asset import may complete while Task 1 runs; Task 2 must use final reviewed assets for its build/QA.

### Task 1: Pure authoring state, provider contracts, and acknowledged preview operations

**Files:**

- Create: `crates/contracts/src/authoring.rs` or focused `authoring/` modules; expose from `crates/contracts/src/lib.rs`.
- Create: `crates/client-app/src/authoring.rs` or focused `authoring/` modules; expose from `crates/client-app/src/lib.rs`.
- Create: `fixtures/ui/authoring-v1.json`, `docs/contracts/authoring-v1.md`.
- Test: `crates/contracts/tests/authoring_fixture.rs`, `crates/client-app/tests/authoring_flow.rs` and one focused provider/geometry test file if separation helps.
- Modify manifests/lockfile only if the existing serde dependencies cannot serve the implementation; no unrelated upgrades.

**Interfaces:**

- Reuse `CharacterId`, `Character`, `Availability`, and typed operation identity from the original contracts.
- Produce `wonderland_contracts::authoring::{AuthoringProjection, AuthoringIntent, AuthoringRequest, AuthoringEvent, AuthoringError}` and documented supporting types for valid looks, profile/home/catalog/owned-instance data, editor drafts, and grid poses.
- Produce `wonderland_client_app::authoring::AuthoringState::new(projection: AuthoringProjection) -> Self`, `dispatch(intent: AuthoringIntent) -> Result<Vec<AuthoringRequest>, AuthoringError>`, `receive(event: AuthoringEvent) -> Result<(), AuthoringError>`, and `preview_authoring_projection() -> AuthoringProjection`.
- Produce a pure `PreviewAuthoringProvider` constructed from a validated projection, with a `handle(&mut self, request: &AuthoringRequest) -> AuthoringEvent` method and read-only snapshot access. Typed committed replies include operation/base revision, complete validated snapshot, and outcome identity. Define constructor/error signatures and all enums in the report so Task 2 consumes the exact API.
- Produce an explicitly named preview projector that takes the current compatible `UiProjection` and authoring snapshot, returns a validated newer `UiProjection`, mirrors profile names/money, and enables Home. It must not mutate `ShellState`; the browser delivers it through `UiEvent::ProjectionUpdated` only after the matching authoring commit.
- Support creation/outfit drafts, catalog candidate selection, grid movement/direction, purchase, move, store, placement from inventory, and close/cancel. Profile/room selection is explicit; per-profile permissions and data are isolated.
- Catalog IDs/prices/categories/footprints are exactly the six rows in the spec. Look IDs follow `<profile>-everyday`, `<profile>-smart`, `<profile>-active`; each profile identity permits only its own looks. The original five profiles and budgets seed the fixture, with one empty room per profile.
- If a necessary intermediate type/variant is not prescribed, choose the smallest coherent API, document it, and report it before the UI task starts. Do not recreate unrelated simulation or economy systems.

- [ ] **Step 1: Write meaningful native tests before implementation.** Cover fixture round-trip/validation; all five identities × three valid looks; create/save pending without committed mutation; matching created identity exactly once; invalid/empty/Unicode name handling; cap at eight; Cancel preserves appearance; outfit commit updates only its target; wrong operation/base/revision or invalid snapshot leaves prior state intact; replacement invalidates drafts/pending without operation reuse.
- [ ] **Step 2: Add provider and placement tests.** Exact budget 1,250 → 1,070 for one 180 armchair only after commitment; duplicate request/reply does not charge twice; stale requests reject; denied/poor/occupied/entry/out-of-bounds/cap cases leave provider snapshot unchanged; 2 × 1 rotates to 1 × 2; own-footprint move succeeds; cancel move retains placement; Store preserves money/instance identity; Place again does not charge; another actor cannot use the stored instance or spend the owner's money.
- [ ] **Step 3: Run the focused tests and record expected missing-API or failed behavioral assertions.** Use the existing toolchain environment, not dependency/network errors as a red phase.
- [ ] **Step 4: Implement the contracts, pure reducer, bounded preview provider/fixture, and preview shell projector.** Validate before mutation, bound counts and integer arithmetic, preserve the original public game behavior, and document the live adapter boundary and event ordering. Keep modules focused.
- [ ] **Step 5: Run `cargo test --workspace --locked`, `cargo fmt --all -- --check`, and native clippy for the affected crates with `-D warnings`.** Verify the original 29 tests remain passing. Report the exact new test count and any concrete limitations.
- [ ] **Step 6: Commit only owned files and write the task report.** Include the complete exported API, state/receipt ordering, source paths, commands/results, and any concerns for Task 2. Commit message: `feat(ui): add character and room authoring contracts`.

### Task 2: Character stage, wardrobe, Home catalog/placement UI, and bounded persistence

**Files:**

- Create: `apps/web-shell/src/screens/creator.rs`, `apps/web-shell/src/screens/home.rs`, focused `components/authoring/` modules, `apps/web-shell/src/authoring_bridge.rs`, `apps/web-shell/src/persistence.rs`, and supporting geometry module(s).
- Modify: `apps/web-shell/src/{app,bridge,lib}.rs`, `screens/{mod,avatars,lot,city}.rs`, `components/{mod,hud,scene}.rs` only as needed for integration.
- Create/modify: focused authoring styles in `apps/web-shell/public/`, `index.html` or import path, shell README, local viewport harness as needed, Browser UI workflow only if new paths need its existing checks.
- Test: `apps/web-shell/tests/authoring_geometry.rs`, `apps/web-shell/tests/authoring_persistence.rs`, and non-DOM fixture adapter tests for newly introduced logic.
- Consume the reviewed Task 1 API and integrator-owned artwork/manifest; do not change shared core contracts without reporting a specific integration defect.

**Interfaces:**

- Consume Task 1's public API from its contract doc/report. UI state is read-only; dispatch/receive own mutation. Retain the original `Ui` game bridge and add an authoring adapter with an independent current-session guard.
- Add fixed 850 ms preview reply scheduling, disposal on shell cleanup, and explicit matching request checks. Do not classify a current reply using an old retained error. Project into the original shell only after accepted authoring data, then update selection/focus for the appropriate current editor.
- Persist a versioned acknowledged snapshot envelope under `wonderland.authoring.v1`, bounded to 256 KiB. Export native-testable encode/decode helpers; separate web storage access. Invalid/unsupported/oversized data is preserved and disables writing for that load, while a temporary preview remains usable. Storage failures show concise persistence feedback.
- Route the new creator/wardrobe as full-stage authoring views and Home based on the actual selected lot ID. Dynamic character data must remain reactive inside keyed children: saving a name/look or creating a profile must refresh every relevant label/image, not only first render.
- Fixed preview grid: 8 × 6; use a documented isometric affine floor projection and inverse picking with camera pan/zoom. The integrator reviews floor alignment against the new Home illustration before final acceptance; geometry values may be calibrated from that art without changing canonical grid dimensions. Use the same transform for image, tile, ghost, and placed objects.

- [ ] **Step 1: Read the exact Task 1 report and visual brief, then implement the native codec/geometry tests first.** Verify screen→grid→screen through pan/zoom, edge cells and invalid points, rotated extent, bounded envelope round trips, invalid version/corrupt/oversized input, and no persistence of draft/pending data. Test runtime behavior rather than source text.
- [ ] **Step 2: Implement the authoring bridge and persistence using the defined API.** Support saved snapshot loading, typed one-shot rejection scenarios, safe temporary fallback, and matching commit-to-shell ordering. Add `reject-authoring`, `read-only-home`, `poor-home`, and `empty-catalog` fixture selection while preserving original fixture behavior. Scenario URLs bypass local saved state.
- [ ] **Step 3: Implement full-stage Create a Sim and Change outfit.** Use five profile cards per selection page plus Create; include previous/next controls when needed. Creator identity/three-look cards update the stage; a small nameplate accepts Unicode/IME input. Save/Create pending locks submission/navigation until the short preview reply; rejection preserves draft; Cancel restores committed state; accepted selection and all portraits update. No profile deletion.
- [ ] **Step 4: Implement Home, Buy, Build, and inventory.** Use actual six-object thumbnails, category/search, scene tile picks, translucent ghost plus explicit validity reason, and Buy and place. Existing placed instances expose Move/Rotate/Store and stored items can be placed again. Preserve per-profile money/room isolation, pending/rejected/committed states, and original café behavior. Do not present unavailable architectural construction or simulation actions as functioning.
- [ ] **Step 5: Implement responsive and accessible behavior.** Keep scene/stage dominant and primary controls reachable at desktop and 390 × 844; all essential pointer actions have keyboard counterparts, typing keys are not intercepted, and Escape/focus restoration reveals scene targets without native scrolling. Keep overlays within chrome/HUD exclusions and honor reduced motion.
- [ ] **Step 6: Document run/fixture/persistence instructions, asset mapping, and precise service/3D replacement boundaries.** Integrator writes the source-backed cross-swarm handoff note and design QA evidence. Do not mark the full game or W11 screen matrix complete.
- [ ] **Step 7: Run workspace tests, fmt, native/WASM clippy, Node syntax checks only for touched tooling, and the actual release Trunk build.** Start or preserve the supervised preview, notify the integrator to refresh, fix concrete browser findings, and rerun covering checks. Browser verification belongs to the integrator.
- [ ] **Step 8: Commit only owned files and write the task report.** Include screen/scenario behavior, exported helpers, commands/results, final build timestamp, and limitations. Commit message: `feat(ui): add visual character and home editing`.
