# Swarm C: 3D geometry source notes

These notes describe the pure `wonderland-render-3d` implementation and the
source boundaries needed by its consumers. They follow the mandatory geometry
source-discovery report and direct inspection of the repository's C# sources.
Source names below are relative to the repository root.

## Source map

| Area | Source |
| --- | --- |
| Lot units and altitude origin | `TSOClient/tso.world/Model/Blueprint.cs`, altitude/interpolation code around lines 159–196 |
| Terrain normals, grass offset, and UVs | `TSOClient/tso.world/Components/TerrainComponent.cs`, geometry generation around lines 141–274 and 336–340 |
| Floor halves, water, and build support | `TSOClient/tso.world/Components/3DFloorGeometry.cs`, lines 68–100, 182–224, and 711–733 |
| Walls, styles, thickness, and cuts | `TSOClient/tso.world/RC/WallComponentRC.cs`, lines 40–351 |
| Pool composition and projection | `TSOClient/tso.world/Components/Geometry/Modelled3DPool.cs` and `Modelled3DFloor.cs` |
| Roof eligibility, rectangles, and surface pieces | `TSOClient/tso.world/Components/RoofComponent.cs`, lines 142–178, 186–430, and 455–710 |
| Depth reconstruction and source UV projection | `TSOClient/tso.files/RC/DGRP3DMesh.cs`, lines 186–559 |
| QEM simplification | `TSOClient/tso.common/MeshSimplify/{Simplify,SymmetricMatrix,MSVertex,MSTriangle,MSRef}.cs` |
| Reconstruction parameters and normal generation | `TSOClient/tso.files/RC/DGRPRCParams.cs` and `DGRP3DVert.cs` |
| Mesh selection and texture/archive boundary | `TSOClient/tso.content/RCMeshProvider.cs` and `TSOClient/tso.files/RC/DGRP3DGeometry.cs` |
| City channels and coarse/detailed surfaces | `TSOClient/tso.client/Rendering/City/{CityMapData,CityGeometry}.cs` |
| Boundary and persistent-location conversion | `TSOClient/FSO.Common.Domain/Realestate/MapCoordinates.cs` |
| Foliage and neighborhoods | `TSOClient/tso.client/Rendering/City/{CityFoliage,CityNeighGeom}.cs` |
| City coordinates and camera | `TSOClient/tso.client/Rendering/City/CityCamera3D.cs`, especially lines 184–217 and 610–644 |
| Facade placement | `TSOClient/tso.client/Rendering/City/Terrain.cs`, around lines 1807–1814 |
| Lot camera modes | `TSOClient/tso.world/Utils/Camera/{CameraController3D,CameraControllerFP,CameraControllerDirect}.cs` |
| Weather, time, and sky | `TSOClient/tso.world/Model/WeatherController.cs`, `TSOClient/tso.common/Rendering/TimeOfDayConfig.cs`, and `TSOClient/tso.world/Components/AbstractSkyDome.cs` |

## Coordinates and visual input

The implementation uses the C-local core contract's column-vector matrices.
A source row-vector composition is reversed once when expressed as column
matrices. Mesh output is presentation state; it never replaces authoritative
architecture or collision.

Lot terrain uses `(width+1)*(height+1)` signed corner heights and an explicit
base altitude. The final graphics height at one-based level L is:

```text
(raw_corner_interpolation - base_alt) * 9/160 + (L-1) * 8.85
```

X and Z are map X/Y multiplied by 3. Roof altitude centers are explicit input,
not inferred from a floor plane. Bilinear contact uses the same altitude
origin and bounds and reports presentation height only. Large finite contact
bounds are averaged in f64, and an unrepresentable final translation fails.

The additional visual inputs are floor IDs and both diagonal half IDs; indoor
and support flags; directional wall presence, patterns, styles, and object
style overrides; grass state and terrain colors; roof material, pitch,
advanced-surface flag, average texture color, and UV scale; and optionally
the normalized authored pool tile/corner meshes. They are not invented as
fields of the A contract.

