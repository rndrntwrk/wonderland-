# Source world presentation

`wonderland-world-view` is the normalized source-document and rendering boundary
for the integrated lot view. It uses the imported C `render-3d` geometry builder
and the existing `render-core::reference::ReferenceSurface` color, depth and ID
buffers. `apps/web-shell/src/world_renderer.rs` displays those rasterized pixels
on a Canvas 2D surface, including browsers without WebGL2.

## Source inputs

`WorldDocument::original_empty_lot()` parses the byte-for-byte original
`TSOClient/FSO.Content.TSO/Content/Blueprints/empty_lot_fso.xml`, copied into
`apps/web-shell/public/assets/world/empty_lot_fso.xml`. This source has 77×77
tiles, five initial source VM stories, 1,269 floor records, no walls or pool
records, and 18 object records. Ten objects are off-world controllers; eight
are placed objects whose original model/texture resources are unavailable in
this checkout. The actual XML is an empty lot, not a furnished house.

`WorldDocument::from_blueprint_xml(xml, origin, source_revision)` follows
`XmlHouseData` and `VMWorldActivator`: XML architecture levels are zero based,
object levels are one based with zero off-world, and patterns, styles,
directions and groups remain source fields. It rejects malformed structure,
duplicate coordinates, nonfinite or out-of-range data, DTD/entities and
unbounded allocations before producing geometry. Effective source identity is
SHA-256 of the original XML bytes.

The source VM constructor initializes this particular blueprint's terrain to
raw zero. This is not a generated landscape. For supplied heightfields,
`source_terrain` implements `TerrainComponent.GetElevationPoint`'s raw-zero
right/bottom edge, while separately preserving
`VMArchitectureTerrain.RegenerateCenters`' wrapped-neighbor integer average.

