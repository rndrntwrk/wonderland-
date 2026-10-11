# Isometric renderer source contract and review corrections

## Scope and evidence

This document covers the pure Rust presentation implementation in
`crates/render-iso`. Its source baseline is FreeSO C# commit
`4c6b3e8f5835b228723caea3c9f683c62f244f73`; source paths below are relative to
`TSOClient/`. No original game sprite, palette, mask, or other payload is
redistributed.

The implementation contains source-equation helpers, normalized sprite
preparation, a fragment oracle, batch planning, cutaways, lighting arithmetic,
view-transition state, and invalidation planning. It is not a compiled or
qualified GPU renderer. Analytic fixtures establish the behavior they exercise;
they do not substitute for original-runtime captures.

The five review corrections described below were prepared through GitHub's
unattached-blob route after the workspace became unavailable. Their Rust
regressions require execution in CI; no local test pass is claimed for this
revision. Independent arithmetic checks were used to confirm the expected yaw
boundary and RGB-lighting values.

## Source ledger

| Area | Source anchors | Implemented contract |
|---|---|---|
| Zoom metrics | `tso.world/WorldState.cs:702` | Far tiles 32x16, Medium 64x32, Near 128x64; cadges 34x96/68x192/136x384; baselines 87/174/348; terrain offsets -59/-118/-235. |
| Projection and ordering | `tso.world/WorldState.cs:588,628,657,683` | Four cardinal screen bases, inverse ground projection, source vertical projection and draw-order scalar; graphics units map tile(x,y,z) to 3*(x,z,y). |
| Camera | `tso.world/Utils/WorldCamera.cs:161,191,220`; `tso.world/Utils/Camera/CameraController2D.cs:26` | Source orthographic diagonals 64/128/256, inverse depth ranges, negative near plane, yaw 315/225/135/45 and pitch 30; independently snapped camera center. |
| DGRP placement | `tso.world/Utils/DGRPRenderer.cs:148,171,196,204,234` | First exact rotated direction/zoom image, null/missing layer skips, dynamic masks, cadge placement, source truncation, logical dimensions, and resolved visual position. |
| Texture padding | `tso.files/Formats/IFF/Chunks/SPR2.cs:639,704`; `tso.common/Utils/TextureUtils.cs:512` | Logical and physical extents remain distinct. Shared point/clamp UVs sample transparent upload padding rather than extending the logical edge. |
| World offsets | `tso.world/Utils/_2DWorldBatch.cs:359`; `tso.content/ContentSrc/Effects/2DWorldBatch.fx:188,202` | Preparation bakes the world offset into each emitted world anchor; CPU and shader reconstruction use that same anchor exactly once. |
| Depth | `tso.world/Utils/_2DWorldBatch.cs:545,561`; `tso.content/ContentSrc/Effects/2DWorldBatch.fx:163,188,221` | Rotation-specific back/front anchors; byte fraction (1-q/255)/.4; clip-z/w and source tiny xy bias; inverse-WVP lighting reconstruction without a second divide. |
| Alpha and room modes | `tso.content/ContentSrc/Effects/2DWorldBatch.fx:94,113,244,319,450,527,559` | Pass-specific coverage; wall MRT mask replaces alpha; reserved rooms 65535/65534/65533 mean unlit/invert/grayscale; ordinary room low-byte 0 bypasses lighting. |
| Gamma | `tso.content/ContentSrc/Effects/LightingCommon.fx:151,175,183` | Basic power 2.2 transfer and advanced polynomial/root approximation; straight RGBA input followed by one premultiplication. |
| Packed depth and IDs | `tso.content/ContentSrc/Effects/2DWorldBatch.fx:57,527`; `tso.world/Platform/WorldPlatform2D.cs:193` | Literal RGB float depth packing including d=1 wrap; source-positive short pick codes are allocated separately from generation-aware game identities. |
| Batches and residency | `tso.world/Utils/_2DWorldBatch.cs:448,519,633`; `tso.world/WorldEntities.cs:109,172` | Stable render-mode/draw-order sorting followed by adjacent compatible material runs; explicit missing depth; portable 16383-quad cap; software overlap runs. |
| Static surfaces | `tso.world/Model/ScrollBuffer.cs:11`; `tso.world/WorldStatic.cs:47`; `tso.world/Utils/_2DWorldBatch.cs:236,728` | Source grid step 512/precise_zoom and world-translation depth correction; final framebuffer placement follows the explicit zero-origin target decision below. |
| Light atlas | `tso.world/LMap/LMapBatch.cs:110,128,145`; `tso.world/WorldState.cs:424` | Six slots in 3x2 layout, resolution 8 or 16, width>64 software-depth ultra fallback, bounded physical texture sizes and nonsquare scissor correction. |
| Room/point lighting | `tso.content/ContentSrc/Effects/LightMap2D.fx:79,113,145`; `tso.world/LMap/LMapBatch.cs:449,562,608,632` | Packed room-map diagonal selection; power 2.2 point falloff; wall red and floor green shadow roles; outdoor accumulation, outside-color multiplication, then indoor .70 scaling. |
| Surround light adjustment | `tso.content/ContentSrc/Effects/LightingCommon.fx:25,32,51`; `tso.world/Components/SubWorldComponent.cs:120,208` | Independent RGB LightingAdjust ratios apply after computing the shadow average from unadjusted atlas intensity. |
| Light scheduling | `tso.world/LMap/LMapBatch.cs:212,270`; `tso.world/LMap/LightData.cs:62` | Stable floor ordering, unimportant priority doubling, important priority MAX, absent-wall/above-floor queue removal, and source unstored clustering weight. |
| Cutaways | `tso.world/Components/WallComponent.cs:614,632,792,876` | Only the selected top floor cuts; source cardinal/diagonal neighbor rules, neighboring cut rotation, special diagonals, and cut-door omission. |
| Wall styles/depth | `tso.world/Components/WallComponent.cs:156,298,823`; `tso.common/Utils/TextureGenerator.cs:154,212` | Cuts 1/2 select styles 7/8 and overlays 252/253; principal TL/TR/horizontal/vertical depth images preserve source dimensions and nearest-even bytes. |
| View transitions | `tso.world/Utils/Camera/CameraControllers.cs:51,69,82,155`; `tso.world/Utils/Camera/CameraController2D.cs:59,87,105,144`; `tso.common/Utils/DirectionUtils.cs:47` | TRS/slerp view interpolation, projection remaining-weight powers 1/50 into 3D and 5 out, source double yaw quantization, quarter-turn cosine ease and zoom sine ease. |
| Invalidation | `tso.world/Model/BlueprintChanges.cs:47,63,142,186,245` | Explicit sprite/static/wall/floor/room/light/roof/device dependencies; source time and outside-color thresholds; full versus cut-only wall recache. |

