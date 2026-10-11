# Swarm C — views and audio specification

## Purpose and scope

Implement the third swarm in the supplied FreeSO rewrite plan: C, covering
W07–W10 and SL.5. The outcome is a Rust presentation pipeline for the existing
game's isometric, hybrid, full-3D, city, avatar, and audio behavior, with runnable
fixtures and explicit browser/native integration. Source baseline:
`4c6b3e8f5835b228723caea3c9f683c62f244f73` in `rndrntwrk/wonderland-`.

The user authorized taking over this existing planned workstream. The branch is
`feat/swarm-c-views-audio`. The implementation is additive to the source baseline
and can be reviewed independently of the pending A/B branches.

## Boundaries

- Preserve Full2D, Hybrid2D, and Full3D as separate view modes. A graphics backend
  is a different dimension from a view mode.
- Preserve 30 simulation ticks per second. Rendering and audio device clocks
  must never advance the VM, consume its RNG, or emit authoritative events.
- Simulation/game identities remain independent of engine entity/asset handles.
- Read immutable, versioned presentation inputs. Private EOD state, database
  handles, and ledger authority are outside these inputs.
- Maintain explicit world units: lot tile `(x,y,z)` maps to graphics
  `(3*x,3*z,3*y)`. DGRP and SLOT content offsets use their own named conversions.
- Presentation caches are disposable, bounded, keyed by effective content and
  derivation parameters, and invalidated after content/device/lot changes.
- Consume B's normalized straight-RGBA sprites, raw depth bytes, source IDs,
  f32-preserving rig/animation metadata, and audio metadata through adapters.
  Do not negate coordinates a second time after B's `FreeSo` conversion.
- Do not redistribute original game assets. Synthetic fixtures are labeled;
  source excerpts/reference probes remain outside shipping crates.
- Global contracts and workspace membership remain F's integration responsibility.
  C-local contracts are versioned and documented, and are not declared to be an
  adopted global schema.

## Design

Use six independent presentation crates: `render-core`, `render-iso`,
`render-3d`, `avatar-view`, `audio-runtime`, and an isolated integration/probe
package. Common math, identities, CPU mesh/image data, validated frames, cache
lifetime, picking, and reference rasterization live in `render-core`.
The other four libraries depend downwards on that package, never on each other
or the simulation. An integration adapter assembles their outputs for an engine.

The dependency-light libraries retain Rust 1.75 compatibility so the checked
simulation/reference environment can execute their tests natively and on WASM.
Engine adapters use a separately pinned Rust 1.95.0 toolchain, Bevy 0.19.1, and
Fyrox 1.0.1. The current Bevy release declares Rust 1.95; changing a simulation
toolchain is not necessary to test a presentation engine.

Bevy remains the supplied plan's lead candidate, pending the representative
comparison. Its WebGPU and WebGL2 binaries must be built separately because
the release's `webgpu` feature overrides `webgl2`. Fyrox is evaluated on its
documented WebGL2 path. No engine is reported as qualified before the actual
comparison passes. CPU image fixtures are a reference for geometry, depth,
alpha, picking, and transforms, not a GPU benchmark.

## Required behavior

### Isometric and hybrid

Preserve rotated DGRP matching and sprite offsets, logical image versus padded
texture dimensions, palette/straight-alpha inputs, source depth conversion,
lighting transfer, stable batching, cutaways, floor visibility, and preserved
camera/selection intent through view changes. Picking is generation-aware and
checks current visual identity after asynchronous readback. A missing depth
resource never inherits another sprite's bound sampler.

### 3D and city

Generate terrain, floor halves, walls, roofs, and pool geometry from explicit
semantic/visual lot inputs. Preserve source units, normals, materials, and
contact alignment. Reconstruction has explicit authored/user override,
embedded, and regenerated-cache precedence and hashes effective content plus
parameters. Visual meshes do not redefine simulation collision.

City terrain uses exact source RGB classifications and pixel coordinates;
packed map positions are not persistent destination IDs. City-to-lot transition
state retains the camera and selected live destination, handles stale responses,
and has a fixture directory adapter until E's real directory is integrated.

### Avatars

Validate rig hierarchy and bind ranges, match mesh bone names correctly, retain
the source's two bone-local positions for blend vertices, and expose an explicit
normal policy. Pose interpolation consumes simulation timeline positions without
creating gameplay events. Attachments, contact, head look, outfit replacement,
LOD, and previews have bounded resource ownership and interruption behavior.

### Audio

Port the actual HIT registry with a complete status census, bounded execution,
explicit source quirks, ordered effects, and presentation-only random state.
Map accepted game/animation cues to stable identities and suppress replayed
events. Preserve source volume groups, station/FSC behavior, interruption and
stop semantics. Browser playback requires explicit gesture unlock, bounded
decoding/voices, and clean suspend/resume/lot reset. Native playback and cooking
use explicit backend/codec capabilities; unsupported UTK or missing licensed
payloads must not become a reported successful decode.

## Acceptance and limits

Each package needs analytic fixtures and adversarial validation tests. Rendering
also needs same-scene image, mesh, and ID-buffer evidence; avatars need 30/60/120
Hz pose sampling and 32/64-actor lifecycle fixtures; audio needs opcode/control
flow vectors and live adapter lifecycle tests. Native/WASM comparisons cover
the dependency-light algorithms separately from engine/backend browser runs.

The engine gate must record exact versions, features, dependency locks, artifacts,
device/backend, CPU/GPU timing where measurable, memory, DOM focus, interruption,
and device loss. Headless browser evidence is labeled separately from physical
iPhone/iPad Safari, Android Chrome, and desktop device acceptance. Missing
providers or physical devices remain named gates; fixture success cannot close
real-content, service, or physical-device acceptance.

## Source and provider references

- Original plan inputs: `FreeSO-Rust-Browser-Rewrite-Plan.md`,
  `FreeSO-Parallel-Work-Packages.md`, `FreeSO-Source-Inventory.md` supplied by user.
- A simulation handoff: PR #6, commit
  `8a0e251d19e222a0a6833d7408ca629f674e1729`.
- B content metadata inspected at commit
  `2c189402160f8c80ef1fc8f0f58ac004350c1863`.
- Detailed per-module source anchors and compatibility decisions accompany the
  implementation in the source notes and coverage ledger.
