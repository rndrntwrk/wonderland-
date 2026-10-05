//! Authored IFF/PIFF/tuning fixture sent through the production cooker.
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use wonderland_asset_cooker::{
    interchange::{chunk_id, import_spec},
    manifest::{cook_resources, CookLimits},
    registry::{
        CookSpec, DynamicSpec, LocaleSpec, PatchSpec, ResourceOverride, Rights, SourceFormat,
        SourceSpec, TuningSpec, UpgradeSpec,
    },
};
use wonderland_content_ir::manifest::{Digest, Origin, Redistribution};
use wonderland_content_runtime_bridge::{
    content::{ImportOptions, RuntimeMetadata},
    cooked_metadata::{ImportOptionsV1, RuntimeMetadataV1},
    sim_core::world::{Footprint, PlacementRules},
    SIM_CORE_REVISION,
};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic::*,
};

pub const BASELINE: &str = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
pub struct Fixture {
    pub manifest: Vec<u8>,
    pub packs: BTreeMap<Digest, Vec<u8>>,
    pub draft: Vec<u8>,
    pub removed_source: PathBuf,
}

impl Fixture {
    #[allow(dead_code)] // Shared by test targets that exercise the byte API only.
    pub fn write_release(&self, path: &std::path::Path) {
        assert!(!self.removed_source.exists());
        std::fs::write(path.join("manifest.json"), &self.manifest).unwrap();
        std::fs::write(path.join("draft.json"), &self.draft).unwrap();
        for (digest, bytes) in &self.packs {
            std::fs::write(path.join(format!("{}.wlp", digest.as_str())), bytes).unwrap();
        }
        std::fs::write(
            path.join("scenario.json"),
            br#"{
            "schema_version":1,"mode":"ts1","lot_id":11,"authority_epoch":7,
            "seed":123,"lot_width":8,"lot_height":8,"lot_levels":1,
            "object_guid":123,"tile_x":3,"tile_y":3,"level":1,"facing":0,
            "initial_attributes":[0],"routine_id":4096,"args":[],"instruction_budget":10
        }"#,
        )
        .unwrap();
    }
}

fn chunk(kind: [u8; 4], id: u16, data: Vec<u8>) -> IffChunk {
    let mut label = [0; 64];
    if kind == *b"BHAV" {
        label[..14].copy_from_slice(b"Named behavior");
    }
    IffChunk {
        key: ChunkKey { kind, id },
        flags: 0,
        label,
        data,
    }
}

fn behavior(literal: u8) -> Bhav {
    Bhav {
        format_version: 0x8002,
        kind: 0,
        args: 0,
        locals: 0,
        tree_version: 1,
        reserved: vec![0; 2],
        trailing: vec![],
        instructions: vec![
            BhavInstruction {
                opcode: 2,
                true_pointer: 1,
                false_pointer: 255,
                operand: [0, 0, 0, 0, 0, 5, 8, 26],
            },
            BhavInstruction {
                opcode: 2,
                true_pointer: 2,
                false_pointer: 255,
                operand: [1, 0, literal, 0, 0, 5, 8, 7],
            },
            BhavInstruction {
                opcode: 2,
                true_pointer: 254,
                false_pointer: 255,
                operand: [0, 0, 1, 0, 0, 5, 0, 8],
            },
        ],
    }
}

pub fn cook() -> Fixture {
    cook_custom(|_| {})
}

