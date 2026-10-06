use wonderland_render_core::{AssetKey, RgbaImage, Vec2, Vec3};
use wonderland_render_iso::*;

fn maps(outside: bool) -> PreparedRoomMaps {
    maps_width(outside, 3)
}
fn maps_width(outside: bool, width: u32) -> PreparedRoomMaps {
    prepare_room_maps(
        &RoomMapInput {
            source: AssetKey([1; 32]),
            lot_id: 1,
            epoch: 1,
            revision: 1,
            width,
            height: 3,
            stories: 2,
            cells: (0..width * 3 * 2)
                .map(|i| RoomMapCell {
                    first: if i < width * 3 { 1 } else { 2 },
                    second: if i < width * 3 { 1 } else { 2 },
                    diagonal: RoomDiagonal::None,
                    floor_pattern: 1,
                })
                .collect(),
            rooms: vec![
                LightingRoom {
                    id: 1,
                    floor: 0,
                    outside,
                    outside_light: 100,
                    ambient_light: 0,
                },
                LightingRoom {
                    id: 2,
                    floor: 1,
                    outside,
                    outside_light: 100,
                    ambient_light: 0,
                },
            ],
            minimum: [0; 4],
            outside: [255; 4],
        },
        LightingBudget::default(),
    )
    .unwrap()
}
fn sun() -> DirectionalLighting {
    DirectionalLighting {
        light: ShadowLight {
            kind: ShadowLightKind::Directional,
            position_sixteenths: Vec2::ZERO,
            direction: Vec2::new(1., 0.),
            radius_sixteenths: 1.,
            falloff_multiplier: 1.,
        },
        shadow_multiplier: 1.,
        sun_direction: Vec3::new(0., -1., 0.),
    }
}
fn source() -> WcrcGeometryInput {
    WcrcGeometryInput {
        source: AssetKey([2; 32]),
        quality_divider: 1,
        walls: vec![],
        floors: vec![],
        roofs: vec![],
        objects: vec![],
        noise: WcrcShadowTexture {
            source: AssetKey([3; 32]),
            image: RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![[128; 4]],
            },
        },
    }
}
fn plane(floor: u8) -> WcrcHorizontalMesh {
    WcrcHorizontalMesh {
        source: AssetKey([4; 32]),
        floor,
        vertices: vec![
            Vec3::new(-30., 99., -30.),
            Vec3::new(30., -99., -30.),
            Vec3::new(30., 8., 30.),
            Vec3::new(-30., 2., 30.),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}
fn wall(offset: bool, alpha: u8) -> WcrcWallGroup {
    WcrcWallGroup {
        source: AssetKey([5; 32]),
        floor: 0,
        use_offset: offset,
        mask: WcrcShadowTexture {
            source: AssetKey([6; 32]),
            image: RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![[255, 255, 255, alpha]],
            },
        },
        vertices: vec![
            WcrcWallVertex {
                position: Vec3::new(0.5, 0., 0.),
                texture: Vec2::new(0., 0.),
            },
            WcrcWallVertex {
                position: Vec3::new(0.5, 2., 0.),
                texture: Vec2::new(1., 0.),
            },
            WcrcWallVertex {
                position: Vec3::new(0.5, 2., 0.3),
                texture: Vec2::new(1., 0.3),
            },
            WcrcWallVertex {
                position: Vec3::new(0.5, 0., 0.3),
                texture: Vec2::new(0., 0.3),
            },
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

#[test]
fn wcrc_captures_source_floor_and_roof_height_attenuation() {
    let m = maps(true);
    let mut s = source();
    s.floors.push(plane(1));
    let capture =
        capture_wcrc_outside(&s, &m, sun(), false, false, 0, LightingBudget::default()).unwrap();
    assert_eq!([capture.image().width, capture.image().height], [32, 32]);
    assert_eq!(capture.image().pixels[16 * 32 + 16], [204, 0, 0, 255]);
    s.floors.clear();
    s.roofs.push(plane(1));
    let roof =
        capture_wcrc_outside(&s, &m, sun(), false, false, 0, LightingBudget::default()).unwrap();
    assert_eq!(roof.image().pixels[16 * 32 + 16], [153, 0, 0, 255]);
    s.quality_divider = 2;
    let low =
        capture_wcrc_outside(&s, &m, sun(), false, false, 0, LightingBudget::default()).unwrap();
    assert_eq!([low.image().width, low.image().height], [16, 16]);
    assert_eq!(source_outside_quality_divider(7.).unwrap(), 2);
}
#[test]
fn wcrc_removes_cutaways_applies_mask_and_preserves_ordered_source_channels() {
    let m = maps(true);
    let mut s = source();
    s.walls.push(wall(false, 255));
    let first =
        capture_wcrc_outside(&s, &m, sun(), false, false, 0, LightingBudget::default()).unwrap();
    // The cutaway top .3 is restored to ceil(.3-.001)=1. Source starts
    // MaxRed even when its initial wall group has UseOffset=false.
    assert_eq!(first.image().pixels[12 * 32 + 20], [239, 0, 0, 255]);
    s.walls = vec![wall(true, 0), wall(false, 255)];
    let after_transition =
        capture_wcrc_outside(&s, &m, sun(), false, false, 0, LightingBudget::default()).unwrap();
    assert_eq!(
        after_transition.image().pixels[12 * 32 + 20],
        [0, 239, 0, 255]
    );
    s.walls[1].mask.image.pixels[0][3] = 0;
    let masked =
        capture_wcrc_outside(&s, &m, sun(), false, false, 0, LightingBudget::default()).unwrap();
    assert_eq!(masked.image().pixels[12 * 32 + 20], [0, 0, 0, 255]);
}
#[test]
fn wcrc_executes_four_quantized_blurs_and_indoor_bleed_in_real_atlases() {
    let m = maps(false);
    let mut s = source();
    s.floors.push(plane(1));
    let sunlight =
        prepare_wcrc_sunlight(&s, &m, sun(), false, false, LightingBudget::default()).unwrap();
    // A fully covered floor is hardened from height .8 to opaque by PCF stage3.
    assert_eq!(sunlight.floors()[0].image().pixels[16 * 32 + 16][0], 255);
    let input = LightingSceneInput {
        room_maps: &m,
        geometry: &[],
        lights: &[],
        outdoors: Some(sun()),
        ultra: false,
        software_depth: false,
        directional: true,
    };
    let lit = prepare_lighting_scene_with_wcrc(&input, &[], &sunlight, LightingBudget::default())
        .unwrap();
    assert!(lit
        .passes()
        .iter()
        .any(|p| p.kind == LightPassKind::LightBleed));
    assert_eq!(lit.color_atlas().pixels[8 * 48 + 8], [0; 4]);
    assert!(lit
        .direction_atlas()
        .unwrap()
        .pixels
        .iter()
        .any(|p| p[3] < 0.));
    let outdoor = maps(true);
    let sunlight =
        prepare_wcrc_sunlight(&s, &outdoor, sun(), false, false, LightingBudget::default())
            .unwrap();
    let lit = prepare_lighting_scene_with_wcrc(
        &LightingSceneInput {
            room_maps: &outdoor,
            ..input
        },
        &[],
        &sunlight,
        LightingBudget::default(),
    )
    .unwrap();
    assert_eq!(lit.color_atlas().pixels[8 * 48 + 8], [64; 4]);
}
#[test]
fn wcrc_rejects_malformed_masks_indices_counts_work_and_stale_inputs() {
    let m = maps(true);
    let mut s = source();
    s.walls.push(wall(false, 255));
    let mut b = LightingBudget::default();
    b.max_raster_samples = 100;
    assert!(prepare_wcrc_sunlight(&s, &m, sun(), false, false, b).is_err());
    b = LightingBudget::default();
    b.max_total_bytes = 8;
    assert!(prepare_wcrc_sunlight(&s, &m, sun(), false, false, b).is_err());
    s.quality_divider = 4;
    assert!(prepare_wcrc_sunlight(&s, &m, sun(), false, false, LightingBudget::default()).is_err());
    s.quality_divider = 1;
    s.walls[0].indices[0] = 99;
    assert!(prepare_wcrc_sunlight(&s, &m, sun(), false, false, LightingBudget::default()).is_err());
    s.walls[0].indices[0] = 0;
    s.walls[0].mask.image.pixels.clear();
    assert!(prepare_wcrc_sunlight(&s, &m, sun(), false, false, LightingBudget::default()).is_err());
}

#[test]
fn wcrc_target_respects_large_lot_software_depth_without_disabling_ultra_objects() {
    let maps = maps_width(true, 65);
    let mut input = source();
    let mesh = MeshShadowInput {
        source: AssetKey([9; 32]),
        vertices: vec![
            Vec3::new(0., 3., 0.),
            Vec3::new(3., 3., 0.),
            Vec3::new(0., 3., 3.),
        ],
        indices: vec![0, 1, 2],
    };
    input.objects.push(WcrcObjectMeshes {
        target_floor: 0,
        meshes: vec![mesh],
    });
    let capture = capture_wcrc_outside(
        &input,
        &maps,
        sun(),
        true,
        true,
        0,
        LightingBudget::default(),
    )
    .unwrap();
    assert_eq!([capture.image().width, capture.image().height], [1024, 32]);
    assert!(capture.image().pixels.iter().any(|p| p[1] > 0));
    let sunlight = prepare_wcrc_sunlight(
        &source(),
        &maps,
        sun(),
        true,
        true,
        LightingBudget::default(),
    )
    .unwrap();
    let mismatched = LightingSceneInput {
        room_maps: &maps,
        geometry: &[],
        lights: &[],
        outdoors: Some(sun()),
        ultra: true,
        software_depth: false,
        directional: false,
    };
    assert!(prepare_lighting_scene_with_wcrc(
        &mismatched,
        &[],
        &sunlight,
        LightingBudget::default()
    )
    .is_err());
}

#[test]
fn wcrc_raw_and_pcf_targets_have_distinct_owned_identities_and_nonuniform_pixels() {
    let maps = maps(true);
    let mut input = source();
    input.walls.push(wall(false, 255));
    input.walls.push(wall(true, 0));
    let mut green = wall(false, 255);
    for v in &mut green.vertices {
        v.position.x += 0.5;
        v.position.y += 0.25;
    }
    input.walls.push(green);
    input.noise.image = RgbaImage {
        width: 4,
        height: 4,
        pixels: (0..16)
            .map(|i| [(i * 53 % 255) as u8, (i * 97 % 255) as u8, 0, 255])
            .collect(),
    };
    let raw = capture_wcrc_outside(
        &input,
        &maps,
        sun(),
        false,
        false,
        0,
        LightingBudget::default(),
    )
    .unwrap();
    let prepared = prepare_wcrc_sunlight(
        &input,
        &maps,
        sun(),
        false,
        false,
        LightingBudget::default(),
    )
    .unwrap();
    let filtered = &prepared.floors()[0];
    assert_ne!(raw.image().pixels, filtered.image().pixels);
    assert_ne!(raw.key(), filtered.key());
    if let Ok(path) = std::env::var("WCRC_PROBE_OUTPUT") {
        let json=format!("{{\"width\":{},\"height\":{},\"blueprint_width\":3,\"falloff\":1.0,\"noise\":{{\"width\":4,\"height\":4,\"pixels\":{:?}}},\"raw\":{:?},\"filtered\":{:?}}}",raw.image().width,raw.image().height,input.noise.image.pixels,raw.image().pixels,filtered.image().pixels);
        std::fs::write(path, json).unwrap();
    }
}

#[test]
fn standard_64_tile_five_floor_wcrc_executes_real_geometry_with_default_budgets() {
    let budget = LightingBudget::default();
    let maps = prepare_room_maps(
        &RoomMapInput {
            source: AssetKey([19; 32]),
            lot_id: 1,
            epoch: 1,
            revision: 1,
            width: 64,
            height: 64,
            stories: 5,
            cells: (0..64 * 64 * 5)
                .map(|i| RoomMapCell {
                    first: (i / (64 * 64) + 1) as u16,
                    second: (i / (64 * 64) + 1) as u16,
                    diagonal: RoomDiagonal::None,
                    floor_pattern: 1,
                })
                .collect(),
            rooms: (0..5)
                .map(|floor| LightingRoom {
                    id: u16::from(floor) + 1,
                    floor,
                    outside: true,
                    outside_light: 100,
                    ambient_light: 0,
                })
                .collect(),
            minimum: [0; 4],
            outside: [255; 4],
        },
        budget,
    )
    .unwrap();
    let mut input = source();
    input.walls.push(wall(false, 255));
    let mut floor = plane(1);
    floor.vertices = vec![
        Vec3::new(3., 0., 3.),
        Vec3::new(6., 0., 3.),
        Vec3::new(6., 0., 6.),
        Vec3::new(3., 0., 6.),
    ];
    input.floors.push(floor.clone());
    floor.floor = 0;
    for v in &mut floor.vertices {
        v.x += 30.;
        v.z += 30.;
    }
    input.roofs.push(floor);
    input.objects.push(WcrcObjectMeshes {
        target_floor: 0,
        meshes: vec![MeshShadowInput {
            source: AssetKey([20; 32]),
            vertices: vec![
                Vec3::new(12., 3., 12.),
                Vec3::new(15., 3., 12.),
                Vec3::new(12., 3., 15.),
            ],
            indices: vec![0, 1, 2],
        }],
    });
    input.noise.image = RgbaImage {
        width: 512,
        height: 512,
        pixels: (0..512 * 512)
            .map(|i| [(i * 53 % 255) as u8, (i * 97 % 255) as u8, 0, 255])
            .collect(),
    };
    for ultra in [false, true] {
        let start = std::time::Instant::now();
        let sunlight = prepare_wcrc_sunlight(&input, &maps, sun(), ultra, false, budget).unwrap();
        assert!(sunlight.floors()[0].image().pixels.iter().any(|p| p[0] > 0));
        eprintln!(
            "64x64x5 Ultra={ultra} capture work={}",
            sunlight.raster_samples()
        );
        let scene = prepare_lighting_scene_with_wcrc(
            &LightingSceneInput {
                room_maps: &maps,
                geometry: &[],
                lights: &[],
                outdoors: Some(sun()),
                ultra,
                software_depth: false,
                directional: true,
            },
            &[],
            &sunlight,
            budget,
        )
        .unwrap();
        assert!(scene.raster_samples() <= budget.max_raster_samples);
        assert!(scene
            .color_atlas()
            .pixels
            .iter()
            .any(|p| p[0] > 0 && p[0] < 255));
        eprintln!("64x64x5 Ultra={ultra}: samples={}, retained_sunlight={}, retained_atlas={}, elapsed_ms={}",scene.raster_samples(),sunlight.resident_bytes(),scene.resident_bytes(),start.elapsed().as_millis());
    }
}

#[test]
fn wcrc_capture_identity_rejects_changed_map_sun_and_ultra() {
    let a = maps(true);
    let b = maps(false);
    let sunlight = prepare_wcrc_sunlight(
        &source(),
        &a,
        sun(),
        false,
        false,
        LightingBudget::default(),
    )
    .unwrap();
    let base = LightingSceneInput {
        room_maps: &a,
        geometry: &[],
        lights: &[],
        outdoors: Some(sun()),
        ultra: false,
        software_depth: false,
        directional: false,
    };
    assert!(prepare_lighting_scene_with_wcrc(
        &LightingSceneInput {
            room_maps: &b,
            ..base
        },
        &[],
        &sunlight,
        LightingBudget::default()
    )
    .is_err());
    let altered = DirectionalLighting {
        shadow_multiplier: 0.5,
        ..sun()
    };
    assert!(prepare_lighting_scene_with_wcrc(
        &LightingSceneInput {
            outdoors: Some(altered),
            ..base
        },
        &[],
        &sunlight,
        LightingBudget::default()
    )
    .is_err());
    assert!(prepare_lighting_scene_with_wcrc(
        &LightingSceneInput {
            ultra: true,
            ..base
        },
        &[],
        &sunlight,
        LightingBudget::default()
    )
    .is_err());
}

#[test]
fn source_noise_uses_decimate_mips_with_point_lod_for_pcf() {
    let maps = maps(true);
    let mut input = source();
    input.walls.push(wall(false, 255));
    input.walls.push(wall(true, 0));
    let mut green = wall(false, 255);
    for v in &mut green.vertices {
        v.position.x += 0.5;
        v.position.y += 0.25;
    }
    input.walls.push(green);
    input.noise.image = RgbaImage {
        width: 512,
        height: 512,
        pixels: (0..512 * 512)
            .map(|i| {
                [
                    ((i % 512) / 2) as u8,
                    ((i / 512) / 2) as u8,
                    0,
                    if i % 19 == 0 { 0 } else { 255 },
                ]
            })
            .collect(),
    };
    let raw = capture_wcrc_outside(
        &input,
        &maps,
        sun(),
        false,
        false,
        0,
        LightingBudget::default(),
    )
    .unwrap();
    let prepared = prepare_wcrc_sunlight(
        &input,
        &maps,
        sun(),
        false,
        false,
        LightingBudget::default(),
    )
    .unwrap();
    let filtered = &prepared.floors()[0];
    assert!(filtered.image().pixels.iter().any(|p| p[0] > 0));
    assert!(filtered.image().pixels.iter().any(|p| p[1] > 0));
    assert_ne!(raw.image().pixels, filtered.image().pixels);
    if let Ok(path) = std::env::var("WCRC_NOISE_PROBE_OUTPUT") {
        let json=format!("{{\"width\":{},\"height\":{},\"blueprint_width\":3,\"falloff\":1.0,\"source_noise_mips\":true,\"noise\":{{\"width\":512,\"height\":512,\"pixels\":{:?}}},\"raw\":{:?},\"filtered\":{:?}}}",raw.image().width,raw.image().height,input.noise.image.pixels,raw.image().pixels,filtered.image().pixels);
        std::fs::write(path, json).unwrap();
    }
}
