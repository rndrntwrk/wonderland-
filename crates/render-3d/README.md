# wonderland-render-3d

Pure presentation geometry for Wonderland's 3D lot and city views. The crate
consumes explicit visual inputs and emits owned `wonderland-render-core` meshes,
camera poses, environment values, and transition state. It owns no simulation
state, service connection, content filesystem, GPU device, or global random
generator.

Imported from Swarm C commit `f6f78be1fef247f2db47e19f56d94054f0c9e88c`.
Its original source mapping is the preserved FreeSO baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`. The crate now participates in the
root Wonderland workspace and uses its shared render-core. The package retains
MPL-2.0 and `#![forbid(unsafe_code)]`; root profiles keep overflow checks enabled.

The root Rust 1.99 lint gate adds equivalent Rust source style corrections:
copy values directly instead of cloning them, enumerate the same bounded
simplifier deletion slice, and initialize test options directly. Two documented
item-level argument-count allowances preserve the source cubic helpers' explicit
sampling and interpolation inputs. Geometry arithmetic, topology order, public
layouts, and serialized fields are unchanged. The original `TSOClient` source
is unchanged.

## Run

From the repository root:

```sh
cargo test -p wonderland-render-3d
cargo run -p wonderland-render-3d --example facade_worker
```

The example writes OBJ geometry to stdout and its identity, counts, and missing
asset diagnostics to stderr. A checked-in result is
[fixtures/synthetic-facade.obj](fixtures/synthetic-facade.obj). Its inputs are
the original synthetic fixture, content key `[0x53; 32]`, revision 1, and Y
squish 0.5. It contains 488 vertices and 240 triangles. Repeated execution was
verified byte for byte.

## Main interfaces

| Area | Entry points | Output |
| --- | --- | --- |
| Lot | `lot::build_lot`, `rebuild_dirty`, `plan_rebuild` | Terrain, floors, walls, roofs, pools, and water as material-tagged mesh parts |
| Synthetic inputs | `lot::synthetic_lot()`, `city::synthetic_city()` | A sloped 6×6 two-story lot and a lossless city class-map fixture |
| Reconstruction | `reconstruction::reconstruct`, `select_params`, `derivation_key` | Grouped source-sprite meshes, texture identities, contact bounds, and derivation diagnostics |
| Simplification | `reconstruction::simplification::simplify_mesh` | A bounded source QEM edge-collapse result and target/work statistics |
| Resolution | `reconstruction::resolution::MeshResolver` | Explicit override/cache/reconstruction precedence with bounded residency |
| Normalized FSOm objects | `objects::PreparedFsom`, `ObjectMeshSlot` | Immutable multi-material groups/textures, ordered source draw contracts, bounded validation and generation-fenced replacement |
| City | `city::build_city_parts`, `build_near_patch_parts` | Class, blend, road-edge, and road-corner parts; secondary mask UVs |
| Facade | `city::facade::bake_facade`, `to_obj` | Content-addressed geometry, bounds, and validated OBJ text |
| Cameras | `camera::OrbitCamera`, `CityCamera`, `FirstPersonCamera`, `direct_pose` | Source-coordinate camera poses with checked projection |
| Environment | `environment::automatic_weather`, `decode_weather`, `time_of_day`, `choose_quality` | Presentation weather, sky/light color, and capability-limited quality policy |
| City/lot routing | `city::transition::CityLotTransition` | Session-bound admission/first-frame state and saved city intent |

A minimal geometry consumer:

```rust
use wonderland_render_3d::{
    city::{build_city_parts, build_near_patch_parts, synthetic_city, CityBoundary},
    lot::{build_lot, synthetic_lot, BuildOptions},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lot = synthetic_lot();
    let lot_parts = build_lot(&lot, &BuildOptions::default())?;

    let city = synthetic_city();
    let city_parts = build_city_parts(&city, CityBoundary::Rectangle, 20_000)?;
    let detailed_parts = build_near_patch_parts(
        &city, (1, 1), (2, 2), 4, CityBoundary::Rectangle, 20_000,
    )?;

    for part in &detailed_parts {
        let secondary_mask_uv = part.mask_uv();
        if let Some(uv) = secondary_mask_uv {
            assert_eq!(uv.len(), part.mesh.vertices.len());
        }
    }
    assert!(!lot_parts.parts.is_empty());
    assert!(!city_parts.is_empty());
    Ok(())
}
```

