# Swarm C representative engine probes

These are executable engine adapters for one **synthetic**, immutable Wonderland
presentation fixture. They are separate from the Rust 1.75 simulation/reference
workspace. Compilation, GPU rendering, image parity and physical-device acceptance
are separate gates; inspect the recorded CI results before treating any as passed.

| Adapter | Exact engine | Browser backend | Native backend |
|---|---|---|---|
| `bevy/` | Bevy `=0.19.1` | Separate `webgpu` and `webgl2` artifacts | Bevy/wgpu; Linux feature `x11` |
| `fyrox/` | Fyrox `=1.0.1` | WebGL2 | OpenGL |

Both adapters pin the newer compiler through `rust-toolchain.toml` to **1.95.0**.
Each manifest has its own `[workspace]` and lock. Engine dependencies never enter
the authoritative simulation graph. The `fixture/` package remains independently
testable with the reference compiler.

## Build and run

Use a provisioned compiler, matching WASM target libraries, engine dependencies
and native graphics libraries. The reference environment's Rust 1.75 and offline
vendor cache cannot build either engine. Do not replace the reference toolchain
or its cargo configuration to attempt these builds.

From the repository root, after resolving and recording the two engine locks:

```sh
cargo +1.95.0 test --locked --manifest-path probes/engine-bakeoff/bevy/Cargo.toml --no-default-features --features x11 --lib
cargo +1.95.0 run --locked --manifest-path probes/engine-bakeoff/bevy/Cargo.toml --no-default-features --features x11 -- --mode hybrid2d --avatars 32 --tick 30 --frames 120
cargo +1.95.0 test --locked --manifest-path probes/engine-bakeoff/fyrox/Cargo.toml --lib
cargo +1.95.0 run --locked --manifest-path probes/engine-bakeoff/fyrox/Cargo.toml -- --mode hybrid2d --avatars 32 --tick 30 --frames 120
```

Omit `--frames` to keep the native window open. Keys `1`, `2`, and `3` change view.
`WONDERLAND_PROBE_FRAMES` supplies the same exit limit. Bevy counts render schedule
cleanup visits; Fyrox counts completed custom LDR passes that issued fixture draw
calls. These counters do not certify nonempty fixture pixels or screenshot parity.
The native logs preserve `WONDERLAND_PROBE` JSON and engine renderer diagnostics.

Build the three browser artifacts independently:

```sh
cargo +1.95.0 build --locked --manifest-path probes/engine-bakeoff/bevy/Cargo.toml --lib --target wasm32-unknown-unknown --no-default-features --features webgpu --target-dir engine-target/bevy-webgpu
cargo +1.95.0 build --locked --manifest-path probes/engine-bakeoff/bevy/Cargo.toml --lib --target wasm32-unknown-unknown --no-default-features --features webgl2 --target-dir engine-target/bevy-webgl2
cargo +1.95.0 build --locked --manifest-path probes/engine-bakeoff/fyrox/Cargo.toml --lib --target wasm32-unknown-unknown --target-dir engine-target/fyrox-webgl2
```

Bevy deliberately emits `compile_error!` if both browser features are enabled.
Never use `--all-features` for that adapter. Ordinary non-atomic WASM is the
baseline: no `+atomics`, shared memory, worker initialization, COOP or COEP.

The root-owned CI wrapper is `tools/swarm-c/build-engine.sh`, selected with
`WONDERLAND_ENGINE=bevy|fyrox` and `WONDERLAND_BACKEND=native|webgpu|webgl2`.
It resolves a missing initial lock, retains the exact lock as evidence, and
requires locked builds thereafter. A generated initial lock must be reviewed and
committed before subsequent reproducible comparison runs.

### Package the browser host

`web/package.sh` packages an **already built** WASM binary. It requires an
installed `wasm-bindgen` CLI whose version exactly equals the crate version in
that engine's `Cargo.lock`; it reads that version and rejects a mismatch. It does
not install tools or invent a current binding version.

