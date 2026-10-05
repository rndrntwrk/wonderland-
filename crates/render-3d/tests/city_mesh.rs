use wonderland_render_3d::{city::*, lot::*, Error};
use wonderland_render_core::{RenderLimits, Vec2, Vec3};
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}
fn pixel() -> CityPixel {
    CityPixel {
        terrain: [0, 255, 0, 255],
        elevation: 0,
        forest: [0, 0, 0, 255],
        density: 0,
        road: 0,
    }
}

// Catches red-byte scale changes, swapped map axes, and road layer order/UV errors.
#[test]
fn city_geometry_uses_pixel_positions_heights_and_source_road_uvs() {
    let mut map = CityMap {
        width: 2,
        height: 2,
        pixels: vec![pixel(); 4],
    };
    map.pixels[1].elevation = 12;
    map.pixels[3].elevation = 12;
    map.pixels[0].road = 0x21;
    let parts = build_city_parts(&map, CityBoundary::Rectangle, 1000).unwrap();
    let ground = &parts
        .iter()
        .find(|p| p.kind == CityPartKind::Terrain(TerrainClass::Grass))
        .unwrap()
        .mesh;
    assert_eq!(ground.vertices[1].position, Vec3::new(1., 1., 0.));
    assert_eq!(ground.vertices[2].position, Vec3::new(1., 1., 1.));
    assert_eq!(ground.indices, [0, 1, 2, 0, 2, 3]);
    assert!(ground.vertices[0].normal.x < 0.);
    close(ground.vertices[0].normal.x, -0.70710677);
    let road = parts
        .iter()
        .find(|p| p.kind == CityPartKind::RoadEdge(5))
        .unwrap();
    assert_eq!(road.mesh.vertices[0].uv, Vec2::new(0.75, 0.));
    assert!(parts.iter().any(|p| p.kind == CityPartKind::RoadCorner(2)));
    let mesh = build_city_mesh(&map, CityBoundary::Rectangle).unwrap();
    assert_eq!((mesh.vertices.len(), mesh.indices.len()), (24, 36));
    mesh.validate(&RenderLimits::default()).unwrap();
    assert!(matches!(
        build_city_parts(&map, CityBoundary::Rectangle, 1),
        Err(Error::BudgetExceeded(_))
    ));
    map.pixels.pop();
    assert!(build_city_mesh(&map, CityBoundary::Rectangle).is_err());
}

// Catches unordered terrain blending and accidentally drawing void as grass.
#[test]
fn city_layers_skip_void_and_blend_toward_the_next_higher_neighbor() {
    let mut map = CityMap {
        width: 3,
        height: 2,
        pixels: vec![pixel(); 6],
    };
    map.pixels[1].terrain = [255, 255, 0, 255];
    map.pixels[2].terrain = [12, 0, 255, 255];
    map.pixels[3].terrain = [0, 0, 0, 255];
    let parts = build_city_parts(&map, CityBoundary::Rectangle, 1000).unwrap();
    assert_eq!(
        parts
            .iter()
            .filter(|p| matches!(p.kind, CityPartKind::Terrain(_)))
            .count(),
        5
    );
    assert!(parts.iter().any(|p| p.tile == (0, 0)
        && p.kind
            == CityPartKind::Blend {
                class: TerrainClass::Sand,
                mask: 14
            }));
    assert_eq!(
        parts
            .iter()
            .filter_map(|p| if let CityPartKind::Terrain(c) = p.kind {
                Some(c as u8)
            } else {
                None
            })
            .collect::<Vec<_>>(),
        vec![0, 0, 0, 1, 4]
    );
}

