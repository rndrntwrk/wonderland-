#[path = "support/native_avatar_bank.rs"]
mod bank;
use std::sync::Arc;
use wonderland_game_runtime::sim_core::avatars::outfits::{OutfitReference, OutfitState};
use wonderland_game_runtime::{
    AvatarAnimation, AvatarTimeline, AvatarVisual, AvatarVisualFrame, EntityRef, ObjectId,
};
use wonderland_render_core::{AssetKey, Vec3};
use wonderland_web_shell::native_avatar::{NativeAvatarPoseHistory, NativeAvatarProjection};
use wonderland_world_view::*;

fn frame(tick: u64) -> AvatarVisualFrame {
    AvatarVisualFrame {
        revision: WorldRevision {
            lot_id: Some(11),
            epoch: 7,
            tick,
            architecture_revision: 1,
            content: AssetKey([42; 32]),
        },
        avatars: vec![AvatarVisual {
            entity: EntityRef {
                object_id: ObjectId(3),
                generation: 4,
            },
            guid: 0x7FD96B54,
            visual_revision: 2,
            container: None,
            scale_percent: 100,
            display_flags: 0,
            ghost: false,
            outfits: OutfitState {
                head: Some(OutfitReference::Id(bank::HEAD)),
                body: Some(OutfitReference::Id(bank::BODY)),
                ..Default::default()
            },
            animations: AvatarTimeline::default(),
        }],
    }
}
fn animation(name: &str, value: f32) -> AvatarAnimation {
    AvatarAnimation {
        resource: name.into(),
        num_frames: 2,
        current_frame: value,
        speed: 1.,
        weight: 1.,
        backwards: false,
        end_reached: false,
        looping: false,
    }
}
fn world(frame: &AvatarVisualFrame) -> WorldDocument {
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:retention",
        "synthetic original-format fixture",
    )
    .unwrap();
    world.provenance.kind = WorldSourceKind::LiveSession;
    world.source_counts = None;
    world.revision = frame.revision;
    world.objects = frame
        .avatars
        .iter()
        .map(|a| WorldObject {
            source_guid: a.guid,
            blueprint: None,
            snapshot: None,
            entity: Some(wonderland_render_core::EntityRef {
                object_id: a.entity.object_id.0 as u32,
                generation: a.entity.generation,
            }),
            visual_revision: a.visual_revision,
            position_tiles: Vec3::new(3.5, 3.5, 0.),
            yaw_radians: 0.,
            dynamic_flags: [0; 2],
            room: 0,
            level: 1,
            visible: true,
            selectable: true,
            model: None,
        })
        .collect();
    world
}
fn project(
    history: &NativeAvatarPoseHistory,
    frame: &AvatarVisualFrame,
    bank: &Arc<wonderland_avatar_content::ImportedContent>,
) -> WorldDocument {
    let mut world = world(frame);
    NativeAvatarProjection::prepare_retained(frame, &world, bank, history)
        .unwrap()
        .apply(&mut world, bank, &bank::pixels(bank))
        .unwrap();
    world
}
#[test]
fn ended_and_removed_timelines_keep_the_last_accepted_bone_transform() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut start = frame(9);
    start.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&start, Arc::clone(&bank)).unwrap();
    let posed = project(&history, &start, &bank);
    let mut ended = start.clone();
    ended.revision.tick = 10;
    ended.avatars[0].animations.layers[0].end_reached = true;
    history.observe(&ended, Arc::clone(&bank)).unwrap();
    let current = project(&history, &ended, &bank);
    assert_eq!(
        current.models[0].groups, posed.models[0].groups,
        "ending animation must not snap its untouched channels to the bind pose"
    );
    ended.revision.tick = 11;
    ended.avatars[0].animations.layers.clear();
    history.observe(&ended, Arc::clone(&bank)).unwrap();
    assert_eq!(
        project(&history, &ended, &bank).models[0].groups,
        posed.models[0].groups
    );
}
#[test]
fn repeated_draws_and_same_tick_notifications_do_not_compound_blending() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    let mut ended = animation("base.anim", 0.);
    ended.end_reached = true;
    sample.avatars[0].animations.layers = vec![ended, animation("blend.anim", 0.)];
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let first = project(&history, &sample, &bank);
    for _ in 0..120 {
        history.observe(&sample, Arc::clone(&bank)).unwrap();
        assert_eq!(project(&history, &sample, &bank), first);
    }
    sample.revision.tick = 10;
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_ne!(
        project(&history, &sample, &bank).models[0].groups,
        first.models[0].groups,
        "the half-weight retained blend must advance once on the next accepted tick"
    );
}
#[test]
fn explicit_checkpoint_reset_does_not_invent_precheckpoint_visual_history() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    sample.revision.tick = 10;
    sample.avatars[0].animations.layers.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let retained = project(&history, &sample, &bank);
    history.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let reset = project(&history, &sample, &bank);
    assert_ne!(retained.models[0].groups, reset.models[0].groups);
}

