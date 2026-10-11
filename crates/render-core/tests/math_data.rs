use wonderland_render_core::{math::*, units, *};

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}
fn vector(a: Vec3, b: Vec3) {
    near(a.x, b.x);
    near(a.y, b.y);
    near(a.z, b.z);
}

#[test]
fn trs_uses_column_vectors_and_parent_order() {
    let q = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2).unwrap();
    let local = Mat4::from_trs(Vec3::new(2., 0., 0.), q, Vec3::new(2., 3., 4.));
    let parent = Mat4::from_translation(Vec3::new(0., 5., 0.));
    vector(
        (parent * local).transform_point3(Vec3::X),
        Vec3::new(2., 7., 0.),
    );
    vector(local.transform_vector3(Vec3::X), Vec3::new(0., 2., 0.));
}

#[test]
fn inverse_recovers_scaled_rotated_translated_point() {
    let q = Quat::from_axis_angle(Vec3::new(1., 2., 3.), 0.7).unwrap();
    let m = Mat4::from_trs(Vec3::new(3., -7., 2.), q, Vec3::new(-2., 3., 0.25));
    let p = Vec3::new(11., -4., 8.);
    vector(
        m.inverse().unwrap().transform_point3(m.transform_point3(p)),
        p,
    );
    assert!(Mat4::from_scale(Vec3::new(1., 0., 1.)).inverse().is_none());
}

#[test]
fn quaternion_slerp_takes_shortest_path_and_handles_antipodes() {
    let q = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2).unwrap();
    let negative = Quat::new(-q.x, -q.y, -q.z, -q.w);
    vector(
        Quat::IDENTITY.slerp(negative, 0.5).rotate_vec3(Vec3::X),
        Vec3::new(0.5f32.sqrt(), 0.5f32.sqrt(), 0.),
    );
    vector(q.slerp(negative, 0.5).rotate_vec3(Vec3::X), Vec3::Y);
}

#[test]
fn normalization_and_axis_angle_reject_invalid_inputs() {
    assert!(Quat::new(0., 0., 0., 0.).normalize().is_none());
    assert!(Quat::from_axis_angle(Vec3::ZERO, 1.).is_none());
    assert!(Quat::from_axis_angle(Vec3::X, f32::NAN).is_none());
    assert_eq!(
        Vec3::new(f32::INFINITY, 0., 0.).normalize_or_zero(),
        Vec3::ZERO
    );
    vector(
        Vec3::new(3., 4., 0.).normalize_or_zero(),
        Vec3::new(0.6, 0.8, 0.),
    );
}

#[test]
fn perspective_is_right_handed_zero_to_one_depth() {
    let m = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 2., 1., 11.).unwrap();
    let near_clip = m.transform_vec4([0., 0., -1., 1.]);
    let far_clip = m.transform_vec4([0., 0., -11., 1.]);
    near(near_clip[2] / near_clip[3], 0.);
    near(far_clip[2] / far_clip[3], 1.);
    let side = m.transform_vec4([2., 0., -1., 1.]);
    near(side[0] / side[3], 1.);
    assert!(Mat4::perspective_rh(1., 0., 1., 2.).is_none());
    assert!(Mat4::perspective_rh(1., 1., 2., 1.).is_none());
}

#[test]
fn orthographic_and_look_at_agree_on_right_handed_axes() {
    let view = Mat4::look_at_rh(Vec3::new(0., 0., 5.), Vec3::ZERO, Vec3::Y).unwrap();
    vector(view.transform_point3(Vec3::ZERO), Vec3::new(0., 0., -5.));
    let m = Mat4::orthographic_rh(-2., 2., -1., 1., 1., 11.).unwrap();
    vector(
        m.transform_point3(Vec3::new(2., 1., -1.)),
        Vec3::new(1., 1., 0.),
    );
    vector(
        m.transform_point3(Vec3::new(-2., -1., -11.)),
        Vec3::new(-1., -1., 1.),
    );
    assert!(Mat4::look_at_rh(Vec3::ZERO, Vec3::ZERO, Vec3::Y).is_none());
    assert!(Mat4::look_at_rh(Vec3::ZERO, Vec3::Z, Vec3::Z).is_none());
}

#[test]
fn scaled_ray_parameter_is_preserved_and_parallel_slabs_are_checked() {
    let b = Aabb::new(Vec3::ZERO, Vec3::ONE).unwrap();
    assert_eq!(
        b.ray_interval(Ray {
            origin: Vec3::new(-2., 0.5, 0.5),
            direction: Vec3::new(2., 0., 0.)
        }),
        Some((1., 1.5))
    );
    assert_eq!(
        b.ray_interval(Ray {
            origin: Vec3::new(-2., 2., 0.5),
            direction: Vec3::X
        }),
        None
    );
    assert_eq!(
        b.ray_interval(Ray {
            origin: Vec3::new(0.5, 0.5, 0.5),
            direction: Vec3::ZERO
        }),
        None
    );
    assert_eq!(
        b.ray_interval(Ray {
            origin: Vec3::new(2., 0.5, 0.5),
            direction: Vec3::X
        }),
        None
    );
}

