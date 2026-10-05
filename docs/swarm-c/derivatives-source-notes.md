# Thumbnail and facade derivatives: source mapping and limits

## Implemented boundary

`crates/render-core/src/derivatives.rs` now prepares immutable normalized render
requests, produces real day/night thumbnail and facade atlas pixels through the
CPU reference rasterizer, and schedules those requests with bounded queued,
running and resident lifetimes. `tools/swarm-c/facade-worker` is a Rust 1.75
library/CLI that reads a bounded normalized request and writes PNGs plus complete
reproduction/provenance metadata. No authoritative simulation state is changed.

The adapter accepts source-prepared colors/textures or explicitly unlit materials.
Its uniform lighting multiplier is stated in the request. This is a usable CPU
derivative producer, not a claim that legacy room lights, shadows, reconstructed
sprite materials, GPU sampling or licensed content have already been reproduced.

## Source ownership and mappings

| Source | Behavior carried into the adapter | Boundary retained |
| --- | --- | --- |
| [`LotFacadeGenerator.GenerateWalls`](../../TSOClient/tso.world/Facade/LotFacadeGenerator.cs) | Source wall lengths, first-fit bins, camera height/distance, outside-side flip, atlas rectangles, one-pixel displacement and edge bleed | Caller supplies exterior room/fence classification and midpoint terrain altitude |
| [`LotFacadeGenerator.GenerateFloor`](../../TSOClient/tso.world/Facade/LotFacadeGenerator.cs) | Top-down camera, 3×2 atlas cells, one-pixel scissor inset, layer/object/roof selection and generated overlay slot | Caller supplies the actual normalized floor, terrain mask, object and roof meshes |
| [`LotThumbContent`](../../TSOClient/tso.client/Rendering/City/LotThumbContent.cs) | Separate disposable derivative identities, bounded residency, dead-request rejection, last-use eviction and held resources | API/CDN fetch and decode are outside this CPU worker |
| [`CityFacadeLock`](../../TSOClient/tso.client/Rendering/City/CityFacadeLock.cs) | Owned pin/lease lifetime for a specific installed facade | City visibility, neighborhood spatial queries and GPU uploads remain in the city client |
| [`FSOFacadeWorker.Program`](../../TSOClient/FSOFacadeWorker/Program.cs) | Separate daytime/nighttime prepared states, bounded standalone artifact production | No save loading, service authentication, simulation light mutation, service upload or deployment is invented |
| [`WorldPlatform3D.GetLotThumb`](../../TSOClient/tso.world/Platform/WorldPlatform3D.cs) and [`WorldPlatform2D.GetLotThumb`](../../TSOClient/tso.world/Platform/WorldPlatform2D.cs) | Immutable camera/material inputs and explicit thumbnail output | Source camera centering, buildable-area selection and 2D sprite preparation still have to be supplied by the world-view adapters |

### Wall atlas equations

The constants retain the source defaults: eight pixels per tile, 22-pixel wall
height, maximum wall width 64 tiles (512 atlas pixels), and one-pixel edge gaps.
For endpoints in 1/16-tile coordinates, physical length is their Euclidean
distance divided by 16. The pixel length is source round-to-nearest-even of
`physical_length × 8`, capped at 512. Degenerate or subpixel segments are rejected
instead of issuing an empty scissor rectangle.

The first-fit bin check adds a leading gap only when a row already contains a
wall. The trailing gap is charged after the fit check, matching the source rule
that no extra space is required after a wall reaching the texture edge. A row
advances by `22 + 2 × 1 = 24` pixels. Texture height is
`ceil_to_four(max(1, row_count × 24 − 2))`; an empty wall atlas is therefore 512×4.
All dimensions, view counts and output bytes are checked before rendering.

The source midpoint is `(p0 + p1) / 32` tiles. The caller supplies the result of
the room-map outside test, which reverses both the wall normal and horizontal
projection when required. The eye is one tile outside the midpoint, converted
to graphics units with a factor of three. Camera height is
`(floor + 0.5) × 2.95 × 3 + midpoint_altitude × 3 + 0.2`.
The orthographic width is `3 × physical_length`, height `2.90 × 3`, and near/far
planes are 0 and 6 graphics units.

The source atlas translation includes a `+2 / atlas_height` term in clip Y. The
local wall surface retains the equivalent `+2 / 22` clip offset, which moves the
image upward by one output pixel. This term is not silently removed as a presumed
half-pixel workaround. After rendering, the nearest edge pixel is copied to each
available surrounding row/column, including corners, matching `BleedRect`.

