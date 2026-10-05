use std::sync::Arc;
use wonderland_avatar_view::*;
use wonderland_render_core::{math::*, AssetKey, EntityRef};
fn clip(rig: &Rig, name: &str, a: f32, b: f32) -> Arc<Clip> {
    Arc::new(
        Clip::new(
            rig,
            fixtures::animation(name, a, b),
            AssetKey([20; 32]),
            AvatarLimits::default(),
        )
        .unwrap(),
    )
}
fn layer(clip: Arc<Clip>, weight: f32) -> TimelineLayer {
    TimelineLayer {
        clip,
        current_frame: 0.0,
        speed: 1.0,
        weight,
        backwards: false,
        end_reached: false,
        looping: false,
    }
}
#[test]
fn prefix_order_ended_weight_carry_and_missing_channels() {
    let rig = fixtures::synthetic_rig();
    let a = clip(&rig, "a", 0.0, 2.0);
    let b = clip(&rig, "b", 8.0, 10.0);
    let mut p = rig.bind_pose();
    let t = Timeline {
        layers: vec![layer(a.clone(), 1.0), layer(b.clone(), 3.0)],
        carry: None,
    };
    sample_timeline(&rig, &mut p, &t, 0.0).unwrap();
    assert_eq!(p.locals[1].translation.x, 6.0);
    assert_eq!(p.locals[0], rig.bind_pose().locals[0]);
    let mut ended = t.clone();
    ended.layers[0].end_reached = true;
    p.locals[1].translation = Vec3::new(4.0, 0.0, 0.0);
    sample_timeline(&rig, &mut p, &ended, 0.0).unwrap();
    assert_eq!(p.locals[1].translation.x, 7.0);
    ended.carry = Some(CarryPose {
        clip: a,
        frame: 1.0,
    });
    sample_timeline(&rig, &mut p, &ended, 0.8).unwrap();
    assert_eq!(p.locals[1].translation.x, 2.0);
}
#[test]
fn reverse_fraction_track_end_exact_names_and_zero_prefix() {
    let rig = fixtures::synthetic_rig();
    let c = clip(&rig, "reverse", 2.0, 8.0);
    let mut l = layer(c.clone(), 1.0);
    l.backwards = true;
    l.current_frame = 0.0;
    let mut p = rig.bind_pose();
    sample_timeline(
        &rig,
        &mut p,
        &Timeline {
            layers: vec![l.clone()],
            carry: None,
        },
        0.2,
    )
    .unwrap();
    assert_eq!(p.locals[1].translation.x, 2.0);
    l.current_frame = 2.0;
    l.backwards = false;
    sample_timeline(
        &rig,
        &mut p,
        &Timeline {
            layers: vec![l.clone()],
            carry: None,
        },
        0.8,
    )
    .unwrap();
    assert_eq!(p.locals[1].translation.x, 8.0);
    l.current_frame = 0.0;
    l.weight = 0.0;
    p.locals[1].translation.x = 5.0;
    sample_timeline(
        &rig,
        &mut p,
        &Timeline {
            layers: vec![l],
            carry: None,
        },
        0.4,
    )
    .unwrap();
    assert_eq!(p.locals[1].translation.x, 5.0);
    let mut source = fixtures::animation("wrong-case", 1.0, 9.0);
    source.motions[0].bone_name = "head".into();
    let wrong =
        Arc::new(Clip::new(&rig, source, AssetKey([0; 32]), AvatarLimits::default()).unwrap());
    sample_timeline(
        &rig,
        &mut p,
        &Timeline {
            layers: vec![layer(wrong, 1.0)],
            carry: None,
        },
        0.4,
    )
    .unwrap();
    assert_eq!(p.locals[1].translation.x, 5.0);
}
#[test]
fn refresh_cadence_cannot_accumulate_pose_or_mutate_timeline() {
    let rig = fixtures::synthetic_rig();
    let c = clip(&rig, "hurry", 0.0, 8.0);
    let mut l = layer(c, 1.0);
    l.speed = 2.0;
    l.looping = true;
    let t = Timeline {
        layers: vec![l],
        carry: None,
    };
    let entity = EntityRef {
        object_id: 7,
        generation: 1,
    };
    for hz in [30, 60, 120] {
        let mut player = PosePlayer::new(entity, &rig);
        assert!(player.commit(1, &rig, t.clone()).unwrap());
        let committed = player.retained().clone();
        for i in 0..hz {
            let f = (i as f32 / (hz as f32) * 0.5).min(0.999);
            let p = player.sample(&rig, f).unwrap();
            assert_eq!(p, player.sample(&rig, f).unwrap());
        }
        assert_eq!(player.retained(), &committed);
        assert!(!player.commit(1, &rig, t.clone()).unwrap());
        assert_eq!(t.layers[0].current_frame, 0.0);
        assert!(player.commit(0, &rig, t.clone()).is_err());
    }
}
#[test]
fn invalid_active_channels_fail_atomically() {
    let rig = fixtures::synthetic_rig();
    let mut s = fixtures::animation("empty", 0.0, 1.0);
    s.motions[0].frame_count = 0;
    assert!(Clip::new(&rig, s, AssetKey([0; 32]), AvatarLimits::default()).is_err());
    let mut player = PosePlayer::new(
        EntityRef {
            object_id: 1,
            generation: 1,
        },
        &rig,
    );
    let before = player.retained().clone();
    let t = Timeline {
        layers: vec![layer(clip(&rig, "bad-weight", 0.0, 1.0), f32::NAN)],
        carry: None,
    };
    assert!(player.commit(1, &rig, t).is_err());
    assert_eq!(player.retained(), &before);
}
#[test]
fn resolved_lookup_name_is_separate_from_animation_internal_name() {
    let rig = fixtures::synthetic_rig();
    let source = fixtures::animation("internal-name", 0.0, 8.0);
    let key = AssetKey([73; 32]);
    let clip = Clip::new_resolved(
        &rig,
        source,
        key,
        "Provider_Name.anim".into(),
        AvatarLimits::default(),
    )
    .unwrap();
    assert!(clip.matches_projection("pROVIDER_nAME.anim", 3, key));
    assert!(!clip.matches_projection(" Provider_Name.anim", 3, key));
    assert!(!clip.matches_projection("internal-name", 3, key));
}

