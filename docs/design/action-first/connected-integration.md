# Connected Wonderland integration handoff

This branch advances the action-focused redesign into original-service account flows, source city/lot rendering, player panels, source authoring requests and an integrated A/B simulation library. The existing character/Home preview and original FreeSO source remain available. The [21-surface capability map](player-capability-map.md) records what works together and what still needs implementation.

**This is a reviewable integration increment, not a complete replacement for the original FreeSO client.** The connected lot currently presents source snapshots; it does not continuously restore and replay the original VM. Bookmarks/profile updates, comprehensive building/catalog/inventory adapters and most specialized object dialogs are still code work. A live server or an asset upload alone cannot close those gaps.

## Review target and source pins

The destination is the owner's fork, [`rndrntwrk/wonderland-`](https://github.com/rndrntwrk/wonderland-). This work is stacked on [PR #10](https://github.com/rndrntwrk/wonderland-/pull/10), `feat/character-and-home-ui`. The integration branch is `feat/playable-world-integration`; its proposed PR base is `feat/character-and-home-ui`. The inspected source branches and PRs are unmerged. Importing their code here is not evidence of upstream branch integration or deployment.

| Input | Exact revision |
| --- | --- |
| Corrected browser shell / PR #10 | `7ee14583ca10ed86511d717566295ea44d481bf7` |
| A simulation | `8a0e251d19e222a0a6833d7408ca629f674e1729` |
| B content, interactions and tools | `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7` |
| C rendering and audio | `f6f78be1fef247f2db47e19f56d94054f0c9e88c` |
| Original FreeSO baseline retained under `TSOClient` | `4c6b3e8f5835b228723caea3c9f683c62f244f73` |

The manifests and lockfile integrate those packages while retaining the earlier original-content decoder corrections. See the [implementation specification](../../superpowers/specs/2026-10-05-connected-game-integration.md), [game-runtime source notes](../../../crates/game-runtime/README.md), [world renderer](../../../crates/world-view/README.md) and [audio notes](../../../crates/audio-content/README.md).

## What each mode proves

| Surface | Data and execution | Reviewable behavior | Boundary |
| --- | --- | --- | --- |
| **Preview** | Explicit local provider and existing versioned device saves | Character grid/stage, original-content creator, local destination/object/Home journeys, outfits and room arrangement | Preview travel/action responses and illustrated rooms are not connected game transactions. They remain labeled preview content. |
| **Original lot** | Checked-in original empty-lot XML, or supplied blueprint XML/normalized world JSON; source geometry and software depth rendering | Pan/rotate/zoom, floor/wall/roof controls and depth-correct tile/object inspection | Local source inspection, not assigned to a saved Home. Missing meshes/textures stay diagnosed. This file picker does not currently import arbitrary FSOv saves. |
| **Connected account/city** | Original OAuth/XML/directory APIs through a native gateway, plus city/lot ticket protocol | Account roster, creation, city map/discovery, property/social/mail/neighborhood flows with source outcomes | Requires operator configuration and a real service. Controlled peers establish protocol behavior, not deployed compatibility. |
| **Connected property snapshot** | Original FSOv38 StateSync checked against browser/source epochs, selected avatar and lot incarnation | Source architecture, resolved source models, snapshot queue/needs/budget, refresh and supported guarded requests | `RefreshOnly`: later source ticks can make the snapshot stale. Refresh is not action acknowledgement or continuous simulation. |
| **A/B deterministic runtime** | Original resources and A's accepted command/tick contract | Native tests execute unchanged original BHAVs, detached checks, queues/cancellation and confirmed construction | Not mounted as a restored original-server VM. Original `VMNetTickList`, FSOv and A snapshots are different protocols. |

