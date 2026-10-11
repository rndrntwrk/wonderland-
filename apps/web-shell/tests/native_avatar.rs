//! Native scene uses accepted avatar identities and imported Vitaboy resources.
#[path = "support/native_avatar_bank.rs"]
mod bank;
use bank::{BODY, HEAD};
use wonderland_game_runtime::sim_core::avatars::outfits::{OutfitReference, OutfitState};
use wonderland_game_runtime::{
    AvatarAnimation, AvatarTimeline, AvatarVisual, AvatarVisualFrame, EntityRef, ObjectId,
};
use wonderland_render_core::{AssetKey, Vec3};
use wonderland_web_shell::native_avatar::NativeAvatarProjection;
use wonderland_world_view::*;

const ADULT: u32 = 0x7FD96B54;
fn scene() -> (WorldDocument, AvatarVisualFrame) {
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "test:native-avatar",
        "declared synthetic fixture",
    )
    .unwrap();
    world.provenance.kind = WorldSourceKind::LiveSession;
    world.source_counts = None;
    world.revision = WorldRevision {
        lot_id: Some(11),
        epoch: 7,
        tick: 9,
        architecture_revision: 1,
        content: AssetKey([42; 32]),
    };
    let entity = EntityRef {
        object_id: ObjectId(3),
        generation: 4,
    };
    world.objects.push(WorldObject {
        source_guid: ADULT,
        blueprint: None,
        snapshot: None,
        entity: Some(wonderland_render_core::EntityRef {
            object_id: 3,
            generation: 4,
        }),
        visual_revision: 2,
        position_tiles: Vec3::new(3.5, 3.5, 0.),
        yaw_radians: 1.2,
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
            entity,
            guid: ADULT,
            visual_revision: 2,
            container: None,
            scale_percent: 100,
            display_flags: 0,
            ghost: false,
            outfits: OutfitState {
                head: Some(OutfitReference::Id(HEAD)),
                body: Some(OutfitReference::Id(BODY)),
                ..Default::default()
            },
            animations: AvatarTimeline::default(),
        }],
    };
    (world, frame)
}
fn apply(world: &mut WorldDocument, frame: &AvatarVisualFrame) {
    let bank = bank::bank();
    NativeAvatarProjection::prepare(frame, world, &bank)
        .unwrap()
        .apply(world, &bank, &bank::pixels(&bank))
        .unwrap();
}
#[test]
fn admitted_avatar_gets_original_format_mesh_without_changing_simulation_identity() {
    let (mut world, frame) = scene();
    let revision = world.revision;
    let before = world.objects[0].clone();
    apply(&mut world, &frame);
    let model = &world.models[world.objects[0]
        .model
        .expect("native avatar must not be blank")];
    assert_eq!(model.context, ModelContext::Vitaboy);
    assert_eq!(model.bounds.max.y, 4.);
    let mut after = world.objects[0].clone();
    after.model = None;
    assert_eq!(after.visual_revision, frame.revision.tick);
    after.visual_revision = before.visual_revision;
    assert_eq!(after, before);
    assert_eq!(world.revision, revision);
}
#[test]
fn avatar_projection_does_not_accept_a_frame_from_a_different_tick_or_generation() {
    let (world, mut frame) = scene();
    let bank = bank::bank();
    frame.revision.tick += 1;
    assert!(NativeAvatarProjection::prepare(&frame, &world, &bank).is_err());
    frame.revision = world.revision;
    frame.avatars[0].entity.generation += 1;
    assert!(NativeAvatarProjection::prepare(&frame, &world, &bank).is_err());
}
#[test]
fn accepted_current_animation_frames_change_the_skinned_vertices() {
    let (mut a, mut frame) = scene();
    let bank = bank::bank();
    let clip = bank.animation("base.anim").unwrap();
    let animation = AvatarAnimation {
        resource: "base.anim".into(),
        num_frames: clip.source().num_frames,
        current_frame: 0.,
        speed: 1.,
        weight: 1.,
        backwards: false,
        end_reached: false,
        looping: false,
    };
    frame.avatars[0].animations.layers.push(animation);
    NativeAvatarProjection::prepare(&frame, &a, &bank)
        .unwrap()
        .apply(&mut a, &bank, &bank::pixels(&bank))
        .unwrap();
    let mut b = scene().0;
    b.revision.tick += 1;
    frame.revision = b.revision;
    frame.avatars[0].animations.layers[0].current_frame = 1.;
    NativeAvatarProjection::prepare(&frame, &b, &bank)
        .unwrap()
        .apply(&mut b, &bank, &bank::pixels(&bank))
        .unwrap();
    assert_ne!(a.models[0].groups[0][0].mesh, b.models[0].groups[0][0].mesh);
    assert_ne!(a.models[0].effective_content, b.models[0].effective_content);
}
#[test]
fn absent_texture_is_diagnosed_without_inventing_a_model() {
    let (mut world, frame) = scene();
    let bank = bank::bank();
    NativeAvatarProjection::prepare(&frame, &world, &bank)
        .unwrap()
        .apply(&mut world, &bank, &Default::default())
        .unwrap();
    assert!(world.objects[0].model.is_none());
    assert!(
        world
            .diagnostics
            .iter()
            .any(|d| d.code == "native_avatar_texture_unavailable")
    );
}
#[test]
fn container_and_unresolved_rig_do_not_render_a_grounded_substitute() {
    for container in [true, false] {
        let (mut world, mut frame) = scene();
        if container {
            frame.avatars[0].container = Some((
                EntityRef {
                    object_id: ObjectId(9),
                    generation: 1,
                },
                0,
            ));
        } else {
            frame.avatars[0].guid = 123;
            world.objects[0].source_guid = 123;
        }
        apply(&mut world, &frame);
        assert!(world.objects[0].model.is_none());
        assert!(
            world
                .diagnostics
                .iter()
                .any(|d| d.code == "native_avatar_resource_unavailable")
        );
    }
}
#[test]
fn scaling_and_display_tint_use_the_accepted_avatar_fields() {
    let (mut world, mut frame) = scene();
    frame.avatars[0].scale_percent = 50;
    frame.avatars[0].ghost = true;
    apply(&mut world, &frame);
    let model = &world.models[world.objects[0].model.unwrap()];
    assert_eq!(model.bounds.max.y, 2.);
    assert_eq!(
        model.groups[0][0].mesh.vertices[0].color,
        [1., 1., 1., 64. / 255.]
    );
}
#[test]
fn prepared_texture_results_cannot_attach_to_replaced_world() {
    let (mut world, frame) = scene();
    let bank = bank::bank();
    let ready = NativeAvatarProjection::prepare(&frame, &world, &bank).unwrap();
    world.revision.epoch += 1;
    let before = world.clone();
    assert!(
        ready
            .apply(&mut world, &bank, &bank::pixels(&bank))
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn hidden_avatars_do_not_allocate_models_or_request_textures() {
    let (mut world, frame) = scene();
    world.objects[0].visible = false;
    world.objects[0].selectable = false;
    let bank = bank::bank();
    let prepared = NativeAvatarProjection::prepare(&frame, &world, &bank).unwrap();
    assert!(prepared.texture_keys().is_empty());
    prepared
        .apply(&mut world, &bank, &bank::pixels(&bank))
        .unwrap();
    assert!(world.models.is_empty());
}
#[test]
fn animation_metadata_mismatch_cannot_be_silently_rendered() {
    let (mut world, mut frame) = scene();
    frame.avatars[0].animations.layers.push(AvatarAnimation {
        resource: "base.anim".into(),
        num_frames: 999,
        current_frame: 0.,
        speed: 1.,
        weight: 1.,
        backwards: false,
        end_reached: false,
        looping: false,
    });
    apply(&mut world, &frame);
    assert!(world.models.is_empty());
    assert!(
        world
            .diagnostics
            .iter()
            .any(|d| d.message.contains("metadata differs"))
    );
}
#[test]
fn invalid_gesture_does_not_strip_hands_and_present_an_incomplete_avatar() {
    let (mut world, mut frame) = scene();
    frame.avatars[0].animations.left_hand = 900;
    apply(&mut world, &frame);
    assert!(world.models.is_empty());
    assert!(
        world
            .diagnostics
            .iter()
            .any(|d| d.code == "native_avatar_resource_unavailable")
    );
}
#[test]
fn duplicate_avatar_frame_identity_is_rejected_before_mutation() {
    let (world, mut frame) = scene();
    frame.avatars.push(frame.avatars[0].clone());
    let bank = bank::bank();
    assert!(NativeAvatarProjection::prepare(&frame, &world, &bank).is_err());
}

#[test]
fn native_pose_frames_admit_through_real_frame_store_and_expire_previous_picks() {
    use std::sync::Arc;
    let (mut a, mut frame) = scene();
    frame.avatars[0].animations.layers.push(AvatarAnimation {
        resource: "base.anim".into(),
        num_frames: 2,
        current_frame: 0.,
        speed: 1.,
        weight: 1.,
        backwards: false,
        end_reached: false,
        looping: true,
    });
    apply(&mut a, &frame);
    let mut renderer = WorldRenderer::new(Arc::new(a)).unwrap();
    renderer
        .render(ViewportControls::default(), 240, 240)
        .unwrap();
    let pick = (0..240)
        .flat_map(|y| (0..240).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            renderer.pick(x, y).filter(|p| {
                matches!(
                    p.target,
                    WorldPickTarget::Object {
                        entity: Some(_),
                        ..
                    }
                )
            })
        })
        .expect("Native avatar has real depth-tested pixels");
    let (mut b, _) = scene();
    b.revision.tick += 1;
    frame.revision = b.revision;
    frame.avatars[0].animations.layers[0].current_frame = 1.;
    apply(&mut b, &frame);
    renderer
        .replace_document(Arc::new(b))
        .expect("A new accepted pose needs a new visual revision");
    assert!(renderer.resolve_pick(&pick).is_none());
    renderer
        .render(ViewportControls::default(), 240, 240)
        .unwrap();
}

fn native_pick(world: &WorldDocument) -> WorldPick {
    WorldPick {
        revision: world.revision,
        frame_generation: 1,
        screen: [10, 10],
        target: WorldPickTarget::Object {
            entity: world.objects[0].entity,
            source_guid: world.objects[0].source_guid,
            source_record: None,
        },
    }
}
#[test]
fn a_newer_hidden_or_unselectable_scene_cannot_accept_a_previous_visible_mesh_pick() {
    use wonderland_web_shell::native_avatar::native_pick_entity;
    let (world, _) = scene();
    let pick = native_pick(&world);
    for (visible, selectable) in [(false, false), (true, false), (false, true)] {
        let mut current = world.clone();
        current.revision.tick += 1;
        current.objects[0].visible = visible;
        current.objects[0].selectable = selectable;
        assert_eq!(native_pick_entity(&current, &pick), None);
    }
}
#[test]
fn current_visible_mesh_selection_keeps_identity_across_ordinary_tick_latency() {
    use wonderland_web_shell::native_avatar::native_pick_entity;
    let (mut world, frame) = scene();
    let pick = native_pick(&world);
    world.revision.tick += 2;
    assert_eq!(
        native_pick_entity(&world, &pick),
        Some(frame.avatars[0].entity)
    );
}
#[test]
fn native_mesh_selection_rejects_replaced_entity_and_source_lifetimes() {
    use wonderland_web_shell::native_avatar::native_pick_entity;
    let (world, _) = scene();
    let pick = native_pick(&world);
    for case in 0..6 {
        let mut current = world.clone();
        match case {
            0 => current.revision.epoch += 1,
            1 => current.revision.lot_id = Some(99),
            2 => current.revision.content = AssetKey([99; 32]),
            3 => current.objects[0].entity.as_mut().unwrap().generation += 1,
            4 => current.objects[0].source_guid += 1,
            _ => current.objects.clear(),
        }
        assert_eq!(native_pick_entity(&current, &pick), None, "case {case}");
    }
}
