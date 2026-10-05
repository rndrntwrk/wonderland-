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

The current published comparison is commit
`f6f78be1fef247f2db47e19f56d94054f0c9e88c`, exercised by
[run 37353615543](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543).
Both native adapters and all three browser variants compiled. Both native
software-renderer jobs passed, including nine Bevy tests and twelve Fyrox tests.
The separate reference gate passed 374 Rust tests and all 18 complete
native/WASM records; locked CPAL tests and actual ALSA-null execution also passed.
These are separate evidence classes, recorded in the
[verification ledger](../swarm-c/VERIFICATION.md).

| Browser candidate | Current evidence at f6f78be | Selection consequence |
|---|---|---|
| Bevy WebGL2 | All six colors, DPR/resize and lifecycle checks passed. Full2D/64 and Hybrid2D/64 retained 3 physical-ID mismatches each; Full3D ID comparisons passed. | Required exact-ID acceptance remains failed. |
| Fyrox WebGL2 | All six colors, DPR/resize and lifecycle checks passed. Full2D/64 and Hybrid2D/64 retained 5 and 4 physical-ID mismatches respectively; Full3D ID comparisons passed. | Required exact-ID acceptance remains failed. |
| Bevy WebGPU | Capture geometry, lifecycle and the separate mapped-pixel diagnostic passed. The runner recorded 15 image failures, with DOM content visible in the measured canvas region. | Clean engine pixel output has not been independently established; image acceptance remains failed. |

All three browser jobs therefore remain failed. Independent review verified all
twelve WebGPU crops against the retained full-page PNGs byte-for-byte. Correct
extraction does not prove correct canvas contents: identical DPR 1 color/pick
images and page controls at DPR 2 remain unexplained. The earlier origin-error
inference is unproven, and direct engine-target readback is not yet available to
isolate the renderer's output. The remaining WebGL2 IDs require diagnosis at
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
CPU reference. The interactive pick path remains a generation-checked CPU pick.
Full advanced lighting, production content, day/night facade atlas integration
and the live city client are outside this small fixture. The separate CPU
derivative worker does produce normalized day/night PNG atlases, and the
normalized FSOm adapter emits ordered multi-material/stencil commands. Their
library/source tests do not implement the corresponding engine passes or
complete world-view composition.

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

Resolve the exact-ID discrepancies in both WebGL2 adapters and establish a clean,
independent observation of the WebGPU engine target. Then rerun the complete
browser rendering and lifecycle gates with the committed engine/npm locks and
unchanged acceptance thresholds. Complete C's lighting/material/picking/client
composition, integrate real provider/content inputs, and run the physical-device
matrix. These are concrete remaining implementation and acceptance conditions;
the passed synthetic reference and OS null-audio results do not close them.

The eventual selection record should name the chosen engine and supported
backend policy, link the exact builds/locks and captures, compare correctness,
lifecycle and measured performance, and describe any capabilities requiring
additional implementation. Until those conditions are met, both adapters remain
comparison probes and the global integration owner has no adopted engine change.
