use std::collections::{BTreeMap, BTreeSet};
use wonderland_asset_cooker::packs::*;
use wonderland_content_ir::manifest::*;

fn input() -> Vec<PackResource> {
    vec![
        PackResource {
            id: "b".into(),
            kind: ResourceKind::Visual,
            bytes: vec![4, 5],
        },
        PackResource {
            id: "a".into(),
            kind: ResourceKind::Semantic,
            bytes: vec![1, 2, 3],
        },
    ]
}
#[test]
fn repeat_build_is_deterministic_and_index_extracts_exact_resources() {
    let limits = PackLimits::default();
    let one = build_pack(&input(), &limits).unwrap();
    let mut reverse = input();
    reverse.reverse();
    assert_eq!(one, build_pack(&reverse, &limits).unwrap());
    assert_eq!(&one[..8], b"WLDPACK\0");
    assert_eq!(&one[8..16], &[1, 0, 0, 0, 2, 0, 0, 0]);
    // Each fixed index entry is 52 bytes plus its one-byte identifier.
    assert_eq!(&one[16..24], &106u64.to_le_bytes());
    assert_eq!(&one[24..32], &5u64.to_le_bytes());
    let index = verify_pack(&one, &Digest::of(&one), &limits).unwrap();
    assert_eq!(index.get("a").unwrap(), [1, 2, 3]);
    assert_eq!(index.get("b").unwrap(), [4, 5]);
    assert!(index.get("missing").is_err());
}

#[test]
fn rejects_wrong_hash_and_all_truncated_prefixes() {
    let limits = PackLimits::default();
    assert!(verify_pack(b"invalid", &Digest::of(b"invalid"), &limits).is_err());
    let bytes = build_pack(&input(), &limits).unwrap();
    assert!(verify_pack(&bytes, &Digest::of(b"wrong release"), &limits).is_err());
    for prefix in 0..bytes.len() {
        let candidate = &bytes[..prefix];
        assert!(
            verify_pack(candidate, &Digest::of(candidate), &limits).is_err(),
            "prefix {prefix}"
        );
    }
}

#[test]
fn malicious_overlaps_counts_reserved_flags_and_payload_edits_fail_even_with_rehashed_outer_pack() {
    let limits = PackLimits::default();
    let valid = build_pack(&input(), &limits).unwrap();
    for (offset, patch) in [
        (12, u32::MAX.to_le_bytes().to_vec()),
        (36, vec![1]),
        (32 + 53 + 37, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut bad = valid.clone();
        bad[offset..offset + patch.len()].copy_from_slice(&patch);
        assert!(
            verify_pack(&bad, &Digest::of(&bad), &limits).is_err(),
            "offset {offset}"
        );
    }
    let mut bad_payload = valid.clone();
    *bad_payload.last_mut().unwrap() ^= 1;
    assert!(verify_pack(&bad_payload, &Digest::of(&bad_payload), &limits).is_err());
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(verify_pack(&trailing, &Digest::of(&trailing), &limits).is_err());
}

#[test]
fn duplicate_ids_unsafe_ids_and_allocation_limits_rejected_before_pack_creation() {
    let limits = PackLimits::default();
    let mut duplicates = input();
    duplicates[1].id = "b".into();
    assert!(build_pack(&duplicates, &limits).is_err());
    let mut traversal = input();
    traversal[0].id = "../../secret".into();
    assert!(build_pack(&traversal, &limits).is_err());
    let small = PackLimits {
        max_resource_bytes: 2,
        ..limits.clone()
    };
    assert!(build_pack(&input(), &small).is_err());
    let small = PackLimits {
        max_pack_bytes: 100,
        ..limits.clone()
    };
    assert!(build_pack(&input(), &small).is_err());
    assert!(build_pack(&[], &limits).is_err());
}

#[test]
fn verified_pack_is_checked_against_manifest_members_and_payload_hashes() {
    let limits = PackLimits::default();
    let bytes = build_pack(&input(), &limits).unwrap();
    let hash = Digest::of(&bytes);
    let mut resources = BTreeMap::new();
    for entry in input() {
        resources.insert(
            entry.id.clone(),
            ResourceRecord {
                content_hash: Digest::of(&entry.bytes),
                pack: hash.clone(),
                kind: entry.kind,
                codec: ResourceCodec::IffChunk,
                dependencies: vec![],
                simulation_critical: entry.kind == ResourceKind::Semantic,
                locale: None,
                variants: BTreeSet::new(),
                provenance: Provenance {
                    origin: Origin::Authored,
                    source: format!("fixture/{}", entry.id),
                    source_hash: Digest::of(&entry.bytes),
                    patch_hashes: vec![],
                    tuning_hash: None,
                    license: Some("CC0-1.0".into()),
                    redistribution: Redistribution::Allowed,
                },
            },
        );
    }
    let manifest_limits = ManifestLimits::default();
    let manifest = AssetManifest {
        schema_version: 1,
        pack_format_version: 1,
        source_baseline: "4c6b3e8f5835b228723caea3c9f683c62f244f73".into(),
        content_version: Digest::of(&[]),
        tuning_version: Digest::of(&[]),
        packs: BTreeMap::from([(
            hash.clone(),
            PackRecord {
                byte_len: bytes.len() as u64,
                format_version: 1,
                resources: vec!["a".into(), "b".into()],
            },
        )]),
        resources,
    }
    .seal(&manifest_limits)
    .unwrap();
    let index = verify_pack(&bytes, &hash, &limits).unwrap();
    index
        .validate_manifest(&manifest, &manifest_limits)
        .unwrap();
    let mut wrong = manifest.clone();
    wrong.resources.get_mut("a").unwrap().content_hash = Digest::of(b"wrong");
    let wrong = wrong.seal(&manifest_limits).unwrap();
    assert!(index.validate_manifest(&wrong, &manifest_limits).is_err());
}
