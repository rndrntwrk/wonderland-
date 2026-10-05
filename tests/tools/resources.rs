use wonderland_creator::{
    sha256, Edit, ResourceDocument, ResourceGuard, ResourceOperation, ResourceTransaction,
};
use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk},
    Limits,
};
fn fixture() -> Vec<u8> {
    let mut bytes = vec![0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1";
    bytes[..magic.len()].copy_from_slice(magic);
    let bhav = vec![
        2, 128, 2, 0, 0, 1, 2, 0, 3, 0, 0x12, 0x34, 1, 0, 1, 255, 1, 2, 3, 4, 5, 6, 7, 8, 2, 0,
        254, 255, 8, 7, 6, 5, 4, 3, 2, 1, 0xab,
    ];
    for (kind, id, data) in [
        (*b"BHAV", 4096u16, bhav),
        (*b"ZZZZ", 9, vec![0, 1, 0xff, 2, 3]),
    ] {
        bytes.extend_from_slice(&kind);
        bytes.extend_from_slice(&((76 + data.len()) as u32).to_be_bytes());
        bytes.extend_from_slice(&id.to_be_bytes());
        bytes.extend_from_slice(&0x1234u16.to_be_bytes());
        bytes.extend_from_slice(&[0x7e; 64]);
        bytes.extend_from_slice(&data);
    }
    bytes
}
#[test]
fn no_op_and_branch_edits_reopen_without_unrelated_changes() {
    let original = fixture();
    let limits = Limits::default();
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    assert_eq!(doc.export(&limits).unwrap(), original);
    let key = ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    };
    let guard = doc.guard(key, &limits).unwrap();
    assert_eq!(guard.source_hash, sha256(&original));
    assert_eq!(guard.format_version, Some(0x8002));
    doc.edit(
        key,
        &guard,
        Edit::BhavBranch {
            instruction: 0,
            true_pointer: 254,
            false_pointer: 255,
        },
        &limits,
    )
    .unwrap();
    let edited = doc.export(&limits).unwrap();
    let mut expected = original.clone();
    expected[64 + 76 + 14] = 254;
    assert_eq!(edited, expected);
    let reopened = ResourceDocument::import(&edited, &limits).unwrap();
    reopened.validate(&limits).unwrap();
    assert_eq!(
        reopened
            .chunk(ChunkKey {
                kind: *b"ZZZZ",
                id: 9
            })
            .unwrap()
            .data,
        [0, 1, 255, 2, 3]
    );
}
#[test]
fn invalid_branch_hash_and_version_leave_document_unchanged() {
    let original = fixture();
    let limits = Limits::default();
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    };
    let mut guard = doc.guard(key, &limits).unwrap();
    assert!(doc
        .edit(
            key,
            &guard,
            Edit::BhavBranch {
                instruction: 0,
                true_pointer: 2,
                false_pointer: 255
            },
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), original);
    guard.source_hash = "0".repeat(64);
    assert!(doc
        .edit(
            key,
            &guard,
            Edit::BhavOperand {
                instruction: 0,
                operand: [0; 8]
            },
            &limits
        )
        .is_err());
    guard = doc.guard(key, &limits).unwrap();
    guard.format_version = Some(0x8003);
    assert!(doc
        .edit(
            key,
            &guard,
            Edit::BhavOperand {
                instruction: 0,
                operand: [0; 8]
            },
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), original);
}
#[test]
fn raw_unknown_replacement_keeps_known_chunks() {
    let limits = Limits::default();
    let original = fixture();
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    let unknown = ChunkKey {
        kind: *b"ZZZZ",
        id: 9,
    };
    let before = doc
        .chunk(ChunkKey {
            kind: *b"BHAV",
            id: 4096,
        })
        .unwrap()
        .clone();
    let guard = doc.guard(unknown, &limits).unwrap();
    doc.edit(unknown, &guard, Edit::UnknownBytes(vec![8, 7, 6]), &limits)
        .unwrap();
    let reopened = ResourceDocument::import(&doc.export(&limits).unwrap(), &limits).unwrap();
    assert_eq!(*reopened.chunk(before.key).unwrap(), before);
    assert_eq!(reopened.chunk(unknown).unwrap().data, [8, 7, 6]);
}
fn one_resource(kind: [u8; 4], data: Vec<u8>) -> Vec<u8> {
    let mut original = fixture();
    original.truncate(64);
    original.extend_from_slice(&kind);
    original.extend_from_slice(&((76 + data.len()) as u32).to_be_bytes());
    original.extend_from_slice(&1u16.to_be_bytes());
    original.extend_from_slice(&9u16.to_be_bytes());
    original.extend_from_slice(&[88; 64]);
    original.extend_from_slice(&data);
    original
}
#[test]
fn string_tuning_and_old_slot_edits_keep_other_fields_and_tail() {
    let limits = Limits::default();
    let strings = vec![
        0xfd, 0xff, 2, 0, 1, b'a', 0, b'c', 0, 2, b'b', 0, b'd', 0, 0xa3, 0xa3,
    ];
    let original = one_resource(*b"STR#", strings.clone());
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"STR#",
        id: 1,
    };
    let guard = doc.guard(key, &limits).unwrap();
    doc.edit(
        key,
        &guard,
        Edit::StringValue {
            set: 1,
            index: 0,
            value: "é".into(),
        },
        &limits,
    )
    .unwrap();
    let mut expected = strings;
    expected[10] = 0xe9;
    assert_eq!(doc.chunk(key).unwrap().data, expected);
    // Source format is Latin-1; do not silently replace unsupported Unicode.
    let guard = doc.guard(key, &limits).unwrap();
    let before = doc.export(&limits).unwrap();
    assert!(doc
        .edit(
            key,
            &guard,
            Edit::StringValue {
                set: 1,
                index: 0,
                value: "🍎".into()
            },
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), before);
    let original = one_resource(*b"BCON", vec![2, 0xa1, 1, 0, 2, 0, 0xcc]);
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"BCON",
        id: 1,
    };
    let guard = doc.guard(key, &limits).unwrap();
    doc.edit(
        key,
        &guard,
        Edit::TuningConstant {
            index: 1,
            value: 0xfedc,
        },
        &limits,
    )
    .unwrap();
    assert_eq!(
        doc.chunk(key).unwrap().data,
        [2, 0xa1, 1, 0, 0xdc, 0xfe, 0xcc]
    );
    let mut slot = Vec::new();
    slot.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    slot.extend_from_slice(&4u32.to_le_bytes());
    slot.extend_from_slice(b"TOLS");
    slot.extend_from_slice(&1u32.to_le_bytes());
    slot.extend_from_slice(&2u16.to_le_bytes());
    for f in [1f32, 2f32, 3f32] {
        slot.extend_from_slice(&f.to_le_bytes());
    }
    for v in [5i32, 6, 7, 8, -1] {
        slot.extend_from_slice(&v.to_le_bytes());
    }
    slot.push(0xab);
    let original = one_resource(*b"SLOT", slot.clone());
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"SLOT",
        id: 1,
    };
    let guard = doc.guard(key, &limits).unwrap();
    assert_eq!(guard.format_version, Some(4));
    doc.edit(
        key,
        &guard,
        Edit::SlotOffset {
            index: 0,
            offset: [1., -9.5, 3.],
        },
        &limits,
    )
    .unwrap();
    slot[22..26].copy_from_slice(&(-9.5f32).to_le_bytes());
    assert_eq!(doc.chunk(key).unwrap().data, slot);
    let guard = doc.guard(key, &limits).unwrap();
    let before = doc.export(&limits).unwrap();
    assert!(doc
        .edit(
            key,
            &guard,
            Edit::SlotOffset {
                index: 0,
                offset: [f32::NAN, 0., 0.]
            },
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), before);
}
#[test]
fn indexed_source_passthrough_and_edit_rejection_are_explicit() {
    let limits = Limits::default();
    let mut original = one_resource(*b"rsmp", vec![0x99, 0x88]);
    original[60..64].copy_from_slice(&64u32.to_be_bytes());
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    assert_eq!(doc.export(&limits).unwrap(), original);
    let key = ChunkKey {
        kind: *b"rsmp",
        id: 1,
    };
    let guard = doc.guard(key, &limits).unwrap();
    assert!(doc
        .edit(key, &guard, Edit::UnknownBytes(vec![0x22]), &limits)
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), original);
}
#[test]
fn aggregate_limits_and_resource_hash_conflicts_are_rejected() {
    let original = fixture();
    let limits = Limits::default();
    let mut doc = ResourceDocument::import(&original, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    };
    let mut guard = doc.guard(key, &limits).unwrap();
    guard.resource_hash = "a".repeat(64);
    assert!(doc
        .edit(
            key,
            &guard,
            Edit::BhavOperand {
                instruction: 0,
                operand: [0; 8]
            },
            &limits
        )
        .is_err());
    let low = Limits {
        max_total_decoded_bytes: original.len() * 2,
        ..limits
    };
    assert!(ResourceDocument::import(&original, &low).is_err());
}
#[test]
fn cfg_reports_unreachable_code_and_source_253_fallback() {
    use wonderland_creator::validate_cfg;
    use wonderland_legacy_formats::semantic::{Bhav, BhavInstruction};
    let bhav = Bhav {
        format_version: 0x8002,
        kind: 0,
        args: 0,
        locals: 0,
        tree_version: 0,
        reserved: vec![0; 2],
        trailing: vec![],
        instructions: vec![
            BhavInstruction {
                opcode: 1,
                true_pointer: 253,
                false_pointer: 254,
                operand: [0; 8],
            },
            BhavInstruction {
                opcode: 2,
                true_pointer: 253,
                false_pointer: 253,
                operand: [0; 8],
            },
        ],
    };
    let report = validate_cfg(&bhav).unwrap();
    assert_eq!(report.unreachable, [1]);
    assert_eq!(report.alternate_error_instructions, [1]);
}

