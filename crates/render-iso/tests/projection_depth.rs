use wonderland_render_core::*;
use wonderland_render_iso::*;
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.00005, "{a} != {b}");
}
fn point(a: Vec2, b: Vec2) {
    near(a.x, b.x);
    near(a.y, b.y);
}
fn projection(zoom: Zoom, rotation: Rotation) -> Projection {
    Projection::new(zoom, rotation, 1., Vec3::ZERO, Vec2::new(800., 600.)).unwrap()
}

#[test]
fn tile_bases_and_inverse_preserve_all_twelve_views() {
    let x = [(64., 32.), (-64., 32.), (-64., -32.), (64., -32.)];
    let y = [(-64., 32.), (-64., -32.), (64., -32.), (64., 32.)];
    for (zoom, scale) in [(Zoom::Near, 1.), (Zoom::Medium, 0.5), (Zoom::Far, 0.25)] {
        for (i, rot) in Rotation::ALL.into_iter().enumerate() {
            let p = projection(zoom, rot);
            point(
                p.project_tile(Vec3::X),
                Vec2::new(x[i].0 * scale, x[i].1 * scale),
            );
            point(
                p.project_tile(Vec3::Y),
                Vec2::new(y[i].0 * scale, y[i].1 * scale),
            );
            point(
                p.inverse_ground(p.project_tile(Vec3::new(-2.25, 3.5, 0.))),
                Vec2::new(-2.25, 3.5),
            );
            near(p.project_tile(Vec3::Z).y, -78.383675 * scale);
        }
    }
}
#[test]
fn nearest_even_truncation_and_negative_cache_floor_remain_distinct() {
    for (input, want) in [
        (-2.5, -2.),
        (-1.5, -2.),
        (-0.5, 0.),
        (0.5, 0.),
        (1.5, 2.),
        (2.5, 2.),
    ] {
        near(round_even(input), want);
    }
    point(
        cache_grid_origin(Vec2::new(-0.01, -512.01), 1.).unwrap(),
        Vec2::new(-512., -1024.),
    );
    point(
        cache_grid_origin(Vec2::new(255.9, 256.), 2.).unwrap(),
        Vec2::new(0., 256.),
    );
    assert!(cache_grid_origin(Vec2::ZERO, 0.).is_err());
}
#[test]
fn camera_and_sprite_offsets_preserve_cadge_alignment() {
    for (rot, want) in [
        (Rotation::TopLeft, (332., -16.)),
        (Rotation::TopRight, (268., -48.)),
        (Rotation::BottomRight, (332., -80.)),
        (Rotation::BottomLeft, (396., -48.)),
    ] {
        point(
            projection(Zoom::Near, rot).sprite_screen_offset(),
            Vec2::new(want.0, want.1),
        );
    }
    let p = Projection::new(
        Zoom::Near,
        Rotation::TopLeft,
        2.,
        Vec3::new(1., 0., 0.),
        Vec2::new(800., 600.),
    )
    .unwrap();
    point(
        p.framebuffer_point(Vec3::new(1., 0., 0.)),
        Vec2::new(400., 300.),
    );
    point(
        p.framebuffer_point(Vec3::new(2., 0., 0.)),
        Vec2::new(528., 364.),
    );
    point(
        projection(Zoom::Near, Rotation::TopLeft)
            .project_tile(projection(Zoom::Near, Rotation::TopLeft).camera_center()),
        Vec2::new(-0.5, 0.),
    );
}
#[test]
fn projection_rejects_unbounded_and_nonfinite_camera_inputs() {
    assert!(Projection::new(
        Zoom::Near,
        Rotation::TopLeft,
        0.,
        Vec3::ZERO,
        Vec2::new(1., 1.)
    )
    .is_err());
    assert!(Projection::new(
        Zoom::Near,
        Rotation::TopLeft,
        1.,
        Vec3::new(f32::NAN, 0., 0.),
        Vec2::new(1., 1.)
    )
    .is_err());
    assert!(Projection::new(
        Zoom::Near,
        Rotation::TopLeft,
        1.,
        Vec3::ZERO,
        Vec2::new(-1., 1.)
    )
    .is_err());
}
#[test]
fn source_camera_matrix_matches_unshifted_screen_basis() {
    for zoom in Zoom::ALL {
        for rot in Rotation::ALL {
            let p = projection(zoom, rot);
            let m = p.world_view_projection().unwrap();
            let zero = m.transform_vec4([0., 0., 0., 1.]);
            let clip = m.transform_vec4([3., 0., 0., 1.]);
            let delta = Vec2::new((clip[0] - zero[0]) * 400., -(clip[1] - zero[1]) * 300.);
            point(delta, p.project_tile(Vec3::X));
        }
    }
}
#[test]
fn depth_anchors_use_rotation_specific_offsets_and_unclamped_fraction() {
    for (rot, back, front) in [
        (Rotation::TopLeft, 0.15, 3.15),
        (Rotation::TopRight, 2.85, -0.15),
        (Rotation::BottomRight, 2.85, -0.15),
        (Rotation::BottomLeft, 0.15, 3.15),
    ] {
        let a = DepthAnchors::new(Vec3::ZERO, Vec3::ZERO, rot, Mat4::IDENTITY).unwrap();
        near(a.back.depth, back);
        near(a.front.depth, front);
        near(a.sample_byte(153), front);
        near(a.sample_byte(0), back + 2.5 * (front - back));
        near(a.sample_w(128), 1.);
    }
    assert!(DepthAnchors::new(
        Vec3::new(f32::NAN, 0., 0.),
        Vec3::ZERO,
        Rotation::TopLeft,
        Mat4::IDENTITY
    )
    .is_err());
}
#[test]
fn depth_binding_and_cache_restore_never_reuse_previous_sampler() {
    assert_eq!(DepthInput::None.sample(0, 0), None);
    assert_eq!(DepthInput::Constant(128).sample(0, 0), Some(128));
    let bytes = DepthInput::Bytes {
        key: AssetKey([1; 32]),
        width: 2,
        height: 1,
        values: vec![153, 255],
    };
    assert_eq!(bytes.sample(1, 0), Some(255));
    assert_eq!(bytes.sample(2, 0), None);
    near(
        restore_depth(0.2, Vec3::new(1., 2., 0.3), Mat4::IDENTITY).unwrap(),
        0.5,
    );
}

#[test]
fn advanced_lighting_reconstruction_preserves_clip_w_without_second_division() {
    let a = DepthAnchors::new(Vec3::ZERO, Vec3::ZERO, Rotation::TopLeft, Mat4::IDENTITY).unwrap();
    let w = a
        .reconstruct_lighting_position(Vec2::new(0.2, 0.3), 153, Mat4::IDENTITY)
        .unwrap();
    near(w[0], 0.2);
    near(w[1], 0.3);
    near(w[2], 3.15);
    near(w[3], 1.);
}

#[test]
fn cached_surface_destination_uses_framebuffer_delta_and_world_translation() {
    let s = cache_restore_placement(
        Vec2::new(-512., 0.),
        Vec2::new(-400.5, 0.5),
        Vec3::new(1., 2., 0.),
        Vec3::new(3., 5., 0.),
        [1312, 1112],
        1.,
    )
    .unwrap();
    assert_eq!(
        s.destination,
        Rect {
            x: -111.5,
            y: -0.5,
            width: 1312.,
            height: 1112.
        }
    );
    assert_eq!(s.world_translation, Vec3::new(-6., 0., -9.));
}
