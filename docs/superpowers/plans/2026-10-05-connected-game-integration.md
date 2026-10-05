# Connected Wonderland Game Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Connect the preserved action-focused browser interface to original account services, source-backed world rendering and real simulation, and implement the remaining player surfaces.

**Architecture:** Keep presentation separate from service and simulation authority. Import compatible A/B/C portable crates at pinned revisions, use explicit account/session/world projections, and preserve the existing preview as a named mode. The original city/lot TCP protocol requires a native gateway; supported HTTP account routes use the original bearer API.

**Tech Stack:** Existing Rust 1.99 / Leptos browser workspace, portable Rust source readers/simulation/renderers, native Rust gateway, original FreeSO service protocols.

**Spec:** `docs/superpowers/specs/2026-10-05-connected-game-integration.md`

## Global Constraints

- Preserve original `TSOClient` and `Other` source and existing preview saves.
- Keep source identities, ownership, account, avatar, shard, lot and incarnation identities distinct.
- No fixture-specific product caps, fake live data or optimistic authoritative writes.
- Credentials and tokens never enter logs, URLs or persistent browser storage.
- Keep the approved action-focused design and independent original appearance choices.
- Import exact A/B/C revisions from the spec; preserve the correction's decoder fixes.
- Browser proof distinguishes a source-backed local world from a live connected lot.
- Publish only to `rndrntwrk/wonderland-`; do not merge or deploy as part of this plan.

## Review Focus

- A roster entry has no live motive/balance data: UI must show unknown values instead of fabricated state; Task 1 parsing and Task 4 projection tests.
- A prior login/request finishes after logout or a new login: stale sessions cannot restore data or apply actions; Task 1 lifecycle and Task 4 adapter tests.
- A crafted XML/world/packet declares huge or inconsistent counts: reject bounded input before allocation or rendering; Tasks 1, 2 and 3 tests.
- An object incarnation, lot revision or authority changes after selection: reject the stale operation without editing a newer world; Tasks 3 and 5 tests.
- A WebGL context is absent or lost: source world must remain visible and picking must match the software depth buffer; Task 2 reference-rendering and browser checks.

---

### Task 1: Original account protocol and native browser gateway

**Files:** Create `crates/game-services/` and `services/browser-gateway/`; tests live with each crate. Do not edit browser screens or root workspace manifests until the root integrates the reported dependencies.

**Interfaces:**
- Produces DOM-free `wonderland_game_services` account/session types and original HTTP/XML parsers, with lossless source IDs and explicit unknown live values.
- Produces native `wonderland-browser-gateway` configuration, health/session endpoints and the authenticated native protocol connection needed by a browser.
- Consumes source routes and packet semantics from the original server/client; do not define pretend legacy endpoints.

- [ ] Read the exact source protocol audit and original code before choosing request/response fields.
- [ ] Write failing tests for HTTP-200 OAuth errors, 64-bit outfit IDs, missing motives, malformed XML, stale session responses, arbitrary socket destination rejection and ticket/session isolation.
- [ ] Implement original account route builders/parsers and a session state machine; expose bounded errors without credential contents.
- [ ] Implement the gateway's real native protocol transport and usable operations from original packet definitions. Clearly enumerate any operation blocked by a missing server/provider.
- [ ] Run crate tests and source-compatible request/response integration tests, then commit this owned file set.

### Task 2: Source lot document, real geometry and browser renderer

**Files:** Import `crates/render-3d/`; create `crates/world-view/`, `apps/web-shell/src/world_renderer.rs`, source-lot public assets and tests. Root integrates manifests/module declarations and mounts the component.

**Interfaces:**
- Produces `wonderland_world_view` with a versioned normalized source world document, source blueprint parser, geometry/camera/picking adapter and validation.
- Produces a browser `WorldViewport` component that consumes a normalized world document and emits typed picks; document its exact public signature for shell integration.
- Consumes existing `wonderland-render-core` reference z-buffer/ID buffer and source `render-3d::lot::build_lot`.