#[test]
fn transaction_publishes_add_remove_and_exact_metadata_together() {
    let limits = Limits::default();
    let source = fixture();
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    let bhav_key = ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    };
    let unknown_key = ChunkKey {
        kind: *b"ZZZZ",
        id: 9,
    };
    let bhav_guard = document.guard(bhav_key, &limits).unwrap();
    let unknown_guard = document.guard(unknown_key, &limits).unwrap();
    let transaction = ResourceTransaction {
        source_hash: sha256(&source),
        operations: vec![
            ResourceOperation::Remove {
                key: unknown_key,
                expected: (&unknown_guard).into(),
            },
            ResourceOperation::Add {
                chunk: IffChunk {
                    key: ChunkKey {
                        kind: *b"BCON",
                        id: 42,
                    },
                    flags: 0xabcd,
                    label: [0xa3; 64],
                    data: vec![1, 7, 0x34, 0x12, 0xee],
                },
            },
            ResourceOperation::SetMetadata {
                key: bhav_key,
                expected: (&bhav_guard).into(),
                new_key: ChunkKey {
                    kind: *b"BHAV",
                    id: 4097,
                },
                flags: 0xfedc,
                label: [0x80; 64],
            },
        ],
    };
    document.transact(&transaction, &limits).unwrap();
    let output = document.export(&limits).unwrap();
    let mut expected = source[..64 + 76 + 37].to_vec();
    expected[72..74].copy_from_slice(&4097u16.to_be_bytes());
    expected[74..76].copy_from_slice(&0xfedcu16.to_be_bytes());
    expected[76..140].fill(0x80);
    expected.extend_from_slice(b"BCON");
    expected.extend_from_slice(&81u32.to_be_bytes());
    expected.extend_from_slice(&42u16.to_be_bytes());
    expected.extend_from_slice(&0xabcdu16.to_be_bytes());
    expected.extend_from_slice(&[0xa3; 64]);
    expected.extend_from_slice(&[1, 7, 0x34, 0x12, 0xee]);
    assert_eq!(output, expected);
    assert!(document.chunk(unknown_key).is_err());
    assert!(document.chunk(bhav_key).is_err());
    let reopened = ResourceDocument::import(&output, &limits).unwrap();
    reopened.validate(&limits).unwrap();
    assert_eq!(document.file(), reopened.file());
    assert!(
        document.transact(&transaction, &limits).is_err(),
        "replay must conflict with the new source"
    );
    assert_eq!(document.export(&limits).unwrap(), expected);
}

