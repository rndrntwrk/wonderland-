//! Authored envelopes exercise source-defined layout mappings. They are not
//! counted as original gameplay objects. The embedded execution BHAV is real.
use wonderland_content_ir::objects::{resolve_content, ResolveRequest, ResolvedContent};
use wonderland_content_runtime_bridge::{
    content::*,
    sim_core::{
        vm::RoutineScope,
        world::{Footprint, PlacementRules},
    },
};
use wonderland_legacy_formats::{
    iff::{self, ChunkKey, IffChunk, IffFile},
    semantic::*,
    Limits,
};

fn chunk(kind: [u8; 4], id: u16, data: Vec<u8>) -> IffChunk {
    IffChunk {
        key: ChunkKey { kind, id },
        flags: 0,
        label: [0; 64],
        data,
    }
}

fn fixture() -> IffFile {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = iff::decode(
        &std::fs::read(
            root.join("TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"),
        )
        .unwrap(),
        &Limits::default(),
    )
    .unwrap();
    let routine = source
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BHAV" && c.key.id == 4110)
        .unwrap()
        .clone();
    let mut object = Objd {
        version: 142,
        fields: vec![0; 103],
        trailing: vec![],
    };
    object.set_field("GUID1", 123).unwrap();
    object.set_field("NumAttributes", 2).unwrap();
    object.set_field("UsesFnTable", 1).unwrap();
    object.set_field("AnimationTableID", 130).unwrap();
    object.set_field("BodyStringID", 131).unwrap();
    let mut functions = vec![0; 16 + 10 * 4];
    functions[4..8].copy_from_slice(&1u32.to_le_bytes());
    functions[8..12].copy_from_slice(b"fJBO");
    functions[12..16].copy_from_slice(&10u32.to_le_bytes());
    functions[16 + 9 * 4..16 + 9 * 4 + 2].copy_from_slice(&4110u16.to_le_bytes());
    functions[16 + 9 * 4 + 2..16 + 10 * 4].copy_from_slice(&4110u16.to_le_bytes());
    let mut labels = Strings::new(-4);
    labels.sets[0] = ["Stock", "Price", "Unused", "Flag"]
        .into_iter()
        .map(|s| StringItem {
            language: 1,
            value: LegacyString::from_text(s, TextEncoding::Utf8).unwrap(),
            comment: LegacyString::empty(TextEncoding::Utf8),
        })
        .collect();
    IffFile {
        header: source.header,
        chunks: vec![
            routine,
            chunk(
                *b"OBJD",
                16807,
                encode_objd(&object, &Limits::default()).unwrap(),
            ),
            chunk(*b"OBJf", 16807, functions),
            chunk(
                *b"STR#",
                256,
                encode_strings(&labels, &Limits::default()).unwrap(),
            ),
            chunk(
                *b"BCON",
                4096,
                encode_bcon(
                    &Bcon {
                        flags: 0,
                        constants: vec![65535, 17],
                        trailing: vec![],
                    },
                    &Limits::default(),
                )
                .unwrap(),
            ),
        ],
    }
}

fn resolve(file: IffFile) -> ResolvedContent {
    resolve_content(
        &ResolveRequest::new("authored-layout.iff", file),
        &Limits::default(),
    )
    .unwrap()
}

fn import(resolved: &ResolvedContent) -> Result<ImportedContent, String> {
    import_content(
        &[ObjectImport {
            resolved,
            object_chunk_id: 16807,
            semiglobal_owner: None,
            runtime: RuntimeMetadata {
                footprint: Footprint::rectangle(-4, -4, 4, 4),
                placement_rules: PlacementRules::default(),
                master_guid: None,
                family: 0,
                routing_slots: vec![],
            },
        }],
        ImportOptions::default(),
        &Limits::default(),
    )
}

#[test]
fn objf_bindings_attributes_strings_and_signed_tuning_reach_actual_content_set() {
    let resolved = resolve(fixture());
    let result = import(&resolved).unwrap();
    let object = result.content.object(123).unwrap();
    assert_eq!(object.attributes.len(), 4);
    assert_eq!(object.definition[0], 142);
    assert_eq!(object.definition[1], 0);
    assert_eq!(object.definition[14], 123);
    assert_eq!(object.definition[58], 2);
    assert_eq!(object.object_data[8], 256); // VMEntity constructor's ChairFacing.
    assert_eq!(object.entry_point_count, 10);
    assert_eq!(object.entry_points[&9].id, 4110);
    assert_eq!(
        object.entry_conditions[&9].scope,
        RoutineScope::Private(123)
    );
    assert_eq!(object.animation_table_id, 130);
    assert_eq!(object.body_string_id, 131);
    assert_eq!(result.content.string(123, 256, 3), Some("Flag"));
    assert_eq!(result.content.tuning().values[&(123, 4096, 0)], -1);
    assert_eq!(result.content.tuning().values[&(123, 4096, 1)], 17);
    assert_eq!(result.objects[0].effective_identity, resolved.identity);
    assert!(result.objects[0].interactions.is_none());
}