- [ ] Read the renderer audit, source blueprint and source geometry contract.
- [ ] Write failing tests for original lot dimensions/floors, invalid coordinates/counts, floor/wall visibility, camera transform and depth-correct object/tile picks.
- [ ] Import portable C geometry while preserving the current render-core/avatar implementation.
- [ ] Implement source XML parsing and normalized live-world input with source provenance and lossless IDs.
- [ ] Implement the real browser scene, camera/picking and software fallback; use actual source assets and explicit missing-resource state.
- [ ] Run native geometry/renderer tests, WASM compile and browser checks after mounting; commit owned files.

### Task 3: Actual A/B simulation and content integration

**Files:** Import `crates/sim-core/`, `crates/content-ir/`, `crates/content-runtime-bridge/`, necessary tools/tests and B interactions. Create `crates/game-runtime/` for the integration adapter. Preserve the current legacy reader fixes; root resolves shared manifests and lock.

**Interfaces:**
- Consumes the exact A `SimRuntime` and B original-content bridge pinned in the spec.
- Produces a DOM-free local/authority runtime adapter with actual import, accepted ticks, interaction queries/queue actions, state/event projections and normalized world output.
- Produces the complete query/check and queue seam needed by B providers; source advertisement/action variants must not be silently lost.

- [ ] Write failing tests against original routines for detached query purity, available action variants, stale generations, actual accepted interaction execution and cancellation.
- [ ] Assemble compatible source crates and add only the missing B semantic additions to current hardened readers.
- [ ] Extend the real A runtime seam to support B checks and queues, preserving in-tick versus detached semantics.
- [ ] Project live needs/queue/object/architecture state, with unsupported source behavior explicit.
- [ ] Run focused original-content tests, native/WASM compatibility and meaningful cross-crate integration tests; commit owned files.

### Task 4: Connected account and complete player panels

**Files:** Browser session adapter, account/city entry screens, `components/player_menu.rs`, contextual player/service panels, service reducer tests. Root coordinates changes to `app.rs`, `bridge.rs` and authoring transport.

**Interfaces:**
- Consumes Task 1 account/session/service types and original HTTP/gateway operations.
- Receives live runtime projections from Task 3; does not synthesize motives, identity or authority from preview data.
- Produces usable login/roster/shard/admission/reconnect plus profile, people, bookmarks, chat/inbox, property, neighborhood and EOD views with actual responses and controls.

- [ ] Test stale replies after logout, lost sessions, rejected actions retaining drafts and unread/selection state.
- [ ] Implement configured connected mode and keep preview saves scoped to preview mode.
- [ ] Implement each capability-map surface with source data and complete loading/empty/permission/pending/rejection behavior.
- [ ] Verify keyboard/touch/focus flow and compact overlays in the running browser; commit owned files.

### Task 5: Construction, wardrobe, purchases and settings connections

**Files:** Source-driven authoring/runtime adapters, Home/lot controls, settings and audio adapter; behavioral tests adjacent to the adapters.

**Interfaces:**
- Consumes Task 2 canonical camera/picking, Task 3 authority and Task 1 owned/EOD service sessions.
- Extends existing authoring state only where needed to preserve separate lot and owner identities.

- [ ] Write rejection/ownership/revision tests for owned wardrobe, purchases, placement and each architecture operation family.
- [ ] Connect actual wardrobe EOD ownership/default/delete semantics and original catalog/quote data.
- [ ] Implement direct scene construction using actual source architecture commands and accepted/rejected state.
- [ ] Connect original graphics/camera/floor/wall/audio preferences to implemented adapters, including user-gesture audio activation and reduced motion.
- [ ] Verify saved-data migration, overlapping sessions, costs/refunds and original object ownership; commit changes.

### Task 6: Integrated verification and publication

**Files:** Workspace manifests/lock, reproducible development and gateway configuration, CI, capability evidence and PR descriptions.

**Interfaces:** Consumes all task reports and exact commit ranges. Produces one runnable integration and a row-by-row capability status with evidence.

- [ ] Run formatting, workspace tests, native/WASM lint and release browser build on the assembled code.
- [ ] Run real browser account/renderer/world/authoring/service journeys with supplied local source resources and controlled gateways; distinguish fixtures from live services.
- [ ] Independently review source preservation, credential/session boundaries, original ID/geometry fidelity and authoritative operations.
- [ ] Fix all material findings and verify the complete affected flow.
- [ ] Publish reviewable PR increments to the Wonderland fork, preserving existing open work and reporting any concrete external connection prerequisite.
