use wonderland_render_core::derivatives::source::*;
use wonderland_render_core::derivatives::*;
use wonderland_render_core::*;

fn world() -> SourceWorld {
    SourceWorld {
        width: 20,
        height: 20,
        stories: 5,
        altitude: vec![0; 400],
        base_alt: 0,
        tiles: vec![SourceTile::default(); 2000],
        rooms: None,
        fine_area: None,
    }
}
fn input() -> DerivativeInput {
    DerivativeInput {
        frame: RenderFrame {
            stamp: FrameStamp {
                lot_id: 5,
                epoch: 1,
                tick: 1,
                architecture_revision: 1,
                content: AssetKey([3; 32]),
            },
            entities: vec![],
            selected: None,
        },
        source_provenance: AssetKey([1; 32]),
        lighting: [LightingPass::DAY, LightingPass::NIGHT],
        materials: vec![],
        draws: vec![],
        output: DerivativeOutput::Thumbnail(ThumbnailRequest {
            width: 1,
            height: 1,
            clip_from_world: Mat4::IDENTITY,
            clear: [0; 4],
        }),
    }
}

#[test]
fn source_altitude_clamps_corners_and_preserves_base_and_fractional_interpolation() {
    let mut w = world();
    w.base_alt = 10;
    for y in 0..20 {
        for x in 0..20 {
            w.altitude[y * 20 + x] = (10 + x * 16 + y * 32) as i16;
        }
    }
    assert!((w.interp_altitude(2.5, 3.25).unwrap() - 2.7).abs() < 1e-5);
    assert!((w.interp_altitude(0., 0.).unwrap() - 0.9).abs() < 1e-5);
    assert!((w.interp_altitude(40., 40.).unwrap() - 17.1).abs() < 1e-5);
}

#[test]
fn source_thumbnail_uses_far_topleft_576_camera_and_buildable_area() {
    let w = world();
    let p = w.thumbnail_plan(SourceThumbnailMode::Tso).unwrap();
    assert_eq!([p.request.width, p.request.height], [576, 576]);
    assert_eq!(p.buildable_bounds, [6, 6, 7, 7]);
    assert_eq!(p.center_tile, Vec2::new(10., 10.));
    let c = p.request.clip_from_world.transform_vec4([30., 0., 30., 1.]);
    // CameraController2D's -2 far-zoom pixels produce +0.5 output pixels.
    assert!((c[0] * 288. - 0.5).abs() < 0.001);
    let x = p.request.clip_from_world.transform_vec4([33., 0., 30., 1.]);
    assert!(((x[0] - c[0]) * 288. - 4.).abs() < 0.001);
    assert!(((x[1] - c[1]) * 288. + 2.).abs() < 0.001);
    let ts1 = w.thumbnail_plan(SourceThumbnailMode::Ts1).unwrap();
    assert_eq!([ts1.request.width, ts1.request.height], [320, 320]);
}

#[test]
fn fine_area_changes_center_and_crops_terrain_without_floor_based_room_guesses() {
    let mut w = world();
    let mut fine = vec![false; 400];
    for y in 7..11 {
        for x in 6..10 {
            fine[y * 20 + x] = true;
        }
    }
    w.fine_area = Some(fine);
    w.altitude.fill(160);
    let p = w.thumbnail_plan(SourceThumbnailMode::Tso).unwrap();
    assert_eq!(p.buildable_bounds, [6, 7, 4, 4]);
    let expected_shift = 3.0 / 2.95 * 230.0 / 16.0 / 4.0;
    assert!((p.center_tile.x - (8.0 - expected_shift)).abs() < 0.00001);
    assert!((p.center_tile.y - (9.0 - expected_shift)).abs() < 0.00001);
    assert!(p.contains_tile(6, 7));
    assert!(!p.contains_tile(10, 7));
    let mut empty = w.clone();
    empty.fine_area.as_mut().unwrap().fill(false);
    assert!(empty.thumbnail_plan(SourceThumbnailMode::Tso).is_err());
    w.tiles[8 * 20 + 8].floor_pattern = 7;
    let topology = w.room_topology().unwrap();
    assert!(topology.rooms[(topology.map[8 * 20 + 8] & 0xffff) as usize].is_outside);
}

#[test]
fn source_room_bases_and_outside_sample_select_walls_and_fences_in_source_order() {
    let mut w = world();
    let line = [[64, 64], [128, 64]];
    let fence = [[128, 64], [128, 96]];
    let mut map = vec![0; 2000];
    map[4 * 20 + 6] = 2;
    w.rooms = Some(SourceRooms {
        rooms: vec![
            SourceRoom {
                id: 0,
                base: 0,
                floor: 0,
                is_outside: true,
                wall_lines: vec![],
                fence_lines: vec![],
            },
            SourceRoom {
                id: 1,
                base: 1,
                floor: 0,
                is_outside: true,
                wall_lines: vec![line],
                fence_lines: vec![fence],
            },
            SourceRoom {
                id: 2,
                base: 2,
                floor: 0,
                is_outside: false,
                wall_lines: vec![line],
                fence_lines: vec![],
            },
            SourceRoom {
                id: 3,
                base: 1,
                floor: 0,
                is_outside: true,
                wall_lines: vec![line],
                fence_lines: vec![],
            },
        ],
        map,
    });
    let f = w.facade_request(SourceFacadeOptions::default()).unwrap();
    assert_eq!(f.walls.len(), 2);
    assert_eq!(f.walls[0].points, line);
    assert_eq!(f.walls[1].points, fence);
    assert_eq!(f.walls[0].outside, OutsideSide::Right);
    assert_eq!([f.floor_tiles, f.floor_resolution_per_tile], [64, 2]);
}

