use wonderland_render_core::{AssetKey, Vec2, Vec3};
use wonderland_render_iso::*;

fn room_input() -> RoomMapInput {
    RoomMapInput {
        source: AssetKey([9; 32]),
        lot_id: 7,
        epoch: 3,
        revision: 1,
        width: 3,
        height: 3,
        stories: 1,
        cells: vec![
            RoomMapCell {
                first: 1,
                second: 1,
                diagonal: RoomDiagonal::None,
                floor_pattern: 0
            };
            9
        ],
        rooms: vec![
            LightingRoom {
                id: 1,
                floor: 0,
                outside: true,
                outside_light: 100,
                ambient_light: 0,
            },
            LightingRoom {
                id: 2,
                floor: 0,
                outside: false,
                outside_light: 25,
                ambient_light: 20,
            },
        ],
        minimum: [30, 40, 50, 20],
        outside: [100, 200, 255, 255],
    }
}

// Catches dropped second IDs/orientation, or treating the diagonal boundary as an arbitrary side.
#[test]
fn packed_room_upload_and_diagonal_pixel_masks_follow_the_source() {
    let mut input = room_input();
    input.cells[0] = RoomMapCell {
        first: 1,
        second: 2,
        diagonal: RoomDiagonal::Horizontal,
        floor_pattern: 0,
    };
    input.cells[1] = RoomMapCell {
        first: 1,
        second: 2,
        diagonal: RoomDiagonal::Vertical,
        floor_pattern: 0,
    };
    let result = prepare_room_maps(&input, LightingBudget::default()).unwrap();
    assert_eq!(result.maps()[0].pixels[0], [1, 0, 2, 128]);
    assert_eq!(result.maps()[0].pixels[1], [1, 0, 2, 0]);
    assert_eq!(result.room_at(0, Vec2::new(0.25, 0.25)).unwrap(), 1);
    assert_eq!(result.room_at(0, Vec2::new(0.5, 0.5)).unwrap(), 2);
    assert_eq!(result.room_at(0, Vec2::new(1.75, 0.25)).unwrap(), 1);
    assert_eq!(result.room_at(0, Vec2::new(1.5, 0.5)).unwrap(), 2);
    assert_eq!(result.basic_colors().pixels[2], [81, 101, 114, 255]);
    assert_eq!(result.basic_colors().pixels[65535], [255; 4]);
    assert_eq!(result.indoors()[0], 127);
    assert_eq!(result.indoors()[1], 127);
    assert_eq!(result.indoors()[2], 0);
}

// Catches GPU cache aliasing when bytes change without a caller revision bump.
#[test]
fn room_resources_hash_effective_bytes_and_own_their_uploads() {
    let mut input = room_input();
    let first = prepare_room_maps(&input, LightingBudget::default()).unwrap();
    let again = prepare_room_maps(&input, LightingBudget::default()).unwrap();
    assert_eq!(first.key(), again.key());
    input.cells[0].floor_pattern = 1;
    let changed = prepare_room_maps(&input, LightingBudget::default()).unwrap();
    assert_ne!(first.key(), changed.key());
    input.rooms[0].outside_light = 1;
    assert_eq!(first.basic_colors().pixels[1], [100, 200, 255, 255]);
    assert_ne!(
        changed.key(),
        prepare_room_maps(&input, LightingBudget::default())
            .unwrap()
            .key()
    );
}

// Catches diagonal-bit aliases, nonexistent/cross-floor rooms, and pre-allocation overflow.
#[test]
fn room_map_admission_rejects_invalid_ids_sizes_and_total_residency() {
    let mut input = room_input();
    input.cells[0].second = 32768;
    assert!(prepare_room_maps(&input, LightingBudget::default()).is_err());
    input = room_input();
    input.rooms[1].floor = 1;
    assert!(prepare_room_maps(&input, LightingBudget::default()).is_err());
    input = room_input();
    input.width = u32::MAX;
    assert!(prepare_room_maps(&input, LightingBudget::default()).is_err());
    let mut budget = LightingBudget::default();
    budget.max_total_bytes = 10;
    assert!(prepare_room_maps(&room_input(), budget).is_err());
    input = room_input();
    input.stories = 6;
    assert!(prepare_room_maps(&input, LightingBudget::default()).is_err());
}

fn scene_point(id: u64, outdoors_color: bool, color: [u8; 3]) -> SceneLight {
    SceneLight {
        id,
        room: 2,
        floor: 0,
        light: ShadowLight {
            kind: ShadowLightKind::Point,
            position_sixteenths: Vec2::new(1., 1.),
            direction: Vec2::new(1., 0.),
            radius_sixteenths: 32.,
            falloff_multiplier: 1.,
        },
        color,
        intensity: 1.,
        outdoors_color,
        window_room: None,
        height: 2.2125,
    }
}
fn indoor_maps() -> PreparedRoomMaps {
    let mut input = room_input();
    input.minimum = [0; 4];
    input.outside = [64, 128, 192, 255];
    for cell in &mut input.cells {
        cell.first = 2;
        cell.second = 2;
    }
    prepare_room_maps(&input, LightingBudget::default()).unwrap()
}

