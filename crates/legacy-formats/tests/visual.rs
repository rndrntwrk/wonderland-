use wonderland_legacy_formats::{sprites::*, ErrorKind, Limits};

fn palette() -> Palette {
    decode_palt(
        &[
            0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 12, 13, 21, 22, 23, 31, 32, 33,
        ],
        &Limits::default(),
    )
    .unwrap()
}

fn spr2(payload: &[u8], width: u16, height: u16) -> Vec<u8> {
    let mut b = vec![233, 3, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 233, 3, 0, 0];
    b.extend((16u32 + payload.len() as u32).to_le_bytes());
    b.extend(width.to_le_bytes());
    b.extend(height.to_le_bytes());
    b.extend([7, 0, 0, 0, 0, 0, 0, 0, 0xfe, 0xff, 5, 0]);
    b.extend(payload);
    b
}

#[test]
fn palt_rgb_becomes_opaque_and_preserves_reserved_bytes() {
    assert_eq!(
        palette().colors,
        [[11, 12, 13, 255], [21, 22, 23, 255], [31, 32, 33, 255]]
    );
}

#[test]
fn spr2_explicit_alpha_depth_padding_transparency_and_position() {
    // Row count 12: depth/color run, depth/color/5-bit-alpha run with padding.
    // Second row skips all pixels. Transparent palette RGB survives a skip.
    let b = spr2(
        &[
            12, 0, 1, 0x20, 9, 1, 1, 0x40, 19, 2, 15, 0, 1, 0x80, 0, 0xa0,
        ],
        2,
        2,
    );
    let s = decode_spr2(&b, &palette(), &Limits::default()).unwrap();
    let f = &s.frames[0];
    assert_eq!(f.position, [5, -2]);
    assert_eq!(f.palette_id, 7);
    assert_eq!(
        f.rgba.as_ref().unwrap(),
        &[
            [21, 22, 23, 255],
            [31, 32, 33, 123],
            [11, 12, 13, 0],
            [11, 12, 13, 0]
        ]
    );
    assert_eq!(f.depth.as_ref().unwrap(), &[9, 19, 255, 255]);
    assert_eq!(f.indices.as_ref().unwrap(), &[1, 2, 0, 0]);
}

