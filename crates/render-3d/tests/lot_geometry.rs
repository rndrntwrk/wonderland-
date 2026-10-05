use wonderland_render_3d::{lot::*, Error};
use wonderland_render_core::{Mesh, RenderLimits, Vec2, Vec3, Vertex};

fn p(x: u16, y: u16, level: u8) -> TileCoord {
    TileCoord { x, y, level }
}
fn tile(lot: &mut VisualLot, x: u16, y: u16, l: u8) -> &mut VisualTile {
    let i = lot.tile_index(p(x, y, l)).unwrap();
    &mut lot.tiles[i]
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

// Catches square-lot loops, flipped axes and applying BaseAlt or story height twice.
#[test]
fn rectangular_terrain_uses_literal_units_origin_and_source_winding() {
    let mut lot = VisualLot::flat(2, 3, 2).unwrap();
    lot.base_alt = 80;
    lot.terrain = vec![80, 96, 112, 80, 96, 112, 80, 96, 112, 80, 96, 112];
    tile(&mut lot, 1, 2, 2).floor = 7;
    let out = build_lot(&lot, &BuildOptions::default()).unwrap();
    let ground: Vec<_> = out
        .parts
        .iter()
        .filter(|p| p.kind == SurfaceKind::Terrain)
        .collect();
    assert_eq!(ground.len(), 6);
    let g = &ground[0].mesh;
    assert_eq!(g.indices, [0, 1, 2, 2, 3, 0]);
    assert_eq!(g.vertices[0].position, Vec3::new(0., 0., 0.));
    assert_eq!(g.vertices[1].position, Vec3::new(3., 0.90000004, 0.));
    assert_eq!(g.vertices[2].uv, Vec2::new(0.5, 1.));
    assert!(g
        .vertices
        .iter()
        .all(|v| v.normal.y > 0. && v.normal.x < 0.));
    let floor = out
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::Floor)
        .unwrap();
    close(floor.mesh.vertices[0].position.y, 9.75);
    close(floor.mesh.vertices[2].position.z, 9.);
    for part in out.parts {
        part.mesh.validate(&RenderLimits::default()).unwrap();
    }
}

// Catches replacing bilinear provider contact with the rendering diagonal.
#[test]
fn contact_bilinear_saddle_and_negative_altitude_are_explicit() {
    let mut lot = VisualLot::flat(1, 1, 2).unwrap();
    lot.terrain = vec![-16, 16, 16, -16];
    close(lot.contact_height(0.5, 0.5, 1).unwrap(), 0.);
    close(lot.contact_height(0., 0., 2).unwrap(), 7.95);
    assert!(lot.contact_height(f32::NAN, 0., 1).is_err());
    assert!(lot.contact_height(2., 0., 1).is_err());
}

// Catches reusing vertical side/half mapping for a horizontal diagonal.
#[test]
fn both_diagonals_preserve_a_half_and_legacy_side_membership() {
    assert_eq!(floor_half_indices(Diagonal::Vertical, 0), [0, 1, 2]);
    assert_eq!(floor_half_indices(Diagonal::Vertical, 1), [2, 3, 0]);
    assert_eq!(floor_half_indices(Diagonal::Horizontal, 0), [0, 1, 3]);
    assert_eq!(floor_half_indices(Diagonal::Horizontal, 1), [1, 2, 3]);
    assert_eq!(source_side_to_half(Diagonal::Vertical, true), 0);
    assert_eq!(source_side_to_half(Diagonal::Vertical, false), 1);
    assert_eq!(source_side_to_half(Diagonal::Horizontal, true), 1);
    assert_eq!(source_side_to_half(Diagonal::Horizontal, false), 0);
    for d in [Diagonal::Vertical, Diagonal::Horizontal] {
        let mut lot = VisualLot::flat(1, 1, 1).unwrap();
        lot.tiles[0].diagonal = Some(d);
        lot.tiles[0].half_floors = Some([17, 19]);
        let out = build_lot(&lot, &BuildOptions::default()).unwrap();
        let f: Vec<_> = out
            .parts
            .iter()
            .filter(|p| p.kind == SurfaceKind::Floor)
            .collect();
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].material, f[1].material), (17, 19));
        assert_eq!((f[0].mesh.indices.len(), f[1].mesh.indices.len()), (3, 3));
    }
}

