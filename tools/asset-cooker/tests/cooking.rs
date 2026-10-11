use std::collections::BTreeSet;
use wonderland_asset_cooker::manifest::*;
use wonderland_asset_cooker::packs::verify_pack;
use wonderland_content_ir::manifest::*;

const BASE: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
fn prepared(id: &str) -> PreparedResource {
    let mut payload = vec![0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    payload[..magic.len()].copy_from_slice(magic);
    payload.extend_from_slice(b"BCON");
    payload.extend_from_slice(&80u32.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes());
    payload.extend_from_slice(&0u16.to_be_bytes());
    payload.extend_from_slice(&[0; 64]);
    payload.extend_from_slice(&[1, 0, 7, 0]);
    PreparedResource {
        id: id.into(),
        group: id.into(),
        kind: ResourceKind::Semantic,
        codec: ResourceCodec::IffChunk,
        payload,
        dependencies: vec![],
        simulation_critical: true,
        locale: None,
        variants: BTreeSet::new(),
        provenance: Provenance {
            origin: Origin::Authored,
            source: format!("fixture/{id}"),
            source_hash: Digest::of(b"source"),
            patch_hashes: vec![],
            tuning_hash: None,
            license: Some("CC0-1.0".into()),
            redistribution: Redistribution::Allowed,
        },
    }
}

#[test]
fn cooking_rebuilds_identically_and_readiness_excludes_unrelated_group() {
    let limits = CookLimits::default();
    let one = cook_resources(
        BASE,
        Digest::of(b"tuning"),
        &[prepared("chair"), prepared("bed")],
        &limits,
    )
    .unwrap();
    let two = cook_resources(
        BASE,
        Digest::of(b"tuning"),
        &[prepared("bed"), prepared("chair")],
        &limits,
    )
    .unwrap();
    assert_eq!(one.packs, two.packs);
    assert_eq!(
        one.manifest.canonical_bytes(&limits.manifest).unwrap(),
        two.manifest.canonical_bytes(&limits.manifest).unwrap()
    );
    assert_eq!(one.packs.len(), 2);
    let plan = one
        .manifest
        .load_plan(
            &["chair".into()],
            LoadPhase::Simulation,
            None,
            &BTreeSet::new(),
            &limits.manifest,
        )
        .unwrap();
    assert_eq!(plan.resources, ["chair"]);
    assert_eq!(plan.packs.len(), 1);
    for (hash, pack) in &one.packs {
        verify_pack(pack, hash, &limits.pack)
            .unwrap()
            .validate_manifest(&one.manifest, &limits.manifest)
            .unwrap();
    }
}

#[test]
fn pack_splitting_is_bounded_and_patch_or_tuning_changes_cache_identity() {
    let mut limits = CookLimits::default();
    limits.pack.max_pack_bytes = 300;
    let mut a = prepared("a");
    a.group = "shared".into();
    let mut b = prepared("b");
    b.group = "shared".into();
    let one = cook_resources(BASE, Digest::of(b"t1"), &[a.clone(), b.clone()], &limits).unwrap();
    assert_eq!(one.packs.len(), 2);
    assert!(one.packs.values().all(|p| p.len() <= 300));
    a.provenance.patch_hashes.push(Digest::of(b"new patch"));
    let changed = cook_resources(BASE, Digest::of(b"t1"), &[a, b], &limits).unwrap();
    assert_ne!(
        one.manifest.content_version,
        changed.manifest.content_version
    );
    assert_ne!(
        one.manifest
            .derived_cache_key("a", "mesh-v1", "full3d", &limits.manifest)
            .unwrap(),
        changed
            .manifest
            .derived_cache_key("a", "mesh-v1", "full3d", &limits.manifest)
            .unwrap()
    );
    let tuning = cook_resources(BASE, Digest::of(b"t2"), &[prepared("a")], &limits).unwrap();
    assert_ne!(
        one.manifest.content_version,
        tuning.manifest.content_version
    );
}

#[test]
fn failed_closure_or_duplicate_resource_does_not_return_a_partial_release() {
    let limits = CookLimits::default();
    let mut bad = prepared("a");
    bad.dependencies.push("missing".into());
    assert!(cook_resources(BASE, Digest::of(&[]), &[bad], &limits).is_err());
    assert!(cook_resources(
        BASE,
        Digest::of(&[]),
        &[prepared("a"), prepared("a")],
        &limits
    )
    .is_err());
    let mut raw = prepared("raw");
    raw.codec = ResourceCodec::Opaque;
    assert!(cook_resources(BASE, Digest::of(&[]), &[raw], &limits).is_err());
}

#[test]
fn audio_members_validate_pcm_frames_and_stay_out_of_simulation_readiness() {
    // Literal mono PCM16 RIFF: 22,050 Hz, two frames. No encoder used.
    let wave = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0data\x04\0\0\0\x01\0\xff\x7f";
    let limits = CookLimits::default();
    let mut audio = prepared("audio/chime");
    audio.kind = ResourceKind::Audio;
    audio.codec = ResourceCodec::PcmWave;
    audio.simulation_critical = false;
    audio.payload = wave.to_vec();
    let cooked = cook_resources(BASE, Digest::of(b"tuning"), &[audio.clone()], &limits).unwrap();
    let simulation = cooked
        .manifest
        .load_plan(
            &[audio.id.clone()],
            LoadPhase::Simulation,
            None,
            &BTreeSet::new(),
            &limits.manifest,
        )
        .unwrap();
    assert!(simulation.resources.is_empty());
    let all = cooked
        .manifest
        .load_plan(
            &[audio.id.clone()],
            LoadPhase::All,
            None,
            &BTreeSet::new(),
            &limits.manifest,
        )
        .unwrap();
    assert_eq!(all.resources, [audio.id.clone()]);
    audio.payload[32] = 1; // block alignment contradicts PCM16 mono.
    assert!(cook_resources(BASE, Digest::of(b"tuning"), &[audio.clone()], &limits).is_err());
    audio.payload = wave.to_vec();
    audio.kind = ResourceKind::Semantic;
    assert!(cook_resources(BASE, Digest::of(b"tuning"), &[audio], &limits).is_err());
}