#[test]
fn transformed_box_encloses_all_corners_with_negative_scale() {
    let b = Aabb::new(Vec3::ZERO, Vec3::ONE).unwrap();
    let out = b
        .transformed(Mat4::from_trs(
            Vec3::new(5., 0., 0.),
            Quat::IDENTITY,
            Vec3::new(-2., 3., 4.),
        ))
        .unwrap();
    assert_eq!(
        out,
        Aabb::new(Vec3::new(3., 0., 0.), Vec3::new(5., 3., 4.)).unwrap()
    );
    assert!(Aabb::new(Vec3::new(f32::NAN, 0., 0.), Vec3::ONE).is_none());
    assert!(Aabb::from_points(&[]).is_none());
}

#[test]
fn source_offsets_convert_once_with_distinct_vertical_divisor() {
    vector(
        units::tile_to_graphics(Vec3::new(2., 4., 1.)),
        Vec3::new(6., 3., 12.),
    );
    vector(
        units::dgrp_offset_to_tiles(Vec3::new(16., 32., 5.)),
        Vec3::new(1., 2., 1.),
    );
    vector(
        units::slot_offset_to_graphics(Vec3::new(16., 32., 5.)),
        Vec3::new(3., 3., 6.),
    );
    near(units::terrain_height_to_tiles(160.), 3.);
    near(units::story_height_to_tiles(2.), 5.9);
}

#[test]
fn transform_sampling_rejects_invalid_fraction_or_nonunit_rotation() {
    let mut next = Transform::IDENTITY;
    next.translation = Vec3::new(10., 0., 0.);
    vector(
        Transform::IDENTITY
            .interpolate(next, 0.25)
            .unwrap()
            .translation,
        Vec3::new(2.5, 0., 0.),
    );
    assert!(Transform::IDENTITY.interpolate(next, -0.1).is_none());
    assert!(Transform::IDENTITY.interpolate(next, f32::NAN).is_none());
    next.rotation = Quat::new(0., 0., 0., 2.);
    assert!(Transform::IDENTITY.interpolate(next, 0.5).is_none());
}

#[test]
fn mesh_validation_rejects_nonfinite_and_out_of_range_indices_and_limits() {
    let mut mesh = Mesh {
        vertices: vec![Vertex {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            uv: Vec2::ZERO,
            color: [1.; 4],
        }],
        indices: vec![0, 0, 0],
    };
    assert!(mesh.validate(&RenderLimits::default()).is_ok());
    mesh.indices[0] = 1;
    assert!(mesh.validate(&RenderLimits::default()).is_err());
    mesh.indices[0] = 0;
    mesh.vertices[0].uv.x = f32::NAN;
    assert!(mesh.validate(&RenderLimits::default()).is_err());
    mesh.vertices[0].uv.x = 0.;
    let limits = RenderLimits {
        max_vertices: 0,
        ..Default::default()
    };
    assert!(mesh.validate(&limits).is_err());
}

#[test]
fn rgba_validation_checks_length_dimensions_and_pixels_before_use() {
    let mut image = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![[0; 4]; 3],
    };
    assert!(image.validate(&RenderLimits::default()).is_err());
    image.pixels.push([255; 4]);
    assert!(image.validate(&RenderLimits::default()).is_ok());
    image.width = u32::MAX;
    assert!(image.validate(&RenderLimits::default()).is_err());
    image.width = 0;
    assert!(image.validate(&RenderLimits::default()).is_err());
}

#[test]
fn owned_contract_data_roundtrips_without_engine_handles() {
    let frame = RenderFrame {
        stamp: FrameStamp {
            lot_id: 1,
            epoch: 2,
            tick: 3,
            architecture_revision: 4,
            content: AssetKey([5; 32]),
        },
        entities: vec![],
        selected: None,
    };
    let bytes = bincode::serialize(&frame).unwrap();
    let decoded: RenderFrame = bincode::deserialize(&bytes).unwrap();
    assert_eq!(decoded, frame);
}

#[test]
fn singular_nontrivial_matrix_never_produces_a_false_inverse() {
    let m = Mat4 {
        cols: [
            [1., 2., 3., 4.],
            [2., 3., 5., 7.],
            [3., 5., 8., 11.],
            [1., 1., 1., 1.],
        ],
    };
    assert!(m.inverse().is_none());
}

#[test]
fn large_finite_vector_length_and_projection_do_not_overflow_intermediates() {
    near(Vec3::new(3e20, 4e20, 0.).length() / 1e20, 5.);
    let m = Mat4::perspective_rh(1., 1., 1e20, 2e20).unwrap();
    let p = m.transform_vec4([0., 0., -1e20, 1.]);
    near(p[2] / p[3], 0.);
}
