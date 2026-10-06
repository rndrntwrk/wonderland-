# Thumbnail and facade derivatives: source mapping and ownership

## Implemented path

The derivative path prepares immutable source-world geometry and cameras,
renders bounded day/night image jobs, writes the original `FSOf` v1 container,
reads legacy RGBA/DXT5 containers, and owns CPU and GPU lifetimes across source,
lot, content, and device changes. The core and standalone worker remain Rust
1.75 and have no engine or simulation dependency.

The implementation is split into:

- [`derivatives.rs`](../../crates/render-core/src/derivatives.rs): validated
  materials and draws, camera/atlas planning, CPU rendering, incremental tasks,
  queue admission, cancellation, result installation, and held leases.
- [`derivatives/source.rs`](../../crates/render-core/src/derivatives/source.rs):
  Blueprint altitude, room/exterior extraction, bounded topology fallback,
  original thumbnail camera/crop, and ground/floor/overlay/roof/wall geometry.
- [`derivatives/fsof.rs`](../../crates/render-core/src/derivatives/fsof.rs):
  original container field order, bounded raw/gzip decode/write, BC3 texture
  decoding, and safe mesh conversion.
- [`derivatives/upload.rs`](../../crates/render-core/src/derivatives/upload.rs):
  backend resource ownership, pending/resident/retained budgets, source and device
  generation checks, and stale upload disposal.
- [`city/facade.rs`](../../crates/render-3d/src/city/facade.rs): original container
  admission into separate textured floor and wall meshes using the existing city
  transform. FSOf vertices are already in tile units and are not divided by three
  a second time.
- [`facade-worker`](../../tools/swarm-c/facade-worker/README.md): portable source
  requests, actual PNG/FSOf output, deterministic replay, and independent decoder
  verification.

The client adapter consumes its actual `WorldDocument`/`PreparedWorld`. Exact
`WorldDerivativeSourceMetadata` may supply room base aliases, source wall/fence
lines, and FineArea; missing exact metadata stays absent. Static XML objects
remain scene draws rather than invented live entities. Floor and roof levels
come from each prepared source part. The adapter orders terrain before coplanar
authored flooring for the derivative's LessEqual depth contract.

Night rendering requires explicit source-prepared material passes and outside
color. The companion is bound to both the external revision and the complete
source-document digest. Missing night data produces three day images, no night
image files, no night metadata phase, and an FSOf with its night flag clear.
Neither the worker nor the UI changes simulation time or manufactures room-light
color from a visual tile ID.

## Original source mapping

| Source | Implemented behavior | Required source input |
| --- | --- | --- |
| [`LotFacadeGenerator.GenerateWalls`](../../TSOClient/tso.world/Facade/LotFacadeGenerator.cs) | Exterior base-room lines followed by fences, room-map side test, midpoint altitude, wall camera, first-fit bins, gaps and bleed | Exact room tables/lines when available; otherwise validated source wall tiles |
| [`LotFacadeGenerator.GenerateFloor/GetFSOF`](../../TSOClient/tso.world/Facade/LotFacadeGenerator.cs) | Top-down selection, one-pixel inset, ground subdivisions and averaging, copied floors, raised overlay, source roof/wall UVs | Actual prepared terrain, floor, object and roof meshes/materials |
| [`Blueprint`](../../TSOClient/tso.world/Model/Blueprint.cs) | Raw altitude interpolation, base altitude, rooms, floor-use detection, FineArea/buildable bounds, thumbnail center | Source altitude and optional exact masks |
| [`WorldCamera`](../../TSOClient/tso.world/Utils/WorldCamera.cs) and camera controller | Far/TopLeft TSO and TS1 thumbnail size/zoom, center quantization, source view/projection | Lot dimensions, altitude and optional FineArea |
| [`FSOF`](../../TSOClient/tso.files/RC/FSOF.cs) | Header/body order, gzip, RGBA/DXT5 textures, day/night flag/color, two 32-byte vertex streams and index arrays | Original bytes or validated generated images/geometry |
| [`LotThumbContent`](../../TSOClient/tso.client/Rendering/City/LotThumbContent.cs) and [`CityFacadeLock`](../../TSOClient/tso.client/Rendering/City/CityFacadeLock.cs) | Disposable identities, bounded residency, held lifetimes, dead-result rejection | Requested lots and actual backend resource objects |
| [`FSOFacadeWorker.Program`](../../TSOClient/FSOFacadeWorker/Program.cs) | Separate day/night states, floor size/resolution, worker subdivisions and standalone output | Admitted world/materials; authoritative save admission and publishing remain external |

### Source rooms and terrain

`SourceRooms.rooms` is indexed by original room ID including sentinel zero. Each
room preserves `id`, `base`, zero-based floor, outside classification, wall lines
and fence lines in integral sixteenths. The packed map keeps both low/high u16
halves. Only outside rooms whose base equals their own ID emit facade lines.
The side test samples a point 0.6 tiles along the midpoint normal, reads the
map's first room half, resolves its base room, and reverses the wall when needed.

