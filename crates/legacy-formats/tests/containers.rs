// SPDX-License-Identifier: MPL-2.0
use wonderland_legacy_formats::compression::{decompress_qfs, decompress_refpack};
use wonderland_legacy_formats::iff::{self, IffDocument};
use wonderland_legacy_formats::{dbpf, far, Compression, ContainerFormat, ResourceKey};
use wonderland_legacy_formats::{reader::Reader, ErrorKind, Limits};

#[test]
fn reader_observes_endianness_and_signed_values() {
    let mut r = Reader::new(&[0x12, 0x34, 0xfe, 0xff, 0x89, 0xab, 0xcd, 0xef]);
    assert_eq!(r.u16_be().unwrap(), 0x1234);
    assert_eq!(r.i16_le().unwrap(), -2);
    assert_eq!(r.u32_be().unwrap(), 0x89abcdef);
    assert_eq!(r.remaining(), 0);
}

#[test]
fn reader_failures_preserve_position_and_report_byte_offsets() {
    let mut r = Reader::new(&[1, 2, 3]);
    r.skip(1).unwrap();
    let err = r.u32_le().unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::Truncated, 1));
    assert_eq!(r.position(), 1);
    assert_eq!(
        r.read_bytes(usize::MAX).unwrap_err().kind,
        ErrorKind::Overflow
    );
    assert_eq!(r.seek(4).unwrap_err().kind, ErrorKind::Truncated);
    assert_eq!(r.position(), 1);
}

#[test]
fn c_strings_bound_scan_and_validate_utf8() {
    let mut r = Reader::new(b"ab\0z");
    assert_eq!(r.c_string(2).unwrap(), "ab");
    assert_eq!(r.u8().unwrap(), b'z');
    assert_eq!(
        Reader::new(b"abc\0").c_string(2).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        Reader::new(b"ab").c_string(2).unwrap_err().kind,
        ErrorKind::Truncated
    );
    assert_eq!(
        Reader::new(&[0xff, 0]).c_string(2).unwrap_err().kind,
        ErrorKind::InvalidData
    );
}

