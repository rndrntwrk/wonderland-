// SPDX-License-Identifier: MPL-2.0
use wonderland_legacy_formats::iff::{self, ChunkKey, IffChunk, IffDocument};
use wonderland_legacy_formats::{ErrorKind, Limits};

// Independently assembled from srcs.zip:mk_iff.cpp:374-406 and
// srcs.zip:iff.cpp:98-111,174-202 (under Other/tools/Iffinator/Iffinator).
// Envelope offsets are 64, 142, 221. v0 has 60 map bytes; v1 has 63.
fn source(version: u32) -> Vec<u8> {
    let mut out = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1".to_vec();
    out.resize(60, 0);
    out.extend_from_slice(&221u32.to_be_bytes());
    out.extend(chunk(b"XY?!", 0x1234, 9, b"A", &[0x91, 0xfe]));
    // Bytes beyond the visible NUL label are opaque, and must survive edits.
    out[78] = 0xff;
    out.extend(chunk(b"DATA", 2, 0x10, b"beta", &[1, 2, 3]));
    let mut map = vec![0, 0, 0, 0];
    map.extend_from_slice(&version.to_le_bytes());
    map.extend_from_slice(b"pmsr");
    map.extend_from_slice(&(if version == 0 { 136u32 } else { 139 }).to_le_bytes());
    map.extend_from_slice(&2u32.to_le_bytes());
    map.extend_from_slice(b"!?YX\x01\0\0\0\x40\0\0\0\x34\x12");
    if version == 1 {
        map.extend_from_slice(&[0, 0]);
    }
    map.extend_from_slice(&[9, 0]);
    if version == 0 {
        map.extend_from_slice(b"A\0");
    } else {
        map.extend_from_slice(b"\x01A");
    }
    map.extend_from_slice(b"ATAD\x01\0\0\0\x8e\0\0\0\x02\0");
    if version == 1 {
        map.extend_from_slice(&[0, 0]);
    }
    map.extend_from_slice(&[0x10, 0]);
    if version == 0 {
        map.extend_from_slice(b"beta\0\xa3");
    } else {
        map.extend_from_slice(b"\x04beta");
    }
    out.extend(chunk(b"rsmp", 0, 0x10, b"map", &map));
    out
}

fn chunk(kind: &[u8; 4], id: u16, flags: u16, label: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend_from_slice(&((76 + data.len()) as u32).to_be_bytes());
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&flags.to_be_bytes());
    let mut name = [0; 64];
    name[..label.len()].copy_from_slice(label);
    out.extend_from_slice(&name);
    out.extend_from_slice(data);
    out
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[test]
fn supported_maps_keep_exact_original_and_only_change_requested_same_size_payload() {
    for version in [0, 1] {
        let bytes = source(version);
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(document.encode(&Limits::default()).unwrap(), bytes);
        document.file_mut().chunks[0].data[0] = 0x77;
        let mut expected = bytes;
        expected[140] = 0x77;
        assert_eq!(document.encode(&Limits::default()).unwrap(), expected);
    }
}

#[test]
fn payload_growth_relocates_header_and_entries_without_changing_unrelated_bytes() {
    for version in [0, 1] {
        let bytes = source(version);
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        document.file_mut().chunks[0]
            .data
            .extend_from_slice(&[4, 5]);
        let mut expected = bytes.clone();
        expected.splice(142..142, [4, 5]);
        expected[60..64].copy_from_slice(&223u32.to_be_bytes());
        expected[68..72].copy_from_slice(&80u32.to_be_bytes());
        let entry_offset = 299 + if version == 0 { 46 } else { 48 };
        expected[entry_offset..entry_offset + 4].copy_from_slice(&144u32.to_le_bytes());
        let actual = document.encode(&Limits::default()).unwrap();
        assert_eq!(actual, expected);
        let reopened = IffDocument::decode(&actual, &Limits::default()).unwrap();
        assert_eq!(reopened.encode(&Limits::default()).unwrap(), actual);
    }
}

