use std::sync::Arc;
use wonderland_world_view::*;
#[path = "support/lighting.rs"]
mod lighting;

fn finish(world: WorldDocument) -> WorldFacadeOutput {
    let mut job = WorldFacadeJob::new(Arc::new(world), FacadeExportOptions::default()).unwrap();
    for _ in 0..20_000 {
        if let Some(result) = job.step(8).unwrap() {
            return result;
        }
    }
    panic!("facade did not finish within its work budget");
}
#[test]
fn loaded_lit_world_produces_deterministic_source_fsof_without_mutation() {
    let world = lighting::lit_world();
    let before = world.clone();
    let a = finish(world.clone());
    let b = finish(world.clone());
    assert_eq!(a.bytes, b.bytes);
    assert_eq!(a.metadata_json, b.metadata_json);
    assert!(a.bytes.starts_with(b"FSOf"));
    assert_eq!(world, before);
    let decoded =
        wonderland_render_core::derivatives::fsof::Fsof::decode(&a.bytes, Default::default())
            .unwrap();
    assert!(
        decoded.night.is_none(),
        "one supplied light state must not invent night"
    );
    assert!(!decoded.geometry.floor.indices.is_empty());
    assert!(
        decoded
            .floor_texture
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0)
    );
    let metadata: serde_json::Value = serde_json::from_str(&a.metadata_json).unwrap();
    assert_eq!(metadata["revision"]["tick"], "1");
    assert_eq!(metadata["kind"], "presentation_facade");
}
#[test]
fn shadow_changes_facade_pixels_and_identity_but_not_geometry() {
    let mut world = lighting::lit_world();
    let shadow = finish(world.clone());
    world.lighting.as_mut().unwrap().geometry.clear();
    world.lighting.as_mut().unwrap().revision += 1;
    let lit = finish(world);
    assert_ne!(shadow.bytes, lit.bytes);
    assert_ne!(shadow.source_hash, lit.source_hash);
    let decode = |b: &[u8]| {
        wonderland_render_core::derivatives::fsof::Fsof::decode(b, Default::default()).unwrap()
    };
    let (a, b) = (decode(&shadow.bytes), decode(&lit.bytes));
    assert_eq!(a.geometry, b.geometry);
    assert_ne!(a.floor_texture, b.floor_texture);
}
#[test]
fn cancelled_job_is_terminal_and_never_yields_an_artifact() {
    let mut job = WorldFacadeJob::new(Arc::new(lighting::lit_world()), Default::default()).unwrap();
    assert!(job.step(1).unwrap().is_none());
    job.cancel();
    assert!(job.step(1).is_err());
    assert!(job.step(32).is_err());
}
#[test]
fn malformed_source_and_unbounded_requests_fail_before_export() {
    let mut world = lighting::lit_world();
    world.lighting.as_mut().unwrap().geometry[0].walls[0][0].x = f32::NAN;
    assert!(WorldFacadeJob::new(Arc::new(world), Default::default()).is_err());
    let world = Arc::new(lighting::lit_world());
    for resolution in [0, 9, u16::MAX] {
        assert!(
            WorldFacadeJob::new(
                world.clone(),
                FacadeExportOptions {
                    pixels_per_tile: resolution
                }
            )
            .is_err()
        );
    }
}

#[path = "../examples/support/masked.rs"]
mod masked;
#[test]
fn original_mask_commands_reach_facades_without_inventing_a_hidden_group() {
    use wonderland_render_core::derivatives::fsof::Fsof;
    let decode = |mut world: WorldDocument| {
        // A facade's top-down floor cells cannot see the open vertical portal
        // sheet. Supply an exterior wall so its side-view atlas observes it.
        world.objects[0].position_tiles = wonderland_render_core::Vec3::new(1., 0.5, 0.);
        let tile = &mut world.lot.tiles[4 + 1];
        tile.wall.north = true;
        tile.wall.styles[1] = 1;
        Fsof::decode(&finish(world).bytes, Default::default()).unwrap()
    };
    let normal = decode(masked::masked(ModelMaskKind::Normal));
    let portal = decode(masked::masked(ModelMaskKind::Portal));
    let blue = |f: &Fsof| {
        f.floor_texture
            .as_chunks::<4>()
            .0
            .iter()
            .chain(f.wall_texture.as_chunks::<4>().0.iter())
            .filter(|p| {
                p[3] > 0 && p[2] > p[0].saturating_add(20) && p[2] > p[1].saturating_add(20)
            })
            .count()
    };
    let red = |f: &Fsof| {
        f.floor_texture
            .as_chunks::<4>()
            .0
            .iter()
            .chain(f.wall_texture.as_chunks::<4>().0.iter())
            .filter(|p| {
                p[3] > 0 && p[0] > p[1].saturating_add(20) && p[0] > p[2].saturating_add(20)
            })
            .count()
    };
    assert!(
        red(&normal) > 0 && red(&portal) > 0,
        "empty render cannot pass mask acceptance"
    );
    assert_eq!(
        blue(&normal),
        0,
        "normal mask must honor its disabled final group"
    );
    assert!(
        blue(&portal) > 0,
        "portal's forced final group must actually render"
    );
    let mut unmasked = masked::masked(ModelMaskKind::Portal);
    unmasked.models[0].depth_mask = None;
    unmasked.objects[0].dynamic_flags = [u64::MAX; 2];
    let plain = decode(unmasked);
    assert!(
        blue(&plain) > blue(&portal),
        "portal stencil must restrict visible final pixels"
    );
}

#[test]
fn export_budget_and_cadence_never_change_a_live_gpu_ticket() {
    let world = Arc::new(masked::masked(ModelMaskKind::Normal));
    let mut renderer = WorldRenderer::new(world.clone()).unwrap();
    let (packet, _) = renderer.prepare_gpu(Default::default(), 160, 120).unwrap();
    let id = packet.draws.iter().find(|d| d.pick_id > 0).unwrap().pick_id;
    let generation = packet.generation.parse().unwrap();
    let before = renderer.resolve_gpu_pick(generation, id, 50, 50).unwrap();
    let mut slow = WorldFacadeJob::new(world.clone(), Default::default()).unwrap();
    assert!(slow.step(0).is_err());
    assert!(slow.step(129).is_err());
    let first = loop {
        if let Some(output) = slow.step(1).unwrap() {
            break output;
        }
    };
    assert!(slow.step(1).is_err(), "completed export is terminal");
    let mut fast = WorldFacadeJob::new(world, Default::default()).unwrap();
    let second = loop {
        if let Some(output) = fast.step(128).unwrap() {
            break output;
        }
    };
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.metadata_json, second.metadata_json);
    assert_eq!(
        renderer.resolve_gpu_pick(generation, id, 50, 50),
        Some(before)
    );
}

#[test]
fn excessive_individual_meshes_are_refused_before_cooperative_rasterization() {
    let mut world = masked::masked(ModelMaskKind::Normal);
    world.models[0].groups[0][0].mesh.indices = [0, 1, 2].repeat(8193);
    let error = WorldFacadeJob::new(Arc::new(world), Default::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("cooperative draw budget"));
}

#[test]
fn source_metadata_has_a_budget_before_any_atlas_is_created() {
    let mut world = lighting::lit_world();
    world.provenance.origin = "x".repeat(4097);
    let error = WorldFacadeJob::new(Arc::new(world), Default::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("provenance byte budget"));
}
