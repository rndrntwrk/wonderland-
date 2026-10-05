# Wonderland: action-focused browser UI, first integration slice

## Outcome and authority

Implement the approved character grid, directly selectable city map, and object-anchored lot actions in Wonderland. The user approved the revised game screens and authorized starting work and opening pull requests. The target is `rndrntwrk/wonderland-`, a direct FreeSO fork. Its `master` baseline is `4c6b3e8f5835b228723caea3c9f683c62f244f73`.

This is the first W11 UI increment of the separately prepared FreeSO Rust/browser rewrite. The existing C# client remains a behavior reference. This increment implements a Rust-authored browser shell with explicit fixture adapters; it does not implement the simulation, a multiplayer service, asset import, or the production 3D renderer.

## Approved interaction direction

The game world and characters occupy the screen. Select a character by clicking a large visual card. Select a destination by clicking its place on the map. Select an object in the lot to reveal actions attached to it. Create, edit, buy, and build will use visual selection and direct manipulation as subsequent increments. Avoid replacing these actions with profile forms, directories, dashboards, or persistent inspector sidebars.

Approved reference images supplied during this conversation:

- Character grid and full-body stage: `exec-9e1b0635-a979-439e-ba18-a9bafb4c63a7.png`.
- Direct city map: `exec-dcef709e-a28f-432c-8cb3-649478c547ec.png`.
- Café with three object actions and labeled needs: `exec-ea0c822b-a985-4ec1-9c6a-2f9e03cee803.png`.

The image names identify design references, not runtime dependencies. Runtime artwork is separately generated, contains no embedded UI, is checked into the repository with provenance, and is composed with semantic interactive controls.

The approved references are available in the repository's [visual design record](../../design/action-first/README.md).

## First PR scope

### Character selection

- Five selectable adults: Maya, Jules, Nico, Amara, Leo. Their cards are a 3-by-2 desktop grid, with the sixth slot reserved for Create a Sim.
- A large selected character stage and a clear green `Play as {name}` action. Selecting another card updates the stage, accessible selected state, and button label.
- The selected appearance follows the approved reference: Maya has dark skin, a curly high puff, mustard jacket, white top, indigo jeans, and white sneakers. Other characters retain the reference's distinct silhouettes and colors.
- Character creation and outfit editing are visibly unavailable in this increment with a concise reason. Do not fake saving edits or rotate a flat image and describe it as 3D character rotation.
- Empty and unavailable character projections leave a comprehensible path to retry or return; no panic, stale selection, or enabled Play without a playable character.

### City

- A full-screen illustrated Quack's Creek fixture map. Destinations are positioned in a defined normalized map coordinate space. Semantic buttons overlay their locations and move with the same pan/zoom transform as the map.
- Harbor Café, Park, Arcade, and Home can be selected directly. The selected place receives green emphasis and a small anchored placard with its name, population, and `Visit` action. An unavailable destination explains why.
- Harbor Café is the available lot fixture for this increment. Other locations are selectable on the map but display a concise unavailable reason until their own scenes arrive; the café illustration must not be reused to impersonate them.
- Keyboard users can reach the same destinations without needing pointer coordinates. Focus is visible. Place controls remain discoverable at narrow widths.
- Pan/zoom and reset controls operate on the fixture map. The world remains the dominant surface; do not add a directory sidebar.
- `Visit` submits a request, shows pending feedback, and enters a destination only when the adapter acknowledges it. A rejection stays on the map and offers retry. Back returns to character selection.

### Lot interface

