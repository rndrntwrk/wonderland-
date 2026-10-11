# Action-focused browser UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the first Wonderland Rust browser UI PR with character selection, direct map travel, and object-anchored interaction controls.

**Architecture:** A renderer-independent contract and pure state reducer drive a Leptos CSR shell. A clearly labeled deterministic fixture adapter allows the UI to integrate before the simulation and production renderer. Scene artwork and selectable object assets compose an illustrated presentation adapter that can later be replaced without changing UI intent semantics.

**Tech Stack:** Rust 1.99.0 (edition 2024), Leptos 0.8.21 CSR, serde, wasm-bindgen/web-sys, Trunk, HTML/CSS, vendored artwork/font/icon assets.

**Spec:** `docs/superpowers/specs/2026-10-05-action-first-browser-ui.md`

## Global Constraints

- Target only `rndrntwrk/wonderland-`, feature branch `feat/action-first-browser-ui`, baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Rust 1.99.0, edition 2024; stable Leptos 0.8.21 CSR with Trunk and `wasm32-unknown-unknown`.
- Browser gameplay and state are authored in Rust. HTML/CSS, generated bindings, asset data, and development/test scripts are allowed. Do not create a React replacement.
- No legacy C# behavior changes, auth/server integration, live transactions, or production renderer in this PR.
- Keep shared IDs and UI projection/intents in `crates/contracts`; no engine, DOM, or transport dependencies there.
- All fixture success is visibly identified by a small `UI preview` label and explained in the README. Never describe illustrated artwork as working 3D gameplay.
- Reproduce the selected game references with real art and semantic controls. No form/dashboard substitutes or mockup-as-background shortcuts.
- All new assets have repository-local provenance and license information. Keep binary outputs, tool caches, and local review records out of git.

## Review Focus

- Rapid repeated input and out-of-order replies must not duplicate actions or acknowledge the wrong operation; owned by Task 1 tests.
- Changed object generation/revision or a removed selection must invalidate offers and in-flight work; owned by Task 1 tests.
- Back navigation during travel must not allow a late reply to pull the user into a different screen; owned by Task 1 tests.
- Pan/zoom and narrow resizing must preserve the map/object anchor relationship and reachable controls; owned by Task 2 browser verification.
- Keyboard-only and reduced-motion users must be able to select, act, dismiss, and return focus without an invisible focus trap; owned by Task 2 browser verification.

## File ownership

- Task 1: workspace manifests/lockfile/toolchain, contracts, native client state, JSON fixtures, contract documentation, root gitignore additions.
- Task 2: web-shell app and dependencies, dev/build tooling, CI, UI README, browser behavior evidence. Task 2 may extend only web-shell dependency entries in the root workspace/lockfile and calibrate the JSON fixture's presentation coordinates, populations, and place availability to reviewed art.
- Asset workers: generated art only under assigned temporary asset output paths; integrator imports reviewed assets into `apps/web-shell/public/assets/` with provenance. No asset worker edits code or runs a browser.
- Integrator: this spec/plan, asset inventory, ledger, screenshots, design QA, final PR. All product code and code fixes go through the task implementer.

### Task 1: Typed presentation contracts and safe UI transitions

**Files:**