## Input and unit contract

`VisualLot` includes the cosmetic fields missing from the authoritative lot
projection: one-based level tiles, corner terrain heights and base altitude,
altitude centers, grass state, floor-half membership, indoor/support flags,
wall patterns/styles and object-style overrides, roof material/pitch/advanced
mode, roof average color, and roof texture scale. `VisualLot::flat` is an input
construction helper, not a content-provider adapter.

Lot output is in graphics units. A tile spans 3 units; a story rises
`2.95 * 3 = 8.85`; terrain graphics height is
`(raw_height - base_alt) * 9 / 160`. City output uses one map pixel per world
unit and elevation `red / 12`.

The source lot/city camera-center mapping uses the interior span 72 and offset
2. Facade geometry uses the separate source scale 1/77. `facade_transform`
accepts source tile units; `bake_facade` first divides lot graphics coordinates
by 3, so a 77×77 flat lot occupies one city tile.

`OrbitCamera` is a lot camera. `CityCamera` uses city coordinates directly,
including source orbit/first-person target heights and the city near plane.
`CityIntent.camera` therefore stores `CityCamera`.

## Material and topology details

The lot builder retains source per-path triangle ordering and explicit normals.
Adapters must bind the appropriate source material/raster state. The OBJ
exporter reorients each face to its supplied normals for ordinary mesh viewers.

Both diagonal floor orientations are supported; callers must supply the two
source half-floor IDs when they differ. Wall patterns and styles remain
separate output metadata. Roofs retain half-tile eligibility, source rectangle
expansion order, hip geometry, and advanced rim/underside/edge parts.

Authored pool input requires 16 tile meshes and four corner meshes, already
normalized to the source model convention. When those assets are absent, the
lot builder emits a synthetic pool surface/bowl and a diagnostic naming the
missing source OBJ/texture family. That fallback is not an authored pool asset.

City blend parts keep terrain UVs in `Vertex::uv`.
`CityPart::mask_uv()` returns the separate, unmirrored 7×3 blend-mask
coordinates for every vertex. Road UVs use the source mirrored 8×4 atlas.
Coarse tiles use a TL–BR diagonal; detailed patches use TR–BL. The merged
`build_city_mesh` and `build_near_patch` helpers are useful for geometry
inspection; material consumers should retain the parts and secondary UV channel.

`RendererWithFade` preserves the active renderer's ten-tile side fade.
`RendererDiamond`, `ServerDiamond`, and `LegacyMapData` preserve their distinct
boundary predicates. Canonical boundary modes require 512×512 channels.
Small synthetic maps explicitly use `Rectangle`.

## Reconstruction, identity, and resource limits

The reconstruction path preserves the source projection, byte-255 missing depth,
0.065 tile-unit discontinuity checks, DoorFix selection, counter correction,
dynamic groups, and optional alpha filtering. Simplification uses the active
source QEM schedule: triangle target divided by 100, aggressiveness 3.5, and
125 iterations. Afterwards it applies the source inverse-camera UV projection
and half-pixel offset before generating normals and converting units.

QEM reports `SimplificationTargetNotReached` if border/flip constraints leave
more triangles than requested. It returns an error on exhausted work or
reference limits. It no longer reports `SimplifierUnavailable`.

The source-sensitive QEM steps preserve MonoGame's intermediate f32 rounding,
reciprocal vector division, lerp order, and reciprocal-before-minor evaluation.
The shared core's robust vector math remains unchanged. An independent
comparison against the unmodified source C# simplifier matched all 21 small
fixtures exactly in ordered topology and position/UV bits. Permanent curved
surface regressions cover both intermediate collapse counts and final output.

Derived identities use SHA-256 over effective source/patch identity, full
parameter bytes, sprite metadata/channels, and algorithm version. Source sprite
ordinals survive a missing-depth entry. Ordinal 65535 is reserved for authored
custom texture payloads. The current depth derivation namespace is v2.

