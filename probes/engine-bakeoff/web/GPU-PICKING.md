# Engine GPU selection and readback

Interactive `selectAt(x, y)` uses logical 640×480 coordinates with a top-left
origin. Each request freezes the current `FrameStore` pick tickets in Rust and
gets its own serial, command sequence and scene revision. The previous selection
remains visible until that request completes. GPU resources and returned color
indices never become game identities.

The offscreen ID format is `rgb24-opaque-v1`: index = R | G << 8 | B << 16,
with alpha 255. Index zero means no hit, including an ownerless occluder. A
nonzero index resolves only through that request's frozen ticket table and the
current `FrameStore`. Invalid pixels, map failures and stale tickets preserve
the previous selection and publish a failed or stale status.

The host reports `pickState`, `pickPendingCommand`, `pickCompletedCommand` and
`pickError`. An automation client must await `pickCompletedCommand >= sequence`
and then require `pickState === 'completed'`. `lastCommand` only acknowledges
command processing; it does not acknowledge a GPU result. Accepted results have
`selectionSource === 'gpu-id-readback-generation-checked'`.

## Backend ownership

Bevy owns a separate RGBA8 ID image, camera and render layer. ID materials are
immutable copies of the source materials with the ID flag enabled. The ID camera
is active only while a pick is pending. Readiness requires the complete source
draw phase for that camera, every source draw's compiled pipeline, the prepared
target-format output pipeline, and two rendered frames for the current request.
An unrelated optional compute pipeline remaining queued does not block that
view. A timeout reports the source draw count, view readiness, image readiness,
revision and a bounded list of pending pipeline labels.

After rendering, the adapter copies one pixel into a single 256-byte staging
buffer and maps it asynchronously. Completion and cancellation take exclusive
ownership of the buffer before calling GPU APIs. Successful completion unmaps
and destroys it; a failed map only destroys it; cancellation unmaps a pending
map and destroys it. The callback and transfer cleanup cannot release the same
buffer twice. A guarded mailbox rejects callbacks for superseded requests.

Fyrox uses its source render pass, a separate 1×1 RGBA8/depth framebuffer and
the engine's asynchronous PBO/fence API. Translating the viewport selects the
requested logical pixel while retaining the source projection, textures,
draw order, depth and alpha tests. Uniform blocks for the offscreen pass capture
the ID flag; the visible material flag is restored before drawing that pass.
There is at most one transfer in flight. Cancelled transfers continue to drain,
including while the scene is suspended, so the engine-owned fence is retained
until completion or a real graphics-device reset.

Neither adapter changes the visible pass when handling a selection. A new
request, fixture replacement, mode/tick change, suspension or device reset
cancels admission of the previous result, including a late no-hit pixel.

## Canvas diagnostic

`readGpuFrame()` is a separate diagnostic that copies the actual Bevy WebGPU
canvas texture after engine submission. It is not used for interactive picking.
The diagnostic retains the 15-second deadline and records the phase that
stalled: texture acquisition, submission, copy, validation or mapping. The host
snapshot includes `gpuReadbackDiagnostic` with timestamps, texture/submission
counts and completion/cancellation details. Reconfiguration, resize, fixture
change and device loss cancel a pending diagnostic and release its buffer.

Raw texture correctness, direct canvas readback and browser page presentation
are independent evidence. The browser picker regression does not qualify page
presentation; the screenshot gate remains required.

## Regression commands

The dependency-light bridge harness runs on Rust 1.75:

```sh
cargo test --manifest-path probes/engine-bakeoff/web/bridge-tests/Cargo.toml --locked
node --test probes/engine-bakeoff/web/host-core.test.mjs probes/engine-bakeoff/web/gpu-readback.test.mjs
```

The two browser tests require Playwright and a Chromium GPU backend. Set
`WONDERLAND_PLAYWRIGHT_MODULE` to an installed Playwright `index.mjs` when the
repository's `tools/swarm-c/node_modules` is absent. `WONDERLAND_CHROMIUM` can
select an explicit browser executable.

