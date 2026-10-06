# Swarm C — views and audio

Swarm C implements the presentation work described by W07–W10 and SL.5: isometric
and hybrid drawing, 3D lot/city geometry, avatar visuals, audio, and a representative
Bevy/Fyrox comparison. The implementation is additive to source baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73` and is reviewed in
[PR #12](https://github.com/rndrntwrk/wonderland-/pull/12).

The branch includes the presentation libraries, a normalized FSOm object adapter,
a CPU thumbnail/day/night facade worker with bounded scheduling, and a continuous
native audio transport with a separate CPAL device binding. These are concrete
code paths; complete client composition and acceptance still have separate gates.
The reviewed renderer/fixture follow-up is published at
`d247ebb94d1d4de79247983b4f62fffb7e06427d`. Its complete reference CI and both
native engine jobs passed. The reference job passed 377 Rust package tests,
including 16 fixture tests, plus 19 Node audio, six host, six readback-protocol
and 11 Python audio tests. All 32 unchanged-source codec comparisons matched
complete WAV bytes. All 18 complete native/WASM observations matched, and
the genuine pinned-A 60-tick probe preserved every state hash and ordered event
at 30/60/120 Hz. The [retained reference evidence](evidence/reference-d247ebb.json)
records the exact outputs and source/runtime checks.

Fixture version 2 uses the measured full-cycle avatar footprint to prevent
unintended overlap in the synthetic crowd. It retains the same lot/camera,
32/64 counts, depth laws and exact-ID criterion. Integral host layout and the
new WebGPU engine/canvas/page readback diagnostic address the earlier measurement
questions. Both Bevy and Fyrox WebGL2 passed all six color and physical-ID scenes
plus the full lifecycle gate. Bevy WebGPU remains failed: a minimal test reproduced
a browser presentation failure without Bevy, while all six direct engine-canvas
ID snapshots passed. Its raw GPU copy diagnostic also remains incomplete. The
retained [engine evidence](evidence/engines-d247ebb.json) distinguishes those
results; the ledger preserves the three failed `f6f78be`
browser jobs. Hosted software results do not qualify physical GPUs.

The separate native-audio job passed locked CPAL compilation, four configuration
tests and the OS ALSA-null stream; its
[retained evidence](evidence/native-audio-d247ebb.json) includes the actual callback
results. Browser parity, complete client composition, real-provider/content
coverage and physical-device acceptance remain open. The commit-specific
[verification ledger](VERIFICATION.md) retains the successful evidence and the
browser failures/corrections without extending a result to untested revisions.

## Code map

| Location | Responsibility |
|---|---|
| [render-core](../../crates/render-core/) | Validated immutable frames, units/math, game identities, stale-pick rejection, bounded residency, CPU reference rasterization, thumbnail/facade rendering and job lifetimes |
| [render-iso](../../crates/render-iso/) | Source DGRP/projection/depth/alpha and lighting equations, batches, cutaways, view transitions, invalidation |
| [render-3d](../../crates/render-3d/) | Lot architecture, depth reconstruction and override/cache resolution, normalized multi-material FSOm objects and ordered draw contracts, city geometry/transition state, cameras/environment/quality policy |
| [avatar-view](../../crates/avatar-view/) | Normalized rig/mesh admission, dual-position skinning, retained poses, appearance, attachments/contact/look, previews, cooking and LOD |
| [audio-runtime](../../crates/audio-runtime/) | HIT execution/status census, causal cues, mixer intents, PCM/XA/UTK, FSC/stations/ambience, browser playback and continuous native callback transport |
| [facade-worker](../../tools/swarm-c/facade-worker/) | Bounded normalized request decoding, six PNG derivative outputs, complete reproduction metadata and independent repeat/PNG verification |
| [engine-bakeoff](../../probes/engine-bakeoff/) | Shared synthetic scene/reference, native/WASM replay, Bevy and Fyrox adapters, browser host |
| [verification tools](../../tools/swarm-c/) | Package, source-differential, A/C authority-boundary and actual engine/browser checks |

The five libraries, fixture/replay packages, native audio transport and facade
worker have independent Rust 1.75 workspaces. Engine adapters and the isolated
CPAL device CI use Rust 1.95.0; their dependencies do not enter the authoritative
simulation graph. CPAL's actual ALSA-null execution passed in
[job 111926069292](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069292).
Use its retained dependency lock for subsequent locked runs. Physical speakers,
device unplug/reopen and production-load qualification remain separate gates.

## Start here

- [SPEC.md](SPEC.md) states the agreed scope and acceptance requirements.
- [CONTRACT.md](CONTRACT.md) defines the C-local types, coordinates and lifetime rules.
- [COVERAGE.md](COVERAGE.md) maps all 17 planned scope items to implementation,
  proof, unfinished code and acceptance conditions.
- [HANDOFF.md](HANDOFF.md) identifies provider inputs, integration order and ownership.
- [VERIFICATION.md](VERIFICATION.md) gives reproducible commands and the evidence ledger.
- [FSOm source notes](fsom-source-notes.md), [derivative source notes](derivatives-source-notes.md)
  and the [native callback guide](../../crates/audio-runtime/native/README.md)
  describe the new adapters and their exact integration boundaries.
- [Engine decision](../decisions/engine.md) records why engine selection remains pending.

From a provisioned checkout:

```sh
bash tools/swarm-c/verify.sh
bash tools/swarm-c/verify-native-wasm.sh
bash tools/swarm-c/verify-audio.sh
```

The first command tests the pure libraries, fixture/replay packages, native audio
transport, facade worker and lightweight JavaScript checks. The other commands
need the matching WASM target and installed Mono/FFmpeg tools respectively.
Actual CPAL, engine and pinned-A checks have separate setup instructions in
[VERIFICATION.md](VERIFICATION.md).

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
