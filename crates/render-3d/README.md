# wonderland-render-3d

Pure presentation geometry for Wonderland's 3D lot and city views. The crate
consumes explicit visual inputs and emits owned `wonderland-render-core` meshes,
camera poses, environment values, and transition state. It owns no simulation
state, service connection, content filesystem, GPU device, or global random
generator.

The package uses Rust 1.75, an independent Cargo workspace, MPL-2.0, and
`#![forbid(unsafe_code)]`. Release builds keep overflow checks enabled.

## Run

From the repository root:

```sh
cargo test --manifest-path crates/render-3d/Cargo.toml
cargo test --release --manifest-path crates/render-3d/Cargo.toml
cargo fmt --manifest-path crates/render-3d/Cargo.toml -- --check
cargo run --release --manifest-path crates/render-3d/Cargo.toml --example facade_worker
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

## Remaining integration work

The crate accepts normalized data. It does not decode a complete FSOm archive
with all material/group/depth-mask payloads, open the content provider, resolve
external custom textures versus embedded MTEX, or produce legacy facade
day/night texture atlases. `MeshCandidate` is a normalized single mesh plus
mask kind; it is not the full archive schema. The explicit depth-pass planner
still needs a physical engine depth/stencil implementation and acceptance run.

Real pool/tree/roof/floor/wall texture assets, source character/object content
through the actual provider, two live directory destinations and admitted lot
frames, and target-device lighting/AA/LOD/weather performance remain separate
acceptance work. Terrain/floor contact and camera height constraints do not
establish movement legality or wall collision.

Detailed source constants, deliberate edge-case fixes, and acceptance boundaries
are in [geometry-source-notes.md](../../docs/swarm-c/geometry-source-notes.md).

## Verification

The current package passes 79 tests in release mode with overflow checks
enabled and passes `cargo fmt -- --check`. An independent run also passed all
79 debug tests, eight source-review regressions, and the 21 source QEM
comparisons. These checks cover pure inputs and CPU outputs; they do not
constitute the remaining provider, live admission, or physical rendering gates.
