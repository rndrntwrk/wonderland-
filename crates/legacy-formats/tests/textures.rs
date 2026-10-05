use wonderland_legacy_formats::{textures::decode_png, ErrorKind, Limits};
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
// PNG chunks and zlib payloads generated independently with Python struct/zlib.
const RGBA:&str="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000001149444154789c63e051b260880a70fb0f00068a02568c6c76be0000000049454e44ae426082";
const INDEXED:&str="89504e470d0a1a0a0000000d4948445200000002000000010103000000ceecedc900000006504c54450a141e28323cd51bb4e90000000274524e5300809b2b4e180000000a49444154789c63700000004200412937f4ef0000000049454e44ae426082";
const GRAY16:&str="89504e470d0a1a0a0000000d494844520000000200000001100000000081d9fc150000000d49444154789c631032597d1600030c01bf6eb9c65d0000000049454e44ae426082";
#[test]
fn png_expands_source_palette_transparency_and_reports_precision_conversion() {
    let a = decode_png(&bytes(RGBA), &Limits::default()).unwrap();
    assert_eq!((a.width, a.height, a.source_bit_depth), (2, 1, 8));
    assert_eq!(a.rgba, [[12, 34, 56, 0], [90, 80, 70, 255]]);
    let p = decode_png(&bytes(INDEXED), &Limits::default()).unwrap();
    assert_eq!(p.source_bit_depth, 1);
    assert_eq!(p.rgba, [[10, 20, 30, 0], [40, 50, 60, 128]]);
    let p = decode_png(&bytes(GRAY16), &Limits::default()).unwrap();
    assert_eq!(p.source_bit_depth, 16);
    assert_eq!(p.rgba, [[0x12, 0x12, 0x12, 255], [0xab, 0xab, 0xab, 255]]);
}
#[test]
fn png_validates_whole_container_and_bounds_before_pixel_allocation() {
    let raw = bytes(RGBA);
    for end in 0..raw.len() {
        assert!(decode_png(&raw[..end], &Limits::default()).is_err());
    }
    let mut corrupt = raw.clone();
    corrupt[29] ^= 1;
    assert!(decode_png(&corrupt, &Limits::default()).is_err());
    let mut appended = raw.clone();
    appended.push(0);
    assert!(decode_png(&appended, &Limits::default()).is_err());
    let limits = Limits {
        max_pixels: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_png(&raw, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    let limits = Limits {
        max_total_decoded_bytes: 1000,
        ..Limits::default()
    };
    assert_eq!(
        decode_png(&raw, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    let limits = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    assert_eq!(
        decode_png(&raw, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}
