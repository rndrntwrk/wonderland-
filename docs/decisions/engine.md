# Engine decision — representative comparison still pending

**Status: pending acceptance; no production engine selected.** Bevy remains the
lead candidate named by the supplied plan. C has also implemented a Fyrox adapter
so the decision can use the same actual workload and explicit evidence.

## Isolated candidates

| Candidate | Pinned compiler and engine | Paths exercised |
|---|---|---|
| Bevy | Rust 1.95.0; Bevy `=0.19.1` | Native Linux/wgpu with X11; separate browser WebGPU and WebGL2 binaries |
| Fyrox | Rust 1.95.0; Fyrox `=1.0.1` | Native OpenGL and browser WebGL2 |

See the [probe manifests, shaders and host](../../probes/engine-bakeoff/).
The five presentation libraries and the shared fixture/replay remain separately
buildable with Rust 1.75. Engine entity IDs, assets and callbacks stay outside
simulation state. Choosing an engine must not change the 30 Hz authority boundary.

Bevy's browser variants are compiled separately; combining `webgpu` and
`webgl2` is deliberately rejected by the adapter. No Fyrox WebGPU path is claimed.
All browser probes use ordinary WASM, without atomics, shared-memory workers or
a cross-origin-isolation requirement.

## What the available comparison establishes

### Current continuation: asynchronous selection and raw readback

The continuation implements genuine offscreen GPU selection for both engine
adapters. Each request captures private `FrameStore` tickets; a returned RGB24
index resolves only against those tickets and the current generation/content
state. Completion is asynchronous, and cancellation rejects late results after
scene replacement, suspension, reset or device loss. Interactive selection
does not change the visible color pass. The
[implementation contract and reproducible commands](../../probes/engine-bakeoff/web/GPU-PICKING.md)
describe the shared protocol and backend ownership.

All three rebuilt browser variants have actual continuation evidence on
Chromium `141.0.7390.37` with SwiftShader, Rust `1.95.0`, and ordinary WASM without
shared memory or cross-origin isolation. The builds use dev optimization level 1
with debug information disabled; these runs establish correctness and lifecycle
behavior, not release performance.

| Browser candidate | Current offscreen selection evidence | Current raw canvas evidence |
|---|---|---|
| [Bevy WebGPU](../../probes/engine-bakeoff/web/evidence/bevy-webgpu-final-picker.json) | All six scenes; 60 exact picks; 18 interruptions after actual GPU map completion was held; no console errors. | All 12 copies mapped in 191–348 ms; 20,319 ID samples with zero mismatches, including 6,560 ownerless samples; all six color comparisons pass unchanged thresholds. |
| [Bevy WebGL2](../../probes/engine-bakeoff/web/evidence/bevy-webgl2-picker.json) | All six scenes; 60 exact picks; 18 interruptions after actual PBO transfer with fence completion held; no console errors. | This continuation run checks offscreen selection. Its ordinary color/physical-ID screenshot evidence remains the separate historical gate below. |
| [Fyrox WebGL2](../../probes/engine-bakeoff/web/evidence/fyrox-webgl2-picker.json) | All six scenes; 60 exact picks; 18 interruptions after actual PBO transfer with fence completion held; no console errors. | This continuation run checks offscreen selection. Its ordinary color/physical-ID screenshot evidence remains the separate historical gate below. |

The final Bevy WebGPU artifact includes the corrected source-view pipeline
readiness and one-shot readback buffer ownership. Readiness checks every draw
pipeline and the output pipeline for the ID camera; an unrelated optional
`sparse buffer update pipeline` remaining queued on WebGL cannot stall that
camera. Exclusive buffer ownership prevents successful completion and later
cleanup from unmapping the same buffer twice. The
[build record](../../probes/engine-bakeoff/web/evidence/bevy-webgpu-build.json)
retains the exact compiled, metadata-stripped and packaged WASM hashes. Removing
name/debug metadata to fit local binder memory left every executable section
byte-for-byte unchanged.

The raw-copy diagnostic now records bounded acquisition, submission, copy,
validation and mapping stages, with request-local cancellation and cleanup.
The final raw comparisons have maximum color mean error 0.19410 bytes, RMS
3.13382 bytes, and channel fraction over eight bytes 0.006157; acceptance stays
at 4, 12, and 0.03 with zero permitted ID mismatches.