- Use an illustrated café fixture scene as the replaceable presentation adapter. Independently composited character and coffee-machine assets are actual selectable elements; never use an entire mockup screenshot as the application.
- Selecting the coffee machine reveals three object-anchored actions: `Make coffee`, `Clean`, `Inspect`. Available, unavailable, focused, and selected states are distinguishable. The menu stays anchored during scene pan/zoom and clamps inside the usable viewport.
- `Make coffee` creates a pending operation; it becomes an acknowledged queue item only after a fixture response. A rejection shows its reason. Queue cancellation has pending and acknowledged states; unrelated or stale responses cannot mutate another operation.
- A compact HUD shows the chosen character, money, and labeled Energy, Hunger, Fun, Social meters. `All needs` exposes all eight legacy needs, including Hygiene, Bladder, Comfort, and Room.
- Back returns to the city while preserving the selected character and destination. Pending stale travel/action responses cannot change a different screen or resurrect cleared state.
- Additional game controls can be present only with clear unavailable states where the next subsystem is required. Actual 3D orbit, view switching, build/buy placement, simulation, chat transport, and outfit rotation are subsequent work. Do not imply those are functioning.

## Presentation and accessibility

- Preserve the rounded blue game chrome, strong green selected/primary state, warm scenic artwork, large faces, and minimal text of the approved references. CSS gradients may reproduce control chrome; actual imagery and icons use real image assets.
- Use a bundled open font close to the reference and a consistent existing icon set with its license. Avoid emoji, hand-drawn SVG icons, generic placeholder art, and remote runtime font/CDN dependencies.
- Desktop reference ratio is 16:9. Verify at 1440-by-900 or the nearest available desktop viewport and a narrow 390-by-844 viewport. Narrow screens may rearrange the grid and stage; they must preserve the character/map/action emphasis and avoid horizontal page overflow.
- Semantic buttons, accessible names, selected state, visible keyboard focus, Escape dismissal, appropriate focus restoration, live status announcements, and reduced-motion support are required.
- Keep a small `UI preview` label visible so a fixture response is not mistaken for a live service. Detailed adapter limitations belong in the README and PR, not in every player-facing control.

## Architecture

- Rust 1.99.0, edition 2024; stable Leptos 0.8.21 CSR with Trunk and `wasm32-unknown-unknown`.
- A root Cargo workspace introduces `crates/contracts`, `crates/client-app`, and `apps/web-shell`. The contracts crate has no browser, renderer, network, or simulation dependency. The client-app crate owns pure UI state transitions and has no DOM dependency.
- `UiProjection` is a versioned, bounded read-only presentation fixture, not a full-world copy or frozen multiplayer wire protocol. Use stable string IDs and generation-aware object references. Contract fields and fixture schema are documented under `docs/contracts/`.
- `ShellState::new(projection: UiProjection) -> Self`, `ShellState::dispatch(intent: UiIntent) -> Result<Vec<UiRequest>, UiError>`, and `ShellState::receive(event: UiEvent) -> Result<(), UiError>` form the shell boundary. Selected avatar/destination/target, screen, pending operations, and queue are readable from state. Only matching current requests and revisions are applied.
- Requests have unique operation IDs and distinguish travel, interaction, and cancellation. Interaction requests carry stable target reference, action ID, and expected revision. Future server adapters supply authenticated actor identity and verify permissions independently; displayed eligibility is never authorization.
- A deterministic fixture adapter supplies delayed responses for the UI preview. It cannot silently become a live transport. Scene coordinates and picking are presentation data; persistent contracts never contain DOM nodes or engine entity IDs.
- The fixture scene is an illustration with pan/zoom, not a 3D renderer. Production Bevy/Fyrox evaluation, actual projected anchors, picking, occlusion, camera ownership, and WebGPU/WebGL2 fallback remain renderer work packages.

## Verification and deliverables

- Native tests exercise valid/invalid selection, unavailable destinations/actions, pending versus acknowledgment, rejection, duplicate/stale replies, cancellation, back navigation during pending work, and exact all-eight-needs coverage.
- Build the Leptos shell to WASM. Run formatting and clippy on the workspace's applicable targets. Add a focused CI workflow for this new workspace without changing legacy build jobs.
- Open the actual browser preview, use the primary journey, test keyboard and narrow layout, inspect console output, and capture the implemented screens. A successful build or static HTTP response is insufficient.
- Include documented local commands, fixture semantics, adapter contracts, asset provenance/licenses, known limitations, and the next integration tasks. Do not distribute proprietary TSO game assets.
- Open a feature PR against Wonderland's `master`. Do not merge or deploy.