## Normalized input and coordinate ownership

DGRP sprite offsets arrive already converted from the source integer fields to
`f32`. Object offsets retain their raw source units: x/y are sixteenths of a tile,
z is fifths of a tile. Object direction rotates those offsets before projection.
`SpriteFrame.position` is not added to DGRP placement. Effective image selection
uses the original layer order and first exact direction/zoom match.

An instance supplies resolved visual position, room/base-room mapping, floor,
visibility/cutaway state, dynamic base/count and two 64-bit masks, plus
generation-aware identity and visual revision. These values are presentation
inputs; they do not decide movement, routing, placement legality, or simulation
state. SPR# fallback depth 128 remains distinct from explicitly missing depth.

`PreparePolicy.world_offset` is in graphics units. `PreparedSprite.world_anchor`
and every `SpriteShaderVertex.world_anchor` include it. The depth oracle uses the
same resolved value with zero additional offset. An adapter drawing these
vertices must use a zero WorldOffset uniform; reapplying the policy would double
the offset. Different offsets may safely share a material batch because each
sprite retains its own vertex anchor. Batches still belong to one frame/view
projection and its explicitly bound resources.

`MaskInput.rgba` contains the logical image. `physical_size` defines its uploaded
extent, whose remaining texels are transparent zero. The engine/provider must
upload that padding consistently. The fragment oracle derives mask texels from
the same mirrored UV used for color/depth, clamps at the physical boundary, and
returns alpha 0 for padding inside it. A mask with different physical dimensions
therefore samples the correct shared-UV area instead of using color pixel indices
as mask indices.