Terrain preserves the source unit-step shading normal and the source
`x > 1`/`y > 1` backward-neighbor rule. Rectangular bounds are corrected.
Grass intensity reads the source offset `(y-1)*width+x-1`, with the source
out-of-range fallback, rather than silently using the current tile.

## Floor, wall, pool, roof, and dirty behavior

The tile corner order for generated quads is TL, TR, BR, BL. Floor-half
membership is explicit:

| Orientation | Half 0 | Half 1 | Legacy true side |
| --- | --- | --- | --- |
| Vertical | TL, TR, BR: `[0,1,2]` | BR, BL, TL: `[2,3,0]` | Half 0 |
| Horizontal | TL, TR, BL: `[0,1,3]` | TR, BR, BL: `[1,2,3]` | Half 1 |

The source full floor/terrain winding is retained. Floors use zero as air,
65535 as pool, and 65534 as water. Supported build-mode air uses 65503.
Water cardinal connectivity derives 65504 plus the N/W/S/E mask; the explicit
legacy alternate-pool mode derives 65520 plus that mask. Normal pool rendering
keeps ID 65535. No calendar is consulted to select an alternate mode.

Walls preserve west/north ownership, neighbor reverse-face patterns, diagonal
two-sided faces, and source top caps. Base styles 1 and 255 select thick walls;
object-style overrides do not silently change that classification. Thickness
is 0.075 graphics units, cap offset is 0.005, and full height is `2.95*3`.
Fence height is 0.98 of full height. Cut endpoints use 0.12 of height and the
source rotation-dependent neighboring samples. Rectangular loop and boundary
errors are corrected without changing valid source direction order.

Pool cardinal bits are N=1, W=2, S=4, E=8; diagonal neighbors determine the four
concave corner pieces. Source composition always includes tile 15, then the
selected edge tile when different, followed by concave corners. Authored model
coordinates are projected through bilinear terrain and their triangle indices
are reversed as in `Modelled3DFloor`. Missing assets are reported as:

```text
Content/3D/floor/pool_hq_0..15.obj
Content/3D/floor/poolcorner_hq_0..3.obj
Content/3D/floor/pool.png
```

A synthetic bowl/surface fallback is provided for deterministic unlicensed
fixtures. It is not reported as authored pool reconstruction.

Roof eligibility uses source half tiles (8 of 16 subtiles), indoor space below,
cardinal/diagonal overhang rules, and current-level room/floor/diagonal
exclusions. Scanning begins at half-grid index 2. Rectangle expansion follows
+X, -X, +Y, -Y; overlapping rectangles and the source containment/range
conditions are retained. Hip rise is half the smaller rectangle span times
pitch. The advanced path adds a 0.5-unit rim drop, underside, and edge strips
with source 0.7 and 0.005 offsets. Zero-pitch roofs have finite fallback normals.
Custom roof texture scale and average color are explicit provider inputs;
source custom textures use scale 0.2 while the fallback scale is 1.

Dirty rebuilds return only affected tile parts and roof levels. A changed
terrain vertex expands the tile region far enough to include shared vertices
and normal neighbors; floor/wall/room changes include an eight-neighbor region.
The changed level and roof-above dependencies are explicit. Tile/vertex/index
and roof-query budgets fail atomically. This is presentation invalidation,
not a new architecture or collision solver.

## Reconstruction and QEM

Parameter precedence is embedded FSOR, then filename configuration, then
defaults. If the selected configuration excludes the DGRP range, defaults
apply; filename parameters are not consulted again. DoorFix selects rotations
0/3 for subindex low byte 1 and rotations 1/2 otherwise.

