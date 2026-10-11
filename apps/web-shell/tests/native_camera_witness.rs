//! Native scene uses accepted avatar identities and imported Vitaboy resources.
#[allow(dead_code)]
#[path = "support/native_avatar_bank.rs"]
mod bank;
use bank::{BODY, HEAD};
use wonderland_game_runtime::sim_core::avatars::outfits::{OutfitReference, OutfitState};
use wonderland_game_runtime::{
    AvatarTimeline, AvatarVisual, AvatarVisualFrame, EntityRef, ObjectId,
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

#[path = "support/native_camera_witness.rs"]
mod camera;
#[test]
fn browser_camera_witness_is_volumetric_and_visible_at_cardinal_turns() {
    use std::sync::Arc;
    use wonderland_avatar_content::{ImportLimits, ImportRequest, NamedBytes, import};
    let files = camera::volumetric(bank::files());
    let content = import(
        ImportRequest {
            files: files
                .iter()
                .map(|(name, bytes)| NamedBytes {
                    name,
                    bytes,
                    key: None,
                })
                .collect(),
            skeleton_name: "adult.skel",
            collections: vec![],
        },
        &ImportLimits::default(),
    )
    .unwrap();
    let (mut world, frame) = scene();
    // Native browser witness: north-facing actor, as in native_browser_peer.
    world.objects[0].yaw_radians = 0.;
    NativeAvatarProjection::prepare(&frame, &world, &content)
        .unwrap()
        .apply(&mut world, &content, &bank::pixels(&content))
        .unwrap();
    let bounds = world.models[0].bounds;
    assert!(
        bounds.max.z - bounds.min.z > 0.5,
        "The old planar triangle becomes edge-on; it cannot prove compositor visibility at every camera turn"
    );
    let mut renderer = WorldRenderer::new(Arc::new(world)).unwrap();
    for turn in 0..8 {
        let controls = ViewportControls {
            yaw_radians: turn as f32 * std::f32::consts::FRAC_PI_4,
            ..Default::default()
        };
        renderer.render(controls, 240, 180).unwrap();
        let red = renderer
            .image()
            .unwrap()
            .pixels
            .iter()
            .filter(|p| p[0] > 150 && p[1] < 30 && p[2] < 30 && p[3] > 0)
            .count();
        assert!(red >= 4, "camera {turn}: only {red} witness pixels");
    }
    // Only this explicitly labelled visual mesh changes; source-format animations,
    // skeleton, texture and outfit names/bytes remain the original authored fixtures.
    for ((old_name, old), (new_name, new)) in bank::files().iter().zip(&files) {
        assert_eq!(old_name, new_name);
        if !old_name.ends_with(".mesh") {
            assert_eq!(old, new);
        }
    }
}

#[test]
fn planar_camera_witness_negative_control_reproduces_an_empty_projection() {
    use std::sync::Arc;
    let content = bank::bank();
    let (mut world, frame) = scene();
    world.objects[0].yaw_radians = 0.;
    NativeAvatarProjection::prepare(&frame, &world, &content)
        .unwrap()
        .apply(&mut world, &content, &bank::pixels(&content))
        .unwrap();
    let mut renderer = WorldRenderer::new(Arc::new(world)).unwrap();
    let mut minimum = usize::MAX;
    for turn in 0..8 {
        renderer
            .render(
                ViewportControls {
                    yaw_radians: turn as f32 * std::f32::consts::FRAC_PI_4,
                    ..Default::default()
                },
                240,
                180,
            )
            .unwrap();
        minimum = minimum.min(
            renderer
                .image()
                .unwrap()
                .pixels
                .iter()
                .filter(|p| p[0] > 150 && p[1] < 30 && p[2] < 30 && p[3] > 0)
                .count(),
        );
    }
    assert_eq!(
        minimum, 0,
        "negative control must expose the old planar witness's blind angle"
    );
}
