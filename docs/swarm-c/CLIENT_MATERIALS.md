# Client source-material passes — W08.2 continuation

## Scope and source

This increment continues PR23 at `3dbb7f71d18c5ec999367d36feba2cceb1d1844c`.
It connects the existing `render-3d::objects::PreparedFsom::scene(Color)` commands
into `world-view` and the actual `world-gpu.mjs` owner. Previously the scene
builder omitted every model carrying a normal or portal depth mask.

The source-derived object command interpreter remains unchanged. This is a
presentation-only consumer, not an authority, simulation, content-provider,
dependency-version or shared-contract change. The prior PNG capture, import,
source identities, controls and other application screens are preserved.

## Implemented behavior

Normal masks now execute mark, far-depth clear and visible body passes. Portal
masks retain stencil through their body passes, draw the final group even with
its dynamic bit off, and clear stencil without rewriting depth afterward.
Projected clockwise and counterclockwise stencil operations remain independent.
Forced mask depth is applied before homogeneous clipping, matching the source
MaskFar shader. Source non-premultiplied blending applies SourceAlpha and
InverseSourceAlpha to both RGB and alpha, rather than substituting ordinary
over-compositing.

Depth-only commands write neither visible color nor selection identity. A
multi-part object has one frame-local GPU pick index across all of its visible
material passes. The Rust renderer still checks the exact frame generation and
live source target; no GPU index becomes a persistent object ID or permission.
All per-draw depth/stencil/color/blend state is reset explicitly. The normal
capture and actual context-loss recovery paths use the same renderer.

The local presentation packet is now schema 2 with an explicit optional pipeline
per draw. The browser continues to accept ordinary schema-1 packets but rejects
schema-1 packets carrying new pipeline fields, so older envelopes cannot silently
drop material behavior. Unknown operations, non-finite depths, malformed winding
faces, and invisible masks carrying textures or pick IDs fail before staging.
This packet is not the versioned A/B/F multiplayer protocol.

## Bounds and compatibility

Mask commands contribute to the expanded scene work/buffer budget. Prepared mask
geometry is shared per source model across its commands and instances. GPU
surface and upload limits remain 1,048,576 pixels and 128 MiB respectively;
per-command budget accounting includes the additional material state. The CPU
reference additionally retains one bounded stencil byte per pixel.

The CPU reference's prior callers retain their default behavior unless they opt
into an explicit pipeline. Existing reference digest fields remain unchanged:
color, depth and identity. Stencil is inspected directly in the new tests; an
old digest is not claimed to include it.

Frustum rejection is bypassed only for commands that force clip-space depth.
Clipping those commands against their original Z would incorrectly discard a
source MaskFar pass. Ordinary bodies and architecture retain their previous
culling. This follows shader semantics rather than changing source placement.

Texture filtering remains the existing nearest-clamped client/reference path,
and the pre-existing flat visual shading remains in this viewer. This increment
does not claim original linear/mipmap filtering, directional/disabled-room
lighting, lightmap blending or complete original material/capture parity. Those
require their own source adapters and qualification.

## Verified execution — 7 October 2026

Tested head: `d52689c8db00d66bb544f043efb55da1338fd74a`; tested tree:
`1276cc2229ff2e2ff62f970b96083c4e265d311b`. This final guide/evidence update
changes documentation only. Core source was published at
`4dd4a7eed31f0b1af41c5cf2ca7242a527fc2327`; the later correction only tightens
click selection in the application test, not the renderer.

