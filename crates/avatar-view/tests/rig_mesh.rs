use wonderland_avatar_view::*;
use wonderland_render_core::{math::*, AssetKey};
fn close(a: Vec3, b: Vec3) {
    assert!((a - b).length() < 0.00001, "{a:?} != {b:?}");
}
#[test]
fn source_row_matrix_conversion_retains_palette_order() {
    let mut source = fixtures::skeleton();
    source.bones[0].translation = bits3(Vec3::new(1.0, 0.0, 0.0));
    source.bones[0].rotation =
        bits4(Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2).unwrap());
    source.bones[1].translation = bits3(Vec3::new(0.0, 2.0, 0.0));
    let rig = Rig::new(source, AssetKey([1; 32]), AvatarLimits::default()).unwrap();
    let pose = rig.bind_pose();
    close(
        pose.palette[0].transform_point3(Vec3::ZERO),
        Vec3::new(1.0, 0.0, 0.0),
    );
    close(
        pose.palette[1].transform_point3(Vec3::ZERO),
        Vec3::new(-1.0, 0.0, 0.0),
    );
}
#[test]
fn hierarchy_and_source_data_faults_fail_before_binding() {
    for kind in 0..6 {
        let mut source = fixtures::skeleton();
        match kind {
            0 => {
                source.bones[1].parent = Some(1);
                source.bones[1].parent_name = "HEAD".into();
            }
            1 => source.bones[1].parent_name = "absent".into(),
            2 => source.bones[0].name = "BASE".into(),
            3 => source.coordinate_policy = CoordinatePolicy::Source,
            4 => source.bones[1].translation[0] = F32Bits(f32::NAN.to_bits()),
            _ => source.bones[1].rotation = bits4(Quat::new(0.0, 0.0, 0.0, 2.0)),
        }
        assert!(
            Rig::new(source, AssetKey([0; 32]), AvatarLimits::default()).is_err(),
            "kind {kind}"
        );
    }
}
#[test]
fn two_local_positions_and_primary_normal_are_source_correct() {
    let mut source = fixtures::skeleton();
    source.bones[0].translation = bits3(Vec3::X);
    source.bones[1].translation = bits3(Vec3::new(-1.0, 1.0, 0.0));
    let rig = Rig::new(source, AssetKey([1; 32]), AvatarLimits::default()).unwrap();
    let mut mesh = fixtures::source_mesh();
    mesh.vertices[0].position = bits3(Vec3::X);
    mesh.vertices[0].normal = bits3(Vec3::ZERO);
    mesh.blend_vertices[0].position = bits3(Vec3::new(0.0, 2.0, 0.0));
    mesh.blend_vertices[0].raw_weight = 8192;
    let prepared =
        PreparedMesh::prepare(&rig, &mesh, AssetKey([2; 32]), AvatarLimits::default()).unwrap();
    let output = prepared.skin(&rig.bind_pose(), Mat4::IDENTITY).unwrap();
    close(output.vertices[0].position, Vec3::new(1.5, 0.75, 0.0));
    close(output.vertices[0].normal, Vec3::Y);
    assert_eq!(output.indices, vec![0, 1, 2]);
    assert_eq!(prepared.vertices[0].secondary_normal, Vec3::ZERO);
    assert_eq!(prepared.vertices[0].secondary_joint, 1);
}
#[test]
fn last_blend_record_wins_and_weights_are_not_clamped() {
    let rig = fixtures::synthetic_rig();
    let mut mesh = fixtures::source_mesh();
    mesh.blend_vertices.push(BlendVertex {
        raw_weight: 40000,
        other_vertex: 0,
        position: bits3(Vec3::Z),
        normal: bits3(Vec3::X),
    });
    mesh.bindings[1].blend_vertex_count = 2;
    let p = PreparedMesh::prepare(&rig, &mesh, AssetKey([3; 32]), AvatarLimits::default()).unwrap();
    assert_eq!(p.vertices[0].weight, 1.220703125);
    assert_eq!(p.vertices[0].secondary_position, Vec3::Z);
    mesh.blend_vertices[1].raw_weight = -16384;
    assert_eq!(
        PreparedMesh::prepare(&rig, &mesh, AssetKey([3; 32]), AvatarLimits::default())
            .unwrap()
            .vertices[0]
            .weight,
        -0.5
    );
}
#[test]
fn binds_are_rig_specific_and_invalid_ranges_are_rejected() {
    let rig = fixtures::synthetic_rig();
    let mesh = fixtures::source_mesh();
    let p = PreparedMesh::prepare(&rig, &mesh, AssetKey([3; 32]), AvatarLimits::default()).unwrap();
    let mut s = fixtures::skeleton();
    s.bones.swap(0, 1);
    s.root = 1;
    s.bones[0].index = 0;
    s.bones[0].parent = Some(1);
    s.bones[1].index = 1;
    s.bones[1].children = vec![0];
    let rig2 = Rig::new(s, AssetKey([4; 32]), AvatarLimits::default()).unwrap();
    let p2 =
        PreparedMesh::prepare(&rig2, &mesh, AssetKey([3; 32]), AvatarLimits::default()).unwrap();
    assert_ne!(p.cache_key, p2.cache_key);
    assert_eq!(p2.vertices[0].secondary_joint, 0);
    let mut bad = mesh;
    bad.bindings[0].first_real_vertex = -1;
    assert!(PreparedMesh::prepare(&rig, &bad, AssetKey([3; 32]), AvatarLimits::default()).is_err());
}
#[test]
fn representative_fixture_is_articulated_and_has_dual_joint_vertices() {
    let rig = fixtures::representative_rig();
    let mesh = fixtures::representative_mesh(&rig);
    assert_eq!(rig.source().bones.len(), 14);
    assert!(mesh.indices.len() / 3 > 100);
    assert!(
        mesh.vertices
            .iter()
            .filter(|v| v.weight != 0.0 && v.primary_joint != v.secondary_joint)
            .count()
            > 20
    );
    let output = mesh.skin(&rig.bind_pose(), Mat4::IDENTITY).unwrap();
    assert!(
        Aabb::from_points(
            &output
                .vertices
                .iter()
                .map(|v| v.position)
                .collect::<Vec<_>>()
        )
        .unwrap()
        .max
        .y > 2.0
    );
}
#[test]
fn conservative_envelope_contains_rotated_and_translated_samples() {
    let rig = fixtures::representative_rig();
    let mesh = fixtures::representative_mesh(&rig);
    let clip = Clip::new(
        &rig,
        fixtures::animation("swing", -3.0, 5.0),
        AssetKey([83; 32]),
        AvatarLimits::default(),
    )
    .unwrap();
    let bounds = mesh
        .conservative_animation_bounds(&rig, &[&clip], Mat4::IDENTITY)
        .unwrap();
    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut pose = rig.bind_pose();
        pose.locals[3].translation.x = -3.0 + 8.0 * t;
        pose.locals[4].rotation = Quat::from_axis_angle(Vec3::Z, t * std::f32::consts::PI).unwrap();
        pose.rebuild(&rig).unwrap();
        let output = mesh.skin(&pose, Mat4::IDENTITY).unwrap();
        assert!(output.vertices.iter().all(|v| bounds.contains(v.position)));
    }
}
#[test]
fn provider_normalized_signed_zero_is_not_converted_again() {
    let mut source = fixtures::skeleton();
    source.bones[0].translation = bits3(Vec3::new(-0.0, 0.0, 0.0));
    let rig = Rig::new(source, AssetKey([0; 32]), AvatarLimits::default()).unwrap();
    assert_eq!(rig.source().bones[0].translation[0].0, 0x80000000);
    assert_eq!(
        rig.bind_pose().locals[0].translation.x.to_bits(),
        0x80000000
    );
}
#[test]
fn oversized_palette_and_forged_prepared_indices_fail_explicitly() {
    let source = fixtures::skeleton();
    assert!(Rig::new(
        source,
        AssetKey([0; 32]),
        AvatarLimits {
            max_bones: 1,
            ..AvatarLimits::default()
        }
    )
    .is_err());
    let rig = fixtures::synthetic_rig();
    let mut mesh = fixtures::synthetic_mesh(&rig);
    mesh.indices[0] = 100;
    assert!(mesh.skin(&rig.bind_pose(), Mat4::IDENTITY).is_err());
}
#[test]
fn finite_source_whose_hierarchy_overflows_is_rejected_without_bind_pose_panic() {
    let mut source = fixtures::skeleton();
    source.bones[0].translation = bits3(Vec3::new(3.0e38, 0.0, 0.0));
    source.bones[1].translation = bits3(Vec3::new(3.0e38, 0.0, 0.0));
    assert!(Rig::new(source, AssetKey([0; 32]), AvatarLimits::default()).is_err());
}
