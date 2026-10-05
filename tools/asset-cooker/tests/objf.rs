// SPDX-License-Identifier: MPL-2.0
use std::{collections::BTreeSet, fs, path::PathBuf};
use wonderland_asset_cooker::{
    interchange::import_spec,
    manifest::{cook_resources, CookLimits},
    registry::{CookSpec, ResourceOverride},
};
use wonderland_content_ir::{
    manifest::{LoadPhase, ResourceCodec, ResourceKind},
    packs::verify_pack,
};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk},
    semantic::{decode_objf, ObjfFunction},
};

struct TempRoot(PathBuf);
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn original_layout_objf_survives_import_critical_packing_and_verified_loading() {
    let limits = CookLimits::default();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packs");
    let root =
        TempRoot(std::env::temp_dir().join(format!("wonderland-objf-cook-{}", std::process::id())));
    fs::create_dir(&root.0).unwrap();
    let mut file = iff::decode(
        &fs::read(fixtures.join("authored.iff")).unwrap(),
        &limits.legacy,
    )
    .unwrap();
    let payload = vec![
        0, 0, 0, 0, 0x22, 0x11, 0, 0, b'f', b'J', b'B', b'O', 1, 0, 0, 0, 0, 0, 0, 0x10,
    ];
    file.chunks.push(IffChunk {
        key: ChunkKey {
            kind: *b"OBJf",
            id: 1,
        },
        flags: 0xa5a5,
        label: [0; 64],
        data: payload.clone(),
    });
    fs::write(
        root.0.join("authored.iff"),
        iff::encode(&file, &limits.legacy).unwrap(),
    )
    .unwrap();
    let mut spec =
        CookSpec::from_json(&fs::read(fixtures.join("demo.json")).unwrap(), &limits).unwrap();
    let id = "fixture/chunk-4f424a66-0001".to_owned();
    spec.overrides.push(ResourceOverride {
        id: id.clone(),
        dependencies: vec!["fixture/chunk-42484156-1000".into()],
        simulation_critical: true,
        locale: None,
        variants: BTreeSet::new(),
    });
    let imported = import_spec(&spec, &root.0, &limits).unwrap();
    let resource = imported.resources.iter().find(|r| r.id == id).unwrap();
    assert_eq!(resource.kind, ResourceKind::Semantic);
    assert_eq!(resource.codec, ResourceCodec::IffChunk);
    assert!(resource.simulation_critical);
    let cooked = cook_resources(
        &spec.source_baseline,
        spec.tuning_version.clone(),
        &imported.resources,
        &limits,
    )
    .unwrap();
    let repeated = cook_resources(
        &spec.source_baseline,
        spec.tuning_version,
        &imported.resources,
        &limits,
    )
    .unwrap();
    assert_eq!(
        cooked.manifest.canonical_bytes(&limits.manifest).unwrap(),
        repeated.manifest.canonical_bytes(&limits.manifest).unwrap()
    );
    assert_eq!(cooked.packs, repeated.packs);
    let plan = cooked
        .manifest
        .load_plan(
            std::slice::from_ref(&id),
            LoadPhase::Simulation,
            None,
            &BTreeSet::new(),
            &limits.manifest,
        )
        .unwrap();
    assert_eq!(plan.resources.len(), 4);
    let record = &cooked.manifest.resources[&id];
    let pack = verify_pack(&cooked.packs[&record.pack], &record.pack, &limits.pack).unwrap();
    pack.validate_manifest(&cooked.manifest, &limits.manifest)
        .unwrap();
    let reopened = iff::decode(pack.get(&id).unwrap(), &limits.legacy).unwrap();
    assert_eq!(reopened.chunks.len(), 1);
    assert_eq!(reopened.chunks[0].flags, 0xa5a5);
    assert_eq!(reopened.chunks[0].data, payload);
    assert_eq!(
        decode_objf(&reopened.chunks[0].data, &limits.legacy)
            .unwrap()
            .functions,
        [ObjfFunction {
            condition: 0,
            action: 4096
        }]
    );

    file.chunks.last_mut().unwrap().data[12] = 2;
    fs::write(
        root.0.join("authored.iff"),
        iff::encode(&file, &limits.legacy).unwrap(),
    )
    .unwrap();
    assert!(import_spec(
        &CookSpec {
            tuning_version: cooked.manifest.tuning_version.clone(),
            ..spec
        },
        &root.0,
        &limits
    )
    .is_err());
}