[Scoped run 37555822859](https://github.com/rndrntwrk/wonderland-/actions/runs/37555822859)
passed **188 affected Rust tests**, zero failed/ignored, **32 Node tests**,
formatting, actual WASM client Clippy and the real Chromium WebGL2 gate. Strict
affected native Clippy also passed in the guarded publication run and locally.
The downloaded source archive matches all 22 locally tested modified files;
temporary transport/tooling/publisher files are absent.

The native tests include **1,872 fragment comparisons** against the unchanged
source-derived Rust object interpreter across winding, stencil, depth, discard,
blend and identity cases. This is not a new run of the original C# GPU. Initial
mask-admission/order/budget/sharing and protocol tests failed before implementation.
A separate failing regression caught distinct pick indexes for the visible passes
of one object; frame-local deduplication fixes it. The portal test requires
visible final-group pixels and a more visible unmasked control, so an empty
stencil cannot pass.

Six actual browser scenes retain default, rotated and cutaway baselines and add
normal, portal and mixed masks. They pass **155 exact GPU-pick comparisons**,
including masked object and final portal pixels. Six actual PNG exports match
the contemporaneous source buffer pixel-for-pixel. Twenty replacements,
interrupted transfers, delayed PNG completion, actual context loss/restoration
and disposal retain their resource/generation checks. Color limits remain MAE 4,
RMS 12 and fraction-over-eight 0.03; maximum observed RMS in the final run is 1.832027. No image or
identity threshold was weakened. Fixtures remain synthetic normalized documents,
not redistributed original game assets or representative load benchmarks.

### Actual release application

[Browser UI run 37555827077](https://github.com/rndrntwrk/wonderland-/actions/runs/37555827077)
passed the **full workspace tests**, browser audio and GPU/PNG tests, formatting,
strict native/WASM Clippy, optimized Trunk build, standalone GPU gate and actual
built-application gate. Its PR merge revision
`f413b4097d1ca4f3cc6ffda84bb37a85529e6fe7` has no file differences from the tested
head. Optional source/corpus dispositions remain in the workspace logs; ignored
cases are not counted as executed here.

The downloaded application report contains **22 recorded checks**, including
eight PNG exports and six application screenshots. It records zero browser
errors and zero HTTP error responses. Original source-lot entry, imports and
invalid-import retention, rotation, narrow layout, capture cancellation and
close/reopen remain in the executed path. Material fixtures enter through the
actual DOM file control and resolve real GPU picks through Rust:

| Fixture | Red body pixels | Final portal pixels | Rust-resolved selection | After actual context loss/restoration |
| --- | ---: | ---: | --- | --- |
| Normal mask | 3,890 | 0 | `Object 313D2F9A` | PNG pixels identical; selection retained. |
| Portal mask | 3,604 | 453 | `Object 313D2F9A` | PNG pixels identical; selection retained. |

Eight downloaded PNGs include 2,989,941 pixel comparisons with the displayed
buffers and verified metadata. Marker-color detection only chooses a position
for the automated click; production selection uses GPU IDs and Rust validation,
never a color-key rule.

The first full application run, `37554188564`, passed compilation and standalone
GPU checks but clicked brown terrain, mistaking RGB `[144,112,62]` for the red
fixture object. The screenshot correctly showed terrain selection. The test now
requires dominance over both other RGB channels. Two witness regressions fail
under the old predicate; an assembly regression fails until the actual runner
calls the helper. All four new selector tests pass, as does the fresh complete
application run. The failed report and captured witness are retained. No renderer
or rendering tolerance was changed for this test-only correction.

### Evidence identity

- [Scoped artifact 11455125326](https://github.com/rndrntwrk/wonderland-/actions/runs/37555822859/artifacts/11455125326): ZIP SHA-256 `f9aa2d6e2c342b4f0839759c5994c29aacc7bd5e1ee5d29461a13b9e42d7e566`.
- [Application artifact 11454841493](https://github.com/rndrntwrk/wonderland-/actions/runs/37555827077/artifacts/11454841493): ZIP SHA-256 `d17b35a4e197db02536074048fab4179f43eff8ee71e89254f7877684ad5508f`; report SHA-256 `e0bbab8bd0d1b0441f9988e27b1ad19ba2fbc3e4fb492f37de28564acddb8d07`.
- Release WASM: `wonderland-web-shell-c2571e1c2cbecb49_bg.wasm`, 8,494,841 bytes; SHA-256 `48b4aa6063acebb833584b8277564e804705a44dfae9496bddd0cab997053c23`.
- Bundled GPU module SHA-256: `052c73e15553450901b4eadb65db7887ae62cfb6e94b4739e569ecfd414becf8`, identical to the checked source file. The test-only correction leaves the release WASM and GPU module unchanged.

Archives, reports, image hashes, published sources and the unchanged merge tree
were checked during handoff. The machine-readable record is
[evidence/client-materials-2026-10-07.json](evidence/client-materials-2026-10-07.json).
Preserve artifacts before their finite retention expires. Use the pinned compiler,
lockfile and Playwright setup from the read-only workflows to reproduce:

```sh
cargo test -p wonderland-render-core -p wonderland-render-3d -p wonderland-world-view --locked
node --test apps/web-shell/tests/gpu/*.test.mjs
cargo run -p wonderland-world-view --example gpu_fixture --locked -- tests/output/source-gpu
node apps/web-shell/tests/gpu/browser.mjs
(cd apps/web-shell && trunk build --release --locked)
node apps/web-shell/tests/gpu/application.mjs
```

Browser evidence uses Chromium 151.0.7922.34 and software WebGL2. Local managed
Chromium denied localhost before graphics initialization; that attempt was not
counted as GPU evidence. The initial publisher passed code checks but its token
could not change workflow files. Source and the two exact read-only CI updates
were subsequently published through separately authorized paths, without force
or additional permissions.

## Review and remaining work

The implementation received an author self-review; an independent reviewer was
not available in this session. The per-pass object-index finding was fixed with
a demonstrated failing regression. Review covered winding, cleanup, clipping,
source buffer sharing, allocation bounds, protocol rejection and preservation of
PNG/cancellation ownership. Independent review remains appropriate before merge.

This leaf does not finish Swarm C. Source sprite/Full2D/hybrid composition,
advanced lighting/environment hosts, full avatar/audio hosts, thumbnails/FSOf
facades and original-VM continuation integration remain separate. Complete
source cohorts, physical GPU/browser/audio devices, measured performance and
live multiplayer acceptance are also outstanding. No merge or deployment is
included.
