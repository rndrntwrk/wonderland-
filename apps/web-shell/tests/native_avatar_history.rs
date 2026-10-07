//! Presentation retention, not fabricated pre-checkpoint or authoritative bone state.
#[path = "support/native_avatar_bank.rs"]
mod bank;
use std::sync::Arc;
use wonderland_avatar_content::{self as content, ImportedContent};
use wonderland_game_runtime::sim_core::avatars::outfits::{OutfitReference, OutfitState};
use wonderland_game_runtime::{
    AvatarAnimation, AvatarTimeline, AvatarVisual, AvatarVisualFrame, EntityRef, ObjectId,
};
use wonderland_render_core::{AssetKey, Vec3};
use wonderland_web_shell::native_avatar::NativeAvatarHistory;
use wonderland_world_view::*;

fn scene(tick: u64) -> (WorldDocument, AvatarVisualFrame) {
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:retained-pose",
        "explicit original-format synthetic fixture",
    )
    .unwrap();
    world.provenance.kind = WorldSourceKind::LiveSession;
    world.source_counts = None;
    world.revision = WorldRevision {
        lot_id: Some(11),
        epoch: 7,
        tick,
        architecture_revision: 1,
        content: AssetKey([42; 32]),
    };
    world.objects.push(WorldObject {
        source_guid: 0x7FD96B54,
        blueprint: None,
        snapshot: None,
        entity: Some(wonderland_render_core::EntityRef {
            object_id: 3,
            generation: 4,
        }),
        visual_revision: 2,
        position_tiles: Vec3::new(3.5, 3.5, 0.),
        yaw_radians: 0.,
        dynamic_flags: [0; 2],
        room: 0,
        level: 1,
        visible: true,
        selectable: true,
        model: None,
    });
    let frame = AvatarVisualFrame {
        revision: world.revision,
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
    };
    (world, frame)
}
fn layer(name: &str, frame: f32) -> AvatarAnimation {
    AvatarAnimation {
        resource: name.into(),
        num_frames: 2,
        current_frame: frame,
        speed: 1.,
        weight: 1.,
        backwards: false,
        end_reached: false,
        looping: false,
    }
}
fn render(
    history: &mut NativeAvatarHistory,
    bank: &Arc<ImportedContent>,
    world: &mut WorldDocument,
    frame: &AvatarVisualFrame,
) {
    let prepared = history.prepare(frame, world, bank).unwrap();
    history.apply(prepared, world, &bank::pixels(bank)).unwrap();
}
fn mesh(world: &WorldDocument) -> &wonderland_render_core::Mesh {
    &world.models[world.objects[0].model.unwrap()].groups[0][0].mesh
}
fn at(tick: u64, clip: &str, value: f32) -> (WorldDocument, AvatarVisualFrame) {
    let (world, mut frame) = scene(tick);
    frame.avatars[0].animations.layers.push(layer(clip, value));
    (world, frame)
}
fn bank_with_rotation() -> Arc<ImportedContent> {
    // Rotation-only channel: source RenderFrame must not clear its retained translation.
    let name = "turn";
    let mut bytes = 2u32.to_be_bytes().to_vec();
    bytes.extend((name.len() as i16).to_be_bytes());
    bytes.extend(name.as_bytes());
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend(0f32.to_le_bytes());
    bytes.push(0);
    bytes.extend(0u32.to_be_bytes());
    bytes.extend(2u32.to_be_bytes());
    for quaternion in [
        [0f32, 0., 0., 1.],
        [
            0.,
            0.,
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ],
    ] {
        for n in quaternion {
            bytes.extend(n.to_le_bytes());
        }
    }
    for n in [1u32, 0] {
        bytes.extend(n.to_be_bytes());
    }
    bytes.push(4);
    bytes.extend(b"ROOT");
    bytes.extend(2u32.to_be_bytes());
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend([0, 1]);
    bytes.extend((-1i32).to_be_bytes());
    bytes.extend(0i32.to_be_bytes());
    bytes.extend([0, 0]);
    let mut files = bank::files();
    files.push(("turn.anim".into(), bytes));
    Arc::new(
        content::import(
            content::ImportRequest {
                files: files
                    .iter()
                    .map(|(name, bytes)| content::NamedBytes {
                        name,
                        bytes,
                        key: None,
                    })
                    .collect(),
                skeleton_name: "adult.skel",
                collections: vec![],
            },
            &content::ImportLimits::default(),
        )
        .unwrap(),
    )
}
#[test]
fn ended_layer_retains_the_last_presented_skeleton_instead_of_snapping_to_bind_pose() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut first, frame) = at(9, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    let (mut end, mut frame) = at(10, "base.anim", 1.);
    frame.avatars[0].animations.layers[0].end_reached = true;
    render(&mut history, &bank, &mut end, &frame);
    assert_eq!(
        mesh(&end),
        mesh(&first),
        "EndReached writes no new channels; previously sampled bone transforms must survive"
    );
    let (mut bind, frame) = scene(11);
    render(
        &mut NativeAvatarHistory::default(),
        &bank,
        &mut bind,
        &frame,
    );
    assert_ne!(
        mesh(&first),
        mesh(&bind),
        "The witness must actually differ from the bind pose"
    );
}
#[test]
fn replacing_a_translation_clip_with_a_rotation_only_clip_preserves_translation() {
    let bank = bank_with_rotation();
    let mut history = NativeAvatarHistory::default();
    let (mut initial, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut initial, &frame);
    let (mut rotated, frame) = at(2, "turn.anim", 1.);
    render(&mut history, &bank, &mut rotated, &frame);
    let (mut bind_rotated, frame) = at(2, "turn.anim", 1.);
    render(
        &mut NativeAvatarHistory::default(),
        &bank,
        &mut bind_rotated,
        &frame,
    );
    for (actual, bind) in mesh(&rotated)
        .vertices
        .iter()
        .zip(&mesh(&bind_rotated).vertices)
    {
        // The original decoder negates source X; a translation of +4 becomes -4.
        assert!((actual.position.x - bind.position.x + 4.).abs() < 0.00001);
    }
    assert_ne!(
        mesh(&rotated),
        mesh(&initial),
        "Rotation-only clip must still rotate the actual mesh"
    );
}
#[test]
fn same_tick_repreparation_does_not_accumulate_partial_layer_blending() {
    let bank = bank_with_rotation();
    let mut history = NativeAvatarHistory::default();
    let (mut initial, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut initial, &frame);
    let (template, mut frame) = at(2, "turn.anim", 0.);
    frame.avatars[0]
        .animations
        .layers
        .push(layer("blend.anim", 0.));
    let mut first = template.clone();
    render(&mut history, &bank, &mut first, &frame);
    for _ in 0..12 {
        let mut repeated = template.clone();
        render(&mut history, &bank, &mut repeated, &frame);
        assert_eq!(mesh(&first), mesh(&repeated));
    }
    // 0.5 * source translation -8 + 0.5 * retained -4 = -6.
    assert!(
        (mesh(&first).vertices[0].position.x - mesh(&initial).vertices[0].position.x + 2.).abs()
            < 0.00001
    );
}
#[test]
fn removing_carry_keeps_channels_unwritten_by_the_remaining_timeline() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut carried, mut frame) = scene(1);
    frame.avatars[0].animations.carry = Some(layer("carry.anim", 1.9));
    render(&mut history, &bank, &mut carried, &frame);
    let (mut removed, frame) = scene(2);
    render(&mut history, &bank, &mut removed, &frame);
    assert_eq!(mesh(&removed), mesh(&carried));
}
#[test]
fn hidden_avatar_can_update_retained_channels_without_allocating_a_model() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut hidden, frame) = at(1, "base.anim", 1.);
    hidden.objects[0].visible = false;
    let prepared = history.prepare(&frame, &hidden, &bank).unwrap();
    assert!(prepared.texture_keys().is_empty());
    history
        .apply(prepared, &mut hidden, &Default::default())
        .unwrap();
    assert!(hidden.models.is_empty());
    let (mut visible, frame) = scene(2);
    render(&mut history, &bank, &mut visible, &frame);
    let (mut expected, frame) = at(2, "base.anim", 1.);
    render(
        &mut NativeAvatarHistory::default(),
        &bank,
        &mut expected,
        &frame,
    );
    assert_eq!(mesh(&visible), mesh(&expected));
}
#[test]
fn new_entity_generation_does_not_inherit_the_departed_avatars_pose() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut initial, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut initial, &frame);
    let (mut replacement, mut frame) = scene(2);
    replacement.objects[0].entity.as_mut().unwrap().generation += 1;
    frame.avatars[0].entity.generation += 1;
    let mut fresh = replacement.clone();
    render(
        &mut NativeAvatarHistory::default(),
        &bank,
        &mut fresh,
        &frame,
    );
    render(&mut history, &bank, &mut replacement, &frame);
    assert_eq!(mesh(&replacement), mesh(&fresh));
    assert_eq!(history.retained_avatar_count(), 1);
}
#[test]
fn changing_bank_or_world_scope_discards_visual_history() {
    for change in 0..4 {
        let bank = Arc::new(bank::bank());
        let mut history = NativeAvatarHistory::default();
        let (mut initial, frame) = at(1, "base.anim", 1.);
        render(&mut history, &bank, &mut initial, &frame);
        let (mut world, mut frame) = scene(2);
        let mut selected = Arc::clone(&bank);
        match change {
            0 => selected = Arc::new(bank::bank()),
            1 => world.revision.epoch += 1,
            2 => world.revision.lot_id = Some(12),
            3 => world.revision.content = AssetKey([9; 32]),
            _ => unreachable!(),
        }
        frame.revision = world.revision;
        let mut expected = world.clone();
        render(
            &mut NativeAvatarHistory::default(),
            &selected,
            &mut expected,
            &frame,
        );
        render(&mut history, &selected, &mut world, &frame);
        assert_eq!(mesh(&world), mesh(&expected));
    }
}
#[test]
fn stale_preparations_and_reset_cannot_commit_into_a_newer_history() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut old, frame) = at(1, "base.anim", 1.);
    let prepared = history.prepare(&frame, &old, &bank).unwrap();
    let (mut current, frame) = at(2, "blend.anim", 1.);
    render(&mut history, &bank, &mut current, &frame);
    let before = old.clone();
    assert!(
        history
            .apply(prepared, &mut old, &bank::pixels(&bank))
            .is_err()
    );
    assert_eq!(old, before);
    let (mut old, frame) = at(3, "base.anim", 0.);
    let prepared = history.prepare(&frame, &old, &bank).unwrap();
    history.clear();
    assert!(
        history
            .apply(prepared, &mut old, &bank::pixels(&bank))
            .is_err()
    );
    assert_eq!(history.retained_avatar_count(), 0);
}
#[test]
fn backwards_or_conflicting_same_tick_samples_are_rejected_without_replacing_history() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut first, frame) = at(9, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    let (world, frame) = at(8, "base.anim", 0.);
    assert!(history.prepare(&frame, &world, &bank).is_err());
    let (world, frame) = at(9, "base.anim", 0.);
    assert!(history.prepare(&frame, &world, &bank).is_err());
    let (mut retained, frame) = scene(10);
    render(&mut history, &bank, &mut retained, &frame);
    assert_eq!(mesh(&retained), mesh(&first));
}
#[test]
fn failed_scene_application_does_not_commit_a_candidate_pose() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut first, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    let (mut stale, frame) = at(2, "blend.anim", 1.);
    let sample = history.prepare(&frame, &stale, &bank).unwrap();
    stale.objects[0].visual_revision += 1;
    let before = stale.clone();
    assert!(
        history
            .apply(sample, &mut stale, &bank::pixels(&bank))
            .is_err()
    );
    assert_eq!(stale, before);
    let (mut retained, frame) = scene(3);
    render(&mut history, &bank, &mut retained, &frame);
    assert_eq!(mesh(&retained), mesh(&first));
}
#[test]
fn departure_prunes_pose_and_new_checkpoint_does_not_invent_prior_history() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut first, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    let (mut empty, mut frame) = scene(2);
    empty.objects.clear();
    frame.avatars.clear();
    render(&mut history, &bank, &mut empty, &frame);
    assert_eq!(history.retained_avatar_count(), 0);
    let (mut first, frame) = at(3, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    history.clear();
    let (mut restored, frame) = scene(4);
    render(&mut history, &bank, &mut restored, &frame);
    let (mut fresh, frame) = scene(4);
    render(
        &mut NativeAvatarHistory::default(),
        &bank,
        &mut fresh,
        &frame,
    );
    assert_eq!(mesh(&restored), mesh(&fresh));
}

