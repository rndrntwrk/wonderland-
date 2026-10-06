# Common presentation foundation: source and compatibility notes

This package implements C-local `CONTRACT.md` v1 at source baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. Its math, admission, picking,
cache ownership and CPU reference rendering are presentation algorithms.
They do not advance simulation ticks, consume VM randomness, issue commands,
or define collision legality. No renderer or audio backend handle enters a
serialized `RenderFrame`.

## Source units

The brief's `TSOClient/tso.world/WorldSpace.cs` does not exist at this baseline.
`WorldSpace` is defined in `TSOClient/tso.world/WorldState.cs:480` instead.

| Source anchor | Observed behavior | Core conversion |
| --- | --- | --- |
| `WorldState.cs:482,663–685` | Three graphics units per tile; tile `(x,y,z)` maps to graphics `(3*x,3*z,3*y)`. | `units::tile_to_graphics`; inverse `graphics_to_tile`. |
| `Model/Blueprint.cs:114–116,159–162` | Raw terrain altitude relative to `BaseAlt` is multiplied by `3/160` to produce tile height. | `terrain_height_to_tiles` accepts a raw altitude delta, not an absolute raw altitude. |
| `Components/WallComponent.cs:140`; `Components/FloorComponent.cs:55`; `WorldState.cs:369` | Story elevation uses `(level-1)*2.95` tile units. | `story_height_to_tiles` accepts a story offset/count; adapters subtract the level origin. |
| `Utils/DGRPRenderer.cs:208–211` | DGRP center offsets divide horizontal x/y by 16 and vertical z by 5, then rotate in tile coordinates. | `dgrp_offset_to_tiles`, with `dgrp_offset_to_graphics` for the subsequent axis mapping and scale. |
| `Components/ObjectComponent.cs:179–185` | SLOT horizontal offsets divide by 16; vertical height or offset divides by 5. The source resolves nonzero fixed SLOT heights using `SLOT.HeightOffsets`. | Adapters resolve the height override first, then use `slot_offset_to_tiles` or `slot_offset_to_graphics`. |
| `tso.files/RC/DGRP3DMesh.cs:231` | Reconstruction changes offset axis order to `(x,z,y)` with divisors `(16,5,16)`. | The named conversions keep this axis change explicit. |
| `Components/EntityComponent.cs:115–122` | Setting visual position already adds terrain and multitile elevation. | `tile_to_graphics` consumes the final visual tile position and adds no elevation. |

These helpers do not implement SLOT routing, support legality, height-table
selection, or DGRP camera matching. Those remain provider/adapter concerns.
The conversion helpers are arithmetic; ingestion validates finite values before
using them in frames or uploads. Data already normalized by B's `FreeSo`
conversion must not receive a second handedness conversion.

## Math conventions and validation

`Vec2`, `Vec3`, `Quat`, `Mat4`, `Aabb` and `Ray` are owned `Copy` f32 values
with serde support. Matrix storage is column-major; vectors are columns.
The source row-vector expression `localRotation * localTranslation * parent`
therefore becomes `parent * translation * rotation`, once at the adapter
boundary. `Mat4::from_trs` is translation × rotation × scale.

Perspective and orthographic projection use a right-handed view looking along
negative Z, with depth from zero at the near plane to one at the far plane.
Projection constructors reject nonfinite or reversed ranges, nonpositive
perspective near/aspect, and invalid FOV. `look_at_rh` rejects coincident eye
and target and parallel/zero up vectors. Orthographic near may be zero.

Quaternion normalization and axis-angle construction return `None` for invalid
finite input or a zero norm/axis. Shortest-path slerp accepts validated unit
inputs. `Transform` validation requires finite translation/scale, a finite
unit quaternion within `1e-4` of squared norm one, and a finite resulting
matrix. Finite negative and zero scale are permitted; a singular matrix has
no inverse. Presentation interpolation requires a finite fraction in `[0,1]`
and valid endpoints. The reference path uses no authoritative timeline.

Vector normalization and length use f64 intermediate magnitudes to avoid
overflow for representable f32 results. Perspective coefficient products and
orthographic spans/sums also use f64 intermediates. Quaternion vector rotation
keeps the ordinary f32 path and widens its cross products if an intermediate
overflows. Inversion uses f64 Gauss–Jordan elimination with partial pivoting,
then rejects an unrepresentable f32 result or one whose delivered inverse has
an absolute two-sided identity residual above `1e-3`, evaluated in f64. This
rejects roundoff pivots from exactly dependent columns and unusable numerical
inverses while retaining representable tiny diagonal scales. Ill-conditioned
matrices may return `None`; accurate inversion of every such matrix is not promised.

