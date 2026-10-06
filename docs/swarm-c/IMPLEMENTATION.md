# Swarm C Views and Audio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Follow the assigned file ownership and record meaningful red/green evidence.

**Goal:** Implement Swarm C's W07–W10 and SL.5 presentation/audio work with
source-grounded algorithms, executable fixtures, engine adapters, and a precise
acceptance ledger.

**Architecture:** Independently buildable presentation libraries consume
immutable C-local projections and normalized content. A common render-core owns
math, validation, meshes/images, picking and resource lifetime. Engine/browser
adapters and the representative comparison have isolated newer dependencies.

**Tech Stack:** Rust 1.75 core libraries; serde 1.0.195, bincode 1.3.3 and sha2
0.10.8 where needed; engine gate Rust 1.95.0, Bevy 0.19.1, Fyrox 1.0.1;
Node/browser adapters, original C# source probes, optional external ffmpeg tools.

**Spec:** [SPEC.md](SPEC.md). Exact common API: [CONTRACT.md](CONTRACT.md).

## Global constraints

- Source baseline `4c6b3e8f5835b228723caea3c9f683c62f244f73`.
- Preserve 30 Hz authoritative simulation; render/audio cadence cannot change it.
- Full2D, Hybrid2D, Full3D and WebGPU/WebGL2 are separate coverage dimensions.
- No renderer dependencies in sim-core; no shared schema/workspace edits.
- Synthetic fixture evidence is distinct from licensed content and physical GPUs.
- All limits/unsupported paths and intentional legacy deviations are explicit.
- One writer per file; root owns the shared contract and publication. Parallel
  library work is disjoint. Agents do not commit or alter other lanes.

## Review focus

1. Malformed dimensions, hierarchy/index ranges, NaN/Infinity, and allocation
   overflow must reject before upload or partial frame/asset replacement (T1–T5).
2. An asynchronous pick/cue for an old lot, reused entity, changed visual target,
   or old decode/device generation must not affect the new view (T1, T5, T6).
3. Source coordinate conversion, depth/alpha and dual-local-position skinning
   must happen once; source analytic fixtures catch sign/order errors (T2–T4).
4. 30/60/120 Hz drawing, mute, suspension, and repeated accepted ticks must not
   create new authoritative events or duplicate playback (T4–T6).
5. Repeated outfit/lot/device changes must release bounded resources, preserve
   selection/camera intent when valid, and reject stale async completions (T1–T6).

## Task 1: Common presentation foundation

**Files:** `crates/render-core/{Cargo.toml,Cargo.lock,.gitignore,src/,tests/}`;
`docs/swarm-c/core-source-notes.md`.

**Consumes:** CONTRACT.md v1 and immutable inputs. **Produces:** the exact common
math/data APIs; validated frame store; generation-aware picking; bounded cache
and thumbnail identity; CPU reference surface/triangle rasterization.

- [ ] Add analytic tests for transforms/inverses, quaternion shortest-path
  interpolation, right-handed clip depth, scaled ray boxes, invalid finite data.
- [ ] Add tests for atomic frame rejection and stale/current pick tickets,
  byte-accounted LRU/pin/reset behavior, transparency/depth/ID-buffer overlap.
- [ ] Observe the missing behavior, implement the common API, and run complete
  package tests and rustfmt. Keep rasterization an explicit reference path.
- [ ] Record source units, input limits, tests, and reviewable compatibility choices.

## Task 2: Isometric/hybrid pipeline (W07.2–W07.4)

**Files:** `crates/render-iso/{Cargo.toml,Cargo.lock,.gitignore,src/,tests/}`;
`docs/swarm-c/iso-source-notes.md`; C-only shader/fixture files within this crate.

**Consumes:** core math/images/meshes and normalized DGRP/sprites. **Produces:**
rotation/zoom projection, explicit sprite-depth and lighting/material output,
stable batches, floor/cutaway masks and view-transition state.

- [ ] Pin source literals for DGRP offsets, three zooms/four rotations, padded UVs,
  fallback/missing depth, alpha thresholds, lighting, and multi-tile occlusion.
- [ ] Test dynamic layers, mirrored sprites, no stale depth binding, stable ties,
  hidden floors, and camera/selection continuity through all view modes.
- [ ] Implement pure CPU/reference and shader-facing outputs. Run package tests.
- [ ] Write source notes including source sampler/blend/transition discrepancies;
  do not claim GPU parity from CPU assertions.

## Task 3: 3D lot/city/environment pipeline (W08 and SL.5)

**Files:** `crates/render-3d/{Cargo.toml,Cargo.lock,.gitignore,src/,tests/}`;
`docs/swarm-c/geometry-source-notes.md`; `tools/facade-worker/` if needed.

