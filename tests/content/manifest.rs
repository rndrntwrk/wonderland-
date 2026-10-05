use std::collections::{BTreeMap, BTreeSet};
use wonderland_manifest_check::*;

fn resource(label: &str, dependencies: &[&str], critical: bool) -> ResourceRecord {
    ResourceRecord {
        content_hash: Digest::of(label.as_bytes()),
        pack: Digest::of(label.as_bytes()),
        kind: if critical {
            ResourceKind::Semantic
        } else {
            ResourceKind::Visual
        },
        codec: ResourceCodec::IffChunk,
        dependencies: dependencies.iter().map(|s| s.to_string()).collect(),
        simulation_critical: critical,
        locale: None,
        variants: BTreeSet::new(),
        provenance: Provenance {
            origin: Origin::Authored,
            source: format!("fixture/{label}"),
            source_hash: Digest::of(label.as_bytes()),
            patch_hashes: vec![],
            tuning_hash: None,
            license: Some("CC0-1.0".into()),
            redistribution: Redistribution::Allowed,
        },
    }
}
fn fixture() -> AssetManifest {
    let resources: BTreeMap<String, ResourceRecord> = BTreeMap::from([
        ("global".into(), resource("global", &[], true)),
        ("chair".into(), resource("chair", &["global"], true)),
        (
            "chair-image".into(),
            resource("chair-image", &["chair"], false),
        ),
        ("unrelated".into(), resource("unrelated", &[], false)),
    ]);
    let packs = resources
        .iter()
        .map(|(id, r)| {
            (
                r.pack.clone(),
                PackRecord {
                    byte_len: 100,
                    format_version: PACK_VERSION,
                    resources: vec![id.clone()],
                },
            )
        })
        .collect();
    AssetManifest {
        schema_version: MANIFEST_VERSION,
        pack_format_version: PACK_VERSION,
        source_baseline: "4c6b3e8f5835b228723caea3c9f683c62f244f73".into(),
        content_version: Digest::of(&[]),
        tuning_version: Digest::of(b"no-tuning"),
        packs,
        resources,
    }
}

