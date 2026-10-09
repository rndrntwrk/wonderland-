use std::sync::Arc;
use wonderland_world_view::*;
#[path = "../examples/support/masked.rs"]
mod support;

#[test]
fn masked_source_models_are_not_omitted_from_the_real_scene() {
    for kind in [ModelMaskKind::Normal, ModelMaskKind::Portal] {
        let document = support::masked(kind);
        document.validate().unwrap();
        let scene = build_scene(&document, ViewportControls::default()).unwrap();
        assert!(
            scene.parts.iter().any(|part| part.object == Some(0)),
            "masked object was dropped"
        );
        assert!(
            !scene
                .diagnostics
                .iter()
                .any(|d| d.code == "unsupported_object_depth_mask")
        );
    }
}
#[test]
fn source_mask_passes_reach_the_gpu_packet_in_file_order() {
    for (kind, expected) in [(ModelMaskKind::Normal, 3), (ModelMaskKind::Portal, 5)] {
        let mut renderer = WorldRenderer::new(Arc::new(support::masked(kind))).unwrap();
        let (frame, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
        let encoded = serde_json::to_value(frame).unwrap();
        assert_eq!(encoded["schema"], 2);
        let draws: Vec<_> = encoded["draws"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["pipeline"].is_object())
            .collect();
        assert_eq!(draws.len(), expected);
        assert_eq!(draws[0]["pipeline"]["blend"], "no_color");
        assert_eq!(draws[0]["pipeline"]["stencil"]["clockwise"]["pass"], "zero");
        assert_eq!(
            draws[0]["pipeline"]["stencil"]["counterclockwise"]["pass"],
            "replace"
        );
        assert_eq!(draws[1]["pipeline"]["forced_depth"], 1.);
        assert_eq!(draws[2]["pipeline"]["blend"], "non_premultiplied");
        assert!(draws[2]["pick_id"].as_u64().unwrap() > 0);
        for draw in &draws {
            if draw["pipeline"]["blend"] == "no_color" {
                assert_eq!(draw["pick_id"], 0);
                assert!(draw["texture"].is_null());
            }
        }
        if kind == ModelMaskKind::Portal {
            assert_eq!(
                draws[3]["pipeline"]["stencil"]["clockwise"]["compare"],
                "equal"
            );
            assert_eq!(draws[4]["pipeline"]["depth_write"], false);
            assert_eq!(draws[4]["pipeline"]["stencil"]["clockwise"]["pass"], "zero");
        }
    }
}
#[test]
fn masks_count_toward_the_expanded_scene_budget() {
    let document = support::masked(ModelMaskKind::Portal);
    // 16 terrain quads + two body meshes + three mask commands do not fit.
    let budget = SceneBudget {
        max_vertices: 90,
        ..Default::default()
    };
    assert!(build_scene_with_budget(&document, Default::default(), budget).is_err());
}
#[test]
fn masked_model_instances_share_buffers_without_sharing_pick_identities() {
    let mut document = support::masked(ModelMaskKind::Portal);
    let mut second = document.objects[0].clone();
    second.entity.as_mut().unwrap().object_id = 43;
    second.position_tiles.x = 2.5;
    document.objects.push(second);
    let document = Arc::new(document);
    let original = serde_json::to_vec(document.as_ref()).unwrap();
    let scene = build_scene(&document, Default::default()).unwrap();
    let a: Vec<_> = scene.parts.iter().filter(|p| p.object == Some(0)).collect();
    let b: Vec<_> = scene.parts.iter().filter(|p| p.object == Some(1)).collect();
    assert_eq!(a.len(), 5);
    assert_eq!(b.len(), 5);
    for (a, b) in a.iter().zip(&b) {
        assert!(Arc::ptr_eq(&a.mesh, &b.mesh));
    }
    let mut renderer = WorldRenderer::new(Arc::clone(&document)).unwrap();
    let (gpu, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    let generation = gpu.generation.parse().unwrap();
    let targets: Vec<_> = gpu
        .draws
        .iter()
        .filter_map(|draw| renderer.resolve_gpu_pick(generation, draw.pick_id, 128, 96))
        .collect();
    assert!(
        targets.iter().any(
            |p| matches!(p.target,WorldPickTarget::Object{entity:Some(e),..} if e.object_id==42)
        )
    );
    assert!(
        targets.iter().any(
            |p| matches!(p.target,WorldPickTarget::Object{entity:Some(e),..} if e.object_id==43)
        )
    );
    assert_eq!(original, serde_json::to_vec(document.as_ref()).unwrap());
}

#[test]
fn an_object_keeps_one_pick_index_across_its_ordered_material_passes() {
    let mut renderer =
        WorldRenderer::new(Arc::new(support::masked(ModelMaskKind::Portal))).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    let indices: std::collections::BTreeSet<_> = frame
        .draws
        .iter()
        .filter(|draw| draw.pipeline.is_some() && draw.pick_id != 0)
        .map(|draw| draw.pick_id)
        .collect();
    assert_eq!(
        indices.len(),
        1,
        "a portal's final body must resolve the same frame-local object index"
    );
}

#[test]
fn portal_final_group_is_visibly_clipped_even_when_its_dynamic_bit_is_off() {
    let mut normal = WorldRenderer::new(Arc::new(support::masked(ModelMaskKind::Normal))).unwrap();
    let portal_document = Arc::new(support::masked(ModelMaskKind::Portal));
    assert_eq!(portal_document.objects[0].dynamic_flags, [0, 0]);
    let mut portal = WorldRenderer::new(Arc::clone(&portal_document)).unwrap();
    normal.render(Default::default(), 256, 192).unwrap();
    portal.render(Default::default(), 256, 192).unwrap();
    let blue = |p: &[u8; 4]| p[2] > p[0].saturating_mul(2) && p[2] > 100;
    assert_eq!(
        normal
            .image()
            .unwrap()
            .pixels
            .iter()
            .filter(|p| blue(p))
            .count(),
        0
    );
    let points: Vec<_> = portal
        .image()
        .unwrap()
        .pixels
        .iter()
        .enumerate()
        .filter(|(_, p)| blue(p))
        .map(|(index, _)| (index % 256, index / 256))
        .collect();
    assert!(
        points.len() >= 20,
        "portal regression must exercise actual final-group pixels, not an empty stencil"
    );
    for (x, y) in &points {
        assert!(matches!(portal.pick(*x as u32,*y as u32).unwrap().target,
            WorldPickTarget::Object {entity: Some(e),..} if e.object_id == 42));
    }
    let original = serde_json::to_vec(portal_document.as_ref()).unwrap();
    let mut no_mask = portal_document.as_ref().clone();
    no_mask.models[0].depth_mask = None;
    no_mask.objects[0].dynamic_flags[0] = 1 << 1;
    let mut unmasked = WorldRenderer::new(Arc::new(no_mask)).unwrap();
    unmasked.render(Default::default(), 256, 192).unwrap();
    assert!(
        unmasked
            .image()
            .unwrap()
            .pixels
            .iter()
            .filter(|p| blue(p))
            .count()
            > points.len(),
        "portal stencil must exclude part of the final mesh"
    );
    assert_eq!(
        serde_json::to_vec(portal_document.as_ref()).unwrap(),
        original
    );
}
