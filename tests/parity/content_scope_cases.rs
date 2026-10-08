//! Authored format fixtures for the F dependency preflight, not original engine tests.
#[path = "../../tools/replay/content_scope.rs"]
mod scope;
use scope::{Resource, inspect};
use wonderland_legacy_formats::{
    Limits,
    iff::{ChunkKey, IffChunk, IffFile},
    semantic::*,
};
fn chunk(kind: [u8; 4], id: u16, data: Vec<u8>) -> IffChunk {
    IffChunk {
        key: ChunkKey { kind, id },
        flags: 0,
        label: [0; 64],
        data,
    }
}
fn bhav(id: u16, call: u16) -> IffChunk {
    let mut operand = [0u8; 8];
    operand[5] = 5;
    operand[7] = 7; // my attribute 0 := literal 0
    chunk(
        *b"BHAV",
        id,
        encode_bhav(
            &Bhav {
                format_version: 0x8002,
                kind: 0,
                args: 4,
                locals: 0,
                tree_version: 1,
                reserved: vec![0; 2],
                instructions: vec![BhavInstruction {
                    opcode: call,
                    true_pointer: 254,
                    false_pointer: 255,
                    operand,
                }],
                trailing: vec![],
            },
            &Limits::default(),
        )
        .unwrap(),
    )
}
fn objd(id: u16, main: u16) -> IffChunk {
    let mut o = Objd {
        version: 142,
        fields: vec![0; 103],
        trailing: vec![],
    };
    o.set_field("GUID1", 44).unwrap();
    o.set_field("NumAttributes", 4).unwrap();
    o.set_field("BHAV_MainID", main).unwrap();
    chunk(*b"OBJD", id, encode_objd(&o, &Limits::default()).unwrap())
}
fn resource(name: &str, chunks: Vec<IffChunk>) -> Resource {
    let mut header = [0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..magic.len()].copy_from_slice(magic);
    Resource {
        name: name.into(),
        sha256: "1".repeat(64),
        file: IffFile { header, chunks },
    }
}
fn local() -> Resource {
    resource("local.iff", vec![objd(1, 4096), bhav(4096, 2)])
}
fn glob(name: &str) -> IffChunk {
    chunk(
        *b"GLOB",
        1,
        encode_glob(
            &Glob {
                name: LegacyString::from_text(name, TextEncoding::Ascii).unwrap(),
                storage: GlobEncoding::Pascal,
                terminated: false,
                trailing: vec![],
            },
            &Limits::default(),
        )
        .unwrap(),
    )
}
#[test]
fn local_static_closure_has_a_recorded_real_root() {
    let r = inspect(&local(), None, None).unwrap();
    assert!(r.static_complete);
    assert!(r.visited.contains(&"private:4096".into()));
}
#[test]
fn missing_semiglobal_is_not_equivalent_to_empty_globals() {
    let mut l = local();
    l.file.chunks.push(glob("chairglobals"));
    let r = inspect(&l, None, None).unwrap();
    assert!(!r.static_complete);
    assert!(r.missing.iter().any(|x| x.contains("chairglobals")));
}
#[test]
fn wrong_provider_name_is_rejected() {
    let mut l = local();
    l.file.chunks.push(glob("chairglobals"));
    let s = resource("other.iff", vec![]);
    assert!(inspect(&l, Some(&s), None).is_err());
}
#[test]
fn a_present_wrong_namespace_does_not_satisfy_a_global_call() {
    let mut l = local();
    l.file.chunks = vec![objd(1, 4096), bhav(4096, 500), bhav(500, 2)];
    let r = inspect(&l, None, None).unwrap();
    assert!(!r.static_complete);
    assert!(r.missing.iter().any(|x| x.contains("global:500")));
}
#[test]
fn empty_semiglobal_does_not_supply_a_missing_routine() {
    let l = resource("local.iff", vec![objd(1, 8192), glob("chairglobals")]);
    let s = resource("chairglobals.iff", vec![]);
    let r = inspect(&l, Some(&s), None).unwrap();
    assert!(!r.static_complete);
    assert!(r.missing.iter().any(|x| x.contains("semiglobal:8192")));
}
#[test]
fn transitive_private_semi_global_calls_are_followed() {
    let l = resource(
        "local.iff",
        vec![objd(1, 4096), bhav(4096, 8192), glob("chairglobals")],
    );
    let s = resource("chairglobals.iff", vec![bhav(8192, 500)]);
    let g = resource("global.iff", vec![bhav(500, 2)]);
    let r = inspect(&l, Some(&s), Some(&g)).unwrap();
    assert!(r.static_complete);
    assert_eq!(
        r.visited,
        vec!["global:500", "private:4096", "semiglobal:8192"]
    );
}
#[test]
fn cycles_terminate_and_unrelated_globals_are_not_roots() {
    let l = resource(
        "local.iff",
        vec![objd(1, 4096), bhav(4096, 4097), bhav(4097, 4096)],
    );
    let g = resource("global.iff", vec![bhav(500, 501)]);
    let r = inspect(&l, None, Some(&g)).unwrap();
    assert!(r.static_complete);
    assert_eq!(r.visited.len(), 2);
}
#[test]
fn missing_objf_is_a_blocker_not_an_empty_function_table() {
    let mut l = local();
    let mut o = decode_objd(&l.file.chunks[0].data, &Limits::default()).unwrap();
    o.set_field("UsesFnTable", 1).unwrap();
    l.file.chunks[0].data = encode_objd(&o, &Limits::default()).unwrap();
    let r = inspect(&l, None, None).unwrap();
    assert!(!r.static_complete);
    assert!(r.missing.iter().any(|x| x.contains("OBJf:1")));
}
#[test]
fn objf_checks_and_actions_both_contribute_roots() {
    let mut l = local();
    let mut o = decode_objd(&l.file.chunks[0].data, &Limits::default()).unwrap();
    o.set_field("UsesFnTable", 1).unwrap();
    l.file.chunks[0].data = encode_objd(&o, &Limits::default()).unwrap();
    l.file.chunks.push(chunk(
        *b"OBJf",
        1,
        encode_objf(
            &Objf {
                padding: 0,
                version: 1,
                functions: vec![ObjfFunction {
                    condition: 700,
                    action: 800,
                }],
                trailing: vec![],
            },
            &Limits::default(),
        )
        .unwrap(),
    ));
    let r = inspect(&l, None, None).unwrap();
    assert!(!r.static_complete);
    assert!(r.missing.iter().any(|x| x.contains("global:700")));
    assert!(r.missing.iter().any(|x| x.contains("global:800")));
}
#[test]
fn ambiguous_glob_and_duplicate_resources_are_rejected() {
    let mut l = local();
    l.file
        .chunks
        .extend([glob("chairglobals"), glob("chairglobals")]);
    assert!(inspect(&l, None, None).is_err());
}
#[test]
fn malformed_source_bhav_does_not_get_skipped() {
    let mut l = local();
    l.file.chunks[1].data.truncate(2);
    assert!(inspect(&l, None, None).is_err());
}
#[test]
fn invalid_branch_is_rejected_by_shipping_bridge() {
    let mut l = local();
    let mut b = decode_bhav(&l.file.chunks[1].data, &Limits::default()).unwrap();
    b.instructions[0].true_pointer = 2;
    l.file.chunks[1].data = encode_bhav(&b, &Limits::default()).unwrap();
    assert!(inspect(&l, None, None).is_err());
}
#[test]
fn zero_guid_and_missing_object_definition_are_not_complete_objects() {
    let r = resource("empty.iff", vec![bhav(4096, 2)]);
    assert!(inspect(&r, None, None).is_err());
    let mut l = local();
    let mut o = decode_objd(&l.file.chunks[0].data, &Limits::default()).unwrap();
    o.set_field("GUID1", 0).unwrap();
    l.file.chunks[0].data = encode_objd(&o, &Limits::default()).unwrap();
    assert!(inspect(&l, None, None).is_err());
}
#[test]
fn unexpected_semiglobal_is_not_implicitly_bound() {
    let s = resource("chairglobals.iff", vec![bhav(8192, 2)]);
    assert!(inspect(&local(), Some(&s), None).is_err());
}
#[test]
fn repeated_audit_is_deterministic_and_does_not_mutate_source() {
    let l = local();
    let before = l.file.clone();
    let a = serde_json::to_vec(&inspect(&l, None, None).unwrap()).unwrap();
    let b = serde_json::to_vec(&inspect(&l, None, None).unwrap()).unwrap();
    assert_eq!(a, b);
    assert_eq!(l.file, before);
}
#[test]
fn decoded_resource_is_hash_pinned_before_semantics() {
    let l = local();
    let bytes = wonderland_legacy_formats::iff::encode(&l.file, &Limits::default()).unwrap();
    let h = scope::hash(&bytes);
    let r = Resource::decode("local.iff", &bytes, &h).unwrap();
    assert!(inspect(&r, None, None).unwrap().static_complete);
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(Resource::decode("local.iff", &bad, &h).is_err());
    assert!(Resource::decode("local.iff", &bytes, &"0".repeat(64)).is_err());
}
#[test]
fn names_and_digests_cannot_hide_an_invalid_input() {
    assert!(Resource::decode("\0bad.iff", &[], &"1".repeat(64)).is_err());
    assert!(Resource::decode("bad.iff", &[], "abc").is_err());
}
#[test]
fn unicode_scope_labels_fail_without_panicking() {
    let mut l = local();
    l.file.chunks.push(glob("chairglobals"));
    let s = resource("ああ.iff", vec![]);
    assert!(inspect(&l, Some(&s), None).is_err());
    let s = resource("aaaaああ", vec![]);
    assert!(inspect(&l, Some(&s), None).is_err());
}
#[test]
fn ttab_actions_are_static_roots_not_permission_grants() {
    let mut l = local();
    let mut table = decode_ttab(&[0, 0], &Limits::default()).unwrap();
    table.version = Some(8);
    table.interactions.push(TtabInteraction {
        action_function: 700,
        test_function: 701,
        motives: vec![],
        flags: 1,
        string_index: 0,
        attenuation_code: 0,
        attenuation_value_bits: 0,
        autonomy_threshold: 0,
        joining_index: 0,
        flags2: 30,
    });
    l.file.chunks.push(chunk(
        *b"TTAB",
        128,
        encode_ttab(&table, &Limits::default()).unwrap(),
    ));
    let r = inspect(&l, None, None).unwrap();
    assert!(!r.static_complete);
    assert!(r.missing.iter().any(|x| x.contains("global:700")));
    assert!(r.missing.iter().any(|x| x.contains("global:701")));
    assert!(!r.whole_object_runtime_qualified);
}
#[test]
fn global_routines_keep_the_original_private_code_owner() {
    let l = resource("local.iff", vec![objd(1, 500), bhav(4097, 2)]);
    let g = resource("global.iff", vec![bhav(500, 4097)]);
    let a = inspect(&l, None, Some(&g)).unwrap();
    assert!(a.static_complete);
    assert!(
        a.edges
            .iter()
            .any(|e| e.from == "global:500" && e.to == "private:4097")
    );
}
#[test]
fn private_entry_does_not_fall_back_to_a_global_file() {
    let l = resource("local.iff", vec![objd(1, 4096)]);
    let g = resource("global.iff", vec![bhav(4096, 2)]);
    let a = inspect(&l, None, Some(&g)).unwrap();
    assert!(!a.static_complete);
    assert!(a.missing.contains(&"private:4096".into()));
}
#[test]
fn provider_filename_case_uses_the_source_case_insensitive_rule() {
    let mut l = local();
    l.file.chunks.push(glob("chairglobals"));
    let s = resource("Assets/CHAIRGLOBALS.IFF", vec![]);
    let a = inspect(&l, Some(&s), None).unwrap();
    assert!(a.static_complete);
    assert!(!a.whole_object_runtime_qualified);
    assert!(!a.unassessed.is_empty());
}
#[test]
fn multipart_child_requires_one_real_original_master_definition() {
    let mut l = local();
    let mut o = decode_objd(&l.file.chunks[0].data, &Limits::default()).unwrap();
    o.set_field("MasterID", 12).unwrap();
    o.set_field("SubIndex", 1).unwrap();
    l.file.chunks[0].data = encode_objd(&o, &Limits::default()).unwrap();
    let a = inspect(&l, None, None).unwrap();
    assert!(!a.static_complete);
    assert!(a.missing.iter().any(|m| m.contains("unique master OBJD")));
    let mut master = o;
    master.set_field("GUID1", 45).unwrap();
    master.set_field("SubIndex", 65535).unwrap();
    l.file.chunks.push(chunk(
        *b"OBJD",
        2,
        encode_objd(&master, &Limits::default()).unwrap(),
    ));
    assert!(inspect(&l, None, None).unwrap().static_complete);
}
#[test]
fn conflicting_master_definitions_remain_blocked() {
    let mut l = local();
    let mut o = decode_objd(&l.file.chunks[0].data, &Limits::default()).unwrap();
    o.set_field("MasterID", 12).unwrap();
    o.set_field("SubIndex", 1).unwrap();
    l.file.chunks[0].data = encode_objd(&o, &Limits::default()).unwrap();
    for i in [2, 3] {
        o.set_field("GUID1", i + 44).unwrap();
        o.set_field("SubIndex", 65535).unwrap();
        l.file.chunks.push(chunk(
            *b"OBJD",
            i,
            encode_objd(&o, &Limits::default()).unwrap(),
        ));
    }
    assert!(!inspect(&l, None, None).unwrap().static_complete);
}
#[test]
fn resource_size_and_chunk_limits_are_enforced() {
    let mut l = local();
    l.file.chunks[1].data = vec![0; scope::limits().max_resource_bytes + 1];
    assert!(inspect(&l, None, None).is_err());
}
#[test]
fn unapplied_patch_is_not_a_base_semiglobal() {
    let mut l = local();
    l.file.chunks.push(glob("chairglobals"));
    let s = resource("chairglobals.iff", vec![chunk(*b"PIFF", 1, vec![])]);
    assert!(inspect(&l, Some(&s), None).is_err());
}
#[test]
fn supplied_scope_identity_is_retained_without_claiming_runtime_qualification() {
    let mut l = local();
    l.file.chunks.push(glob("chairglobals"));
    let mut s = resource("chairglobals.iff", vec![]);
    s.sha256 = "2".repeat(64);
    let a = inspect(&l, Some(&s), None).unwrap();
    assert_eq!(a.supplied_scopes["semiglobal"], "2".repeat(64));
    assert!(!a.whole_object_runtime_qualified);
}