## Final framebuffer convention

Prepared sprite rectangles and shader vertex positions are **final framebuffer
pixel edges**. The origin is the top-left corner (0,0); pixel centers lie halfway
between integer edges. Positive x moves right and positive y moves down.
Precise zoom is applied once around the viewport center. The engine maps these
positions to clip space and must not apply source `_2DWorldBatch.ResetMatrices`
pixel shifts or precise zoom a second time.

Cache rendering and restoration use this same convention.
`cache_restore_placement` accepts the effective stored/current pixel origins in
unzoomed coordinates, including the cache's chosen origin, and the physical
surface width/height. It returns `(stored-current)*precise_zoom` as a fractional
framebuffer translation. An unchanged origin restores at (0,0); surface dimensions
are not scaled again. World translation remains a separate graphics-space value
for cached-depth restoration.

This is an explicit target choice. The source DirectX batch projection adds 2
pixels while `DrawScrollBuffer` subtracts 2 in the raw destination; its other
backend uses 1.5/half-pixel adjustments. The source also truncates scaled viewport
sizes, centering offsets, and cache translations. The target omits those backend
adjustments together and retains fractional cache translations so direct and
cached geometry agree at noninteger precise zoom. Source camera snapping, cadge
rounding, local DGRP truncation, and the negative-floor cache grid are unchanged.

This convention is not a claim of legacy backend screenshot parity. Physical
sampling, edge coverage, and cached/direct captures remain engine gates.

## Depth, material, and ID contract

For raw depth byte q, interpolation uses `(1-q/255)/0.4` without clamping; values
below 153 extrapolate beyond the front anchor. Modern source HLSL reads Alpha8's
alpha channel and the older path reads red. A target adapter must select one raw
channel explicitly. Color, depth, masks and ambient room lookup use point/clamp;
advanced light and direction use linear/clamp.

| Pass | RGBA8 alpha that survives |
|---|---|
| Basic/simple ID | 1..255 |
| Color/depth, wall MRT, cached restore | 3..255 |
| Depth ID | 26..255 |

Wall MRT uses mask alpha in place of color alpha, even when color alpha is zero.
The basic source wall shader has a different cutoff; `AlphaPass::Wall` models
MRT coverage. ID passes use the appropriate coverage predicate and a separately
bound encoded ID color. Cached colors are already premultiplied.

Room 65535 is unlit, 65534 inverts RGB, and 65533 uses grayscale weights
(.2989,.587,.114). Luminous sprites select 65535 except when invert/disabled is
already requested. Ordinary low-byte-zero rooms bypass light multiplication.
Engine sRGB conversion must not duplicate the source transfer or premultiplication.
`SpriteShaderVertex::floats` supplies the explicit 48-byte layout; Rust memory
layout is not an upload contract.

## Lighting contract

`floor_light_color` accepts `lighting_adjust: [f32; 3]`. Existing uniform
adjustments use `[value; 3]`. Source surround updates divide main-world outside RGB
by the surround's older outside RGB independently, so a scalar cannot express all
valid inputs. Compute average and shadow ratio from the original atlas intensity,
then multiply the target RGB by LightingAdjust. Pre-adjusting atlas RGB before
the average changes floor-shadow strength.

The room-light oracle evaluates shader arithmetic before attachment quantization.
It clears to minimum alpha, adds supplied outdoor contributions and outdoor-colored
points, multiplies outside RGB/average alpha, then adds indoor points with .70 scale.
Window intensity is ambient/150 and values below .2 are skipped. Point falloff is
`clamp(1-distance/radius,0,1)^2.2`; wall shadow reduces RGB and alpha, while floor
shadow reduces alpha. Ultra floor shadow averages 25 green-channel samples.
Source floor-light interpolation may extrapolate; it is not silently clamped.