Source reconstruction uses the active near-image sprite order per rotation.
The projection preserves the source `(-72,-344,zOff)` origin, `1.43/128`
scale, Y flip, X rotation -π/6, and Y rotation π/4 times `1+2*rotation`.
Object offsets divide source X/Y by 16 and source Z by 5. The active values are
zOff=-55 and factor=0.39; BlenderTweak uses -57.5 and 0.40. Depth range is
110.851251 before the source scaling. Mirroring starts at the sprite's right
edge. Only depth byte 255 is absent; byte 254 remains present. The optional
alpha policy is explicit, with source depth-only presence as the default.

Four-corner reconstruction requires both diagonals to satisfy the 0.065
tile-unit threshold before emitting either triangle. Three-corner
reconstruction requires all three edges. Counter correction uses the nearest
source image-space border pixel in the `(0.38,0.4]` band, extrapolation divisor
71.55, and clamp `0.498 + 0.001*(rotation%2)`. It preserves source Y/Z only in
the un-clamped branch and caps nearest-border work before mutation.

The previously missing simplifier is now ported from the source's five local
QEM classes. The active configuration is:

| Parameter or predicate | Value |
| --- | --- |
| Target | Original triangle count / 100 |
| Iterations | 125 |
| Aggressiveness | 3.5 |
| Iteration threshold | `1e-9 * (iteration+3)^aggressiveness` |
| Reference rebuild/compaction | Every fifth iteration |
| Collinear-edge rejection | Absolute direction dot greater than 0.999 |
| Normal-flip rejection | Dot with original triangle normal below 0.2 |
| Border handling | Source border classification; endpoints must share border status |
| Singular quadric fallback | Endpoint 1, endpoint 2, midpoint; midpoint wins ties |

The port uses f32 positions and f64 symmetric quadrics. The source-sensitive
steps use local MonoGame-compatible arithmetic: the squared length sum is
rounded in f32, square root executes in f64 and rounds back to f32, and
normalization/division multiply by the f32 reciprocal. UV lerp is
`a + (b-a)*t`. The optimum computes the reciprocal of the determinant before
multiplying the minor. Widening the vector squares or rearranging those
operations changes curved-surface collapse order; the shared core's robust
math is deliberately unchanged.

The independent source review executed unmodified `Simplify.cs` and its four
companion types with the repository's MonoGame DLL. Plane, bowl, ripple,
and holed-grid fixtures at 1/5/6/20/125 iterations, plus an octahedron, produced
21 exact comparisons of ordered triangles and every position/UV f32 bit.
The permanent tests retain the curved bowl's intermediate vertex/triangle
counts and final bit patterns. Textual zero was canonicalized in the comparison
fixture because the C# and Rust parsers treat a written negative zero
differently. This is evidence for those source algorithms and inputs, not
a claim of complete asset or GPU parity.

Border-neighbor counts use a bounded ordered map, preserving the source
count-equals-one result while
avoiding a quadratic search at a high-valence vertex. Invalid/nonfinite
collapses are rejected, and zero-length edge normalization is not allowed to
produce NaNs. The source preset caps primitive visits at 100 million and
reference entries at 8 million; exhaustion is an error with immutable input.
Generic QEM mesh output regenerates normals. Reconstruction resets those
normals and applies the source flip-aware normal generation after reprojection.

After QEM, UVs are recomputed through the inverse source sprite transform.
The source half-pixel offset is applied before the mirror branch. This is
covered for all four rotations and both flip values. A request to simplify
executes QEM; `SimplifierUnavailable` is no longer emitted. A constrained
result can report `SimplificationTargetNotReached` with its actual triangle
count and iterations.

Orphans are removed before retained contact bounds are derived. Source
texture identity and sprite ordinal accompany each reconstructed part;
missing depth advances the ordinal and completion count. Ordinal 65535 is
reserved for authored custom textures. Mesh dimensions, pixel totals, group
limits, finite coordinates, and aggregate mesh counts are checked.

## Override/cache and archive boundary

Effective reconstruction keys are SHA-256 over source and patch identities,
algorithm version, full parameter bytes, offsets, rotations/flip/group metadata,
all pixels, and optional depth bytes. The active derivation namespace is v2
because it includes QEM and post-simplifier UV behavior.