#[test]
fn missing_function_and_truncated_objf_reject_instead_of_dropping_entrypoints() {
    let mut missing = fixture();
    missing.chunks.retain(|c| c.key.kind != *b"BHAV");
    assert!(import(&resolve(missing)).unwrap_err().contains("routine"));
    let mut truncated = fixture();
    truncated
        .chunks
        .iter_mut()
        .find(|c| c.key.kind == *b"OBJf")
        .unwrap()
        .data
        .pop();
    // OBJf is now a semantic source resource, so truncation is rejected at
    // resolution before the runtime adapter is reached.
    assert!(resolve_content(
        &ResolveRequest::new("authored-layout.iff", truncated),
        &Limits::default(),
    )
    .is_err());
    let mut trailing = fixture();
    trailing
        .chunks
        .iter_mut()
        .find(|c| c.key.kind == *b"OBJf")
        .unwrap()
        .data
        .push(0);
    assert!(import(&resolve(trailing)).unwrap_err().contains("OBJf"));
}

#[test]
fn changes_after_effective_resolution_are_rejected() {
    let mut resolved = resolve(fixture());
    resolved.iff.chunks[0].data[16] ^= 1;
    assert!(import(&resolved).unwrap_err().contains("identity"));
}

#[test]
fn real_chair_requires_its_effective_semiglobal_before_import() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(
        root.join("TSOClient/FSO.Content.TSO/Content/Objects/Chair_fso_Bouncy_Beach_Ball.iff"),
    )
    .unwrap();
    let chair = resolve_content(
        &ResolveRequest::new(
            "Chair_fso_Bouncy_Beach_Ball.iff",
            iff::decode(&bytes, &Limits::default()).unwrap(),
        ),
        &Limits::default(),
    )
    .unwrap();
    assert!(import(&chair).unwrap_err().contains("semiglobal"));
}

fn with_interaction() -> IffFile {
    let mut file = fixture();
    let mut object = decode_objd(&file.chunks[1].data, &Limits::default()).unwrap();
    object.set_field("TreeTableID", 128).unwrap();
    file.chunks[1].data = encode_objd(&object, &Limits::default()).unwrap();
    let mut table = decode_ttab(&[0, 0], &Limits::default()).unwrap();
    table.version = Some(8);
    table.interactions.push(TtabInteraction {
        action_function: 4110,
        test_function: 4110,
        motives: vec![],
        flags: 1,
        string_index: 0,
        attenuation_code: 0,
        attenuation_value_bits: 0,
        autonomy_threshold: 0,
        joining_index: 0,
        flags2: 30,
    });
    file.chunks.push(chunk(
        *b"TTAB",
        128,
        encode_ttab(&table, &Limits::default()).unwrap(),
    ));
    let labels = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"STR#")
        .unwrap()
        .data
        .clone();
    file.chunks.push(chunk(*b"TTAs", 128, labels));
    file
}

#[test]
fn ttab_outputs_use_effective_bytes_instead_of_mutable_cached_semantics() {
    let mut resolved = resolve(with_interaction());
    let normal = import(&resolved).unwrap();
    let offered = &normal.objects[0].interactions.as_ref().unwrap()[0];
    assert_eq!(offered.tta_index, 0);
    assert_eq!(offered.label.as_deref(), Some("Stock"));
    assert_eq!(offered.action.scope, RoutineScope::Private(123));
    for entry in &mut resolved.semantic_resources {
        if let DecodedSemantic::Ttab(table) = &mut entry.semantic {
            table.interactions[0].flags = 0xdead_beef;
        }
    }
    assert!(import(&resolved).unwrap_err().contains("identity"));
}