#[test]
fn failure_after_a_valid_candidate_edit_keeps_document_and_guards_unchanged() {
    let limits = Limits::default();
    let source = fixture();
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    let bhav_key = ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    };
    let unknown_key = ChunkKey {
        kind: *b"ZZZZ",
        id: 9,
    };
    let before = document.file().clone();
    let guard = document.guard(bhav_key, &limits).unwrap();
    let transaction = ResourceTransaction {
        source_hash: guard.source_hash.clone(),
        operations: vec![
            ResourceOperation::Edit {
                key: bhav_key,
                expected: (&guard).into(),
                edit: Edit::BhavOperand {
                    instruction: 0,
                    operand: [0; 8],
                },
            },
            ResourceOperation::SetMetadata {
                key: unknown_key,
                expected: (&document.guard(unknown_key, &limits).unwrap()).into(),
                // An existing opaque payload cannot become a malformed known resource.
                new_key: ChunkKey {
                    kind: *b"BHAV",
                    id: 20,
                },
                flags: 0,
                label: [0; 64],
            },
        ],
    };
    assert!(document.transact(&transaction, &limits).is_err());
    assert_eq!(document.file(), &before);
    assert_eq!(document.export(&limits).unwrap(), source);
    let after_guard = document.guard(bhav_key, &limits).unwrap();
    assert_eq!(after_guard.source_hash, guard.source_hash);
    assert_eq!(after_guard.resource_hash, guard.resource_hash);
    assert_eq!(after_guard.format_version, guard.format_version);
}