These passes do **not** qualify ordinary Bevy WebGPU page presentation. The
historical hosted-browser presentation failure remains recorded separately;
the local headed-browser experiment could not reach graphics because its
AF_UNIX socket creation was denied by the environment. The current adapters
were built and executed in browser WASM; native engine execution below remains
historical and is not relabeled as a run of these new readback changes. No
production engine or physical-device backend has been selected by these results.

### Published baseline at d247ebb

The previous reviewed follow-up was published at
`d247ebb94d1d4de79247983b4f62fffb7e06427d`, exercised by
[run 37358255818](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818).
Its complete reference passed 377 Rust tests, including the 16-test fixture
suite, and all 18 native/WASM records. Both native engine jobs and the locked
CPAL/null-stream job also passed. Both Bevy and Fyrox WebGL2 passed all six
color/physical-ID scenes and lifecycle checks with zero failures. Bevy WebGPU
failed at that revision: its ordinary scene images failed and nine of twelve raw GPU
copies timed out. All six direct engine-canvas ID snapshots passed, and a minimal
clear reproduced a hosted-browser page-presentation failure without Bevy. Each
outcome is recorded independently; these
hosted-software passes do not select a production engine or qualify a device.

That revision corrected the synthetic crowd's unintended physical overlap using
its full animation footprint, preserves existing source/coplanar regressions,
and removed fractional host transforms. It also added a reviewed diagnostic that
distinguishes engine-texture copies, direct canvas snapshots and page capture.
The [fixture review](../swarm-c/evidence/fixture-v2-review.json) and
[readback protocol review](../swarm-c/evidence/gpu-readback-protocol.json) establish
their local source/ownership behavior. Actual browser evidence is recorded
separately in the [verification ledger](../swarm-c/VERIFICATION.md).
The candidate and backend policy remain unselected.

### Previous comparison at f6f78be

The previous published comparison was commit
`f6f78be1fef247f2db47e19f56d94054f0c9e88c`, exercised by
[run 37353615543](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543).
Both native adapters and all three browser variants compiled. Both native
software-renderer jobs passed, including nine Bevy tests and twelve Fyrox tests.
The separate reference gate passed 374 Rust tests and all 18 complete
native/WASM records; locked CPAL tests and actual ALSA-null execution also passed.
These are separate evidence classes, recorded in the
[verification ledger](../swarm-c/VERIFICATION.md).

| Browser candidate | Recorded evidence at f6f78be | Selection consequence |
|---|---|---|
| Bevy WebGL2 | All six colors, DPR/resize and lifecycle checks passed. Full2D/64 and Hybrid2D/64 retained 3 physical-ID mismatches each; Full3D ID comparisons passed. | Required exact-ID acceptance remains failed. |
| Fyrox WebGL2 | All six colors, DPR/resize and lifecycle checks passed. Full2D/64 and Hybrid2D/64 retained 5 and 4 physical-ID mismatches respectively; Full3D ID comparisons passed. | Required exact-ID acceptance remains failed. |
| Bevy WebGPU | Capture geometry, lifecycle and the separate mapped-pixel diagnostic passed. The runner recorded 15 image failures, with DOM content visible in the measured canvas region. | Clean engine pixel output has not been independently established; image acceptance remains failed. |

All three browser jobs at that revision remain failed. Independent review verified all
twelve WebGPU crops against the retained full-page PNGs byte-for-byte. Correct
extraction does not prove correct canvas contents: identical DPR 1 color/pick
images and page controls at DPR 2 remain unexplained. The earlier origin-error
inference is unproven, and direct engine-target readback was not yet available to
isolate the renderer's output. The WebGL2 IDs required diagnosis at
their actual physical pixel grid. Neither a successful lifecycle check nor a
small mismatch count permits a production choice. No color threshold or exact-ID
requirement has been relaxed.

### Historical checkpoints

At commit `809ec200e186871970b2e4cabb06d7f6c08ecd9e`,
[CI run 37337020509](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509)
built both native adapters and all three browser packages. Both native hosted
software-runtime jobs passed. Browser WebGL2 reached image comparison but failed
capture dimensions; WebGPU timed out during readiness. These are useful build
and startup results, and they do not establish color/ID parity or a winning engine.