The CPU implementation renders each disjoint wall cell separately and copies it
into its recorded atlas rectangle. This preserves the viewport/scissor equations
without requiring an entire GPU atlas surface for each wall. Reference clipping,
nearest clamped texture sampling, two-sided triangles, straight byte RGBA blending
and the source `LessEqual` depth test remain explicit. Exact GPU rasterization and
premultiplied/bilinear facade-consumer equivalence are separate checks.

The blend equations are deliberately named as well: the normalized CPU output
uses straight-RGBA Porter–Duff source-over, with alpha
`source_alpha + destination_alpha × (1 − source_alpha)` and RGB divided by the
resulting alpha. The repository's MonoGame `BlendState.NonPremultiplied` uses
SourceAlpha / InverseSourceAlpha factors for **both RGB and alpha**, without that
RGB normalization; its alpha therefore includes `source_alpha²`. Partially
transparent source GPU render targets need not equal these normalized PNG bytes.
An independent probe of the unchanged MonoGame assembly confirmed those blend
factors and the default LessEqual depth function. Exact legacy blend/storage
conversion remains separate from this supported CPU output contract.

### Floor atlas and layer selection

Cell size is `floor_resolution_per_tile × floor_tiles`; the default 64×2 produces
a 384×256 atlas with six 128×128 cells. Source slot `i` occupies column `i % 3`,
row `i / 3`. The camera eye is `(lot_width × 1.5, 200, lot_height × 1.5)`, looking
vertically downward to Y=0 with +Z as the up vector. Orthographic width and height
are `floor_tiles × 3`, with depth range 0..400. The resulting top-down image
reverses ground X and uses ground Z as camera up, matching the source equations.

Each cell retains its one-pixel scissor inset. Ordinary floor `i` draws normalized
terrain floor `i`, the ground mask for floor zero, objects on one-based level
`i + 1`, and source-selected roof levels when `roof_on_floor` is enabled. The
object-only overlay draws level-one objects. Wall cells draw wall meshes plus
objects meeting the source floor threshold; homogeneous clipping supplies the
visible-region rejection. Draw insertion order is preserved, so callers can
supply source world ordering. Invisible frame-owned objects are omitted.

Two source issues are kept visible rather than hidden behind a parity claim:

1. `GenerateFloor` places its object-only overlay in slot `stories` when that
   slot is reached. The separate `GetFSOF` raised-floor mesh points to slot 5.
   For fewer than five stories these are different slots. This worker describes
   the **generated** layout in `AtlasRegion` metadata and does not write an FSOF
   mesh/container that would imply the separate UV convention has been resolved.
2. `GenerateWalls(..., justTexture: true)` allocates/renders a new target but only
   assigns `WallTarget` inside the `!justTexture` branch. `GetFSOF` then reads
   `WallTarget` for the night wall bytes. The CPU adapter deliberately owns and
   emits separate daytime/nighttime wall images; it does not reproduce that
   retained-daytime-target lifetime defect. The source's second wall-bleed loop
   also advances its X position only in the non-texture-only branch, which this
   immutable per-phase implementation avoids.

### Thumbnail scope

Both original world platforms force the 2D camera, Far zoom, TopLeft rotation and
the highest story. The source uses 576×576 pixels for TSO, with different TS1
size/zoom behavior, computes `GetThumbCenterTile`, and applies buildable-area
cropping. The new thumbnail accepts its complete clip matrix and ordered
normalized geometry explicitly. Its procedural fixture uses a 256×256
orthographic camera; this is a declared fixture choice, not a replacement claim
for the source centering/cropping algorithm or a full 2D sprite thumbnail.

## Generation, completion and memory ownership

The queue admits frames through the existing validated `FrameStore`. Invalid
frame/entity data cannot replace a good frame. Every accepted frame change or
explicit reset advances a monotonic derivative lifetime. Equality of a content
hash is insufficient to validate a callback: A→B→A changes, same-boundary resets,
same-key supersession and a different queue instance all reject old work.

A ticket contains an opaque queue identity, a frame lifetime, a request serial
and its derived key. Queue identity uses an owned allocation rather than a
caller-chosen integer that could collide after recreating a controller. Request
serials never reset. Counter exhaustion fails admission instead of wrapping.
Prepared requests and artifacts expose only shared immutable references; public
input structs must pass validation again to become a new prepared request.

