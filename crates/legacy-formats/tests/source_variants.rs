use wonderland_legacy_formats::{semantic::*, sprites::*, vitaboy::F32Bits, Limits};

fn ttab(version: u16, tsbo: bool) -> Vec<u8> {
    let mut bytes = vec![1, 0];
    bytes.extend(version.to_le_bytes());
    if version == 9 || version == 10 || (version > 10 && tsbo) {
        bytes.push(0);
    }
    bytes.extend(4096u16.to_le_bytes());
    bytes.extend(4097u16.to_le_bytes());
    for n in [1u32, 0xdeadbeef, 27, 3, 0x80000000, 99, 0xffffffff] {
        bytes.extend(n.to_le_bytes());
    }
    bytes.extend((-7i16).to_le_bytes());
    bytes.extend(42i16.to_le_bytes());
    bytes.extend(13u16.to_le_bytes());
    if version > 9 && !tsbo {
        bytes.extend(0x12345678u32.to_le_bytes());
    }
    bytes
}

#[test]
fn later_ttab_versions_follow_original_standard_and_tsbo_field_selection() {
    for version in [11, 12, 65535] {
        for variant in [TtabVariant::Standard, TtabVariant::Tsbo] {
            let raw = ttab(version, variant == TtabVariant::Tsbo);
            let mut value = decode_ttab_with_variant(&raw, variant, &Limits::default()).unwrap();
            assert_eq!(value.interactions[0].action_function, 4096);
            assert_eq!(value.interactions[0].attenuation_value_bits, 0x80000000);
            assert_eq!(
                value.interactions[0].flags2,
                if variant == TtabVariant::Standard {
                    0x12345678
                } else {
                    0x1e
                }
            );
            assert_eq!(encode_ttab(&value, &Limits::default()).unwrap(), raw);
            value.interactions[0].action_function = 4099;
            let encoded = encode_ttab(&value, &Limits::default()).unwrap();
            assert_eq!(
                decode_ttab_with_variant(&encoded, variant, &Limits::default())
                    .unwrap()
                    .interactions,
                value.interactions
            );
        }
    }
}

#[test]
fn dgrp_writes_all_five_source_layouts_without_losing_absent_fields() {
    for version in 20000u16..=20004 {
        let old = version < 20003;
        let mut bytes = version.to_le_bytes().to_vec();
        if old {
            bytes.extend(1u16.to_le_bytes());
            bytes.extend(1u16.to_le_bytes());
            bytes.extend([2, 3]);
        } else {
            for n in [1u32, 2, 3, 1] {
                bytes.extend(n.to_le_bytes());
            }
        }
        if old {
            for n in [9u16, 23, 7, 5, 0xfffe, 13] {
                bytes.extend(n.to_le_bytes());
            }
            if version == 20001 {
                bytes.extend(0x80000000u32.to_le_bytes());
            }
        } else {
            for n in [23u32, 7, 0xfffffffe, 13, 0x80000000, 5] {
                bytes.extend(n.to_le_bytes());
            }
            if version == 20004 {
                bytes.extend(0x3fc00000u32.to_le_bytes());
                bytes.extend(0xbf000000u32.to_le_bytes());
            }
        }
        let mut group = decode_dgrp(&bytes, &Limits::default()).unwrap();
        assert_eq!(encode_dgrp(&group, &Limits::default()).unwrap(), bytes);
        if version != 20004 {
            group.images[0].sprites[0].object_offset[0] = F32Bits(0x80000000);
            assert!(encode_dgrp(&group, &Limits::default()).is_err());
        }
    }
}

#[test]
fn spr_authoring_retains_pixel_planes_with_version_and_byte_order() {
    let palette = Palette {
        version: 1,
        reserved: [0; 8],
        colors: vec![[0, 0, 0, 255], [20, 30, 40, 255]],
    };
    for version in [1000u32, 1001] {
        for order in [SpriteByteOrder::LittleEndian, SpriteByteOrder::BigEndian] {
            if version == 1001 && order == SpriteByteOrder::BigEndian {
                continue;
            }
            let mut raw = Vec::new();
            let number = |n: u32| {
                if order == SpriteByteOrder::LittleEndian {
                    n.to_le_bytes()
                } else {
                    n.to_be_bytes()
                }
            };
            for n in [version, 1, 42] {
                raw.extend(number(n));
            }
            if version == 1000 {
                raw.extend(number(16));
            } else {
                raw.extend(number(1001));
                raw.extend(number(18));
            }
            raw.extend(number(27));
            if order == SpriteByteOrder::LittleEndian {
                raw.extend([1, 0, 3, 0]);
            } else {
                raw.extend([0, 1, 0, 3]);
            }
            raw.extend([4, 8, 1, 1, 2, 2, 1, 0, 5, 0]);
            let sprite = decode_spr(&raw, &palette, &Limits::default()).unwrap();
            let encoded = encode_spr(&sprite, &palette, &Limits::default()).unwrap();
            let decoded = decode_spr(&encoded, &palette, &Limits::default()).unwrap();
            assert_eq!(decoded.kind, SpriteKind::Spr);
            assert_eq!(decoded.byte_order, order);
            assert_eq!(decoded.version, version);
            assert_eq!(decoded.frames[0].rgba, sprite.frames[0].rgba);
            assert_eq!(decoded.frames[0].indices, sprite.frames[0].indices);
            assert_eq!(decoded.frames[0].reserved, 27);
            let mut invalid = sprite.clone();
            invalid.frames[0].rgba.as_mut().unwrap()[1][3] = 10;
            assert!(encode_spr(&invalid, &palette, &Limits::default()).is_err());
        }
    }
}
