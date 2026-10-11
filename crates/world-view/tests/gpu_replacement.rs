use std::sync::Arc;
use wonderland_world_view::*;

fn fixture(origin: &str) -> Arc<WorldDocument> {
    let mut document = WorldDocument::from_blueprint_xml(
        "<house><size>4</size><world><floors><floor level=\"0\" x=\"1\" y=\"1\" value=\"9\"/></floors><walls/></world><objects/></house>",
        origin, "gpu-replacement-v1",
    ).unwrap();
    document.provenance.kind = WorldSourceKind::TestFixture;
    document.source_counts = None;
    Arc::new(document)
}
fn admitted() -> (WorldRenderer, u64, u32) {
    let mut renderer = WorldRenderer::new(fixture("first")).unwrap();
    let (frame, _) = renderer.prepare_gpu(Default::default(), 128, 96).unwrap();
    let id = frame.draws.iter().find(|d| d.pick_id > 0).unwrap().pick_id;
    (renderer, frame.generation.parse().unwrap(), id)
}
#[test]
fn failed_graphics_publication_retains_previous_frame_and_pick_ticket() {
    let (mut renderer, generation, id) = admitted();
    let before = renderer.resolve_gpu_pick(generation, id, 64, 48).unwrap();
    let mut attempts = 0;
    let result = renderer.update_gpu(fixture("second"), Default::default(), 128, 96, |_| {
        attempts += 1;
        Err(WorldError("device allocation refused candidate".into()))
    });
    assert_eq!(attempts, 1);
    assert!(result.unwrap_err().to_string().contains("allocation"));
    assert_eq!(
        renderer.resolve_gpu_pick(generation, id, 64, 48),
        Some(before)
    );
}
#[test]
fn failed_preparation_never_calls_graphics_or_discards_admitted_frame() {
    let (mut renderer, generation, id) = admitted();
    let before = renderer.resolve_gpu_pick(generation, id, 64, 48).unwrap();
    for (width, height) in [(0, 96), (4097, 1), (1025, 1024)] {
        let result =
            renderer.update_gpu(fixture("second"), Default::default(), width, height, |_| {
                panic!("invalid preparation reached the device")
            });
        assert!(result.is_err());
        assert_eq!(
            renderer.resolve_gpu_pick(generation, id, 64, 48),
            Some(before.clone())
        );
    }
}
#[test]
fn only_successful_device_publication_commits_new_identity() {
    let (mut renderer, generation, id) = admitted();
    let mut next_id = 0;
    let (next, stats) = renderer
        .update_gpu(fixture("second"), Default::default(), 256, 192, |frame| {
            next_id = frame.draws.iter().find(|d| d.pick_id > 0).unwrap().pick_id;
            Ok(())
        })
        .unwrap();
    assert!(next.parse::<u64>().unwrap() > generation);
    assert_eq!((stats.width, stats.height), (256, 192));
    assert!(renderer.resolve_gpu_pick(generation, id, 64, 48).is_none());
    assert!(
        renderer
            .resolve_gpu_pick(next.parse().unwrap(), next_id, 128, 96)
            .is_some()
    );
}
#[test]
fn failed_gpu_candidate_keeps_the_entire_cpu_raster_and_selection() {
    let mut renderer = WorldRenderer::new(fixture("first")).unwrap();
    renderer.render(Default::default(), 128, 96).unwrap();
    let before = renderer.image().unwrap().clone();
    let pick = (0..96)
        .flat_map(|y| (0..128).map(move |x| (x, y)))
        .find_map(|(x, y)| renderer.pick(x, y))
        .unwrap();
    assert!(
        renderer
            .update_gpu(fixture("second"), Default::default(), 128, 96, |_| {
                Err(WorldError("draw failed".into()))
            })
            .is_err()
    );
    assert_eq!(renderer.image(), Some(&before));
    assert!(renderer.resolve_pick(&pick).is_some());
}

#[path = "../examples/support/masked.rs"]
mod mask_fixture;
#[path = "support/replacement.rs"]
mod replacement;
#[test]
fn schema_valid_repeated_models_fail_expansion_without_retiring_previous_world() {
    let (mut renderer, generation, id) = admitted();
    let before = renderer.resolve_gpu_pick(generation, id, 64, 48).unwrap();
    let document = Arc::new(replacement::overbudget());
    document.validate().unwrap();
    let error = renderer
        .update_gpu(document, Default::default(), 128, 96, |_| {
            panic!("expanded scene refusal must happen before device allocation")
        })
        .unwrap_err();
    assert!(
        error.to_string().contains("expanded world scene budget"),
        "{error}"
    );
    assert_eq!(
        renderer.resolve_gpu_pick(generation, id, 64, 48),
        Some(before)
    );
}
