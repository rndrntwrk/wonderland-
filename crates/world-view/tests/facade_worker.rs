use std::sync::Arc;
use wonderland_world_view::*;
#[path = "support/lighting.rs"]
mod lighting;
#[test]
fn worker_executes_the_same_validated_facade_not_a_second_renderer() {
    let world = lighting::lit_world();
    let bytes = encode_facade_worker_request(&world, Default::default()).unwrap();
    let actual = execute_facade_worker_request(&bytes).unwrap();
    let mut job = WorldFacadeJob::new(Arc::new(world), Default::default()).unwrap();
    let expected = loop {
        if let Some(output) = job.step(8).unwrap() {
            break output;
        }
    };
    assert_eq!(actual.bytes, expected.bytes);
    assert_eq!(actual.metadata_json, expected.metadata_json);
    assert_eq!(actual.source_hash, expected.source_hash);
}
#[test]
fn worker_rejects_untrusted_envelopes_and_invalid_geometry() {
    let bytes = encode_facade_worker_request(&lighting::lit_world(), Default::default()).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for case in 0..3 {
        let mut bad = value.clone();
        match case {
            0 => bad["schema"] = 2.into(),
            1 => bad["extra"] = true.into(),
            _ => bad["options"]["pixels_per_tile"] = 0.into(),
        }
        assert!(execute_facade_worker_request(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    assert!(execute_facade_worker_request(&bytes[..bytes.len() / 2]).is_err());
    let mut world = lighting::lit_world();
    world.lot.terrain.corners.clear();
    let request = encode_facade_worker_request(&world, Default::default()).unwrap();
    assert!(execute_facade_worker_request(&request).is_err());
}
#[test]
fn worker_has_bounded_source_transfer_in_both_directions() {
    assert!(execute_facade_worker_request(&[]).is_err());
    assert!(execute_facade_worker_request(&vec![b' '; 32 * 1024 * 1024 + 1]).is_err());
    let mut world = lighting::lit_world();
    world.provenance.origin = "x".repeat(32 * 1024 * 1024);
    assert!(encode_facade_worker_request(&world, Default::default()).is_err());
}
