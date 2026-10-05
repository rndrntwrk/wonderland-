# Swarm C verification

This ledger records what ran and what each result proves. It intentionally
separates historical local evidence, commit-specific CI, synthetic reference
comparisons and physical acceptance. Do not carry a passing result over to a
modified commit without rerunning the affected checks.

## Recorded evidence

Evidence snapshot: 2026-10-05. Source baseline:
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The renderer checkpoint is `95c45b0bdc514669e616648d61258c33563adc5b`.
Its [reference job 111875220500](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639/job/111875220500)
repeated the native/WASM, source-codec/native-audio and pinned-A successes.
The combined reference job still failed because the already-reviewed audio-test
and formatting corrections were not part of that renderer-only checkpoint.
The library correction batch resolved those failures and received the successful
combined CI result recorded below. The later adapters received the expanded
reference and native-device successes recorded below. The current published
verification revision is `d247ebb94d1d4de79247983b4f62fffb7e06427d`.

## Reference and native execution at d247ebb

[Reference job 111926068709](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926068709)
completed successfully at `d247ebb94d1d4de79247983b4f62fffb7e06427d` in
[run 37358255818](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818).
All formatting gates and **377 Rust package tests** passed: core 73, iso 60,
geometry 105, avatar 35, audio 59, fixture 16, native transport 25 and facade
worker 4. Replay's empty test targets also passed. Actual Node execution passed
19 audio, six host and six GPU-readback protocol tests, plus the PNG/ID comparator
self-test. All 11 Python audio tests and all 32 unchanged-C# XA/UTK complete-WAV
comparisons passed.

All 18 complete native/WASM observation records matched exactly. The WASM
SHA-256 is `1216468ac77c092d8b7b62ea5da9af6ec2ee2846a878b749202b25e54b9f29b4`.
The hosted reference measured 1,506.966942 ms and 21,299,200 memory bytes; these
are synthetic algorithm observations, not GPU performance measurements.

The actual pinned-A probe executed 60 ticks with C absent and at 30/60/120 Hz.
Every state hash and ordered event matched, with two genuine cues and zero
duplicate cues in each sampled cadence. The
[reference evidence summary](evidence/reference-d247ebb.json) retains exact
package counts, source/runtime results, fixture-version identity and full records.

All six version 2 scene/reference hash pairs match the independently reviewed
local fixture output. They deliberately differ from version 1 because crowd
placement and content identity changed; no old-image equivalence is claimed.
The artifact's manifest records logical 640×480 and physical 1280×960 ID grids,
without including the raw ID/depth buffers. The separate derivative worker again
produced six PNGs and eight byte-identical repeat files, rejected five malformed
requests and preserved an existing output directory. Its artifact hash remains
`99796348fce39734e1e89f44e3c4eacf9a05e94fc58f26cc79fe3c19b3fc2800`.

Both native engine jobs also completed successfully:
[Bevy 111926069218, nine tests](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069218)
and [Fyrox 111926069368, twelve tests](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069368).
These are native software-renderer execution results. Native pixel parity and
physical-GPU qualification remain separate.

[Native-audio job 111926069292](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069292)
passed all four configuration tests with the retained CPAL lock and executed the
real OS ALSA-null stream. The device used 48,000 Hz stereo F32, a 1,024-frame
buffer, a 2,048-frame ring and 256-frame mixing blocks. The smoke recorded 9,606
callbacks, 18,432 copied frames, two completed voices and zero device errors.
Live loop suspend/resume, live stop/reset and stale-session rejection all passed.
Its 1,893,632 underrun frames came from the unpaced null device;
`physical_output_verified` remains false. The
[native-audio evidence](evidence/native-audio-d247ebb.json) records exact logs,
hashes and the unchanged lock. This run does not qualify physical speakers,
physical hot unplug, other platform backends or production-load latency.

## Browser execution at d247ebb

The [retained engine evidence](evidence/engines-d247ebb.json) records all five
engine jobs, exact check results, native/browser reports, diagnostic results,
artifact hashes and independent review provenance for this revision.