#[test]
fn duplicate_target_collision_stale_resource_and_version_fail_without_mutation() {
    let limits = Limits::default();
    let source = fixture();
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"BHAV",
        id: 4096,
    };
    let guard = document.guard(key, &limits).unwrap();
    let remove = ResourceOperation::Remove {
        key,
        expected: (&guard).into(),
    };
    let cases = [
        vec![remove.clone(), remove],
        vec![ResourceOperation::Remove {
            key,
            expected: ResourceGuard {
                resource_hash: "0".repeat(64),
                format_version: Some(0x8002),
            },
        }],
        vec![ResourceOperation::Remove {
            key,
            expected: ResourceGuard {
                resource_hash: guard.resource_hash.clone(),
                format_version: None,
            },
        }],
        vec![ResourceOperation::SetMetadata {
            key,
            expected: (&guard).into(),
            new_key: ChunkKey {
                kind: *b"ZZZZ",
                id: 9,
            },
            flags: 0,
            label: [0; 64],
        }],
        vec![ResourceOperation::Add {
            chunk: document.chunk(key).unwrap().clone(),
        }],
    ];
    for operations in cases {
        assert!(document
            .transact(
                &ResourceTransaction {
                    source_hash: sha256(&source),
                    operations
                },
                &limits
            )
            .is_err());
        assert_eq!(document.export(&limits).unwrap(), source);
    }
}

