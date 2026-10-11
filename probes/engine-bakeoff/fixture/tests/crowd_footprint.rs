use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_engine_fixture::*;
use wonderland_render_core::{AssetKey, EntityRef, ViewMode};

fn avatars(scene: &FixtureScene) -> Vec<&DrawMesh> {
    scene
        .draws
        .iter()
        .filter(|draw| draw.owner.is_some_and(|id| id.object_id >= 1000))
        .collect()
}

fn footprint(draw: &DrawMesh) -> [f32; 4] {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for vertex in &draw.mesh.vertices {
        let p = draw.model.transform_point3(vertex.position);
        bounds[0] = bounds[0].min(p.x);
        bounds[1] = bounds[1].min(p.z);
        bounds[2] = bounds[2].max(p.x);
        bounds[3] = bounds[3].max(p.z);
    }
    bounds
}

#[test]
fn every_animation_phase_keeps_actual_avatar_geometry_separate_and_inside_the_lot() {
    let baseline = representative_scene(ViewMode::Full2D, 64, 0).unwrap();
    let initial = avatars(&baseline);
    let mut minimum_separation = f32::INFINITY;
    let mut changed_pose = false;
    for tick in 0..60 {
        let scene = representative_scene(ViewMode::Full2D, 64, tick).unwrap();
        let draws = avatars(&scene);
        assert_eq!(draws.len(), 64);
        assert_eq!(scene.frame.stamp.tick, tick);
        let bounds: Vec<_> = draws.iter().map(|draw| footprint(draw)).collect();
        for (i, draw) in draws.iter().enumerate() {
            assert_eq!(
                draw.owner,
                Some(EntityRef {
                    object_id: 1000 + i as u32,
                    generation: 1
                })
            );
            assert_eq!(
                draw.model, initial[i].model,
                "placement changed at phase {tick}"
            );
            let [left, top, right, bottom] = bounds[i];
            assert!(
                left >= 0. && top >= 0. && right <= 18. && bottom <= 18.,
                "phase {tick}, avatar {i}, footprint {:?}",
                bounds[i]
            );
            for j in 0..i {
                let a = bounds[j];
                let gap = (left - a[2])
                    .max(a[0] - right)
                    .max(top - a[3])
                    .max(a[1] - bottom);
                assert!(
                    gap >= 0.125,
                    "phase {tick}, avatars {j}/{i}, separation {gap}"
                );
                minimum_separation = minimum_separation.min(gap);
            }
        }
        changed_pose |= draws[0].mesh != initial[0].mesh;
    }
    assert!(
        changed_pose,
        "all animation phases unexpectedly use the same pose"
    );
    eprintln!(
        "crowd full-cycle minimum horizontal separation: {minimum_separation:.9} world units"
    );
}

#[test]
fn corrected_fixture_versions_inputs_and_keeps_count_view_and_tick_identity_contracts() {
    assert_eq!(FIXTURE_VERSION, 2);
    let content = AssetKey(Sha256::digest(b"wonderland-c-synthetic-fixture-v2").into());
    let baseline = representative_scene(ViewMode::Full2D, 64, 30).unwrap();
    let baseline_draws = avatars(&baseline);
    for mode in [ViewMode::Full2D, ViewMode::Hybrid2D, ViewMode::Full3D] {
        for count in [32, 64] {
            let scene = representative_scene(mode, count, 30).unwrap();
            assert_eq!(scene.frame.stamp, baseline.frame.stamp);
            assert_eq!(scene.frame.stamp.content, content);
            assert_eq!(scene.frame.selected, baseline.frame.selected);
            let draws = avatars(&scene);
            assert_eq!(draws.len(), count as usize);
            for (draw, original) in draws.iter().zip(&baseline_draws) {
                assert_eq!(draw.owner, original.owner);
                assert_eq!(draw.model, original.model);
                assert_eq!(draw.mesh, original.mesh);
                let entity = scene
                    .frame
                    .entities
                    .iter()
                    .find(|entity| Some(entity.reference) == draw.owner)
                    .unwrap();
                assert_eq!(entity.visual_revision, 31);
                assert_eq!(entity.transform.matrix(), draw.model);
            }
        }
    }
    for tick in [60, 75, u64::MAX - 1] {
        let scene = representative_scene(ViewMode::Full2D, 32, tick).unwrap();
        assert_eq!(scene.frame.stamp.tick, tick);
        for (draw, original) in avatars(&scene).iter().zip(&baseline_draws) {
            assert_eq!(draw.owner, original.owner);
            assert_eq!(draw.model, original.model);
        }
        assert!(scene
            .frame
            .entities
            .iter()
            .all(|entity| entity.visual_revision == tick + 1));
    }
    assert!(representative_scene(ViewMode::Full2D, 65, 30).is_err());
    assert!(representative_scene(ViewMode::Full2D, 64, u64::MAX).is_err());
}

#[test]
fn every_view_and_count_retains_visible_meshes_sprites_and_ownerless_occlusion() {
    for mode in [ViewMode::Full2D, ViewMode::Hybrid2D, ViewMode::Full3D] {
        for count in [32, 64] {
            let scene = representative_scene(mode, count, 30).unwrap();
            let full = reference_frame(&scene).unwrap();
            let mut without_architecture = scene.clone();
            without_architecture
                .draws
                .retain(|draw| draw.owner.is_some());
            let unobstructed = reference_frame(&without_architecture).unwrap();
            let mut visible = BTreeMap::<u32, usize>::new();
            let mut occluded = BTreeMap::<u32, usize>::new();
            let mut ownerless_geometry = 0;
            for (i, id) in full.ids.iter().enumerate() {
                if let Some(id) = id {
                    *visible.entry(id.object_id).or_default() += 1;
                } else {
                    if full.depths[i].is_finite() {
                        ownerless_geometry += 1;
                    }
                    if let Some(id) = unobstructed.ids[i] {
                        *occluded.entry(id.object_id).or_default() += 1;
                    }
                }
            }
            let visible_mesh_pixels: usize = visible
                .iter()
                .filter(|(id, _)| **id >= 1000)
                .map(|(_, n)| n)
                .sum();
            let visible_meshes: BTreeSet<_> = visible.keys().filter(|id| **id >= 1000).collect();
            let blocked_pixels: usize = occluded.values().sum();
            assert!(
                visible_mesh_pixels >= 100 && visible_meshes.len() >= 4,
                "{mode:?}/{count}: {visible:?}"
            );
            for sprite in 100..103 {
                assert!(
                    visible.get(&sprite).copied().unwrap_or(0) > 0,
                    "{mode:?}/{count}: invisible sprite {sprite}"
                );
            }
            assert!(
                ownerless_geometry >= 1000,
                "{mode:?}/{count}: ownerless geometry {ownerless_geometry}"
            );
            assert!(
                blocked_pixels >= 10,
                "{mode:?}/{count}: no meaningful ownerless occlusion"
            );
            eprintln!("coverage {mode:?}/{count}: mesh_pixels={visible_mesh_pixels}, visible_meshes={}, sprite_pixels=[{},{},{}], ownerless_geometry={ownerless_geometry}, ownerless_occluded_pixels={blocked_pixels}", visible_meshes.len(), visible[&100], visible[&101], visible[&102]);
        }
    }
}