- Create: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`.
- Create: `crates/contracts/Cargo.toml`, `crates/contracts/src/lib.rs`, focused contract modules as needed.
- Create: `crates/client-app/Cargo.toml`, `crates/client-app/src/lib.rs`, focused state/fixture modules as needed.
- Create: `fixtures/ui/preview-v1.json`, `docs/contracts/ui-projection-v1.md`.
- Modify: `.gitignore` only for new Rust/web build artifacts.
- Test: `crates/client-app/tests/ui_flow.rs`, `crates/contracts/tests/fixture.rs`.

**Interfaces:**

- Produces crate `wonderland-contracts` with `UiProjection`, `UiIntent`, `UiRequest`, `UiEvent`, `UiError`, stable typed string IDs, generation-aware `EntityRef`, availability, offer, needs, queue, and request status types.
- Produces crate `wonderland-client-app` with `ShellState::new(projection: UiProjection) -> Self`, `ShellState::dispatch(intent: UiIntent) -> Result<Vec<UiRequest>, UiError>`, `ShellState::receive(event: UiEvent) -> Result<(), UiError>`, and `preview_projection() -> UiProjection`.
- `ShellState` exposes readable screen, selection, pending request, acknowledged queue, and projection state. UI intents include selecting a character/place/object, playing, visiting, taking an offer, cancelling, and going back. Replies preserve operation identity and distinguish request acceptance/rejection and cancellation acknowledgment.
- The implementer defines the exact internal enum variants and serializable fields, documents them, and reports the stable exported API for the next task. No network wire compatibility is implied.

- [ ] **Step 1: Write native behavioral tests before implementation.** Cover five playable characters; Play without selection; valid Play then city selection; unavailable travel; a submitted travel remaining pending until the matching reply; rejection remaining in city; duplicate acceptance idempotence; Back invalidating a pending travel; stale generation/revision action rejection; unoffered/disabled actions; duplicate action suppression; accepted queue visibility; cancellation waiting for its own reply; unrelated replies ignored; and exactly Energy/Hunger/Fun/Social/Hygiene/Bladder/Comfort/Room in fixtures. Use real serialized fixture round-trip tests, not source text assertions.
- [ ] **Step 2: Run `cargo test --workspace` and record an expected failing behavioral assertion or missing API before writing the implementation.** Toolchain setup may precede the test run. Do not report a network failure as the red phase.
- [ ] **Step 3: Implement the minimal contracts, bounded projection validation, reducer, and deterministic fixture helpers.** Keep state transition code DOM-free. Use request sequence/operation identity to invalidate stale work. Avoid actor authority in UI eligibility and document the external adapter boundary.
- [ ] **Step 4: Run `cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets -- -D warnings`.** Record meaningful passing evidence. If a target does not exist yet, test the two created packages explicitly and report that scope.
- [ ] **Step 5: Commit only owned files with message `feat(ui): add presentation contracts and request state`.** Write the report with exported API, commands/results, data limits, and known constraints.

### Task 2: Leptos game shell, integrated fixtures, and browser-ready packaging

**Files:**

- Create: `apps/web-shell/Cargo.toml`, `apps/web-shell/Trunk.toml`, `apps/web-shell/index.html`, `apps/web-shell/src/main.rs`.
- Create: `apps/web-shell/src/{app,bridge}.rs`, `apps/web-shell/src/screens/{avatars,city,lot}.rs`, `apps/web-shell/src/components/` with focused components.
- Create: `apps/web-shell/public/styles.css`, supporting focused CSS files if useful, `apps/web-shell/package.json`, `apps/web-shell/scripts/`.
- Consume reviewed assets: `apps/web-shell/public/assets/` and `ASSETS.md`.
- Create: `apps/web-shell/README.md`, `.github/workflows/browser-ui.yml`.
- Create: `tests/ui/mobile-viewport.html`, a test-only same-origin 390-by-844 iframe harness, kept outside production build inputs.
- Create: `tests/ui/prepare-browser-qa.py` and root `design-qa.md`, owned by the integrator for reproducible source/live comparisons and the final evidence record.
- Modify: root workspace membership and lockfile only for shell dependencies.
- Modify: `fixtures/ui/preview-v1.json` only to align anchors with the reviewed art and declare which destinations have a preview scene.
- Verify: actual browser preview and focused native tests for any new non-DOM logic. Integrator owns final screenshot/design report files.

**Interfaces:**

- Consumes the Task 1 public contracts, `ShellState` API, and `preview_projection()`; read its contract doc and report before coding. Do not recreate shared enums in the shell.
- Consumes the reviewed asset manifest and approved reference images identified by the spec. Runtime art paths are in `apps/web-shell/public/assets/ASSETS.md` when ready.
- Produces a static WASM browser build and a development command `npm run dev -- --host 0.0.0.0 --port 4173 --strictPort` that serves this Leptos app; Node is tooling only.
- Fixture bridge applies matching delayed replies and renders pending/accepted/rejected/cancelling states. It cancels or safely ignores timers after navigation/unmount. Scene display and pick/anchor calculation are separate from the contracts and state reducer.
- Only Harbor Café has a lot fixture in this increment. Other map destinations remain directly selectable but are unavailable to Visit with a concise preview reason; do not present the café interior as another destination. Set café population to `6`, matching the approved reference, and calibrate all map/object anchors to the reviewed assets in the visual brief.

- [ ] **Step 1: Read the exact visual references, measure grid/stage/world/HUD proportions, and implement semantic Leptos screen structure using the shared state.** Keep unavailable secondary controls explicit. The primary journey is character card → Play → map place → Visit acknowledgment → coffee machine → action acknowledgment → cancel → city. All needs opens and closes with correct focus handling.
- [ ] **Step 2: Add consistent rounded blue/green game chrome, font/icons, and reviewed art in every assigned position.** Implement responsive layout and real pan/zoom/reset for the illustrated map/lot adapter; selectable object coordinates and their menu must share the transform. Do not simulate 3D orbit with a flat image transform or use a full mockup screenshot as a page.
- [ ] **Step 3: Implement accessible behavior.** Buttons have names and selected state, menu uses ordinary actionable buttons, Escape dismisses and returns focus, live announcements are concise, all eight needs have labels/values, and reduced-motion is honored. Disabled states expose an explanation. Ensure the object menu remains on screen near edges.
- [ ] **Step 4: Package reproducible commands, focused CI, asset provenance references, and README limits.** Include actual build and test commands, adapter replacement instructions, and the next 3D/simulation integration tasks. Keep toolchain downloads in local caches, never committed binaries.
- [ ] **Step 5: Build the wasm32 target with Trunk, run the native suite and formatting/clippy for applicable native/WASM packages, and start the preview.** Record exact commands and build evidence. Browser verification is performed by the integrator against this running app. Fix reported integration or material visual defects, rerun only covering checks, and leave the preview running.
- [ ] **Step 6: Commit owned files with message `feat(ui): implement action-focused browser game shell`.** Write the report with routes, interaction outcomes, commands/results, and scope limitations.