```sh
bash probes/engine-bakeoff/web/package.sh bevy-webgpu engine-target/bevy-webgpu/wasm32-unknown-unknown/debug/bevy_gate.wasm engine-dist
bash probes/engine-bakeoff/web/package.sh bevy-webgl2 engine-target/bevy-webgl2/wasm32-unknown-unknown/debug/bevy_gate.wasm engine-dist
bash probes/engine-bakeoff/web/package.sh fyrox-webgl2 engine-target/fyrox-webgl2/wasm32-unknown-unknown/debug/fyrox_gate.wasm engine-dist
python3 probes/engine-bakeoff/web/serve.py engine-dist --port 8080
```

The resulting host imports `./pkg/<variant>/engine.js`, calls bindgen's initializer,
checks the exported memory, and calls the engine's `run()` export. It includes the
shared browser audio adapter under `audio/`. The host requires no npm build.

Example initial fixture selection:

```text
http://127.0.0.1:8080/?variant=bevy-webgpu&mode=hybrid2d&avatars=32&tick=30
```

Supported variants are `bevy-webgpu`, `bevy-webgl2`, `fyrox-webgl2`; view modes
are `full2d`, `hybrid2d`, `full3d`; avatar counts are `1..64`; ticks are nonnegative
JavaScript safe integers. The initial query is frozen. Later explicit commands
request another immutable fixture; rendering clocks never advance its tick.

## What is actually uploaded

`web/upload.rs` supplies the same CPU arrays and raw textures to both engines:

- All generated architectural/object/avatar triangles and indices, with each
  core model matrix applied once; normals use the inverse transpose. The shared
  avatar fixture already performs source dual-local-position CPU skinning.
- Logical-size sprite images sampled nearest, raw depth in a dedicated channel,
  and effective alpha where a mask replaces source image alpha. Each sprite owns
  its texture binding, so missing data cannot inherit a previous sprite sampler.
- One finite, right-handed core camera projection. Bevy's custom vertex shader
  changes `z` to `w-z` for reverse depth; Fyrox changes it to `2z-w` for OpenGL.
- Custom WGSL/GLSL source material branches for depth, gamma lighting, special
  room color policies, premultiplication, and a separate ID visualization mode.

The custom shader's sprite depth is exactly
`back + (1 - q/255) / 0.4 * (front - back)`; color coverage requires byte alpha
greater than 2; ID coverage requires byte alpha at least 26. The GPU ID color is
a transient 24-bit index into `idMap`; its paired game object ID and generation
are the authoritative identity. Engine entity/node/asset handles stay private.

**Selection currently uses the CPU reference and generation-checked tickets.**
`setPass('pick')` is the actual engine shader ID visualization path, but asynchronous
GPU ID readback remains pending. The browser gate reads the actual ID visualization
screenshot and compares its stable interior bytes with the independent CPU reference.
Bevy uses `CompositingSpace::Srgb`, and Fyrox draws in a custom LDR pass after tone
mapping and FXAA. Both paths blend source-encoded, premultiplied colors in source
draw order. Final output parity must still be measured; no GPU parity claim follows
from the source arithmetic tests.

Replacing the fixture rebuilds presentation resources, preserves a live selection,
and invalidates old pick tickets. Bevy removes owned assets; Fyrox releases the old
scene and flushes this isolated probe's renderer caches. Resource residency and
frame timing must still be measured under repeated changes.

## Browser automation contract

`window.__wonderlandProbe` exposes:

| Method | Contract |
|---|---|
| `snapshot()` | Side-effect-free copy of runtime observations and current fixture state |
| `alignForCapture()` | Translate the complete host to integer document pixels without changing drawable size |
| `setMode(mode)`, `setTick(tick)` | Queue explicit fixture replacement; return command sequence |
| `selectAt(x,y)` | Queue a reference pick; integer coordinates must be within 640×480 |
| `setPass('color'|'pick')` | Change custom GPU color/ID visualization branch |
| `suspend()`, `resume()` | Pause/resume presentation input and update accounting |
| `simulateLoss()` | Invalidate pick tickets and pause; separately counted synthetic loss |
| `reloadFixture()` | Rebuild the current fixture and reset audio lifecycle |
| `resize(width,height)` | Request CSS canvas-host dimensions, bounded to 1..4096 |
| `loseContext()`, `restoreContext()` | Request actual WebGL context loss/restoration through its extension |
| `loseDevice()` | Destroy the observed WebGPU device; only `device.lost` records actual loss |
| `recover()` | Reload the application to rebuild resources after actual device/context loss |
| `startAudio()`, `stopAudio()`, `audioSnapshot()` | Actual browser audio fixture controls, still subject to gesture unlock |

