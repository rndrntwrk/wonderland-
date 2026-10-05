use wonderland_render_3d::city::*;

// Catches lossy RGB matching and conflating unknown client colors with grass.
#[test]
fn exact_palette_includes_alpha_and_keeps_forests_semantic() {
    for (c, t) in [
        ([0, 255, 0, 255], TerrainClass::Grass),
        ([255, 255, 0, 255], TerrainClass::Sand),
        ([255, 0, 0, 255], TerrainClass::Rock),
        ([255, 255, 255, 255], TerrainClass::Snow),
        ([12, 0, 255, 255], TerrainClass::Water),
        ([0, 0, 0, 255], TerrainClass::Void),
        ([0, 254, 0, 255], TerrainClass::Void),
        ([0, 255, 0, 254], TerrainClass::Void),
    ] {
        assert_eq!(terrain_class(c), t);
    }
    assert_eq!(forest_class([0, 106, 40, 255]), ForestClass::Fir);
    assert_eq!(forest_class([0, 235, 66, 255]), ForestClass::Birch);
    assert_eq!(forest_class([255, 0, 0, 255]), ForestClass::Cactus);
    assert_eq!(forest_class([255, 252, 0, 255]), ForestClass::Palm);
    assert_eq!(forest_class([0, 0, 0, 255]), ForestClass::None);
    assert_eq!(forest_class([0, 0, 0, 0]), ForestClass::Unknown);
    assert_eq!(ForestClass::Cactus.atlas_2d(), Some(2));
    assert_eq!(ForestClass::Cactus.atlas_3d(), Some(3));
    assert_eq!(ForestClass::Palm.atlas_2d(), Some(3));
    assert_eq!(ForestClass::Palm.atlas_3d(), Some(2));
}

// Catches an off-by-one that changes visible/admissible city destinations.
#[test]
fn distinct_legacy_boundaries_preserve_inclusive_server_edge() {
    for (y, start, end) in [
        (0, 306, 307),
        (204, 102, 511),
        (205, 101, 512),
        (306, 0, 411),
        (511, 205, 206),
    ] {
        assert!(in_bounds(start, y, CityBoundary::RendererDiamond));
        assert!(in_bounds(end - 1, y, CityBoundary::RendererDiamond));
        assert!(!in_bounds(start - 1, y, CityBoundary::RendererDiamond));
        assert!(!in_bounds(end, y, CityBoundary::RendererDiamond));
        assert!(in_bounds(
            end,
            y,
            CityBoundary::ServerDiamond { padding: 0 }
        ));
    }
    assert!(in_bounds(0, 306, CityBoundary::RendererDiamond));
    assert!(!in_bounds(0, 306, CityBoundary::LegacyMapData));
    assert!(!in_bounds(
        306,
        0,
        CityBoundary::ServerDiamond { padding: 1 }
    ));
    assert!(!in_bounds(
        100,
        205,
        CityBoundary::ServerDiamond { padding: 1 }
    ));
    assert!(in_bounds(
        102,
        205,
        CityBoundary::ServerDiamond { padding: 1 }
    ));
    assert!(!in_bounds(0, -1, CityBoundary::RendererDiamond));
    assert_eq!(pack_location(306, 205), 0x013200cd);
    assert_eq!(unpack_location(0xffff0001), (65535, 1));
}

// Catches bit-order inversions and the density=255 top bucket overflow.
#[test]
fn all_road_nibbles_and_density_thresholds_use_source_atlases() {
    let edges = [
        None,
        Some(5),
        Some(12),
        Some(13),
        Some(7),
        Some(6),
        Some(15),
        Some(14),
        Some(28),
        Some(29),
        Some(20),
        Some(21),
        Some(31),
        Some(30),
        Some(23),
        Some(22),
    ];
    let corners = [
        None,
        Some(8),
        Some(2),
        Some(26),
        Some(3),
        Some(17),
        Some(16),
        Some(10),
        Some(25),
        Some(24),
        Some(9),
        Some(18),
        Some(1),
        Some(27),
        Some(11),
        Some(19),
    ];
    for byte in 0..=255u8 {
        assert_eq!(
            road_atlas(byte),
            (edges[(byte & 15) as usize], corners[(byte >> 4) as usize])
        );
    }
    assert_eq!(
        (blend_atlas(0), blend_atlas(8), blend_atlas(15)),
        (11, 1, 8)
    );
    for (d, n) in [
        (0, 0),
        (63, 0),
        (64, 1),
        (127, 1),
        (128, 4),
        (191, 4),
        (192, 7),
        (254, 7),
        (255, 15),
    ] {
        assert_eq!(tree_count(d), n);
    }
}

// Catches substituting Catmull-Rom and changing equal-distance input order.
#[test]
fn patch_boundary_is_linear_and_neighborhood_ties_keep_first() {
    assert!((cubic([0., 10., 30., 80.], 0.5, -1.) - 20.).abs() < 1e-6);
    assert!((cubic([0., 10., 30., 80.], 0.5, 0.) - 18.75).abs() < 1e-6);
    assert_eq!(
        nearest_neighborhood(0., 0., &[(7, 0., 0.), (8, 1., 1.)]),
        Some(7)
    );
    assert_eq!(
        nearest_neighborhood(0., 0., &[(8, 1., 1.), (7, 0., 0.)]),
        Some(8)
    );
    assert_eq!(nearest_neighborhood(0., 0., &[]), None);
}
