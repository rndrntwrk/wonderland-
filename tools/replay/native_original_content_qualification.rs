//! Private source qualification using the shipping cooker/resolver and VM decoder.
//! Input bytes, cooked content, and reports remain outside the source tree.
#[allow(dead_code)]
mod content_scope;
mod original_animations;
mod original_cooked;
mod original_semiglobal_namespaces;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use wonderland_asset_cooker::{
    fs_io, interchange::resolve_iff_source, manifest::CookLimits, registry::CookSpec,
};
use wonderland_content_ir::{
    objects::{ResolvedContent, sha256_hex},
    patches::iff_sha256,
};
use wonderland_legacy_formats::{
    iff::IffFile,
    semantic::{OBJD_FIELDS, decode_bhav, decode_objd, decode_objf, decode_slot, decode_ttab},
};

fn graph(content: &ResolvedContent, roots: &[u16], limits: &CookLimits) -> Value {
    let mut pending: BTreeSet<_> = roots.iter().copied().collect();
    let mut visited = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut invalid = BTreeMap::new();
    let mut edges = Vec::new();
    let mut primitives = BTreeMap::<u16, usize>::new();
    let mut routines = Vec::new();
    while let Some(id) = pending.pop_first() {
        if !visited.insert(id) {
            continue;
        }
        if visited.len() > 4096 || edges.len() > 32768 {
            return json!({"error":"closure bound"});
        }
        let Some(chunk) = content.scopes().lookup_bhav(id) else {
            missing.insert(id);
            continue;
        };
        routines.push(json!({"id":id,"label":String::from_utf8_lossy(&chunk.label).trim_end_matches('\0'),"bhav":decode_bhav(&chunk.data, &limits.legacy).map_err(|e|e.to_string())}));
        match wonderland_content_runtime_bridge::import_bhav(chunk, &limits.legacy) {
            Err(error) => {
                invalid.insert(id, error);
            }
            Ok(routine) => {
                for (index, instruction) in routine.instructions().iter().enumerate() {
                    if instruction.opcode >= 256 {
                        edges.push(json!({"from":id,"instruction":index,"to":instruction.opcode}));
                        if !visited.contains(&instruction.opcode) {
                            pending.insert(instruction.opcode);
                        }
                    } else {
                        *primitives.entry(instruction.opcode).or_default() += 1;
                    }
                }
            }
        }
    }
    json!({"roots":roots,"visited":visited,"missing":missing,"invalid":invalid,"edges":edges,
        "routines":routines,"primitive_counts":primitives,"direct_call_static_complete":missing.is_empty() && invalid.is_empty(),"dynamic_named_tree_calls_evaluated":false,"executed":false})
}
fn resource(
    name: &str,
    file: &IffFile,
    limits: &CookLimits,
) -> Result<content_scope::Resource, String> {
    Ok(content_scope::Resource {
        name: name.into(),
        sha256: sha256_hex(&iff_sha256(file, &limits.legacy).map_err(|e| e.to_string())?),
        file: file.clone(),
    })
}
fn summarize(content: &ResolvedContent, limits: &CookLimits) -> Result<Value, String> {
    let private = resource(&content.source_name, &content.iff, limits)?;
    let semi = content
        .semiglobal
        .as_ref()
        .map(|s| resource(&s.name, &s.iff, limits))
        .transpose()?;
    let global = content
        .global
        .as_ref()
        .map(|s| resource(&s.name, &s.iff, limits))
        .transpose()?;
    let audit = match content_scope::inspect(&private, semi.as_ref(), global.as_ref()) {
        Ok(audit) => json!({"status":"inspected","audit":audit}),
        Err(error) => json!({"status":"rejected","error":error}),
    };
    let mut objects = Vec::new();
    let mut slots = Vec::new();
    let mut functions = Vec::new();
    let mut tables = Vec::new();
    let mut menu_roots = BTreeSet::new();
    let mut entry_roots = BTreeSet::new();
    for (scope, file) in [
        ("private", Some(&content.iff)),
        ("semiglobal", content.semiglobal.as_ref().map(|s| &s.iff)),
        ("global", content.global.as_ref().map(|s| &s.iff)),
    ] {
        let Some(file) = file else {
            continue;
        };
        for chunk in &file.chunks {
            match &chunk.key.kind {
                b"OBJD" if scope == "private" => {
                    let object =
                        decode_objd(&chunk.data, &limits.legacy).map_err(|e| e.to_string())?;
                    let fields: BTreeMap<_, _> = OBJD_FIELDS
                        .iter()
                        .filter_map(|name| object.field(name).map(|v| (*name, v)))
                        .collect();
                    objects.push(json!({"chunk_id":chunk.key.id,"guid":object.guid(),"version":object.version,"fields":fields}));
                }
                b"OBJf" if scope == "private" => {
                    let table =
                        decode_objf(&chunk.data, &limits.legacy).map_err(|e| e.to_string())?;
                    for function in &table.functions {
                        entry_roots.extend(
                            [function.condition, function.action]
                                .into_iter()
                                .filter(|id| *id != 0),
                        );
                    }
                    functions.push(json!({"chunk_id":chunk.key.id,"table":table}));
                }
                b"SLOT" if scope == "private" => {
                    slots.push(json!({"chunk_id":chunk.key.id,"source":decode_slot(&chunk.data,&limits.legacy).map_err(|e|e.to_string())?}));
                }
                b"TTAB" => {
                    let table =
                        decode_ttab(&chunk.data, &limits.legacy).map_err(|e| e.to_string())?;
                    for action in &table.interactions {
                        menu_roots.extend(
                            [action.action_function, action.test_function]
                                .into_iter()
                                .filter(|id| *id != 0),
                        );
                    }
                    tables.push(json!({"scope":scope,"chunk_id":chunk.key.id,"table":table}));
                }
                _ => {}
            }
        }
    }
    let relevant = if content.source_name == "templateperson.iff" {
        vec![8373]
    } else if content.source_name == "destination.iff" {
        vec![4096, 4097, 4106, 4099]
    } else if content.source_name == "mailboxvacation.iff" {
        vec![271, 280, 4096, 4101]
    } else {
        vec![271, 280, 4102, 4103, 4104, 4105]
    };
    Ok(
        json!({"source_name":content.source_name,"source_sha256":sha256_hex(&content.source_hash),
        "effective_sha256":private.sha256,"effective_identity":sha256_hex(&content.identity),
        "applied_patches":content.applied_patches,"scope_provenance":content.scope_provenance,
        "tuning_identity":sha256_hex(&content.tuning.identity),"tuning_counts":{"private":content.tuning.private_cache.len(),"semiglobal":content.tuning.semiglobal_cache.len(),"global":content.tuning.global_cache.len(),"replacements":content.tuning.replacements.len()},
        "objects":objects,"functions":functions,"slots":slots,"tables":tables,"whole_scope_audit":audit,"selected_root_closure":graph(content,&relevant,limits),
        "all_effective_menu_closure":graph(content,&menu_roots.into_iter().collect::<Vec<_>>(),limits),
        "all_effective_entrypoint_closure":graph(content,&entry_roots.into_iter().collect::<Vec<_>>(),limits)}),
    )
}
fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 && args.len() != 4 {
        return Err("Expected private cook spec path, approved tuning.dat path, private output directory, and optional portable original-animation manifest".into());
    }
    let path = Path::new(&args[0]);
    let root = path.parent().ok_or("Missing spec directory")?;
    let limits = CookLimits::default();
    let bytes =
        fs_io::read_bounded(path, limits.manifest.max_manifest_bytes).map_err(|e| e.to_string())?;
    let spec = CookSpec::from_json(&bytes, &limits).map_err(|e| e.to_string())?;
    let mut sources = Vec::new();
    let mut resolved_sources = Vec::new();
    for source in &spec.sources {
        let outcome = match resolve_iff_source(&spec, &source.id, root, &limits) {
            Err(error) => json!({"status":"resolver-rejected","error":error.to_string()}),
            Ok(resolved) => {
                let base = summarize(&resolved.content, &limits)?;
                let mut permutations = Vec::new();
                for (a, b) in [(0, 1), (2, 3), (5, 6)] {
                    let mut swapped = spec.clone();
                    let selected = swapped
                        .sources
                        .iter_mut()
                        .find(|s| s.id == source.id)
                        .unwrap();
                    if b >= selected.patches.len() {
                        continue;
                    }
                    selected.patches.swap(a, b);
                    permutations.push(match resolve_iff_source(&swapped,&source.id,root,&limits) {
                        Err(error)=>json!({"swapped_indices":[a,b],"error":error.to_string()}),
                        Ok(other)=>json!({"swapped_indices":[a,b],"effective_primary_equal":resolved.content.iff==other.content.iff,
                            "effective_semiglobal_equal":resolved.content.semiglobal.as_ref().map(|s|&s.iff)==other.content.semiglobal.as_ref().map(|s|&s.iff),
                            "effective_global_equal":resolved.content.global.as_ref().map(|s|&s.iff)==other.content.global.as_ref().map(|s|&s.iff),
                            "tuning_equal":resolved.content.tuning==other.content.tuning}),
                    });
                }
                resolved_sources.push(resolved.content);
                json!({"status":"resolved","content":base,"input_hashes":resolved.input_hashes,"patch_permutation_checks":permutations})
            }
        };
        sources.push(json!({"id":source.id,"result":outcome}));
    }
    // Both output paths consume one verified input cohort, so a second file
    // read cannot silently select different animation bytes or metadata.
    let animation_inputs = args.get(3)
        .map(|path| original_animations::read(Path::new(path), &limits))
        .transpose()?
        .unwrap_or_default();
    let execution = qualify_runtime(
        &resolved_sources,
        Path::new(&args[1]),
        Path::new(&args[2]),
        &limits,
        &animation_inputs,
    )
    .unwrap_or_else(|error| json!({"status":"blocked","error":error}));
    let tuning_bytes = fs_io::read_bounded(Path::new(&args[1]), limits.legacy.max_input_bytes)
        .map_err(|e| e.to_string())?;
    let cooked = original_cooked::export(
        &spec,
        root,
        &resolved_sources,
        &tuning_bytes,
        &Path::new(&args[2]).join("cooked"),
        &limits,
        &animation_inputs,
    )
    .unwrap_or_else(|error| json!({"status":"blocked","error":error}));
    Ok(
        json!({"schema":"wonderland.private-original-content-qualification.v1","source_baseline":spec.source_baseline,
        "sources":sources,"runtime_qualification":execution,"cooked_qualification":cooked,"playable_release_qualified":false,
        "remaining_runtime_inputs":["avatar appearance/animation provider closure","integrated two-account shared-lot acceptance"]}),
    )
}
fn qualify_runtime(
    contents: &[ResolvedContent],
    tuning_path: &Path,
    output: &Path,
    limits: &CookLimits,
    animation_inputs: &original_animations::AnimationInputs,
) -> Result<Value, String> {
    use wonderland_content_runtime_bridge::{
        content::{ImportOptions, ObjectImport, import_content},
        cooked_metadata::RuntimeMetadataV1,
        sim_core::{
            ids::{ObjectId, PersistentId},
            interactions::PrincipalKey,
            runtime::*,
            vm::VmMode,
            world::*,
        },
        source_metadata::{tso_motive_tuning, tso_object_metadata},
    };
    let semiglobal_owners = original_semiglobal_namespaces::owners(contents, limits)?;
    let bytes = fs_io::read_bounded(tuning_path, limits.legacy.max_input_bytes)
        .map_err(|e| e.to_string())?;
    let motives = tso_motive_tuning(&bytes, &limits.legacy)?;
    let mut imports = Vec::new();
    let mut metadata = Vec::new();
    for (content, semiglobal_owner) in contents.iter().zip(semiglobal_owners) {
        for chunk in content.iff.chunks.iter().filter(|c| c.key.kind == *b"OBJD") {
            let runtime = tso_object_metadata(&content.iff, chunk.key.id, &limits.legacy)?;
            metadata.push(json!({"source":content.source_name,"chunk":chunk.key.id,"runtime":RuntimeMetadataV1::from(runtime.clone())}));
            imports.push(ObjectImport {
                resolved: content,
                object_chunk_id: chunk.key.id,
                // This is an explicit importer namespace, not an original GUID.
                semiglobal_owner,
                runtime,
            });
        }
    }
    let mut options = ImportOptions::default();
    options.runtime_tuning.tso_motives = Some(motives.tuning.clone());
    options.animations = animation_inputs.metadata.clone();
    let mut imported = import_content(&imports, options, &limits.legacy)?;
    animation_inputs.verify_loaded(&imported.content)?;
    let global: Vec<_> = imported
        .objects
        .iter()
        .flat_map(|o| o.global_interactions.clone())
        .collect();
    let catalog = wonderland_content_runtime_bridge::interactions::RuntimeCatalog::from_imported(
        &imported,
        &global,
        &Default::default(),
    )?;
    let tables = imported
        .objects
        .iter()
        .map(|object| {
            let local = catalog.local(object.guid).expect("catalog verified object");
            (
                object.guid,
                wonderland_content_runtime_bridge::sim_core::state::InteractionTable {
                    local_table_present: local.is_some(),
                    definitions: local
                        .unwrap_or(&[])
                        .iter()
                        .chain(catalog.global(object.guid))
                        .cloned()
                        .collect(),
                },
            )
        })
        .collect();
    imported.content = imported.content.with_interaction_tables(tables)?;
    // Match GameRuntime's source attachment, including the original motive
    // advertisements; omitting these produces a different content identity.
    let advertisements = imported.objects.iter().flat_map(|object| {
        use wonderland_content_runtime_bridge::sim_core::interactions::{InteractionKey, InteractionScope};
        object.interactions.as_deref().unwrap_or(&[]).iter()
            .map(|entry| (InteractionScope::Local, entry))
            .chain(object.global_interactions.iter().map(|entry| (InteractionScope::Global, entry)))
            .map(move |(scope, entry)| ((object.guid, InteractionKey { tta_index: entry.tta_index, scope }), entry.advertisement.clone()))
    }).collect();
    imported.content = imported.content.with_interaction_advertisements(advertisements)?;
    let descriptor = imported.content.descriptor()?;
    let mut r = SimRuntime::new(
        imported.content,
        LotModel::new(8, 8, 2).map_err(|e| format!("{e:?}"))?,
        RuntimeConfig::new(VmMode::Tso, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .map_err(|e| format!("{e:?}"))?;
    let tick = r
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 0x7FD96B54,
            position: TilePos::new(1, 1, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(1),
            avatar: true,
        })])
        .map_err(|e| format!("{e:?}"))?;
    let spawn = match r.step(&tick) {
        Ok(out) => format!("{out:?}"),
        Err(e) => {
            return Ok(
                json!({"status":"spawn-blocked","error":format!("{e:?}"),"metadata":metadata,"tuning_source_sha256":motives.source_sha256,"runtime_descriptor":descriptor}),
            );
        }
    };
    let actor = r
        .state()
        .entities
        .get(&ObjectId(1))
        .ok_or("spawn did not retain TemplatePerson")?
        .info
        .reference;
    let tick = r
        .next_tick(vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: Some(interaction_adapter::InteractionAccess {
                principal: PrincipalKey(9),
                allow_hidden: false,
            }),
        }])
        .map_err(|e| format!("{e:?}"))?;
    let authority = r.step(&tick).map_err(|e| format!("{e:?}"))?;
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    std::fs::write(
        output.join("original-content.bin"),
        bincode::serialize(r.content()).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        output.join("original-state.snapshot"),
        wonderland_content_runtime_bridge::sim_core::snapshot::encode(r.state(), r.content())
            .map_err(|e| format!("{e:?}"))?,
    )
    .map_err(|e| e.to_string())?;
    let offers = r.terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center());
    Ok(
        json!({"status":"runtime-executed","scope":"isolated authentic TemplatePerson/destination; no account admission or appearance assets","metadata":metadata,"animation_metadata_count":animation_inputs.metadata.len(),"tuning_source_sha256":motives.source_sha256,"runtime_descriptor":descriptor,"spawn":spawn,"authority":format!("{authority:?}"),"terrain_offers":format!("{offers:?}")}),
    )
}
fn main() {
    match run() {
        Ok(report) => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