Documents without exact room records use a bounded two-half-tile connectivity
pass over supplied walls and diagonals, permeable fences, outer edges, and
explicit indoor flags. Adjacent collinear exterior segments are merged. Floor
paint alone does not establish an enclosed room. This fallback does not claim to
recover original room IDs or base aliases.

Raw `Blueprint.Altitude` contains width × height i16 samples. A client with
(width+1) × (height+1) corner storage extracts the first width entries of each of
the first height rows. Interpolation uses the original base/next-cell clamps,
fractional remainder, base altitude and 3/160 terrain factor. Floor use is the
highest story with patterns or wall segments, with a minimum of one; objects and
roofs do not invent a used architectural floor.

### Wall atlas equations

Defaults remain eight pixels per tile, 22 pixels high, 512 atlas pixels wide,
and one-pixel gaps. Tile length is sixteenth-coordinate Euclidean length divided
by 16. Pixel length is round-to-nearest-even of length × 8, capped at 512.
Degenerate segments fail before scissor allocation. First-fit bins charge a
leading gap only on nonempty rows; the trailing gap is charged after the fit
check. Rows advance by 24 pixels. Height is
`ceil_to_four(max(1, row_count × 24 − 2))`; an empty atlas is 512 × 4.

The midpoint is `(p0+p1)/32` tiles. The eye sits one tile outside, converted to
graphics units by three. Camera height is
`(floor+0.5) × 2.95 × 3 + midpoint_altitude × 3 + 0.2`. Orthographic width is
`3 × tile_length`, height is `2.90 × 3`, and near/far are 0 and 6. The source
`+2/atlas_height` Y translation remains `+2/22` on each local surface, displacing
rendered walls upward one pixel. Available bordering pixels, including corners,
receive the nearest edge value. Disjoint strips are rendered separately and
copied into the planned atlas, preserving the original viewport/scissor equations.

### Floors, overlay and roof geometry

Defaults 64 floor tiles × 2 pixels per tile produce six 128 × 128 cells in a
384 × 256 atlas. Slot `i` occupies `(i % 3, i / 3)`. The eye is
`(lot_width × 1.5, 200, lot_height × 1.5)`, looking down to Y=0 with +Z up.
Projection width/height are floor tiles × 3 and depth is 0..400. Ground X is
reversed; ground Z points up. Each cell retains the one-pixel scissor inset.

Floor `i` draws its terrain/floor, the source ground mask on floor zero, objects
on one-based level `i+1`, and source-selected roofs. The object-only cell draws
first-floor objects. Wall strips include objects above the source floor
threshold. Invisible frame-owned objects are omitted.

The worker subdivision default is five, matching `FSOFacadeWorker`; the original
generator class alone defaults to ten. The 64-tile square starts at
`(lot_size−64)/2`, so a 77-tile lot spans 6.5..70.5. Vertices near inside cells
average those source altitudes; others use source interpolation. The base mesh
is copied at 2.95-tile story increments, plus an overlay at half a story. Roof
vertices convert graphics units to tiles once and receive source atlas UVs.
Wall rectangles produce four vertices and six indices per strip.

Two original defects remain explicit:

1. `GenerateFloor` places the object texture in slot `stories`, whereas `GetFSOF`
   points the raised overlay at slot 5. For fewer than five stories these differ.
   Both source conventions are preserved, rather than silently moving atlas cells.
2. The texture-only night wall call does not replace `WallTarget`, and a bleed
   X increment also sits inside the non-texture-only branch. This implementation
   owns independent night images and fixes those target/bleed lifetime errors
   rather than copying retained daytime wall pixels into night.

The original combined-floor normal generation indexes only initial floor
indices. Generated geometry computes finite normals over all faces. Legacy
FSOf reading preserves normal float bits, including NaNs; explicit conversion
to a render mesh repairs invalid/zero normals.

### Original thumbnail camera

Both source platforms force the 2D camera, Far zoom, TopLeft rotation and highest
story. TSO is 576 × 576 at precise zoom 0.25; TS1 uses lot width × 16 and precise
zoom 0.5. Source Far half-tile dimensions are 16 × 8. Center quantization,
Y half-pixel adjustment, two-pixel test-vector correction, 315° Y and 30° X
angles, projection scale and signed near/far constants are preserved.

Without FineArea, the buildable rectangle is `(6,6,width−13,height−13)` and the
center is the integer lot midpoint. FineArea uses inclusive mask bounds and the
original far-camera altitude correction. Floor/terrain draws use source tile
placements; multi-tile batches must declare one. Objects are filtered by FineArea
when present. Walls and roofs retain source inclusion. The normalized fixture
still uses its explicit 256 × 256 camera; `source-fixture` exercises 576 × 576.

## FSOf format and bounds

