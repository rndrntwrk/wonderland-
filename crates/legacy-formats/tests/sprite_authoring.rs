use wonderland_legacy_formats::{sprites::*, ErrorKind, Limits};

fn palette() -> Palette {
    Palette {
        version: 1,
        reserved: [0xa3; 8],
        colors: vec![[11, 12, 13, 255], [21, 22, 23, 255], [31, 32, 33, 255]],
    }
}

fn authored(version: u32, width: u16, height: u16, pixels: &[(u8, u8, u8)]) -> SpriteSet {
    let palette = palette();
    SpriteSet {
        kind: SpriteKind::Spr2,
        version,
        byte_order: SpriteByteOrder::LittleEndian,
        default_palette_id: 7,
        declared_frame_count: 1,
        frames: vec![SpriteFrame {
            version,
            source_offset: 0,
            encoded_size: 0,
            width,
            height,
            flags: 7,
            palette_id: 7,
            raw_palette_id: Some(0),
            transparent_index: Some(0),
            position: [5, -2],
            reserved: 0,
            rgba: Some(
                pixels
                    .iter()
                    .map(|&(i, a, _)| {
                        let mut color = palette.colors[i as usize];
                        color[3] = a;
                        color
                    })
                    .collect(),
            ),
            indices: Some(pixels.iter().map(|p| p.0).collect()),
            depth: Some(pixels.iter().map(|p| p.2).collect()),
            fallback_depth: None,
        }],
    }
}

fn same_pixels(left: &SpriteSet, right: &SpriteSet) {
    assert_eq!(left.version, right.version);
    assert_eq!(left.default_palette_id, right.default_palette_id);
    assert_eq!(left.frames.len(), right.frames.len());
    for (a, b) in left.frames.iter().zip(&right.frames) {
        assert_eq!(
            (a.width, a.height, a.flags, a.palette_id, a.position),
            (b.width, b.height, b.flags, b.palette_id, b.position)
        );
        assert_eq!(a.rgba, b.rgba);
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.depth, b.depth);
    }
}

#[test]
fn palette_writer_preserves_version_reserved_rgb_and_rejects_silent_alpha_loss() {
    let mut p = palette();
    let encoded = encode_palt(&p, &Limits::default()).unwrap();
    assert_eq!(
        &encoded[..16],
        &[1, 0, 0, 0, 3, 0, 0, 0, 0xa3, 0xa3, 0xa3, 0xa3, 0xa3, 0xa3, 0xa3, 0xa3]
    );
    assert_eq!(decode_palt(&encoded, &Limits::default()).unwrap(), p);
    p.colors[1][3] = 254;
    assert_eq!(
        encode_palt(&p, &Limits::default()).unwrap_err().kind,
        ErrorKind::InvalidData
    );
}

#[test]
fn source_encoder_commands_match_and_roundtrip_both_sprite_layouts() {
    // The unchanged SPR2FrameEncoder.cs independently emits these 22 bytes:
    // color-only odd run + pad, alpha/depth odd run + pad, opaque depth,
    // transparent pixel, one skipped transparent row, end marker.
    let expected = [
        18, 0, 1, 0xc0, 1, 0, 1, 0x40, 19, 2, 15, 0, 1, 0x20, 9, 1, 1, 0x60, 1, 0x80, 0, 0xa0,
    ];
    for version in [1000, 1001] {
        let source = authored(
            version,
            4,
            2,
            &[
                (1, 255, 0),
                (2, 123, 19),
                (1, 255, 9),
                (0, 0, 255),
                (0, 0, 255),
                (0, 0, 255),
                (0, 0, 255),
                (0, 0, 255),
            ],
        );
        let encoded = encode_spr2(&source, &palette(), &Limits::default()).unwrap();
        let start = if version == 1000 { 32 } else { 36 };
        assert_eq!(&encoded[start..], &expected);
        same_pixels(
            &source,
            &decode_spr2(&encoded, &palette(), &Limits::default()).unwrap(),
        );
    }
}