#[test]
fn explicit_key_swap_uses_source_guards_and_preserves_chunk_order() {
    let limits = Limits::default();
    let source = fixture();
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    // Both resources become opaque kinds; their exact payloads remain in source order.
    let keys = [
        ChunkKey {
            kind: *b"BHAV",
            id: 4096,
        },
        ChunkKey {
            kind: *b"ZZZZ",
            id: 9,
        },
    ];
    let operations = keys
        .iter()
        .map(|key| {
            let chunk = document.chunk(*key).unwrap();
            ResourceOperation::SetMetadata {
                key: *key,
                expected: (&document.guard(*key, &limits).unwrap()).into(),
                new_key: ChunkKey {
                    kind: *b"ZZZZ",
                    id: if key.id == 9 { 4096 } else { 9 },
                },
                flags: chunk.flags,
                label: chunk.label,
            }
        })
        .collect();
    document
        .transact(
            &ResourceTransaction {
                source_hash: sha256(&source),
                operations,
            },
            &limits,
        )
        .unwrap();
    assert_eq!(
        document.file().chunks[0].key,
        ChunkKey {
            kind: *b"ZZZZ",
            id: 9
        }
    );
    assert_eq!(
        document.file().chunks[1].key,
        ChunkKey {
            kind: *b"ZZZZ",
            id: 4096
        }
    );
    assert_eq!(document.file().chunks[1].data, [0, 1, 255, 2, 3]);
    assert_eq!(document.file().chunks[0].data.len(), 37);
}

#[test]
fn added_known_semantics_are_validated_and_unknown_payloads_remain_opaque() {
    let limits = Limits::default();
    let source = fixture();
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    for kind in [
        *b"BHAV", *b"BCON", *b"STR#", *b"CTSS", *b"TTAs", *b"OBJD", *b"SLOT", *b"TTAB", *b"GLOB",
        *b"PIFF", *b"PALT",
    ] {
        let operation = ResourceOperation::Add {
            chunk: IffChunk {
                key: ChunkKey { kind, id: 20 },
                flags: 0,
                label: [0; 64],
                data: vec![],
            },
        };
        assert!(
            document
                .transact(
                    &ResourceTransaction {
                        source_hash: sha256(&source),
                        operations: vec![operation]
                    },
                    &limits
                )
                .is_err(),
            "malformed known kind {kind:?} was accepted"
        );
        assert_eq!(document.export(&limits).unwrap(), source);
    }
    let payload = vec![0xff, 0, 0xfd, 0xfe];
    document
        .transact(
            &ResourceTransaction {
                source_hash: sha256(&source),
                operations: vec![ResourceOperation::Add {
                    chunk: IffChunk {
                        key: ChunkKey {
                            kind: [0xff, 0, 1, 2],
                            id: 20,
                        },
                        flags: 0,
                        label: [0xff; 64],
                        data: payload.clone(),
                    },
                }],
            },
            &limits,
        )
        .unwrap();
    assert_eq!(document.file().chunks[2].data, payload);
    assert_eq!(document.file().chunks[2].label, [0xff; 64]);
}

#[test]
fn map_resources_cannot_be_directly_authored_even_as_no_ops() {
    let limits = Limits::default();
    let mut source = one_resource(*b"rsmp", vec![0x99, 0x88]);
    source[60..64].copy_from_slice(&64u32.to_be_bytes());
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    let key = ChunkKey {
        kind: *b"rsmp",
        id: 1,
    };
    let guard = document.guard(key, &limits).unwrap();
    let chunk = document.chunk(key).unwrap().clone();
    for operation in [
        ResourceOperation::Remove {
            key,
            expected: (&guard).into(),
        },
        ResourceOperation::SetMetadata {
            key,
            expected: (&guard).into(),
            new_key: key,
            flags: chunk.flags,
            label: chunk.label,
        },
        ResourceOperation::Add {
            chunk: IffChunk {
                key: ChunkKey {
                    kind: *b"rsmp",
                    id: 2,
                },
                ..chunk
            },
        },
    ] {
        assert!(document
            .transact(
                &ResourceTransaction {
                    source_hash: guard.source_hash.clone(),
                    operations: vec![operation]
                },
                &limits
            )
            .is_err());
        assert_eq!(document.export(&limits).unwrap(), source);
    }
    document
        .transact(
            &ResourceTransaction {
                source_hash: sha256(&source),
                operations: vec![],
            },
            &limits,
        )
        .unwrap();
    assert_eq!(document.export(&limits).unwrap(), source);
}