// Catches turning upstairs air into an ordinary floor or omitting water ground.
#[test]
fn zero_floor_water_and_build_support_have_separate_presentation_paths() {
    let mut lot = VisualLot::flat(2, 1, 2).unwrap();
    tile(&mut lot, 0, 0, 1).floor = WATER;
    tile(&mut lot, 1, 0, 2).supported = true;
    let mut options = BuildOptions::default();
    options.build_mode = true;
    let out = build_lot(&lot, &options).unwrap();
    let water = out
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::Water)
        .unwrap();
    close(water.mesh.vertices[0].position.y, 0.05);
    assert_eq!(
        out.parts
            .iter()
            .filter(|p| p.kind == SurfaceKind::Floor)
            .count(),
        0
    );
    assert_eq!(
        out.parts
            .iter()
            .filter(|p| p.kind == SurfaceKind::BuildSupport && p.material == 65503)
            .count(),
        1
    );
    assert_eq!(
        out.parts
            .iter()
            .filter(|p| p.kind == SurfaceKind::Terrain)
            .count(),
        2
    );
}

// Catches cut rotation, ignored object style overrides, fence heights, and lost caps.
#[test]
fn walls_preserve_thickness_caps_style_and_cut_endpoint_height() {
    let mut lot = VisualLot::flat(4, 4, 2).unwrap();
    let w = &mut tile(&mut lot, 2, 2, 1).wall;
    w.north = true;
    w.styles[1] = 1;
    w.object_styles[1] = 42;
    w.patterns[1] = 8;
    let mut options = BuildOptions::default();
    options.cutaway = Some(Cutaway {
        level: 1,
        rotation: 0,
        tiles: vec![true; 16],
    });
    let out = build_lot(&lot, &options).unwrap();
    let faces: Vec<_> = out
        .parts
        .iter()
        .filter(|p| p.kind == SurfaceKind::Wall)
        .collect();
    assert!(faces.len() >= 4);
    assert!(faces.iter().any(|p| p.style == 42));
    let max_y = faces
        .iter()
        .flat_map(|p| p.mesh.vertices.iter())
        .map(|v| v.position.y)
        .fold(f32::NEG_INFINITY, f32::max);
    close(max_y, 1.062);
    let top = out
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::WallTop)
        .unwrap();
    assert!(top.mesh.vertices.iter().all(|v| v.normal.y > 0.9));
    close(top.mesh.vertices[0].position.y, 1.05315);
    tile(&mut lot, 2, 2, 1).wall.styles[1] = 7;
    let out = build_lot(&lot, &options).unwrap();
    let fence = out
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::Wall)
        .unwrap();
    let max_y = fence
        .mesh
        .vertices
        .iter()
        .map(|v| v.position.y)
        .fold(0., f32::max);
    close(max_y, 8.673);
    options.visible_level = 0;
    assert!(build_lot(&lot, &options).is_err());
}

// Catches cardinal-bit reorder and concave-corner omissions for all masks.
#[test]
fn pool_topology_covers_all_masks_and_interior_margin() {
    for m in 0..=255u8 {
        let (s, c) = pool_selector(m);
        let want = if m & 1 != 0 { 1 } else { 0 }
            | if m & 64 != 0 { 2 } else { 0 }
            | if m & 16 != 0 { 4 } else { 0 }
            | if m & 4 != 0 { 8 } else { 0 };
        assert_eq!(s, want);
        assert_eq!(c.contains(&0), m & 65 == 65 && m & 128 == 0);
        assert_eq!(c.contains(&1), m & 80 == 80 && m & 32 == 0);
        assert_eq!(c.contains(&2), m & 20 == 20 && m & 8 == 0);
        assert_eq!(c.contains(&3), m & 5 == 5 && m & 2 == 0);
    }
    assert_eq!(pool_selector(85), (15, vec![0, 1, 2, 3]));
    let mut lot = VisualLot::flat(4, 4, 1).unwrap();
    tile(&mut lot, 1, 1, 1).floor = POOL;
    assert_eq!(pool_neighbors(&lot, p(1, 1, 1)), 227);
    let out = build_lot(&lot, &BuildOptions::default()).unwrap();
    assert!(out
        .parts
        .iter()
        .any(|p| p.kind == SurfaceKind::Pool && p.mesh.indices.len() >= 6));
    assert!(out.missing_assets.iter().any(|s| s.contains("pool_hq")));
}

