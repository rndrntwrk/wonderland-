//! Independently authored private, semiglobal and global inputs for the real cooker.
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use wonderland_asset_cooker::{
    interchange::{chunk_id, import_spec},
    manifest::{cook_resources, CookLimits},
    registry::{
        CookSpec, DynamicSpec, LocaleSpec, ResourceOverride, Rights, ScopeSpec, SourceFormat,
        SourceSpec, TuningSpec,
    },
};
use wonderland_content_ir::manifest::{Digest, Origin, Redistribution};
use wonderland_content_runtime_bridge::{
    content::{ImportOptions, RuntimeMetadata},
    cooked::{
        CookedRuntimeDraftV1, ObjectBindingV1, RuntimeRecipeV1, ScopeBindingV1, ScopeNamespaceV1,
        SemiglobalBindingV1, BINDING_VERSION, SOURCE_BASELINE,
    },
    cooked_metadata::{ImportOptionsV1, RuntimeMetadataV1},
    sim_core::world::{Footprint, PlacementRules},
    SIM_CORE_REVISION,
};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic::{
        encode_bcon, encode_bhav, encode_glob, encode_objd, Bcon, Bhav, BhavInstruction, Glob,
        GlobEncoding, LegacyString, Objd, TextEncoding,
    },
    Limits,
};

pub const OBJECT_GUID: u32 = 123;
pub const SHARED_OWNER: u32 = 900;

pub struct Fixture {
    pub manifest: Vec<u8>,
    pub packs: BTreeMap<Digest, Vec<u8>>,
    pub draft: CookedRuntimeDraftV1,
    pub removed_source: PathBuf,
}

fn chunk(kind: [u8; 4], id: u16, name: &str, data: Vec<u8>) -> IffChunk {
    let mut label = [0; 64];
    label[..name.len()].copy_from_slice(name.as_bytes());
    IffChunk {
        key: ChunkKey { kind, id },
        flags: 0,
        label,
        data,
    }
}

fn routine(id: u16, name: &str, opcode: u16, operand: [u8; 8], limits: &Limits) -> IffChunk {
    chunk(
        *b"BHAV",
        id,
        name,
        encode_bhav(
            &Bhav {
                format_version: 0x8002,
                kind: 0,
                args: 0,
                locals: 0,
                tree_version: 1,
                reserved: vec![0; 2],
                trailing: vec![],
                instructions: vec![BhavInstruction {
                    opcode,
                    true_pointer: 254,
                    false_pointer: 255,
                    operand,
                }],
            },
            limits,
        )
        .unwrap(),
    )
}

fn constant(table: u16, value: u16, limits: &Limits) -> IffChunk {
    chunk(
        *b"BCON",
        table,
        "",
        encode_bcon(
            &Bcon {
                flags: 0,
                constants: vec![value],
                trailing: vec![],
            },
            limits,
        )
        .unwrap(),
    )
}

