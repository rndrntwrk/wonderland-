//! Synthetic numeric contracts only; no original avatar operands or assets.
use std::sync::Arc;
use wonderland_avatar_view::*;
use wonderland_render_core::{math::*, AssetKey};

fn clip(rig: &Rig, a: Quat, b: Quat) -> Arc<Clip> {
    let mut source = fixtures::animation("synthetic-raw-rotation", 0.0, 0.0);
    source.rotations = vec![bits4(a), bits4(b)];
    source.motions[0].bone_name = "ROOT".into();
    source.motions[0].translation_flag = 0;
    source.motions[0].rotation_flag = 1;
    source.motions[0].first_rotation_index = 0;
    Arc::new(Clip::new(rig, source, AssetKey([84; 32]), AvatarLimits::default()).unwrap())
}
fn layer(clip: Arc<Clip>) -> TimelineLayer {
    TimelineLayer {
        clip,
        current_frame: 0.0,
        speed: 1.0,
        weight: 1.0,
        backwards: false,
        end_reached: false,
        looping: false,
    }
}
fn sample(rig: &Rig, layers: Vec<TimelineLayer>, fraction: f32) -> Pose {
    let mut pose = rig.bind_pose();
    sample_timeline(
        rig,
        &mut pose,
        &Timeline {
            layers,
            carry: None,
        },
        fraction,
    )
    .unwrap();
    pose
}
fn close(q: Quat, expected: Quat) {
    for (a, b) in [q.x, q.y, q.z, q.w]
        .into_iter()
        .zip([expected.x, expected.y, expected.z, expected.w])
    {
        assert!((a - b).abs() < 0.000002, "{q:?} != {expected:?}");
    }
}

#[test]
fn original_slerp_contract_retains_nonunit_output_and_stored_bits() {
    let rig = fixtures::synthetic_rig();
    let a = Quat::new(-0.0, 0.0, 0.0, 2.0);
    let b = Quat::new(0.0, 1.0, 0.0, 0.0);
    let clip = clip(&rig, a, b);
    let original = clip.source().clone();
    let pose = sample(&rig, vec![layer(clip.clone())], 0.5);
    close(
        pose.locals[0].rotation,
        Quat::new(
            0.0,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
            std::f32::consts::SQRT_2,
        ),
    );
    assert!(!pose.locals[0].rotation.is_unit());
    assert!(
        a.slerp(b, 0.5).is_unit(),
        "generic render-core behavior must stay unchanged"
    );
    assert_eq!(*clip.source(), original);
    assert_eq!(clip.source().rotations[0][0].0, (-0.0f32).to_bits());
    let mut retained = pose.clone();
    retained.rebuild(&rig).unwrap();
    assert_eq!(retained, pose);
}

#[test]
fn raw_negative_fraction_and_raw_matrix_match_source_contract() {
    let rig = fixtures::synthetic_rig();
    let q = Quat::new(-0.0, 0.5, 0.0, 1.0);
    let mut l = layer(clip(&rig, q, q));
    l.backwards = true;
    let pose = sample(&rig, vec![l], 0.25);
    assert_eq!(bits4(pose.locals[0].rotation), bits4(q));
    let transformed = pose.palette[0].transform_vector3(Vec3::X);
    assert_eq!(transformed, Vec3::new(0.5, 0.0, -1.0));
}

#[test]
fn source_provenance_does_not_admit_arbitrary_nonunit_pose_or_rig_edits() {
    let rig = fixtures::synthetic_rig();
    let q = Quat::new(0.0, 0.5, 0.0, 1.0);
    let mut pose = rig.bind_pose();
    pose.locals[0].rotation = q;
    assert!(pose.rebuild(&rig).is_err());
    let mut source = fixtures::skeleton();
    source.bones[0].rotation = bits4(q);
    assert!(Rig::new(source, AssetKey([0; 32]), AvatarLimits::default()).is_err());
    let mut sampled = sample(&rig, vec![layer(clip(&rig, q, q))], 0.5);
    sampled.locals[0].rotation.x = 0.01;
    assert!(sampled.rebuild(&rig).is_err());
}

#[test]
fn nonfinite_and_norm_overflow_channels_still_fail_admission() {
    let rig = fixtures::synthetic_rig();
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        let mut source = fixtures::animation("invalid", 0.0, 0.0);
        source.rotations = vec![bits4(Quat::new(x, 0.0, 0.0, 1.0))];
        assert!(Clip::new(&rig, source, AssetKey([0; 32]), AvatarLimits::default()).is_err());
    }
}

#[test]
fn source_antipodes_and_near_linear_branch_preserve_raw_magnitude() {
    let rig = fixtures::synthetic_rig();
    let a = Quat::new(0.0, 0.0, 0.0, 1.25);
    close(
        sample(
            &rig,
            vec![layer(clip(&rig, a, Quat::new(0.0, 0.0, 0.0, -1.25)))],
            0.37,
        )
        .locals[0]
            .rotation,
        a,
    );
    let b = Quat::new(0.0, 0.002, 0.0, 1.25);
    close(
        sample(&rig, vec![layer(clip(&rig, a, b))], 0.25).locals[0].rotation,
        Quat::new(0.0, 0.0005, 0.0, 1.25),
    );
}

#[test]
fn source_norm_derived_envelope_contains_fractional_layered_raw_rotations() {
    let rig = fixtures::synthetic_rig();
    let mesh = fixtures::synthetic_mesh(&rig);
    let a = clip(
        &rig,
        Quat::new(0.0, 2.0, 0.0, 0.0),
        Quat::new(2.0, 0.0, 0.0, 0.0),
    );
    let b = clip(&rig, Quat::new(0.0, 0.0, 2.0, 0.0), Quat::IDENTITY);
    let bounds = mesh
        .conservative_animation_bounds(&rig, &[&a, &b], Mat4::IDENTITY)
        .unwrap();
    for fraction in [0.0, 0.125, 0.5, 0.875, 0.999] {
        let mut second = layer(b.clone());
        second.weight = 3.0;
        let pose = sample(&rig, vec![layer(a.clone()), second], fraction);
        let output = mesh.skin(&pose, Mat4::IDENTITY).unwrap();
        assert!(output.vertices.iter().all(|v| bounds.contains(v.position)));
    }
}

#[test]
fn finite_channels_with_matrix_or_hierarchy_overflow_fail_atomically() {
    let rig = fixtures::synthetic_rig();
    for hierarchy in [false, true] {
        let q = if hierarchy {
            Quat::new(1.0e10, 0.0, 0.0, 0.0)
        } else {
            Quat::new(1.0e19, 1.0e19, 0.0, 0.0)
        };
        let base = clip(&rig, q, q);
        let clip = if hierarchy {
            let mut source = base.source().clone();
            let mut motion = source.motions[0].clone();
            motion.bone_name = "HEAD".into();
            source.motions.push(motion);
            Arc::new(Clip::new(&rig, source, AssetKey([85; 32]), AvatarLimits::default()).unwrap())
        } else {
            base
        };
        let mut pose = rig.bind_pose();
        let before = pose.clone();
        assert!(sample_timeline(
            &rig,
            &mut pose,
            &Timeline {
                layers: vec![layer(clip)],
                carry: None
            },
            0.0
        )
        .is_err());
        assert_eq!(pose, before);
    }
}