Resolution checks memory using the effective candidate identity, then explicit
user override, authored external override, embedded mesh, matching generated
cache, and reconstruction. Supported normalized candidates carry FSOm version
1–3 metadata; authored reconstruction version 0 is accepted, and obsolete
generated versions are rejected. Empty/corrupt candidates fall through.
Generated cache identity must match the requested key.

The source's ambiguous UserDir enumeration versus ContentDir opening is
replaced by explicit caller-supplied candidate origins. Cache invalidation
records are bounded. On tombstone saturation the resolver conservatively
ignores all generated candidates while keeping overrides and embedded data
eligible. Residency reports mesh bytes, entry count, and invalidation state.
Returned owned clones belong to the caller's separate residency budget.

`MeshCandidate` is a normalized mesh and mask kind. It does not parse a complete
FSoM gzip payload or carry every per-group material/depth-mask surface. Source
`DGRP3DGeometry` uses PixelSPR/PixelDir, custom-texture sentinel 65535,
external-custom versus embedded-MTEX precedence, and SPF/IFF provider lookup.
That archive/material/texture adapter is unfinished integration work.
The depth-pass planner preserves clear-depth-0, clear-depth-1, visible body
groups, final portal group, and portal-stencil clear ordering. It is a CPU
plan, not physical GPU depth/stencil acceptance.

## City channels, bounds, layers, and identity

Terrain and forest channels use exact RGBA matching, including alpha 255:

| Terrain | RGBA | Class |
| --- | --- | --- |
| Grass | 0,255,0,255 | 0 |
| Sand | 255,255,0,255 | 1 |
| Rock | 255,0,0,255 | 2 |
| Snow | 255,255,255,255 | 3 |
| Water | 12,0,255,255 | 4 |
| Any other value | Any unmatched tuple | Void 255 |

Forests are fir `0,106,40`, birch `0,235,66`, cactus `255,0,0`, palm
`255,252,0`, and black for none. Unmatched colors remain unknown.
Cactus/palm use atlas slots 2/3 in 2D and 3/2 in 3D. Density buckets are
`density*4/255`, with instance counts 0, 1, 4, 7, and 15. The local .NET
Framework RNG uses seed `y*512+x`, source unchecked subtraction, road
clearance 0.15, source model variants, and scale 1/75. It does not consume
simulation randomness.

The exact atlas lookup arrays are retained:

```text
Blend:      11,7,15,2,9,6,0,4,1,16,20,12,14,18,10,8
Road edge:  -1,5,12,13,7,6,15,14,28,29,20,21,31,30,23,22
Road corner:-1,8,2,26,3,17,16,10,25,24,9,18,1,27,11,19
```

Elevation is red/12 and shading normal differences use red/6. Coarse tiles
use source indices `[0,1,2,0,2,3]` and 16×16 chunk ordering. Blend priority
selects the next higher adjacent terrain class. Primary terrain UVs remain
world coordinates divided by 4, with a separate unmirrored 7×3 mask channel.
Road-edge and road-corner layers use mirrored 8×4 UVs.

Detailed patches retain the source Hermite continuity settings at the patch
edge, interpolated normals, TR–BL indices
`[0,1,subdiv+1,subdiv+1,1,subdiv+2]`, and all blend/road layers. Secondary mask
UVs are emitted for every vertex through `CityPart::mask_uv()`. The default
source subdivision is 4; the API permits bounded subdivisions 1–8.

For row y, start is `abs(y-306)`; end is `307+y` for y<205 and `717-y`
otherwise. These predicates are deliberately separate:

| Boundary | Predicate |
| --- | --- |
| RendererDiamond | y in 0..511, x >= start and x < end |
| RendererWithFade | Renderer side range expanded by 10; clipped to the 512×512 image |
| ServerDiamond | Inclusive x <= end, with the source padding offsets |
| LegacyMapData | x > 0 and x < 512; y in 0..511 |
| Rectangle | Explicit synthetic rectangular policy |