AABB transformation accepts finite affine matrices only and bounds all eight
corners. Projective matrices can cross homogeneous zero and are rejected.
Ray slabs preserve the original direction magnitude: returned values are
nonnegative ray parameters, not distances for a non-unit direction. Parallel
slabs outside the box, zero directions, nonfinite rays and unrepresentable
interval endpoints fail explicitly.

## Mesh and image limits

`RenderLimits::default()` permits 65,535 entities, 2,000,000 vertices,
6,000,000 indices, 4,096 pixels per image dimension and 16,777,216 texture
pixels. Callers can lower each limit. Mesh validation rejects nonfinite
position/normal/UV/color, out-of-range indices, and an index count not divisible
by three. Zero normals and degenerate triangles remain valid CPU geometry;
the reference rasterizer discards zero-area triangles. Colors need be finite;
the reference path clamps its color channels to `[0,1]`.

Images must have nonzero dimensions, a checked pixel product within limits,
and exactly one owned straight-RGBA pixel per logical pixel. There is no implicit
padding or inherited texture/depth binding. Validation itself allocates no
image or mesh buffers. Reference surface allocation checks dimensions/pixels
first and uses fallible reservation.

Serde implementations preserve values and own their data. They are not an
untrusted-stream decoder with a byte/count budget. An ingestion adapter must
bound serialized envelopes and collection counts before allocating decoded
payloads; it must call validation before reference drawing or engine upload.
`RenderFrame` admission validates an already owned full frame.

## Atomic frame and pick policy

`frame::FrameStore` requires an explicit `reset(lot_id, epoch)` before the first
frame and before changing either value. A reset clears the displayed frame,
indices and generation tombstones, and increments an internal reset generation.
It invalidates picks even when the supplied lot/epoch match the old values.

Admission validates every candidate entity and selection before changing the
current frame or identity history. Duplicate object IDs, generation zero,
invalid current/previous transforms, decreasing architecture/visual revisions,
decreasing generation and inconsistent selection reject the entire candidate.
Ticks can skip; an identical full-frame replay is an idempotent success, while
another candidate at the same or an older tick is rejected. Replay identity
therefore remains separate from presentation cadence.

Any transform, previous-transform, asset, level, visibility or selectability
change requires an increased visual revision for an unchanged entity generation.
An unrelated entity update does not invalidate a ticket. Selection must name
the exact generation of a selectable entity present in the candidate; an
invisible selectable entity can remain selected. Tickets themselves require
the target to be visible and selectable.

Deleted object IDs retain their greatest generation until reset. Reappearing
or reused IDs must increase generation. Tombstones are bounded by
`max_entities` unique object IDs since reset, in addition to the per-frame
entity limit. A caller expecting more unique identity churn in one lot/epoch
must configure that budget accordingly; exhaustion rejects candidates
atomically. The store cannot silently discard tombstones and revive tickets.

`PickTicket` contains the game identity and visual revision plus opaque
lot/epoch/content, content-generation, reset-generation and device-generation fields. Resolution
checks the current frame and target identity; movement, changed interpolation
history, deletion/reuse, content replacement, device reset, or lot reset fails
an old ticket. Each accepted content replacement increases content-generation,
so changing from content A to B and back to A cannot revive an old ticket.
Rejected admission leaves this generation unchanged. A counter that reaches its u64 limit permanently disables picks
for that store rather than wrapping into an old identity.

## Cache and thumbnail ownership

`cache::DerivedKey` hashes a domain marker, effective patched source hash,
content/patch identity, algorithm version and length-prefixed parameter bytes
with SHA-256. A provider's unpatched resource ID alone is insufficient.
`ThumbnailKey` adds view mode, output dimensions and length-prefixed request
parameters. Callers include every effective camera, appearance/pose, lighting,
and override input in these parameter bytes.

`BoundedCache<T>` limits both entry count and `ResourceBytes` categories:
encoded bytes, decoded CPU bytes, staging bytes and GPU bytes. Accounting uses
checked additions and rejects category/total overflow. The caller supplies
measured owned payload costs; generic Rust values cannot discover GPU allocator
sizes. Cache metadata overhead is bounded by entry count and is not claimed as
payload bytes. Admission plans LRU eviction before mutation, retaining all
existing entries if pinned residency prevents fitting the candidate.
The incoming owned value is dropped on failed admission.

Reads update LRU order; key order breaks ties deterministically. Pins are
counted, prevent eviction/replacement/removal, and must be released by their
owner. Explicit `clear` is a reset boundary and drops all values including
pinned ones. Eviction and replacement drop cache-owned handles. Removal transfers
ownership to the caller, which is then responsible for dropping the payload.

