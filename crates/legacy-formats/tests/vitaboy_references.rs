use wonderland_legacy_formats::{vitaboy::*, ErrorKind, Limits};

#[test]
fn purchasable_keeps_original_big_endian_id_and_ignored_prefix() {
    let bytes = [
        0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 8, 0xde, 0xad, 0xbe, 0xef, 1, 2, 3, 4, 5, 6, 7, 8,
    ];
    let p = decode_purchasable_outfit(&bytes, &Limits::default()).unwrap();
    assert_eq!(p.version, 7);
    assert_eq!(p.asset_prefix, [0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(p.outfit_id, 0x0102030405060708);
    assert_eq!(
        encode_purchasable_outfit(&p, &Limits::default()).unwrap(),
        bytes
    );
    for i in 0..bytes.len() {
        assert!(decode_purchasable_outfit(&bytes[..i], &Limits::default()).is_err());
    }
}

#[test]
fn collection_retains_negative_duplicate_indices_and_source_identifier_order() {
    let bytes = [
        0, 0, 0, 2, 255, 255, 255, 255, 0, 0, 0, 11, 0, 0, 0, 12, 255, 255, 255, 255, 0, 0, 0, 21,
        0, 0, 0, 22,
    ];
    let c = decode_collection(&bytes, &Limits::default()).unwrap();
    assert_eq!(
        c.items.iter().map(|i| i.index).collect::<Vec<_>>(),
        [-1, -1]
    );
    assert_eq!(
        c.items[1].outfit,
        FileKey {
            file_id: 21,
            type_id: 22
        }
    );
    assert_eq!(encode_collection(&c, &Limits::default()).unwrap(), bytes);
    assert!(decode_collection(&[255, 255, 255, 255], &Limits::default()).is_err());
    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_collection(&bytes, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    assert!(encode_collection(&c, &limits).is_err());
}

#[test]
fn hand_group_preserves_skin_hand_gesture_order_and_requires_exact_layout() {
    let mut bytes = vec![0, 0, 0, 1];
    for i in 0..18u32 {
        bytes.extend((100 + i).to_be_bytes());
        bytes.extend((200 + i).to_be_bytes());
    }
    let h = decode_hand_group(&bytes, &Limits::default()).unwrap();
    assert_eq!(
        h.appearances[3],
        FileKey {
            file_id: 103,
            type_id: 203
        }
    );
    assert_eq!(
        h.appearances[17],
        FileKey {
            file_id: 117,
            type_id: 217
        }
    );
    assert_eq!(encode_hand_group(&h, &Limits::default()).unwrap(), bytes);
    bytes.push(0);
    assert!(decode_hand_group(&bytes, &Limits::default()).is_err());
}
