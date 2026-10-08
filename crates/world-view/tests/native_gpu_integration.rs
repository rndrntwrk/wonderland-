use std::sync::Arc;
use wonderland_world_view::*;
#[path = "../examples/support/avatar.rs"]
mod avatar;

#[test]
fn native_avatar_gpu_packets_preserve_wrapped_uv_and_frame_identity() {
    let world = avatar::wrapped_avatar();
    let mut renderer = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    let json = serde_json::to_value(&frame).unwrap();
    let generation = frame.generation.parse().unwrap();
    let avatar_draw = frame
        .draws
        .iter()
        .enumerate()
        .find(|(_, draw)| {
            renderer
                .resolve_gpu_pick(generation, draw.pick_id, 128, 96)
                .is_some_and(|pick| matches!(pick.target, WorldPickTarget::Object { .. }))
        })
        .expect("avatar has a real GPU selection target");
    let i = avatar_draw.0;
    assert_eq!(
        json["schema"], 4,
        "wrap semantics must not be sent as a clamp-only schema"
    );
    assert_eq!(json["draws"][i]["texture_address"], "wrap");
    assert!(
        frame.meshes[avatar_draw.1.mesh]
            .vertices
            .as_chunks::<9>()
            .0
            .iter()
            .all(|v| v[3] == 1.25 && v[4] == -0.75),
        "do not wrap vertices before interpolation"
    );
    assert!(
        json["draws"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["texture_address"] == "clamp")
    );
    let picked = renderer
        .resolve_gpu_pick(generation, avatar_draw.1.pick_id, 128, 96)
        .unwrap();
    assert_eq!(picked.revision.tick, 9_007_199_254_740_993);
    assert!(
        matches!(picked.target,WorldPickTarget::Object{entity:Some(e),..} if Some(e)==world.objects[0].entity)
    );
}

fn export(world: WorldDocument) -> Vec<u8> {
    let mut job = WorldFacadeJob::new(Arc::new(world), Default::default()).unwrap();
    loop {
        if let Some(value) = job.step(128).unwrap() {
            return value.bytes.clone();
        }
    }
}
#[test]
fn facade_and_live_renderer_use_identical_avatar_addressing() {
    let wrapped = avatar::wrapped_avatar();
    let mut normalized = wrapped.clone();
    for vertex in &mut normalized.models[0].groups[0][0].mesh.vertices {
        vertex.uv.x = 0.25;
        vertex.uv.y = 0.25;
    }
    let mut a = WorldRenderer::new(Arc::new(wrapped.clone())).unwrap();
    let mut b = WorldRenderer::new(Arc::new(normalized.clone())).unwrap();
    a.render(Default::default(), 256, 192).unwrap();
    b.render(Default::default(), 256, 192).unwrap();
    assert_eq!(
        a.image(),
        b.image(),
        "live reference already wraps the original UV coordinates"
    );
    let mut missing = wrapped.clone();
    missing.objects.clear();
    missing.models.clear();
    assert_ne!(
        export(wrapped.clone()),
        export(missing),
        "the export witness must contain visible avatar pixels"
    );
    assert_eq!(
        export(wrapped),
        export(normalized),
        "facades must not silently clamp an avatar that wraps in the live view"
    );
}