The header is `FSOf`, little-endian version 1, and a raw/gzip byte. The body is
compression type, floor width/height, wall width/height, night flag, counted day
textures, optional counted night textures and packed RGBA light color, then the
floor and wall meshes. A vertex is position XYZ, UV XY, normal XYZ: eight
little-endian f32 values. Signed i32 wire indices must reference their own mesh.

RGBA mode 0 and BC3/DXT5 mode 1 support reading and preserving/writing encoded
containers. New artifacts use lossless RGBA inside gzip; there is no new BC3
encoder. BC3 decoding implements both alpha palettes, its always-four-color
color palette, and partial edge blocks.

Default limits: 128 MiB encoded, 128 MiB decoded, 4096 per dimension, 32 million
combined texture pixels, two million combined vertices, six million combined
indices. Counts and exact texture sizes are checked before reservation.
Streaming gzip bounds expansion and checks checksum/end, rejecting appended or
concatenated members and trailing body fields. Writing validates before output
and bounds the stream.

## Scheduling, invalidation and GPU handoff

Validated `FrameStore` admission establishes a monotonic lifetime. Changed actual
bytes, A→B→A transitions, explicit same-boundary reset, same-key supersession,
and new queue instances reject old work. Tickets bind opaque queue identity,
lifetime, monotonically increasing attempt and key. Exhausted counters fail
admission. Keys hash actual source topology, masks, altitude, geometry, textures,
camera, draw order, frame/entity generations, lighting and algorithm version 2.

`DerivativeJob::execute` is synchronous. Browsers use `into_task()` and
`DerivativeTask::step(max_draws)` to yield between bounded draw batches.
Cancellation checks occur between views/draws. Cancellation/reset removes queued
payloads immediately but retains executing tasks' counts/bytes until completion
or Drop. Failed or stale attempts cannot remove replacements.

Images and source geometry remain pinned by exact-generation CPU leases. Reset
hides old entries while their bytes remain charged until the last lease drops.
Admission preflights byte/entry limits before evicting an unpinned LRU entry.
A stale lease retains immutable owned bytes, reports `is_current()==false`, and
cannot be uploaded.

`DerivativeGpuCache<T>` takes the actual backend resource type, reserves packed,
aligned and staging allocations before upload, and pins the CPU lease for the
GPU lifetime. Tokens bind source digest, frame stamp, key, attempt and device
generation. Completion rechecks lifetimes before installation; rejected objects
run their real destructor. Pending jobs and retained leases remain charged after
reset. The client owns a separate WebGL2 facade canvas, uploads actual textures
and 32-byte geometry, waits on a real fence, and deletes texture, buffer,
framebuffer and program handles through Rust ownership.

Budgets include vector capacities, output pixels and geometry, image/region
records, largest reference surface, transformed mesh scratch and clipping
cushion. Work bounds use conservative projected rectangles, up to seven clipped
triangles and vertex traversal; eye-plane crossings use the full surface.
Browser upload reserves GPU payload, both staging copies and preview surfaces.
Allocator metadata, thread stacks, opaque driver storage and unrelated caller
copies are not claimed as exact process/GPU memory measurements.

## Evidence and practical limits

Focused suites cover 26 renderer/queue tests, five original-container tests,
nine source-world/camera/topology tests, and four GPU ownership tests. The worker
has five tests and the city consumer has a source-container transform test.
The source verifier independently reads actual FSOf using Python struct/bounded
gzip and decodes each PNG with CRC-checked zlib. It compares nine replay files,
container pixels against decoded PNGs, source geometry/camera dimensions,
day-only absence, five malformed requests, and existing-directory preservation.

Current hashes and measured reservation/work counts are recorded in
[`source-verification.json`](../../tools/swarm-c/facade-worker/fixtures/source-verification.json)
and [`synthetic-verification.json`](../../tools/swarm-c/facade-worker/fixtures/synthetic-verification.json).
These are synthetic evidence, not licensed-content or GPU pixel identity proof.
The earlier independent MonoGame matrix comparison checked 23 rectangles,
368 coefficients and 115 points; its maximum atlas difference was 0.000040875
pixels against a 0.001-pixel tolerance. Historic review/allocator measurements
are labeled separately from current algorithm measurements.

CPU output specifies nearest-clamped sampling, two-sided triangles, LessEqual
depth and normalized straight-RGBA Porter–Duff source-over. MonoGame
`NonPremultiplied` applies SourceAlpha/InverseSourceAlpha to both RGB and alpha,
so its target alpha includes source_alpha². Partially transparent bytes can
differ. The city consumer has its own premultiplication, bilinear filtering and
alpha-threshold behavior. A valid source container and preview do not prove
original GPU bit/pixel parity.

Room/point/shadow lighting and material-specific shaders belong to the source
presentation producer. This adapter accepts prepared states; it does not infer
an unavailable night shader from room averages. Stencil/forced-depth source
passes require their supported backend or an explicit unavailable diagnostic.
Service authentication, save admission, city visibility policy and publishing
remain with their authoritative owners.