| Browser job | Observed result at d247ebb |
|---|---|
| [Bevy WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069307) | Passed all six scenes with zero failures: color, exact physical-ID comparisons and the complete lifecycle gate. |
| [Fyrox WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069294) | Passed all six scenes with zero failures: color, exact physical-ID comparisons and the complete lifecycle gate. |
| [Bevy WebGPU](https://github.com/rndrntwrk/wonderland-/actions/runs/37358255818/job/111926069461) | Failed with 17 reported failures: 15 ordinary scene-image failures and two aggregate raw-readback diagnostic failures. All 16 ordinary lifecycle checks passed; the two added aggregate diagnostic entries failed. |

Both WebGL2 reports record `software-browser-checks-passed` and
`rendererQualified: false`. They close this revision's hosted software gate for
both backends using fixture version 2; physical-device, complete-client and production-performance
acceptance remain separate. Source LessEqual and the exact zero-ID
criterion remain unchanged; these fixture results do not prove general
coplanar production-scene parity.

Each WebGL2 variant passed 38 ordinary checks and all 16 ordinary lifecycle
checks, including 52,719 exact sampled ID centers and 15,167 ownerless occlusion
centers across the six scenes.

The WebGPU report contains 18 lifecycle/diagnostic entries: 16 passing ordinary
lifecycle checks and two failed aggregate engine-output diagnostics. That list
must not be reported as 18 passing lifecycle checks. The diagnostic distinguishes actual engine output, canvas snapshots and
page presentation; its passing parts do not override the failed ordinary gate:

| Observation | Actual result | Limit |
|---|---|---|
| Six direct engine-canvas ID snapshots | All six matched the physical CPU oracle: 52,719 sampled pixels, zero ID mismatches. | These are direct canvas PNGs, separate from page screenshots and raw mapped-texture copies. |
| Raw GPU copies | 3 of 12 completed. Full2D/32 color passed every unchanged color limit, with RMS 2.8075; its ID copy matched 3,799 samples with zero mismatches. Full2D/64 color bytes were retained. | Nine copies timed out. The 64-avatar physical color copy had no matching physical color oracle, so no parity score is claimed for it. |
| Independent 4×4 clear without Bevy | Mapped GPU bytes and direct canvas PNG were `[64,128,191,255]`; the page crop showed background `[153,68,47,255]`. | This reproduces a failure of the hosted browser's presentation path without the engine. It does not close the engine's incomplete copy diagnostic or ordinary scene gate. |

The retained [direct canvas PNG](evidence/webgpu-d247ebb/clear-canvas.png),
[page crop](evidence/webgpu-d247ebb/clear-crop.png) and
[full-page PNG](evidence/webgpu-d247ebb/clear-page.png) make that minimal
GPU/canvas-versus-presentation discrepancy independently inspectable.

The independent clear provides stronger evidence than the earlier origin
inference: correct GPU/canvas content can fail to appear in page presentation in
this environment. The ordinary WebGPU image gate remains failed, and incomplete
mapped copies remain visible as failures. The result is not promoted to complete
WebGPU rendering, presentation, performance or physical-device acceptance.

The nine generic readback deadlines do not identify whether acquisition, copy
submission, error-scope settlement or mapping stalled. The concrete diagnostic
follow-up is bounded per-stage progress capture for those operations. These
reports alone do not establish a source defect or justify a larger timeout;
the reviewed source remains at `d247ebb`.

## Reviewed correction published at d247ebb

The follow-up separates three issues without changing renderer shaders, source
`LessEqual`, color tolerances or the zero-ID-mismatch criterion. Fixture version 2
places the synthetic crowd using the maximum skinned horizontal footprint over
its full animation cycle. This removes unintended physical interpenetration
between neighboring mannequins while retaining the same lot, camera, 32/64
workloads and dedicated coplanar-depth regression. The
[version 1 counterexample](evidence/fixture-v1-coplanar.json) remains recorded;
changing that input does not establish general coplanar GPU agreement.

The [independent version 2 review](evidence/fixture-v2-review.json) built 448
scenes, exercised all 60 phases across the six mode/count combinations, and
checked 120,960 pairs of bounds. Minimum measured horizontal separation was
1.002498627 world units, with a minimum lot margin of 0.125. Prefix identity,
fixed placement across ticks/views, wrapped ticks, limits and repeat hashes
passed. The local fixture suite passed 16 tests, including three new regressions;
both existing coplanar regressions remain. Independent strictly-nearer ownerless
geometry still occluded avatars and sprites in every tested mode/count case.
This validates the synthetic input correction without removing occlusion from
the workload or asserting arbitrary coplanar GPU rasterization parity.

The host uses integral heading dimensions and relative layout offsets with
`transform:none`, avoiding the fractional compositor translation implicated in
the extra Fyrox edge values. Together with fixture version 2, this revision
passed both WebGL2 variants' unchanged exact-ID and color gates. That combined
result does not separately attribute every prior discrepancy to one correction.

WebGPU diagnostics retain the ordinary scene screenshots before adding
`COPY_SRC` to the engine surface. They then record actual submitted-texture
copies, direct canvas snapshots and page presentation separately. Copy validation
errors reject before mapped bytes can be mistaken for engine output; timeouts,
reconfiguration and capture failures preserve the evidence already collected.
The [independent protocol review](evidence/gpu-readback-protocol.json) passed 16
local Node tests. These tests use mock GPU devices and actual host-function
invocations; they do not prove GPU pixel or presentation parity. The affected
checks executed in the exact `d247ebb` run above. The passing WebGL2 results and
partial WebGPU evidence do not overwrite the failed WebGPU gate. The f6f78be
failures below remain failed historical results.

## Reference and native execution at f6f78be

[Reference job 111910339991](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910339991)
passed at `f6f78be1fef247f2db47e19f56d94054f0c9e88c` in
[run 37353615543](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543).
All formatting gates and **374 Rust package tests** passed: core 73, iso 60,
geometry 105, avatar 35, audio 59, fixture 13, native transport 25 and facade
worker 4. The fixture count includes the six physical-resolution regressions
introduced after `bc529fd4`; the historical 368-test result remains below.

All 18 complete native/WASM observation records matched exactly, with zero host
imports and non-shared memory. The WASM SHA-256 is
`aa0d98ed2424a7f5078467e1c6f722fafe37f887c51f0fb9276383bd730e55b5`.
The hosted reference measured 862.730868 ms and 21,364,736 memory bytes. All 32
unchanged-source XA/UTK comparisons matched complete WAV bytes; 19 Node audio
tests, five host tests, the PNG/ID comparator self-test and 11 Python audio tests
passed. The genuine pinned-A probe preserved every state hash and ordered event
over 60 ticks with C absent and sampled at 30/60/120 Hz. The
[reference evidence summary](evidence/reference-f6f78be.json) retains the exact
records, hashes, counts and source/runtime results. Reference execution does not
measure GPU performance or prove complete client integration.

The six physical-reference manifest entries contain logical 640×480 and physical
1280×960 ID outputs. All six logical scene/reference hash pairs match the prior
`bc529fd4` artifact. The PNG worker again produced the six derivative images and
byte-identical eight-file repeat output; its hashes and malformed-input checks
are retained in the same evidence summary.

Both native engine jobs also passed at this exact revision:
[Bevy, nine tests](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910340268)
and [Fyrox, twelve tests](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910340397),
followed by bounded software-renderer execution. Their
engine startup/draw evidence does not establish native image parity or physical
GPU qualification. Browser job outcomes are recorded separately below.

[Native-audio job 111910340211](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910340211)
passed with the retained CPAL lock, all four configuration tests and the actual
OS ALSA-null stream. It observed 10,744 callbacks, 18,432 copied frames, two
completed voices and zero device errors. The smoke's suspend/resume, stop/reset
and stale-session checks passed. Its 2,101,504 underrun frames came from the
unpaced null device; they are not a physical-device latency or load measurement.
`physical_output_verified` remains false. The
[native-audio evidence summary](evidence/native-audio-f6f78be.json) retains the
exact report, configuration and lock provenance. Speakers, physical hot unplug,
platform coverage and production-load acceptance remain open.

## Browser verification at f6f78be

All three WASM variants compiled and packaged in
[run 37353615543](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543).
All three browser jobs completed with failures; successful portions do not
override the required zero-mismatch image gate.

The [retained engine evidence](evidence/engines-f6f78be.json) records all five
engine job outcomes, native reports, browser color and physical-ID measurements,
check names, artifact hashes and screenshot provenance from this exact revision.

| Variant | Passing execution | Remaining image failure |
|---|---|---|
| [Bevy WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910340419) | All six color cases, physical DPR/resize, lifecycle checks and Full3D physical-ID comparisons passed. | Full2D/64 and Hybrid2D/64 at DPR 2 each had 3 stable-interior ID mismatches. |
| [Fyrox WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910340404) | All six color cases, physical DPR/resize, lifecycle checks and Full3D physical-ID comparisons passed. | Full2D/64 had 5 and Hybrid2D/64 had 4 stable-interior ID mismatches at DPR 2. |
| [Bevy WebGPU](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543/job/111910340515) | Capture geometry, lifecycle and the separate mapped-pixel device diagnostic passed. | The runner reported 15 image failures. Raw screenshots include DOM controls inside the measured canvas rectangle, so the extraction geometry check alone did not establish a clean rendering capture. |

The `f6f78be` WebGL2 results checked categorical IDs at their actual physical
resolution but retained the recorded few mismatches under the zero-mismatch
rule. The later fixture/layout correction and its successful WebGL2 runs are
recorded above. Independent review verified all twelve
WebGPU image crops byte-for-byte against the retained full-page PNGs. It also
observed identical color and pick images at DPR 1 and DOM controls inside the
measured canvas region at DPR 2. Exact extraction is a validated measurement
improvement; it does not establish correct canvas contents. The earlier claim
that Chromium ignored the requested element origin is not proven: malformed
contents within the correctly located canvas remain possible. The screenshots
do not isolate the renderer's output. At `f6f78be`, direct engine-target readback
was unavailable. The later `d247ebb` diagnostic implements the observation path;
its actual result is recorded separately above. Closing image parity remains
C implementation/verification work, not a provider or physical-device
gate. No production engine is selected.

## Expanded reference CI passed at bc529fd4

The [complete reference job 111899190511](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190511)
passed at `bc529fd4c08be5473b2da549f0aa80cebe17a45f`, covering the new FSOm,
derivative and native transport code. All formatting gates and 368 Rust package
tests passed: core 73, iso 60, geometry 105, avatar 35, audio 59, fixture 7,
native transport 25 and facade worker 4. Replay also built and its empty test
targets passed. The 19 Node audio tests, five host tests and PNG/ID self-test passed.

The independent PNG verifier regenerated six images with all eight output files
identical across a request round trip. Its artifact hash matched the reviewed
local/committed `99796348fce39734e1e89f44e3c4eacf9a05e94fc58f26cc79fe3c19b3fc2800`.
Five malformed envelopes were rejected and existing output was preserved.

All 18 actual native/WASM observation records matched exactly with zero host
imports and ordinary non-shared memory. The WASM SHA256 was
`9a18a9eee238a2875660b4d8defc4d7bd8bde476b256242a6afc2b42dcb98f29`.
The hosted reference measured 1,226.03655 ms and 21,364,736 memory bytes; these
are synthetic algorithm observations, not GPU performance measurements.

All 32 unchanged-C# XA/UTK comparisons matched complete WAV bytes, and all 11
Python/FFmpeg/FFplay tests passed. The actual pinned-A probe ran 60 ticks with C
absent and at 30/60/120 Hz; every state hash and ordered event matched, with two
genuine cues and zero duplicate cues per cadence. The
[retained evidence summary](evidence/reference-bc529fd.json) includes the full
18 records, exact hashes, package counts, source comparison and A probe evidence.

All five engine variants also compiled at this commit. Both native jobs passed:
[Bevy, nine tests](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190565)
and [Fyrox, twelve tests](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190420).
Native-device evidence is recorded separately below. Browser verification remains
separate; successful reference/native jobs do not override a failed image gate.

## Browser capture findings at bc529fd4

The three browser jobs completed with failures in their screenshot comparisons:

| Job | Passing execution | Remaining captured-image failure |
|---|---|---|
| [Bevy WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190578) | All six color cases, physical DPR/backing resize checks and every lifecycle check passed. | One ID mismatch in Full3D/64: a CSS-sized screenshot averaged categorical IDs from a 1280×960 backing buffer. |
| [Fyrox WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190430) | All six colors, both DPR/backing resize checks and every lifecycle check passed; no failed requests. The software-browser step took about 13 minutes. | Each of the three 64-avatar scenes had one ID mismatch in the reduced screenshot. |
| [Bevy WebGPU](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190626) | Startup, all lifecycle checks and the independent mapped-pixel device diagnostic passed; its exact pixel was `[64,128,191,255]`. | All six color/ID pairs failed; the captured images included page header/control content where fixture output was expected. This did not establish whether the requested origin was ignored or the canvas contents themselves were malformed. |

The subsequent capture measurement improvement retains a full-page device-pixel screenshot and
extracts the exact observed canvas rectangle. Encoded IDs are never resampled;
they use a freshly rasterized reference at the measured physical backing size.
Logical selection retains its separate 640×480 coordinate contract. Color
downsampling and exact-ID comparison are explicit separate operations.

The new fixture API and six physical-resolution regressions passed, bringing
the local fixture suite to 13. All six logical scene/output hash pairs match
the actual `bc529fd4` reference artifact. A separate direct comparison of 18 retained pre-change local
PPM/ID/depth files also matched byte for byte; those raw files were local output,
not downloaded CI buffers. Independent capture review passed nine Node
challenges, including production capture calls across sizes/DPRs, rejected
geometry drift, exact byte extraction and a formerly unreachable background
sampling case. Zero ID mismatches and the existing color thresholds remain
required. The correction is included in the 13-test fixture suite that passed
CI at `f6f78be`; actual browser image outcomes are recorded separately above.

## Combined reference CI passed at c6a8fbb

The [complete reference job 111885186958](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493/job/111885186958)
passed at commit `c6a8fbb70af6cc76e9363913159ecdff49ff27a7`. Every package
format/test step and all independent source/runtime probes succeeded. This closes
the audio-test and formatting failures recorded below for earlier checkpoints.

The run exercised core 46, iso 60, geometry 79, avatar 32 integration plus 3
compile-fail tests, audio 59 and the 6 shared-fixture tests. Actual Node browser
audio and host tests and the PNG/ID comparator self-test also passed.

All 18 native/WASM observation records were exactly equal, with zero host imports
and non-shared memory. The WASM artifact SHA256 was
`ff54e4efc2954a62c6d8c04edea1bbbbc443ef0bd466f9ca0004aae082b1f41c`.
Observed reference execution was 854.071 ms and 21,364,736 memory bytes. The
32 unchanged-source codec comparisons, 11 Python tests and pinned-A 60-tick
absent/30/60/120 Hz probe also passed, with two genuine cues per run and zero
duplicates. These reference timings are unrelated to physical GPU performance.

Both native engine jobs in the same run passed after all five engine variants
compiled successfully. All three browser jobs failed; their completed results
and the subsequent correction evidence are recorded below. Reference success
does not imply browser parity.

## Completed engine run at c6a8fbb

All results in this table come from
[run 37346178493](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493)
at the exact `c6a8fbb` commit above.

| Job | Result | Observed scope or failure |
|---|---|---|
| [Bevy native](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493/job/111885187390) | Passed | Build, 8 tests and native software-renderer execution. Physical-GPU and native pixel qualification remain open. |
| [Fyrox native](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493/job/111885187356) | Passed | Build, 10 tests and actual LDR shader/draw execution; the recorded native scene submitted 182 draw calls and 10,552 triangles. |
| [Bevy WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493/job/111885187228) | Built/packaged; browser gate failed | All six stable-interior GPU-ID comparisons had zero mismatches. The 32-avatar Full2D/Hybrid2D colors exceeded RMS 12; the other four color cases passed. DPR 2 resize sizing also failed. |
| [Fyrox WebGL2](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493/job/111885187287) | Built/packaged; browser gate failed | All three DPR 1 ID cases passed, with the same coplanar color discrepancy in two modes. DPR 2 had a renderer/backing-size mismatch; resize and a missing `resources.registry` also failed. |
| [Bevy WebGPU](https://github.com/rndrntwrk/wonderland-/actions/runs/37346178493/job/111885187352) | Built/packaged; browser gate failed | Device loss occurred before the second frame. A bare 4×4 WebGPU clear/readback also failed without Bevy; Chromium 151.0.7922.34 stderr identified SharedImage swapchain allocation failure. |

### Coplanar reference correction and same-capture recomparison

The captured GPU images showed a later coplanar ground floor while the CPU
reference retained earlier terrain. The reference default used strict Less;
both engines used the source LessEqual equivalent. The correction adds explicit
`DepthComparison::LessEqual` for the shared fixture and derivative renderer while
preserving strict Less as the core default. No epsilon or comparator tolerance
changed. Two core policy regressions and the new fixture coplanar regression
passed; the complete fixture suite passed 7 tests.

Regenerating CPU references from the same scene hashes and comparing the actual
run-6 GPU captures produced these results:

| Captured images | Corrected comparison |
|---|---|
| Bevy WebGL2, all six mode/count cases | Color and stable-interior IDs passed at the unchanged thresholds. Full2D/Hybrid2D with 32 avatars improved from RMS 13.064/12.859 to 2.464/2.725. |
| Fyrox WebGL2, all three DPR 1 cases | Color and stable-interior IDs passed. Full2D/Hybrid2D RMS became 2.138/2.550. |

This is a local recomparison of retained actual GPU captures, not a fresh passing
browser job. The new physical-DPR setup, Fyrox renderer/backing-size coordination,
resource registry packaging and Chromium Vulkan/SwiftShader configuration need
the next complete CI execution. Source and implementation choices are in
[core notes](core-source-notes.md) and [engine notes](engine-source-notes.md).

## Adapter extension batch: local tests and independent review

These additions postdate `c6a8fbb` and passed the expanded reference CI at
`bc529fd4` recorded above. The following local checks and independent reviews
were completed on 2026-10-05; their additional source/review probes have the
specific scope described in each row.

| Scope | Executed evidence | Remaining gate |
|---|---|---|
| Normalized FSOm objects | 26 new tests; complete geometry suite 105 debug and 105 release tests. Eight independent object challenges passed. Unchanged C# normal generation matched twelve f32 bit patterns; source world/blend/depth states were executed. The reviewed no-depth-attachment lightmap regression passed after correction. [Source evidence](fsom-source-notes.md). | B archive/texture/provider integration, actual engine shader/stencil execution and authorized patched-content comparison. |
| Derivatives and reference depth | Core 73 tests: original 46, 25 derivative regressions and 2 explicit depth-policy regressions. Worker 4 tests. Six independent lifecycle/decode/depth challenges passed; a coplanar case was reproduced failing before the correction and passing afterward. Expanded reference CI subsequently passed at `bc529fd4` and `f6f78be`. | Complete source/client preparation and GPU integration. |
| Source wall equations | Original C# comparison covered 23 atlas rectangles, 368 matrix coefficients and 115 transformed wall points. Maximum discrepancy was 0.000040875 atlas pixels against a 0.001 tolerance. | Real-content and GPU raster/blend/filter comparison. |
| Actual derivative PNGs | Six nonempty PNGs and three distinct day/night pairs; all eight output files were byte-identical on repeat. Five malformed requests failed gracefully and an existing output directory was preserved. Independent Pillow decoding checked all six PNGs, 4,982 bleed pixels and all used floor-cell borders. [Committed manifest](../../tools/swarm-c/facade-worker/fixtures/synthetic-verification.json). | Legacy lighting/shadows, source thumbnail centering/cropping, FSOF mesh/container/DXT5/consumer behavior and live city integration remain explicit algorithms. |
| Derivative memory ownership | Independent 512-view challenge measured 439,104 bytes against a 448,832-byte reservation. The final fixture reserved 3,732,912 bytes and retained 1,607,144 bytes of artifact payload. | These are bounded synthetic owned-memory observations, not total process or GPU memory measurements. |
| Continuous native audio transport | 25 debug and 25 release tests; 12 independent challenges passed. Allocation instrumentation observed zero allocation/deallocation in data-callback paths. Deterministic reset/suspend/fault races, completion backpressure, small-block/high-rate refill and minimum buffering were exercised. [Contract/tests](../../crates/audio-runtime/native/README.md). | The separate CPAL compilation/configuration/ALSA-null job passed at `bc529fd4`, as recorded below. Physical/platform/load qualification remains open. |

The derivative artifact SHA-256 is
`99796348fce39734e1e89f44e3c4eacf9a05e94fc58f26cc79fe3c19b3fc2800`.
The committed manifest retains effective-input, PNG and decoded-RGBA hashes.
Its 31,788,844 work units are an admission model, not an elapsed-time benchmark.
No adapter review is recorded as physical-device or live-provider acceptance.

## Actual native-audio CI passed at bc529fd4

[Native-audio job 111899190176](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093/job/111899190176)
passed at commit `bc529fd4c08be5473b2da549f0aa80cebe17a45f` in
[run 37350305093](https://github.com/rndrntwrk/wonderland-/actions/runs/37350305093).
The device workspace compiled against actual CPAL 0.15.3 and ALSA, all four
configuration tests passed, the real ALSA-null stream smoke passed, and the job
uploaded its logs and complete resolved dependency lock. Independent review
approved the observed backend execution. The durable
[native-audio evidence summary](evidence/native-audio-bc529fd.json) records the
test names, reviewed source blobs, artifact hashes and exact smoke report.

The default/null device negotiated 48,000 Hz stereo F32, a 1,024-frame fixed
device buffer, a 2,048-frame ring and 256-frame mixing blocks. The smoke recorded
10,663 callbacks, 18,432 copied frames, two completed voices and zero device
errors. Loop suspend/resume, live stop/reset and stale-session rejection all
passed. It reports `physical_output_verified: false`. Its 2,091,264 underrun
frames came from an unpaced null device, which can consume faster than wall time;
these checks establish native execution and lifecycle behavior, not physical
latency or real-time performance.

The retained [CPAL lock](../../crates/audio-runtime/native/cpal/Cargo.lock) is
28,738 bytes with SHA-256
`f76004178c0e2d06345d22db4e23fb76191b433d2aa55492ca347626a40ab693`.
Subsequent verification uses that graph with `--locked`. Physical speakers,
device unplug/reopen, platform configurations and production-load qualification
remain open. Unknown device buffer ranges remain an explicitly rejected
configuration. The expanded reference and both native engine jobs also passed at
`bc529fd4`; browser verification has its own outcomes and remaining corrections.

## Reviewed library correction batch

The following local checks and independent reviews ran on the correction files
on 2026-10-05 before the successful combined CI result above.

| Scope | Fresh result | Independent review and limit |
|---|---|---|
| Geometry | 79 debug tests and 79 release tests passed | 8 independent regression cases passed. All 21 runs of the unchanged C# simplifier matched ordered triangle indices and every output position/UV f32 bit. Permanent intermediate-schedule and output bowl regressions caught the arithmetic drift before correction. |
| Avatar | 32 integration tests and 3 compile-fail doctests passed | 6 independent regression/source-vector cases, cooker compilation and the genuine pinned-A probe passed after the identity getter migration. Retagging rig/clip construction identity is impossible through the public API; incompatible poses fail without partial mutation. |
| Audio | 59 Rust tests, 19 actual Node tests and 11 Python tests passed | Reviewer reran 8 focused Rust/Node/Python checks covering all four lifecycle/transaction findings and both cooker fixes. All 32 unchanged-source XA/UTK vectors again produced identical complete WAV bytes. FFplay used a dummy device; no physical output proof. |
| Formatting | Geometry, avatar, audio and iso formatting checks passed | The corrected `Stop { voice: second }` test now compiles. The combined package script subsequently passed at c6a8fbb. |

The source notes and committed regressions record the original equations and
specific fixes. Synthetic/source tests do not replace provider, device or complete
client acceptance.

## Historical CI evidence

| Check | Recorded result | Evidence and limit |
|---|---|---|
| Bevy native build and runtime | Passed at C commit `809ec200e186871970b2e4cabb06d7f6c08ecd9e` | [Run 37337020509, job 111854251163](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251163). Rust 1.95.0, hosted Linux software graphics, bounded native fixture run. No native image-parity or physical-GPU claim. |
| Fyrox native build and runtime | Passed at the same commit | [Job 111854251326](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251326). Same evidence class; the native report explicitly leaves hardware and image parity unqualified. |
| Bevy WebGPU build/package | Passed; browser job failed | [Job 111854251231](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251231). Actual WASM and matching bindgen package were built. Browser readiness timed out; no image qualification. |
| Bevy WebGL2 build/package | Passed; browser job failed | [Job 111854251466](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251466). Browser reached image comparison; fractional CSS capture geometry caused a dimension failure. No color/ID parity result can be inferred. |
| Fyrox WebGL2 build/package | Passed; browser job failed | [Job 111854251146](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251146). Same capture-dimension boundary. |
| Complete reference CI | Failed at both recorded checkpoints | [Earlier job 111854251030](https://github.com/rndrntwrk/wonderland-/actions/runs/37337020509/job/111854251030); [latest job 111864173654](https://github.com/rndrntwrk/wonderland-/actions/runs/37339944118/job/111864173654). At the latter, core/iso/geometry/avatar/fixture suites passed, while the new audio test's `Stop { second }` field typo blocked its test compilation and three formatting checks failed. Individual green probes do not override this combined failure. |
| A/C simulation authority boundary | Passed locally and in reference CI at `96066054` | [Probe and provenance](../../tools/swarm-c/avatar-integration/README.md) and the latest reference job: genuine pinned-A execution for 60 ticks with C absent and at 30/60/120 Hz. Every state hash and ordered event vector matched; two genuine A cues ran in each cadence and duplicate accepted ticks produced no extra cues/events. This is a synthetic native integration fixture, not a browser or real-content run. |
| Original-source XA/UTK comparison and native audio | Passed locally and in reference CI at `96066054` | [reference_probe.py](../../tools/swarm-c/audio-cooker/reference_probe.py): 12 XA and 20 UTK synthetic vectors produced exactly equal complete WAV bytes from unchanged C# and Rust decoders. All nine Python cooker/decoder/native-player tests passed, including actual FFplay with a dummy device. Audible output and a live native callback backend remain separate gates. |
| Avatar original-method vectors | Passed for the recorded hashed assembly | [Source result](../../tools/swarm-c/avatar-cooker/source-probe-results.txt) and [scope](avatar-source-notes.md). Extracted original methods, synthetic vectors and explicit tolerance; assembly version is 0.0.0.0. Historical MonoGame 3.6.0.1625 parity is not established. |
| Native/WASM presentation equality | Passed at `96066054` | [verify-native-wasm.sh](../../tools/swarm-c/verify-native-wasm.sh) executed all 18 records with exact equality, zero host imports and non-shared memory. WASM SHA256: `f443cd88401d3b49611e55ed8c2c26b395f187ac8e8cb5a69fb3ff329f4c1667`. Observed runner execution was about 1.14 seconds with 21,364,736 memory bytes; this is reference-algorithm evidence, not GPU/browser performance. |
| Physical devices and live providers | Open | No iPhone/iPad Safari, Android Chrome, desktop physical-GPU or speaker-output qualification, licensed-content cohort, or live E city/admission acceptance is claimed. |

Both native engine jobs also passed at `96066054` in
[run 37339944118](https://github.com/rndrntwrk/wonderland-/actions/runs/37339944118)
and at `95c45b0` in
[run 37343212639](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639).
The latter compiled and executed the new Bevy compositing and Fyrox LDR paths:
[Bevy job 111875220761](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639/job/111875220761)
passed 8 library tests and native Vulkan/llvmpipe startup;
[Fyrox job 111875220905](https://github.com/rndrntwrk/wonderland-/actions/runs/37343212639/job/111875220905)
passed 10 tests and native shader/draw execution. Native smoke still does not
establish pixel parity. Browser image parity remains open.

Later successful runs should be appended with their exact commit, command/job,
artifact and scope. Preserve a failed run when it explains a correction; do not
rewrite it as success because the next revision changed.

## Reproduce the pure-library and reference checks

Run from the C repository root with Rust 1.75.0, its rustfmt component, Node 24 and
the dependency locks available:

```sh
bash tools/swarm-c/verify.sh
```

This attempts independent formatting and test checks for all five libraries,
the fixture/replay packages, native audio transport and facade worker, generates
reference output, exercises the browser
audio and host-control tests, and runs the PNG/ID comparator self-test. A failure
does not prevent the other independent checks from reporting. The combined
command exits unsuccessfully if any check failed.

Reference output under `tools/swarm-c/output/reference/` includes color PPM,
little-endian game-ID and depth buffers, fixture hashes/mesh counts in
`manifest.json`, and reference PCM. These are synthetic CPU outputs. The
fixture uses 32/64 instances of a 14-bone, 312-vertex avatar, source-depth sprites,
architecture and a small city. It does not contain a licensed production scene.

## Execute native and ordinary WASM from the same source

Install the `wasm32-unknown-unknown` target for the same Rust 1.75.0 compiler, then:

```sh
bash tools/swarm-c/verify-native-wasm.sh
```

The script builds and executes both targets from the current checkout. The
comparator checks 18 records: three view modes, 32/64 avatars and ticks 0/15/30.
It compares the full observation records, including scene/reference/audio
hashes, geometry counts and picking observations. It also checks zero host
imports, non-shared WASM memory and rejection of invalid mode/count input.

Evidence is written to `tools/swarm-c/output/native-wasm/`: native JSONL,
verification log and a report with WASM SHA256, scenarios, elapsed time and
memory. Any exact mismatch fails. This is an algorithm/cross-target gate; it
does not measure an engine, GPU or browser compositor.

## Audio and original-source comparison

Install Python 3, Mono/mcs, mono-runtime and FFmpeg/FFplay, then:

```sh
bash tools/swarm-c/verify-audio.sh
```

The script builds the actual `audio-decode` binary, runs cooker/decoder/native
player tests, and runs the unchanged-source XA/UTK differential probe. It writes
`tools/swarm-c/output/audio/source-differential.json` plus logs. Source and
synthetic-vector hashes belong in that evidence; do not replace the original
decoder with a rewritten expectation.

The Python native-player tests use dummy output and buffered file playback. They prove process,
deadline, pause/resume/stop and decoding behavior, not audible speaker output or
a continuous low-latency callback backend. The new transport and CPAL checks below
exercise separate implementations. Browser unit tests use controlled
backend boundaries; actual AudioContext behavior is exercised separately by
the engine/browser runner.

### Continuous native transport and actual CPAL stream

The pure transport has no new registry dependency and runs independently:

```sh
cargo test --locked --manifest-path crates/audio-runtime/native/Cargo.toml
cargo test --locked --release --manifest-path crates/audio-runtime/native/Cargo.toml
```

The isolated CPAL workspace pins 0.15.3. Device-enabled CI uses Rust 1.95.0,
`pkg-config` and `libasound2-dev`. Use the dependency lock retained from the
successful device job above:

```sh
cargo test --locked --manifest-path crates/audio-runtime/native/cpal/Cargo.toml
bash crates/audio-runtime/native/cpal/run-alsa-null.sh
```

The smoke uses real OS/CPAL callbacks routed through an isolated ALSA null
configuration. It requires two one-shot completions, repeated live-loop copies,
continued silent callback activity while suspended, resumed copies, live stop/reset,
stale-session rejection and no device errors. Its result explicitly sets physical
output false. Null output can consume faster than wall time, so callback/underrun
counts do not establish physical latency or production scheduling performance.
Full setup and platform limitations are in the [device guide](../../crates/audio-runtime/native/cpal/README.md).

## Reproduce thumbnail and facade artifacts

The [worker guide](../../tools/swarm-c/facade-worker/README.md) describes the
normalized request protocol and all six image outputs. From a fresh output path:

```sh
cargo build --locked --manifest-path tools/swarm-c/facade-worker/Cargo.toml
python3 tools/swarm-c/facade-worker/verify.py \
  tools/swarm-c/facade-worker/target/debug/wonderland-facade-worker \
  /tmp/swarm-c-facade-verification
```

When `CARGO_TARGET_DIR` is set, use its `debug/wonderland-facade-worker` instead.
The destination must not already exist. The verifier independently checks PNG
CRCs, zlib termination, dimensions and RGBA hashes, then repeat equality and
malformed-request behavior. The source-controlled manifest records one actual
fixture run; rerun after any change to source math, material handling, layout,
identity, metadata or PNG encoding.

## Pinned A/C authority boundary

Use a separate checkout of the same repository at the exact published A commit.
The runner validates its HEAD and does not alter that checkout:

```sh
SWARM_A_PATH=/absolute/path/to/pinned-A RUSTUP_TOOLCHAIN=1.75.0 \
  python3 tools/swarm-c/avatar-integration/run.py tools/swarm-c/output/avatar-a
```

The temporary probe manifest reads A's libraries and existing synthetic fixture
helpers. It executes actual simulation transitions. Render sampling is read-only,
and source event order/cue counts are compared, not just a C-generated counter.
A's dependencies must be available before the runner's offline Cargo invocation.
The C workflow obtains the pinned provider checkout and fetches its locked
dependencies explicitly.

## Actual engine and browser checks

The [workflow](../../.github/workflows/swarm-c.yml) provisions isolated Rust
1.95.0, matching WASM target libraries and Linux graphics/audio dependencies.
Use an equivalent environment for local runs:

```sh
WONDERLAND_ENGINE=bevy WONDERLAND_BACKEND=native \
  bash tools/swarm-c/build-engine.sh
WONDERLAND_ENGINE=bevy \
  bash tools/swarm-c/run-native-engine.sh
```

Repeat with `fyrox`. For browser runs, select each of
`bevy:webgpu`, `bevy:webgl2`, `fyrox:webgl2` independently:

```sh
WONDERLAND_ENGINE=bevy WONDERLAND_BACKEND=webgl2 \
  bash tools/swarm-c/build-engine.sh
WONDERLAND_ENGINE=bevy WONDERLAND_BACKEND=webgl2 \
  bash tools/swarm-c/run-browser-engine.sh
```

The build wrapper retains compiler/source metadata, dependency resolution and
artifact hashes. Browser packaging requires a wasm-bindgen CLI matching that
engine's lock. The browser wrapper installs the pinned Playwright dependency
and actual Chromium. Both resolved engine Cargo locks and the npm lock were
committed at `95c45b0`; subsequent builds use those graphs. Bevy's WebGPU/WebGL2 feature sets are
mutually exclusive; do not use `--all-features`.

The browser runner checks actual backend observations, ordinary memory without
cross-origin isolation, submitted draws, same-scene color and GPU-ID screenshots,
input focus/cancellation, resize, suspension, AudioContext gesture/lifecycle,
repeated fixture release and actual device/context loss with application reload.
Interactive selection still uses the CPU reference with generation checks;
screenshot ID readback does not implement asynchronous GPU picking.

At the recorded runner revision, the scene matrix uses all three view modes with
32 avatars at DPR 1 and 64 at DPR 2. It is not the full count-by-DPR Cartesian
product. The declared comparison thresholds are zero stable-interior ID
mismatches, color mean absolute byte error at most 4, RMS byte error at most 12,
and at most 3% of channels differing by more than 8. Preserve raw screenshots,
metrics and the reason for any threshold change.

Outputs under `tools/swarm-c/output/<engine>-<backend>/` include build/native
reports or browser JSON, console records, screenshots and traces. CI uploads
these artifacts even after failure, with 14-day retention; retain the acceptance
evidence durably before those artifacts expire. A report's
`rendererQualified: false` remains deliberate: hosted software rendering does
not qualify physical production devices.