The atlas has six logical slots, but the source physically exposes five room-map
textures. Zero-based floor index 6 is rejected rather than indexing an unavailable
binding. Actual room maps, wall/object shadow samples, solar/environment inputs,
and the source basic room-color table remain explicit provider/engine inputs.
Repeated RGBA8 render-target saturation is not modeled by the floating-point
accumulation oracle.

## Explicit target decisions and retained quirks

| Decision | Reason and practical limit |
|---|---|
| Dynamic count above 128, overflowing ranges, and sprite IDs above ushort fail. | Prevent aliasing through source shifts/narrowing; incompatible content needs an explicit compatibility adapter. |
| Exact cardinal object-offset rotations. | Avoid source trig epsilon; a truncation-boundary pixel can differ from the original runtime. |
| Complete material identity and explicit no-depth state. | Prevent stale depth samplers and source grouping collisions; source grouped by color/mask only. |
| Stable adjacent batches, with at most 16383 quads. | Preserve transparent order and portable 16-bit indices; software overlap splitting remains bounded by admitted geometry. |
| Premultiplied blending by default. | Source dynamic drawing also applies a NonPremultiplied state; `LegacyDynamicNonPremultiplied` exposes that different result explicitly. |
| Final zero-origin framebuffer/cache placement. | Omit the source backend pixel shifts as a pair and retain fractional cache translations; see the coordinate contract above. |
| Nonfinite clips and zero clip-w fail. | Do not qualify undefined source projections or reuse prior state. |
| Nonsquare scissor y uses height. | Correct the source width-for-y bug; the other source width-2 wall-offset denominator remains literal. |
| Out-of-lot cutaway neighbors are uncut and out-of-lot objects return false. | Source helper indexing can be undefined or leave stale hide state. |
| Standalone room/unimportant-light invalidations are retained. | Source Dirty.ALL omits those flag bits. |
| Dynamic residency changes at exactly 2.5 presentation seconds. | Removes the source ring's half-second scheduling quantization without using simulation ticks. |
| Settled Hybrid2D uses geometry architecture and sprite objects. | Explicit target three-view behavior; source stable hybrid still uses cached 2D walls. |
| 2D camera/selection intent survives view switches. | Full3D can request Near sprite resources without replacing the user's desired 2D zoom. |

Retained source behavior includes depth endpoint wrap, the low-byte-zero room
bypass, pass-specific alpha thresholds, unclamped floor-light interpolation,
projection-transition powers, double-precision yaw boundary behavior, and the
light-cluster merge that does not update stored weight.

## Review regressions and remaining gates

The added regressions cover:

- World offsets appearing exactly once in emitted anchors and CPU depth, including
  compatible batching of sprites with different offsets.
- Transparent mask padding, independent physical dimensions, and mirrored shared UVs.
- Direct versus restored sprite vertices for all four rotations, negative scroll,
  identity restoration, and noninteger precise zoom.
- Source double yaw calculations for the f32 quarter-turn boundaries and their
  adjacent representable values, including wrapped/negative inputs.
- Nonuniform RGB surround adjustment with partial floor shadow and invalid components.

B still supplies effective IFF/PIFF identity and revisions, OBJD dynamic fields,
base-room mappings, architecture masks, and explicit FloorCopy/ZAsAlpha/pool
recipes. Principal wall depth generation is present; complete floor, junction,
corner, headline, wall-style and authored content treatments remain provider work.
Medium/far cardinal wall depth heights 135/67 remain different from destination
heights 136/68; do not silently resize them into matching shapes.

Root/core own frame admission, aggregate cache residency, content/device epochs,
generation-aware asynchronous picking, reference-scene rasterization, and visual
derivatives. Source ID picking does not include architecture occluders; adapters
must record whether they implement that behavior or visible-pixel picking.
The shared engine fixture uses precomputed effective mask alpha.

No compiled WGSL/GLSL, actual GPU captures, backend depth equality, source attachment
quantization, complete provider adoption, browser behavior, physical-device
behavior, or representative performance is established by these helpers. Requested
PPX depth format is ignored by the source in favor of Depth24Stencil8; adapters
must record the actual format/comparator. The older iOS/GL2 shader also differs in
lighting, software depth, floor discard and offset handling. Those gates remain
open after the review corrections.
