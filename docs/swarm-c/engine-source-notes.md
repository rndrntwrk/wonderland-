# Engine adapter source notes

## Version and evidence boundary

The adapters use Bevy **=0.19.1** and Fyrox **=1.0.1**, in independent Cargo
workspaces with Rust **1.95.0**. The authoritative core and the shared fixture
remain independent of the engine dependency graphs. The local reference
environment has Rust 1.75 and does not contain either engine's dependency set.
Engine compilation and browser execution therefore run in the isolated CI jobs.

These notes describe released APIs and the adapter's conversion decisions. They
do not substitute for compiler output, screenshots, exact ID-pixel comparisons,
physical-device runs or performance measurements.

At published commit `f6f78be1fef247f2db47e19f56d94054f0c9e88c`,
[run 37353615543](https://github.com/rndrntwrk/wonderland-/actions/runs/37353615543)
built all five engine variants. Both native software-runtime jobs passed,
including nine Bevy tests and twelve Fyrox tests. All three browser jobs still
failed image acceptance. Both WebGL2 variants passed all six color comparisons,
DPR/resize and lifecycle checks but retained a few exact-ID discrepancies in
Full2D/Hybrid2D at 64 avatars/DPR 2. WebGPU passed measured capture geometry,
lifecycle and its separate device diagnostic, while page content remained in
the recorded canvas images. The complete commit-specific results and limits are
in [VERIFICATION.md](VERIFICATION.md). No production engine is selected.

## Released source used

| Area | Primary source |
|---|---|
| Bevy material trait and sorting bias | [material.rs, v0.19.1](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_pbr/src/material.rs) |
| Bevy camera compositing component | [components.rs, v0.19.1](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_camera/src/components.rs) |
| Bevy camera extraction and target format | [camera.rs, v0.19.1](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_render/src/camera.rs) |
| Bevy clear-color conversion and render attachments | [view/mod.rs, v0.19.1](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_render/src/view/mod.rs) |
| Bevy final display conversion | [blit.wgsl, v0.19.1](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_core_pipeline/src/blit/blit.wgsl) |
| Bevy transparent ordering | [core_3d/mod.rs, v0.19.1](https://github.com/bevyengine/bevy/blob/v0.19.1/crates/bevy_core_pipeline/src/core_3d/mod.rs) |
| Fyrox plugin lifecycle and GameResult | [plugin/mod.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-impl/src/plugin/mod.rs) |
| Fyrox materials and property bindings | [fyrox-material/src/lib.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-material/src/lib.rs) |
| Fyrox shader schema and disabled passes | [shader/mod.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-material/src/shader/mod.rs) |
| Fyrox custom LDR hooks and target formats | [renderer/mod.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-impl/src/renderer/mod.rs) |
| Fyrox reusable draw bundles and uniform uploads | [renderer/bundle.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-impl/src/renderer/bundle.rs) |
| Fyrox mandatory Forward tone mapping | [hdr_map.shader, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-impl/src/renderer/shaders/hdr_map.shader) |
| Fyrox renderer statistics | [renderer/stats.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-impl/src/renderer/stats.rs) |
| Playwright element screenshot bounds | [screenshotter.ts, v1.62.1](https://github.com/microsoft/playwright/blob/v1.62.1/packages/playwright-core/src/server/screenshotter.ts) |
| Fyrox GL clear values | [framebuffer.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-graphics-gl/src/framebuffer.rs) |
| Fyrox WASM canvas allocation and native-only resize | [server.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-graphics-gl/src/server.rs) |
| Resolved winit physical canvas sizing | [resize_scaling.rs, v0.30.13](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/web/web_sys/resize_scaling.rs) |
| Observed Chromium version's WebGPU software pixel-test configuration | [pixel_test_pages.py, 151.0.7922.34](https://github.com/chromium/chromium/blob/151.0.7922.34/content/test/gpu/gpu_tests/pixel_test_pages.py) |
| Fyrox external-resource registry format and default path | [registry.rs, release source](https://github.com/FyroxEngine/Fyrox/blob/f916d01304d945271b6d7d70569c1333f563a34f/fyrox-resource/src/registry.rs) |

The Fyrox commit above is the release-exact source pinned by the prior engine
discovery report. API choices use that source, including typed scene handles,
current material resources and plugin methods returning GameResult. Older
tutorial signatures are not used.

## Shared scene conversion

Both adapters call wonderland-engine-fixture and upload the same immutable
FixtureScene. Core matrices are column-major, column-vector and right-handed.
The upload layer applies a model matrix once to each mesh's positions and its
inverse transpose to normals. It preserves UVs, source vertex colors and triangle
indices. Invalid geometry, images, material values, unsupported lit materials or
single-sided materials
are rejected before the adapter replaces the active fixture.
Current fixture meshes are explicitly unlit and double-sided. Matched source
winding and culling for single-sided production materials remain a separate gate.

The fixture supplies CPU-skinned avatar geometry that preserves the original
dual-local-position skinning semantics. An engine skeleton or animation clock
does not replace those outputs. Explicit mode and tick commands build another
fixture; neither rendering nor audio advances the authoritative simulation.

The camera looks down the core's negative Z axis. Fyrox's scene camera uses a
positive local Z look vector, so its node orientation is converted separately.
The custom shaders still receive the core's exact view-projection matrix. Bevy
changes core clip depth z to w-z for its reverse-depth attachment. Fyrox changes
z to 2z-w for OpenGL clip coordinates. Both use the same screen-space sprite
rectangles in the fixed 640 by 480 fixture viewport.

## Sprite depth, alpha and color

Color images are raw UNORM textures sampled nearest. The upload layer packs raw
depth q into the red auxiliary channel and effective alpha into green. A supplied
mask replaces image alpha. The shader reconstructs depth as:

    back + (1 - q / 255) / 0.4 * (front - back)

With normalized UNORM sampling, q/255 is already the sampled red value. Bevy
reverses the reconstructed depth once; Fyrox writes the forward value. Out-of-range
depth is discarded. Color coverage requires byte alpha greater than 2, while
the ID visualization requires byte alpha at least 26. Source room color policies
and the source gamma lighting transfer run before premultiplication.

Source-space alpha blending requires gamma-encoded colors to remain encoded
during the blend itself. In Bevy 0.19.1, CompositingSpace::Srgb selects an
Rgba8Unorm intermediate and converts the completed image for the display in its
final blit. The custom shader consequently emits encoded source RGB directly.

The actual checkpoint-4 WebGL2 run rejected that target because Bevy requests
alternate sRGB texture views, while the WebGL2 adapter lacks VIEW_FORMATS.
The WebGL2 variant therefore also sets Hdr to select Rgba16Float storage, which
the released target allocator creates without alternate view formats. The
CompositingSpace::Srgb tag still preserves encoded blending and final display
conversion. Tonemapping::None exits the released tone-mapping node before any
transform. This path needs the same color/ID gate as every other backend;
floating-point storage is not itself proof of equivalent output.

Fyrox's ordinary Forward path always applies ACES luminance tone mapping,
including when exposure is Manual(1.0). It would therefore change both source
colors and exact ID bytes. The adapter disables the built-in Forward shader pass
and registers a WonderlandSource pass through SceneRenderPass::on_ldr_render.
It clears the RGBA8 LDR target and depth, then draws the existing engine bundles
after tone mapping and FXAA. The shader emits encoded RGB directly and uses
premultiplied blending. This also avoids the HDR path's intermediate alpha
quantization.

Both adapters preserve the fixture's draw order. Bevy uses the material sorting
bias across the representative camera's bounded geometry; the bias affects draw
ordering, not fragment depth. Fyrox orders its distinct-material bundles by scene
insertion order. These disposable engine handles never become game identities.

The output transfer, clipping edges, attachment quantization, depth equality and
GPU implementation still require actual color/ID screenshot comparison.

## Identity, reset and loss

The GPU ID visualization encodes a transient 24-bit index. The host's idMap maps
that index to object_id plus generation. Ownerless architecture writes opaque
black ID and depth, so it occludes selectable entities behind it.

Interactive selection currently calls the shared CPU reference and resolves a
FrameStore pick ticket. This is labeled cpu-reference-generation-checked.
Asynchronous engine ID-buffer readback remains pending. The browser gate separately
reads the actual GPU ID visualization screenshot and compares its stable interior
bytes with the independent reference, including ownerless occlusion.

Fixture replacement preserves the FrameStore reset counter and invalidates old
tickets even when the new fixture is byte-identical. Bevy removes owned mesh,
material and image assets. Fyrox removes the old scene and flushes this isolated
probe's renderer caches. Owned-handle counts and actual renderer cache statistics
are exposed where available; they are not physical GPU memory-byte measurements.

The host records simulated loss separately from observed WebGL context events
and WebGPU device.lost. Actual loss suspends input and audio. A WebGL restore event
does not claim resource reconstruction; recover reloads the application. Real
backend observations come only from contexts and devices created by the engine.
A request or an independent capability probe cannot set actualBackend.

## Browser and native evidence

The shared host exports window.__wonderlandProbe for explicit mode/tick/pass
commands, bounded fixture-pixel selection, readiness, backend, scene hash, audio,
resource observations, resize, suspend and loss controls. Snapshot calls do not
increase submission or update counts. Canvas input is gated by DOM focus.
Browser audio is unlocked by an actual button gesture through the shared audio
adapter, using bounded synthetic PCM.

Bevy native logs the adapter selected by its renderer, separately from render
schedule visit counts. Fyrox counts completed custom passes that issued fixture
draw calls. Neither observation proves nonempty pixels. Browser automation checks
actual submissions, nonshared WASM, no cross-origin isolation, all three views
with 32 and 64 avatars, DPR/resize, focus, cancellation, resource resets, audio
lifecycle and actual loss/reload recovery.

Headless SwiftShader/Mesa evidence is a software smoke and parity gate. Physical
desktop, iPhone/Safari, iPad and Android GPU acceptance, speaker output and
representative performance remain separate gates.

## Capture bounds and startup diagnostics

Playwright 1.62.1 passes element bounds through enclosingIntRect before capture.
The host translates its canvas container to integer document pixels. Recorded
checkpoint bc529fd4 screenshots included page header/control content where the
fixture was expected. This did not prove that Chromium ignored the requested
origin; the correctly located canvas could contain malformed contents. The
runner now retains a full-page
PNG and extracts the measured document-space rectangle with exact RGBA row copies.
It checks the full PNG dimensions, integer CSS/device origins and extents, crop
bounds, backing dimensions for IDs, and unchanged document/canvas geometry after
capture. A screenshot at the wrong origin cannot establish renderer parity.

Color comparison remains at 640 by 480 CSS pixels with the original limits.
Discrete ID bytes use physical device pixels and a separately rerasterized CPU
reference selected by exact width/height from the manifest's idReferences array.
The supported grids are 640 by 480 and 1280 by 960. A fixed 5 by 5 physical CPU
neighborhood defines stable samples, with zero permitted byte mismatches. CPU
ownerless finite-depth pixels must occlude with ID zero; sparse pure-background
samples are chosen by sample-grid ordinals. The earlier coordinate-modulo phase
could never select background and is covered by a corrupt-background regression.
GPU ID maps must have unique valid game identities and injective positive 24-bit
indices. Interactive CPU selection uses an independent logical 640 by 480 point.

Fyrox 1.0.1 allocates its WASM canvas backing store once during graphics creation.
Its later GraphicsServer::set_frame_size only resizes native surfaces. The adapter
therefore matches the canvas backing dimensions to renderer.get_frame_size before
each browser render, writing the attributes only when the size changes. The
renderer still receives its physical dimensions through winit's ordinary resize
events. Published metrics expose the renderer dimensions, window dimensions and
window scale factor independently of the host's CSS/backing measurements.

The resolved winit 0.30.13 uses ResizeObserver.devicePixelContentBoxSize where
available. Browser DPR emulation can disagree with this physical pixel box, so
the runner starts a separate Chromium process with --force-device-scale-factor
for each DPR and checks a plain canvas's CSS and physical pixel boxes before
loading the engine. This diagnostic creates no graphics context. A mismatch
fails the density check rather than accepting a differently scaled image.
Checkpoint bc529fd4 passed both DPR/backing resize checks on all three browser
backends. Actual WebGL2 color comparisons passed in all six scenes per engine.
The remaining encoded-ID errors were measured through CSS-downsampled DPR2 PNGs;
the physical capture/reference correction subsequently ran at `f6f78be`.
At that revision both WebGL2 variants passed all color cases, DPR/resize and
lifecycle checks. Full3D physical-ID checks passed, but Full2D/64 and
Hybrid2D/64 retained 3/3 mismatches for Bevy and 5/4 for Fyrox. The reference and
captured IDs now share physical dimensions without categorical resampling.
Those remaining differences require diagnosis against the actual shader/depth/
coverage outputs; no exception to the zero stable-interior mismatch rule is made.

The host emits bounded bootstrap records for module loading, WASM initialization,
engine run, adapter request/result, device observation, canvas configuration and
initial fixture publication. WebGPU device observation also occurs when the
engine configures its own GPU canvas. Null adapters and missing navigator.gpu
become explicit startup failures. These diagnostic hooks do not claim that a
previous timeout has been resolved. In checkpoint bc529fd4, WebGPU reached engine
readiness, completed lifecycle checks and passed the independent mapped-pixel
diagnostic. Its malformed captured scene content still prevented parity acceptance.
The `f6f78be` full-page screenshots passed the new crop/geometry checks, but DOM
controls were visible inside the measured canvas region and the browser job
recorded 15 image failures. Independent review verified all twelve crops as exact
byte extraction from the retained full-page PNGs, with identical DPR 1 color/pick
images and page controls visible at DPR 2. Those checks establish extraction
correctness, not the correctness of the canvas contents or a definitive origin/
compositor root cause. Direct engine-target readback is not yet available, so
the evidence does not isolate the engine's pixel output
or establish its parity. Implementing that independent observation and resolving
the failures remain C work.

Checkpoint 4 reproduced WebGPU loss on the same device that configured the
engine canvas, without an observed JavaScript GPUDevice.destroy call. The runner
now retains bounded Chromium process stderr and performs a separate WebGPU clear
and mapped-pixel diagnostic after both engine runs. That diagnostic cannot warm
the engine's cold start, set its observed backend, or satisfy renderer parity.
HTTP error responses are also recorded with their actual URLs instead of
assuming that an unidentified browser 404 was a favicon.

Checkpoint 5's process log isolated the WebGPU startup failure to Chromium's
display integration: SharedImageBackingFactory could not allocate the WebGPU
swapchain image, and the separate 4 by 4 diagnostic reproduced the same error
without Bevy. The runner configuration follows Chromium 151.0.7922.34's own
VulkanSwiftShader pixel-test flags: Vulkan and SwiftShader are selected for the
display compositor, ANGLE and WebGPU together, with a disabled Vulkan surface
for the headless path. Checkpoint bc529fd4 observed the actual WebGPU device and
successful image creation, and `f6f78be` again passed the separate device
diagnostic. These independent device checks do not satisfy the engine's failing
color/ID capture gate.

Phase records and an eight-minute per-phase watchdog bound otherwise unbounded
page evaluation and graphics teardown without imposing a short total-run budget.
Expiration preserves reports, console and process diagnostics before terminating
only the Chromium process launched by this runner. The completed Fyrox software
browser run lasted about thirteen minutes, with all lifecycle checks passing.

The URL-aware HTTP log identified Fyrox's remaining 404 as
data/resources.registry. Fyrox's released loader expects a RON UUID-to-path map
there. The host now ships an empty map because every resource in this fixture is
embedded. A native regression compares that file with Fyrox's own serialization
of an empty registry. External-resource integration must populate this map when
it is introduced; the runner continues to fail unexpected HTTP errors.