#[test]
fn original_table_advertisements_and_first_global_table_survive_conversion() {
    use wonderland_content_ir::objects::SourceContent;
    let mut file = with_interaction();
    let local = file
        .chunks
        .iter_mut()
        .find(|c| c.key.kind == *b"TTAB")
        .unwrap();
    let mut table = decode_ttab(&local.data, &Limits::default()).unwrap();
    table.interactions[0].motives = vec![TtabMotive {
        minimum: -31,
        delta: 742,
        personality_modifier: 12,
    }];
    table.interactions[0].attenuation_code = 3;
    table.interactions[0].attenuation_value_bits = 0.375f32.to_bits();
    table.interactions[0].autonomy_threshold = 1234;
    table.interactions[0].joining_index = -1;
    local.data = encode_ttab(&table, &Limits::default()).unwrap();
    let mut global_action = file.chunks[0].clone();
    global_action.key.id = 256;
    let labels = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"TTAs")
        .unwrap()
        .data
        .clone();
    table.interactions[0].action_function = 256;
    table.interactions[0].test_function = 4110;
    let first = chunk(
        *b"TTAB",
        19,
        encode_ttab(&table, &Limits::default()).unwrap(),
    );
    let mut request = ResolveRequest::new("global-table-source-order.iff", file);
    request.global = Some(SourceContent {
        name: "global.iff".into(),
        iff: IffFile {
            header: request.source.iff.header,
            chunks: vec![
                global_action,
                first,
                chunk(*b"TTAs", 55, labels),
                chunk(*b"TTAB", 1, vec![0, 0]),
            ],
        },
    });
    let resolved = resolve_content(&request, &Limits::default()).unwrap();
    let imported = import(&resolved).unwrap();
    let local = &imported.objects[0].interactions.as_ref().unwrap()[0];
    assert_eq!(local.advertisement.motives[0].minimum, -31);
    assert_eq!(local.advertisement.motives[0].delta, 742);
    assert_eq!(local.advertisement.motives[0].personality_modifier, 12);
    assert_eq!(local.advertisement.attenuation_code, 3);
    assert_eq!(
        local.advertisement.attenuation_value_bits,
        0.375f32.to_bits()
    );
    assert_eq!(local.advertisement.autonomy_threshold, 1234);
    assert_eq!(local.advertisement.joining_index, -1);
    let global = &imported.objects[0].global_interactions[0];
    assert_eq!(global.action.scope, RoutineScope::Global);
    assert_eq!(global.check.unwrap().scope, RoutineScope::Private(123));
    // GetRoutineWithOwner deliberately retains Object for every BHAV scope.
    assert_eq!(global.code_owner, 123);
    assert_eq!(global.label.as_deref(), Some("Stock"));
}

#[test]
fn source_objd_version_bounds_exclude_preserved_extra_words_and_136_repair() {
    for (version, words) in [
        (136, 78),
        (138, 93),
        (139, 94),
        (140, 95),
        (141, 95),
        (142, 103),
    ] {
        let mut file = fixture();
        let mut object = decode_objd(&file.chunks[1].data, &Limits::default()).unwrap();
        object.version = version;
        object.fields.resize(106, 0);
        object.fields[words] = 4444; // Preserved extra word is not source RawData.
        object.set_field("UsesFnTable", 0).unwrap();
        file.chunks[1].data = encode_objd(&object, &Limits::default()).unwrap();
        let imported = import(&resolve(file)).unwrap();
        let definition = imported.content.object(123).unwrap();
        assert_eq!(definition.definition.len(), words + 2, "version {version}");
        assert_eq!(definition.definition[0], version as i16);
        if version == 136 {
            assert!(!definition.entry_points.contains_key(&29));
        }
    }
}

#[test]
fn multipart_subobject_inherits_missing_interaction_table_and_master_raw_definition() {
    for sub_index in [0, 0xff00] {
        multipart_inheritance(sub_index);
    }
}