// Catches final-only saturation, the wrong outdoor/indoor draw order, and omitting real output pixels.
#[test]
fn production_atlas_saturates_each_pass_then_multiplies_outside_before_indoor_points() {
    let maps = indoor_maps();
    let lights = [
        scene_point(1, false, [255, 0, 0]),
        scene_point(2, true, [255; 3]),
        scene_point(3, true, [255; 3]),
    ];
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights: &lights,
        outdoors: None,
        ultra: false,
        software_depth: false,
        directional: true,
    };
    let scene = prepare_lighting_scene(&input, LightingBudget::default()).unwrap();
    assert_eq!(scene.color_atlas().pixels[0], [243, 128, 192, 188]);
    assert_eq!(scene.color_atlas().width, 48);
    assert_eq!(scene.color_atlas().height, 32);
    let order: Vec<_> = scene
        .passes()
        .iter()
        .filter(|p| p.room == 2)
        .map(|p| p.kind)
        .collect();
    assert_eq!(
        order,
        [
            LightPassKind::Clear,
            LightPassKind::Point(2),
            LightPassKind::Point(3),
            LightPassKind::MultiplyOutside,
            LightPassKind::Point(1)
        ]
    );
    assert!(scene
        .direction_atlas()
        .unwrap()
        .pixels
        .iter()
        .flatten()
        .all(|v| v.is_finite()));
    assert!(scene
        .direction_atlas()
        .unwrap()
        .pixels
        .iter()
        .any(|v| v[3] > 0.));
}

// Catches caller-supplied shadow constants replacing actual source wall and object passes.
#[test]
fn production_atlas_builds_and_executes_shadow_targets_with_room_masks() {
    let maps = indoor_maps();
    let light = SceneLight {
        light: ShadowLight {
            position_sixteenths: Vec2::new(1., 16.),
            ..scene_point(1, false, [255; 3]).light
        },
        ..scene_point(1, false, [255; 3])
    };
    let geometry = [RoomShadowGeometry {
        room: 2,
        floor: 0,
        walls: vec![[Vec2::new(16., 0.), Vec2::new(16., 32.)]],
        objects: vec![],
    }];
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &geometry,
        lights: &[light],
        outdoors: None,
        ultra: false,
        software_depth: false,
        directional: false,
    };
    let scene = prepare_lighting_scene(&input, LightingBudget::default()).unwrap();
    assert!(scene.color_atlas().pixels[8 * 48 + 3][0] > 50);
    assert_eq!(scene.color_atlas().pixels[8 * 48 + 12], [0; 4]);
    assert!(scene.passes().iter().any(|p| p
        .wall_shadow
        .as_ref()
        .map_or(false, |m| !m.indices().is_empty())));
    let mut changed = geometry.clone();
    changed[0].walls.clear();
    let second = prepare_lighting_scene(
        &LightingSceneInput {
            geometry: &changed,
            ..input
        },
        LightingBudget::default(),
    )
    .unwrap();
    assert!(second.color_atlas().pixels[8 * 48 + 12][0] > 0);
    assert_ne!(scene.key(), second.key());
}

// Catches room/window aliasing and a budget applied independently per light instead of to the scene.
#[test]
fn production_scene_rejects_missing_window_rooms_duplicates_and_combined_work() {
    let maps = indoor_maps();
    let light = scene_point(1, false, [255; 3]);
    let lights = vec![light, light];
    let make = |lights| LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights,
        outdoors: None,
        ultra: false,
        software_depth: false,
        directional: false,
    };
    assert!(prepare_lighting_scene(&make(&lights), LightingBudget::default()).is_err());
    let invalid_window = [SceneLight {
        window_room: Some(500),
        ..light
    }];
    assert!(prepare_lighting_scene(&make(&invalid_window), LightingBudget::default()).is_err());
    let valid = [light];
    let mut budget = LightingBudget::default();
    budget.max_raster_samples = 4;
    assert!(prepare_lighting_scene(&make(&valid), budget).is_err());
    let mut sun = DirectionalLighting {
        light: ShadowLight {
            kind: ShadowLightKind::Directional,
            ..light.light
        },
        shadow_multiplier: 1.,
        sun_direction: Vec3::new(0., -1., 0.),
    };
    sun.shadow_multiplier = f32::NAN;
    assert!(prepare_lighting_scene(
        &LightingSceneInput {
            outdoors: Some(sun),
            ..make(&[])
        },
        LightingBudget::default()
    )
    .is_err());
}