The resolver order is memory, user override, authored external override,
embedded normalized mesh, matching generated cache, and reconstruction.
Malformed or obsolete candidates fall through with a reason. Generated-cache
invalidation cannot grow an unbounded tombstone set; saturation conservatively
ignores all generated candidates while keeping authored candidates eligible.

Default geometry bounds are 2,000,000 vertices and 6,000,000 indices. Lot input
allows dimensions up to 256, up to 16 levels, and at most 262,144 cells.
Detailed city patches are at most 32×32 tiles with subdivisions 1 through 8.
Reconstruction bounds images to 4096 per axis and 16,777,216 pixels; counter
correction caps nearest-border comparisons at 20,000,000. The source QEM preset
caps visits at 100,000,000 and stored references at 8,000,000. Smaller budgets
can be supplied through the relevant builder/options.

## Transition ownership

Construct a city/lot transition with
`CityLotTransition::new(directory, intent, session_generation)`.
The transport/session owner must inject a nonzero generation that it never
reuses across reconnects or controller recreation. Both tickets and receipts
carry that generation; receipts also echo the request serial.

An accepted receipt must match the persistent destination, directory and
destination revisions, lot ID, and epoch. A matching first frame is required
before entering `InLot`. Returning to the city preserves saved camera intent.
This is routing consistency around an independently validated core frame.
`DirectoryProvenance::Fixture` is explicit, and no test claims live directory
or admission acceptance.

## Normalized FSOm object adapter

[`objects.rs`](src/objects.rs) consumes a complete bounded `NormalizedFsom` from
the content provider. It preserves source part order, resolved textures/UV scale,
static group zero, both 64-bit dynamic banks, masks and effective content identity.
Prepared meshes and identities are immutable. `ObjectMeshSlot` validates a
replacement before installing it and rejects stale entity/session/reset/content
completions without removing the current model.

`PreparedFsom::scene` emits actual borrowed meshes/materials plus ordered shader,
blend, culling, depth and two-sided stencil commands. Source world/lightmap
transforms, normal/portal mask passes and the disabled-room exception remain
explicit. The fragment-state oracle exercises those policies; engine integration
must create the corresponding real GPU resources and passes.

The source mappings, original C# normal/world/blend/depth comparisons, memory
limits and exact B/engine boundaries are in
[fsom-source-notes.md](docs/fsom-source-notes.md).

## Remaining integration work

The crate accepts normalized data. It does not decode a complete FSOm archive
with all material/group/depth-mask payloads, open the content provider, or resolve
external custom textures versus embedded MTEX. B supplies those decoded and
resolved values to the new normalized object adapter. `MeshCandidate` remains
the reconstruction resolver's single-mesh representation; it does not replace
the separate complete `NormalizedFsom` contract. Source depth/stencil/material
commands still need engine execution and acceptance.

The upstream C implementation also contains a separate CPU facade worker. It
is not imported by this geometry package. Legacy room-light/shadow preparation,
thumbnail centering/cropping, FSOF geometry/container/DXT5 output and consumer
blend/filter equivalence remain distinct source algorithms; production city
scheduling and GPU texture ownership also need integration.

The integration's [`world-view`](../world-view/README.md) adapter supplies the
original pool OBJ family and texture, parses the actual empty-lot XML, and
renders normalized source worlds with a software depth/ID buffer. Source
character/object resolution, unresolved roof/floor/wall textures, live directory
admissions and target-device lighting/AA/LOD/weather performance remain separate
acceptance work. Terrain/floor contact and camera height constraints do not
establish movement legality or wall collision.

Detailed source constants, deliberate edge-case fixes, and acceptance boundaries
are in [geometry-source-notes.md](docs/geometry-source-notes.md).

## Verification

The 105 imported tests pass in the integrated debug/test profile with overflow
checks enabled. They include 26 normalized-object tests. Upstream C separately
recorded release checks, eight geometry review regressions, 21 source QEM
comparisons and eight object challenges; those historical results are not new
browser or live-service verification. These tests cover pure inputs and CPU
outputs, with separate provider, live admission and physical rendering gates.