// Catches composition order, pool scaling, and accidental A corner reorder.
#[test]
fn authored_pool_projection_is_bilinear_with_reversed_winding() {
    let v = |x, y, z| Vertex {
        position: Vec3::new(x, y, z),
        normal: Vec3::Y,
        uv: Vec2::ZERO,
        color: [1.; 4],
    };
    let mesh = Mesh {
        vertices: vec![v(0., -1., 0.), v(1., -1., 0.), v(1., -1., 1.)],
        indices: vec![0, 1, 2],
    };
    let mut lot = VisualLot::flat(4, 4, 1).unwrap();
    lot.terrain.fill(16);
    tile(&mut lot, 2, 2, 1).floor = POOL;
    let mut options = BuildOptions::default();
    options.pool_assets = Some(PoolAssets {
        tiles: vec![mesh.clone(); 16],
        corners: vec![mesh; 4],
    });
    let out = build_lot(&lot, &options).unwrap();
    let pool = out
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::Pool)
        .unwrap();
    assert_eq!(&pool.mesh.indices[..3], &[0, 2, 1]);
    close(pool.mesh.vertices[0].position.y, -2.1);
    close(pool.mesh.vertices[0].position.x, 6.);
    assert!(out.missing_assets.is_empty());
}

// Catches packing roofs on tile boundaries instead of source half-tile overhangs.
#[test]
fn roof_eligibility_growth_and_hip_rise_are_source_half_tiles() {
    let mut lot = VisualLot::flat(6, 6, 1).unwrap();
    tile(&mut lot, 2, 2, 1).indoors = true;
    lot.roof = Some(RoofStyle {
        material: 3,
        pitch: 0.5,
        advanced: false,
    });
    assert!(!roofable(&lot, 16, 16, 1));
    assert!(roofable(&lot, 24, 24, 2));
    assert!(!roofable(&lot, 16, 16, 2));
    assert_eq!(
        roof_rectangles(&lot, 2, 10000).unwrap(),
        vec![RoofRect {
            x1: 24,
            y1: 24,
            x2: 56,
            y2: 56
        }]
    );
    let out = build_lot(&lot, &BuildOptions::default()).unwrap();
    let roof = out
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::Roof)
        .unwrap();
    assert_eq!(
        (roof.mesh.vertices.len(), roof.mesh.indices.len()),
        (16, 24)
    );
    close(
        roof.mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(0., f32::max),
        10.35,
    );
    assert!(roof
        .mesh
        .vertices
        .iter()
        .all(|v| v.normal.is_finite() && v.normal.y > 0.));
    assert!(roof_rectangles(&lot, 2, 1).is_err());
}

// Catches degenerate zero-pitch normals and missing current-level blockers.
#[test]
fn zero_pitch_and_stacked_room_roof_exclusions_are_valid() {
    let mut lot = VisualLot::flat(6, 6, 2).unwrap();
    tile(&mut lot, 2, 2, 1).indoors = true;
    tile(&mut lot, 2, 2, 2).floor = 9;
    lot.roof = Some(RoofStyle {
        material: 1,
        pitch: 0.,
        advanced: true,
    });
    assert!(!roofable(&lot, 32, 32, 2));
    tile(&mut lot, 2, 2, 2).floor = 0;
    let out = build_lot(&lot, &BuildOptions::default()).unwrap();
    for p in out.parts {
        p.mesh.validate(&RenderLimits::default()).unwrap();
    }
}

