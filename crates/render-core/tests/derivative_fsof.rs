use wonderland_render_core::derivatives::fsof::*;
use wonderland_render_core::derivatives::ImageRole;
use wonderland_render_core::{Vec2, Vec3};

// Independent wire fixture: every field is appended in FSOF.Save order.
fn source_bytes() -> Vec<u8> {
    let mut out = b"FSOf\x01\0\0\0\0".to_vec();
    for n in [0i32, 1, 1, 1, 1] {
        out.extend(n.to_le_bytes());
    }
    out.push(1);
    for pixel in [
        [1, 2, 3, 4],
        [5, 6, 7, 8],
        [9, 10, 11, 12],
        [13, 14, 15, 16],
    ] {
        out.extend(4i32.to_le_bytes());
        out.extend(pixel);
    }
    out.extend([17, 18, 19, 20]);
    out.extend(3i32.to_le_bytes());
    for position in [[1f32, 2., 3.], [4., 5., 6.], [7., 8., 9.]] {
        for x in position.into_iter().chain([0.25, 0.75, 0., 1., 0.]) {
            out.extend(x.to_le_bytes());
        }
    }
    out.extend(3i32.to_le_bytes());
    for n in [2i32, 1, 0] {
        out.extend(n.to_le_bytes());
    }
    out.extend(0i32.to_le_bytes());
    out.extend(0i32.to_le_bytes());
    out
}

#[test]
fn source_vertex_layout_night_color_and_mesh_indices_round_trip_exactly() {
    let bytes = source_bytes();
    let f = Fsof::decode(&bytes, FsofLimits::default()).unwrap();
    assert_eq!(f.compression, TextureCompression::Rgba8);
    assert_eq!(f.night.as_ref().unwrap().light_color, [17, 18, 19, 20]);
    assert_eq!(f.geometry.floor.vertices[0].position, Vec3::new(1., 2., 3.));
    assert_eq!(f.geometry.floor.vertices[0].uv, Vec2::new(0.25, 0.75));
    assert_eq!(f.geometry.floor.vertices[0].normal, Vec3::new(0., 1., 0.));
    assert_eq!(f.geometry.floor.indices, [2, 1, 0]);
    assert_eq!(f.encode(false, FsofLimits::default()).unwrap(), bytes);
    assert_eq!(
        f.texture(ImageRole::WallNight, FsofLimits::default())
            .unwrap()
            .pixels,
        [[13, 14, 15, 16]]
    );
}

#[test]
fn source_gzip_body_preserves_uncompressed_header_and_checks_crc() {
    let f = Fsof::decode(&source_bytes(), FsofLimits::default()).unwrap();
    let compressed = f.encode(true, FsofLimits::default()).unwrap();
    assert_eq!(&compressed[..9], b"FSOf\x01\0\0\0\x01");
    assert_eq!(&compressed[9..11], &[0x1f, 0x8b]);
    assert_eq!(Fsof::decode(&compressed, FsofLimits::default()).unwrap(), f);
    let mut corrupt = compressed.clone();
    let n = corrupt.len();
    corrupt[n - 8] ^= 1;
    assert!(Fsof::decode(&corrupt, FsofLimits::default()).is_err());
    let mut appended = compressed;
    appended.push(0);
    assert!(Fsof::decode(&appended, FsofLimits::default()).is_err());
}

#[test]
fn malformed_counts_dimensions_versions_and_trailing_data_fail_closed() {
    let bytes = source_bytes();
    for (offset, replacement) in [
        (4, 2i32),
        (9, -1),
        (13, 0),
        (17, i32::MAX),
        (30, -1),
        (70, i32::MAX),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
        assert!(
            Fsof::decode(&bad, FsofLimits::default()).is_err(),
            "offset {offset}"
        );
    }
    for end in [0, 4, 8, 9, 29, 33, 69, 70, 73, 120, 189] {
        assert!(Fsof::decode(&bytes[..end.min(bytes.len())], FsofLimits::default()).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    assert!(Fsof::decode(&bad, FsofLimits::default()).is_err());
    let mut bad = bytes.clone();
    bad[8] = 2;
    assert!(Fsof::decode(&bad, FsofLimits::default()).is_err());
    let mut bad = bytes.clone();
    bad[74..78].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(Fsof::decode(&bad, FsofLimits::default()).is_err());
}

#[test]
fn gzip_expansion_and_total_mesh_memory_are_bounded_before_allocation() {
    let f = Fsof::decode(&source_bytes(), FsofLimits::default()).unwrap();
    let compressed = f.encode(true, FsofLimits::default()).unwrap();
    assert!(Fsof::decode(
        &compressed,
        FsofLimits {
            max_decoded_bytes: 64,
            ..FsofLimits::default()
        }
    )
    .is_err());
    assert!(Fsof::decode(
        &source_bytes(),
        FsofLimits {
            max_vertices: 2,
            ..FsofLimits::default()
        }
    )
    .is_err());
    let mut destination = vec![];
    assert!(f
        .write(
            &mut destination,
            false,
            FsofLimits {
                max_indices: 2,
                ..FsofLimits::default()
            }
        )
        .is_err());
    assert!(destination.is_empty());
}

#[test]
fn dxt5_uses_four_color_palette_even_when_first_565_endpoint_is_lower() {
    let mut f = Fsof::decode(&source_bytes(), FsofLimits::default()).unwrap();
    f.compression = TextureCompression::Dxt5;
    f.floor_width = 4;
    f.floor_height = 4;
    f.wall_width = 4;
    f.wall_height = 4;
    // alpha endpoints 0,255, selector 7 => 255; blue/red color endpoints,
    // selector 3 => one blue + two red thirds, not transparent BC1 mode.
    let block = vec![
        0, 255, 255, 255, 255, 255, 255, 255, 31, 0, 0, 248, 255, 255, 255, 255,
    ];
    f.floor_texture = block.clone();
    f.wall_texture = block;
    f.night = None;
    let image = f
        .texture(ImageRole::FloorDay, FsofLimits::default())
        .unwrap();
    assert_eq!(image.pixels, vec![[170, 0, 85, 255]; 16]);
    let encoded = f.encode(true, FsofLimits::default()).unwrap();
    assert_eq!(Fsof::decode(&encoded, FsofLimits::default()).unwrap(), f);
}
