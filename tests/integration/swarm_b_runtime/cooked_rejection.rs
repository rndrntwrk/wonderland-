//! Coherent but mismatched releases must fail beyond simple file hash checks.
use serde_json::Value;
use wonderland_asset_cooker::manifest::{cook_resources, CookLimits, PreparedResource};
use wonderland_content_ir::{
    manifest::{AssetManifest, Digest, ResourceCodec},
    packs::verify_pack,
};
use wonderland_content_runtime_bridge::cooked::{CookedLoadLimits, PackBytes, PreparedDraft};

#[path = "support/cooked_fixture.rs"]
mod fixture;

fn refs(f: &fixture::Fixture) -> Vec<PackBytes<'_>> {
    f.packs
        .iter()
        .map(|(digest, bytes)| PackBytes { digest, bytes })
        .collect()
}
fn value(f: &fixture::Fixture) -> Value {
    serde_json::from_slice(&f.draft).unwrap()
}
fn set_draft(f: &mut fixture::Fixture, value: Value) {
    f.draft = serde_json::to_vec(&value).unwrap();
}
fn rejected(f: &fixture::Fixture, limits: &CookedLoadLimits) -> String {
    assert!(!f.removed_source.exists());
    match PreparedDraft::from_json(&f.draft, &f.manifest, limits) {
        Err(error) => error,
        Ok(draft) => draft.seal(&refs(f), limits).unwrap_err(),
    }
}
fn reseal_manifest(f: &mut fixture::Fixture, edit: impl FnOnce(&mut AssetManifest)) {
    let limits = CookLimits::default();
    let mut manifest = AssetManifest::from_json(&f.manifest, &limits.manifest).unwrap();
    edit(&mut manifest);
    f.manifest = manifest
        .seal(&limits.manifest)
        .unwrap()
        .canonical_bytes(&limits.manifest)
        .unwrap();
    let mut draft = value(f);
    draft["recipe"]["manifest_sha256"] = serde_json::to_value(Digest::of(&f.manifest)).unwrap();
    set_draft(f, draft);
}
fn repack(f: &mut fixture::Fixture, mut edit: impl FnMut(&str, &mut Vec<u8>)) {
    let limits = CookLimits::default();
    let manifest = AssetManifest::from_json(&f.manifest, &limits.manifest).unwrap();
    let mut resources = Vec::new();
    for (digest, bytes) in &f.packs {
        let index = verify_pack(bytes, digest, &limits.pack).unwrap();
        for id in index.entries().keys() {
            let record = &manifest.resources[id];
            let mut payload = index.get(id).unwrap().to_vec();
            edit(id, &mut payload);
            resources.push(PreparedResource {
                id: id.clone(),
                group: "selected".into(),
                kind: record.kind,
                codec: record.codec,
                payload,
                dependencies: record.dependencies.clone(),
                simulation_critical: record.simulation_critical,
                locale: record.locale.clone(),
                variants: record.variants.clone(),
                provenance: record.provenance.clone(),
            });
        }
    }
    let cooked = cook_resources(
        fixture::BASELINE,
        manifest.tuning_version,
        &resources,
        &limits,
    )
    .unwrap();
    f.manifest = cooked.manifest.canonical_bytes(&limits.manifest).unwrap();
    f.packs = cooked.packs;
    let mut draft = value(f);
    draft["recipe"]["manifest_sha256"] = serde_json::to_value(Digest::of(&f.manifest)).unwrap();
    set_draft(f, draft);
}

#[test]
fn criticality_codec_and_provenance_are_checked_even_with_valid_release_hashes() {
    let limits = CookedLoadLimits::default();
    let bhav = "fixture/chunk-42484156-1000";
    let mut f = fixture::cook();
    reseal_manifest(&mut f, |m| {
        m.resources.get_mut(bhav).unwrap().simulation_critical = false
    });
    assert!(rejected(&f, &limits).contains("critical"));
    let mut f = fixture::cook();
    reseal_manifest(&mut f, |m| {
        m.resources.get_mut(bhav).unwrap().codec = ResourceCodec::IffTtabTsbo
    });
    assert!(rejected(&f, &limits).contains("codec"));
    let mut f = fixture::cook();
    reseal_manifest(&mut f, |m| {
        m.resources.get_mut(bhav).unwrap().provenance.source_hash = Digest::of(b"different source")
    });
    assert!(rejected(&f, &limits).contains("provenance"));
    let mut f = fixture::cook();
    reseal_manifest(&mut f, |m| {
        m.resources
            .get_mut("fixture/resolved-tuning")
            .unwrap()
            .provenance
            .source_hash = Digest::of(b"different tuning source")
    });
    assert!(rejected(&f, &limits).contains("provenance"));
}

