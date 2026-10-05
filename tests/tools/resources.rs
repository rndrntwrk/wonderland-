use wonderland_creator::{sha256, Edit, ResourceDocument};
use wonderland_legacy_formats::{iff::ChunkKey, Limits};
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