// Catches a discontinuity between coarse terrain and the subdivided patch edge.
#[test]
fn near_patch_hermite_edges_join_coarse_heights() {
    let mut map = CityMap {
        width: 5,
        height: 5,
        pixels: vec![pixel(); 25],
    };
    for y in 0..5 {
        for x in 0..5 {
            map.pixels[y * 5 + x].elevation = (x * 12 + y * 6) as u8;
        }
    }
    let patch = build_near_patch(&map, (1, 1), (2, 2), 4).unwrap();
    assert_eq!((patch.vertices.len(), patch.indices.len()), (100, 384));
    close(patch.vertices[0].position.y, 1.5);
    close(patch.vertices[4].position.y, 2.5);
    close(patch.vertices[20].position.y, 2.);
    assert!(build_near_patch(&map, (1, 1), (33, 1), 4).is_err());
    assert!(build_near_patch(&map, (1, 1), (1, 1), 0).is_err());
}

// Catches conflating the source 72 camera mapping with the 77 facade scale.
#[test]
fn city_lot_coordinate_mapping_and_facade_are_distinct() {
    let city = (306, 205);
    let center = lot_center_to_city(city, Vec2::new(38., 38.)).unwrap();
    assert_eq!(center, Vec2::new(306.5, 205.5));
    assert_eq!(
        city_center_to_lot(city, center).unwrap(),
        Vec2::new(38., 38.)
    );
    let transform = facade_transform(city, [12, 12, 12, 12], 0.5).unwrap();
    let p = transform.transform_point3(Vec3::new(77., 77., 0.));
    close(p.x, 307.);
    close(p.y, 1.5);
    close(p.z, 206.);
    assert!(facade_transform(city, [0; 4], f32::NAN).is_err());
}

// Catches RNG use from global/simulation state and omitted source road clearances.
#[test]
fn foliage_is_deterministic_and_respects_road_clearance() {
    let mut map = CityMap {
        width: 3,
        height: 3,
        pixels: vec![pixel(); 9],
    };
    map.pixels[4].forest = [255, 0, 0, 255];
    map.pixels[4].density = 255;
    map.pixels[4].road = 15;
    let f = foliage_instances(&map, 1, 1).unwrap();
    assert_eq!(f.len(), 15);
    assert_eq!(f, foliage_instances(&map, 1, 1).unwrap());
    for t in f {
        assert!((12..=14).contains(&t.model));
        assert!(
            t.position.x >= 1.15
                && t.position.x < 1.85
                && t.position.z >= 1.15
                && t.position.z < 1.85
        );
        close(t.scale, 1. / 75.);
    }
}

// Catches replacing the integrated fixture with a flat or empty scene.
#[test]
fn shared_synthetic_fixtures_exercise_all_geometry_paths() {
    let lot = synthetic_lot();
    assert_eq!((lot.width, lot.height, lot.levels), (6, 6, 2));
    let out = build_lot(&lot, &BuildOptions::default()).unwrap();
    for kind in [
        SurfaceKind::Terrain,
        SurfaceKind::Floor,
        SurfaceKind::Wall,
        SurfaceKind::Roof,
        SurfaceKind::Pool,
    ] {
        assert!(out.parts.iter().any(|p| p.kind == kind));
    }
    assert!(lot
        .tiles
        .iter()
        .any(|t| t.diagonal == Some(Diagonal::Vertical)));
    assert!(lot
        .tiles
        .iter()
        .any(|t| t.diagonal == Some(Diagonal::Horizontal)));
    assert!(lot.terrain.iter().any(|h| *h != lot.terrain[0]));
    let city = synthetic_city();
    let parts = build_city_parts(&city, CityBoundary::Rectangle, 10000).unwrap();
    for class in [
        TerrainClass::Grass,
        TerrainClass::Sand,
        TerrainClass::Rock,
        TerrainClass::Snow,
        TerrainClass::Water,
    ] {
        assert!(parts.iter().any(|p| p.kind == CityPartKind::Terrain(class)));
    }
}

#[test]
fn city_inverse_rejects_finite_input_when_tile_output_overflows() {
    assert!(city_center_to_lot((0, 0), Vec2::new(f32::MAX, f32::MAX)).is_err());
}