The browser's pure `snapshot_world(&Snapshot, epoch, tick,
presentation_generation)` adapter converts decoded original FSOv38 snapshots.
It retains the source dimensions, all floors and wall records, raw height/grass
arrays, terrain classes, roof style/pitch, object GUIDs and 1/16-tile positions.
`WorldObject.snapshot` retains short ObjectID, persistent ID, source record,
raw position, avatar distinction and presentation generation. The original
`OUT_OF_WORLD=(-32768,-32768,1)` sentinel stays hidden. The object contact
interpolator follows `Blueprint.InterpAltitude`'s interior clamp rather than
the terrain mesh edge rule; game-object positions subtract the source half
tile exactly once before the mesh's graphics-center transform.

## Identity and revision contract

`WorldDocument` has schema version 1. Source kind, path/origin, original
revision and effective content key are mandatory. Its lot, material and model
data are owned values, with no browser/GPU handles. A source XML record or a
snapshot's reusable short ObjectID is never forged into a live `EntityRef`.
`WorldRevision`'s u64 fields and both 64-bit dynamic sprite banks serialize as
decimal strings so JavaScript cannot round them.

| Input | Revision and identity meaning |
| --- | --- |
| `OriginalXml` | Source file hash; no live lot or entity identity |
| `LegacySnapshot` | Receiver presentation generation; exact source IDs remain separate; no claim of deterministic VM restore |
| `LiveSession` | Admitted database lot ID, epoch, tick, architecture revision and versioned `EntityRef` values |
| `SavedWorld` / `TestFixture` | Explicit producer provenance; neither implies live admission |

In original FSOv, `VMTSOLotState.LotID` is a packed city location, assigned
from `LotPersist.location` by `LotContainer.cs`. The snapshot adapter records
it as such in provenance and leaves `WorldRevision.lot_id=None`. Directory DB
identity must come from a separately validated session. The snapshot contains
no authoritative entity generation or architecture revision; the supplied
presentation generation expires picks but cannot authorize a command.

The renderer admits live frames through the existing `FrameStore`. It rejects
unversioned changes, deleted/reused IDs and stale picks. Camera redraws,
replacement frames and device reset invalidate displayed pick generations.
Within an unchanged snapshot boundary, a changed document requires an
increasing presentation generation. Rejection preserves the accepted frame.

## Public browser interface

```rust
#[component]
pub fn WorldViewport(
    #[prop(into)] world: Signal<Arc<WorldDocument>>,
    controls: RwSignal<ViewportControls>,
    on_pick: Callback<WorldPick>,
) -> impl IntoView
```

The caller owns controls and handles accepted picks. `ViewportControls` has
yaw/pitch, zoom, pan, one-based visible level, `WallMode::{Up,Down,Cutaway}`
and a roof toggle. The camera, floors and wall cuts change the same meshes
used for visible pixels and picking. Pointer drag pans, Shift/right drag
rotates, and wheel/pinch zooms. Keyboard controls are arrows, Q/E, +/−,
Page Up/Down, Home and Enter. Events use canvas backing coordinates after
CSS scaling. Drawing coalesces through requestAnimationFrame and has explicit
teardown on component disposal.

`WorldPick` carries the accepted `WorldRevision`, displayed frame generation,
backing-store pixel, and a typed target:

```rust
WorldPickTarget::Tile { x, y, level, surface }
WorldPickTarget::Object { entity, source_guid, source_record }
```

Architecture has no fake live entity ID. The private hit surface uses local
indices exclusively inside the renderer. Both color and hit buffers receive
the same triangles, texture alpha and depth comparison. Ownerless walls clear
IDs and occlude objects. Source object draws use the original LessEqual tie
rule; the shared reference surface retains its strict Less default elsewhere.

## Source geometry and content

The imported geometry builder retains source tile size 3 graphics units,
story height 8.85, terrain height `(raw-base)*9/160`, diagonal half floors,
wall thickness/caps and source roof/pool topology. The adapter accepts explicit
room/support/cutaway data. Missing room maps are not inferred from floors;
unavailable cutaway maps leave walls up with a diagnostic. Roof metadata is
retained even when a snapshot lacks the room graph needed to emit a roof.

The pool loader uses all 16 original `pool_hq_*.obj` variants, four corner
meshes and `pool.png` from `TSOClient/tso.content/Content/3D/floor`. It applies
the exact modelled-floor origin/UV conversion. Hashes and source revision are
in the asset family's `provenance.json`. A pool replaces its source terrain
tile, allowing the authored below-ground bowl to remain visible. The world
adapter always supplies these assets; it does not use the C fixture pool
fallback as original content.

Normalized FSOm models preserve effective content keys, all material groups,
static and dynamic parts, texture identity, UV scale, version and bounds.
No XML GUID is replaced by a made-up prop. The checked-in IFF/FSOm resource
audit found no original models for the eight placed empty-lot GUIDs. Missing
floor/wall/roof textures retain geometry with a clearly diagnosed neutral
material. The source Christmas-tree meshes are not assigned to unrelated
objects merely because the files exist.

## Implemented limits and remaining parity

This adapter currently executes the software rendering path. It has no GPU
backend, source weather/AA/LOD integration, shadow/lightmap preparation or
portal/normal-mask stencil execution. A model requiring those masks is
explicitly unavailable rather than drawn with incorrect visibility. Raster
materials use nearest/clamp sampling, a fixed vertex-light approximation and
straight-alpha compositing; translucent texels carry an explicit diagnostic
because the source alpha-channel accumulation differs.

Source avatar Vitaboy visuals and object resources require provider resolution.
Snapshots preserve their actual entity records and report missing visuals.
Contained-object SLOT/bone positioning and multi-part ground-contact
adjustments need the source object/placement projection. Room/support graphs,
cutaway maps and architecture name-map resource resolution are separate
inputs; unavailable information is not invented. These boundaries do not
prevent the available terrain/floors/walls from being rendered and picked.

Resource bounds are 256 tiles per dimension, 16 stories, 262,144 cells, 65,535
objects/materials/models, and bounded vertex/texture budgets. These are
allocation protections, not new gameplay rules. Software surfaces allow at
most 1,048,576 pixels; the browser scales its sampling surface to 393,216
pixels without altering lot dimensions, construction permissions or IDs.

`SceneBudget` also bounds the complete expanded view to 2,000,000 vertices,
6,000,000 indices, 262,144 parts and 256 MiB of accounted buffers by default.
`build_scene_with_budget` can supply a smaller allocation policy. A checked
preflight reserves visible instances together with terrain, floor halves,
walls/caps, authored pools and source roof rectangles before creating any
architecture meshes or prepared models. It conservatively includes all model
groups, including hidden dynamic groups, and accounts for temporary prepared
resource copies. The C builder receives the remaining vertex/index budget as
a second guard. Roof counting uses bounded source topology metadata, without
creating roof meshes. These budgets bound generated resources and work; they
are not a claim to measure every allocator bookkeeping byte.

Each prepared model's images and UV-adjusted, shaded meshes are retained once
in `Arc` buffers and shared by every instance. `ScenePart.mesh` is `Arc<Mesh>`;
ordinary mesh reads continue through dereference. Repetition cannot multiply
the source image or immutable mesh allocation. Instance transforms and pick
identities remain separate.

## Verification

```sh
cargo test -p wonderland-render-core -p wonderland-render-3d -p wonderland-world-view
cargo test -p wonderland-web-shell --test snapshot_world
cargo check -p wonderland-web-shell --target wasm32-unknown-unknown
```

Tests cover the complete original XML, source record counts and controllers,
nonflat terrain edges, strict decimal identities, invalid source input,
camera/floor/wall/cutaway/roof behavior, authored pool geometry/texture,
depth-correct object/tile picking, live generation reuse and stale snapshots.
Geometry fixtures and test cubes are explicitly identified as tests. No
native or browser test alone establishes live server admission or full game
parity.