#[test]
fn insertion_adds_type_and_entry_and_updates_both_map_sizes() {
    let bytes = source(0);
    let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
    let inserted = IffChunk {
        key: ChunkKey {
            kind: *b"MORE",
            id: 7,
        },
        flags: 0x40,
        label: {
            let mut label = [0; 64];
            label[..3].copy_from_slice(b"new");
            label
        },
        data: vec![0x99],
    };
    document.file_mut().chunks.insert(2, inserted);
    let actual = document.encode(&Limits::default()).unwrap();
    assert_eq!(be32(&actual, 60), 298);
    assert_eq!(&actual[64..221], &bytes[64..221]);
    assert_eq!(be32(&actual, 302), 156);
    assert_eq!(le32(&actual, 386), 156);
    assert_eq!(le32(&actual, 390), 3);
    assert_eq!(
        &actual[434..454],
        b"EROM\x01\0\0\0\xdd\0\0\0\x07\0\x40\0new\0"
    );
    assert_eq!(actual.len(), 454);
}

#[test]
fn deletion_removes_empty_type_and_preserves_remaining_chunk() {
    let bytes = source(0);
    let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
    document.file_mut().chunks.remove(0);
    let actual = document.encode(&Limits::default()).unwrap();
    assert_eq!(be32(&actual, 60), 143);
    assert_eq!(&actual[64..143], &bytes[142..221]);
    assert_eq!(be32(&actual, 147), 118);
    assert_eq!(le32(&actual, 231), 118);
    assert_eq!(le32(&actual, 235), 1);
    assert_eq!(
        &actual[239..261],
        b"ATAD\x01\0\0\0\x40\0\0\0\x02\0\x10\0beta\0\xa3"
    );
}

#[test]
fn reordering_including_map_location_recomputes_all_chunk_offsets() {
    let bytes = source(0);
    let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
    document.file_mut().chunks.reverse();
    let actual = document.encode(&Limits::default()).unwrap();
    assert_eq!(be32(&actual, 60), 64);
    assert_eq!(le32(&actual, 168), 279);
    assert_eq!(le32(&actual, 186), 200);
    assert_eq!(&actual[200..279], &bytes[142..221]);
    assert_eq!(&actual[279..357], &bytes[64..142]);
    assert_eq!(actual.len(), 357);
}