#[test]
fn bounded_tile_topology_merges_exterior_edges_and_keeps_fences_permeable() {
    let mut w = world();
    for y in 4..6 {
        w.tiles[y * 20 + 4].walls[0] = true;
        w.tiles[y * 20 + 6].walls[0] = true;
    }
    for x in 4..6 {
        w.tiles[4 * 20 + x].walls[1] = true;
        w.tiles[6 * 20 + x].walls[1] = true;
    }
    w.tiles[2 * 20 + 2].fences[0] = true;
    let rooms = w.room_topology().unwrap();
    assert!(!rooms.rooms[(rooms.map[4 * 20 + 4] & 0xffff) as usize].is_outside);
    assert!(rooms.rooms[(rooms.map[2 * 20 + 2] & 0xffff) as usize].is_outside);
    let f = w.facade_request(SourceFacadeOptions::default()).unwrap();
    assert_eq!(f.walls.len(), 5);
    let mut bad = w.clone();
    bad.tiles.pop();
    assert!(bad.room_topology().is_err());
}

#[test]
fn source_worker_ground_subdivision_and_overlay_use_source_geometry_units() {
    let mut w = world();
    w.altitude.fill(160);
    w.tiles[3 * 400 + 7 * 20 + 7].floor_pattern = 2;
    let prepared = w
        .prepare(
            input(),
            SourceFacadeOptions {
                thumbnail: None,
                ..SourceFacadeOptions::default()
            },
            DerivativeRenderLimits::default(),
        )
        .unwrap();
    let request = prepared.into_request();
    assert_eq!(
        request.input().lighting,
        [LightingPass::DAY, LightingPass::NIGHT]
    );
    let output = request.render().unwrap();
    let geo = output.facade_geometry().unwrap();
    // Four floors + the source half-story overlay. Worker uses GROUND_SUBDIV=5.
    assert_eq!(geo.floor.vertices.len(), 36 * 5);
    assert_eq!(geo.floor.indices.len(), 150 * 5);
    assert_eq!(geo.floor.vertices[0].position, Vec3::new(-22., 3., -22.));
    assert_eq!(
        geo.floor.vertices[36].position,
        Vec3::new(-22., 4.475, -22.)
    );
    assert_eq!(geo.floor.vertices[36].uv, Vec2::new(1., 1.));
    assert_eq!(&geo.floor.indices[..6], &[7, 1, 0, 0, 6, 7]);
    let f = output.to_fsof([80, 90, 100, 255]).unwrap();
    let wire = f.encode(true, Default::default()).unwrap();
    assert_eq!(
        wonderland_render_core::derivatives::fsof::Fsof::decode(&wire, Default::default()).unwrap(),
        f
    );
}

#[test]
fn source_room_input_changes_identity_even_if_declared_provenance_is_unchanged() {
    let a = world();
    let mut b = a.clone();
    b.altitude[8 * 20 + 8] = 1;
    let options = SourceFacadeOptions {
        thumbnail: None,
        ..SourceFacadeOptions::default()
    };
    let pa = a
        .prepare(input(), options.clone(), Default::default())
        .unwrap();
    let pb = b.prepare(input(), options, Default::default()).unwrap();
    assert_ne!(pa.request().key(), pb.request().key());
    assert!(pa.request().reservation_bytes() > pa.request().render().unwrap().resident_bytes());
}

#[test]
fn source_thumbnail_accepts_a_real_tile_field_and_filters_default_buildable_slice() {
    let w = world();
    let mut input = input();
    input.materials.push(DerivativeMaterial::solid(
        [0.2, 0.8, 0.3, 1.],
        [0.2, 0.8, 0.3, 1.],
    ));
    let mut placements = Vec::new();
    for y in 0..20 {
        for x in 0..20 {
            let x = x as f32 * 3.;
            let y = y as f32 * 3.;
            input.draws.push(DerivativeDraw {
                owner: None,
                source_asset: AssetKey([5; 32]),
                model: Mat4::IDENTITY,
                material: 0,
                layer: DrawLayer::Floor(0),
                mesh: Mesh {
                    vertices: [[x, y], [x + 3., y], [x + 3., y + 3.], [x, y + 3.]]
                        .into_iter()
                        .map(|[x, z]| Vertex {
                            position: Vec3::new(x, 0., z),
                            normal: Vec3::Y,
                            uv: Vec2::ZERO,
                            color: [1.; 4],
                        })
                        .collect(),
                    indices: vec![0, 1, 2, 0, 2, 3],
                },
            });
            placements.push(Some([(x / 3.) as u16, (y / 3.) as u16, 1]));
        }
    }
    let prepared = w
        .prepare_thumbnail(
            input,
            SourceThumbnailMode::Tso,
            placements,
            Default::default(),
        )
        .unwrap();
    assert!(prepared.work_units() < 5_000_000);
    let artifact = prepared.render().unwrap();
    let image = &artifact.images()[0].image;
    let count = image.pixels.iter().filter(|p| p[3] != 0).count();
    assert!(
        count > 500 && count < 1000,
        "source slice has {count} colored pixels"
    );
}

#[test]
fn absent_night_state_produces_no_night_images_or_fsof_night_payload() {
    let w = world();
    let prepared = w
        .prepare(
            input(),
            SourceFacadeOptions {
                thumbnail: None,
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap()
        .into_request()
        .day_only();
    let artifact = prepared.render().unwrap();
    assert_eq!(artifact.images().len(), 2);
    assert!(artifact
        .images()
        .iter()
        .all(|i| matches!(i.role, ImageRole::FloorDay | ImageRole::WallDay)));
    assert!(artifact.to_fsof([0; 4]).unwrap().night.is_none());
}
