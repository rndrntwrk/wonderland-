use wonderland_legacy_formats::{
    iff::{ChunkKey, IffChunk},
    semantic::{decode_objf, decode_semantic, encode_objf, DecodedSemantic, ObjfFunction},
    ErrorKind, Limits,
};

// Literal little-endian source layout: pad, version, fJBO, count,
// then condition/action pairs. The condition must not become the action.
const TABLE: &[u8] = &[
    0x78, 0x56, 0x34, 0x12, 7, 0, 0, 0, b'f', b'J', b'B', b'O', 2, 0, 0, 0, 0x01, 0x10, 0x02, 0x10,
    0xff, 0xff, 0x03, 0x20, 0xde, 0xad,
];

#[test]
fn objf_is_decoded_as_semantic_content_for_critical_cooking() {
    let chunk = IffChunk {
        key: ChunkKey {
            kind: *b"OBJf",
            id: 1,
        },
        flags: 0,
        label: [0; 64],
        data: TABLE.to_vec(),
    };
    assert!(!matches!(
        decode_semantic(&chunk, &Limits::default()).unwrap(),
        DecodedSemantic::Unknown { .. }
    ));
}

#[test]
fn objf_preserves_source_words_condition_action_order_and_tail() {
    let table = decode_objf(TABLE, &Limits::default()).unwrap();
    assert_eq!(table.padding, 0x12345678);
    assert_eq!(table.version, 7);
    assert_eq!(
        table.functions,
        [
            ObjfFunction {
                condition: 0x1001,
                action: 0x1002
            },
            ObjfFunction {
                condition: 0xffff,
                action: 0x2003
            },
        ]
    );
    assert_eq!(table.trailing, [0xde, 0xad]);
    assert_eq!(encode_objf(&table, &Limits::default()).unwrap(), TABLE);
    assert_eq!(
        DecodedSemantic::Objf(table).retained_heap_bytes().unwrap(),
        10
    );
}

#[test]
fn objf_edits_only_requested_function_and_round_trips_empty_tables() {
    let mut table = decode_objf(TABLE, &Limits::default()).unwrap();
    table.functions[1].action = 0x3210;
    let output = encode_objf(&table, &Limits::default()).unwrap();
    assert_eq!(&output[..22], &TABLE[..22]);
    assert_eq!(&output[22..], &[0x10, 0x32, 0xde, 0xad]);
    let empty = [
        0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, b'f', b'J', b'B', b'O', 0, 0, 0, 0,
    ];
    let table = decode_objf(
        &empty,
        &Limits {
            max_entries: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    assert!(table.functions.is_empty());
    assert_eq!(encode_objf(&table, &Limits::default()).unwrap(), empty);
}

#[test]
fn objf_rejects_bad_magic_and_truncation_before_allocating_entries() {
    for length in 0..24 {
        assert!(
            decode_objf(&TABLE[..length], &Limits::default()).is_err(),
            "length {length}"
        );
    }
    let mut corrupt = TABLE.to_vec();
    corrupt[8] = b'x';
    assert_eq!(
        decode_objf(&corrupt, &Limits::default()).unwrap_err().kind,
        ErrorKind::InvalidData
    );
    corrupt[8] = b'f';
    corrupt[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_objf(&corrupt, &Limits::default()).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn objf_checks_entry_resource_input_and_allocation_limits() {
    let table = decode_objf(TABLE, &Limits::default()).unwrap();
    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_objf(TABLE, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        encode_objf(&table, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    for limits in [
        Limits {
            max_input_bytes: TABLE.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_resource_bytes: TABLE.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_total_decoded_bytes: TABLE.len() - 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            decode_objf(TABLE, &limits).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
    }
    for limits in [
        Limits {
            max_resource_bytes: TABLE.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_total_decoded_bytes: TABLE.len() - 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            encode_objf(&table, &limits).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
    }
    let exact = Limits {
        max_input_bytes: TABLE.len(),
        max_resource_bytes: TABLE.len(),
        max_total_decoded_bytes: TABLE.len(),
        ..Limits::default()
    };
    assert_eq!(
        encode_objf(&decode_objf(TABLE, &exact).unwrap(), &exact).unwrap(),
        TABLE
    );
}
