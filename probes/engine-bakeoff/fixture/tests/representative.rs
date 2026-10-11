use wonderland_engine_fixture::*;
use wonderland_render_core::{RenderLimits, ViewMode};

#[test]
fn same_inputs_produce_identical_engine_payloads_and_preserve_game_identity() {
    let a = representative_scene(ViewMode::Hybrid2D, 32, 15).unwrap();
    let b = representative_scene(ViewMode::Hybrid2D, 32, 15).unwrap();
    assert_eq!(a.hash, b.hash);
    assert_eq!(
        bincode::serialize(&a).unwrap(),
        bincode::serialize(&b).unwrap()
    );
    assert_eq!(a.frame.stamp.tick, 15);
    assert_eq!(a.frame.entities.len(), 35);
    assert_eq!(
        a.draws
            .iter()
            .filter(|d| d.name.starts_with("avatar-"))
            .count(),
        32
    );
    assert_eq!(a.sprites.len(), 3);
    for draw in &a.draws {
        draw.mesh.validate(&RenderLimits::default()).unwrap();
        if let Some(id) = draw.owner {
            assert!(a.frame.entities.iter().any(|e| e.reference == id));
        }
    }
    assert!(a
        .sprites
        .iter()
        .all(|s| a.frame.entities.iter().any(|e| e.reference == s.owner)));
}

#[test]
fn three_views_keep_the_same_selection_and_simulation_stamp() {
    let modes = [ViewMode::Full2D, ViewMode::Hybrid2D, ViewMode::Full3D];
    let scenes: Vec<_> = modes
        .into_iter()
        .map(|mode| representative_scene(mode, 64, 30).unwrap())
        .collect();
    assert_eq!(scenes[0].frame.stamp, scenes[1].frame.stamp);
    assert_eq!(scenes[1].frame.stamp, scenes[2].frame.stamp);
    assert_eq!(scenes[0].frame.selected, scenes[2].frame.selected);
    assert_ne!(scenes[0].hash, scenes[1].hash);
    assert_ne!(scenes[1].hash, scenes[2].hash);
    assert!(scenes[0].camera.orthographic && scenes[1].camera.orthographic);
    assert!(!scenes[2].camera.orthographic);
}

#[test]
fn fixture_limits_fail_before_building_unbounded_avatar_meshes() {
    assert!(representative_scene(ViewMode::Full3D, 65, 0).is_err());
    assert!(representative_scene(ViewMode::Full2D, 0, 0).is_ok());
}

#[test]
fn render_cadence_cannot_duplicate_accepted_audio_cues() {
    let (baseline, starts) = audio_reference(30).unwrap();
    assert_eq!(starts, 3);
    assert_eq!(baseline.len(), 96000);
    assert!(baseline.iter().any(|v| *v != 0));
    for hz in [60, 120] {
        let (samples, count) = audio_reference(hz).unwrap();
        assert_eq!(count, 3);
        assert_eq!(samples, baseline);
    }
}

#[test]
fn reference_renders_actual_geometry_and_game_id_buffers() {
    let scene = representative_scene(ViewMode::Hybrid2D, 32, 30).unwrap();
    let frame = reference_frame(&scene).unwrap();
    assert_eq!(frame.image.pixels.len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.ids.len(), frame.image.pixels.len());
    assert!(
        frame
            .image
            .pixels
            .iter()
            .filter(|pixel| **pixel != [22, 29, 40, 255])
            .count()
            > 10000
    );
    assert!(frame.ids.iter().flatten().any(|id| id.object_id >= 1000));
    assert!(frame
        .ids
        .iter()
        .flatten()
        .any(|id| (100..103).contains(&id.object_id)));
    assert!(pick_at(&scene, WIDTH, 10).is_none());
    assert!(pick_at(&scene, 10, HEIGHT).is_none());
}

#[test]
fn source_mask_hole_replaces_sprite_alpha_in_color_and_selection_passes() {
    let mut scene = representative_scene(ViewMode::Full2D, 0, 0).unwrap();
    scene.draws.clear();
    scene.sprites = vec![scene.sprites[1].clone()];
    let sprite = &scene.sprites[0];
    let x = (sprite.rect[0] + sprite.rect[2] * 0.5) as u32;
    let y = (sprite.rect[1] + sprite.rect[3] * 0.5) as u32;
    let frame = reference_frame(&scene).unwrap();
    assert_eq!(
        frame.image.pixels[(y * WIDTH + x) as usize],
        [22, 29, 40, 255]
    );
    assert_eq!(frame.ids[(y * WIDTH + x) as usize], None);
}