#[test]
fn native_gpu_replacement_rejects_old_frames_and_keeps_hidden_avatar_unpickable() {
    let mut world = avatar::wrapped_avatar();
    let mut renderer = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    let (old, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    world.revision.tick += 1;
    world.objects[0].visible = false;
    world.objects[0].selectable = false;
    world.objects[0].visual_revision += 1;
    renderer.replace_document(Arc::new(world)).unwrap();
    for draw in old.draws {
        assert!(
            renderer
                .resolve_gpu_pick(old.generation.parse().unwrap(), draw.pick_id, 128, 96)
                .is_none()
        );
    }
    let (new, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    for draw in new.draws {
        assert!(
            !renderer
                .resolve_gpu_pick(new.generation.parse().unwrap(), draw.pick_id, 128, 96)
                .is_some_and(|p| matches!(p.target, WorldPickTarget::Object { .. }))
        );
    }
}

fn native_pose_pair() -> (WorldDocument, WorldDocument) {
    let mut old = avatar::wrapped_avatar();
    old.provenance.kind = WorldSourceKind::LiveSession;
    old.objects[0].visual_revision = old.revision.tick;
    old.models[0].effective_content = old.models[0].effective_source;
    let mut new = old.clone();
    new.revision.tick += 1;
    new.objects[0].visual_revision = new.revision.tick;
    new.models[0].effective_source = wonderland_render_core::AssetKey([29; 32]);
    new.models[0].effective_content = new.models[0].effective_source;
    for v in &mut new.models[0].groups[0][0].mesh.vertices {
        v.position.y += 0.1;
    }
    new.models[0].bounds.min.y += 0.1;
    new.models[0].bounds.max.y += 0.1;
    (old, new)
}
#[test]
fn native_pose_only_change_admits_without_relabeling_authoritative_content() {
    let (old, new) = native_pose_pair();
    assert_eq!(old.revision.content, new.revision.content);
    let mut renderer = WorldRenderer::new(Arc::new(old)).unwrap();
    let (prior, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    renderer
        .replace_document(Arc::new(new))
        .expect("accepted posed geometry is not an immutable content-pack mutation");
    let (current, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    assert_ne!(prior.generation, current.generation);
    assert_ne!(
        serde_json::to_value(prior.meshes).unwrap(),
        serde_json::to_value(current.meshes).unwrap()
    );
}
#[test]
fn pose_admission_does_not_allow_unchanged_resource_identity_to_hide_texture_edits() {
    let (old, mut new) = native_pose_pair();
    new.models[0].textures[0].image.pixels[0][0] += 1;
    let mut renderer = WorldRenderer::new(Arc::new(old)).unwrap();
    assert!(renderer.replace_document(Arc::new(new)).is_err());
}
#[test]
fn pose_admission_rejects_same_tick_reused_pose_digest_and_non_native_sources() {
    for which in 0..3 {
        let (mut old, mut new) = native_pose_pair();
        match which {
            0 => new.revision.tick = old.revision.tick,
            1 => {
                new.models[0].effective_source = old.models[0].effective_source;
                new.models[0].effective_content = old.models[0].effective_content;
            }
            _ => {
                old.provenance.kind = WorldSourceKind::TestFixture;
                new.provenance.kind = old.provenance.kind;
            }
        }
        let mut renderer = WorldRenderer::new(Arc::new(old)).unwrap();
        assert!(
            renderer.replace_document(Arc::new(new)).is_err(),
            "invalid pose case {which} admitted"
        );
    }
}

#[test]
fn failed_pose_replacement_keeps_the_previously_admitted_gpu_pick() {
    let (old, mut bad) = native_pose_pair();
    bad.models[0].textures[0].image.pixels[0][1] -= 1;
    let mut renderer = WorldRenderer::new(Arc::new(old)).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    let id = frame
        .draws
        .iter()
        .find(|d| {
            renderer
                .resolve_gpu_pick(frame.generation.parse().unwrap(), d.pick_id, 128, 96)
                .is_some_and(|p| matches!(p.target, WorldPickTarget::Object { .. }))
        })
        .unwrap()
        .pick_id;
    assert!(renderer.replace_document(Arc::new(bad)).is_err());
    assert!(
        renderer
            .resolve_gpu_pick(frame.generation.parse().unwrap(), id, 128, 96)
            .is_some()
    );
}

#[test]
fn accepted_animation_does_not_exempt_static_models_or_ownerless_pose_data() {
    for case in 0..4 {
        let (mut old, mut new) = native_pose_pair();
        match case {
            0 => {
                let mut original = old.models[0].clone();
                original.context = ModelContext::Standalone;
                original.format_version = 3;
                original.effective_source = wonderland_render_core::AssetKey([61; 32]);
                old.models.push(original.clone());
                original.bounds.min.y -= 0.1;
                new.models.push(original);
            }
            1 => new.objects[0].model = None,
            2 => new.objects[0].visual_revision = old.revision.tick,
            _ => new.provenance.effective_source = wonderland_render_core::AssetKey([63; 32]),
        }
        let mut renderer = WorldRenderer::new(Arc::new(old)).unwrap();
        assert!(
            renderer.replace_document(Arc::new(new)).is_err(),
            "unsafe posed-resource case {case}"
        );
    }
}

#[test]
fn accepted_hidden_pose_removal_and_new_entity_generation_can_repopulate_gpu_scene() {
    let (old, mut hidden) = native_pose_pair();
    let mut renderer = WorldRenderer::new(Arc::new(old)).unwrap();
    hidden.models.clear();
    hidden.objects[0].model = None;
    hidden.objects[0].visible = false;
    hidden.objects[0].selectable = false;
    renderer.replace_document(Arc::new(hidden.clone())).unwrap();
    let mut restored = native_pose_pair().1;
    restored.revision.tick = hidden.revision.tick + 1;
    restored.objects[0].visual_revision = restored.revision.tick;
    restored.objects[0].entity.as_mut().unwrap().generation += 1;
    let generation = restored.objects[0].entity.unwrap().generation;
    renderer.replace_document(Arc::new(restored)).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    assert!(frame.draws.iter().any(|d|renderer.resolve_gpu_pick(frame.generation.parse().unwrap(),d.pick_id,128,96).is_some_and(|p|matches!(p.target,WorldPickTarget::Object{entity:Some(e),..} if e.generation==generation))));
}