Wait for `snapshot().lastCommand >= returnedSequence` before asserting command
effects. Readiness is named `scene-uploaded`; the host additionally requires an
observed engine backend and no startup errors. `actualBackend` remains null until
the engine creates a real WebGL2 context or WebGPU device. There is no silent
fallback: an unexpected backend is a failure. Screenshots and nonempty pixel
assertions establish rendering separately.

`gpuSubmissions` observes actual WebGPU queue submissions; `glDrawCalls` observes
actual GL draw calls; `renderScheduleVisits` is an engine control-flow counter.
`submittedFrameTimes` samples browser animation-frame intervals with submissions;
it is not GPU execution time. Snapshot calls never advance any counter.
`wasmMemoryShared` is checked against the actual exported WASM memory buffer.
`engineResourceOwnership` reports owned engine handles rather than fixture counts.
Fyrox also exposes actual renderer cache entries, CPU render time and custom-pass
draw counts. These measurements are merged with fixture state; they are neither
GPU memory-byte measurements nor GPU execution timings.

DOM login/chat/search/interaction fields retain keyboard input. Game view keys
and pointer selection require canvas focus; pointer cancellation cannot select.
Pointer coordinates scale to the fixed fixture viewport after resize. Fyrox's
appended canvas is reparented inside `#canvas-host`. CSS controls its dimensions;
the engine's HiDPI and resize behavior remains an observed gate.

Audio unlock is a direct click on `#unlock-audio`; automatic resume never bypasses
browser gesture policy. `#resume` performs gesture resume. `startAudio()` uses a
bounded synthetic 24 kHz sine PCM fixture. Audio counts in snapshots come from
the actual shared adapter. A locked voice is not reported as audible playback.

Actual loss sets a separate `lost` lifecycle and suspends input/audio. A GL restore
event changes it to `restored-awaiting-reload`; neither event claims recreated
engine resources. `recover()` is the explicit safe recovery path in this probe.

## Local lightweight checks

```sh
node --test probes/engine-bakeoff/web/host-core.test.mjs
node --check probes/engine-bakeoff/web/host.mjs
rustc --test probes/engine-bakeoff/web/math.rs -o /tmp/wonderland-engine-math
/tmp/wonderland-engine-math
bash -n probes/engine-bakeoff/web/package.sh
```

These checks cover control/transfer arithmetic, not engine compilation. The
coordinated `tools/swarm-c/browser-gate.mjs` performs available browser scenarios.
Headless software renderer results remain distinct from physical Safari/iPhone,
iPad, Android Chrome and desktop hardware acceptance.

## Evidence status after the first engine CI run

At checkpoint `ca79bbd251491277d9fd5838d3f84d247675709a`, Fyrox native
compilation and all ten library tests succeeded, and Fyrox WebGL2 WASM compiled.
Its browser packaging then failed while hashing a directory; its native executable
stopped because `libxkbcommon-x11.so` was missing. Bevy's three builds reported
three API compatibility errors involving optional depth state and a mutable asset
guard. The coordinated checkpoint `809ec200e186871970b2e4cabb06d7f6c08ecd9e`
contains those compiler, system-library and packaging corrections. Its next CI
run succeeded for both native jobs and all three WASM builds. Both WebGL2 browser
runs then stopped at strict screenshot dimensions, while WebGPU readiness timed
out. The source-space compositing, custom LDR, capture-alignment and bootstrap
changes require a fresh engine/browser run. No renderer or physical-device
qualification is implied.

Element screenshots require integer document-pixel bounds because Playwright
rounds fractional rectangles outward. The gate calls alignForCapture(), checks
the exact CSS drawable size and then checks the PNG is exactly 640 by 480. It does
not repair mismatches by image resizing. Bootstrap events are available in the
snapshot and emitted as WONDERLAND_BOOTSTRAP console records for startup diagnosis.