**Consumes:** core math/meshes/images and explicit visual lot/city inputs.
**Produces:** terrain/wall/floor/roof/pool meshes, reconstructed/override object
selection, city geometry/destinations/transitions, cameras/environment/quality.

- [ ] Test literal slopes/story units, both diagonal floor halves, wall/roof
  winding/normals, pool adjacency, cutaways and bounded dirty-region rebuilds.
- [ ] Test reconstruction discontinuities/alpha, content+parameter identity,
  override precedence, changed patches, empty/corrupt meshes and contact bounds.
- [ ] Test exact city colors/elevation/road masks, map bounds, stale destination
  transitions, returning camera intent, camera collision policy and quality tiers.
- [ ] Implement and run complete package tests. Record exact missing visual inputs
  from A/B and fixture-only directory boundaries. Produce deterministic facades.

## Task 4: Avatar visual pipeline (W09)

**Files:** `crates/avatar-view/{Cargo.toml,Cargo.lock,.gitignore,src/,tests/}`;
`docs/swarm-c/avatar-source-notes.md`; C-only avatar cook/preview tool paths.

**Consumes:** core math/meshes and B-compatible normalized rig/appearance data;
explicit simulation timeline/visual contact inputs. **Produces:** validated rig
binding, source-correct skinning/pose outputs, attachments/contact/look and LOD.

- [ ] Test row/column conversion once, case-insensitive bone matching, hierarchy
  cycles/missing parents, dual-local blended vertices and source normal policy.
- [ ] Test interpolation, loop/reverse/hurry samples and event-free pose sampling
  at 30/60/120 Hz; attachment/contact changes and departed participants.
- [ ] Test atomic outfit replacement, preview bounds, repeated asset release and
  deterministic 32/64-avatar culling/LOD fixtures.
- [ ] Implement, run complete package tests, and document the exact A/B adapter
  boundaries including missing continuous route/slot and TS1 rig providers.

## Task 5: HIT and playback pipeline (W10)

**Files:** `crates/audio-runtime/{Cargo.toml,Cargo.lock,.gitignore,src/,tests/}`;
`tools/swarm-c/audio-cooker/`; `docs/swarm-c/audio-source-notes.md`.

**Consumes:** core cue identities, B-compatible HIT/event/track/sample metadata
and raw code bytes; an explicit sample/backend capability provider.
**Produces:** full opcode status census, bounded HIT/FSC execution, ordered mixer
intents, replay-safe cues, station/ambience lifecycle, native/browser adapters.

- [ ] Add source vectors for actual opcode/control flow/register/wait/random
  behavior and source stubs. Test malformed code, stack limits, and runaway loops.
- [ ] Test repeated tick/cue suppression, stale epoch/entity/decode completions,
  stop/interruption/volume group handling, source station fade and FSC ordering.
- [ ] Implement gesture/suspend/resume and bounded decode/voice ownership; test
  real adapter logic with controlled backend boundaries. Playback stays cosmetic.
- [ ] Implement explicit WAV/codec cooking with bounded subprocess/error paths,
  preserve metadata/provenance, and record unsupported codecs without false success.
- [ ] Run complete Rust and adapter tests; write source census and provenance notes.

## Task 6: Representative engine, integration, and publication (W07.1)

**Files:** `probes/engine-bakeoff/`; `tools/swarm-c/`; C-only fixtures under
`fixtures/render/swarm-c/`; `docs/swarm-c/{README,COVERAGE,HANDOFF,VERIFICATION}.md`;
`docs/decisions/engine.md`; an isolated C-scoped CI workflow if required.

**Consumes:** all five libraries and one shared synthetic scene/cue fixture.
**Produces:** native reference output; Bevy and Fyrox adapters; independent Bevy
WebGPU/WebGL2 build commands; browser capability/input/audio lifecycle harness;
recorded comparison and exact remaining acceptance ledger for 17 work items.

- [ ] Assemble one same-input scene with transparent/depth sprites, architecture,
  animated rig, picking, city/lot transition and simulation-stamped audio cues.
- [ ] Run native/WASM algorithm comparisons and engine builds where the actual
  toolchain/runner exists. Pin exact dependencies and report unavailable gates.
- [ ] Exercise real browser backend startup/fallback, focus, resize, suspend and
  device loss on available runners; label headless and physical evidence separately.
- [ ] Obtain independent package and cross-lane review; fix concrete findings.
- [ ] Run final package suites, static checks and applicable build/browser gates.
  Publish verified files to the feature branch and draft PR; verify remote tree.

## Acceptance accounting

Implementation progress and provider integration are recorded separately from
source parity, GPU backend, physical-device, content-cohort and performance
qualification. No source no-op, request adapter, synthetic fixture, or unavailable
runner can be counted as a fully qualified gameplay/device capability.
