use wonderland_legacy_formats::{semantic::*, ErrorKind, Limits};

#[test]
fn bhav_versioned_headers_operands_and_raw_tail() {
    for (version, header) in [
        (0x8000u16, vec![2, 0, 11, 12, 13, 14, 15, 16, 17, 18]),
        (0x8001u16, vec![2, 0, 21, 22, 23, 24, 25, 26, 27, 28]),
        (
            0x8002u16,
            vec![2, 0, 7, 4, 0x34, 0x12, 0x78, 0x56, 0xab, 0xcd],
        ),
        (0x8003u16, vec![7, 4, 5, 0xab, 0xcd, 0x78, 0x56, 2, 0, 0, 0]),
    ] {
        let mut bytes = version.to_le_bytes().to_vec();
        bytes.extend(header);
        bytes.extend([0x34, 0x12, 1, 255, 1, 2, 3, 4, 5, 6, 7, 8]);
        bytes.extend([0, 0x20, 254, 253, 0xff, 0x80, 0, 3, 9, 8, 7, 6]);
        bytes.extend([0xde, 0xad]);
        let bhav = decode_bhav(&bytes, &Limits::default()).unwrap();
        assert_eq!(bhav.format_version, version);
        assert_eq!(bhav.instructions[0].opcode, 0x1234);
        assert_eq!(bhav.instructions[0].true_pointer, 1);
        assert_eq!(bhav.instructions[0].false_pointer, 255);
        assert_eq!(bhav.instructions[1].operand, [0xff, 0x80, 0, 3, 9, 8, 7, 6]);
        assert_eq!(bhav.trailing, [0xde, 0xad]);
        if version == 0x8002 {
            assert_eq!(bhav.locals, 0x1234);
        }
        if version == 0x8003 {
            assert_eq!(bhav.locals, 5);
        }
        assert_eq!(encode_bhav(&bhav, &Limits::default()).unwrap(), bytes);
    }
}