#[test]
fn missing_ticks_restart_unknown_history_instead_of_compounding_a_guessed_prefix() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let old = project(&history, &sample, &bank);
    sample.revision.tick = 11;
    sample.avatars[0].animations.layers.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_ne!(
        project(&history, &sample, &bank).models[0].groups,
        old.models[0].groups
    );
}
#[test]
fn unchanged_rig_hash_in_a_replaced_content_bank_does_not_keep_old_pose_history() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    sample.revision.tick = 10;
    sample.avatars[0].animations.layers.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let old = project(&history, &sample, &bank);
    let replacement = Arc::new(bank::bank());
    assert_eq!(
        bank.rig.as_ref().unwrap().key,
        replacement.rig.as_ref().unwrap().key
    );
    history.observe(&sample, Arc::clone(&replacement)).unwrap();
    assert_ne!(
        project(&history, &sample, &replacement).models[0].groups,
        old.models[0].groups
    );
    assert!(
        NativeAvatarProjection::prepare_retained(&sample, &world(&sample), &bank, &history)
            .is_err()
    );
}
#[test]
fn stale_conflicting_or_duplicate_identity_frames_leave_the_committed_pose_untouched() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let expected = project(&history, &sample, &bank);
    {
        let mut bad = sample.clone();
        bad.avatars[0].animations.layers[0].current_frame = 0.;
        assert!(history.observe(&bad, Arc::clone(&bank)).is_err());
        bad = sample.clone();
        bad.revision.tick = 8;
        assert!(history.observe(&bad, Arc::clone(&bank)).is_err());
        bad = sample.clone();
        bad.revision.tick = 10;
        bad.avatars.push(bad.avatars[0].clone());
        assert!(history.observe(&bad, Arc::clone(&bank)).is_err());
        assert_eq!(project(&history, &sample, &bank), expected);
    }
}
#[test]
fn source_scope_and_entity_generation_changes_cannot_inherit_someone_elses_pose() {
    for boundary in 0..4 {
        let bank = Arc::new(bank::bank());
        let mut history = NativeAvatarPoseHistory::default();
        let mut sample = frame(9);
        sample.avatars[0]
            .animations
            .layers
            .push(animation("base.anim", 1.));
        history.observe(&sample, Arc::clone(&bank)).unwrap();
        let old = project(&history, &sample, &bank);
        sample.avatars[0].animations.layers.clear();
        sample.revision.tick = 10;
        match boundary {
            0 => sample.revision.lot_id = Some(99),
            1 => sample.revision.epoch += 1,
            2 => sample.revision.content = AssetKey([99; 32]),
            3 => sample.avatars[0].entity.generation += 1,
            _ => unreachable!(),
        };
        history.observe(&sample, Arc::clone(&bank)).unwrap();
        assert_ne!(
            project(&history, &sample, &bank).models[0].groups,
            old.models[0].groups
        );
    }
}
#[test]
fn hidden_avatars_still_receive_pose_updates_and_removal_drops_the_old_entity() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let expected = project(&history, &sample, &bank);
    let mut hidden = world(&sample);
    hidden.objects[0].visible = false;
    hidden.objects[0].selectable = false;
    NativeAvatarProjection::prepare_retained(&sample, &hidden, &bank, &history)
        .unwrap()
        .apply(&mut hidden, &bank, &bank::pixels(&bank))
        .unwrap();
    assert!(hidden.models.is_empty());
    sample.revision.tick = 10;
    sample.avatars[0].animations.layers.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_eq!(
        project(&history, &sample, &bank).models[0].groups,
        expected.models[0].groups
    );
    let mut empty = sample.clone();
    empty.revision.tick = 11;
    empty.avatars.clear();
    history.observe(&empty, Arc::clone(&bank)).unwrap();
    sample.revision.tick = 12;
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_ne!(
        project(&history, &sample, &bank).models[0].groups,
        expected.models[0].groups
    );
}
#[test]
fn failed_animation_resolution_does_not_poison_other_avatars_or_keep_stale_meshes() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars.push(sample.avatars[0].clone());
    sample.avatars[1].entity.object_id = ObjectId(4);
    for avatar in &mut sample.avatars {
        avatar.animations.layers.push(animation("base.anim", 1.));
    }
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let expected = project(&history, &sample, &bank);
    sample.revision.tick = 10;
    sample.avatars[0].animations.layers[0].resource = "missing.anim".into();
    sample.avatars[1].animations.layers.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let current = project(&history, &sample, &bank);
    assert!(current.objects[0].model.is_none());
    assert_eq!(
        current.models[current.objects[1].model.unwrap()].groups,
        expected.models[1].groups
    );
    sample.revision.tick = 11;
    sample.avatars[0].animations.layers.clear();
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_ne!(
        project(&history, &sample, &bank).models[0].groups,
        expected.models[0].groups
    );
}
#[test]
fn coalesced_draw_cadence_has_identical_bone_history_and_repeatedly_releases_the_bank() {
    let bank = Arc::new(bank::bank());
    let mut a = NativeAvatarPoseHistory::default();
    let mut b = NativeAvatarPoseHistory::default();
    let mut sample = frame(1);
    let mut ended = animation("base.anim", 0.);
    ended.end_reached = true;
    sample.avatars[0].animations.layers = vec![ended, animation("blend.anim", 0.)];
    for tick in 1..=60 {
        sample.revision.tick = tick;
        a.observe(&sample, Arc::clone(&bank)).unwrap();
        b.observe(&sample, Arc::clone(&bank)).unwrap();
        for _ in 0..4 {
            let _ = project(&a, &sample, &bank);
        }
        if tick % 4 == 0 {
            let _ = project(&b, &sample, &bank);
        }
    }
    assert_eq!(project(&a, &sample, &bank), project(&b, &sample, &bank));
    let weak = Arc::downgrade(&bank);
    drop(bank);
    a.clear();
    b.clear();
    assert!(weak.upgrade().is_none());
}
#[test]
fn carry_pose_is_applied_after_layers_and_its_untouched_channels_are_retained() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    sample.avatars[0].animations.carry = Some(animation("carry.anim", 1.9));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let expected = project(&history, &sample, &bank);
    sample.revision.tick = 10;
    sample.avatars[0].animations.layers.clear();
    sample.avatars[0].animations.carry = None;
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_eq!(
        project(&history, &sample, &bank).models[0].groups,
        expected.models[0].groups
    );
}