fn multipart_inheritance(sub_index: u16) {
    let mut file = with_interaction();
    let mut master = decode_objd(&file.chunks[1].data, &Limits::default()).unwrap();
    master.set_field("MasterID", 1).unwrap();
    master.set_field("SubIndex", u16::MAX).unwrap();
    file.chunks[1].data = encode_objd(&master, &Limits::default()).unwrap();
    let mut child = master;
    child.set_field("GUID1", 124).unwrap();
    child.set_field("SubIndex", sub_index).unwrap();
    child.set_field("TreeTableID", u16::MAX).unwrap();
    file.chunks.push(chunk(
        *b"OBJD",
        16808,
        encode_objd(&child, &Limits::default()).unwrap(),
    ));
    let mut functions = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"OBJf")
        .unwrap()
        .clone();
    functions.key.id = 16808;
    file.chunks.push(functions);
    let resolved = resolve(file);
    let missing_master = import_content(
        &[ObjectImport {
            resolved: &resolved,
            object_chunk_id: 16808,
            semiglobal_owner: None,
            runtime: RuntimeMetadata {
                footprint: Footprint::default(),
                placement_rules: PlacementRules::default(),
                master_guid: None,
                family: 0,
                routing_slots: vec![],
            },
        }],
        ImportOptions::default(),
        &Limits::default(),
    );
    assert!(
        matches!(missing_master, Err(error) if error.contains("requires its explicit master GUID"))
    );
    let result = import_content(
        &[ObjectImport {
            resolved: &resolved,
            object_chunk_id: 16808,
            semiglobal_owner: None,
            runtime: RuntimeMetadata {
                footprint: Footprint::default(),
                placement_rules: PlacementRules::default(),
                master_guid: Some(123),
                family: 0,
                routing_slots: vec![],
            },
        }],
        ImportOptions::default(),
        &Limits::default(),
    )
    .unwrap();
    let offered = &result.objects[0]
        .interactions
        .as_ref()
        .expect("master TTAB inheritance")[0];
    assert_eq!(offered.label.as_deref(), Some("Stock"));
    assert_eq!(offered.action.scope, RoutineScope::Private(124));
    assert_eq!(offered.code_owner, 124);
    let object = result.content.object(124).unwrap();
    assert_eq!(object.master_definition[0], 142);
    assert_eq!(object.master_definition[14], 123);
    assert_eq!(object.definition[14], 124);
}

#[test]
fn attribute_and_cumulative_output_budgets_reject_before_runtime_construction() {
    for attributes in [4096, 65535] {
        let mut file = fixture();
        let mut object = decode_objd(&file.chunks[1].data, &Limits::default()).unwrap();
        object.set_field("NumAttributes", attributes).unwrap();
        file.chunks[1].data = encode_objd(&object, &Limits::default()).unwrap();
        let resolved = resolve(file);
        let limited = Limits {
            max_total_decoded_bytes: 4096,
            ..Limits::default()
        };
        let result = import_content(
            &[ObjectImport {
                resolved: &resolved,
                object_chunk_id: 16807,
                semiglobal_owner: None,
                runtime: RuntimeMetadata {
                    footprint: Footprint::default(),
                    placement_rules: PlacementRules::default(),
                    master_guid: None,
                    family: 0,
                    routing_slots: vec![],
                },
            }],
            ImportOptions::default(),
            &limited,
        );
        assert!(matches!(result, Err(error) if error.contains("budget") || attributes > 4096));
    }
    let resolved: Vec<_> = (123..126)
        .map(|guid| {
            let mut file = fixture();
            let mut object = decode_objd(&file.chunks[1].data, &Limits::default()).unwrap();
            object.set_field("GUID1", guid).unwrap();
            object.set_field("NumAttributes", 1024).unwrap();
            file.chunks[1].data = encode_objd(&object, &Limits::default()).unwrap();
            resolve(file)
        })
        .collect();
    let inputs: Vec<_> = resolved
        .iter()
        .map(|resolved| ObjectImport {
            resolved,
            object_chunk_id: 16807,
            semiglobal_owner: None,
            runtime: RuntimeMetadata {
                footprint: Footprint::default(),
                placement_rules: PlacementRules::default(),
                master_guid: None,
                family: 0,
                routing_slots: vec![],
            },
        })
        .collect();
    let limited = Limits {
        max_total_decoded_bytes: 16 * 1024,
        ..Limits::default()
    };
    for single in &inputs {
        assert!(import_content(
            std::slice::from_ref(single),
            ImportOptions::default(),
            &limited
        )
        .is_ok());
    }
    assert!(import_content(&inputs, ImportOptions::default(), &limited).is_err());
}