#[test]
fn digest_uses_sha256_and_rejects_noncanonical_input() {
    assert_eq!(
        Digest::of(b"abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    for text in ["../secret", "abc", &"A".repeat(64), &"z".repeat(64)] {
        assert!(Digest::try_from(text.to_string()).is_err());
    }
}

#[test]
fn simulation_plan_loads_exact_transitive_subset_and_deduplicates_cached_packs() {
    let limits = ManifestLimits::default();
    let manifest = fixture().seal(&limits).unwrap();
    let cached = BTreeSet::from([manifest.resources["global"].pack.clone()]);
    let plan = manifest
        .load_plan(
            &["chair-image".into()],
            LoadPhase::Simulation,
            None,
            &cached,
            &limits,
        )
        .unwrap();
    assert_eq!(plan.resources, ["global", "chair"]);
    assert_eq!(plan.packs, [manifest.resources["chair"].pack.clone()]);
    assert_eq!(plan.download_bytes, 100);
    let all = manifest
        .load_plan(
            &["chair-image".into()],
            LoadPhase::All,
            None,
            &BTreeSet::new(),
            &limits,
        )
        .unwrap();
    assert_eq!(all.resources, ["global", "chair", "chair-image"]);
    assert_eq!(all.download_bytes, 300);
    assert!(!all.resources.iter().any(|r| r == "unrelated"));
}

#[test]
fn missing_dependencies_cycles_and_noncritical_tick_inputs_fail_closed() {
    let limits = ManifestLimits::default();
    let mut missing = fixture();
    missing
        .resources
        .get_mut("chair")
        .unwrap()
        .dependencies
        .push("missing".into());
    assert!(missing.seal(&limits).is_err());
    let mut cycle = fixture();
    cycle
        .resources
        .get_mut("global")
        .unwrap()
        .dependencies
        .push("chair".into());
    assert!(cycle.seal(&limits).is_err());
    let mut dependency = fixture();
    dependency
        .resources
        .get_mut("chair")
        .unwrap()
        .dependencies
        .push("unrelated".into());
    assert!(dependency.seal(&limits).is_err());
}

#[test]
fn reverse_pack_index_digest_and_schema_are_verified() {
    let limits = ManifestLimits::default();
    let mut wrong_index = fixture();
    wrong_index
        .packs
        .values_mut()
        .next()
        .unwrap()
        .resources
        .push("chair".into());
    assert!(wrong_index.seal(&limits).is_err());
    let mut changed = fixture().seal(&limits).unwrap();
    changed.resources.get_mut("chair").unwrap().content_hash = Digest::of(b"new");
    assert!(changed.validate(&limits).is_err());
    let mut unknown = fixture();
    unknown.schema_version = 17;
    assert!(unknown.seal(&limits).is_err());
}

#[test]
fn canonicalization_ignores_dependency_enumeration_but_preserves_patch_order() {
    let limits = ManifestLimits::default();
    let mut first = fixture();
    first.resources.get_mut("chair-image").unwrap().dependencies =
        vec!["global".into(), "chair".into()];
    let mut reordered = first.clone();
    reordered
        .resources
        .get_mut("chair-image")
        .unwrap()
        .dependencies
        .reverse();
    assert_eq!(
        first
            .seal(&limits)
            .unwrap()
            .canonical_bytes(&limits)
            .unwrap(),
        reordered
            .seal(&limits)
            .unwrap()
            .canonical_bytes(&limits)
            .unwrap()
    );
    let mut patched = fixture();
    patched
        .resources
        .get_mut("chair")
        .unwrap()
        .provenance
        .patch_hashes = vec![Digest::of(b"a"), Digest::of(b"b")];
    let mut reverse = patched.clone();
    reverse
        .resources
        .get_mut("chair")
        .unwrap()
        .provenance
        .patch_hashes
        .reverse();
    let one = patched.seal(&limits).unwrap();
    let two = reverse.seal(&limits).unwrap();
    assert_ne!(one.content_version, two.content_version);
    assert_ne!(
        one.derived_cache_key("chair-image", "mesh-v1", "full3d", &limits)
            .unwrap(),
        two.derived_cache_key("chair-image", "mesh-v1", "full3d", &limits)
            .unwrap()
    );
}

#[test]
fn limits_variants_and_distribution_rights_are_enforced() {
    let limits = ManifestLimits::default();
    let manifest = fixture().seal(&limits).unwrap();
    let small = ManifestLimits {
        max_download_bytes: 99,
        ..limits.clone()
    };
    assert!(manifest
        .load_plan(
            &["chair".into()],
            LoadPhase::All,
            None,
            &BTreeSet::new(),
            &small
        )
        .is_err());
    let mut variant = fixture();
    variant
        .resources
        .get_mut("chair-image")
        .unwrap()
        .variants
        .insert("full2d".into());
    assert!(variant
        .seal(&limits)
        .unwrap()
        .load_plan(
            &["chair-image".into()],
            LoadPhase::All,
            Some("full3d"),
            &BTreeSet::new(),
            &limits
        )
        .is_err());
    let tiny = ManifestLimits {
        max_manifest_bytes: 2,
        ..limits.clone()
    };
    assert!(AssetManifest::from_json(&manifest.canonical_bytes(&limits).unwrap(), &tiny).is_err());
    let mut restricted = fixture();
    restricted
        .resources
        .get_mut("chair")
        .unwrap()
        .provenance
        .redistribution = Redistribution::Unknown;
    assert!(restricted
        .seal(&limits)
        .unwrap()
        .require_public_distribution()
        .is_err());
    assert!(manifest.require_public_distribution().is_ok());
}

#[test]
fn duplicate_dependencies_and_path_like_resource_ids_are_rejected() {
    let limits = ManifestLimits::default();
    let mut duplicate = fixture();
    duplicate
        .resources
        .get_mut("chair")
        .unwrap()
        .dependencies
        .push("global".into());
    assert!(duplicate.seal(&limits).is_err());
    let mut escape = fixture();
    let old = escape.resources.remove("chair").unwrap();
    escape.resources.insert("../chair".into(), old);
    assert!(escape.seal(&limits).is_err());
}