```sh
node --test probes/engine-bakeoff/web/gpu-readback.browser-test.mjs
WONDERLAND_ENGINE_VARIANT=bevy-webgpu \
WONDERLAND_REFERENCE_DIR=/absolute/path/to/generated/reference \
WONDERLAND_GPU_PICK_REPORT=/absolute/path/to/result.json \
node --test probes/engine-bakeoff/web/gpu-picker.browser-test.mjs
```

The engine test requires the selected WASM artifact under `web/pkg/<variant>`,
the shared audio module under `web/audio`, and the fixture's independently
generated color, ID and depth references. It checks all six mode/crowd scenes,
distinct live owners, ownerless occlusion, background and interrupted requests.
Each interruption is issued after an actual GPU transfer: the harness holds the
WebGPU map notification or the WebGL PBO fence, changes the live scene lifecycle,
and then releases the old completion. The preserved live selection must survive
that late no-hit result. Consecutive successful picks also exercise completed
transfer retirement before the next request.
For Bevy WebGPU it also checks all twelve actual canvas copies against the
existing zero-ID-mismatch requirement and unchanged color tolerances. Supported
variants are `bevy-webgpu`, `bevy-webgl2` and `fyrox-webgl2`; the report records
the observed browser version and exact launch flags.

## Retained continuation evidence

The following actual browser runs use Chromium `141.0.7390.37` with SwiftShader,
Rust `1.95.0`, locked engine dependencies, and ordinary WASM. The recorded build
profile is `dev`, optimization level 1, debug information disabled. These are
correctness and lifecycle observations, not release performance measurements.

| Backend | Exact interactive selections | Interruptions after actual GPU transfer | Additional pixel evidence |
|---|---:|---:|---|
| [Bevy WebGPU](evidence/bevy-webgpu-final-picker.json) | 60 across six scenes | 18 | All 12 canvas copies mapped; 20,319 ID samples with zero mismatches, including 6,560 ownerless samples; all six color comparisons within the unchanged limits. |
| [Bevy WebGL2](evidence/bevy-webgl2-picker.json) | 60 across six scenes | 18 | Offscreen ID pixels; the visible pass stays `color` throughout interactive selection. |
| [Fyrox WebGL2](evidence/fyrox-webgl2-picker.json) | 60 across six scenes | 18 | Offscreen ID pixels; the visible pass stays `color` throughout interactive selection. |

The final WebGPU run includes both the source-view pipeline readiness and
exclusive buffer ownership corrections. Its raw copies completed in 191–348 ms.
Maximum color mean error was 0.19410 bytes, RMS error 3.13382 bytes, and the
fraction of channels differing by more than eight bytes was 0.006157. The gates
remain 4, 12, and 0.03 respectively, with zero allowed ID mismatches.

The [build record](evidence/bevy-webgpu-build.json) records the raw, binder-input
and packaged WASM hashes. To fit the local binder memory budget, the installed
Rust `llvm-tools` `rust-objcopy` removed only name, producer and debug metadata
before `wasm-bindgen 0.2.129`. All ten executable sections were verified
byte-for-byte identical; no instruction optimization or image-gate change was
used for this artifact. Earlier [WebGPU evidence](evidence/bevy-webgpu-inflight-picker.json)
is retained as a historical run rather than relabeled as the final build.

The [pipeline diagnostic](evidence/bevy-webgl2-pipeline-diagnostic.json) records
the unrelated sparse-update compute queue that motivated source-view readiness.
The [GL failure snapshot](evidence/bevy-webgl2-double-unmap-diagnostic.json)
records the completed first pick followed by wgpu-core's `Buffer is not mapped`
validation error. The final GL run exercises consecutive completion and
cancellation on the corrected one-shot owner. Ordinary browser-page presentation
and the physical-device acceptance matrix remain separate from these results.
