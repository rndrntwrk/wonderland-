// SPDX-License-Identifier: MPL-2.0
//! Authored bytes following source field order. No original game assets.
#![forbid(unsafe_code)]

pub const REFPACK: &[u8] = &[0xe0, b'a', b'b', b'c', b'd', 0x04, 0x03, 0xfe, b'!', b'?'];
pub const QFS: &[u8] = &[
    10, 0, 0, 0, 0x10, 0xfb, 0, 0, 10, 0xe0, b'a', b'b', b'c', b'd', 0x04, 0x03, 0xfe, b'!', b'?',
];
pub const BHAV: &[u8] = &[
    2, 0x80, 1, 0, 7, 4, 0x34, 0x12, 0x78, 0x56, 0xab, 0xcd, 0x34, 0x12, 254, 255, 1, 2, 3, 4, 5,
    6, 7, 8, 0xde, 0xad,
];
pub const BCON: &[u8] = &[2, 0xa5, 7, 0, 0xff, 0xff, 0xde, 0xad];
pub const OTF: &[u8] = br#"<O><T i="4096" n="authored"><K i="0" l="base" v="99"/><K i="1" l="signed" v="65535"/></T></O>"#;

/// Animation.Read's mixed endian layout, independent of any Rust encoder.
pub fn animation() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 2, 0, 4, b't', b'e', b's', b't'];
    for bits in [0x447a0000u32, 0x40200000] {
        b.extend(bits.to_le_bytes());
    }
    b.push(1);
    b.extend(1u32.to_be_bytes());
    for bits in [0x3f800000u32, 0x80000000, 0x40400000] {
        b.extend(bits.to_le_bytes());
    }
    b.extend(1u32.to_be_bytes());
    for bits in [0u32, 0, 0, 0x3f800000] {
        b.extend(bits.to_le_bytes());
    }
    b.extend(1u32.to_be_bytes());
    b.extend(123u32.to_be_bytes());
    b.extend([4, b'R', b'O', b'O', b'T']);
    b.extend(1u32.to_be_bytes());
    b.extend(0x447a0000u32.to_le_bytes());
    b.extend([1, 1]);
    b.extend(0u32.to_be_bytes());
    b.extend(0u32.to_be_bytes());
    b.extend([0, 0]); // no properties or time-property lists
    b
}