Cancellation, explicit eviction, LRU eviction and reset remove queued payloads
immediately. They invalidate running work without erasing its count or byte
reservation. The owned running handle acknowledges final release through
completion or `Drop`. A late failure/completion only clears its own serial, so it
cannot remove the replacement request. The renderer checks cancellation between
views and draws; one already-running reference draw is bounded by admission work
limits and completes before the next check.

Cache admission preflights both bytes and entry count before mutating any existing
entry. Unpinned least-recently-used entries are eligible for deterministic
eviction. A lease pins an exact installed generation, including when several
same-key generations overlap. Reset or replacement moves held entries to a
retired table, removes them from lookup, and retains their bytes until the final
lease drops. This avoids the source's shared mutable hold counters and the
`ReleaseLotThumb` dictionary-selection defect, while retaining intended ownership.

Pending reservations include input vector capacities, textures, view commands,
the artifact/image/region records, output pixels, the largest reference surface,
shaded/transformed mesh scratch and an 8 KiB clipping cushion. Resident accounting
includes owned RGBA vectors and artifact metadata. Allocator bookkeeping, thread
stacks and separately cloned caller data are not represented as payload bytes.
Neither queue contains GPU handles or claims GPU memory measurements.

## Validation and remaining algorithms

The final Rust 1.75 run passed **73 core tests**: the existing 46, two explicit
depth-comparison tests, and 25 new derivative tests. The worker passed four tests.
The actual six-image CLI fixture and encoded-request rerun produced eight
byte-identical files. Its artifact SHA-256 is
`99796348fce39734e1e89f44e3c4eacf9a05e94fc58f26cc79fe3c19b3fc2800`.
The normalized request reserves 3,732,912 payload bytes, produces 1,607,144
resident bytes, and admits 31,788,844 deterministic work units. These figures are
synthetic fixture measurements, not runtime/client performance claims.

Independent review approved the implementation after six additional
lifecycle/decode/depth challenges, including a real coplanar red→green check.
A C# comparison against the repository's unchanged MonoGame math checked 23 wall
rectangles, 368 matrix coefficients and 115 transformed points. Maximum atlas
position difference was 0.000040875 pixels against a 0.001-pixel tolerance.
Independent Pillow decoding checked all six PNGs, the eight identical output
files, 4,982 bleed pixels and every used floor-cell border. The small text
[`synthetic-verification.json`](../../tools/swarm-c/facade-worker/fixtures/synthetic-verification.json)
records these results and each actual PNG hash; the CLI/verifier reproduce the
image files without committing binary build artifacts.

The implementation is covered by source-equation, deterministic-render,
frame/owner/material validation, malformed geometry, dimension/work/byte limit,
supersession, stale completion, cancellation, reset, eviction and held-residency
regressions. The standalone verifier renders all six fixture images twice,
compares eight output files byte-for-byte, and independently checks PNG CRCs,
zlib termination, dimensions, decoded RGBA hashes and distinct day/night outputs.
It also requires graceful rejection of five malformed request forms and preserves
an existing output directory. The committed verification manifest records the
actual final-run hashes and counts.

An independent allocator challenge with 512 tiny views measured 439,104 bytes of
owned/peak memory against a 448,832-byte reservation after artifact metadata and
clipping scratch were included. This is a specific synthetic accounting check,
not a universal process-RSS or GPU memory benchmark.

The following source algorithms still require separate implementation or
integration; successful synthetic PNG generation does not close them:

- Legacy room/outside light-map evaluation, per-room shadowing, object light
  enablement and the complete day/night color pipeline. The original worker
  prepares noon with lights off and midnight with lights on; this adapter consumes
  those states as explicit material/light inputs.
- Source room/fence exterior extraction, room-map side classification,
  `InterpAltitude`, source draw ordering, fine/buildable-area masks and complete
  source terrain/object/roof material conversion. The normalized request records
  the supplied decisions and hashes the actual resulting geometry/textures.
- Normalized FSOM/depth-mask material composition and the renderer-specific
  techniques needed for reconstructed sprite geometry. That adapter is separate
  from this worker and must supply supported normalized meshes/materials.
- `GetFSOF` floor/roof geometry assembly, its raised-floor/overlay UV convention,
  DXT5 compression, binary container writing and the consumer's premultiply /
  bilinear / divide-out / alpha-threshold behavior. Output here is RGBA PNG plus
  exact camera/atlas metadata.
- Live city/neighborhood scheduling policy, service fetch/admission, GPU texture
  upload and device-loss ownership, and independent licensed-content visual
  comparison. The queue/library provides lifecycle building blocks without
  inventing those client/provider integrations.
