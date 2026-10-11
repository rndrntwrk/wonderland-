use wonderland_creator::city::{BmpMap, MapLayer};
fn bmp() -> Vec<u8> {
    let mut bytes = vec![0u8; 70];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&70u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&2i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&2i32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&16u32.to_le_bytes());
    // Bottom row first; padding intentionally nonzero.
    bytes[54..70].copy_from_slice(&[1, 2, 3, 4, 5, 6, 99, 98, 7, 8, 9, 10, 11, 12, 97, 96]);
    bytes
}
#[test]
fn pixel_edit_preserves_orientation_padding_and_all_other_bytes() {
    let original = bmp();
    let mut map = BmpMap::decode(&original, 10).unwrap();
    assert_eq!(map.pixel(0, 0).unwrap(), [9, 8, 7]);
    map.set_pixel(0, 0, [255, 0, 0], MapLayer::Terrain).unwrap();
    let mut expected = original.clone();
    expected[62..65].copy_from_slice(&[0, 0, 255]);
    assert_eq!(map.bytes(), expected);
    assert_eq!(map.ppm().len(), 23);
}
#[test]
fn bad_semantic_pixel_and_malformed_bmp_do_not_change_data() {
    let original = bmp();
    let mut map = BmpMap::decode(&original, 10).unwrap();
    assert!(map.set_pixel(0, 0, [7, 8, 9], MapLayer::Terrain).is_err());
    assert!(map.set_pixel(9, 0, [0, 0, 0], MapLayer::Elevation).is_err());
    assert_eq!(map.bytes(), original);
    for offset in [10usize, 14, 18, 22, 28, 30] {
        let mut bad = original.clone();
        bad[offset..offset + 4].fill(255);
        assert!(BmpMap::decode(&bad, 10).is_err(), "offset {offset}");
    }
    assert!(BmpMap::decode(&original[..69], 10).is_err());
    assert!(BmpMap::decode(&original, 3).is_err());
}
#[test]
fn top_down_rgba_edits_keep_alpha_and_trailing_bytes() {
    let mut bytes = vec![0u8; 74];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&74u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&2i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&(-2i32).to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&32u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&16u32.to_le_bytes());
    bytes[54..].copy_from_slice(&[
        1, 2, 3, 0xa1, 4, 5, 6, 0xa2, 7, 8, 9, 0xa3, 10, 11, 12, 0xa4, 99, 98, 97, 96,
    ]);
    let mut map = BmpMap::decode(&bytes, 4).unwrap();
    assert_eq!(map.pixel(0, 0).unwrap(), [3, 2, 1]);
    map.set_pixel(1, 1, [12, 34, 56], MapLayer::VertexColor)
        .unwrap();
    let mut expected = bytes;
    expected[66..69].copy_from_slice(&[56, 34, 12]);
    assert_eq!(map.bytes(), expected);
}
#[test]
fn imported_geometry_getters_and_one_pixel_boundary_remain_consistent() {
    let mut bytes = bmp();
    bytes.truncate(58);
    bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&1i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1i32.to_le_bytes());
    bytes[34..38].copy_from_slice(&4u32.to_le_bytes());
    let mut map = BmpMap::decode(&bytes, 1).unwrap();
    assert_eq!((map.width(), map.height()), (1, 1));
    assert_eq!(map.pixel(0, 0).unwrap(), [3, 2, 1]);
    assert!(map.pixel(1, 0).is_err());
    assert!(map.pixel(0, 1).is_err());
    assert!(map.set_pixel(1, 0, [1, 2, 3], MapLayer::Elevation).is_err());
    assert!(map.set_pixel(0, 1, [1, 2, 3], MapLayer::Elevation).is_err());
    map.validate(MapLayer::Elevation, false).unwrap();
    assert_eq!(map.bytes(), bytes);
    assert_eq!(map.ppm(), b"P6\n1 1\n255\n\x03\x02\x01");
}
