# Wonderland browser shell

Rust-authored, Leptos 0.8.21 CSR interface for the approved character grid, directly selectable Quack's Creek map, and Harbor Café object actions. The browser runs the compiled WebAssembly application. Node is development tooling only.

## Run locally

The repository pins Rust **1.99.0** and the `wasm32-unknown-unknown` target in `rust-toolchain.toml`. Install [Rustup](https://rustup.rs/) if needed, then run from the repository root:

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy --target wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
cd apps/web-shell
npm run dev -- --host 0.0.0.0 --port 4173 --strictPort
```

Use Node 20 or newer for the wrapper. There are no npm package dependencies and no npm install step. `dev` starts Trunk, accepts the three forwarded preview flags, watches the Rust/CSS/fixture inputs, and fails if the requested port is occupied. The app runs at the server's printed address. A plain `trunk serve` also works in a normal Rust development environment.

Some supervised preview environments cannot access a Rust toolchain outside the checkout. If Trunk is unavailable, the same `dev` command serves the existing `dist` directory with Node's HTTP server, including the real `.wasm` bundle with its correct MIME type. It refuses to start without a completed build. Rebuild with Trunk in the development environment and reload that preview after source edits. The fallback does not implement game state, substitute HTML screenshots, or connect to another renderer.

The wrappers normalize the common `NO_COLOR=1` environment value to the boolean spelling accepted by Trunk 0.21.14. Toolchain/cache paths are environment-owned and are never hardcoded into this project.

## Build and verify

From the repository root:

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib --bin wonderland-web-shell --locked -- -D warnings
cd apps/web-shell
npm run build
```

`npm run build` invokes `trunk build --release --locked`. The static build is written to the ignored `apps/web-shell/dist` directory. Serve that directory from an HTTP server with `.wasm` mapped to `application/wasm`; opening `index.html` with a `file:` URL is insufficient. Bundled art, the Nunito font, and Tabler icons use same-origin asset URLs. There are no runtime font CDNs or game-service requests.

The focused [Browser UI workflow](../../.github/workflows/browser-ui.yml) checks the new Rust workspace and builds the WASM shell without changing legacy build jobs. Native tests cover the shared reducer and projection plus camera/overlay geometry and fixture reply behavior. Browser visual and interaction checks remain a separate gate; a successful build alone cannot verify the rendered interface.

## Player flow and controls

1. Select Maya, Jules, Nico, Amara, or Leo. The portrait card, full-body stage, selected state, and green Play label update together.
2. Play enters Quack's Creek. Select a destination on its map lot. Harbor Café and your selected Sim’s Home are available. Park and Arcade retain unavailable reasons.
3. Visit shows a pending state. The café appears only after the adapter's matching acknowledgment.
4. Select the independent coffee-machine sprite. Make coffee, Clean, and Inspect are arranged around the object. Clean is unavailable because the machine is already clean.
5. Make coffee or Inspect first appears as Requested. Only the corresponding accepted reply creates an acknowledged queue item. Cancel shows Cancelling until the distinct cancellation acknowledgment arrives.
6. All needs opens the complete eight labeled meters: Energy, Hunger, Fun, Social, Hygiene, Bladder, Comfort, and Room. Close or Escape returns focus to its opener.
7. Back to city keeps the chosen character and destination. Back again returns to the selected character card.

Drag empty scenery to pan. Use the bottom-right zoom and reset buttons, or focus the scene and use arrow keys, `+`/`-`, and Home. Map labels and sprites use the same world transform as their picks; readable placards and action buttons use projected anchors and viewport clamping. On narrow screens a compact set of destination buttons also brings an offscreen lot into view.

Object-menu Escape returns focus to the machine. City-placard Escape returns focus to the selected place. Settings and All needs are native modal dialogs, retaining browser focus handling. Controls have accessible names and visible focus, selection uses `aria-pressed`, needs expose meter labels/values, and concise status announcements distinguish requests and replies. System reduced motion is honored, with a Reduce motion setting available as well.

## Explicit fixture adapter

`src/bridge.rs` consumes the stable [`ShellState` contract](../../docs/contracts/ui-projection-v1.md) from `wonderland-client-app`; it never mutates the reducer's public presentation fields directly. `src/fixture.rs` constructs preview-only replies. A matching response is delayed by **850 ms** for travel, interaction, and cancellation. It echoes the original operation ID and projection revision. Cancellation uses `CancellationAcknowledged`, not a generic accepted event.

The bridge retains cancellable timeout handles, removes handles after delivery, cancels invalidated work after navigation or selection changes, and clears remaining handles on shell cleanup. Before delivery it also checks that the exact originating request still exists in that shell. Stale callbacks cannot switch another screen or create an acknowledged queue item. Fixture resets use a newer projection through `receive`, preserving operation identity within the shell lifetime.

Reply feedback uses the current event and `receive` result. A retained rejection from an earlier operation does not replace a later acceptance announcement. Map focus reveals destinations outside either axis's usable bounds, and scene clipping prevents native browser scrolling from introducing a second camera offset.

The small **UI preview** label is always visible. These replies demonstrate presentation flow. Authoring changes a local preview budget only; they do not run simulation, spend real money, contact a multiplayer service, authenticate a character, or assert server authorization. Queue items remain acknowledged presentation items until cancelled; they do not fabricate completion progress.

### Reproducible browser scenarios

Optional URL query parameters exercise adapter edge cases without adding test controls to player settings. Each rejection scenario rejects only the first matching request during that page lifetime; retry succeeds.

| Query | Expected behavior |
| --- | --- |
| `?fixture=reject-travel` | First Visit is rejected; remain on the city map with a reason; retry enters the café. |
| `?fixture=reject-interaction` | First machine action is rejected; no acknowledged queue item; retry is accepted. |
| `?fixture=reject-cancellation` | First cancellation is rejected; the acknowledged item stays active; retry removes it. |
| `?fixture=empty-characters` | Empty character state, disabled Play, and a working Try again path. |
| `?fixture=unavailable-characters` | Character selections remain readable, Play is disabled with a reason, and Try again restores the fixture. |

For stale-response checks, start Visit and immediately go Back, or request a machine action and leave the lot before its reply. Wait beyond 850 ms: the earlier screen must not return and its queue must not reappear.

The test-only [`tests/ui/mobile-viewport.html`](../../tests/ui/mobile-viewport.html) harness embeds `/` in a same-origin **390 × 844 CSS-pixel iframe** for browser tools that cannot resize their own viewport. It has no app logic and is outside public/build inputs. For a local QA session, copy it into ignored `dist` after building, visit `/mobile-viewport.html`, and remove the copy before packaging. Normal browser device emulation does not need this harness.

## Artwork and fidelity

The exact approved references are in the [design record](../../docs/design/action-first/README.md). Runtime assets are independent illustrations and transparent sprites; application text, cards, buttons, menu items, HUDs, and request state are semantic Rust-rendered DOM.

The [asset manifest](public/assets/ASSETS.md) records generated artwork provenance, source hashes, Nunito's SIL Open Font License, and Tabler's MIT license. No proprietary TSO assets are included. The original high-resolution PNGs are deliberately retained; production image cooking, delivery-size budgets, LOD, and renderer content are later work.

## Replacement boundaries and next integration work

- **Presentation snapshot and reducer:** Keep `wonderland-contracts` and `wonderland-client-app` DOM-, renderer-, network-, and simulation-free. Consume validated versioned projections and dispatch typed intents; do not write directly into `ShellState` fields.
- **Transport and authentication:** Replace the delayed reply scheduling in `bridge.rs` with an explicit authenticated adapter. Independently authorize actor identity and operations, validate generation/revision, bound source bytes before decoding, and preserve request/response identity. Displayed availability is never authorization.
- **Renderer and picking:** Replace `components/scene.rs` and `geometry.rs` with a renderer adapter that projects stable targets into screen coordinates. Maintain the screen-anchor contract used by `components/actions.rs`; add occlusion and picking without exposing engine entities or DOM nodes to persistent contracts. Evaluate the production 3D camera and WebGPU/WebGL2 fallback separately.
- **Simulation and queue:** Integrate authoritative queue progress/completion, ordered actions, need and money updates, travel rules, and real cancellation outcomes through projections/events.
- **Authoring services:** Replace `authoring_bridge.rs` and the bounded `PreviewAuthoringProvider` with coordinated authoritative authoring and shell projections. Keep typed actor/Home/instance identities, exact operation/base matching, validation, and receipt-before-mirroring ordering. Creation is a preview profile, not an account. Original outfit keys and lot coordinates must remain explicitly mapped to source-content and simulation identities; never cast them into engine entity IDs. The avatar stage renders source meshes; production lot orbit/view switching and chat transport still require their adapters.
- **Content and renderer:** Park and Arcade still need distinct scenes. The Home PNG and calibrated affine adapter in `authoring_geometry.rs` are fixed-camera presentation art, not a movable 3D renderer. Replace image composition and semantic picking with an engine render/pick adapter while retaining accessible keyboard controls, projected ground anchors, usable camera insets, typed outcomes, and ownership checks.

This shell is an integration increment. It includes a bounded original-avatar resource importer and WebGL2 character stage, but live transport, the production world renderer and simulation remain unconnected. It changes no original C# behavior. The complete player-surface inventory and remaining integrations are recorded in [the capability map](../../docs/design/action-first/player-capability-map.md).


## Character and Home authoring preview

Create a Sim opens the full-body garden stage. Head and body use independent source-driven thumbnail grids; skin tone, gender, name and description follow the source account/content fields. Load local original resources to enable actual textured avatar composition. Character creation waits for a matching acknowledged receipt; rejected drafts remain available for retry. Roster pages contain five cards for layout, with no five-identity or eight-profile game rule. Account capacity is explicit provider policy. Historical portrait references remain readable instead of being converted into invented original outfit IDs.

Change outfit uses owned wardrobe entries, categories, default selection and deletion outcomes. Permissions and service readiness come from projections. The source renderer keeps head, body, skin and decoration keys distinct; unsupported decoration role bindings are reported explicitly. Loading binary resources never grants account or wardrobe rights.

Visit Home for the selected profile, then choose Buy. Categories, entries, source keys, prices and permitted rotations come from the catalog projection. The six illustrated starter objects are fixture content. Select an item and floor cell; the candidate footprint explains bounds, occupancy, permissions or budget. Buy and place charges the authorized payer only after a matching newer receipt. Move and Store retain the owned instance and its owner. Place again reuses the instance without charging. Scoped grants support roommate/shared-lot access without transferring object ownership to the lot owner.

The illustrated preview retains its calibrated 8 × 6 scene, while game-facing lot bounds, floors, reserved cells and catalog size are supplied by data. The fixed room, money ceiling and 64-object fixture restrictions are removed from game policy. Build exposes terrain, water, walls, wallpaper, stairs, fireplaces, plants, floors, doors, windows, roof and hand capabilities; construction and world camera actions remain unavailable until an authoritative architecture/renderer adapter is attached. Availability is visible and does not fabricate a successful construction operation.

Focus Home to pan with arrows when no candidate is active, zoom with +/−, and reset with Home. During placement, arrows move the candidate, R rotates directional items, Enter confirms, and Escape cancels. Semantic floor/object/catalog buttons also work by keyboard. Name/search typing and IME composition are not intercepted. Escape restores scene focus without native page scrolling. Reduced motion is honored. Root performs rendered desktop and 390 × 844 verification separately from native geometry tests.

### Local saving and authoring fixtures

Only the validated acknowledged snapshot is encoded in a version-2 envelope at the existing localStorage key `wonderland.authoring.v1`. The stable key preserves conflict detection with older tabs. Version-1 saves migrate without dropping profiles, possessions, balances, poses or historical appearance references. Both decoding and encoding enforce a **16 MiB resource-safety bound**, separate from game capacity. Drafts, pending requests, receipt caches and binary assets are excluded. Persisted availability/grants are presentation data, not live authorization; the actual provider reauthorizes each operation. Reload loads and validates the saved snapshot before projection into the shell. Invalid, unsupported, or oversized existing data is preserved; a temporary preview continues with writing disabled for that load and a visible notice. Storage access/write failures leave the tab usable and explain the failed local save. There is no destructive reset control. Any `fixture` URL bypasses saved data and disables fixture writes.

| Query | Expected authoring behavior |
| --- | --- |
| `?fixture=reject-authoring` | The first authoring request receives a matching typed rejection without committing the provider; draft remains and retry succeeds. |
| `?fixture=read-only-home` | Each Home denies purchase and arrangement, with an explicit candidate reason. |
| `?fixture=poor-home` | Each profile has a preview budget of 20, so catalog placement explains insufficient funds. |
| `?fixture=empty-catalog` | Fresh empty Homes and empty catalog; clear empty state, no invented objects. |

Original travel/action/empty/unavailable fixture behavior is preserved. The authoring session owns its timer and independent lifetime guard. Current-session callbacks additionally compare the exact pending request. `authoring_adapter::deliver` classifies success only after a newer revision plus matching operation/base/receipt; `receive(Ok)` and a retained `last_commit` alone are insufficient. A current rejection is classified from its event, never an older retained error. Verified commits mirror through `preview_project_authoring_to_ui` and `ShellState::receive`, then save. Only original shell selection intents select a new character; DOM code never mutates reducer fields.

### Runtime artwork mapping

Historical illustrated portraits resolve their preserved source references through the existing art manifest. New original-avatar selections resolve native packed outfit/resource keys through the bounded importer, appearance composer and browser mesh renderer; a historic illustration name is never converted to a game outfit ID. Source thumbnails use decoded original image resources. The original content files are local verification inputs and are not committed or deployed with this shell. See [content integration](../../docs/design/action-first/content-integration.md) for imported crate revisions, source-format corrections and verification boundaries.

Home still uses `/assets/authoring/home-scene.png` and the existing directional fixture artwork. Exact art prompts, hashes, dimensions and alpha bounds remain in `public/assets/authoring-manifest.json`. Chrome stays semantic Rust-rendered DOM using bundled Nunito and Tabler assets.