#[test]
fn detailed_patch_uses_source_tr_bl_diagonal_and_preserves_every_material_layer() {
    let mut map = CityMap { width: 3, height: 3, pixels: vec![pixel(); 9] };
    map.pixels[0].elevation = 1;
    map.pixels[1].elevation = 8;
    map.pixels[3].elevation = 24;
    map.pixels[4].elevation = 2;
    map.pixels[0].road = 0x21;
    map.pixels[1].terrain = [255, 255, 0, 255];
    let parts = build_near_patch_parts(&map, (0, 0), (1, 1), 4, CityBoundary::Rectangle, 1000).unwrap();
    assert_eq!(parts.len(), 4);
    for p in &parts {
        p.mesh.validate(&RenderLimits::default()).unwrap();
        assert_eq!((p.mesh.vertices.len(), p.mesh.indices.len()), (25, 96));
        assert_eq!(&p.mesh.indices[..6], &[0, 1, 5, 5, 1, 6]);
    }
    let ground = parts.iter().find(|p| p.kind == CityPartKind::Terrain(TerrainClass::Grass)).unwrap();
    let blend = parts.iter().find(|p| matches!(p.kind, CityPartKind::Blend { .. })).unwrap();
    assert_eq!(blend.mesh.vertices[6].uv, ground.mesh.vertices[6].uv);
    let mask = blend.mask_uv().unwrap();
    assert_eq!(mask.len(), 25);
    close(mask[0].x, 3. / 7.);
    close(mask[0].y, 1. / 3.);
    close(mask[6].x, 3.25 / 7.);
    close(mask[6].y, 1.25 / 3.);
    let edge = parts.iter().find(|p| p.kind == CityPartKind::RoadEdge(5)).unwrap();
    assert_eq!(edge.mesh.vertices[0].uv, Vec2::new(0.75, 0.));
    assert_eq!(edge.mesh.vertices[4].uv, Vec2::new(0.625, 0.));
    assert_eq!(edge.mesh.vertices[6].uv, Vec2::new(0.71875, 0.0625));
    assert!(parts.iter().any(|p| p.kind == CityPartKind::RoadCorner(2)));
    let merged = build_near_patch(&map, (0, 0), (1, 1), 4).unwrap();
    assert_eq!((merged.vertices.len(), merged.indices.len()), (100, 384));
    assert!(build_near_patch_parts(&map, (0, 0), (1, 1), 4, CityBoundary::Rectangle, 99).is_err());
}

#[test]
fn coarse_blend_keeps_terrain_uv_and_an_unmirrored_secondary_mask() {
    let mut map = CityMap { width: 2, height: 2, pixels: vec![pixel(); 4] };
    map.pixels[1].terrain = [255, 255, 0, 255];
    let parts = build_city_parts(&map, CityBoundary::Rectangle, 1000).unwrap();
    let blend = parts.iter().find(|p| p.tile == (0, 0) && matches!(p.kind, CityPartKind::Blend { .. })).unwrap();
    assert_eq!(blend.mesh.vertices[0].uv, Vec2::ZERO);
    assert_eq!(blend.mesh.vertices[1].uv, Vec2::new(0.25, 0.));
    let mask = blend.mask_uv().unwrap();
    close(mask[0].x, 3. / 7.);
    close(mask[1].x, 4. / 7.);
    close(mask[2].y, 2. / 3.);
}

#[test]
fn canonical_city_fade_clamps_samples_and_excludes_edge_material_layers() {
    let mut map = CityMap { width: 512, height: 512, pixels: vec![pixel(); 512 * 512] };
    let y = 306usize;
    // Source row starts at zero; the next row starts at one.
    map.pixels[y * 512].elevation = 12;
    map.pixels[(y + 1) * 512 + 1].elevation = 24;
    map.pixels[y * 512].road = 0x21;
    let patch = build_near_patch_parts(&map, (0, 306), (1, 1), 4, CityBoundary::RendererWithFade, 1000).unwrap();
    assert_eq!(patch.len(), 1);
    let mesh = &patch[0].mesh;
    close(mesh.vertices[0].position.y, 1.);
    close(mesh.vertices[20].position.y, 2.);
    close(mesh.vertices[20].color[3], 8. / 9.);
    assert!(in_bounds(297, 10, CityBoundary::RendererWithFade));
    assert!(!in_bounds(285, 10, CityBoundary::RendererWithFade));
}
