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

## Verification method

The new mask-admission/order/budget/sharing tests failed on the old scene builder
before implementation. A subsequent regression caught distinct pick indexes
being assigned to a portal's two visible passes; it fails before deduplication
and passes after. The first four Node protocol regressions also failed before
schema-2 support. Malformed-envelope tests alone are not counted as successful
rendering evidence.

A separate differential test checks **1,872 fragment cases** against the unchanged
source-derived `ObjectDraw::apply_depth_stencil` and blend oracle. It varies
initial stencil/depth, incoming depth, winding and alpha around the source cutoff,
and checks resulting depth, stencil, color and identity. This is an executable
Rust source-derived oracle, not a new run of the original C# GPU implementation.

The browser fixture generator retains the three previous scenes and adds normal,
portal and mixed-mask source documents. The portal case must produce actual final
group pixels with its dynamic bit disabled; its unmasked comparison must expose
more pixels. It cannot pass with an empty stencil. GPU samples include both the
masked object and portal-final interior. The mixed case follows a portal with an
ordinary translucent object to expose leaked stencil/depth/blend state. Fixtures
are synthetic normalized documents, not redistributed original game assets.

The real application test imports normal and portal documents through the DOM
file control, exports the actual rendered PNG, picks visible object pixels through
Rust, then performs real context loss/restoration and checks identical pixels
and retained selection. The prior ordinary application and PNG scenarios remain.
The normal read-only Browser UI workflow runs both standalone GPU comparisons
and this application test against the actual release build.

Local checks before publication: **188 affected Rust tests**, **28 Node tests**,
strict affected native Clippy and actual WASM client Clippy pass. The attempted
local whole-workspace test was blocked at compilation by original `TSOClient`
files omitted from the source-only handoff archive; it is not counted as a pass.
The managed local browser blocked localhost navigation before graphics execution;
that attempt is not GPU evidence. Fresh full-checkout hosted results must be
recorded separately before declaring browser delivery verified.

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