#[test]
fn spr2_row_and_rle_byte_boundaries_are_enforced() {
    for payload in [
        &[8, 0, 3, 0xc0, 1, 1, 1, 0, 0, 0xa0][..],
        &[4, 0, 1, 0x20, 9, 1, 0, 0xa0][..],
        &[0, 0][..],
        &[3, 0x80, 0, 0xa0][..],
        &[2, 0x20, 0, 0xa0][..],
    ] {
        assert!(decode_spr2(&spr2(payload, 2, 2), &palette(), &Limits::default()).is_err());
    }
    let limits = Limits {
        max_pixels: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_spr2(&spr2(&[0, 0xa0], 2, 2), &palette(), &limits)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn spr_v1000_opaque_repeat_literal_and_transparent_pixels() {
    let mut b = vec![
        232, 3, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 1, 0, 4, 0,
    ];
    b.extend([4, 12, 1, 1, 2, 2, 1, 0, 3, 1, 2, 0, 5, 0]);
    let f = decode_spr(&b, &palette(), &Limits::default())
        .unwrap()
        .frames
        .remove(0);
    assert_eq!(
        f.rgba.unwrap(),
        [
            [0, 0, 0, 0],
            [21, 22, 23, 255],
            [21, 22, 23, 255],
            [31, 32, 33, 255]
        ]
    );
}

#[test]
fn dgrp_retains_integer_coordinates_float_bits_flags_and_rotation() {
    let mut b = vec![0x24, 0x4e, 1, 0, 0, 0, 4, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0];
    for n in [
        7u32, 3, 0xfffffff9, 2147483647, 0x80000000, 5, 0x3fa00000, 0xc0200000,
    ] {
        b.extend(n.to_le_bytes());
    }
    let g = decode_dgrp(&b, &Limits::default()).unwrap();
    let s = &g.images[0].sprites[0];
    assert_eq!(s.sprite_offset, [-7, 2147483647]);
    assert_eq!(
        s.object_offset.map(|x| x.0),
        [0x3fa00000, 0xc0200000, 0x80000000]
    );
    assert!(s.flip());
    assert!(s.luminous());
    assert!(g.image(1, 2, 1).is_some());
    assert!(g.image(1, 2, 0).is_none());
}

#[test]
fn slot_raw_roundtrip_defaults_and_source_proximity_scale() {
    let mut b = vec![
        0, 0, 0, 0, 6, 0, 0, 0, b'T', b'O', b'L', b'S', 1, 0, 0, 0, 2, 0,
    ];
    for n in [
        0x80000000u32,
        0x3f800000,
        0x40000000,
        1,
        2,
        3,
        0x80000000,
        0xffffffff,
        2,
        4,
        3,
        100,
        0,
    ] {
        b.extend(n.to_le_bytes());
    }
    b.extend([0xaa, 0xbb]);
    let s = decode_slot(&b, &Limits::default()).unwrap();
    assert_eq!(
        s.slots[0].effective_proximity(s.version).unwrap(),
        [32, 64, 48]
    );
    assert_eq!(s.slots[0].effective_height(), 5);
    assert_eq!(s.slots[0].offset[0].0, 0x80000000);
    assert_eq!(encode_slot(&s, &Limits::default()).unwrap(), b);
}

#[test]
fn palt_source_versions_zero_and_one_share_the_rgb_layout() {
    // Version 1 occurs in 821 checked-in PALT chunks. The pinned Read method
    // reads the version but uses the same reserved/count/RGB layout for both.
    for version in [0, 1] {
        let mut b = vec![
            version, 0, 0, 0, 1, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 255, 10, 20,
        ];
        let p = decode_palt(&b, &Limits::default()).unwrap();
        assert_eq!(p.version, version as u32);
        assert_eq!(p.reserved, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(p.colors, [[255, 10, 20, 255]]);
        b[0] = 2;
        assert_eq!(
            decode_palt(&b, &Limits::default()).unwrap_err().kind,
            ErrorKind::UnsupportedVersion
        );
    }
}

#[test]
fn spr_big_endian_offset_table_and_v1001_eof_count_semantics() {
    let b = [
        0, 0, 3, 232, 0, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0, 16, 0, 0, 0, 0, 0, 1, 0, 1, 4, 6, 2, 1, 2,
        0, 5, 0,
    ];
    let s = decode_spr(&b, &palette(), &Limits::default()).unwrap();
    assert_eq!(s.byte_order, SpriteByteOrder::BigEndian);
    assert_eq!(s.frames[0].rgba.as_ref().unwrap(), &[[31, 32, 33, 255]]);
    let data = [0, 0, 0, 0, 1, 0, 1, 0, 4, 6, 2, 1, 1, 0, 5, 0];
    // Source SPR 1001 ignores the advertised count and continues until EOF.
    let mut b = vec![233, 3, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 233, 3, 0, 0];
    b.extend((data.len() as u32).to_le_bytes());
    b.extend(data);
    let s = decode_spr(&b, &palette(), &Limits::default()).unwrap();
    assert_eq!(s.declared_frame_count, 0);
    assert_eq!(s.frames.len(), 1);
}

#[test]
fn spr2_old_palette_policy_and_new_resolver_keep_distinct_palettes() {
    let source = spr2(&[6, 0, 1, 0xc0, 1, 0, 0, 0xa0], 1, 1);
    let mut old = vec![232, 3, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0, 16, 0, 0, 0];
    old.extend(&source[20..]);
    old[24..26].copy_from_slice(&99u16.to_le_bytes());
    let mut seen = Vec::new();
    let p = palette();
    let s = decode_spr2_with_palettes(
        &old,
        |id| {
            seen.push(id);
            Some(&p)
        },
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(seen, [7]);
    assert_eq!(s.frames[0].raw_palette_id, Some(99));
    let mut new = source.clone();
    new[8..12].copy_from_slice(&2u32.to_le_bytes());
    new[28..30].copy_from_slice(&0xa3a3u16.to_le_bytes());
    let start = new.len();
    new.extend(&source[12..]);
    new[start + 16..start + 18].copy_from_slice(&9u16.to_le_bytes());
    let mut q = palette();
    q.colors[1] = [1, 2, 3, 255];
    let s = decode_spr2_with_palettes(
        &new,
        |id| match id {
            7 => Some(&p),
            9 => Some(&q),
            _ => None,
        },
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(s.frames[0].palette_id, 7);
    assert_eq!(s.frames[1].palette_id, 9);
    assert_eq!(s.frames[1].rgba.as_ref().unwrap(), &[[1, 2, 3, 255]]);
    assert!(decode_spr2_with_palettes(&new, |_| None, &Limits::default()).is_err());
}

#[test]
fn spr2_opaque_transparent_index_explicit_transparency_and_unfilled_tail() {
    let b = spr2(&[8, 0, 1, 0xc0, 0, 0, 1, 0x60, 0, 0xa0], 4, 1);
    let f = decode_spr2(&b, &palette(), &Limits::default())
        .unwrap()
        .frames
        .remove(0);
    assert_eq!(
        f.rgba.unwrap(),
        [
            [11, 12, 13, 255],
            [11, 12, 13, 0],
            [0, 0, 0, 0],
            [0, 0, 0, 0]
        ]
    );
    assert_eq!(f.depth.unwrap(), [0, 255, 255, 255]);
}

#[test]
fn spr2_all_five_bit_alpha_values_match_source_truncation() {
    let expected = [
        0, 8, 16, 24, 32, 41, 49, 57, 65, 74, 82, 90, 98, 106, 115, 123, 131, 139, 148, 156, 164,
        172, 180, 189, 197, 205, 213, 222, 230, 238, 246, 255,
    ];
    let mut row = vec![100, 0, 32, 0x40];
    for a in 0..32 {
        row.extend([9, 1, a]);
    }
    row.extend([0, 0xa0]);
    let f = decode_spr2(&spr2(&row, 32, 1), &palette(), &Limits::default())
        .unwrap()
        .frames
        .remove(0);
    assert_eq!(
        f.rgba
            .unwrap()
            .into_iter()
            .map(|p| p[3])
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn sprite_truncation_unsupported_commands_offsets_and_aggregate_limits() {
    let b = spr2(&[6, 0, 1, 0xc0, 1, 0, 0, 0xa0], 1, 1);
    let p = palette();
    for end in 0..b.len() {
        assert!(
            decode_spr2(&b[..end], &p, &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut bad = b.clone();
    bad[24] = 0x80;
    assert_eq!(
        decode_spr2(&bad, &p, &Limits::default()).unwrap_err().kind,
        ErrorKind::UnsupportedVersion
    );
    let mut bad = b.clone();
    bad[8..12].copy_from_slice(&0xffffffffu32.to_le_bytes());
    assert_eq!(
        decode_spr2(&bad, &p, &Limits::default()).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    let old = [
        232, 3, 0, 0, 2, 0, 0, 0, 7, 0, 0, 0, 20, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(
        decode_spr2(&old, &p, &Limits::default()).unwrap_err().kind,
        ErrorKind::Overlap
    );
    let limits = Limits {
        max_total_decoded_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_spr2(&b, &p, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn drawing_groups_cover_all_source_version_layouts() {
    for v in 20000u16..=20004 {
        let mut b = v.to_le_bytes().to_vec();
        if v < 20003 {
            b.extend(1u16.to_le_bytes());
            b.extend([1, 0, 16, 3]);
        } else {
            for n in [1u32, 16, 3, 1] {
                b.extend(n.to_le_bytes());
            }
        }
        if v < 20003 {
            for n in [99u16, 7, 2, 0x10, 0xfff9, 32767] {
                b.extend(n.to_le_bytes());
            }
            if v == 20001 {
                b.extend(1.5f32.to_le_bytes());
            }
        } else {
            for n in [7u32, 2, 0xfffffff9, 32767, 0x3fc00000, 0x10] {
                b.extend(n.to_le_bytes());
            }
            if v == 20004 {
                b.extend(2.0f32.to_le_bytes());
                b.extend(3.0f32.to_le_bytes());
            }
        }
        let d = decode_dgrp(&b, &Limits::default()).unwrap();
        let s = &d.images[0].sprites[0];
        assert_eq!(s.sprite_offset, [-7, 32767]);
        assert_eq!(s.legacy_type, if v < 20003 { Some(99) } else { None });
        assert_eq!(
            s.object_offset[2].get(),
            if v == 20001 || v >= 20003 { 1.5 } else { 0.0 }
        );
        assert!(d.image(1, 3, 2).is_some());
        for end in 0..b.len() {
            assert!(decode_dgrp(&b[..end], &Limits::default()).is_err());
        }
    }
}

#[test]
fn slot_all_versions_roundtrip_and_reject_unrepresentable_edits() {
    for version in 4u32..=10 {
        let mut b = vec![0x12, 0x34, 0x56, 0x78];
        b.extend(version.to_le_bytes());
        b.extend(b"TOLS");
        b.extend(1u32.to_le_bytes());
        b.extend(3u16.to_le_bytes());
        for n in [0x80000000u32, 0, 0, 1, 0, 0, 0xffffffff, 0xffffffff] {
            b.extend(n.to_le_bytes());
        }
        if version >= 6 {
            for n in [1i32, 2, 3, 100, 5] {
                b.extend(n.to_le_bytes());
            }
        }
        if version >= 7 {
            b.extend(0.25f32.to_le_bytes());
        }
        if version >= 8 {
            b.extend(0i32.to_le_bytes());
        }
        if version >= 9 {
            b.extend((-3i32).to_le_bytes());
        }
        if version >= 10 {
            b.extend(8i32.to_le_bytes());
        }
        let mut resource = decode_slot(&b, &Limits::default()).unwrap();
        assert_eq!(encode_slot(&resource, &Limits::default()).unwrap(), b);
        assert_eq!(resource.slots[0].effective_height(), 5);
        assert_eq!(
            resource.slots[0].effective_resolution(),
            if version >= 10 { 8 } else { 16 }
        );
        resource.slots[0].offset[1] = wonderland_legacy_formats::vitaboy::F32Bits::from_f32(-2.0);
        let changed = encode_slot(&resource, &Limits::default()).unwrap();
        assert_eq!(&changed[..22], &b[..22]);
        assert_eq!(&changed[26..], &b[26..]);
        resource.slots[0].offset[1] = wonderland_legacy_formats::vitaboy::F32Bits(0x7f800000);
        assert!(encode_slot(&resource, &Limits::default()).is_err());
    }
}