// Catches rebuilding only the changed tile, missing pool/normal halos and unbounded roofs.
#[test]
fn dirty_rebuild_has_normal_pool_halos_and_bounded_roof_level() {
    let lot = VisualLot::flat(8, 8, 2).unwrap();
    let mut dirty = DirtySet::default();
    dirty.terrain_vertices.insert((4, 4));
    dirty.floors.insert(p(3, 3, 1));
    let plan = plan_rebuild(&lot, &dirty, 1000).unwrap();
    assert!(plan.tiles.contains(&p(2, 3, 1)));
    assert!(plan.tiles.contains(&p(5, 4, 1)));
    assert!(plan.tiles.contains(&p(4, 4, 2)));
    assert!(!plan.tiles.contains(&p(0, 0, 1)));
    assert!(plan.roof_levels.contains(&2));
    assert!(matches!(
        plan_rebuild(&lot, &dirty, 1),
        Err(Error::BudgetExceeded(_))
    ));
    let (plan, out) = rebuild_dirty(&lot, &BuildOptions::default(), &dirty).unwrap();
    assert!(out
        .parts
        .iter()
        .all(|p| p.tile.map(|t| plan.tiles.contains(&t)).unwrap_or(true)));
    assert!(out.parts.len() < 64);
}

// Catches preserving malformed projection lengths or allocating beyond declared budget.
#[test]
fn malformed_lots_budgets_and_grass_offset_are_checked() {
    assert!(VisualLot::flat(0, 4, 1).is_err());
    assert!(VisualLot::flat(256, 256, 16).is_err());
    let mut lot = VisualLot::flat(2, 3, 1).unwrap();
    lot.grass = vec![0, 128, 255, 1, 2, 3];
    assert_eq!(grass_shade(&lot, 0, 0), 1.);
    close(grass_shade(&lot, 2, 1), 128. / 255.);
    assert!(grass_eligible(&lot.tiles[0]));
    lot.tiles[0].diagonal = Some(Diagonal::Vertical);
    assert!(!grass_eligible(&lot.tiles[0]));
    let mut o = BuildOptions::default();
    o.budget.max_vertices = 1;
    assert!(build_lot(&lot, &o).is_err());
    lot.terrain.pop();
    assert!(build_lot(&lot, &BuildOptions::default()).is_err());
}

// Catches invalid high-level input overflowing one-based roof arithmetic.
#[test]
fn malformed_public_roof_query_is_total() {
    let mut lot = VisualLot::flat(2, 2, 1).unwrap();
    lot.levels = u8::MAX;
    assert!(!roofable(&lot, 16, 16, 2));
    assert_eq!(lot.tile_index(p(0, 0, 1)), None);
}

// Catches missing water atlas derivation and accidentally enabling the April Fools pool path.
#[test]
fn water_and_optional_legacy_pool_variants_keep_exact_ids() {
    let mut lot = VisualLot::flat(4, 4, 2).unwrap();
    tile(&mut lot, 1, 1, 1).floor = WATER;
    assert_eq!(
        effective_floor_pattern(&lot, p(1, 1, 1), WATER, false, false).unwrap(),
        65507
    );
    assert_eq!(
        effective_floor_pattern(&lot, p(1, 1, 1), POOL, false, false).unwrap(),
        65535
    );
    assert_eq!(
        effective_floor_pattern(&lot, p(1, 1, 1), POOL, false, true).unwrap(),
        65523
    );
    tile(&mut lot, 1, 1, 2).supported = true;
    assert_eq!(
        effective_floor_pattern(&lot, p(1, 1, 2), 0, true, false).unwrap(),
        65503
    );
    assert_eq!(
        effective_floor_pattern(&lot, p(1, 1, 1), 0, true, false).unwrap(),
        0
    );
    let out = build_lot(&lot, &BuildOptions::default()).unwrap();
    assert!(out
        .parts
        .iter()
        .any(|p| p.kind == SurfaceKind::Water && p.material == 65507));
}

