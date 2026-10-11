//! Synthetic standalone Vitaboy bytes for projection tests, not original game assets.
use std::collections::BTreeMap;
use wonderland_avatar_content::{self as content, ImportedContent};
use wonderland_render_core::{AssetKey, RgbaImage};
pub const HEAD: u64 = 0x002000010000000d;
pub const BODY: u64 = 0x002000020000000d;
fn words(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|n| n.to_be_bytes()).collect()
}
fn clip_bytes(name: &str, first: f32, last: f32) -> Vec<u8> {
    let mut bytes = words(&[2]);
    bytes.extend((name.len() as i16).to_be_bytes());
    bytes.extend(name.as_bytes());
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend(0f32.to_le_bytes());
    bytes.push(0);
    bytes.extend(words(&[2]));
    for x in [first, last] {
        for n in [x, 0., 0.] {
            bytes.extend(n.to_le_bytes());
        }
    }
    bytes.extend(words(&[0, 1, 0]));
    bytes.push(4);
    bytes.extend(b"ROOT");
    bytes.extend(words(&[2]));
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend([1, 0]);
    bytes.extend(0i32.to_be_bytes());
    bytes.extend((-1i32).to_be_bytes());
    bytes.extend([0, 0]);
    bytes
}
pub fn files() -> Vec<(String, Vec<u8>)> {
    let mut skeleton = words(&[1]);
    skeleton.push(5);
    skeleton.extend(b"adult");
    skeleton.extend(1i16.to_be_bytes());
    skeleton.extend(words(&[0]));
    skeleton.push(4);
    skeleton.extend(b"ROOT");
    skeleton.push(4);
    skeleton.extend(b"NULL");
    skeleton.push(0);
    for n in [0f32, 0., 0., 0., 0., 0., 1.] {
        skeleton.extend(n.to_le_bytes());
    }
    skeleton.extend(words(&[1, 1, 1]));
    skeleton.extend([0; 8]);
    let mut mesh = words(&[2, 1]);
    mesh.push(4);
    mesh.extend(b"ROOT");
    mesh.extend(words(&[1, 0, 1, 2, 1, 0, 0, 3, 0, 0, 3]));
    for n in [0f32, 0., 1., 0., 0., 1.] {
        mesh.extend(n.to_le_bytes());
    }
    mesh.extend(words(&[0, 3]));
    for point in [[-0.5f32, 0., 0.], [0.5, 0., 0.], [0., 4., 0.]] {
        for n in point.into_iter().chain([0., 0., 1.]) {
            mesh.extend(n.to_le_bytes());
        }
    }
    let mut binding = words(&[1]);
    binding.push(4);
    binding.extend(b"ROOT");
    binding.extend(words(&[8, 0, 600, 9, 8, 0, 700, 14]));
    let mut hands = words(&[1]);
    for _ in 0..18 {
        hands.extend(words(&[500, 12]));
    }
    let texture = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 240, 31,
        0, 5, 0, 1, 255, 114, 156, 82, 103, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let files = vec![
        ("adult.skel".into(), skeleton),
        (
            format!("head.{HEAD:016x}.oft"),
            words(&[1, 0, 500, 12, 500, 12, 500, 12, 37, 0]),
        ),
        (
            format!("body.{BODY:016x}.oft"),
            words(&[1, 0, 500, 12, 500, 12, 500, 12, 37, 0]),
        ),
        (
            "part.000001f40000000c.apr".into(),
            words(&[1, 0, 0, 1, 501, 11]),
        ),
        ("binding.000001f50000000b.bnd".into(), binding),
        ("mesh.0000025800000009.mesh".into(), mesh),
        ("hands.0000002500000012.hag".into(), hands),
        ("texture.000002bc0000000e.png".into(), texture),
        ("base.anim".into(), clip_bytes("base", 0., 4.)),
        ("blend.anim".into(), clip_bytes("blend", 8., 12.)),
        ("carry.anim".into(), clip_bytes("carry", 2., 6.)),
    ];
    files
}
pub fn bank() -> ImportedContent {
    let files = files();
    content::import(
        content::ImportRequest {
            files: files
                .iter()
                .map(|(name, bytes)| content::NamedBytes {
                    name,
                    bytes,
                    key: None,
                })
                .collect(),
            skeleton_name: "adult.skel",
            collections: vec![],
        },
        &content::ImportLimits::default(),
    )
    .unwrap()
}
pub fn pixels(content: &ImportedContent) -> BTreeMap<AssetKey, RgbaImage> {
    content
        .textures
        .keys()
        .map(|key| {
            (
                *key,
                RgbaImage {
                    width: 1,
                    height: 1,
                    pixels: vec![[255, 0, 0, 255]],
                },
            )
        })
        .collect()
}
