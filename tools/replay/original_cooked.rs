//! Private qualification export through the shipping cooker and sealed loader.
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
use wonderland_asset_cooker::{
    interchange::{chunk_id, import_spec},
    manifest::{CookLimits, cook_resources},
    registry::CookSpec,
};
use wonderland_content_ir::{
    manifest::{Digest, ResourceKind},
    objects::ResolvedContent,
};
use wonderland_content_runtime_bridge::{
    SIM_CORE_REVISION,
    content::ImportOptions,
    cooked::*,
    cooked_metadata::{ImportOptionsV1, RuntimeMetadataV1},
    source_metadata::{tso_motive_tuning, tso_object_metadata},
};
use wonderland_legacy_formats::semantic::decode_objd;
use crate::original_animations;

fn json_admission(bytes: &[u8]) -> Result<(usize, usize), String> {
    fn visit(value: &Value) -> (usize, usize) {
        match value {
            Value::Object(values) => values.iter().fold((1, 0), |(nodes, text), (key, value)| {
                let (n, t) = visit(value);
                (nodes + 1 + n, text + key.len() + t)
            }),
            Value::Array(values) => values.iter().fold((1, 0), |(nodes, text), value| {
                let (n, t) = visit(value);
                (nodes + n, text + t)
            }),
            Value::String(text) => (1, text.len()),
            _ => (1, 0),
        }
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let (nodes, text) = visit(&value);
    Ok((nodes, bytes.len() * 2 + nodes * 1024 + text * 8))
}

pub fn export(
    spec: &CookSpec,
    root: &Path,
    contents: &[ResolvedContent],
    tuning_bytes: &[u8],
    output: &Path,
    limits: &CookLimits,
    animation_inputs: &original_animations::AnimationInputs,
) -> Result<Value, String> {
    if contents.len() != spec.sources.len() {
        return Err("cooked qualification requires every declared source to resolve".into());
    }
    let semiglobal_owners = crate::original_semiglobal_namespaces::owners(contents, limits)?;
    let tuning = tso_motive_tuning(tuning_bytes, &limits.legacy)?;
    let mut imported = import_spec(spec, root, limits).map_err(|e| e.to_string())?;
    let mut scopes = Vec::new();
    let mut objects = Vec::new();
    let mut selected = BTreeSet::new();
    let mut shared_global: Option<String> = None;
    let mut global_file = None;
    let mut global_inputs = None;
    for ((source, content), semiglobal_owner) in
        spec.sources.iter().zip(contents).zip(semiglobal_owners)
    {
        for (suffix, namespace, file, name) in [
            (
                "",
                ScopeNamespaceV1::Private,
                Some(&content.iff),
                content.source_name.as_str(),
            ),
            (
                "/semiglobal",
                ScopeNamespaceV1::Semiglobal,
                content.semiglobal.as_ref().map(|s| &s.iff),
                content.semiglobal.as_ref().map_or("", |s| s.name.as_str()),
            ),
            (
                "/global",
                ScopeNamespaceV1::Global,
                content.global.as_ref().map(|s| &s.iff),
                content.global.as_ref().map_or("", |s| s.name.as_str()),
            ),
        ] {
            let Some(file) = file else {
                continue;
            };
            if namespace == ScopeNamespaceV1::Global {
                let inputs = serde_json::to_vec(&(
                    source.global.as_ref(),
                    content.scope_provenance.iter().find(|p| {
                        p.namespace == wonderland_content_ir::objects::ResourceNamespace::Global
                    }),
                ))
                .map_err(|e| e.to_string())?;
                if let Some(previous) = global_file {
                    if previous != file || global_inputs.as_ref() != Some(&inputs) {
                        return Err("qualification cannot merge distinct global source, patch, or OTF inputs".into());
                    }
                    continue;
                }
                global_inputs = Some(inputs);
                global_file = Some(file);
                shared_global = Some(format!("{}/global", source.id));
            }
            let prefix = format!("{}{suffix}", source.id);
            let members: Vec<_> = file
                .chunks
                .iter()
                // These are exactly the bridge's runtime-consumed chunk kinds;
                // editor TREE/TRCN and appearance payloads do not enter a VM scope.
                .filter(|c| {
                    matches!(
                        &c.key.kind,
                        b"BHAV"
                            | b"BCON"
                            | b"STR#"
                            | b"TTAB"
                            | b"TTAs"
                            | b"GLOB"
                            | b"OBJD"
                            | b"OBJf"
                            | b"SLOT"
                    )
                })
                .map(|c| chunk_id(&prefix, c))
                .filter(|id| {
                    imported
                        .resources
                        .iter()
                        .any(|r| &r.id == id && r.kind == ResourceKind::Semantic)
                })
                .collect();
            selected.extend(members.iter().cloned());
            scopes.push(ScopeBindingV1 {
                id: prefix,
                namespace,
                source_name: name.into(),
                resources: members,
            });
        }
        let tuning_resource = format!("{}/resolved-tuning", source.id);
        selected.insert(tuning_resource.clone());
        for chunk in content.iff.chunks.iter().filter(|c| c.key.kind == *b"OBJD") {
            let object = decode_objd(&chunk.data, &limits.legacy).map_err(|e| e.to_string())?;
            objects.push(ObjectBindingV1 {
                guid: object.guid(),
                object_resource: chunk_id(&source.id, chunk),
                private_scope: source.id.clone(),
                semiglobal: semiglobal_owner.map(|owner| SemiglobalBindingV1 {
                    scope: format!("{}/semiglobal", source.id),
                    owner,
                }),
                global_scope: content.global.as_ref().and(shared_global.clone()),
                tuning_resource: tuning_resource.clone(),
                runtime: RuntimeMetadataV1::from(tso_object_metadata(
                    &content.iff,
                    chunk.key.id,
                    &limits.legacy,
                )?),
            });
        }
    }
    for resource in &mut imported.resources {
        resource.simulation_critical = selected.contains(&resource.id);
    }
    imported
        .resources
        .retain(|resource| selected.contains(&resource.id));
    imported.resources.extend(animation_inputs.resources.iter().cloned());
    // Explicit conservative whole-semantic-scope dependency graph: no claim that
    // direct BHAV calls cover dynamic named-tree/object/appearance dependencies.
    for object in &objects {
        let mut dependencies = Vec::new();
        for scope in &scopes {
            if scope.id == object.private_scope
                || object.global_scope.as_ref() == Some(&scope.id)
                || object
                    .semiglobal
                    .as_ref()
                    .is_some_and(|s| s.scope == scope.id)
            {
                dependencies.extend(
                    scope
                        .resources
                        .iter()
                        .filter(|id| *id != &object.object_resource)
                        .cloned(),
                );
            }
        }
        dependencies.push(object.tuning_resource.clone());
        dependencies.sort();
        dependencies.dedup();
        imported
            .resources
            .iter_mut()
            .find(|r| r.id == object.object_resource)
            .ok_or("missing prepared object")?
            .dependencies = dependencies;
    }
    // The source tuning.dat digest is a manifest-level tuning release identity;
    // the sealed binding separately pins every normalized value in its options.
    let cooked = cook_resources(
        &spec.source_baseline,
        tuning.source_sha256.clone(),
        &imported.resources,
        limits,
    )
    .map_err(|e| e.to_string())?;
    let manifest = cooked
        .manifest
        .canonical_bytes(&limits.manifest)
        .map_err(|e| e.to_string())?;
    let mut options = ImportOptions::default();
    options.runtime_tuning.tso_motives = Some(tuning.tuning);
    options.animations = animation_inputs.metadata.clone();
    let draft = CookedRuntimeDraftV1 {
        schema_version: BINDING_VERSION,
        recipe: RuntimeRecipeV1 {
            sim_core_revision: SIM_CORE_REVISION.into(),
            source_baseline: spec.source_baseline.clone(),
            manifest_sha256: Digest::of(&manifest),
            variant: None,
            scopes,
            objects,
            options: ImportOptionsV1::from(options),
        },
    };
    let load_limits = CookedLoadLimits::default();
    let draft_bytes = serde_json::to_vec(&draft).map_err(|e| e.to_string())?;
    let manifest_admission = json_admission(&manifest)?;
    let draft_admission = json_admission(&draft_bytes)?;
    let prepared = PreparedDraft::from_json(&draft_bytes, &manifest, &load_limits).map_err(|e|format!("draft/manifest admission ({} selected resources, {} manifest bytes, {} draft bytes, {} manifest nodes, {} draft nodes, {} aggregate admitted JSON bytes): {e}",selected.len(),manifest.len(),draft_bytes.len(),manifest_admission.0,draft_admission.0,manifest_admission.1+draft_admission.1))?;
    let packs: Vec<_> = prepared
        .required_packs()
        .iter()
        .map(|r| PackBytes {
            digest: &r.digest,
            bytes: &cooked.packs[&r.digest],
        })
        .collect();
    let binding = prepared.seal(&packs, &load_limits)?;
    let bytes = binding.canonical_bytes(&load_limits)?;
    let binding_hash = Digest::of(&bytes);
    let loaded = PreparedRelease::from_binding(&bytes, &binding_hash, &manifest, &load_limits)?
        .load(&packs, &load_limits)?;
    animation_inputs.verify_loaded(&loaded.content)?;
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    for (name, bytes) in [
        ("manifest.json", manifest.as_slice()),
        ("binding.json", bytes.as_slice()),
        ("draft.json", draft_bytes.as_slice()),
    ] {
        std::fs::write(output.join(name), bytes).map_err(|e| e.to_string())?;
    }
    // Include noncritical original visual clips as well as the simulation
    // closure. They are the exact bytes from which sealed timing was derived.
    for (digest, bytes) in &cooked.packs {
        std::fs::write(
            output.join(format!("{}.wlp", digest.as_str())),
            bytes,
        )
        .map_err(|e| e.to_string())?;
    }
    let receipt = json!({"schema":"wonderland.private-original-metadata-receipt.v1","tuning_dat_sha256":tuning.source_sha256,"manifest_sha256":Digest::of(&manifest),"binding_sha256":binding_hash,"runtime":loaded.report.runtime,"critical_semantic_resources":selected.len(),"packs":cooked.packs.len(),"simulation_packs":packs.len(),"original_animations":animation_inputs.receipt,"source_inputs":imported.report.input_hashes,"effective_identities":contents.iter().map(|c|wonderland_content_ir::objects::sha256_hex(&c.identity)).collect::<Vec<_>>(),"playable_qualified":false});
    std::fs::write(
        output.join("metadata-receipt.json"),
        serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(receipt)
}