#[test]
fn a_rotation_only_clip_does_not_erase_translation_written_by_the_previous_clip() {
    // A new synthetic original-format clip writes ONLY the rotation channel.
    // The production decoder and PosePlayer must retain translation independently.
    let words = |values: &[u32]| {
        values
            .iter()
            .flat_map(|n| n.to_be_bytes())
            .collect::<Vec<_>>()
    };
    let mut bytes = words(&[2]);
    bytes.extend(8i16.to_be_bytes());
    bytes.extend(b"rotation");
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend(0f32.to_le_bytes());
    bytes.push(0);
    bytes.extend(words(&[0, 2]));
    for _ in 0..2 {
        for value in [0f32, 0., 0., 1.] {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes.extend(words(&[1, 0]));
    bytes.push(4);
    bytes.extend(b"ROOT");
    bytes.extend(words(&[2]));
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend([0, 1]);
    bytes.extend((-1i32).to_be_bytes());
    bytes.extend(0i32.to_be_bytes());
    bytes.extend([0, 0]);
    let mut files = bank::files();
    files.push(("rotation.anim".into(), bytes));
    let bank = Arc::new(
        wonderland_avatar_content::import(
            wonderland_avatar_content::ImportRequest {
                files: files
                    .iter()
                    .map(|(name, bytes)| wonderland_avatar_content::NamedBytes {
                        name,
                        bytes,
                        key: None,
                    })
                    .collect(),
                skeleton_name: "adult.skel",
                collections: vec![],
            },
            &Default::default(),
        )
        .unwrap(),
    );
    let mut history = NativeAvatarPoseHistory::default();
    let mut sample = frame(9);
    sample.avatars[0]
        .animations
        .layers
        .push(animation("base.anim", 1.));
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    let translated = project(&history, &sample, &bank);
    sample.revision.tick = 10;
    sample.avatars[0].animations.layers = vec![animation("rotation.anim", 1.)];
    history.observe(&sample, Arc::clone(&bank)).unwrap();
    assert_eq!(
        project(&history, &sample, &bank).models[0].groups,
        translated.models[0].groups
    );
}