Both native jobs passed again at `96066054` in
[run 37339944118](https://github.com/rndrntwrk/wonderland-/actions/runs/37339944118).
The same checkpoint also passed actual native/WASM equality for all 18 shared
reference scenarios. That establishes portable algorithm execution; browser
image and physical-device qualification remain open.

The renderer correction checkpoint `95c45b0bdc514669e616648d61258c33563adc5b`
pins both generated Cargo locks and the npm lock, uses Bevy source-color sRGB
compositing, and adds a Fyrox LDR pass after tone mapping. Both native jobs passed
again in [run 37343212639](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639),
including compilation and execution of the new Fyrox shaders. The ledger retains
the later coplanar-depth correction, physical-DPR checks and malformed screenshot
findings, including the validated extraction improvements. The current browser outcomes above supersede pending descriptions
of those earlier checks without rewriting their recorded failures.

### Workload and scope

The fixture supplies the same CPU geometry, source-depth/alpha sprites, synthetic
skinned avatar instances, camera and tick to both adapters. It has all three view
modes and 32/64 workloads. GPU color and ID visualization are compared with the
independent CPU reference. The interactive pick path now uses the engine's
offscreen GPU ID pass and asynchronous, generation-checked request tickets;
CPU reference images are an oracle for tests and are not a selection fallback.
Full advanced lighting, production content, day/night facade atlas integration
and the live city client are outside this small fixture. The separate CPU
derivative worker does produce normalized day/night PNG atlases, and the
normalized FSOm adapter emits ordered multi-material/stencil commands. Their
library/source tests do not implement those passes inside these small engine
probes. Production source-material, lighting, avatar and source-isometric
composition in the separate existing client is independently implemented and
verified; it does not expand the workload exercised by this engine comparison.

The native run is bounded and reports successful scene state, errors and
authoritative ticks advanced. Bevy render-schedule visits and Fyrox draw
statistics are different observations. Neither is a pixel-parity test.
Browser submission intervals are not GPU execution time. Synthetic scene counts
are not production performance measurements.

## Evidence required before selecting an engine

| Decision dimension | Acceptance evidence |
|---|---|
| Rendering correctness | Nonempty same-input captures for each supported backend/view, source depth and alpha edges, mask padding, room/gamma policies, wall/roof/cutaway ordering and game-ID occlusion; explain intentional source deviations. |
| Actual browser integration | Correct backend observed, ordinary-memory baseline, DOM field focus, pointer cancellation and resize/HiDPI behavior; gesture-gated audio, suspend/resume, real context/device loss and complete resource recovery. |
| Resource lifecycle | Repeated lot/content/outfit/mode changes and previews with stable owned resource counts and measured residency; stale picks/decode completions cannot affect new state. |
| Performance and quality | Comparable release artifacts and locked dependencies; stated authorized scene/asset counts; separate pose/skin/upload/CPU-frame/GPU timing where measurable; memory and latency at 32/64 avatars across quality tiers. |
| Target devices | Physical iPhone/iPad Safari, Android Chrome and desktop browser/native runs with device, OS, browser, backend and driver recorded. Hosted software rendering remains a separate evidence class. |
| Integration cost | Demonstrated consumption of A/B/E contracts, complete lighting/material/content paths, input/UI composition, asset ownership, build size and maintenance implications. |

Do not award an engine a pass for an unavailable backend or score a startup
failure as slower performance. Resolve test-harness failures before interpreting
its measurements. GPU/source mismatches require either fixes or a documented
compatibility decision; widening a threshold needs a specific reason and retained
raw evidence.

## Follow-on implementation and selection record

The `d247ebb` run closes the prior fixture discrepancies for both WebGL2
adapters, with all six version 2 scenes passing the unchanged color/exact-ID and
lifecycle gates. It does not prove arbitrary coplanar production scenes.
The continuation closes bounded diagnostic stages, asynchronous GPU selection,
and all twelve actual raw WebGPU copies for its recorded browser and artifact.
Ordinary scene presentation still requires a passing run in a supported
environment, retaining the independently reproduced hosted-browser presentation
failure. Continue using committed engine/npm locks and unchanged acceptance
thresholds. Production engine adoption also requires representative real
provider/content workloads, complete production-pass integration for the chosen
adapter, release performance comparison, and the physical-device matrix.
The separate existing-client implementation and its gates are recorded by the
global integration owner. Synthetic engine fixtures and OS null-audio results
do not substitute for those engine-selection acceptance conditions.

The eventual selection record should name the chosen engine and supported
backend policy, link the exact builds/locks and captures, compare correctness,
lifecycle and measured performance, and describe any capabilities requiring
additional implementation. Until those conditions are met, both adapters remain
comparison probes and the global integration owner has no adopted engine change.
