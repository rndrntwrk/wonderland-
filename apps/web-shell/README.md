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
2. Play enters Quack's Creek. Select a destination on its map lot. Harbor Café shows six visitors and an available Visit button. Park, Arcade, and Home explain that their own preview scenes are not present.
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

The small **UI preview** label is always visible. These replies demonstrate presentation flow. They do not run simulation, spend money, contact a multiplayer service, authenticate a character, or assert server authorization. Queue items remain acknowledged presentation items until cancelled; they do not fabricate completion progress.

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
- **Creation, outfit, catalog, and placement:** Add these subsystem adapters before enabling their controls. Build, Buy, outfit editing, and Create a Sim currently show concise unavailable reasons. Actual 3D orbit/view switching and chat transport are omitted.
- **Content:** Add distinct scenes and validated place availability for Park, Arcade, and Home before enabling Visit. The café fixture must not impersonate those locations. Replace the five-portrait fixture asset mapping with a versioned content manifest when character content becomes dynamic.

This shell is an integration increment, not the completed FreeSO browser rewrite. It adds no legacy C# behavior changes, live transport, persistence, asset importer, production renderer, or simulation.