#[test]
fn configured_limits_reject_input_and_counts() {
    let limits = Limits {
        max_input_bytes: 2,
        ..Limits::default()
    };
    limits.check_input(&[0, 1]).unwrap();
    assert_eq!(
        limits.check_input(&[0, 1, 2]).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    limits.check_count(2, 2, 7, "entries").unwrap();
    let err = limits.check_count(3, 2, 7, "entries").unwrap_err();
    assert_eq!((err.kind, err.offset), (ErrorKind::LimitExceeded, 7));
}

// Source: IffFile.Read/AddChunk: 60-byte identifier, BE map pointer, then
// 76-byte chunk headers. Expectations are literal, never encoder-generated.
fn iff_fixture(version: &[u8; 3]) -> Vec<u8> {
    let mut bytes = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1".to_vec();
    bytes[9..12].copy_from_slice(version);
    bytes.resize(64, 0);
    bytes.extend_from_slice(b"XY?!\0\0\0O\x12\x34\x00\x09");
    let mut label = [0; 64];
    label[..5].copy_from_slice(&[b'a', 0, 0xff, b'z', 0]);
    bytes.extend_from_slice(&label);
    bytes.extend_from_slice(&[0x91, 0x00, 0xfe]);
    bytes
}

#[test]
fn iff_preserves_unknown_chunks_headers_labels_flags_and_data() {
    for version in [b"2.0", b"2.5"] {
        let bytes = iff_fixture(version);
        let file = iff::decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(file.chunks.len(), 1);
        assert_eq!(file.chunks[0].key.kind, *b"XY?!");
        assert_eq!(file.chunks[0].key.id, 0x1234);
        assert_eq!(file.chunks[0].flags, 9);
        assert_eq!(file.chunks[0].label[..5], [b'a', 0, 0xff, b'z', 0]);
        assert_eq!(file.chunks[0].data, [0x91, 0, 0xfe]);
        assert_eq!(iff::encode(&file, &Limits::default()).unwrap(), bytes);
    }
}

#[test]
fn iff_rejects_partial_headers_and_payloads() {
    let bytes = iff_fixture(b"2.5");
    for end in 0..bytes.len() {
        // A complete 64-byte envelope is independently a valid empty IFF.
        if end != 64 {
            assert!(
                iff::decode(&bytes[..end], &Limits::default()).is_err(),
                "accepted prefix {end}"
            );
        }
    }
}

#[test]
fn iff_rejects_duplicate_keys_bad_sizes_and_resource_limits() {
    let bytes = iff_fixture(b"2.5");
    let mut duplicate = bytes.clone();
    duplicate.extend_from_slice(&bytes[64..]);
    assert_eq!(
        iff::decode(&duplicate, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Duplicate
    );
    let mut undersized = bytes.clone();
    undersized[68..72].copy_from_slice(&75u32.to_be_bytes());
    assert_eq!(
        iff::decode(&undersized, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidData
    );
    let limits = Limits {
        max_resource_bytes: 2,
        ..Limits::default()
    };
    assert_eq!(
        iff::decode(&bytes, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn indexed_iff_retains_original_bytes_and_rejects_edits() {
    let mut bytes = iff_fixture(b"2.5");
    bytes[60..64].copy_from_slice(&64u32.to_be_bytes());
    bytes[64..68].copy_from_slice(b"rsmp");
    let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
    assert_eq!(document.encode(&Limits::default()).unwrap(), bytes);
    assert_eq!(
        iff::encode(document.file(), &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
    document.file_mut().chunks[0].data.push(1);
    assert_eq!(
        document.encode(&Limits::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion
    );
}

#[test]
fn refpack_decodes_literal_terminal_and_overlapping_backreferences() {
    let limits = Limits::default();
    assert_eq!(
        decompress_refpack(&[0xff, b'a', b'b', b'c'], 3, &limits).unwrap(),
        b"abc"
    );
    // One literal 'A', then a short distance-one overlapping copy of 3 bytes.
    assert_eq!(
        decompress_refpack(&[0x01, 0x00, b'A', 0xfc], 4, &limits).unwrap(),
        b"AAAA"
    );
    // Medium and long commands carry 1 leading literal and use distance 1.
    assert_eq!(
        decompress_refpack(&[0x80, 0x40, 0x00, b'B', 0xfc], 5, &limits).unwrap(),
        b"BBBBB"
    );
    assert_eq!(
        decompress_refpack(&[0xc1, 0, 0, 0, b'C', 0xfc], 6, &limits).unwrap(),
        b"CCCCCC"
    );
    assert_eq!(
        decompress_refpack(&[0xe0, b'w', b'x', b'y', b'z', 0xfc], 4, &limits).unwrap(),
        b"wxyz"
    );
}

#[test]
fn refpack_rejects_output_expansion_bad_references_and_trailing_data() {
    let limits = Limits::default();
    assert_eq!(
        decompress_refpack(&[0, 0, 0xfc], 3, &limits)
            .unwrap_err()
            .kind,
        ErrorKind::InvalidData
    );
    assert_eq!(
        decompress_refpack(&[0x01, 0, b'A', 0xfc], 3, &limits)
            .unwrap_err()
            .kind,
        ErrorKind::InvalidData
    );
    assert_eq!(
        decompress_refpack(&[0xff, b'a', b'b', b'c'], 4, &limits)
            .unwrap_err()
            .kind,
        ErrorKind::InvalidData
    );
    assert_eq!(
        decompress_refpack(&[0xfc, 0], 0, &limits).unwrap_err().kind,
        ErrorKind::InvalidData
    );
    let bounded = Limits {
        max_resource_bytes: 8,
        ..limits
    };
    assert_eq!(
        decompress_refpack(&[0xfc], 9, &bounded).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    assert_eq!(
        decompress_refpack(&[0xe0, 1, 2, 3, 4], 4, &bounded)
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
}

#[test]
fn qfs_uses_le_command_size_and_be_24bit_output_size() {
    // Four command bytes; 0xFB10 is stored little-endian; output = 3.
    let bytes = [4, 0, 0, 0, 0x10, 0xfb, 0, 0, 3, 0xff, b'q', b'f', b's'];
    assert_eq!(decompress_qfs(&bytes, &Limits::default()).unwrap(), b"qfs");
    for end in 0..bytes.len() {
        assert!(
            decompress_qfs(&bytes[..end], &Limits::default()).is_err(),
            "accepted QFS prefix {end}"
        );
    }
    let mut wrong_size = bytes;
    wrong_size[0] = 12; // Neither command-only nor original header-inclusive size.
    assert_eq!(
        decompress_qfs(&wrong_size, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidData
    );
}

#[test]
fn qfs_original_header_inclusive_size_decodes_without_reading_adjacent_bytes() {
    // Original hands/groups, collections and mesh archives count this header.
    let bytes = [13, 0, 0, 0, 0x10, 0xfb, 0, 0, 3, 0xff, b'q', b'f', b's'];
    assert_eq!(decompress_qfs(&bytes, &Limits::default()).unwrap(), b"qfs");
    for end in 0..bytes.len() {
        assert!(decompress_qfs(&bytes[..end], &Limits::default()).is_err());
    }
    let mut archive = far3_fixture(true);
    archive[25..29].copy_from_slice(&13u32.to_le_bytes());
    let index = far::index_v3(&archive, &Limits::default()).unwrap();
    assert_eq!(index.extract(0, &Limits::default()).unwrap(), b"abc");
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    trailing[0] = 14;
    assert!(decompress_qfs(&trailing, &Limits::default()).is_err());
}

fn far1_fixture(variant: far::Far1Variant) -> Vec<u8> {
    let mut bytes = b"FAR!byAZ\x01\0\0\0\x13\0\0\0abc\x01\0\0\0".to_vec();
    bytes.extend_from_slice(&[3, 0, 0, 0, 3, 0, 0, 0, 16, 0, 0, 0]);
    bytes.extend_from_slice(if variant == far::Far1Variant::A {
        &[2, 0, 0, 0]
    } else {
        &[2, 0]
    });
    bytes.extend_from_slice(b"f1");
    bytes
}

#[test]
fn far1_variants_are_explicit_and_extract_bounded_raw_entries() {
    for variant in [far::Far1Variant::A, far::Far1Variant::B] {
        let bytes = far1_fixture(variant);
        let index = far::index_v1(&bytes, variant, &Limits::default()).unwrap();
        assert_eq!(index.entries().len(), 1);
        assert_eq!(
            index.entries()[0].key,
            ResourceKey::Far1 {
                name: b"f1".to_vec()
            }
        );
        assert_eq!(index.extract(0, &Limits::default()).unwrap(), b"abc");
        assert_eq!(
            index.extract(1, &Limits::default()).unwrap_err().kind,
            ErrorKind::InvalidData
        );
        let limited = Limits {
            max_resource_bytes: 2,
            ..Limits::default()
        };
        assert_eq!(
            index.extract(0, &limited).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
        for end in 0..bytes.len() {
            assert!(
                far::index_v1(&bytes[..end], variant, &Limits::default()).is_err(),
                "accepted FAR1 prefix {end}"
            );
        }
    }
}

#[test]
fn far1_rejects_header_manifest_and_entry_overlap() {
    let original = far1_fixture(far::Far1Variant::B);
    let mut header = original.clone();
    header[31..35].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(
        far::index_v1(&header, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Overlap
    );
    let mut manifest = original.clone();
    manifest[31..35].copy_from_slice(&20u32.to_le_bytes());
    assert_eq!(
        far::index_v1(&manifest, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Overlap
    );
    let mut bad_index = original.clone();
    bad_index[12..16].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(
        far::index_v1(&bad_index, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Overlap
    );
    let mut duplicate = original.clone();
    duplicate[19..23].copy_from_slice(&2u32.to_le_bytes());
    duplicate.extend_from_slice(&original[23..]);
    assert_eq!(
        far::index_v1(&duplicate, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Duplicate
    );
    *duplicate.last_mut().unwrap() = b'2';
    assert_eq!(
        far::index_v1(&duplicate, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Overlap
    );
}

fn far3_fixture(compressed: bool) -> Vec<u8> {
    let mut bytes = b"FAR!byAZ\x03\0\0\0".to_vec();
    bytes.extend_from_slice(&(if compressed { 38u32 } else { 19u32 }).to_le_bytes());
    if compressed {
        bytes.extend_from_slice(&[0; 9]); // Opaque source Persist prefix.
        bytes.extend_from_slice(&[4, 0, 0, 0, 0x10, 0xfb, 0, 0, 3, 0xff, b'a', b'b', b'c']);
    } else {
        bytes.extend_from_slice(b"abc");
    }
    bytes.extend_from_slice(&[1, 0, 0, 0, 3, 0, 0, 0]);
    bytes.extend_from_slice(&[
        if compressed { 22 } else { 3 },
        0,
        0,
        if compressed { 0x80 } else { 0 },
    ]);
    bytes.extend_from_slice(&[16, 0, 0, 0, compressed as u8, 0x55, 2, 0]);
    bytes.extend_from_slice(&[4, 3, 2, 1, 0xd4, 0xc3, 0xb2, 0xa1]);
    bytes.extend_from_slice(b"f3");
    bytes
}

#[test]
fn far3_qfs_command_length_matches_pinned_csharp_reader() {
    // FAR3Archive.GetEntry reads Filesize bytes AFTER the 18-byte Persist/QFS
    // header. Compiling the unchanged pinned C# reader decodes this literal
    // 73-byte vector as "abcdefg". The inner 9 means nine command bytes.
    let bytes = [
        0x46, 0x41, 0x52, 0x21, 0x62, 0x79, 0x41, 0x5a, 0x03, 0, 0, 0, 0x2b, 0, 0, 0, 0x01, 0x07,
        0, 0, 0, 0x09, 0, 0, 0, 0x09, 0, 0, 0, 0x10, 0xfb, 0, 0, 0x07, 0xe0, b'a', b'b', b'c',
        b'd', 0xff, b'e', b'f', b'g', 0x01, 0, 0, 0, 0x07, 0, 0, 0, 0x1b, 0, 0, 0x80, 0x10, 0, 0,
        0, 0x01, 0, 0x02, 0, 0x04, 0x03, 0x02, 0x01, 0xd4, 0xc3, 0xb2, 0xa1, b'f', b'3',
    ];
    let limits = Limits::default();
    let index = far::index_v3(&bytes, &limits).unwrap();
    assert_eq!(index.extract(0, &limits).unwrap(), b"abcdefg");
}

#[test]
fn far3_preserves_type_and_file_key_and_lazily_decodes_persist_qfs() {
    for compressed in [false, true] {
        let bytes = far3_fixture(compressed);
        let index = far::index_v3(&bytes, &Limits::default()).unwrap();
        assert_eq!(index.format(), ContainerFormat::Far3);
        assert_eq!(
            index.entries()[0].key,
            ResourceKey::Far3 {
                type_id: 0x01020304,
                file_id: 0xa1b2c3d4
            }
        );
        assert_eq!(index.entries()[0].name.as_deref(), Some(&b"f3"[..]));
        assert_eq!(index.extract(0, &Limits::default()).unwrap(), b"abc");
        for end in 0..bytes.len() {
            assert!(
                far::index_v3(&bytes[..end], &Limits::default()).is_err(),
                "accepted FAR3 prefix {end}"
            );
        }
    }
    let mut corrupt = far3_fixture(true);
    corrupt[34] = 0; // QFS command becomes an invalid first backreference.
    let index = far::index_v3(&corrupt, &Limits::default()).unwrap();
    assert_eq!(
        index.extract(0, &Limits::default()).unwrap_err().kind,
        ErrorKind::InvalidData
    );
}

#[test]
fn far3_uses_payload_signature_for_flagged_raw_entries_and_ignores_data_type() {
    // FAR3Archive.GetEntry checks IsCompressed and then the payload signature.
    // Original bindings are raw resources even though IsCompressed is one.
    for data_type in [0, 1, 0x80, 0xff] {
        let mut raw = far3_fixture(false);
        let record = 19 + 4;
        raw[record + 7] = data_type;
        raw[record + 12] = 1;
        let index = far::index_v3(&raw, &Limits::default()).unwrap();
        assert_eq!(index.entries()[0].compression, Compression::None);
        assert_eq!(index.extract(0, &Limits::default()).unwrap(), b"abc");

        let mut qfs = far3_fixture(true);
        qfs[38 + 4 + 7] = data_type;
        let index = far::index_v3(&qfs, &Limits::default()).unwrap();
        assert_eq!(index.entries()[0].compression, Compression::Far3PersistQfs);
        assert_eq!(index.extract(0, &Limits::default()).unwrap(), b"abc");
    }
}

#[test]
fn far_rejects_unsupported_versions_counts_and_compression() {
    let mut bytes = far1_fixture(far::Far1Variant::B);
    bytes[8] = 2;
    assert_eq!(
        far::index_v1(&bytes, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
    bytes[8] = 1;
    bytes[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        far::index_v1(&bytes, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    bytes = far1_fixture(far::Far1Variant::B);
    bytes[27] = 2;
    assert_eq!(
        far::index_v1(&bytes, far::Far1Variant::B, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
}

fn dbpf_fixture(major: u32, minor: u32) -> Vec<u8> {
    // These three literal layouts follow DBPFFile.Read's exact source branches.
    let header_size: u32 = if major == 2 {
        76
    } else if minor == 0 {
        96
    } else {
        88
    };
    let mut bytes = b"DBPF".to_vec();
    bytes.extend_from_slice(&major.to_le_bytes());
    bytes.extend_from_slice(&minor.to_le_bytes());
    bytes.extend_from_slice(&[0; 12]);
    if major == 1 && minor == 0 {
        bytes.extend_from_slice(&[0; 8]);
    }
    if major < 2 {
        bytes.extend_from_slice(&7u32.to_le_bytes());
    }
    bytes.extend_from_slice(&1u32.to_le_bytes());
    if major < 2 {
        bytes.extend_from_slice(&(header_size + 3).to_le_bytes());
    }
    bytes.extend_from_slice(&20u32.to_le_bytes());
    if major < 2 {
        bytes.extend_from_slice(&[0; 16]);
    } else {
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&(header_size + 3).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
    }
    bytes.extend_from_slice(&[0; 32]);
    bytes.extend_from_slice(b"dbp");
    bytes.extend_from_slice(&[
        0x44, 0x33, 0x22, 0x11, 0x88, 0x77, 0x66, 0x55, 0xcc, 0xbb, 0xaa, 0x99,
    ]);
    bytes.extend_from_slice(&header_size.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes
}

#[test]
fn dbpf_keeps_full_tgi_identity_and_source_compatibility_is_explicit() {
    let limits = Limits::default();
    let bytes = dbpf_fixture(1, 0);
    let index = dbpf::index(&bytes, &limits).unwrap();
    assert_eq!(
        index.entries()[0].key,
        ResourceKey::Dbpf {
            type_id: 0x11223344,
            group_id: 0x55667788,
            instance_id: 0x99aabbcc
        }
    );
    assert_eq!(index.extract(0, &limits).unwrap(), b"dbp");
    for (major, minor) in [(1, 0), (1, 1), (2, 0)] {
        let bytes = dbpf_fixture(major, minor);
        let index = dbpf::index_source_compatible(&bytes, &limits).unwrap();
        assert_eq!(index.extract(0, &limits).unwrap(), b"dbp");
        if (major, minor) != (1, 0) {
            assert_eq!(
                dbpf::index(&bytes, &limits).unwrap_err().kind,
                ErrorKind::UnsupportedVersion
            );
        }
        for end in 0..bytes.len() {
            assert!(
                dbpf::index_source_compatible(&bytes[..end], &limits).is_err(),
                "accepted DBPF {major}.{minor} prefix {end}"
            );
        }
    }
}

#[test]
fn dbpf_rejects_header_index_payload_overlap_and_wrong_index_layout() {
    let original = dbpf_fixture(1, 0);
    let mut bytes = original.clone();
    bytes[40..44].copy_from_slice(&32u32.to_le_bytes());
    assert_eq!(
        dbpf::index(&bytes, &Limits::default()).unwrap_err().kind,
        ErrorKind::Overlap
    );
    bytes = original.clone();
    bytes[111..115].copy_from_slice(&100u32.to_le_bytes());
    assert_eq!(
        dbpf::index(&bytes, &Limits::default()).unwrap_err().kind,
        ErrorKind::Overlap
    );
    bytes = original.clone();
    bytes[111..115].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(
        dbpf::index(&bytes, &Limits::default()).unwrap_err().kind,
        ErrorKind::Overlap
    );
    bytes = original;
    bytes[60..64].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        dbpf::index(&bytes, &Limits::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion
    );
}