#[test]
fn a_prepared_pose_is_owned_by_one_history_even_before_its_first_commit() {
    let bank = Arc::new(bank::bank());
    let a = NativeAvatarHistory::default();
    let mut b = NativeAvatarHistory::default();
    let (mut world, frame) = at(1, "base.anim", 1.);
    let before = world.clone();
    let prepared = a.prepare(&frame, &world, &bank).unwrap();
    assert!(b.apply(prepared, &mut world, &bank::pixels(&bank)).is_err());
    assert_eq!(world, before);
}
#[test]
fn pose_working_set_is_bounded_before_any_per_avatar_preparation() {
    let bank = Arc::new(bank::bank());
    let history = NativeAvatarHistory::default();
    let (world, mut frame) = scene(1);
    frame.avatars.resize(1025, frame.avatars[0].clone());
    assert!(
        history
            .prepare(&frame, &world, &bank)
            .err()
            .unwrap()
            .to_string()
            .contains("working-set")
    );
    assert_eq!(history.retained_avatar_count(), 0);
}
#[test]
fn missing_texture_completion_and_camera_repreparation_do_not_drift_the_same_pose() {
    let bank = bank_with_rotation();
    let mut history = NativeAvatarHistory::default();
    let (mut first, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    let (mut empty_pixels, mut frame) = at(2, "turn.anim", 0.);
    frame.avatars[0]
        .animations
        .layers
        .push(layer("blend.anim", 0.));
    let prepared = history.prepare(&frame, &empty_pixels, &bank).unwrap();
    assert!(!prepared.texture_keys().is_empty());
    history
        .apply(prepared, &mut empty_pixels, &Default::default())
        .unwrap();
    assert!(empty_pixels.models.is_empty());
    let (mut ready, _) = scene(2);
    render(&mut history, &bank, &mut ready, &frame);
    assert!(
        (mesh(&ready).vertices[0].position.x - mesh(&first).vertices[0].position.x + 2.).abs()
            < 0.00001
    );
    let (mut repeat, _) = scene(2);
    render(&mut history, &bank, &mut repeat, &frame);
    assert_eq!(mesh(&repeat), mesh(&ready));
}
#[test]
fn invalid_clip_drops_unproven_history_without_corrupting_world_identity() {
    let bank = Arc::new(bank::bank());
    let mut history = NativeAvatarHistory::default();
    let (mut first, frame) = at(1, "base.anim", 1.);
    render(&mut history, &bank, &mut first, &frame);
    let (mut unavailable, frame) = at(2, "missing.anim", 1.);
    let revision = unavailable.revision;
    render(&mut history, &bank, &mut unavailable, &frame);
    assert_eq!(history.retained_avatar_count(), 0);
    assert!(unavailable.models.is_empty());
    assert_eq!(unavailable.revision, revision);
    let (mut later, frame) = scene(3);
    render(&mut history, &bank, &mut later, &frame);
    let (mut baseline, frame) = scene(3);
    render(
        &mut NativeAvatarHistory::default(),
        &bank,
        &mut baseline,
        &frame,
    );
    assert_eq!(mesh(&later), mesh(&baseline));
}