pub fn cook() -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "bridge-cooked-scopes-{}-{}",
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
    object.set_field("GUID1", OBJECT_GUID as u16).unwrap();
    let private = IffFile {
        header,
        chunks: vec![
            chunk(
                *b"OBJD",
                128,
                "",
                encode_objd(&object, &limits.legacy).unwrap(),
            ),
            routine(4096, "Private behavior", 8192, [0; 8], &limits.legacy),
            constant(4096, 17, &limits.legacy),
            chunk(
                *b"GLOB",
                1,
                "",
                encode_glob(
                    &Glob {
                        name: LegacyString::from_text("Shared", TextEncoding::Ascii).unwrap(),
                        storage: GlobEncoding::Pascal,
                        terminated: false,
                        trailing: vec![],
                    },
                    &limits.legacy,
                )
                .unwrap(),
            ),
        ],
    };
    let semiglobal = IffFile {
        header,
        chunks: vec![
            routine(8192, "Shared behavior", 256, [0; 8], &limits.legacy),
            constant(8192, 31, &limits.legacy),
        ],
    };
    let global = IffFile {
        header,
        chunks: vec![
            // Source Expression operand: Temp[2] := literal 61.
            routine(
                256,
                "Global behavior",
                2,
                [2, 0, 61, 0, 0, 5, 8, 7],
                &limits.legacy,
            ),
            constant(256, 23, &limits.legacy),
        ],
    };
    for (name, file) in [
        ("private.iff", &private),
        ("shared.iff", &semiglobal),
        ("global.iff", &global),
    ] {
        std::fs::write(root.join(name), iff::encode(file, &limits.legacy).unwrap()).unwrap();
    }

    // Binding order and names come from these authored sources. The cooker
    // report is deliberately never consumed as a source of runtime authority.
    let scopes: Vec<_> = [
        (
            "private",
            ScopeNamespaceV1::Private,
            "Private.iff",
            "scoped",
            &private,
        ),
        (
            "shared",
            ScopeNamespaceV1::Semiglobal,
            "Shared.iff",
            "scoped/semiglobal",
            &semiglobal,
        ),
        (
            "global",
            ScopeNamespaceV1::Global,
            "Global.iff",
            "scoped/global",
            &global,
        ),
    ]
    .into_iter()
    .map(
        |(id, namespace, source_name, prefix, file)| ScopeBindingV1 {
            id: id.into(),
            namespace,
            source_name: source_name.into(),
            resources: file
                .chunks
                .iter()
                .map(|chunk| chunk_id(prefix, chunk))
                .collect(),
        },
    )
    .collect();
    let mut resources: Vec<_> = scopes
        .iter()
        .flat_map(|scope| scope.resources.clone())
        .collect();
    resources.push("scoped/resolved-tuning".into());
    let spec = CookSpec {
        schema_version: 1,
        fixture_only: false,
        source_baseline: SOURCE_BASELINE.into(),
        tuning_version: Digest::of(b"authored scoped tuning"),
        sources: vec![SourceSpec {
            id: "scoped".into(),
            path: "private.iff".into(),
            format: SourceFormat::Iff,
            source_name: Some("Private.iff".into()),
            pack_group: "scoped".into(),
            provenance: Rights {
                origin: Origin::Authored,
                license: Some("CC0-1.0".into()),
                redistribution: Redistribution::Allowed,
            },
            patches: vec![],
            semiglobal: Some(ScopeSpec {
                path: "shared.iff".into(),
                source_name: "Shared.iff".into(),
            }),
            global: Some(ScopeSpec {
                path: "global.iff".into(),
                source_name: "Global.iff".into(),
            }),
            tuning: TuningSpec {
                dynamic_semiglobal: vec![DynamicSpec {
                    table: 0,
                    index: 0,
                    value_bits: 37.9_f32.to_bits(),
                }],
                ..TuningSpec::default()
            },
            resolver_variant: String::new(),
            locale_selection: LocaleSpec::default(),
        }],
        overrides: resources
            .into_iter()
            .map(|id| ResourceOverride {
                id,
                dependencies: vec![],
                simulation_critical: true,
                locale: None,
                variants: Default::default(),
            })
            .collect(),
    };
    let imported = import_spec(&spec, &root, &limits).unwrap();
    let cooked = cook_resources(
        SOURCE_BASELINE,
        spec.tuning_version,
        &imported.resources,
        &limits,
    )
    .unwrap();
    let manifest = cooked.manifest.canonical_bytes(&limits.manifest).unwrap();
    let draft = CookedRuntimeDraftV1 {
        schema_version: BINDING_VERSION,
        recipe: RuntimeRecipeV1 {
            sim_core_revision: SIM_CORE_REVISION.into(),
            source_baseline: SOURCE_BASELINE.into(),
            manifest_sha256: Digest::of(&manifest),
            variant: None,
            scopes,
            objects: vec![ObjectBindingV1 {
                guid: OBJECT_GUID,
                object_resource: chunk_id("scoped", &private.chunks[0]),
                private_scope: "private".into(),
                semiglobal: Some(SemiglobalBindingV1 {
                    scope: "shared".into(),
                    owner: SHARED_OWNER,
                }),
                global_scope: Some("global".into()),
                tuning_resource: "scoped/resolved-tuning".into(),
                runtime: RuntimeMetadataV1::from(RuntimeMetadata {
                    footprint: Footprint::default(),
                    placement_rules: PlacementRules::default(),
                    master_guid: None,
                    family: 0,
                    routing_slots: vec![],
                }),
            }],
            options: ImportOptionsV1::from(ImportOptions::default()),
        },
    };
    std::fs::remove_dir_all(&root).unwrap();
    Fixture {
        manifest,
        packs: cooked.packs,
        draft,
        removed_source: root,
    }
}