pub fn cook_custom(customize: impl FnOnce(&mut IffFile)) -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "bridge-cooked-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let limits = CookLimits::default();
    let mut header = [0; 64];
    let signature = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..signature.len()].copy_from_slice(signature);
    let mut object = Objd {
        version: 142,
        fields: vec![0; 103],
        trailing: vec![],
    };
    object.set_field("GUID1", 123).unwrap();
    object.set_field("NumAttributes", 1).unwrap();
    let original_behavior = encode_bhav(&behavior(1), &limits.legacy).unwrap();
    let mut file = IffFile {
        header,
        chunks: vec![
            chunk(
                *b"BHAV",
                4097,
                encode_bhav(&behavior(90), &limits.legacy).unwrap(),
            ),
            chunk(*b"OBJD", 128, encode_objd(&object, &limits.legacy).unwrap()),
            chunk(*b"BHAV", 4096, original_behavior.clone()),
            chunk(
                *b"BCON",
                4096,
                encode_bcon(
                    &Bcon {
                        flags: 0,
                        constants: vec![7],
                        trailing: vec![],
                    },
                    &limits.legacy,
                )
                .unwrap(),
            ),
        ],
    };
    customize(&mut file);
    std::fs::write(
        root.join("authored.iff"),
        iff::encode(&file, &limits.legacy).unwrap(),
    )
    .unwrap();
    let unrelated = IffFile {
        header,
        chunks: vec![chunk(*b"ZZZZ", 1, b"unselected authored resource".to_vec())],
    };
    std::fs::write(
        root.join("unrelated.iff"),
        iff::encode(&unrelated, &limits.legacy).unwrap(),
    )
    .unwrap();
    let ordered_members: Vec<_> = file.chunks.iter().map(|c| chunk_id("fixture", c)).collect();
    let patched = encode_bhav(&behavior(41), &limits.legacy).unwrap();
    let text = |s: &str| LegacyString::from_text(s, TextEncoding::Utf8).unwrap();
    let descriptor = Piff {
        version: 2,
        source: text("Authored.iff"),
        comment: text("Authored test patch"),
        trailing: vec![],
        entries: vec![PiffEntry {
            kind: *b"BHAV",
            id: 4096,
            comment: text(""),
            operation: PiffOperation::Patch {
                label: text("Named behavior"),
                flags: 0,
                new_id: 4096,
                new_size: patched.len() as u32,
                patches: vec![
                    PiffPatch {
                        offset: 0,
                        size: original_behavior.len() as u32,
                        mode: PiffPatchMode::Remove,
                        data: vec![],
                    },
                    PiffPatch {
                        offset: 0,
                        size: patched.len() as u32,
                        mode: PiffPatchMode::Add,
                        data: patched,
                    },
                ],
            },
        }],
    };
    let patch = IffFile {
        header,
        chunks: vec![chunk(
            *b"PIFF",
            256,
            encode_piff(&descriptor, &limits.legacy).unwrap(),
        )],
    };
    std::fs::write(
        root.join("authored.piff"),
        iff::encode(&patch, &limits.legacy).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("archive.otf"),
        br#"<O><T i="4096" n="x"><K i="0" l="" v="17"/></T></O>"#,
    )
    .unwrap();
    std::fs::write(
        root.join("rewrite.otf"),
        br#"<O><T i="4096" n="x"><K i="0" l="" v="23"/></T></O>"#,
    )
    .unwrap();
    let mut roots = ordered_members.clone();
    roots.push("fixture/resolved-tuning".into());
    let mut spec = CookSpec {
        schema_version: 1,
        fixture_only: false,
        source_baseline: BASELINE.into(),
        tuning_version: Digest::of(b"authored release tuning"),
        sources: vec![SourceSpec {
            id: "fixture".into(),
            path: "authored.iff".into(),
            format: SourceFormat::Iff,
            source_name: Some("Authored.iff".into()),
            pack_group: "selected".into(),
            provenance: Rights {
                origin: Origin::Authored,
                license: Some("CC0-1.0".into()),
                redistribution: Redistribution::Allowed,
            },
            patches: vec![PatchSpec {
                path: "authored.piff".into(),
                is_user: false,
            }],
            semiglobal: None,
            global: None,
            resolver_variant: String::new(),
            locale_selection: LocaleSpec::default(),
            tuning: TuningSpec {
                otf: Some("archive.otf".into()),
                otf_rewrite: Some("rewrite.otf".into()),
                upgrades: vec![UpgradeSpec {
                    table: 4096,
                    index: 0,
                    value: 31,
                }],
                dynamic_private: vec![DynamicSpec {
                    table: 0,
                    index: 0,
                    value_bits: 37.9f32.to_bits(),
                }],
                dynamic_semiglobal: vec![],
            },
        }],
        overrides: roots
            .iter()
            .map(|id| ResourceOverride {
                id: id.clone(),
                dependencies: vec![],
                simulation_critical: true,
                locale: None,
                variants: Default::default(),
            })
            .collect(),
    };
    spec.sources.push(SourceSpec {
        id: "unrelated".into(),
        path: "unrelated.iff".into(),
        format: SourceFormat::Iff,
        source_name: Some("Unrelated.iff".into()),
        pack_group: "not-selected".into(),
        provenance: Rights {
            origin: Origin::Authored,
            license: Some("CC0-1.0".into()),
            redistribution: Redistribution::Allowed,
        },
        patches: vec![],
        semiglobal: None,
        global: None,
        tuning: TuningSpec::default(),
        resolver_variant: String::new(),
        locale_selection: LocaleSpec::default(),
    });
    let imported = import_spec(&spec, &root, &limits).unwrap();
    let cooked =
        cook_resources(BASELINE, spec.tuning_version, &imported.resources, &limits).unwrap();
    let manifest = cooked.manifest.canonical_bytes(&limits.manifest).unwrap();
    let selected_pack = cooked.manifest.resources[&roots[0]].pack.clone();
    let metadata = RuntimeMetadataV1::from(RuntimeMetadata {
        footprint: Footprint::default(),
        placement_rules: PlacementRules::default(),
        master_guid: None,
        family: 0,
        routing_slots: vec![],
    });
    let draft = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "recipe": {
            "sim_core_revision": SIM_CORE_REVISION,
            "source_baseline": BASELINE,
            "manifest_sha256": Digest::of(&manifest),
            "variant": null,
            "scopes": [{"id":"private", "namespace":"private", "source_name":"Authored.iff", "resources":ordered_members}],
            "objects": [{"guid":123, "object_resource":"fixture/chunk-4f424a44-0080", "private_scope":"private", "semiglobal":null, "global_scope":null, "tuning_resource":"fixture/resolved-tuning", "runtime":metadata}],
            "options": ImportOptionsV1::from(ImportOptions::default())
        }
    })).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    Fixture {
        manifest,
        packs: cooked
            .packs
            .into_iter()
            .filter(|(id, _)| *id == selected_pack)
            .collect(),
        draft,
        removed_source: root,
    }
}