#[test]
fn runtime_definition_scope_reads_source_version_guid_and_attribute_indices() {
    use wonderland_content_runtime_bridge::{
        isolated::{IsolatedRuntime, RoutineQuery},
        sim_core::{
            ids::{ObjectId, PersistentId},
            runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
            vm::{PrimitiveExit, VmMode, VmStop},
            world::{Facing, LotModel, TilePos},
        },
    };
    let mut file = fixture();
    let mut probe = decode_bhav(&file.chunks[0].data, &Limits::default()).unwrap();
    probe.instructions = [0u8, 14, 58]
        .into_iter()
        .enumerate()
        .map(|(temp, index)| BhavInstruction {
            opcode: 2,
            true_pointer: if temp == 2 { 254 } else { temp as u8 + 1 },
            false_pointer: 253,
            operand: [temp as u8, 0, index, 0, 0, 5, 8, 21], // Temp := StackObjectDefinition[index].
        })
        .collect();
    file.chunks.push(chunk(
        *b"BHAV",
        4111,
        encode_bhav(&probe, &Limits::default()).unwrap(),
    ));
    let imported = import(&resolve(file)).unwrap();
    let mut runtime = SimRuntime::new(
        imported.content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let actor = runtime.state().entities[&ObjectId(1)].info.reference;
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    let outcome = isolated
        .query(&RoutineQuery {
            actor,
            target: actor,
            code_owner: 123,
            routine_id: 4111,
            args: vec![],
            instruction_budget: 10,
        })
        .unwrap();
    assert_eq!(outcome.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(&outcome.temps[..3], &[142, 123, 2]);
}

fn source_call(template: &IffChunk, id: u16, call: u16) -> IffChunk {
    let mut bhav = decode_bhav(&template.data, &Limits::default()).unwrap();
    bhav.instructions = vec![BhavInstruction {
        opcode: call,
        true_pointer: 254,
        false_pointer: 255,
        operand: [0; 8],
    }];
    chunk(
        *b"BHAV",
        id,
        encode_bhav(&bhav, &Limits::default()).unwrap(),
    )
}

fn scoped_fixture(
    guid: u16,
    global: bool,
    scope_name: &str,
    callee: u16,
    primary_call: Option<u16>,
) -> ResolvedContent {
    use wonderland_content_ir::objects::SourceContent;
    let mut file = fixture();
    let mut objd = decode_objd(&file.chunks[1].data, &Limits::default()).unwrap();
    objd.set_field("GUID1", guid).unwrap();
    file.chunks[1].data = encode_objd(&objd, &Limits::default()).unwrap();
    let template = file.chunks[0].clone();
    if let Some(call) = primary_call {
        file.chunks[0] = source_call(&template, 4110, call);
    }
    let mut scope = IffFile {
        header: file.header,
        chunks: vec![],
    };
    if primary_call.is_some() {
        scope
            .chunks
            .push(source_call(&template, callee, callee + 1));
    } else {
        let mut returned = template;
        returned.key.id = callee;
        scope.chunks.push(returned);
    }
    let mut request = ResolveRequest::new("scoped-object.iff", file);
    let resource = SourceContent {
        name: format!("{scope_name}.iff"),
        iff: scope,
    };
    if global {
        request.global = Some(resource);
    } else {
        request.source.iff.chunks.push(chunk(
            *b"GLOB",
            1,
            encode_glob(
                &Glob {
                    name: LegacyString::from_text(scope_name, TextEncoding::Latin1).unwrap(),
                    storage: GlobEncoding::CString,
                    terminated: true,
                    trailing: vec![],
                },
                &Limits::default(),
            )
            .unwrap(),
        ));
        request.semiglobal = Some(resource);
    }
    resolve_content(&request, &Limits::default()).unwrap()
}

#[test]
fn unrelated_shared_scopes_cannot_supply_each_others_missing_callees() {
    for global in [false, true] {
        let first_id = if global { 1000 } else { 8192 };
        let first = scoped_fixture(123, global, "scope-a", first_id, Some(first_id));
        let second = scoped_fixture(124, global, "scope-b", first_id + 1, None);
        let make = |resolved| ObjectImport {
            resolved,
            object_chunk_id: 16807,
            semiglobal_owner: (!global).then_some(900),
            runtime: RuntimeMetadata {
                footprint: Footprint::default(),
                placement_rules: PlacementRules::default(),
                master_guid: None,
                family: 0,
                routing_slots: vec![],
            },
        };
        assert!(import_content(
            &[make(&first)],
            ImportOptions::default(),
            &Limits::default()
        )
        .is_err());
        let combined = import_content(
            &[make(&first), make(&second)],
            ImportOptions::default(),
            &Limits::default(),
        );
        assert!(matches!(combined, Err(error) if error.contains("scope identity")));
    }
}