#[test]
fn missing_dependency_bindings_and_mixed_runtime_namespaces_reject() {
    let limits = CookedLoadLimits::default();
    let mut f = fixture::cook();
    reseal_manifest(&mut f, |m| {
        m.resources
            .get_mut("fixture/chunk-42484156-1000")
            .unwrap()
            .dependencies
            .push("fixture/chunk-42484156-1001".into())
    });
    let mut v = value(&f);
    v["recipe"]["scopes"][0]["resources"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    set_draft(&mut f, v);
    assert!(rejected(&f, &limits).contains("closure"));
    let mut f = fixture::cook();
    let mut v = value(&f);
    v["recipe"]["scopes"][0]["namespace"] = "global".into();
    set_draft(&mut f, v);
    assert!(rejected(&f, &limits).contains("namespace"));
    let mut f = fixture::cook();
    repack(&mut f, |id, bytes| {
        if id.ends_with("42484156-1001") {
            bytes[72..74].copy_from_slice(&255u16.to_be_bytes());
        }
    });
    assert!(rejected(&f, &limits).contains("namespace"));
}

#[test]
fn duplicate_chunk_keys_are_rejected_even_for_unused_semantic_resources() {
    use wonderland_legacy_formats::{
        iff::{ChunkKey, IffChunk},
        semantic::{encode_strings, Strings},
        Limits,
    };
    let mut f = fixture::cook_custom(|file| {
        for id in [128, 129] {
            file.chunks.push(IffChunk {
                key: ChunkKey { kind: *b"CTSS", id },
                flags: 0,
                label: [0; 64],
                data: encode_strings(&Strings::new(-4), &Limits::default()).unwrap(),
            });
        }
    });
    repack(&mut f, |id, bytes| {
        if id.ends_with("43545353-0081") {
            bytes[72..74].copy_from_slice(&128u16.to_be_bytes());
        }
    });
    assert!(rejected(&f, &CookedLoadLimits::default()).contains("duplicate chunk key"));
}

#[test]
fn duplicate_scopes_members_objects_and_wrong_objd_binding_reject() {
    let limits = CookedLoadLimits::default();
    for change in 0..5 {
        let mut f = fixture::cook();
        let mut v = value(&f);
        match change {
            0 | 1 => {
                let mut scope = v["recipe"]["scopes"][0].clone();
                if change == 1 {
                    scope["id"] = "unused".into();
                }
                v["recipe"]["scopes"].as_array_mut().unwrap().push(scope);
            }
            2 => {
                let member = v["recipe"]["scopes"][0]["resources"][0].clone();
                v["recipe"]["scopes"][0]["resources"]
                    .as_array_mut()
                    .unwrap()
                    .push(member);
            }
            3 => {
                let object = v["recipe"]["objects"][0].clone();
                v["recipe"]["objects"].as_array_mut().unwrap().push(object);
            }
            4 => v["recipe"]["objects"][0]["guid"] = 124.into(),
            _ => unreachable!(),
        }
        set_draft(&mut f, v);
        let error = rejected(&f, &limits);
        assert!(
            error.contains("duplicate") || error.contains("GUID"),
            "{error}"
        );
    }
    let mut f = fixture::cook();
    let mut v = value(&f);
    v["recipe"]["objects"][0]["object_resource"] = "fixture/chunk-42484156-1000".into();
    set_draft(&mut f, v);
    assert!(rejected(&f, &limits).contains("OBJD"));
}

#[test]
fn embedded_tuning_identity_and_scope_presence_are_bound_to_the_recipe() {
    let limits = CookedLoadLimits::default();
    let mut f = fixture::cook();
    repack(&mut f, |id, bytes| {
        if id.ends_with("resolved-tuning") {
            bytes[16] ^= 1;
        }
    });
    assert!(rejected(&f, &limits).contains("tuning identity"));
    let mut f = fixture::cook();
    repack(&mut f, |id, bytes| {
        if id.ends_with("resolved-tuning") {
            bytes[12] = 1;
        }
    });
    assert!(rejected(&f, &limits).contains("scope presence"));
}

#[test]
fn strict_nested_schema_and_input_allocation_limits_apply_before_conversion() {
    let limits = CookedLoadLimits::default();
    for change in 0..6 {
        let mut f = fixture::cook();
        let mut v = value(&f);
        match change {
            0 => {
                v["recipe"]["objects"][0]["runtime"]
                    .as_object_mut()
                    .unwrap()
                    .remove("master_guid");
            }
            1 => {
                v["recipe"]["options"]["runtime_tuning"]
                    .as_object_mut()
                    .unwrap()
                    .remove("tso_motives");
            }
            2 => v["recipe"]["objects"][0]["runtime"]["footprint"] = serde_json::json!([[]]),
            3 => v["recipe"]["objects"][0]["runtime"]["placement_rules"]["typo"] = true.into(),
            4 => v["recipe"]["options"]["ttab_variant"] = serde_json::json!({"standard":null}),
            5 => {
                v["recipe"]["options"]["runtime_tuning"]["relationship_multipliers"] =
                    serde_json::json!([{"key":1,"value_bits":2139095040u32}])
            }
            _ => unreachable!(),
        }
        set_draft(&mut f, v);
        assert!(!rejected(&f, &limits).is_empty());
    }
    let f = fixture::cook();
    let duplicated = format!(
        "{{\"schema_version\":1,{}",
        &std::str::from_utf8(&f.draft).unwrap()[1..]
    );
    assert!(PreparedDraft::from_json(duplicated.as_bytes(), &f.manifest, &limits).is_err());
    let mut small = limits.clone();
    small.max_binding_bytes = f.draft.len() - 1;
    assert!(rejected(&f, &small).contains("byte limit"));
    small = limits.clone();
    small.legacy.max_total_decoded_bytes = 4096;
    assert!(rejected(&f, &small).contains("budget"));
    small = limits.clone();
    small.max_total_pack_bytes = f.packs.values().next().unwrap().len() as u64 - 1;
    assert!(rejected(&f, &small).contains("pack byte budget"));
    let deep = format!("{}0{}", "[".repeat(33), "]".repeat(33));
    assert!(
        matches!(PreparedDraft::from_json(deep.as_bytes(), &f.manifest, &limits), Err(error) if error.contains("depth"))
    );
}