// Catches silently substituting footprint ellipses for the source ultra mesh pass.
#[test]
fn ultra_atlas_executes_projected_object_geometry_and_floor_shadow_blur() {
    let maps = indoor_maps();
    let lights = [scene_point(1, false, [255; 3])];
    let geometry = [RoomShadowGeometry {
        room: 2,
        floor: 0,
        walls: vec![],
        objects: vec![Rect {
            x: 8.,
            y: 8.,
            width: 8.,
            height: 8.,
        }],
    }];
    let meshes = [RoomMeshShadows {
        room: 2,
        floor: 0,
        meshes: vec![MeshShadowInput {
            source: AssetKey([4; 32]),
            vertices: vec![
                Vec3::new(1.5, 3., 1.5),
                Vec3::new(3., 3., 1.5),
                Vec3::new(1.5, 3., 3.),
            ],
            indices: vec![0, 1, 2],
        }],
    }];
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &geometry,
        lights: &lights,
        outdoors: None,
        ultra: true,
        software_depth: false,
        directional: false,
    };
    assert!(prepare_lighting_scene(&input, LightingBudget::default()).is_err());
    let scene =
        prepare_lighting_scene_with_meshes(&input, &meshes, LightingBudget::default()).unwrap();
    assert_eq!(scene.layout().pixels_per_tile, 16);
    assert!(scene.color_atlas().pixels.iter().any(|p| p[3] < p[0]));
    assert!(scene
        .passes()
        .iter()
        .any(|p| !p.projected_object_shadows.is_empty()));
}

// Catches canonical-key capacity growth and missing simultaneous projected
// object targets exceeding an otherwise admitted peak-residency budget.
#[test]
fn production_peak_budget_counts_key_headers() {
    let input = room_input();
    let mut budget = LightingBudget::default();
    budget.max_total_bytes = 262_144 + 9 + 9 * 12 + 2 * 128;
    assert!(prepare_room_maps(&input, budget).is_err());
}

#[test]
fn production_peak_budget_counts_projected_scratch() {
    let maps = indoor_maps();
    let lights = [scene_point(1, false, [255; 3])];
    let meshes = [RoomMeshShadows {
        room: 2,
        floor: 0,
        meshes: vec![MeshShadowInput {
            source: AssetKey([4; 32]),
            vertices: vec![
                Vec3::new(1.5, 3., 1.5),
                Vec3::new(3., 3., 1.5),
                Vec3::new(1.5, 3., 3.),
            ],
            indices: vec![0, 1, 2],
        }],
    }];
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights: &lights,
        outdoors: None,
        ultra: true,
        software_depth: false,
        directional: false,
    };
    let mut budget = LightingBudget::default();
    // Enough for the atlas, old wall/object targets and all retained pass
    // geometry, but not the third target used during mesh MAX compositing.
    budget.max_total_bytes =
        maps.resident_bytes() + (48 * 32 * 4) * 4 + (64 * 64 * 4) + (32 * 32 * 4) + 10_000;
    assert!(prepare_lighting_scene_with_meshes(&input, &meshes, budget).is_err());
}

#[test]
fn production_bounds_scan_is_charged_even_when_all_rooms_are_empty() {
    let mut source = room_input();
    for cell in &mut source.cells {
        cell.first = 0;
        cell.second = 0;
    }
    let maps = prepare_room_maps(&source, LightingBudget::default()).unwrap();
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights: &[],
        outdoors: None,
        ultra: false,
        software_depth: false,
        directional: false,
    };
    let mut budget = LightingBudget::default();
    budget.max_raster_samples = 1;
    assert!(prepare_lighting_scene(&input, budget).is_err());
}

#[test]
fn production_mesh_descriptor_budget_counts_empty_records_and_skips_empty_draws() {
    let maps = indoor_maps();
    let lights = [scene_point(1, false, [255; 3])];
    let input = LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights: &lights,
        outdoors: None,
        ultra: true,
        software_depth: false,
        directional: false,
    };
    let empty = MeshShadowInput {
        source: AssetKey([1; 32]),
        vertices: vec![],
        indices: vec![],
    };
    let meshes = [RoomMeshShadows {
        room: 2,
        floor: 0,
        meshes: vec![empty.clone(), empty],
    }];
    let mut budget = LightingBudget::default();
    budget.max_occluders = 1;
    assert!(prepare_lighting_scene_with_meshes(&input, &meshes, budget).is_err());
    budget.max_occluders = 2;
    let with_empty = prepare_lighting_scene_with_meshes(&input, &meshes, budget).unwrap();
    assert!(with_empty
        .passes()
        .iter()
        .all(|p| p.projected_object_shadows.is_empty()));
    let without = prepare_lighting_scene_with_meshes(&input, &[], budget).unwrap();
    assert_eq!(with_empty.raster_samples(), without.raster_samples());
}