// Catches changing source overlapping rectangle growth into disjoint greedy packing.
#[test]
fn l_shaped_roof_rectangles_overlap_in_source_expand_order() {
    let mut lot = VisualLot::flat(7, 7, 1).unwrap();
    for (x, y) in [(2, 2), (3, 2), (2, 3)] {
        tile(&mut lot, x, y, 1).indoors = true;
    }
    assert_eq!(
        roof_rectangles(&lot, 2, 10000).unwrap(),
        vec![
            RoofRect {
                x1: 24,
                y1: 24,
                x2: 72,
                y2: 56
            },
            RoofRect {
                x1: 24,
                y1: 24,
                x2: 56,
                y2: 72
            }
        ]
    );
}

// Catches roofs bridging a courtyard and diagonal exclusions on the receiving level.
#[test]
fn courtyard_and_diagonal_blockers_remain_open() {
    let mut lot = VisualLot::flat(8, 8, 2).unwrap();
    for y in 2..6 {
        for x in 2..6 {
            if x == 2 || x == 5 || y == 2 || y == 5 {
                tile(&mut lot, x, y, 1).indoors = true;
            }
        }
    }
    assert!(!roofable(&lot, 64, 64, 2));
    let rects = roof_rectangles(&lot, 2, 100000).unwrap();
    assert!(!rects
        .iter()
        .any(|r| r.x1 < 68 && r.x2 > 68 && r.y1 < 68 && r.y2 > 68));
    assert!(roofable(&lot, 32, 32, 2));
    tile(&mut lot, 2, 2, 2).diagonal = Some(Diagonal::Horizontal);
    assert!(!roofable(&lot, 32, 32, 2));
}

// Catches endpoint cut sampling being independent of the camera cut rotation.
#[test]
fn wall_endpoint_cuts_sample_each_source_rotation() {
    let mut lot = VisualLot::flat(5, 5, 1).unwrap();
    let w = &mut tile(&mut lot, 2, 2, 1).wall;
    w.north = true;
    w.styles[1] = 1;
    w.object_styles[1] = 42;
    w.patterns[1] = 8;
    for rotation in 0..4 {
        let mut cuts = vec![false; 25];
        cuts[6] = true;
        let mut options = BuildOptions::default();
        options.cutaway = Some(Cutaway {
            level: 1,
            rotation,
            tiles: cuts,
        });
        let output = build_lot(&lot, &options).unwrap();
        let face = output
            .parts
            .iter()
            .find(|p| p.kind == SurfaceKind::Wall && p.style == 42 && p.material == 8)
            .unwrap();
        close(
            face.mesh.vertices[2].position.y,
            if rotation == 0 { 1.062 } else { 8.85 },
        );
        close(face.mesh.vertices[3].position.y, 8.85);
    }
}

// Catches hard-coding roof texture scale and losing the source rim average color.
#[test]
fn roof_content_appearance_is_an_explicit_visual_input() {
    let mut lot = VisualLot::flat(6, 6, 1).unwrap();
    tile(&mut lot, 2, 2, 1).indoors = true;
    lot.roof = Some(RoofStyle {
        material: 1,
        pitch: 0.5,
        advanced: true,
    });
    lot.roof_texture_scale = 0.2;
    lot.roof_average_color = [0.2, 0.3, 0.4, 1.];
    let output = build_lot(&lot, &BuildOptions::default()).unwrap();
    let roof = output
        .parts
        .iter()
        .find(|p| p.kind == SurfaceKind::Roof)
        .unwrap();
    close(roof.mesh.vertices[0].uv.x, 0.6);
    close(roof.mesh.vertices[0].uv.y, -0.9);
    for part in output.parts.iter().filter(|p| {
        matches!(
            p.kind,
            SurfaceKind::RoofRim | SurfaceKind::RoofUnderside | SurfaceKind::RoofEdge
        )
    }) {
        assert!(part
            .mesh
            .vertices
            .iter()
            .all(|v| v.color == [0.2, 0.3, 0.4, 1.]));
    }
    lot.roof_texture_scale = f32::NAN;
    assert!(build_lot(&lot, &BuildOptions::default()).is_err());
}
