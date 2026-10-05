use wonderland_render_core::*;
use wonderland_render_iso::*;
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.000003, "{a} != {b}");
}
#[test]
fn atlas_layout_and_nonsquare_scissor_use_both_lot_dimensions() {
    let a = LightAtlasLayout::new(76, 76, false, false, 10_000_000).unwrap();
    assert_eq!(a.wall_shadow, [600, 600]);
    assert_eq!(a.color_atlas, [1800, 1200]);
    assert_eq!(a.direction_atlas, [900, 600]);
    assert_eq!(a.floor_slot(4).unwrap(), [1, 1]);
    assert_eq!(a.scissor_origin(4).unwrap(), [600, 600]);
    assert!(a.floor_slot(6).is_err());
    let b = LightAtlasLayout::new(76, 51, false, false, 10_000_000).unwrap();
    assert_eq!(b.scissor_origin(4).unwrap(), [600, 400]);
    let u = LightAtlasLayout::new(76, 76, true, false, 10_000_000).unwrap();
    assert_eq!(u.color_atlas, [3600, 2400]);
    assert_eq!(u.object_shadow, [2400, 2400]);
    let fallback = LightAtlasLayout::new(76, 76, true, true, 10_000_000).unwrap();
    assert_eq!(fallback.pixels_per_tile, 8);
    assert!(LightAtlasLayout::new(1, 5, false, false, 100).is_err());
    assert!(LightAtlasLayout::new(76, 76, true, false, 100).is_err());
}
fn point() -> PointSample {
    PointSample {
        color: [1.; 3],
        intensity: 1.,
        distance_over_radius: 0.5,
        wall_shadow: 0.25,
        floor_shadow: 0.5,
        outdoors_color: false,
        window_ambient: None,
    }
}
#[test]
fn room_light_accumulation_scales_indoor_points_and_respects_window_cutoff() {
    let mut input = RoomLightInput {
        minimum: [0; 4],
        outside: [255; 4],
        outdoor_contribution: [0.; 4],
        points: vec![point()],
    };
    let c = evaluate_room_light(&input, 1).unwrap();
    near(c[0], 0.11425976);
    near(c[3], 0.05712988);
    input.points[0].window_ambient = Some(29);
    assert_eq!(evaluate_room_light(&input, 1).unwrap(), [0.; 4]);
    input.points[0].window_ambient = Some(30);
    near(evaluate_room_light(&input, 1).unwrap()[0], 0.022851951);
    input.points[0].intensity = f32::NAN;
    assert!(evaluate_room_light(&input, 1).is_err());
    assert!(evaluate_room_light(&input, 0).is_err());
}
#[test]
fn source_floor_shadow_factor_is_not_clamped() {
    let lit = floor_light_color([0.5, 0.5, 0.5, 0.25], [0.1; 4], [1.; 3], 0.).unwrap();
    near(lit[0], 0.2777778);
    let extrapolated = floor_light_color([0.5, 0.5, 0.5, 1.], [0.1; 4], [1.; 3], 0.).unwrap();
    near(extrapolated[0], 0.9444445);
    let high = floor_light_color([0.5, 0.5, 0.5, 0.], [0.1; 4], [1.; 3], 1.).unwrap();
    near(high[0], 0.5);
}
fn cluster(x: f32) -> LightCluster {
    LightCluster {
        position_sixteenths: Vec2::new(x, 0.),
        radius_sixteenths: 160.,
        intensity: 1.,
        color: [1.; 4],
        weight: 1.,
        outdoors_color: false,
    }
}
#[test]
fn clustering_explicitly_preserves_source_unstored_weight_quirk() {
    let c = cluster_source_lights(&[cluster(0.), cluster(16.), cluster(32.)], 3).unwrap();
    assert_eq!(c.len(), 1);
    near(c[0].position_sixteenths.x, 20.);
    near(c[0].weight, 1.);
    let mut outdoors = cluster(0.);
    outdoors.outdoors_color = true;
    assert_eq!(
        cluster_source_lights(&[cluster(0.), outdoors], 2)
            .unwrap()
            .len(),
        2
    );
}
fn occ(group: u64) -> OccluderInput {
    OccluderInput {
        reference: EntityRef {
            object_id: group as u32,
            generation: 1,
        },
        group_id: group,
        footprint_sixteenths: Rect {
            x: 16.,
            y: 16.,
            width: 32.,
            height: 16.,
        },
        room: 2,
        floor: 1,
        stationary: true,
        emits_light: false,
        main_source: true,
    }
}
#[test]
fn multitile_occlusion_keeps_provider_group_footprints_without_duplicate_subtiles() {
    let mut child = occ(1);
    child.main_source = false;
    let mut moving = occ(2);
    moving.stationary = false;
    let mut emitting = occ(3);
    emitting.emits_light = true;
    let out = prepare_occlusion(&[occ(1), child, moving, emitting], 2, 1, None, 4).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].footprint_sixteenths.width, 32.);
    assert!(prepare_occlusion(&[occ(1), occ(1)], 2, 1, None, 2).is_err());
    assert!(prepare_occlusion(
        &[occ(1)],
        2,
        1,
        Some(Rect {
            x: 100.,
            y: 100.,
            width: 10.,
            height: 10.
        }),
        1
    )
    .unwrap()
    .is_empty());
}
#[test]
fn light_queue_keeps_source_priority_and_important_room_semantics() {
    let mut q = LightQueue {
        rooms: vec![],
        max_rooms: 4,
    };
    for room in 1..=3 {
        q.invalidate(room, 1, false, true).unwrap();
    }
    assert_eq!(q.take_source_budget(1), [1]);
    assert_eq!(q.rooms.len(), 2);
    q.invalidate(3, 1, false, true).unwrap();
    q.invalidate(4, 0, true, true).unwrap();
    assert_eq!(q.take_source_budget(1), [4, 2, 3]);
    assert!(q.rooms.is_empty());
    q.invalidate(1, 2, false, true).unwrap();
    q.invalidate(2, 1, false, false).unwrap();
    assert!(q.take_source_budget(1).is_empty());
    assert!(q.rooms.is_empty());
}

