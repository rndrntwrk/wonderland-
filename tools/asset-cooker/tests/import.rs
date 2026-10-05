// SPDX-License-Identifier: MPL-2.0
use wonderland_asset_cooker::{interchange::import_spec, manifest::CookLimits, registry::CookSpec};
#[test]
fn rejects_unknown_spec_fields() {
    assert!(CookSpec::from_json(br#"{"schema_version":1,"source_baseline":"4c6b3e8f5835b228723caea3c9f683c62f244f73","tuning_version":"0000000000000000000000000000000000000000000000000000000000000000","sources":[],"overrides":[],"typo":true}"#, &CookLimits::default()).is_err());
}
#[test]
fn fixture_preserves_unknown_and_validates_explicit_critical_closure() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packs");
    let bytes = std::fs::read(root.join("demo.json")).unwrap();
    let spec = CookSpec::from_json(&bytes, &CookLimits::default()).unwrap();
    let imported = import_spec(&spec, &root, &CookLimits::default()).unwrap();
    assert_eq!(imported.resources.len(), 4);
    let unknown = imported
        .resources
        .iter()
        .find(|r| r.id.ends_with("chunk-5a5a5a5a-0007"))
        .unwrap();
    assert!(!unknown.simulation_critical);
    let iff =
        wonderland_legacy_formats::iff::decode(&unknown.payload, &Default::default()).unwrap();
    assert_eq!(iff.chunks[0].data, b"Authored opaque bytes\0\xff");
    assert_eq!(iff.chunks[0].flags, 0x1234);
    assert_eq!(&iff.chunks[0].label[..13], b"Authored ZZZZ");
}

fn fixture_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packs")
}
fn fixture_spec() -> CookSpec {
    CookSpec::from_json(
        &std::fs::read(fixture_root().join("demo.json")).unwrap(),
        &CookLimits::default(),
    )
    .unwrap()
}
#[test]
fn duplicate_ids_unknown_overrides_missing_dependencies_and_malformed_critical_fail() {
    let root = fixture_root();
    let limits = CookLimits::default();
    let mut spec = fixture_spec();
    spec.sources.push(spec.sources[0].clone());
    assert!(import_spec(&spec, &root, &limits).is_err());
    let mut spec = fixture_spec();
    spec.overrides[0].id = "fixture/typo".into();
    assert!(import_spec(&spec, &root, &limits)
        .unwrap_err()
        .to_string()
        .contains("unknown override"));
    let mut spec = fixture_spec();
    spec.overrides[0].dependencies = vec!["missing".into()];
    assert!(import_spec(&spec, &root, &limits).is_err());
    let mut spec = fixture_spec();
    spec.overrides.push(spec.overrides[0].clone());
    assert!(import_spec(&spec, &root, &limits).is_err());
    let mut spec = fixture_spec();
    spec.overrides[0].id = "fixture/chunk-5a5a5a5a-0007".into();
    assert!(import_spec(&spec, &root, &limits).is_err());
    let dir = std::env::temp_dir().join(format!("cooker-malformed-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let mut iff = wonderland_legacy_formats::iff::decode(
        &std::fs::read(root.join("authored.iff")).unwrap(),
        &limits.legacy,
    )
    .unwrap();
    iff.chunks[0].data = vec![0];
    std::fs::write(
        dir.join("authored.iff"),
        wonderland_legacy_formats::iff::encode(&iff, &limits.legacy).unwrap(),
    )
    .unwrap();
    assert!(import_spec(&fixture_spec(), &dir, &limits).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn import_obeys_exact_piff_source_user_precedence_patch_order_and_byte_provenance() {
    use wonderland_asset_cooker::registry::PatchSpec;
    use wonderland_legacy_formats::{
        iff::{self, ChunkKey, IffChunk, IffFile},
        semantic::*,
    };
    let limits = CookLimits::default();
    let dir = std::env::temp_dir().join(format!("cooker-piff-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let original = std::fs::read(fixture_root().join("authored.iff")).unwrap();
    std::fs::write(dir.join("authored.iff"), &original).unwrap();
    let base = iff::decode(&original, &limits.legacy).unwrap();
    let old = base
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"ZZZZ")
        .unwrap()
        .data
        .len();
    let text = |t: &str| LegacyString::from_text(t, TextEncoding::Utf8).unwrap();
    let write_patch = |path: &str, target: &str, data: &[u8], source_len: usize| {
        let descriptor = Piff {
            version: 2,
            source: text(target),
            comment: text(""),
            entries: vec![PiffEntry {
                kind: *b"ZZZZ",
                id: 7,
                comment: text(""),
                operation: PiffOperation::Patch {
                    label: text("Patched unknown"),
                    flags: 0,
                    new_id: 7,
                    new_size: data.len() as u32,
                    patches: vec![
                        PiffPatch {
                            offset: 0,
                            size: source_len as u32,
                            mode: PiffPatchMode::Remove,
                            data: Vec::new(),
                        },
                        PiffPatch {
                            offset: 0,
                            size: data.len() as u32,
                            mode: PiffPatchMode::Add,
                            data: data.to_vec(),
                        },
                    ],
                },
            }],
            trailing: Vec::new(),
        };
        let file = IffFile {
            header: base.header,
            chunks: vec![IffChunk {
                key: ChunkKey {
                    kind: *b"PIFF",
                    id: 256,
                },
                flags: 0,
                label: [0; 64],
                data: encode_piff(&descriptor, &limits.legacy).unwrap(),
            }],
        };
        std::fs::write(dir.join(path), iff::encode(&file, &limits.legacy).unwrap()).unwrap();
    };
    write_patch("base.piff", "Authored.iff", b"base", old);
    write_patch("wrong.piff", "authored.iff", b"wrong", old);
    write_patch("user1.piff", "Authored.iff", b"first", old);
    write_patch("user2.piff", "Authored.iff", b"final", 5);
    let mut spec = fixture_spec();
    spec.sources[0].patches = vec![
        PatchSpec {
            path: "base.piff".into(),
            is_user: false,
        },
        PatchSpec {
            path: "wrong.piff".into(),
            is_user: true,
        },
        PatchSpec {
            path: "user1.piff".into(),
            is_user: true,
        },
        PatchSpec {
            path: "user2.piff".into(),
            is_user: true,
        },
    ];
    let imported = import_spec(&spec, &dir, &limits).unwrap();
    let report = &imported.report.sources[0];
    assert_eq!(
        report
            .applied_patches
            .iter()
            .map(|p| p.order)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(report.suppressed_patches.len(), 1);
    assert_eq!(report.suppressed_patches[0].name, "base.piff");
    let unknown = imported
        .resources
        .iter()
        .find(|r| r.id.ends_with("chunk-5a5a5a5a-0007"))
        .unwrap();
    let chunk = iff::decode(&unknown.payload, &limits.legacy)
        .unwrap()
        .chunks
        .remove(0);
    assert_eq!(chunk.data, b"final");
    assert_eq!(chunk.flags, 0x1234);
    assert_eq!(&chunk.label[..15], b"Patched unknown");
    assert_eq!(
        unknown.provenance.source_hash,
        wonderland_content_ir::manifest::Digest::of(&original)
    );
    assert_eq!(
        unknown.provenance.patch_hashes,
        vec![
            wonderland_content_ir::manifest::Digest::of(
                &std::fs::read(dir.join("user1.piff")).unwrap()
            ),
            wonderland_content_ir::manifest::Digest::of(
                &std::fs::read(dir.join("user2.piff")).unwrap()
            )
        ]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn actual_scopes_otf_rewrite_upgrade_dynamic_values_are_cooked_and_hashed() {
    use wonderland_asset_cooker::registry::{DynamicSpec, ScopeSpec, UpgradeSpec};
    use wonderland_legacy_formats::{
        iff,
        semantic::{encode_bcon, Bcon},
    };
    let limits = CookLimits::default();
    let dir = std::env::temp_dir().join(format!("cooker-tuning-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let original = std::fs::read(fixture_root().join("authored.iff")).unwrap();
    std::fs::write(dir.join("authored.iff"), &original).unwrap();
    let base = iff::decode(&original, &limits.legacy).unwrap();
    for (path, id, value) in [("semi.iff", 8192, 20), ("global.iff", 256, 30)] {
        let mut chunk = base.chunks[1].clone();
        chunk.key.id = id;
        chunk.data = encode_bcon(
            &Bcon {
                flags: 0,
                constants: vec![value],
                trailing: Vec::new(),
            },
            &limits.legacy,
        )
        .unwrap();
        std::fs::write(
            dir.join(path),
            iff::encode(
                &iff::IffFile {
                    header: base.header,
                    chunks: vec![chunk],
                },
                &limits.legacy,
            )
            .unwrap(),
        )
        .unwrap();
    }
    std::fs::write(
        dir.join("archive.otf"),
        br#"<O><T i="4096" n="x"><K i="0" l="" v="100"/><K i="1" l="" v="101"/></T></O>"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("rewrite.otf"),
        br#"<O><T i="4096" n="x"><K i="0" l="" v="200"/></T></O>"#,
    )
    .unwrap();
    let mut spec = fixture_spec();
    spec.sources[0].semiglobal = Some(ScopeSpec {
        path: "semi.iff".into(),
        source_name: "Semi.iff".into(),
    });
    spec.sources[0].global = Some(ScopeSpec {
        path: "global.iff".into(),
        source_name: "Global.iff".into(),
    });
    spec.sources[0].tuning.otf = Some("archive.otf".into());
    spec.sources[0].tuning.otf_rewrite = Some("rewrite.otf".into());
    spec.sources[0].tuning.upgrades = vec![UpgradeSpec {
        table: 4096,
        index: 0,
        value: 201,
    }];
    spec.sources[0].tuning.dynamic_private = vec![DynamicSpec {
        table: 0,
        index: 0,
        value_bits: 9.9f32.to_bits(),
    }];
    spec.sources[0].tuning.dynamic_semiglobal = vec![DynamicSpec {
        table: 0,
        index: 0,
        value_bits: 22.9f32.to_bits(),
    }];
    let imported = import_spec(&spec, &dir, &limits).unwrap();
    assert_eq!(imported.resources.len(), 6);
    let resource = imported
        .resources
        .iter()
        .find(|r| r.id == "fixture/resolved-tuning")
        .unwrap();
    let tuning =
        wonderland_content_ir::tuning_pack::decode(&resource.payload, &limits.legacy).unwrap();
    assert_eq!(tuning.value_or_zero(0), 9);
    assert_eq!(tuning.value_or_zero(1), 11);
    assert_eq!(tuning.value_or_zero(64 << 7), 22);
    assert_eq!(tuning.value_or_zero(128 << 7), 30);
    assert_ne!(
        resource.provenance.tuning_hash.as_ref().unwrap(),
        &spec.tuning_version
    );
    assert_eq!(imported.report.input_hashes.len(), 5);
    let global = imported
        .resources
        .iter()
        .find(|r| r.id == "fixture/global/chunk-42434f4e-0100")
        .unwrap();
    assert_eq!(global.provenance.source, "global.iff");
    assert_eq!(
        global.provenance.source_hash,
        wonderland_content_ir::manifest::Digest::of(
            &std::fs::read(dir.join("global.iff")).unwrap()
        )
    );
    let first_identity = imported
        .report
        .sources
        .iter()
        .find(|r| r.id == "fixture")
        .unwrap()
        .resolver_identity
        .clone();
    spec.sources[0].tuning.dynamic_private[0].value_bits = 10.0f32.to_bits();
    let updated = import_spec(&spec, &dir, &limits).unwrap();
    assert_ne!(
        first_identity,
        updated
            .report
            .sources
            .iter()
            .find(|r| r.id == "fixture")
            .unwrap()
            .resolver_identity
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn archive_import_keeps_full_id_and_never_turns_entry_names_into_paths() {
    use wonderland_asset_cooker::registry::SourceFormat;
    let limits = CookLimits::default();
    let dir = std::env::temp_dir().join(format!("cooker-archives-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let payload = std::fs::read(fixture_root().join("authored.iff")).unwrap();
    for (format, a) in [(SourceFormat::Far1a, true), (SourceFormat::Far1b, false)] {
        let name = b"../Authored.iff";
        let offset = 16 + payload.len();
        let mut far = b"FAR!byAZ".to_vec();
        far.extend_from_slice(&1u32.to_le_bytes());
        far.extend_from_slice(&(offset as u32).to_le_bytes());
        far.extend_from_slice(&payload);
        far.extend_from_slice(&1u32.to_le_bytes());
        far.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        far.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        far.extend_from_slice(&16u32.to_le_bytes());
        if a {
            far.extend_from_slice(&(name.len() as u32).to_le_bytes());
        } else {
            far.extend_from_slice(&(name.len() as u16).to_le_bytes());
        }
        far.extend_from_slice(name);
        std::fs::write(dir.join("archive.far"), &far).unwrap();
        let mut spec = fixture_spec();
        spec.sources[0].path = "archive.far".into();
        spec.sources[0].format = format;
        spec.overrides.clear();
        let imported = import_spec(&spec, &dir, &limits).unwrap();
        assert_eq!(imported.resources.len(), 4);
        assert!(imported.resources.iter().all(|r| r
            .id
            .starts_with("fixture/far1-2e2e2f417574686f7265642e696666/")));
        assert!(
            imported
                .resources
                .iter()
                .all(|r| r.provenance.source_hash
                    == wonderland_content_ir::manifest::Digest::of(&far))
        );
        assert!(!dir.parent().unwrap().join("Authored.iff").exists());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[cfg(unix)]
#[test]
fn nonregular_fifo_is_refused_without_opening() {
    let dir = std::env::temp_dir().join(format!("cooker-fifo-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("fifo");
    assert!(std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    assert!(wonderland_asset_cooker::fs_io::read_bounded(&path, 32)
        .unwrap_err()
        .to_string()
        .contains("nonregular"));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn explicit_tsbo_ttab_survives_real_import_and_codec_validation() {
    use wonderland_legacy_formats::iff;
    let limits = CookLimits::default();
    let dir = std::env::temp_dir().join(format!("cooker-tsbo-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let mut file = iff::decode(
        &std::fs::read(fixture_root().join("authored.iff")).unwrap(),
        &limits.legacy,
    )
    .unwrap();
    let mut chunk = file.chunks.remove(0);
    chunk.key.kind = *b"TTAB";
    chunk.key.id = 1;
    chunk.data = vec![1, 0, 10, 0, 0];
    chunk.data.extend_from_slice(&[0; 32]);
    file.chunks = vec![chunk];
    std::fs::write(
        dir.join("authored.iff"),
        iff::encode(&file, &limits.legacy).unwrap(),
    )
    .unwrap();
    let mut spec = fixture_spec();
    spec.overrides.clear();
    assert!(import_spec(&spec, &dir, &limits).is_err());
    spec.sources[0].resolver_variant = "tsbo".into();
    let imported = import_spec(&spec, &dir, &limits).unwrap();
    let resource = imported
        .resources
        .iter()
        .find(|r| r.id == "fixture/chunk-54544142-0001")
        .unwrap();
    assert_eq!(
        resource.codec,
        wonderland_content_ir::manifest::ResourceCodec::IffTtabTsbo
    );
    wonderland_asset_cooker::manifest::cook_resources(
        &spec.source_baseline,
        spec.tuning_version.clone(),
        &imported.resources,
        &limits,
    )
    .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn far3_and_dbpf_import_preserve_canonical_components_and_opaque_payload() {
    use wonderland_asset_cooker::registry::SourceFormat;
    let limits = CookLimits::default();
    let dir = std::env::temp_dir().join(format!("cooker-opaque-archives-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    // Literal source FAR3 record: type 0x01020304, file 0xa1b2c3d4.
    let mut far =
        b"FAR!byAZ\x03\0\0\0\x13\0\0\0abc\x01\0\0\0\x03\0\0\0\x03\0\0\0\x10\0\0\0\0\x55\x02\0"
            .to_vec();
    far.extend_from_slice(&[4, 3, 2, 1, 0xd4, 0xc3, 0xb2, 0xa1]);
    far.extend_from_slice(b"f3");
    // Conventional DBPF1.0 / index7.0, exact TGI32 and raw bytes.
    let mut dbpf = b"DBPF".to_vec();
    dbpf.extend_from_slice(&1u32.to_le_bytes());
    dbpf.extend_from_slice(&0u32.to_le_bytes());
    dbpf.extend_from_slice(&[0; 20]);
    dbpf.extend_from_slice(&7u32.to_le_bytes());
    dbpf.extend_from_slice(&1u32.to_le_bytes());
    dbpf.extend_from_slice(&99u32.to_le_bytes());
    dbpf.extend_from_slice(&20u32.to_le_bytes());
    dbpf.extend_from_slice(&[0; 48]);
    dbpf.extend_from_slice(b"dbp");
    for value in [0x11223344u32, 0x55667788, 0x99aabbcc, 96, 3] {
        dbpf.extend_from_slice(&value.to_le_bytes());
    }
    for (format, bytes, id, payload) in [
        (
            SourceFormat::Far3,
            far,
            "fixture/far3-01020304-a1b2c3d4",
            b"abc",
        ),
        (
            SourceFormat::Dbpf,
            dbpf,
            "fixture/dbpf-11223344-55667788-99aabbcc",
            b"dbp",
        ),
    ] {
        std::fs::write(dir.join("archive.bin"), &bytes).unwrap();
        let mut spec = fixture_spec();
        spec.sources[0].path = "archive.bin".into();
        spec.sources[0].format = format;
        spec.overrides.clear();
        let imported = import_spec(&spec, &dir, &limits).unwrap();
        assert_eq!(imported.resources.len(), 1);
        assert_eq!(imported.resources[0].id, id);
        assert_eq!(imported.resources[0].payload, payload);
        assert!(!imported.resources[0].simulation_critical);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn standalone_pcm_import_uses_actual_codec_and_refuses_critical_audio() {
    use wonderland_asset_cooker::registry::{ResourceOverride, SourceFormat};
    let limits = CookLimits::default();
    let dir = std::env::temp_dir().join(format!("cooker-audio-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let mut wave = b"RIFF".to_vec();
    wave.extend_from_slice(&38u32.to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&8000u32.to_le_bytes());
    wave.extend_from_slice(&16000u32.to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&2u32.to_le_bytes());
    wave.extend_from_slice(&[0, 0]);
    std::fs::write(dir.join("sample.wav"), wave).unwrap();
    let mut spec = fixture_spec();
    spec.sources[0].path = "sample.wav".into();
    spec.sources[0].format = SourceFormat::PcmWave;
    spec.overrides.clear();
    let imported = import_spec(&spec, &dir, &limits).unwrap();
    assert_eq!(
        imported.resources[0].kind,
        wonderland_content_ir::manifest::ResourceKind::Audio
    );
    assert_eq!(
        imported.resources[0].codec,
        wonderland_content_ir::manifest::ResourceCodec::PcmWave
    );
    spec.overrides.push(ResourceOverride {
        id: "fixture".into(),
        dependencies: Vec::new(),
        simulation_critical: true,
        locale: None,
        variants: Default::default(),
    });
    assert!(import_spec(&spec, &dir, &limits).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn source_palette_and_drawing_edges_survive_empty_override_and_forced_pack_split() {
    use wonderland_asset_cooker::registry::ResourceOverride;
    use wonderland_content_ir::manifest::LoadPhase;
    use wonderland_legacy_formats::iff;
    let mut limits = CookLimits::default();
    limits.pack.max_pack_bytes = 350;
    let dir = std::env::temp_dir().join(format!("cooker-palette-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let mut file = iff::decode(
        &std::fs::read(fixture_root().join("authored.iff")).unwrap(),
        &limits.legacy,
    )
    .unwrap();
    let template = file.chunks[0].clone();
    let mut palette = template.clone();
    palette.key.kind = *b"PALT";
    palette.key.id = 7;
    palette.data = vec![
        0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 12, 13, 21, 22, 23, 31, 32, 33,
    ];
    let mut sprite = template.clone();
    sprite.key.kind = *b"SPR2";
    sprite.key.id = 8;
    let pixels = [6, 0, 1, 0xc0, 1, 0, 0, 0xa0];
    sprite.data = vec![233, 3, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 233, 3, 0, 0];
    sprite.data.extend_from_slice(&24u32.to_le_bytes());
    sprite.data.extend_from_slice(&1u16.to_le_bytes());
    sprite.data.extend_from_slice(&1u16.to_le_bytes());
    sprite
        .data
        .extend_from_slice(&[7, 0, 0, 0, 0, 0, 0, 0, 0xfe, 0xff, 5, 0]);
    sprite.data.extend_from_slice(&pixels);
    let mut drawing = template;
    drawing.key.kind = *b"DGRP";
    drawing.key.id = 9;
    drawing.data = 20000u16.to_le_bytes().to_vec();
    drawing.data.extend_from_slice(&1u16.to_le_bytes());
    drawing.data.extend_from_slice(&1u16.to_le_bytes());
    drawing.data.extend_from_slice(&[1, 1]);
    for value in [0u16, 8, 0, 0, 0, 0] {
        drawing.data.extend_from_slice(&value.to_le_bytes());
    }
    file.chunks = vec![palette, sprite, drawing];
    std::fs::write(
        dir.join("authored.iff"),
        iff::encode(&file, &limits.legacy).unwrap(),
    )
    .unwrap();
    let mut spec = fixture_spec();
    spec.overrides = vec![ResourceOverride {
        id: "fixture/chunk-53505232-0008".into(),
        dependencies: Vec::new(),
        simulation_critical: false,
        locale: None,
        variants: Default::default(),
    }];
    let imported = import_spec(&spec, &dir, &limits).unwrap();
    let resource = imported
        .resources
        .iter()
        .find(|r| r.id == "fixture/chunk-53505232-0008")
        .unwrap();
    assert_eq!(resource.dependencies, vec!["fixture/chunk-50414c54-0007"]);
    let cooked = wonderland_asset_cooker::manifest::cook_resources(
        &spec.source_baseline,
        spec.tuning_version.clone(),
        &imported.resources,
        &limits,
    )
    .unwrap();
    let plan = cooked
        .manifest
        .load_plan(
            &["fixture/chunk-53505232-0008".into()],
            LoadPhase::All,
            None,
            &Default::default(),
            &limits.manifest,
        )
        .unwrap();
    assert_eq!(plan.resources.len(), 2);
    assert_eq!(plan.packs.len(), 2);
    let drawing_plan = cooked
        .manifest
        .load_plan(
            &["fixture/chunk-44475250-0009".into()],
            LoadPhase::All,
            None,
            &Default::default(),
            &limits.manifest,
        )
        .unwrap();
    assert_eq!(drawing_plan.resources.len(), 3);
    file.chunks.retain(|c| c.key.kind != *b"PALT");
    std::fs::write(
        dir.join("authored.iff"),
        iff::encode(&file, &limits.legacy).unwrap(),
    )
    .unwrap();
    assert!(import_spec(&spec, &dir, &limits).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn incomplete_cooked_pack_map_never_publishes_a_manifest() {
    let limits = CookLimits::default();
    let spec = fixture_spec();
    let imported = import_spec(&spec, &fixture_root(), &limits).unwrap();
    let mut cooked = wonderland_asset_cooker::manifest::cook_resources(
        &spec.source_baseline,
        spec.tuning_version,
        &imported.resources,
        &limits,
    )
    .unwrap();
    let hash = cooked.packs.keys().next().unwrap().clone();
    cooked.packs.remove(&hash);
    let output = std::env::temp_dir().join(format!("cooker-missing-pack-{}", std::process::id()));
    assert!(!output.exists());
    assert!(
        wonderland_asset_cooker::fs_io::write_release(&output, &cooked, b"{}", &limits)
            .unwrap_err()
            .to_string()
            .contains("exactly cover")
    );
    assert!(!output.exists());
}
