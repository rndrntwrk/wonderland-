# SDD ledger — plan: docs/superpowers/plans/2026-10-05-connected-game-integration.md

Base: 7ee14583ca10ed86511d717566295ea44d481bf7. Plan/spec commit: bcd0ce1.
User authority: complete all remaining work; latest instruction “proceed to completion”. Existing scope/design approval and PR authorization retained. No merge/deployment.

## Preflight
| Task pair | Shared interface | Finding |
|---|---|---|
| 1 / 4 | Original account/session/protocol types | Task1 publishes signatures before Task4 browser consumption; root coordinates service configuration. |
| 2 / 3 | Normalized source world document | Task2 owns DTO and renderer; Task3 exposes actual runtime world state; conversion is assembled by root after signatures. |
| 2 / 5 | Scene camera/picking/geometry | Task5 consumes renderer picks rather than independent DOM projection. |
| 3 / 5 | Accepted operations and world authority | Build/purchases require true authority, not fake receipts. Source gaps must be implemented or explicitly documented. |
| 1–5 / 6 | Root Cargo manifests, module registration and browser mount | Root owns shared manifests, lock and mount; workers own nonoverlapping new paths. |
| 1 | Task self-consistency | Read actual legacy source before packet implementations; HTTP and TCP admitted separately. |
| 2 | Task self-consistency | Source blueprint exists; source lot may be empty; cannot invent furnished source house. |
| 3 | Task self-consistency | Missing query/queue seam is real work; exact serde pins must be reconciled with existing browser dependencies. |
| 4 | Task self-consistency | Actual account roster has no live HUD values; distinct projection needed. |
| 5 | Task self-consistency | Wardrobe requires active owned EOD session; build source commands and authority are necessary. |
| 6 | Task self-consistency | Local fixture proof and live server proof remain separate. |

Ruling: Continue implementation under the user's repeated explicit completion instruction; do not introduce another approval stop for the continuation plan — this preserves their authorized direction. Cost if wrong: reviewable changes remain on a feature branch.
Ruling: Run independent implementation work concurrently on disjoint owned paths, as required by the active parallel-delegation instructions; serialize root manifests and integration edits. Cost if wrong: integration conflicts are caught before publication.
Ruling: Use existing C source renderer plus software depth/ID buffers, not a new engine migration — compatible with the verified browser fallback and source geometry. Cost if wrong: renderer performance may require an additional GPU backend without changing authoritative state.

## Work

1. Original services and gateway — implemented in `6840f84`. Source review G1–G6 closed. Fresh aggregate native run includes all 49 service/gateway tests and example compilation; scoped strict lint passed.
2. Source city and world — city `2141471`, renderer `7789f3d`, resource limits `194b70e`. Original baseline remains unchanged. The world source review closes memory, picking and stale-view findings. Later source-preserving presentation lint changes are recorded in `32dab57`.
3. Actual simulation/content library — `460f5f9`, `2623075`; strict-lint integration delta `8c27035`. Original BHAV behavior, actions/queues and confirmed construction are exercised by the aggregate tests. This library is not a restored original-server VM.
4. Connected player surfaces — `b3677e3`, transport budgets `ca1cc8b`, direct source-home admission `7dce6f4`. Preserved preview mode, account/city/player panels, identity and reply fences are integrated by root.
5. Source authoring — `a67c3d3`, stale-world/build/rack recovery `f20ea38`, new-lot presentation fences `c89dd4d`. Source review W1–W6 closed. Root connects camera-projected drafts and freshness-checked queue cancellation.
6. Root assembly — startup configuration, source-world screen, source-frame admission, snapshot HUD/world and manifest/CI registration implemented. Final release build, strict lint and native/browser verification passed within the boundaries below. The final source commit and authorized PR publication remain.

## Verification checkpoint

- Final native workspace rerun after all style/boxing cleanup exited 0: 1,254 passed, 0 failed, 4 opt-in source-oracle/corpus cases ignored across 194 summaries. The final log retains one earlier cached creator diagnostic; the completed process and test summaries establish this gate, not an error-free log claim.
- Protocol resource-limit fixes `4913da5`; compact decoded variants `4f1cfdc`; all 27 focused tests and strict lint passed after boxing. Source review I1/M1 closed; synthetic fixture interoperability limitation remains explicit.
- Aggregate native strict Clippy passed, including all targets and the controlled replay example.
- WASM web-shell strict Clippy passed after root browser syntax/style cleanup. The dependency proc-macro-error2 emits Cargo's future-compatibility notice; it is not a current strict-lint failure.
- Browser audio JavaScript tests: 25 passed.
- Original `TSOClient` and `Other`: zero diff against `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Final `trunk build --release --locked` exited 0 at 2026-10-05T20:34:27Z. Final formatting and `git diff --check` exited 0.
- Final rebuilt browser verified original asset loading, 20 head and 20 body choices in the bounded test pack, skin/gender selection and avatar rotation; the unsaved test draft was canceled. Screenshot `01-character-creator.jpg`.
- Preview preservation: Éloïse remains on page 2/2 with budget 950; map destination selection and coffee-machine contextual actions remain usable. Screenshots `02-interactive-map.jpg`, `04-scene-actions.jpg`, `05-preserved-roster-page-2.jpg`. These are local preview actions, not online game receipts.
- Original source lot: floor 1/down disabled, floor 5/up disabled, reset/camera controls, and actual tile 38,37 picking verified. Screenshot `03-source-lot.jpg`; unresolved source object models remain disclosed.
- Narrow 390×844 iframe check verified character layout and page-2 navigation, captured in `06-narrow-roster.jpg`. It is not mobile-device or all-screen responsive acceptance.
- Browser audio dynamic import was blocked by the browser client (`ERR_BLOCKED_BY_CLIENT`). The Node bridge tests passed, but actual browser playback is unverified. Browser connected sign-in/city/lot and deployed original-service acceptance were not established.

## Environment recovery

A stale generated-artifact relocation left current Cargo fingerprints referring to old simulation rlib/rmeta bytes. Rust metadata diagnostics proved the hash mismatch. Root removed only the affected generated simulation/bridge/runtime artifacts and fingerprints and rebuilt with one coordinated gate; the native suite then passed. No source change was made to hide this failure. Finished test/check caches and reproducible download containers were reclaimed to fit the shared disk.

## Remaining product boundary

The 21-surface map and connected-integration handoff distinguish implemented preview/source/connected snapshot capabilities, unfinished original-client adapters, and external service/content prerequisites. No merge, deployment, fabricated live acceptance, or full original-client replacement is authorized or claimed. Source views do not overwrite saved Homes, and all existing local Sims remain reachable.