The world renderer uses the same geometry and camera for visible pixels and picking. It retains real terrain, floor/wall records, heightfields and pool meshes. The eight placed props in the bundled original empty lot lack matching complete models in the checked-in resource set; unresolved props are not replaced with invented furniture. In-world avatar appearance, contained-object placement, room/support maps and advanced rendering still need adapters and source inputs. See the [renderer limitations](../../../crates/world-view/README.md#implemented-limits-and-remaining-parity).

## Run the browser preview

Use pinned Rust 1.99 and Trunk 0.21.14. From the repository root:

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy --target wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
cd apps/web-shell
trunk serve --release --locked
```

The development port is `4173`. The shipped `public/wonderland-config.json` explicitly selects:

```json
{
  "version": 1,
  "mode": "preview",
  "gateway_url": null
}
```

Startup loads this same-origin file with a bounded read and validates its version, mode and endpoint. A missing, malformed or incompatible configuration produces a retry/error screen. A failed connected startup never silently creates a preview session. Build output copies the configuration into `apps/web-shell/dist/`.

For visual review, use **Game content** to load the user's original appearance resources, choose independent heads/bodies, rotate the Sim and move across roster pages. **Original lot** opens the source geometry viewer. Existing Home and illustrated preview journeys remain available after returning. The source audio adapter implements local MP3/WAVE audition separately from accepted runtime audio, but playback was not verified in the final browser: its module import was blocked with `ERR_BLOCKED_BY_CLIENT`.

## Configure an original-service connection

Run the native gateway in another terminal:

```sh
cargo run -p wonderland-browser-gateway --locked
```

It listens on `127.0.0.1:8787` by default. Without configuration, `/health` reports `configured: false`. Supply the operator's own deployment settings from the [gateway README](../../../services/browser-gateway/README.md#run):

| Setting | Required value |
| --- | --- |
| `WONDERLAND_API_BASE_URL` | Original account/directory API base; HTTPS for a remote service. |
| `WONDERLAND_BROWSER_ORIGINS` | Exact allowed browser origins, including scheme and port. |
| `WONDERLAND_TCP_ALLOWLIST` | Final authenticated city/lot `host:port` destinations, resolved and pinned at gateway startup. |
| `WONDERLAND_GATEWAY_BIND` | Listener address if changing the loopback default. |

The original client appends literal `101` to server-selected connection addresses; allow the resulting final destination. The browser does not choose a native destination or source authentication ticket. Remote deployment needs operator-managed HTTPS/WSS and a reverse proxy; this branch does not publish a service or provision certificates.

For a same-origin proxy routing `/gateway/...` to the gateway's corresponding `/...` routes, use:

```json
{
  "version": 1,
  "mode": "connected",
  "gateway_url": "/gateway"
}
```

For local development, `http://127.0.0.1:8787` is also valid. A remote absolute endpoint must use HTTPS. Endpoints cannot contain credentials, query strings or fragments. The player signs in through the account screen; passwords and bearer tokens stay in memory and never enter preview saves or URL parameters. Logout revokes the captured gateway session and invalidates in-flight work. The [HTTP/WebSocket contract](../../../services/browser-gateway/README.md#browser-http-contract) documents routes and lifecycle rules.

### Controlled source-wire verification

The retained automated replay creates ephemeral local HTTP, city and lot peers and drives them through the real gateway:

```sh
cargo test -p wonderland-browser-gateway --test gateway_replay --locked
```

For an interactive local test, the explicitly test-only example uses loopback gateway port `18787`:

```sh
WONDERLAND_REPLAY_BROWSER_ORIGINS='http://127.0.0.1:4173' cargo run -p wonderland-browser-gateway --example controlled_replay --locked
```

Open the browser at that exact page origin and set its connected `gateway_url` to `http://127.0.0.1:18787`. `WONDERLAND_REPLAY_BIND` can change the listener but must stay loopback. A human tester can sign in through the unchanged account screen using synthetic username `controlled-player` and password `test-only`. These are public fixture values, not a deployed account. The fixture returns Controlled City, avatar 42, database lot 1 at packed location 55 and neighborhood 99.

The example performs real gateway city/lot handshakes against its local peers, then reuses the [synthetic v38 source-wire snapshot](../../../crates/vm-protocol/tests/fixtures/README.md) on refresh. It has one fixture object and does not simulate subsequent game ticks or invent edit receipts. Other VM commands do not create fake mutations. These bytes are not original saves or observed production responses. The fixture example is not the production backend. A successful controlled test must never be described as live Wonderland gameplay, and native replay alone does not establish an observed browser login/city/lot journey.

## State, results and save preservation

All returned Sims remain reachable. A five-card page is not a five-Sim limit. The inspected original server's three-avatar policy is a backend rule and does not delete preview Sims. Original 64-bit appearance IDs remain lossless, owned wardrobe records stay separate from asset keys, and packed city locations remain separate from database lot IDs.

Existing preview saves retain profiles, names/descriptions, outfits, possessions, homes and budget. Version migration, conflict detection and storage-failure behavior remain tested. Connected mode does not open those saves or use their balances/ownership as online authority. Opening a local source world does not overwrite Home or import it as a purchase. A's schema-1 checkpoint migration is separate from browser saves and original FSOv.

Results distinguish **pending**, **accepted**, **rejected** and **unknown**. Private-message ACKs acknowledge delivery, not a person's reading. Raw lot commands, retirement and some original deletes lack a universal durable receipt. A packet write, generic OK or lot refresh cannot manufacture success. Matching source effects and subsequent authoritative reads reconcile supported cases. Reconnect does not replay unresolved purchases or other writes. Actor changes, prior lot incarnations and stale callbacks cannot restore old HUD/private state.

## Remaining integration requirements

The capability map gives the exact acceptance list. Its outstanding code work is concentrated in continuous original VM restoration/action execution; source world/avatar/content resolution; complete catalog/inventory/construction and property management; remaining profile/relationship/object-dialog adapters; and active-lot audio/preferences. Each gap is named against its original surface rather than declared complete because a panel exists.

External prerequisites are the operator's deployed service configuration/test account and original resource set: applicable base avatar/object/architecture resources, globals/semiglobals/tuning and sound banks. Those prerequisites are separate from missing code. No live credentials or concrete deployed Wonderland endpoint were supplied during this integration.

## Final verification record

The following results cover the final integrated source and release build. The native test totals were independently summed from all 194 completed Rust test summaries. Worker-focused coverage is linked in the [capability evidence index](player-capability-map.md#evidence-index). The pull request's head commit and tree identify the published revision; its review description records the exact publication IDs.

| Gate | Final result |
| --- | --- |
| Published revision | The pull request's head commit/tree, on `feat/playable-world-integration`, stacked on `feat/character-and-home-ui` / PR #10. |
| `cargo fmt --all -- --check` | **Passed**, exit 0. |
| `git diff --check` | **Passed**, exit 0. |
| Original source preservation | **Passed**, exit 0: `git diff --exit-code 4c6b3e8f5835b228723caea3c9f683c62f244f73 -- TSOClient Other` found no changes. |
| `cargo test --workspace --locked` | **Passed**, exit 0: **1,254 passed, 0 failed, 4 ignored**; no filtered tests. Log: `integration-native-final.log`. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | **Passed**, exit 0. Log: `integration-native-clippy.log`. |
| `cargo clippy -p wonderland-web-shell --target wasm32-unknown-unknown --lib --bin wonderland-web-shell --locked -- -D warnings` | **Passed**, exit 0. Log: `integration-wasm-clippy.log`. |
| `node --test crates/audio-runtime/browser/*.test.mjs` | **Passed: 25/25**, no failures or skipped tests. Log: `audio-node-final.log`. |
| `trunk build --release --locked` in `apps/web-shell` | **Passed**, optimized WASM build and Trunk distribution completed. Log: `integration-browser-build.log`. |
| Final browser: original character creator | **Verified:** original appearance pack import, independent head/body choices, medium skin, Female appearance and rotation; an unsaved Rowan draft was canceled. Captured `01-character-creator.jpg`. |
| Final browser: saved preview roster | **Verified:** sixth Sim Éloïse remains reachable on page 2/2; saved budget §950 remains available. Captured `05-preserved-roster-page-2.jpg`. |
| Final browser: preview map and object actions | **Verified:** Éloïse → Quack's Creek → Harbor Café selection/Visit, then coffee-machine actions with an unavailable Clean reason. Captured `02-interactive-map.jpg` and `04-scene-actions.jpg`. These are preview journeys, not connected city/runtime evidence. |
| Final browser: source lot | **Verified:** floor 1 through 5 boundary controls, camera/reset and actual tile picking. Captured `03-source-lot.jpg`. |
| Final browser: narrow roster | **Verified at 390 × 844 in a desktop Chrome iframe:** roster cards, character stage and actions showed no overlap/clipping; Next reached Éloïse on page 2. Captured `06-narrow-roster.jpg`. This is not mobile-device or all-flow acceptance. |
| Final browser: audio | **Not verified:** audio module import failed with `ERR_BLOCKED_BY_CLIENT`; no playback claim. Node tests establish the adapter's controlled behavior, not successful browser playback. |
| Browser connected account/city/lot journey | **Not verified.** Login UI or controlled native replay alone does not establish a browser login/city/lot journey. |
| Deployed original-server acceptance | **Not performed:** no operator endpoint/account was supplied. This remains distinct from missing code. |

### QA scope and evidence

The automated suite covers source decoding and allocation bounds, real source BHAV execution, queue/cancellation and build admission, gateway handshake/authority/outcomes, session and selection races, geometry/picking, and existing save migration/conflict handling. The controlled gateway replay and example use synthetic source-wire peers; they establish protocol behavior without simulating a deployed city or proving continuous original VM playback.

Four opt-in checks remained ignored: the [Mono avatar source oracle](../../../crates/sim-core/tests/avatar_source_reference.rs), two [original indexed-IFF corpus/version-1 input checks](../../../crates/legacy-formats/tests/indexed_iff_corpus.rs), and the [repository semantic corpus probe](../../../crates/legacy-formats/tests/semantic.rs). Their explicit tool/content requirements and execution instructions remain in those tests. They are excluded from the passing count and are not claimed as completed source-oracle evidence.

The final browser evidence set contains actual captures, with these boundaries:

| Capture | Observed behavior and scope |
| --- | --- |
| `01-character-creator.jpg` | A supplied original pack of about 2.3 MB exposed 20 head and 20 body choices through pagination. Female Head 3 and Body 4 were selected independently with medium skin, and the character was rotated. The unsaved Rowan draft was canceled. This verifies the original-content character stage, not in-world avatar rendering. |
| `02-interactive-map.jpg` | The preserved preview Sim Éloïse selected Harbor Café in Quack's Creek and used Visit. This is the illustrated preview destination flow; it is not a screenshot of connected source-city terrain or live directory results. |
| `03-source-lot.jpg` | Actual original lot geometry was rendered and picked. Floor navigation respected the supplied five-story extent; camera/reset changed the scene; a selected tile exposed its source coordinates. Missing source models remain diagnosed, and this local viewer does not establish a live lot. |
| `04-scene-actions.jpg` | Selecting the coffee machine exposed Make coffee, Inspect and a disabled Clean action with its reason. These are preserved preview action controls, not proof that the original network VM executed an interaction. |
| `05-preserved-roster-page-2.jpg` | The sixth saved Sim, Éloïse, remained accessible on page 2/2 and the saved §950 budget remained available. This demonstrates that visible pagination is not a five-identity account limit. |
| `06-narrow-roster.jpg` | A 390 × 844 iframe in desktop Chrome displayed the three-column roster, six cards including Create, character stage and Play/Change outfit without observed overlap or clipping. Next successfully reached Éloïse on page 2. No mobile hardware emulation or other narrow-screen flow was tested. |

These checks cover the observed desktop release journey and the narrow roster/pagination case only. They do not establish all mobile flows, a connected browser account/city/lot journey, full original resource coverage or continuous source simulation. The source snapshot and asset limitations above still apply.

Rust emitted the dependency's existing `proc-macro-error2` future-compatibility notice while the strict WASM lint and release build still completed successfully. This is distinct from a failed lint/build gate. Browser audio remains the explicit observed exception described in the table.

The [Browser UI workflow](../../../.github/workflows/browser-ui.yml) runs the aggregate Rust/audio/WASM/release gates and retains the preview bundle. PR publication is authorized; merging and deployment are not part of this handoff.