#[test]
fn ultra_floor_shadow_averages_twenty_five_green_samples() {
    let mut samples = [0.; 25];
    for sample in &mut samples[8..] {
        *sample = 1.;
    }
    let indoor = ultra_floor_shadow(samples, 76, 0.5, false).unwrap();
    near(indoor.average, 0.68);
    near(indoor.spacing, 0.0013157895);
    let outdoor = ultra_floor_shadow(samples, 76, 0.5, true).unwrap();
    near(outdoor.spacing, 0.0020467836);
    assert!(ultra_floor_shadow(samples, 0, 0.5, false).is_err());
}

#[test]
fn world_atlas_coordinates_keep_floor_slot_and_grass_offset_policy() {
    let a = LightAtlasLayout::new(76, 76, false, false, 10_000_000).unwrap();
    let p = a
        .atlas_coordinates(Vec3::new(3., 0., 6.), 4, Vec2::ZERO, false)
        .unwrap();
    near(p.x, 0.3377778);
    near(p.y, 0.5133333);
    let grass = a
        .atlas_coordinates(Vec3::ZERO, 0, Vec2::new(1., 1.), true)
        .unwrap();
    assert_eq!(grass, Vec2::ZERO);
    let wall = a
        .atlas_coordinates(Vec3::ZERO, 0, Vec2::new(1., 1.), false)
        .unwrap();
    near(wall.x, -1. / 222.);
    near(wall.y, -1. / 148.);
}

#[test]
fn rgb_surround_adjustment_preserves_the_unadjusted_floor_shadow_average() {
    let intensity = [0.5, 0.5, 0.5, 0.25];
    let minimum = [0.1, 0.1, 0.1, 1.];
    let adjust = [2., 1., 0.5];
    let partial = floor_light_color(intensity, minimum, adjust, 0.).unwrap();
    for (actual, expected) in partial.into_iter().zip([0.5, 0.2777778, 0.16666667, 1.]) {
        near(actual, expected);
    }
    let above_shadow = floor_light_color(intensity, minimum, adjust, 1.).unwrap();
    for (actual, expected) in above_shadow.into_iter().zip([1., 0.5, 0.25, 1.]) {
        near(actual, expected);
    }
    for invalid in [
        [f32::NAN, 1., 1.],
        [1., -0.1, 1.],
        [1., 1., f32::INFINITY],
    ] {
        assert!(floor_light_color(intensity, minimum, invalid, 0.).is_err());
    }
}