#[test]
fn a_clip_bound_to_a_larger_rig_cannot_partially_change_a_pose() {
    let target = fixtures::synthetic_rig();
    let incompatible = fixtures::representative_rig();
    let foreign = clip(&incompatible, "foreign", 10.0, 20.0);
    for carry in [false, true] {
        let mut timeline = Timeline {
            layers: vec![layer(clip(&target, "valid-first", 6.0, 8.0), 1.0)],
            carry: None,
        };
        if carry {
            timeline.carry = Some(CarryPose {
                clip: foreign.clone(),
                frame: 0.0,
            });
        } else {
            timeline.layers.push(layer(foreign.clone(), 1.0));
        }
        let mut pose = target.bind_pose();
        let before = pose.clone();
        assert!(sample_timeline(&target, &mut pose, &timeline, 0.5).is_err());
        assert_eq!(pose, before);
    }
}

#[test]
fn malformed_public_pose_storage_is_rejected_without_mutation() {
    let rig = fixtures::synthetic_rig();
    let timeline = Timeline {
        layers: vec![layer(clip(&rig, "head", 6.0, 8.0), 1.0)],
        carry: None,
    };
    for kind in 0..4 {
        let mut pose = rig.bind_pose();
        match kind {
            0 => pose.locals.truncate(1),
            1 => pose.locals.push(pose.locals[0]),
            2 => pose.locals[0].rotation = Quat::new(0.0, 0.0, 0.0, 2.0),
            _ => pose.rig_key = AssetKey([0; 32]),
        }
        let before = pose.clone();
        assert!(sample_timeline(&rig, &mut pose, &timeline, 0.5).is_err());
        assert_eq!(pose, before);
    }
}
