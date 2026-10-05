use wonderland_legacy_formats::{audio_meta::*, ErrorKind, Limits};

#[test]
fn xa_header_metadata_has_exact_frame_count_without_decompression() {
    let mut b = vec![
        b'X', b'A', b'I', 0, 56, 0, 0, 0, 1, 0, 1, 0, 0x22, 0x56, 0, 0, 0x44, 0xac, 0, 0, 2, 0, 16,
        0,
    ];
    b.extend([0u8; 15]);
    let a = decode_xa(&b, &Limits::default()).unwrap();
    assert_eq!(a.format.channels, 1);
    assert_eq!(a.format.sample_rate, 22050);
    assert_eq!(a.sample_frames, 28);
    assert_eq!(a.payload_offset, 24);
    let tiny = Limits {
        max_total_decoded_bytes: 55,
        ..Limits::default()
    };
    assert_eq!(
        decode_xa(&b, &tiny).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn hitlists_keep_duplicates_ranges_and_order() {
    let binary = [1, 0, 0, 0, 3, 0, 0, 0, 7, 0, 0, 0, 7, 0, 0, 0, 2, 0, 0, 0];
    assert_eq!(
        decode_hitlist(
            &binary,
            HitlistEncoding::VersionedBinary,
            &Limits::default()
        )
        .unwrap()
        .ids,
        [7, 7, 2]
    );
    let mut text = vec![8, 0, 0, 0];
    text.extend(b"9,2-4,2\n");
    assert_eq!(
        decode_hitlist(&text, HitlistEncoding::PascalRanges, &Limits::default())
            .unwrap()
            .ids,
        [9, 2, 3, 4, 2]
    );
    let mut bad = vec![13, 0, 0, 0];
    bad.extend(b"0-4294967295\n");
    assert_eq!(
        decode_hitlist(&bad, HitlistEncoding::PascalRanges, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn track_csv_and_2dkt_wrapper_keep_source_indices() {
    let csv = b"0,2,name,0x10,17,1,unused,2,x,y,z,clsid,3,1,90";
    let mut b = b"2DKT".to_vec();
    b.extend((csv.len() as u32).to_le_bytes());
    b.extend(csv);
    let t = decode_track(&b, &Limits::default()).unwrap();
    assert_eq!(t.version, 2);
    assert_eq!(t.sound_id, 16);
    assert_eq!(t.track_id, 17);
    let d = t.detail.unwrap();
    assert_eq!(d.ducking_priority, 3);
    assert_eq!(d.looped, 1);
    assert_eq!(d.volume, 90);
}

fn xa() -> Vec<u8> {
    let mut b = vec![
        b'X', b'A', b'J', 0, 112, 0, 0, 0, 1, 0, 2, 0, 0x22, 0x56, 0, 0, 0x88, 0x58, 1, 0, 4, 0,
        16, 0,
    ];
    b.extend([0u8; 30]);
    b
}

#[test]
fn xa_stereo_fields_truncation_and_arithmetic_validation() {
    let b = xa();
    let a = decode_audio(&b, &Limits::default()).unwrap();
    assert_eq!(a.encoding, AudioEncoding::XaMusic);
    assert_eq!(a.sample_frames, 28);
    assert_eq!(a.payload_bytes, 30);
    for end in 0..b.len() {
        assert!(
            decode_xa(&b[..end], &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    for at in [4, 8, 10, 12, 16, 20, 22] {
        let mut bad = b.clone();
        bad[at..at + 2].fill(0);
        assert!(decode_xa(&bad, &Limits::default()).is_err(), "field {at}");
    }
}

#[test]
fn utk_header_is_metadata_only_and_extensions_are_explicitly_unsupported() {
    let b = [
        b'U', b'T', b'M', b'0', 0, 2, 0, 0, 20, 0, 0, 0, 1, 0, 1, 0, 0x40, 0x1f, 0, 0, 0x80, 0x3e,
        0, 0, 2, 0, 16, 0, 0, 0, 0, 0, 0, 0,
    ];
    let a = decode_utk(&b, &Limits::default()).unwrap();
    assert_eq!(a.sample_frames, 256);
    assert_eq!(a.wave_format_size, Some(20));
    assert_eq!(a.payload_offset, 32);
    let mut bad = b;
    bad[28] = 1;
    assert_eq!(
        decode_utk(&bad, &Limits::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion
    );
    for end in 0..b.len() {
        assert!(
            decode_utk(&b[..end], &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
}

#[test]
fn pcm_wave_chunk_lengths_padding_and_duplicate_chunks_are_checked() {
    let mut chunks = b"JUNK".to_vec();
    chunks.extend(1u32.to_le_bytes());
    chunks.extend([42, 0]);
    chunks.extend(b"fmt ");
    chunks.extend(16u32.to_le_bytes());
    chunks.extend([1, 0, 1, 0, 0x40, 0x1f, 0, 0, 0x80, 0x3e, 0, 0, 2, 0, 16, 0]);
    chunks.extend(b"data");
    chunks.extend(4u32.to_le_bytes());
    chunks.extend([1, 2, 3, 4]);
    let mut b = b"RIFF".to_vec();
    b.extend((chunks.len() as u32 + 4).to_le_bytes());
    b.extend(b"WAVE");
    b.extend(chunks);
    let a = decode_wave(&b, &Limits::default()).unwrap();
    assert_eq!(a.sample_frames, 2);
    assert_eq!(a.payload_bytes, 4);
    for end in 0..b.len() {
        assert!(decode_wave(&b[..end], &Limits::default()).is_err());
    }
    b.extend(b"data");
    b.extend([0u8; 4]);
    let size = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&size.to_le_bytes());
    assert_eq!(
        decode_wave(&b, &Limits::default()).unwrap_err().kind,
        ErrorKind::Duplicate
    );
}

#[test]
fn hitlist_counted_variant_and_uint_max_range_terminate_exactly() {
    let b = [2, 0, 0, 0, 255, 255, 255, 255, 3, 0, 0, 0];
    assert_eq!(
        decode_hitlist(&b, HitlistEncoding::CountedBinary, &Limits::default())
            .unwrap()
            .ids,
        [u32::MAX, 3]
    );
    let text = b"4294967294-4294967295";
    let mut b = (text.len() as u32).to_le_bytes().to_vec();
    b.extend(text);
    b.push(b'\n');
    assert_eq!(
        decode_hitlist(&b, HitlistEncoding::PascalRanges, &Limits::default())
            .unwrap()
            .ids,
        [u32::MAX - 1, u32::MAX]
    );
    let b = [1, 0, 0, 0, 255, 255, 255, 255];
    assert_eq!(
        decode_hitlist(&b, HitlistEncoding::VersionedBinary, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn unwrapped_track_short_record_and_source_version_one_detail_offsets() {
    let t = decode_track(b"TKDT,1,short,11,12,ETKD", &Limits::default()).unwrap();
    assert!(!t.wrapped);
    assert_eq!(t.detail, None);
    assert_eq!(t.track_name, "short");
    let t = decode_track(
        b"TKDT,1,long,11,12,2,unused,4,x,y,z,6,1,75",
        &Limits::default(),
    )
    .unwrap();
    let d = t.detail.unwrap();
    assert_eq!(
        (
            d.argument_type,
            d.control_group,
            d.ducking_priority,
            d.looped,
            d.volume
        ),
        (2, 4, 6, 1, 75)
    );
    assert!(decode_track(b"TKDT,1,broken,11", &Limits::default()).is_err());
    assert_eq!(
        decode_track(b"TKDT,3,unknown,11,12,ETKD", &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedVersion
    );
}

#[test]
fn event_names_and_hsm_first_definition_keep_original_order_and_case() {
    let events = decode_events(
        b"Door_Open,0X10,0x2A,1,2,3,4\r\nDoor_Open,11,7,5,6,7,8\r\n",
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(events.entries.len(), 2);
    assert_eq!(events.entries[0].source_name, "Door_Open");
    assert_eq!(events.entries[0].name, "door_open");
    assert_eq!(events.entries[0].track_id, 42);
    assert_eq!(events.entries[1].track_id, 7);
    let h = decode_hsm(b"Start 12\nSTART 99\nEnd -4\n", &Limits::default()).unwrap();
    assert_eq!(h.constants.len(), 3);
    assert_eq!(h.first("start"), Some(12));
    assert_eq!(h.first("end"), Some(-4));
    assert!(decode_events(b"Bad,1,2\n", &Limits::default()).is_err());
}

#[test]
fn hit_entrypoint_metadata_keeps_table_order_and_rejects_bad_targets() {
    // Header tags are deliberately opaque: this reader claims metadata only,
    // and does not claim a supported VM bytecode version from these integers.
    let mut b = b"HIT!".to_vec();
    b.extend(1u32.to_le_bytes());
    b.extend(2u32.to_le_bytes());
    b.extend(b"TEST");
    b.extend([0; 4]);
    b.extend(b"ENTP");
    for n in [7u32, 16, 3, 17] {
        b.extend(n.to_le_bytes());
    }
    b.extend(b"eent");
    let h = decode_hit_metadata(&b, &Limits::default()).unwrap();
    assert_eq!(h.table_offset, Some(24));
    assert_eq!(
        h.entrypoints,
        [
            HitEntrypoint {
                track_id: 7,
                address: 16
            },
            HitEntrypoint {
                track_id: 3,
                address: 17
            }
        ]
    );
    let mut bad = b.clone();
    bad[28..32].copy_from_slice(&999u32.to_le_bytes());
    assert!(decode_hit_metadata(&bad, &Limits::default()).is_err());
    let mut bad = b.clone();
    bad[32..36].copy_from_slice(&7u32.to_le_bytes());
    assert_eq!(
        decode_hit_metadata(&bad, &Limits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Duplicate
    );
    assert!(decode_hit_metadata(&b[..b.len() - 1], &Limits::default()).is_err());
}