#[test]
fn quantization_is_explicit_and_reported_for_all_alpha_values() {
    for alpha in 1..=254u8 {
        let source = authored(1001, 1, 1, &[(1, alpha, 20)]);
        let q = (u16::from(alpha) * 31).div_ceil(255);
        let decoded_alpha = (q * 255 / 31) as u8;
        let exact = encode_spr2(&source, &palette(), &Limits::default());
        assert_eq!(exact.is_ok(), alpha == decoded_alpha);
        let p = palette();
        let encoded = encode_spr2_with_palettes(
            &source,
            |_| Some(&p),
            Spr2AlphaMode::QuantizeLikeSource,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(
            encoded.quantized_alpha_pixels,
            usize::from(alpha != decoded_alpha)
        );
        let decoded = decode_spr2(&encoded.bytes, &p, &Limits::default()).unwrap();
        assert_eq!(
            decoded.frames[0].rgba.as_ref().unwrap()[0][3],
            decoded_alpha
        );
    }
}

#[test]
fn mismatched_palette_transparency_channels_and_frame_metadata_are_rejected() {
    let source = authored(1001, 1, 1, &[(1, 255, 20)]);
    for change in 0..8 {
        let mut bad = source.clone();
        match change {
            0 => bad.frames[0].rgba.as_mut().unwrap()[0][0] = 99,
            1 => bad.frames[0].indices.as_mut().unwrap()[0] = 3,
            2 => bad.frames[0].depth = None,
            3 => bad.frames[0].raw_palette_id = Some(8),
            4 => bad.frames[0].rgba.as_mut().unwrap().clear(),
            5 => bad.declared_frame_count = 2,
            6 => bad.frames[0].reserved = 1,
            7 => bad.frames[0].version = 1000,
            _ => unreachable!(),
        }
        assert!(
            encode_spr2(&bad, &palette(), &Limits::default()).is_err(),
            "case {change}"
        );
    }
    let transparent = authored(1001, 1, 1, &[(1, 0, 255)]);
    assert!(encode_spr2(&transparent, &palette(), &Limits::default()).is_err());
}

#[test]
fn encoder_never_wraps_row_or_run_fields_and_splits_long_blank_spans() {
    let wide = authored(1001, 5000, 1, &vec![(1, 255, 20); 5000]);
    assert!(encode_spr2(&wide, &palette(), &Limits::default()).is_err());
    let tall = authored(1001, 1, 9000, &vec![(0, 0, 255); 9000]);
    let bytes = encode_spr2(&tall, &palette(), &Limits::default()).unwrap();
    assert_eq!(&bytes[36..], &[0xff, 0x9f, 0x29, 0x83, 0, 0xa0]);
    same_pixels(
        &tall,
        &decode_spr2(&bytes, &palette(), &Limits::default()).unwrap(),
    );
    let mut long_run = vec![(0, 0, 255); 9000];
    long_run[8999] = (1, 255, 0);
    let source = authored(1001, 9000, 1, &long_run);
    let bytes = encode_spr2(&source, &palette(), &Limits::default()).unwrap();
    same_pixels(
        &source,
        &decode_spr2(&bytes, &palette(), &Limits::default()).unwrap(),
    );
}

#[test]
fn frame_palette_resolution_and_output_allocation_limits_are_checked() {
    let mut source = authored(1001, 1, 1, &[(1, 255, 0)]);
    let mut frame = source.frames[0].clone();
    frame.palette_id = 9;
    frame.raw_palette_id = Some(9);
    source.frames.push(frame);
    source.declared_frame_count = 2;
    let p = palette();
    assert!(encode_spr2_with_palettes(
        &source,
        |id| (id == 7).then_some(&p),
        Spr2AlphaMode::Exact,
        &Limits::default()
    )
    .is_err());
    let bytes = encode_spr2_with_palettes(
        &source,
        |_| Some(&p),
        Spr2AlphaMode::Exact,
        &Limits::default(),
    )
    .unwrap()
    .bytes;
    same_pixels(
        &source,
        &decode_spr2(&bytes, &p, &Limits::default()).unwrap(),
    );
    for limits in [
        Limits {
            max_frames: 1,
            ..Limits::default()
        },
        Limits {
            max_pixels: 0,
            ..Limits::default()
        },
        Limits {
            max_resource_bytes: bytes.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_input_bytes: bytes.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_total_decoded_bytes: 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            encode_spr2(&source, &p, &limits).unwrap_err().kind,
            ErrorKind::LimitExceeded
        );
    }
}

#[test]
fn many_maximum_height_zero_width_frames_fit_the_zero_pixel_budget() {
    let mut source = authored(1001, 0, u16::MAX, &[]);
    source.frames = vec![source.frames[0].clone(); 1000];
    source.declared_frame_count = 1000;
    let limits = Limits {
        max_pixels: 0,
        ..Limits::default()
    };
    let bytes = encode_spr2(&source, &palette(), &limits).unwrap();
    // 12-byte resource header, 8+16-byte frame headers, nine row skips and end.
    assert_eq!(bytes.len(), 12 + 1000 * 44);
    same_pixels(&source, &decode_spr2(&bytes, &palette(), &limits).unwrap());
}
