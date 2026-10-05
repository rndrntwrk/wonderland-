use wonderland_content_ir::{
    objects::{
        resolve_content, DependencyIdentity, ResolveRequest, ResourceNamespace, ResourceScope,
    },
    patches::{apply_patches, iff_sha256, PatchFile},
    strings::{lookup_string, LocaleSelection},
    tuning::{resolve_tuning, DynamicOverride, TuningInputs, TuningOrigin, TuningOverride},
};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk, IffFile},
    semantic::{
        decode_otf, decode_strings, encode_bcon, encode_piff, Bcon, LegacyString, Piff, PiffEntry,
        PiffOperation, PiffPatch, PiffPatchMode, TextEncoding,
    },
    ErrorKind, Limits,
};

fn file(chunks: Vec<IffChunk>) -> IffFile {
    let mut header = [0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE JAMIE DOORNBOS & MAXIS 1";
    header[..magic.len()].copy_from_slice(magic);
    IffFile { header, chunks }
}
fn chunk(kind: [u8; 4], id: u16, bytes: &[u8]) -> IffChunk {
    let mut label = [0; 64];
    label[0] = id as u8;
    IffChunk {
        key: ChunkKey { kind, id },
        flags: 0x1234,
        label,
        data: bytes.to_vec(),
    }
}
fn utf8(text: &str) -> LegacyString {
    LegacyString::from_text(text, TextEncoding::Utf8).unwrap()
}
fn patch(
    name: &str,
    source: &str,
    is_user: bool,
    entries: Vec<PiffEntry>,
    additions: Vec<IffChunk>,
) -> PatchFile {
    let piff = Piff {
        version: 2,
        source: utf8(source),
        comment: utf8(""),
        entries,
        trailing: Vec::new(),
    };
    let mut chunks = vec![chunk(
        *b"PIFF",
        256,
        &encode_piff(&piff, &Limits::default()).unwrap(),
    )];
    chunks.extend(additions);
    PatchFile {
        name: name.to_owned(),
        is_user,
        file: file(chunks),
    }
}
fn entry(id: u16, operation: PiffOperation) -> PiffEntry {
    PiffEntry {
        kind: *b"TEST",
        id,
        comment: utf8(""),
        operation,
    }
}
fn replace(id: u16, new_id: u16, source_len: u32, data: &[u8]) -> PiffEntry {
    entry(
        id,
        PiffOperation::Patch {
            label: utf8(""),
            flags: 0xf00d,
            new_id,
            new_size: data.len() as u32,
            patches: vec![
                PiffPatch {
                    offset: 0,
                    size: source_len,
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
    )
}
fn find(file: &IffFile, kind: [u8; 4], id: u16) -> &IffChunk {
    file.chunks
        .iter()
        .find(|c| c.key == ChunkKey { kind, id })
        .unwrap()
}
fn bcon(id: u16, constants: &[u16]) -> IffChunk {
    chunk(
        *b"BCON",
        id,
        &encode_bcon(
            &Bcon {
                flags: 0,
                constants: constants.to_vec(),
                trailing: Vec::new(),
            },
            &Limits::default(),
        )
        .unwrap(),
    )
}

#[test]
fn patches_defer_id_swaps_remove_add_and_ignore_existing_additions() {
    let source = file(vec![
        chunk(*b"TEST", 1, b"abc"),
        chunk(*b"TEST", 2, b"XYZ"),
        chunk(*b"TEST", 3, b"bye"),
        chunk(*b"UNKN", 8, &[255, 0, 254]),
    ]);
    let original = source.clone();
    let p = patch(
        "edits.piff",
        "Thing.iff",
        false,
        vec![
            replace(1, 2, 3, b"aXc"),
            replace(2, 1, 3, b"QRS"),
            entry(3, PiffOperation::Remove),
            entry(99, PiffOperation::Remove),
            entry(4, PiffOperation::Add),
        ],
        vec![chunk(*b"TEST", 2, b"ignored"), chunk(*b"TEST", 4, b"new")],
    );
    let result = apply_patches("Thing.iff", &source, &[p], &Limits::default()).unwrap();
    assert_eq!(find(&result.file, *b"TEST", 2).data, b"aXc");
    assert_eq!(find(&result.file, *b"TEST", 1).data, b"QRS");
    assert_eq!(find(&result.file, *b"TEST", 2).flags, 0x1234); // source ignores PIFF flags
    assert_eq!(
        find(&result.file, *b"TEST", 2).label,
        original.chunks[0].label
    );
    assert!(!result
        .file
        .chunks
        .iter()
        .any(|c| c.key.id == 3 && c.key.kind == *b"TEST"));
    assert_eq!(find(&result.file, *b"TEST", 4).data, b"new");
    assert_eq!(find(&result.file, *b"UNKN", 8), &original.chunks[3]);
    assert_eq!(source, original);
}

#[test]
fn supplied_user_order_suppresses_all_non_user_patches_for_exact_source_name() {
    let source = file(vec![chunk(*b"TEST", 1, b"abc")]);
    let candidates = vec![
        patch(
            "lowercase",
            "thing.iff",
            true,
            vec![entry(1, PiffOperation::Remove)],
            vec![],
        ),
        patch(
            "base-before",
            "Thing.iff",
            false,
            vec![entry(1, PiffOperation::Remove)],
            vec![],
        ),
        patch(
            "user-a",
            "Thing.iff",
            true,
            vec![replace(1, 1, 3, b"AAA")],
            vec![],
        ),
        patch(
            "base-after",
            "Thing.iff",
            false,
            vec![entry(1, PiffOperation::Remove)],
            vec![],
        ),
        patch(
            "user-b",
            "Thing.iff",
            true,
            vec![replace(1, 1, 3, b"BBB")],
            vec![],
        ),
    ];
    let result = apply_patches("Thing.iff", &source, &candidates, &Limits::default()).unwrap();
    assert_eq!(find(&result.file, *b"TEST", 1).data, b"BBB");
    assert_eq!(
        result
            .applied
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["user-a", "user-b"]
    );
    assert_eq!(
        result.applied.iter().map(|p| p.order).collect::<Vec<_>>(),
        [2, 4]
    );
    assert_eq!(result.suppressed.len(), 2);
    let reversed = vec![candidates[4].clone(), candidates[2].clone()];
    assert_eq!(
        find(
            &apply_patches("Thing.iff", &source, &reversed, &Limits::default())
                .unwrap()
                .file,
            *b"TEST",
            1
        )
        .data,
        b"AAA"
    );
}

#[test]
fn patch_rejection_is_transactional_and_add_markers_have_no_payload() {
    let source = file(vec![chunk(*b"TEST", 1, b"abc")]);
    let snapshot = source.clone();
    let valid = patch(
        "valid",
        "s.iff",
        false,
        vec![replace(1, 1, 3, b"XYZ")],
        vec![],
    );
    let invalid = patch(
        "invalid",
        "s.iff",
        false,
        vec![replace(1, 1, 100, b"abc")],
        vec![],
    );
    assert_eq!(
        apply_patches("s.iff", &source, &[valid, invalid], &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
    assert_eq!(source, snapshot);
    let missing = patch(
        "missing",
        "s.iff",
        false,
        vec![replace(99, 99, 100, b"abc"), entry(5, PiffOperation::Add)],
        vec![],
    );
    assert_eq!(
        apply_patches("s.iff", &source, &[missing], &Limits::default())
            .unwrap()
            .file,
        source
    );
}

#[test]
fn source_deferred_move_can_restore_a_later_removed_object() {
    let source = file(vec![chunk(*b"TEST", 1, b"abc")]);
    let p = patch(
        "deferred",
        "s.iff",
        false,
        vec![replace(1, 2, 3, b"XYZ"), entry(1, PiffOperation::Remove)],
        vec![],
    );
    let result = apply_patches("s.iff", &source, &[p], &Limits::default()).unwrap();
    assert_eq!(find(&result.file, *b"TEST", 2).data, b"XYZ");
}

#[test]
fn tuning_rewrite_replaces_archive_otf_and_duplicate_keys_are_last_wins() {
    let private = file(vec![bcon(4096, &[10, 11])]);
    let mut inputs = TuningInputs {
        otf: Some(
            decode_otf(
                br#"<O><T i="4096" n="x"><K i="0" l="" v="100"/><K i="1" l="" v="101"/></T></O>"#,
                &Limits::default(),
            )
            .unwrap(),
        ),
        ..TuningInputs::default()
    };
    assert_eq!(
        resolve_tuning(&private, None, None, &inputs, &Limits::default())
            .unwrap()
            .value_or_zero(1),
        101
    );
    inputs.otf_rewrite = Some(
        decode_otf(
            br#"<O><T i="4096" n="x"><K i="0" l="" v="200"/><K i="0" l="" v="65535"/></T></O>"#,
            &Limits::default(),
        )
        .unwrap(),
    );
    let tuned = resolve_tuning(&private, None, None, &inputs, &Limits::default()).unwrap();
    assert_eq!(tuned.value_or_zero(0), -1);
    assert_eq!(tuned.lookup_encoded(0).origin, TuningOrigin::OtfRewrite);
    assert_eq!(tuned.value_or_zero(1), 11);
}

#[test]
fn tuning_upgrade_then_dynamic_private_then_semiglobal_and_global_base() {
    let private = file(vec![bcon(4096, &[10])]);
    let semi = file(vec![bcon(8192, &[20])]);
    let global = file(vec![bcon(256, &[30])]);
    let inputs = TuningInputs {
        upgrades: vec![
            TuningOverride {
                table: 4096,
                index: 0,
                value: 300,
            },
            TuningOverride {
                table: 8192,
                index: 0,
                value: 301,
            },
        ],
        dynamic_private: vec![
            DynamicOverride {
                table: 0,
                index: 0,
                value_bits: 400.9f32.to_bits(),
            },
            DynamicOverride {
                table: 4096,
                index: 0,
                value_bits: 450f32.to_bits(),
            },
        ],
        dynamic_semiglobal: vec![DynamicOverride {
            table: 0,
            index: 0,
            value_bits: 500.9f32.to_bits(),
        }],
        ..TuningInputs::default()
    };
    let tuned = resolve_tuning(
        &private,
        Some(&semi),
        Some(&global),
        &inputs,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(tuned.value_or_zero(0), 400);
    assert_eq!(tuned.lookup_encoded(0).origin, TuningOrigin::DynamicPrivate);
    assert_eq!(tuned.value_or_zero(64 << 7), 500);
    assert_eq!(
        tuned.lookup_encoded(64 << 7).origin,
        TuningOrigin::DynamicSemiglobal
    );
    assert_eq!(tuned.value_or_zero(128 << 7), 30);
    assert_eq!(tuned.lookup_encoded(128 << 7).origin, TuningOrigin::Bcon);
}

#[test]
fn tuning_missing_semiglobal_fallback_encoded_mode_quirk_and_float_bounds() {
    let private = file(vec![bcon(8192, &[88]), bcon(4288, &[77])]);
    let tuned = resolve_tuning(
        &private,
        None,
        None,
        &TuningInputs::default(),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(tuned.value_or_zero(64 << 7), 88);
    assert!(tuned.lookup_encoded(64 << 7).used_private_fallback);
    assert_eq!(tuned.value_or_zero(192 << 7), 77);
    assert_eq!(tuned.lookup_encoded(9).origin, TuningOrigin::Missing);
    assert_eq!(tuned.value_or_zero(9), 0);
    for bits in [
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        32768f32.to_bits(),
    ] {
        let inputs = TuningInputs {
            dynamic_private: vec![DynamicOverride {
                table: 0,
                index: 0,
                value_bits: bits,
            }],
            ..TuningInputs::default()
        };
        assert!(resolve_tuning(&private, None, None, &inputs, &Limits::default()).is_err());
    }
}

#[test]
fn language_falls_back_by_whole_set_not_missing_index() {
    let table = decode_strings(
        &[
            0xfd, 0xff, 3, 0, 1, b'a', 0, 0, 1, b'b', 0, 0, 3, b'f', 0, 0,
        ],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(
        lookup_string(
            &table,
            0,
            LocaleSelection {
                requested: 0,
                default_language: 3
            }
        )
        .unwrap()
        .item
        .value
        .text(),
        "f"
    );
    assert!(lookup_string(
        &table,
        1,
        LocaleSelection {
            requested: 3,
            default_language: 1
        }
    )
    .is_none());
    let fallback = lookup_string(
        &table,
        1,
        LocaleSelection {
            requested: 4,
            default_language: 1,
        },
    )
    .unwrap();
    assert_eq!(fallback.item.value.text(), "b");
    assert!(fallback.used_fallback);
    assert_eq!(fallback.language, 1);
    assert!(lookup_string(&table, 99, LocaleSelection::default()).is_none());
}

#[test]
fn routine_namespace_lookup_does_not_mask_missing_resources() {
    let private = file(vec![
        chunk(*b"BHAV", 4096, b"private"),
        chunk(*b"BHAV", 8192, b"wrong-scope"),
    ]);
    let semi = file(vec![chunk(*b"BHAV", 8192, b"semi")]);
    let global = file(vec![chunk(*b"BHAV", 256, b"global")]);
    let scopes = ResourceScope {
        private: &private,
        semiglobal: Some(&semi),
        global: Some(&global),
    };
    assert_eq!(scopes.lookup_bhav(4096).unwrap().data, b"private");
    assert_eq!(scopes.lookup_bhav(8192).unwrap().data, b"semi");
    assert_eq!(scopes.lookup_bhav(256).unwrap().data, b"global");
    assert!(scopes.lookup_bhav(257).is_none());
    assert_eq!(
        scopes
            .lookup(ResourceNamespace::Private, *b"BHAV", 8192)
            .unwrap()
            .data,
        b"wrong-scope"
    );
    let missing = ResourceScope {
        private: &private,
        semiglobal: None,
        global: Some(&global),
    };
    assert!(missing.lookup_bhav(8192).is_none());
}

#[test]
fn effective_identity_binds_all_inputs_and_retains_unknown_chunks() {
    let raw = chunk(*b"UNKN", 65535, &[255, 0, 1, 2, 3]);
    let base = ResolveRequest::new("s.iff", file(vec![bcon(4096, &[10]), raw.clone()]));
    let first = resolve_content(&base, &Limits::default()).unwrap();
    assert_eq!(
        first.source_hash,
        iff_sha256(&base.source.iff, &Limits::default()).unwrap()
    );
    assert_eq!(
        first.identity,
        resolve_content(&base, &Limits::default()).unwrap().identity
    );
    assert_eq!(find(&first.iff, *b"UNKN", 65535), &raw);
    let mut changed = base.clone();
    changed.source.iff.chunks[1].data.push(9);
    assert_ne!(
        first.identity,
        resolve_content(&changed, &Limits::default())
            .unwrap()
            .identity
    );
    let mut variants = Vec::new();
    let mut v = base.clone();
    v.locale.requested = 3;
    variants.push(v);
    let mut v = base.clone();
    v.variant = "software".into();
    variants.push(v);
    let mut v = base.clone();
    v.rights = "private-use".into();
    variants.push(v);
    let mut v = base.clone();
    v.dependencies.push(DependencyIdentity {
        name: "dep".into(),
        sha256: [1; 32],
    });
    variants.push(v);
    let mut v = base.clone();
    v.tuning.upgrades.push(TuningOverride {
        table: 4096,
        index: 0,
        value: 20,
    });
    variants.push(v);
    for changed in variants {
        assert_ne!(
            first.identity,
            resolve_content(&changed, &Limits::default())
                .unwrap()
                .identity
        );
    }
    let a = patch("a", "s.iff", false, vec![], vec![]);
    let b = patch("b", "s.iff", false, vec![], vec![]);
    let mut ab = base.clone();
    ab.patches = vec![a.clone(), b.clone()];
    let mut ba = base.clone();
    ba.patches = vec![b, a];
    let ab = resolve_content(&ab, &Limits::default()).unwrap();
    let ba = resolve_content(&ba, &Limits::default()).unwrap();
    assert_eq!(ab.iff, ba.iff);
    assert_ne!(ab.identity, ba.identity);
}

#[test]
fn bounds_apply_to_resources_patches_and_tuning() {
    let source = file(vec![bcon(4096, &[1])]);
    let limits = Limits {
        max_entries: 0,
        ..Limits::default()
    };
    assert_eq!(
        resolve_content(&ResolveRequest::new("s.iff", source.clone()), &limits)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    let limits = Limits {
        max_resource_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        resolve_content(&ResolveRequest::new("s.iff", source.clone()), &limits)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        resolve_tuning(&source, None, None, &TuningInputs::default(), &limits)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn compact_semantic_resources_share_one_retention_budget() {
    let mut payload = vec![0xfc, 0xff, 1, 40, 0];
    payload.extend([0; 120]);
    let source = file((0..24).map(|id| chunk(*b"STR#", id, &payload)).collect());
    let request = ResolveRequest::new("compact.iff", source);
    let limits = Limits {
        max_total_decoded_bytes: 8192,
        ..Limits::default()
    };
    assert!(matches!(
        resolve_content(&request, &limits),
        Err(wonderland_legacy_formats::Error {
            kind: ErrorKind::LimitExceeded,
            ..
        })
    ));
}
