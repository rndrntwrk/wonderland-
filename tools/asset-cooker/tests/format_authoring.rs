// SPDX-License-Identifier: MPL-2.0
use std::collections::BTreeSet;
use wonderland_asset_cooker::{manifest::*, registry::SourceFormat};
use wonderland_content_ir::manifest::*;

fn resource(codec: ResourceCodec, payload: Vec<u8>) -> PreparedResource {
    PreparedResource {
        id: "fixture/visual".into(),
        group: "fixture".into(),
        kind: ResourceKind::Visual,
        codec,
        payload,
        dependencies: vec![],
        simulation_critical: false,
        locale: None,
        variants: BTreeSet::new(),
        provenance: Provenance {
            origin: Origin::Authored,
            source: "independent format vector".into(),
            source_hash: Digest::of(b"format vector"),
            patch_hashes: vec![],
            tuning_hash: None,
            license: Some("CC0-1.0".into()),
            redistribution: Redistribution::Allowed,
        },
    }
}
fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn vectors() -> Vec<(SourceFormat, ResourceCodec, Vec<u8>)> {
    vec![
        (SourceFormat::VitaboyBcf, ResourceCodec::VitaboyBcf, vec![0; 12]),
        (SourceFormat::VitaboyCmx, ResourceCodec::VitaboyCmx, b"version 300\n0\n0\n0\n".to_vec()),
        (SourceFormat::VitaboyBmf, ResourceCodec::VitaboyBmf, vec![0; 26]),
        (SourceFormat::VitaboySkn, ResourceCodec::VitaboySkn, b"skin\ntexture\n0\n0\n0\n0\n0\n0\n".to_vec()),
        (SourceFormat::VitaboyPurchasableOutfit, ResourceCodec::VitaboyPurchasableOutfit, hex("000000070000000100000008deadbeef0102030405060708")),
        (SourceFormat::VitaboyHandGroup, ResourceCodec::VitaboyHandGroup, vec![0; 148]),
        (SourceFormat::VitaboyCollection, ResourceCodec::VitaboyCollection, vec![0; 4]),
        (SourceFormat::Fsom, ResourceCodec::Fsom, hex("1f8b0800000000000203730bf6cf656680009692d4e2122620831188dd19feff87490041030356d0600fc1c87c747974715436c80e26288f118b05e8f6000052d6f8bdb1000000")),
        (SourceFormat::Nbhm, ResourceCodec::Nbhm, b"NBHm\x01\0\0\0\0\0\0\0\0".to_vec()),
        (SourceFormat::PngTexture, ResourceCodec::PngTexture, hex("89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000001149444154789c63e051b260880a70fb0f00068a02568c6c76be0000000049454e44ae426082")),
    ]
}

#[test]
fn new_codecs_decode_actual_payloads_and_cannot_assert_simulation_readiness() {
    let limits = CookLimits::default();
    for (_, codec, bytes) in vectors() {
        let value = resource(codec, bytes);
        validate_payload(&value, &limits.legacy).unwrap_or_else(|e| panic!("{codec:?}: {e}"));
        let mut truncated = value.clone();
        truncated.payload.clear();
        assert!(
            validate_payload(&truncated, &limits.legacy).is_err(),
            "{codec:?}"
        );
        let mut critical = value.clone();
        critical.simulation_critical = true;
        assert!(
            validate_payload(&critical, &limits.legacy).is_err(),
            "{codec:?}"
        );
        let mut semantic = value;
        semantic.kind = ResourceKind::Semantic;
        assert!(
            validate_payload(&semantic, &limits.legacy).is_err(),
            "{codec:?}"
        );
    }
}

#[test]
fn source_formats_keep_explicit_codec_and_provenance_through_real_imports() {
    use wonderland_asset_cooker::{interchange::import_spec, registry::CookSpec};
    let limits = CookLimits::default();
    let original_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packs");
    let mut spec = CookSpec::from_json(
        &std::fs::read(original_root.join("demo.json")).unwrap(),
        &limits,
    )
    .unwrap();
    let template = spec.sources[0].clone();
    spec.sources.clear();
    spec.overrides.clear();
    let dir = std::env::temp_dir().join(format!("wonderland-new-formats-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let cases = vectors();
    for (i, (format, _, payload)) in cases.iter().enumerate() {
        let mut source = template.clone();
        source.id = format!("format-{i}");
        source.path = format!("source-{i}");
        source.source_name = None;
        source.format = *format;
        std::fs::write(dir.join(&source.path), payload).unwrap();
        spec.sources.push(source);
    }
    let imported = import_spec(&spec, &dir, &limits).unwrap();
    for (i, (_, codec, payload)) in cases.iter().enumerate() {
        let value = imported
            .resources
            .iter()
            .find(|r| r.id == format!("format-{i}"))
            .unwrap();
        assert_eq!(value.codec, *codec);
        assert_eq!(value.payload, *payload);
        assert_eq!(value.provenance.source_hash, Digest::of(payload));
        assert!(!value.simulation_critical);
    }
    let first = cook_resources(
        &spec.source_baseline,
        spec.tuning_version.clone(),
        &imported.resources,
        &limits,
    )
    .unwrap();
    let second = cook_resources(
        &spec.source_baseline,
        spec.tuning_version,
        &imported.resources,
        &limits,
    )
    .unwrap();
    assert_eq!(first.packs, second.packs);
    assert_eq!(
        first.manifest.content_version,
        second.manifest.content_version
    );
    std::fs::remove_dir_all(dir).unwrap();
}