`CachedResource<D,G>` owns immutable decoded data and an optional GPU handle.
Its cache specialization's `device_reset` drops all GPU handles and zeroes GPU
and staging residency while retaining decoded/encoded residency and pins.
For this resource shape, staging allocations must be absent or owned by the GPU
handle so the reset actually releases them. It does not secretly retain external
GPU handles. Other resource shapes implement their reset outside this cache.
Frame and cache device reset are explicit coordinated adapter operations.

`derivatives::DerivativeQueue` adds a separate owner for actual thumbnail/facade
work. It accounts for queued and running reservations, immutable installed
artifacts and leased retired generations. Cancellation does not erase a running
job's reservation; completion or dropping the owned job releases it. Reset hides
installed data while a held lease keeps that exact generation resident and
charged. The renderer, source atlas equations, bounded worker protocol and
remaining source/client algorithms are documented in
[derivatives-source-notes.md](derivatives-source-notes.md).

## CPU reference rasterization

`reference::ReferenceSurface` owns an image, a depth buffer, and an
`Option<EntityRef>` buffer. Buffers are exposed through immutable accessors.
Construction/clear uses positive infinity for empty depth. The digest includes
dimensions, RGBA bytes, exact depth bits and explicit game-ID presence,
object ID and generation. It is fixture evidence, never an engine/GPU handle.

Pixel coordinates have top-left origin. Triangles sample pixel centers and use
a top-left coverage rule, including both winding directions, to avoid shared-edge
double blending. Shared edges are evaluated in canonical endpoint order and
negated for the reverse direction, preserving exact antisymmetry even near
floating-point cancellation. Clipping uses symmetric intersections and snaps
the active plane coordinate exactly; adjacent coincident polygon vertices are
removed. Regressions cover both diagonal splits of clipped quads and tiny
shared-edge roundoff without introducing a subpixel quantization policy.
Mesh drawing validates every input and transformed vertex
before writing, clips homogeneous triangles against all six zero-to-one clip
planes, then triangulates the polygon. Singular `w=0` points cannot be projected
and are omitted. Post-projection depth is interpolated in screen space;
colors and UVs use reciprocal-W perspective interpolation. Extremely tiny
positive W is capped to the largest representable reciprocal f32 for this
reference API and is outside an asserted GPU-equivalence guarantee.

`draw_textured_mesh` multiplies vertex color by a straight-RGBA image sampled
using clamped normalized UVs and nearest filtering. `blit` clips a signed image
origin without integer wrap. `write_fragment` allows sprite adapters to supply
their converted per-pixel source depth. Normal/material lighting must be resolved
by downstream modules into vertex or sprite colors; this path does not invent
legacy shader lighting.

Default fragments use `DepthComparison::Less`, write depth and ID, and reject
alpha zero. `ReferenceSurface::set_depth_comparison(DepthComparison::LessEqual)`
explicitly admits equal-depth fragments in insertion order. The shared engine
fixture and derivative renderer choose that source policy. The ordinary
MonoGame `DepthStencilState.Default` uses LessEqual, as confirmed by the
[FSOm source probe](fsom-source-notes.md); both engine adapters implement the
equivalent rule, with reversed comparison for Bevy's reversed Z.

The original strict reference retained green terrain over a later coplanar floor
while both captured GPUs rendered the floor. Dedicated core and fixture
regressions now cover both policies, including equal-depth ownerless ID clearing
and rejection of the next farther f32 value. This change preserves the default,
introduces no depth epsilon and does not widen image comparison tolerances.
The actual same-capture recomparison is recorded in [VERIFICATION.md](VERIFICATION.md).

A configurable byte alpha cutoff rejects alpha at or below the
cutoff before writing any buffer. Source-over blending computes straight RGBA;
there is no implicit gamma/sRGB transfer. Translucent ordering is the caller's
draw order; callers can disable depth/ID writes for their explicit policy.
An occluding fragment with `id=None` clears a previous ID when ID writing is
enabled. These policies are named reference choices, not claims that every
source shader or engine backend already matches them.

## Tests and remaining acceptance

The package's analytic and adversarial suites cover transform order/inversion,
shortest-path quaternions, handedness/depth, scaled ray slabs, invalid finite
data, serialization, atomic admission, identity churn, stale picks, per-category
LRU/pins/drop/reset, derivation identity, source-over/alpha/depth/IDs, shared
triangle edges, clip-space mesh geometry, texture masks, perspective colors,
and atomic malformed mesh/image rejection.

The extended local core suite passed 73 tests: the original 46, two explicit
depth-comparison regressions and 25 derivative tests. Separate independent source,
lifecycle, memory and PNG evidence is linked in the verification ledger.

All assets in tests are synthetic literals. No original game assets or source
excerpts are distributed in the shipping crate. Native tests and CPU pixel
assertions do not qualify a GPU backend, measure GPU performance, or establish
native/WASM numerical equivalence. Those remain separate integration gates.