Renderer fades sample class/road channels at x clamped to start..end-1.
Height/normal sampling clamps to start..end, and opacity is
`1-min(side_distance/9,1)`. Blend/road layers require strict
`x>start && x<end`. Detailed sampling preserves the source current/next-row
clamps. Image bounds are then checked explicitly, avoiding the source's
possible x=512 row alias at the canonical image edge.

Packed location is `(x<<16)|y`. It is not a persistent destination ID.
Neighborhood lookup uses tile center +0.5 and keeps the first origin on equal
distance. Lot/city camera mapping uses the 72-tile interior and offset 2;
inverse results are checked for overflow.

## Facades, cameras, environment, and quality

The source facade transform is translation to city X+1/Y/Z, rotation -π/2
around Y, and scale `(1/77, y_squish/77, 1/77)`. Corner elevation averages
red/12. The helper takes tile-unit geometry. `bake_facade` composes the
graphics-to-tile division by 3, so a 77×77 flat lot occupies exactly one city
tile. Normals use the inverse transpose. SHA-256 identity includes content,
revision, squish, explicit part/count delimiters, material/style metadata,
and all generated vertices and indices. OBJ export validates its public mesh
before indexing and reorients faces to their explicit normals.

The included synthetic OBJ is geometry only. Legacy facade day/night atlas
rendering is not implemented. No archived or rendered source facade texture
is implied by that artifact.

Lot orbit uses source distance 10, target `(3*x, height+3, 3*y)`, squared zoom,
source cosine pitch mapping, and the near/middle/far inherited values
3.7/7/11. City orbit is a separate type: direct city center, height+0.5,
distance 3.5 adjusted when target zoom exceeds 2, and near plane 0.25.
City first-person preserves the source independent height offset and -10-unit
view direction. Lot first-person near is 1; direct head mode uses near 0.5,
FOV 0.9, the explicit entity generation, and source head clearance.

Camera transition sampling uses transform interpolation and projection-element
interpolation with source default duration 0.66 seconds. Cut rotation uses
ties-to-even. Presentation damping integrates explicit elapsed time and is
tested at 30/60/120 Hz. Camera pose/projection products reject nonfinite output.
Height constraints and external resolved poses are explicit policies; no
collision acceptance is inferred.

Manual weather preserves low-byte intensity /100, manual bit 8, type bits
9–10, and thunder bit 11. Automatic weather uses an injected UTC offset from
2019-01-26, local source RNG, and a 150-second hourly crossfade. Time-of-day
uses the 14 source byte-color stops, interpolates bytes before power 2.2, and
preserves the sky's deliberate 1-to-0 discontinuity. It reads no wall clock.

Quality/LOD is a capability-limited presentation policy with source lighting
levels, MSAA/SSAA distinctions, surrounding-lot and weather flags, and explicit
screen-size LOD thresholds. Those choices have CPU tests. Physical lighting,
depth/stencil, anti-aliasing, weather, and target-device performance remain
acceptance work.

## City/lot transition and acceptance boundary

Directory records carry persistent IDs, location, record revision, availability,
and an optional expected lot ID. A snapshot declares fixture or live-provider
provenance. Duplicate IDs, invalid locations, and zero destination/lot identity
are rejected. Session creation requires a nonzero injected generation that the
transport owner never reuses across reconnects or object recreation.

Request tickets carry that generation and a serial. Receipts echo both and
match destination/directory revisions, lot ID, and epoch. Old-session tickets
and old receipts cannot satisfy a newly created transition. Entering a lot
also requires the accepted first frame's lot and epoch. Directory refresh,
rejection, and returning to the city invalidate pending work while preserving
appropriate camera/selection intent.

The two-destination tests use synthetic IDs and fixture provenance. They prove
routing and stale-result rejection, not a real directory connection,
authorization/admission, authoritative collision, or a provider-fed first frame.
Those acceptance gates, the full FSOm provider/material adapter, source assets,
and facade atlas rendering must remain visible in the C handoff.