#[test]
fn bhav_rejects_unknown_versions_truncated_operands_and_resource_limits() {
    assert_eq!(
        decode_bhav(&[4, 0x80], &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
    let bytes = [0, 0x80, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        decode_bhav(&bytes, &Limits::default()).unwrap_err().kind,
        ErrorKind::Truncated
    );
    let limits = Limits {
        max_entries: 0,
        ..Limits::default()
    };
    assert_eq!(
        decode_bhav(&bytes, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn objd_versions_keep_reserved_words_and_signed_fields() {
    for (version, words) in [
        (136u32, 78),
        (138, 106),
        (139, 94),
        (140, 95),
        (141, 95),
        (142, 105),
    ] {
        let mut bytes = version.to_le_bytes().to_vec();
        for index in 0..words {
            bytes.extend((index as u16).to_le_bytes());
        }
        bytes[4 + 12 * 2..4 + 14 * 2].copy_from_slice(&[0x78, 0x56, 0x34, 0x12]);
        bytes[4 + 9 * 2..4 + 10 * 2].copy_from_slice(&[0xff, 0xff]);
        bytes.push(0x77);
        let value = decode_objd(&bytes, &Limits::default()).unwrap();
        assert_eq!(value.guid(), 0x12345678);
        assert_eq!(value.sub_index(), -1);
        assert_eq!(value.fields[words - 1], (words - 1) as u16);
        assert_eq!(value.trailing, [0x77]);
        assert_eq!(encode_objd(&value, &Limits::default()).unwrap(), bytes);
    }
    assert_eq!(
        decode_objd(&137u32.to_le_bytes(), &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
    assert_eq!(
        decode_objd(&136u32.to_le_bytes(), &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
}

#[test]
fn bcon_and_glob_preserve_unknown_bytes_and_encoding() {
    let bytes = [2, 0xa5, 0xff, 0xff, 0x34, 0x12, 0xde, 0xad];
    let bcon = decode_bcon(&bytes, &Limits::default()).unwrap();
    assert_eq!(bcon.constants, [65535, 0x1234]);
    assert_eq!(bcon.flags, 0xa5);
    assert_eq!(encode_bcon(&bcon, &Limits::default()).unwrap(), bytes);
    for bytes in [
        vec![3, b'f', 0x80, b'o', 0x99],
        b"semi\0tail".to_vec(),
        b"semi".to_vec(),
    ] {
        let glob = decode_glob(&bytes, &Limits::default()).unwrap();
        assert_eq!(encode_glob(&glob, &Limits::default()).unwrap(), bytes);
    }
}

#[test]
fn all_string_layouts_preserve_encoding_interleaving_and_tail() {
    let cases = [
        vec![0, 0, 1, 0, 3, b'a', 0x80, b'b', 0xa3],
        vec![0xff, 0xff, 1, 0, 0xc3, 0xa9, 0, 0xa3],
        vec![0xfe, 0xff, 1, 0, 0xe9, 0, b'c', 0, 0xa3],
        vec![
            0xfd, 0xff, 3, 0, 3, b'f', 0, 0, 0, b'e', 0, 0, 22, b'x', 0, 0, 0xa3,
        ],
        vec![0xfc, 0xff, 2, 1, 0, 0, 2, 0xc3, 0xa9, 1, b'c', 0, 0, 0x55],
    ];
    for bytes in cases {
        let strings = decode_strings(&bytes, &Limits::default()).unwrap();
        assert_eq!(encode_strings(&strings, &Limits::default()).unwrap(), bytes);
    }
    let table = decode_strings(
        &[0xfd, 0xff, 2, 0, 3, b'f', 0, 0, 0, b'e', 0, 0],
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(table.sets[2][0].value.text(), "f");
    assert_eq!(table.sets[0][0].value.text(), "e");
    for format in [-4i16, -3, -2, -1, 0] {
        let bytes = format.to_le_bytes();
        let empty = decode_strings(&bytes, &Limits::default()).unwrap();
        assert!(empty.header_only);
        assert_eq!(encode_strings(&empty, &Limits::default()).unwrap(), bytes);
    }
    let limits = Limits {
        max_string_bytes: 2,
        ..Limits::default()
    };
    assert_eq!(
        decode_strings(&[0, 0, 1, 0, 3, b'a', b'b', b'c'], &limits)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        decode_strings(&[0xfb, 0xff], &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
}

fn piff_bytes(version: u16) -> Vec<u8> {
    let mut b = version.to_le_bytes().to_vec();
    b.extend([5, b's', b'.', b'i', b'f', b'f']);
    if version > 1 {
        b.push(0);
    }
    b.extend([1, 0, b'B', b'C', b'O', b'N', 0, 0x10]);
    if version > 1 {
        b.push(0);
    }
    b.extend([0, 0, 7, 0]); // patch, unchanged label, parsed flags
    if version > 0 {
        b.extend([1, 0x10]);
    }
    b.extend(3u32.to_le_bytes());
    b.extend(2u32.to_le_bytes());
    b.extend([1, 1, 0, 0, 1, 1, b'X']); // remove at output 1, insert at output 1
    b
}

#[test]
fn piff_all_versions_use_output_relative_offsets_and_retain_metadata() {
    for version in [0, 1, 2] {
        let bytes = piff_bytes(version);
        let piff = decode_piff(&bytes, &Limits::default()).unwrap();
        assert_eq!(piff.source.text(), "s.iff");
        assert_eq!(
            apply_piff_entry(&piff.entries[0], b"abc", &Limits::default()).unwrap(),
            b"aXc"
        );
        assert_eq!(encode_piff(&piff, &Limits::default()).unwrap(), bytes);
        match &piff.entries[0].operation {
            PiffOperation::Patch { new_id, flags, .. } => {
                assert_eq!(*new_id, if version == 0 { 0x1000 } else { 0x1001 });
                assert_eq!(*flags, 7);
            }
            _ => panic!("expected patch"),
        }
    }
}

#[test]
fn piff_malformed_offsets_sizes_varints_and_modes_are_rejected() {
    let mut piff = decode_piff(&piff_bytes(2), &Limits::default()).unwrap();
    if let PiffOperation::Patch { patches, .. } = &mut piff.entries[0].operation {
        patches[0].offset = 10;
    }
    assert!(apply_piff_entry(&piff.entries[0], b"abc", &Limits::default()).is_err());
    let mut bytes = piff_bytes(2);
    *bytes.last_mut().unwrap() = b'X';
    let mode_index = bytes.len() - 2;
    bytes[mode_index] = 2;
    assert_eq!(
        decode_piff(&bytes, &Limits::default()).unwrap_err().kind,
        ErrorKind::InvalidData
    );
    assert_eq!(
        decode_piff(&[2, 0, 0xff, 0xff, 0xff, 0xff, 0x1f], &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Overflow
    );
    assert_eq!(
        decode_piff(&[3, 0], &Limits::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion
    );
    let limits = Limits {
        max_resource_bytes: 2,
        ..Limits::default()
    };
    assert_eq!(
        decode_piff(&piff_bytes(2), &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn otf_retains_order_unknown_xml_and_rejects_dtd_depth_and_limits() {
    let xml = br#"<?xml version="1.0"?><O extra="raw"><!--comment--><T i="4096" n="tune"><K i="0" l="a &amp; b" v="-2"/><K i="0" l="duplicate" v="65535"/></T></O>"#;
    let otf = decode_otf(xml, &Limits::default()).unwrap();
    assert_eq!(otf.tables[0].keys[0].label, "a & b");
    assert_eq!(otf.tables[0].keys[0].value, -2);
    assert_eq!(otf.tables[0].keys[1].value, 65535);
    assert_eq!(encode_otf(&otf, &Limits::default()).unwrap(), xml);
    assert!(decode_otf(br#"<!DOCTYPE O [<!ENTITY e "x">]><O/>"#, &Limits::default()).is_err());
    let limits = Limits {
        max_depth: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_otf(xml, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    let limits = Limits {
        max_entries: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_otf(xml, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn ttab_normal_versions_and_tsbo_flags_follow_source_layout() {
    for version in 4u16..=10 {
        for variant in [TtabVariant::Standard, TtabVariant::Tsbo] {
            let mut bytes = vec![1, 0];
            bytes.extend(version.to_le_bytes());
            if version >= 9 {
                bytes.push(0);
            }
            bytes.extend([0x34, 0x12, 0, 0x20]); // action/test
            bytes.extend(1u32.to_le_bytes()); // one motive
            bytes.extend(0x87654321u32.to_le_bytes());
            bytes.extend(42u32.to_le_bytes());
            if version > 6 {
                bytes.extend(3u32.to_le_bytes());
            }
            bytes.extend(0x80000000u32.to_le_bytes()); // negative zero float bits
            bytes.extend(9u32.to_le_bytes());
            bytes.extend((-1i32).to_le_bytes());
            if version > 6 {
                bytes.extend((-10i16).to_le_bytes());
            }
            bytes.extend((-20i16).to_le_bytes());
            if version > 6 {
                bytes.extend(0xfff0u16.to_le_bytes());
            }
            if version > 9 && variant == TtabVariant::Standard {
                bytes.extend(0xaabbccddu32.to_le_bytes());
            }
            bytes.extend([0xde, 0xad]);
            let table = decode_ttab_with_variant(&bytes, variant, &Limits::default()).unwrap();
            let i = &table.interactions[0];
            assert_eq!(i.action_function, 0x1234);
            assert_eq!(i.test_function, 0x2000);
            assert_eq!(i.attenuation_value_bits, 0x80000000);
            assert_eq!(i.joining_index, -1);
            assert_eq!(i.motives[0].delta, -20);
            assert_eq!(i.motives[0].minimum, if version > 6 { -10 } else { 0 });
            assert_eq!(
                i.flags2,
                if version > 9 && variant == TtabVariant::Standard {
                    0xaabbccdd
                } else {
                    0x1e
                }
            );
            assert_eq!(encode_ttab(&table, &Limits::default()).unwrap(), bytes);
        }
    }
}

#[test]
fn ttab_hand_checked_field_compression_and_guarded_edit() {
    // 0x81 = nonzero, width-code 0, five-bit +1 (action).
    // Seven zero fields follow, then nonzero/code0/six-bit -1 (joining).
    let bytes = [1, 0, 9, 0, 1, 0x81, 0x01, 0x3f, 0xde, 0xad];
    let mut table = decode_ttab(&bytes, &Limits::default()).unwrap();
    assert_eq!(table.interactions[0].action_function, 1);
    assert_eq!(table.interactions[0].joining_index, -1);
    assert_eq!(table.trailing, [0xde, 0xad]);
    assert_eq!(encode_ttab(&table, &Limits::default()).unwrap(), bytes);
    table.interactions[0].action_function = 0x1234;
    let encoded = encode_ttab(&table, &Limits::default()).unwrap();
    let changed = decode_ttab(&encoded, &Limits::default()).unwrap();
    assert_eq!(changed.interactions, table.interactions);
    assert_eq!(changed.trailing, table.trailing);
    assert_eq!(
        decode_ttab(&[1, 0, 3, 0], &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
    assert_eq!(
        decode_ttab(&[1, 0, 9, 0, 1], &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
}

#[test]
fn compact_strings_do_not_exceed_budget_through_vector_growth() {
    let mut bytes = vec![0xff, 0xff, 33, 0];
    bytes.extend([0; 33]);
    let limits = Limits {
        max_total_decoded_bytes: 3000,
        ..Limits::default()
    };
    let table = decode_strings(&bytes, &limits).unwrap();
    let retained = table.sets.capacity() * std::mem::size_of::<Vec<StringItem>>()
        + table
            .sets
            .iter()
            .map(|set| set.capacity() * std::mem::size_of::<StringItem>())
            .sum::<usize>();
    assert!(
        retained <= limits.max_total_decoded_bytes,
        "retained={retained}"
    );
}

#[test]
fn otf_edits_only_attribute_spans_and_preflights_aggregate_escaping() {
    let bytes = br#"<O unknown='keep'><metadata>safe &amp; text</metadata><!--retain--><T i="4096" n="table"><K i="0" l="label" v="-2" unknown="raw"/></T></O>"#;
    let mut otf = decode_otf(bytes, &Limits::default()).unwrap();
    otf.tables[0].keys[0].value = 17;
    let expected = std::str::from_utf8(bytes)
        .unwrap()
        .replace("v=\"-2\"", "v=\"17\"");
    assert_eq!(
        encode_otf(&otf, &Limits::default()).unwrap(),
        expected.as_bytes()
    );
    let xml = br#"<O><T i="4096" n="t"><K i="0" l="" v="1"/><K i="1" l="" v="1"/><K i="2" l="" v="1"/><K i="3" l="" v="1"/></T></O>"#;
    let mut otf = decode_otf(xml, &Limits::default()).unwrap();
    for key in &mut otf.tables[0].keys {
        key.label = "\"".repeat(256);
    }
    let limits = Limits {
        max_resource_bytes: 4096,
        max_total_decoded_bytes: 16000,
        max_string_bytes: 1024,
        ..Limits::default()
    };
    assert_eq!(
        encode_otf(&otf, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

/// Read-only probe: no original assets are copied, rewritten, or redistributed.
#[test]
#[ignore = "requires the source checkout corpus; run explicitly for evidence"]
fn repository_semantic_corpus_probe() {
    use std::{collections::BTreeMap, path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let baseline = "4c6b3e8f5835b228723caea3c9f683c62f244f73";
    let paths = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", baseline])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(paths.status.success());
    let mut envelope_good = 0usize;
    let mut envelope_bad = 0usize;
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for name in String::from_utf8(paths.stdout).unwrap().lines() {
        if !name.ends_with(".iff") && !name.ends_with(".piff") && !name.ends_with(".otf") {
            continue;
        }
        let blob = format!("{baseline}:{name}");
        let bytes = Command::new("git")
            .args(["show", &blob])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(bytes.status.success());
        let bytes = bytes.stdout;
        if name.ends_with(".otf") {
            let row = counts.entry("OTF".to_owned()).or_default();
            match decode_otf(&bytes, &Limits::default()) {
                Ok(_) => row.0 += 1,
                Err(e) => {
                    row.1 += 1;
                    failures.push(format!("{name}: {e}"));
                }
            }
            continue;
        }
        let file = match wonderland_legacy_formats::iff::decode(&bytes, &Limits::default()) {
            Ok(file) => {
                envelope_good += 1;
                file
            }
            Err(e) => {
                envelope_bad += 1;
                failures.push(format!("{name}: IFF {e}"));
                continue;
            }
        };
        for chunk in &file.chunks {
            let kind = String::from_utf8_lossy(&chunk.key.kind).into_owned();
            let row = counts.entry(kind).or_default();
            match decode_semantic(chunk, &Limits::default()) {
                Ok(_) => row.0 += 1,
                Err(e) => {
                    row.1 += 1;
                    failures.push(format!(
                        "{name}: {:?}/{} {e}",
                        String::from_utf8_lossy(&chunk.key.kind),
                        chunk.key.id
                    ));
                }
            }
        }
    }
    println!("envelopes: {envelope_good} accepted; {envelope_bad} rejected");
    println!("resource results (including raw unknown acceptance): {counts:?}");
    for failure in &failures {
        println!("CORPUS {failure}");
    }
    println!("total failures: {}", failures.len());
}
