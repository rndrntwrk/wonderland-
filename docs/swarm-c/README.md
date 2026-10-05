# Swarm C — views and audio

Swarm C implements the presentation work described by W07–W10 and SL.5: isometric
and hybrid drawing, 3D lot/city geometry, avatar visuals, audio, and a representative
Bevy/Fyrox comparison. The implementation is additive to source baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73` and is reviewed in
[PR #12](https://github.com/rndrntwrk/wonderland-/pull/12).

**The implementation and its acceptance evidence are separate.** Both engines
have completed native software-renderer startup, and all three browser variants
have compiled and packaged in the recorded CI run. Browser image parity,
production provider integration, real-content coverage, and physical-device
acceptance remain open. See the commit-specific [verification ledger](VERIFICATION.md)
before treating a later revision as verified.

Reference CI at commits `96066054` and `95c45b0` executed all 18 native/WASM
comparison cases with exact equality, all 32 source XA/UTK differential vectors,
and the pinned-A 60-tick authority-boundary probe. The reviewed library correction
batch fixes the audio-test and formatting failures from those runs. Independent
local reviews approved the final avatar, geometry and audio changes; the exact
published batch still requires a combined CI pass. See the verification ledger
for each check's scope and source evidence.

## Code map

| Location | Responsibility |
|---|---|
| [render-core](../../crates/render-core/) | Validated immutable frames, units/math, game identities, stale-pick rejection, bounded residency, CPU reference rasterization |
| [render-iso](../../crates/render-iso/) | Source DGRP/projection/depth/alpha and lighting equations, batches, cutaways, view transitions, invalidation |
| [render-3d](../../crates/render-3d/) | Lot architecture, depth reconstruction and override/cache resolution, city geometry/transition state, cameras/environment/quality policy |
| [avatar-view](../../crates/avatar-view/) | Normalized rig/mesh admission, dual-position skinning, retained poses, appearance, attachments/contact/look, previews, cooking and LOD |
| [audio-runtime](../../crates/audio-runtime/) | HIT execution/status census, causal cues, mixer intents, PCM/XA/UTK, FSC/stations/ambience, native/browser adapters |
| [engine-bakeoff](../../probes/engine-bakeoff/) | Shared synthetic scene/reference, native/WASM replay, Bevy and Fyrox adapters, browser host |
| [verification tools](../../tools/swarm-c/) | Package, source-differential, A/C authority-boundary and actual engine/browser checks |

The five libraries and the fixture/replay packages have independent Rust 1.75
workspaces. Engine adapters use isolated Rust 1.95.0 workspaces; their dependencies
do not enter the authoritative simulation graph.

## Start here

- [SPEC.md](SPEC.md) states the agreed scope and acceptance requirements.
- [CONTRACT.md](CONTRACT.md) defines the C-local types, coordinates and lifetime rules.
- [COVERAGE.md](COVERAGE.md) maps all 17 planned scope items to implementation,
  proof, unfinished code and acceptance conditions.
- [HANDOFF.md](HANDOFF.md) identifies provider inputs, integration order and ownership.
- [VERIFICATION.md](VERIFICATION.md) gives reproducible commands and the evidence ledger.
- [Engine decision](../decisions/engine.md) records why engine selection remains pending.

From a provisioned checkout:

```sh
bash tools/swarm-c/verify.sh
bash tools/swarm-c/verify-native-wasm.sh
bash tools/swarm-c/verify-audio.sh
```

The first command tests the pure libraries, fixture/replay packages and lightweight
JavaScript checks. The other commands need the matching WASM target and installed
Mono/FFmpeg tools respectively. Engine and pinned-A checks have separate setup
instructions in [VERIFICATION.md](VERIFICATION.md).

## Invariants to preserve during integration

Rendering reads accepted 30 Hz simulation state. Rendering cadence, audio's
60 Hz interpreter cadence, decode callbacks and device clocks cannot advance A,
consume its RNG or acknowledge gameplay work. Game entity identity includes a
generation and remains independent of engine handles.

A resolved visual tile position maps to graphics as `(3*x, 3*z, 3*y)`. Content
conversions happen once: B-normalized FreeSO avatar coordinates must not be
negated again, and raw DGRP/SLOT units must not be mistaken for graphics units.

Full2D, Hybrid2D and Full3D remain distinct view modes. WebGPU and WebGL2 describe
backends. The reference scene demonstrates those dimensions with synthetic
inputs; it is not a complete interactive city/lot client.

C-local contracts await explicit adoption by the global integration owner.
Authorized content, live directory/admission services and physical hardware
must supply their own acceptance evidence. No original game payload is included.