#[test]
fn raw_indexed_encoder_stays_closed_without_source_evidence() {
    let file = iff::decode(&source(0), &Limits::default()).unwrap();
    assert_eq!(
        iff::encode(&file, &Limits::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion
    );
}

#[test]
fn original_map_entries_must_match_actual_chunk_boundaries_ids_flags_and_labels() {
    for (at, patch, kind) in [
        (297, vec![1, 0, 0, 0], ErrorKind::UnsupportedVersion),
        (301, vec![2, 0, 0, 0], ErrorKind::UnsupportedVersion),
        (305, b"rsmp".to_vec(), ErrorKind::InvalidMagic),
        (309, vec![137, 0, 0, 0], ErrorKind::UnsupportedVersion),
        (325, vec![65, 0, 0, 0], ErrorKind::InvalidData),
        (325, vec![142, 0, 0, 0], ErrorKind::InvalidData),
        (329, vec![0x35, 0x12], ErrorKind::InvalidData),
        (331, vec![8, 0], ErrorKind::InvalidData),
        (333, vec![b'B'], ErrorKind::InvalidData),
    ] {
        let mut bytes = source(0);
        bytes[at..at + patch.len()].copy_from_slice(&patch);
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        // Even unsupported maps remain exact when untouched.
        assert_eq!(document.encode(&Limits::default()).unwrap(), bytes);
        document.file_mut().chunks[0].data[0] ^= 1;
        assert_eq!(
            document.encode(&Limits::default()).unwrap_err().kind,
            kind,
            "mutation at {at}"
        );
    }
}

#[test]
fn hostile_map_counts_are_bounded_before_iteration_or_allocation() {
    for at in [313, 321] {
        let mut bytes = source(0);
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        document.file_mut().chunks[0].data[0] ^= 1;
        assert_eq!(
            document.encode(&Limits::default()).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
    }
}

#[test]
fn direct_map_or_header_mutation_and_index_removal_are_not_implicit_repairs() {
    for edit in 0..4 {
        let mut document = IffDocument::decode(&source(0), &Limits::default()).unwrap();
        match edit {
            0 => document.file_mut().chunks[2].data[0] = 1,
            1 => document.file_mut().header[60..64].copy_from_slice(&0u32.to_be_bytes()),
            2 => {
                document.file_mut().chunks.pop();
            }
            _ => document.file_mut().chunks[2].key.id = 1,
        }
        assert_eq!(
            document.encode(&Limits::default()).unwrap_err().kind,
            ErrorKind::UnsupportedVersion
        );
    }
}

#[test]
fn rebuilding_observes_output_limits_after_map_growth() {
    let bytes = source(0);
    let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
    document.file_mut().chunks[0].label = {
        let mut name = [0; 64];
        name[..4].copy_from_slice(b"long");
        name
    };
    // Changing a fixed-width chunk label leaves the input envelope length
    // unchanged, but expands the variable-width map label by four bytes.
    for limits in [
        Limits {
            max_input_bytes: 357,
            ..Limits::default()
        },
        Limits {
            max_resource_bytes: 60,
            ..Limits::default()
        },
        Limits {
            max_total_decoded_bytes: 65,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            document.encode(&limits).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
    }
    let exact = Limits {
        max_input_bytes: 361,
        max_resource_bytes: 64,
        ..Limits::default()
    };
    assert_eq!(document.encode(&exact).unwrap().len(), 361);
}

#[test]
fn source_and_edited_map_names_observe_string_byte_limits() {
    for version in [0, 1] {
        let mut document = IffDocument::decode(&source(version), &Limits::default()).unwrap();
        document.file_mut().chunks[0].data[0] ^= 1;
        let source_bound = Limits {
            max_string_bytes: 3,
            ..Limits::default()
        };
        assert_eq!(
            document.encode(&source_bound).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
        document.file_mut().chunks[0].label = {
            let mut name = [0; 64];
            name[..5].copy_from_slice(b"longe");
            name
        };
        let edit_bound = Limits {
            max_string_bytes: 4,
            ..Limits::default()
        };
        assert_eq!(
            document.encode(&edit_bound).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
    }
}

#[test]
fn full_width_byte_labels_rekeys_and_flags_are_encoded_without_text_conversion() {
    for version in [0, 1] {
        let mut document = IffDocument::decode(&source(version), &Limits::default()).unwrap();
        let resource = &mut document.file_mut().chunks[0];
        resource.label = [0xe9; 64];
        resource.key.id = 0x9abc;
        resource.flags = 0x4321;
        let actual = document.encode(&Limits::default()).unwrap();
        let map = &actual[297..];
        assert_eq!(&map[32..34], &[0xbc, 0x9a]);
        if version == 0 {
            assert_eq!(&map[34..36], &[0x21, 0x43]);
            assert_eq!(&map[36..100], &[0xe9; 64]);
            assert_eq!(&map[100..102], &[0, 0]);
            assert_eq!(map.len(), 124);
        } else {
            assert_eq!(&map[34..38], &[0, 0, 0x21, 0x43]);
            assert_eq!(map[38], 64);
            assert_eq!(&map[39..103], &[0xe9; 64]);
            assert_eq!(map.len(), 126);
        }
        // Validate the emitted name as source evidence for a second edit.
        let mut reopened = IffDocument::decode(&actual, &Limits::default()).unwrap();
        reopened.file_mut().chunks[0].data[0] ^= 1;
        reopened.encode(&Limits::default()).unwrap();
    }
}

#[test]
fn recognized_size_conventions_survive_map_growth() {
    for source_size in [0u32, 48, 136] {
        let mut bytes = source(0);
        bytes[309..313].copy_from_slice(&source_size.to_le_bytes());
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        document.file_mut().chunks[0].label[..3].copy_from_slice(b"new");
        document.file_mut().chunks[0].label[3] = 0;
        let actual = document.encode(&Limits::default()).unwrap();
        assert_eq!(
            le32(&actual, 309),
            match source_size {
                0 => 0,
                48 => 50,
                _ => 138,
            }
        );
        assert_eq!(be32(&actual, 225), 138);
    }
}

#[test]
fn truncated_or_trailing_map_data_is_never_repaired_during_an_edit() {
    let bytes = source(0);
    for kept in 0..60 {
        let mut damaged = bytes[..297 + kept].to_vec();
        damaged[225..229].copy_from_slice(&((76 + kept) as u32).to_be_bytes());
        if kept >= 16 {
            damaged[309..313].fill(0);
        }
        let mut document = IffDocument::decode(&damaged, &Limits::default()).unwrap();
        assert_eq!(document.encode(&Limits::default()).unwrap(), damaged);
        document.file_mut().chunks[0].data[0] ^= 1;
        assert!(
            document.encode(&Limits::default()).is_err(),
            "accepted {kept} map bytes"
        );
    }
    let mut trailing = bytes;
    trailing.push(0);
    trailing[225..229].copy_from_slice(&137u32.to_be_bytes());
    trailing[309..313].fill(0);
    let mut document = IffDocument::decode(&trailing, &Limits::default()).unwrap();
    document.file_mut().chunks[0].data[0] ^= 1;
    assert_eq!(
        document.encode(&Limits::default()).unwrap_err().kind,
        ErrorKind::InvalidData
    );
}

#[test]
fn maps_reject_duplicate_groups_duplicate_entries_missing_resources_and_self_entries() {
    let bytes = source(0);
    let mut duplicate_group = bytes.clone();
    duplicate_group[335..339].copy_from_slice(b"!?YX");

    let mut duplicate_entry = bytes[..335].to_vec();
    duplicate_entry.extend_from_slice(&bytes[325..335]);
    duplicate_entry[225..229].copy_from_slice(&124u32.to_be_bytes());
    duplicate_entry[309..313].fill(0);
    duplicate_entry[313..317].copy_from_slice(&1u32.to_le_bytes());
    duplicate_entry[321..325].copy_from_slice(&2u32.to_le_bytes());

    let mut missing = bytes[..335].to_vec();
    missing[225..229].copy_from_slice(&114u32.to_be_bytes());
    missing[309..313].fill(0);
    missing[313..317].copy_from_slice(&1u32.to_le_bytes());

    let mut self_entry = bytes;
    self_entry[317..321].copy_from_slice(b"pmsr");
    for (damaged, kind) in [
        (duplicate_group, ErrorKind::Duplicate),
        (duplicate_entry, ErrorKind::Duplicate),
        (missing, ErrorKind::InvalidData),
        (self_entry, ErrorKind::InvalidData),
    ] {
        let mut document = IffDocument::decode(&damaged, &Limits::default()).unwrap();
        document.file_mut().chunks[0].data[0] ^= 1;
        assert_eq!(document.encode(&Limits::default()).unwrap_err().kind, kind);
    }
}

#[test]
fn v1_rejects_nonzero_high_id_and_ambiguous_counted_labels() {
    for (at, value, kind) in [
        (331, 1, ErrorKind::UnsupportedVersion),
        (335, 65, ErrorKind::InvalidData),
        (336, 0, ErrorKind::InvalidData),
    ] {
        let mut bytes = source(1);
        bytes[at] = value;
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        document.file_mut().chunks[0].data[0] ^= 1;
        assert_eq!(document.encode(&Limits::default()).unwrap_err().kind, kind);
    }
}

#[test]
fn original_pointer_and_map_cardinality_are_required_before_any_rebuild() {
    let mut no_pointer = source(0);
    no_pointer[60..64].fill(0);
    let mut two_maps = source(0);
    two_maps.extend(chunk(b"rsmp", 1, 0x10, b"other", &[0; 20]));
    for bytes in [no_pointer, two_maps] {
        let mut document = IffDocument::decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(document.encode(&Limits::default()).unwrap(), bytes);
        document.file_mut().chunks[0].data[0] ^= 1;
        assert_eq!(
            document.encode(&Limits::default()).unwrap_err().kind,
            ErrorKind::UnsupportedVersion
        );
    }
    let bytes = source(0);
    for pointer in [63u32, 64, 222, 357] {
        let mut damaged = bytes.clone();
        damaged[60..64].copy_from_slice(&pointer.to_be_bytes());
        assert!(IffDocument::decode(&damaged, &Limits::default()).is_err());
    }
}

#[test]
fn explicit_source_api_supports_fresh_baselines_and_rejects_new_index_without_source() {
    let bytes = source(0);
    let mut file = iff::decode(&bytes, &Limits::default()).unwrap();
    file.chunks[0].data.push(7);
    let first = iff::encode_rebuilding_index(&bytes, &file, &Limits::default()).unwrap();
    let mut second_file = iff::decode(&first, &Limits::default()).unwrap();
    second_file.chunks[0].data.push(8);
    let second = iff::encode_rebuilding_index(&first, &second_file, &Limits::default()).unwrap();
    assert_eq!(be32(&second, 60), 223);
    assert_eq!(&second[140..144], &[0x91, 0xfe, 7, 8]);
    let mut unindexed = bytes[..221].to_vec();
    unindexed[60..64].fill(0);
    assert_eq!(
        iff::encode_rebuilding_index(&unindexed, &file, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
}

#[test]
fn deleting_all_resources_retains_valid_empty_map_that_can_be_edited_again() {
    for version in [0, 1] {
        let mut document = IffDocument::decode(&source(version), &Limits::default()).unwrap();
        let resource = document.file_mut().chunks.remove(0);
        document.file_mut().chunks.remove(0);
        let empty = document.encode(&Limits::default()).unwrap();
        assert_eq!(empty.len(), 160);
        assert_eq!(be32(&empty, 60), 64);
        assert_eq!(le32(&empty, 156), 0);
        let mut reopened = IffDocument::decode(&empty, &Limits::default()).unwrap();
        reopened.file_mut().chunks.push(resource);
        let refilled = reopened.encode(&Limits::default()).unwrap();
        assert_eq!(be32(&refilled, 60), 64);
        assert_eq!(le32(&refilled, 156), 1);
        assert_eq!(le32(&refilled, 168), if version == 0 { 178 } else { 180 });
        assert_eq!(
            iff::decode(&refilled, &Limits::default())
                .unwrap()
                .chunks
                .len(),
            2
        );
    }
}

#[test]
fn many_small_resources_cannot_bypass_the_rebuild_workspace_budget() {
    let mut bytes = source(0)[..64].to_vec();
    bytes[60..64].copy_from_slice(&7664u32.to_be_bytes());
    for id in 0..100 {
        bytes.extend(chunk(b"DATA", id, 0, b"", &[]));
    }
    let mut map = b"\0\0\0\0\0\0\0\0pmsr\0\0\0\0\x01\0\0\0ATAD\x64\0\0\0".to_vec();
    for id in 0..100u16 {
        map.extend_from_slice(&(64u32 + u32::from(id) * 76).to_le_bytes());
        map.extend_from_slice(&id.to_le_bytes());
        map.extend_from_slice(&[0, 0, 0, 0]);
    }
    bytes.extend(chunk(b"rsmp", 0, 0x10, b"map", &map));
    let tight = Limits {
        max_total_decoded_bytes: 32 * 1024,
        ..Limits::default()
    };
    let mut document = IffDocument::decode(&bytes, &tight).unwrap();
    assert_eq!(document.encode(&tight).unwrap(), bytes);
    document.file_mut().chunks[0].flags = 1;
    assert_eq!(
        document.encode(&tight).map(|_| ()).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    document.encode(&Limits::default()).unwrap();
}
