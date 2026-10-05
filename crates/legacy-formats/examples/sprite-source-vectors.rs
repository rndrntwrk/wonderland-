// SPDX-License-Identifier: MPL-2.0
//! Authored boundary vectors consumed by the unchanged C# SPR2 reader probe.
use std::io::Write;
use wonderland_legacy_formats::{sprites::*, Limits};

fn palette() -> Palette {
    Palette {
        version: 1,
        reserved: [0xa3; 8],
        colors: (0..300)
            .map(|i| {
                [
                    (i * 13 + (i / 256) * 123) as u8,
                    (i * 7) as u8,
                    (i * 3) as u8,
                    255,
                ]
            })
            .collect(),
    }
}

fn frame(
    version: u32,
    width: u16,
    height: u16,
    flags: u32,
    transparent_index: u16,
    style: u32,
    raw: u16,
) -> SpriteFrame {
    let p = palette();
    let n = width as usize * height as usize;
    let mut rgba = Vec::with_capacity(n);
    let mut indices = Vec::with_capacity(n);
    let mut depth = Vec::with_capacity(n);
    for i in 0..n {
        let (index, alpha, z) = match style {
            0 => (transparent_index as u8, 0, 255),
            1 if i + 1 != n => (transparent_index as u8, 0, 255),
            2 => (41, 255, 0),
            3 => (41, 255, 3),
            4 => (41, 123, 29),
            _ if i % 7 == 0 => (transparent_index as u8, 0, 255),
            _ if i % 7 == 1 && flags & 2 != 0 => (53, 123, 29),
            _ if i % 7 == 2 && flags & 2 != 0 => (63, 255, 17),
            _ => (72, 255, 0),
        };
        let mut color = p.colors[if alpha == 0 {
            transparent_index as usize
        } else {
            index as usize
        }];
        color[3] = alpha;
        rgba.push(color);
        indices.push(index);
        depth.push(z);
    }
    SpriteFrame {
        version,
        source_offset: usize::MAX,
        encoded_size: usize::MAX,
        width,
        height,
        flags,
        palette_id: if version == 1000 || raw == 0 || raw == 0xa3a3 {
            7
        } else {
            raw
        },
        raw_palette_id: Some(raw),
        transparent_index: Some(transparent_index),
        position: [i16::MIN, i16::MAX],
        reserved: 0,
        rgba: Some(rgba),
        indices: Some(indices),
        depth: (flags & 2 != 0).then_some(depth),
        fallback_depth: None,
    }
}

fn u32out(out: &mut impl Write, n: u32) {
    out.write_all(&n.to_le_bytes()).unwrap();
}

fn main() {
    let p = palette();
    let mut cases = Vec::new();
    for version in [1000, 1001] {
        for flags in [1, 3, 5, 7] {
            let frames = vec![
                frame(version, 47, 5, flags, 257, 5, 0),
                frame(version, 0, 65535, flags, 257, 0, 0xa3a3),
                frame(version, 65535, 0, flags, 257, 0, 9),
            ];
            cases.push(SpriteSet {
                kind: SpriteKind::Spr2,
                byte_order: SpriteByteOrder::LittleEndian,
                version,
                default_palette_id: 0xdead0007,
                declared_frame_count: frames.len() as u32,
                frames,
            });
        }
        for (width, height, style, flags) in [
            (1, 65535, 0, 7),
            (65535, 1, 1, 7),
            (8186, 1, 2, 1),
            (4093, 1, 3, 3),
            (2728, 1, 4, 3),
        ] {
            let frames = vec![frame(version, width, height, flags, 257, style, 0)];
            cases.push(SpriteSet {
                kind: SpriteKind::Spr2,
                byte_order: SpriteByteOrder::LittleEndian,
                version,
                default_palette_id: 0xdead0007,
                declared_frame_count: 1,
                frames,
            });
        }
    }
    let destination = std::env::args_os()
        .nth(1)
        .expect("usage: sprite-source-vectors OUTPUT");
    let mut out = std::fs::File::create(destination).unwrap();
    u32out(&mut out, cases.len() as u32);
    let mut byte_count = 0;
    for set in &cases {
        let encoded = encode_spr2(set, &p, &Limits::default()).unwrap();
        byte_count += encoded.len();
        u32out(&mut out, encoded.len() as u32);
        out.write_all(&encoded).unwrap();
        u32out(&mut out, set.default_palette_id);
        u32out(&mut out, set.declared_frame_count);
        for f in &set.frames {
            for n in [
                u32::from(f.width),
                u32::from(f.height),
                f.flags,
                u32::from(f.palette_id),
                u32::from(f.transparent_index.unwrap()),
            ] {
                u32out(&mut out, n);
            }
            out.write_all(&f.position[0].to_le_bytes()).unwrap();
            out.write_all(&f.position[1].to_le_bytes()).unwrap();
            for c in f.rgba.as_ref().unwrap() {
                out.write_all(c).unwrap();
            }
            out.write_all(f.indices.as_ref().unwrap()).unwrap();
            if let Some(z) = &f.depth {
                out.write_all(z).unwrap();
            }
        }
    }
    println!(
        "Authored {} independent boundary cases ({} encoded bytes)",
        cases.len(),
        byte_count
    );
}
