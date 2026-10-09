use std::sync::Arc;
use wonderland_world_view::*;

fn fixture() -> Arc<WorldDocument> {
    let mut document = WorldDocument::from_blueprint_xml(
        "<house><size>4</size><world><floors><floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/></floors><walls/></world><objects/></house>",
        "tests:GPU geometry", "gpu-v1",
    ).unwrap();
    document.provenance.kind = WorldSourceKind::TestFixture;
    document.source_counts = None;
    Arc::new(document)
}

#[test]
fn gpu_frame_prepares_source_geometry_without_a_software_raster() {
    let mut renderer = WorldRenderer::new(fixture()).unwrap();
    let (frame, stats) = renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    assert!(renderer.image().is_none());
    assert!(stats.triangles > 0);
    assert_eq!(frame.schema, 2);
    assert_eq!(frame.draws.len(), stats.parts);
    assert!(frame.meshes.iter().all(|mesh| mesh.vertices.len() % 9 == 0));
    assert!(frame.draws.iter().any(|draw| draw.pick_id > 0));
    let json = serde_json::to_value(&frame).unwrap();
    assert!(json["generation"].is_string());
    assert!(json["draws"][0]["matrix"].as_array().unwrap().len() == 16);
}

#[test]
fn gpu_results_are_frame_scoped_bounded_and_invalidated_on_redraw_or_loss() {
    let mut renderer = WorldRenderer::new(fixture()).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    let generation = frame.generation.parse().unwrap();
    let id = frame
        .draws
        .iter()
        .find(|draw| draw.pick_id > 0)
        .unwrap()
        .pick_id;
    let pick = renderer.resolve_gpu_pick(generation, id, 64, 48).unwrap();
    assert!(matches!(
        pick.target,
        WorldPickTarget::Tile { level: 1, .. }
    ));
    assert!(renderer.resolve_gpu_pick(generation, 0, 64, 48).is_none());
    assert!(
        renderer
            .resolve_gpu_pick(generation, u32::MAX, 64, 48)
            .is_none()
    );
    assert!(renderer.resolve_gpu_pick(generation, id, 128, 48).is_none());
    renderer.prepare_gpu(Default::default(), 256, 192).unwrap();
    assert!(renderer.resolve_gpu_pick(generation, id, 64, 48).is_none());
    let (frame, _) = renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    let generation = frame.generation.parse().unwrap();
    renderer.device_reset();
    assert!(renderer.resolve_gpu_pick(generation, id, 64, 48).is_none());
}

#[test]
fn reference_raster_and_gpu_frames_cannot_reuse_each_others_hits() {
    let mut renderer = WorldRenderer::new(fixture()).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    let generation = frame.generation.parse().unwrap();
    renderer.render(Default::default(), 128, 96).unwrap();
    assert!(renderer.resolve_gpu_pick(generation, 1, 64, 48).is_none());
    let cpu_pick = (0..96)
        .flat_map(|y| (0..128).map(move |x| (x, y)))
        .find_map(|(x, y)| renderer.pick(x, y))
        .unwrap();
    renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    assert!(renderer.resolve_pick(&cpu_pick).is_none());
}

#[test]
fn gpu_surface_limits_fail_without_admitting_a_partial_frame() {
    let mut renderer = WorldRenderer::new(fixture()).unwrap();
    for (width, height) in [(0, 96), (128, 0), (4097, 1), (1025, 1024)] {
        assert!(
            renderer
                .prepare_gpu(Default::default(), width, height)
                .is_err()
        );
    }
    let (frame, _) = renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    assert_eq!(frame.generation, "1");
}