#[test]
fn transaction_limits_are_checked_without_publishing_state() {
    let limits = Limits::default();
    let source = fixture();
    let mut document = ResourceDocument::import(&source, &limits).unwrap();
    let transaction = ResourceTransaction {
        source_hash: sha256(&source),
        operations: vec![ResourceOperation::Add {
            chunk: IffChunk {
                key: ChunkKey {
                    kind: *b"ZZZZ",
                    id: 20,
                },
                flags: 0,
                label: [0; 64],
                data: vec![0; 100],
            },
        }],
    };
    for constrained in [
        Limits {
            max_entries: 2,
            ..limits
        },
        Limits {
            max_resource_bytes: 99,
            ..limits
        },
        Limits {
            max_total_decoded_bytes: 100,
            ..limits
        },
        Limits {
            max_input_bytes: source.len(),
            ..limits
        },
    ] {
        assert!(document.transact(&transaction, &constrained).is_err());
        assert_eq!(document.export(&limits).unwrap(), source);
    }
}

#[test]
fn consecutive_indexed_transactions_refresh_writer_managed_map_and_guards() {
    let limits = Limits::default();
    for version in [0u32, 1] {
        let mut source = fixture()[..64].to_vec();
        source[60..64].copy_from_slice(&142u32.to_be_bytes());
        let mut label = [0; 64];
        label[0] = b'a';
        source.extend_from_slice(b"DATA\0\0\0\x4e\0\x01\0\x10");
        source.extend_from_slice(&label);
        source.extend_from_slice(&[0x91, 0xfe]);
        let mut map = vec![0; 4];
        map.extend_from_slice(&version.to_le_bytes());
        map.extend_from_slice(b"pmsr\0\0\0\0\x01\0\0\0ATAD\x01\0\0\0\x40\0\0\0\x01\0");
        if version == 1 {
            map.extend_from_slice(&[0, 0]);
        }
        map.extend_from_slice(&[0x10, 0]);
        map.extend_from_slice(if version == 0 { b"a\0" } else { b"\x01a" });
        source.extend_from_slice(b"rsmp");
        source.extend_from_slice(&((76 + map.len()) as u32).to_be_bytes());
        source.extend_from_slice(&[0, 0, 0, 0x10]);
        source.extend_from_slice(&[0; 64]);
        source.extend_from_slice(&map);
        let key = ChunkKey {
            kind: *b"DATA",
            id: 1,
        };
        let new_key = ChunkKey {
            kind: *b"DATA",
            id: 2,
        };
        let mut document = ResourceDocument::import(&source, &limits).unwrap();
        let guard = document.guard(key, &limits).unwrap();
        document
            .edit(
                key,
                &guard,
                Edit::UnknownBytes(vec![0x91, 0xfe, 4, 5]),
                &limits,
            )
            .unwrap();
        let grown = document.export(&limits).unwrap();
        assert_eq!(&grown[60..64], &144u32.to_be_bytes());
        assert_eq!(document.file().header[60..64], grown[60..64]);
        let guard = document.guard(key, &limits).unwrap();
        let mut renamed_label = [0; 64];
        renamed_label[..3].copy_from_slice(b"abc");
        document
            .transact(
                &ResourceTransaction {
                    source_hash: guard.source_hash.clone(),
                    operations: vec![ResourceOperation::SetMetadata {
                        key,
                        expected: (&guard).into(),
                        new_key,
                        flags: 3,
                        label: renamed_label,
                    }],
                },
                &limits,
            )
            .unwrap();
        let renamed = document.export(&limits).unwrap();
        assert_ne!(renamed, grown);
        let reopened = ResourceDocument::import(&renamed, &limits).unwrap();
        assert_eq!(document.file(), reopened.file());
        assert_eq!(document.chunk(new_key).unwrap().data, [0x91, 0xfe, 4, 5]);
        let guard = document.guard(new_key, &limits).unwrap();
        assert_eq!(guard.source_hash, sha256(&renamed));
        document
            .edit(new_key, &guard, Edit::UnknownBytes(vec![1]), &limits)
            .unwrap();
        assert_eq!(document.file().header[60..64], 141u32.to_be_bytes());
        assert_eq!(document.chunk(new_key).unwrap().label, renamed_label);
        assert_eq!(document.chunk(new_key).unwrap().flags, 3);
    }
}
