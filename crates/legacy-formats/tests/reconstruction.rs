use wonderland_legacy_formats::{reconstruction::*, ErrorKind, Limits};

fn payload(version: i32) -> Vec<u8> {
    let mut b = b"FSOm".to_vec();
    b.extend(version.to_le_bytes());
    b.extend(0i32.to_le_bytes());
    b.extend([4, b't', b'e', b's', b't']);
    b.extend(2i32.to_le_bytes()); // dynamic groups, including an empty group
    b.extend(1i32.to_le_bytes());
    b.extend(71u16.to_le_bytes());
    b.extend(65535u16.to_le_bytes());
    b.extend(3i32.to_le_bytes());
    for [x, y, z, u, v] in [
        [0f32, -0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0, 0.0, 1.0],
    ] {
        for f in [x, y, z, u, v] {
            b.extend(f.to_le_bytes());
        }
        if version > 1 {
            for f in [0f32, 0.0, 1.0] {
                b.extend(f.to_le_bytes());
            }
        }
    }
    b.extend(3i32.to_le_bytes());
    for i in [2i32, 0, 1] {
        b.extend(i.to_le_bytes());
    }
    b.extend(0i32.to_le_bytes());
    if version > 2 {
        b.extend(0i32.to_le_bytes());
    }
    for f in [0f32, -0.0, 0.0, 1.0, 1.0, 0.0] {
        b.extend(f.to_le_bytes());
    }
    b
}

#[test]
fn fsom_preserves_all_source_layouts_and_dynamic_group_order() {
    for version in [1, 2, 3] {
        let raw = payload(version);
        let model = decode_fsom_payload(&raw, &Limits::default()).unwrap();
        assert_eq!(model.groups.len(), 2);
        assert!(model.groups[1].is_empty());
        assert_eq!(model.groups[0][0].indices, [2, 0, 1]);
        assert_eq!(model.groups[0][0].vertices[0].position[1].0, 0x80000000);
        assert_eq!(
            model.groups[0][0].texture_reference(),
            FsomTextureReference::CustomTexture { id: 71 }
        );
        assert_eq!(
            encode_fsom_payload(&model, &Limits::default()).unwrap(),
            raw
        );
        let gzip = encode_fsom(&model, &Limits::default()).unwrap();
        assert_eq!(decode_fsom(&gzip, &Limits::default()).unwrap(), model);
        assert_eq!(encode_fsom(&model, &Limits::default()).unwrap(), gzip);
    }
}

#[test]
fn fsom_rejects_invalid_indices_masks_nonfinite_and_gzip_corruption() {
    let mut model = decode_fsom_payload(&payload(3), &Limits::default()).unwrap();
    model.groups[0][0].indices[0] = 3;
    assert!(encode_fsom(&model, &Limits::default()).is_err());
    model.groups[0][0].indices[0] = 2;
    model.mask_type = 1;
    assert!(encode_fsom(&model, &Limits::default()).is_err());
    model.mask_type = 0;
    let valid = encode_fsom(&model, &Limits::default()).unwrap();
    for end in 0..valid.len() {
        assert!(
            decode_fsom(&valid[..end], &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut corrupt = valid.clone();
    let n = corrupt.len();
    corrupt[n - 8] ^= 1;
    assert!(decode_fsom(&corrupt, &Limits::default()).is_err());
    let mut appended = valid;
    appended.extend([0, 0]);
    assert!(decode_fsom(&appended, &Limits::default()).is_err());
    model.bounds[0][0].0 = f32::NAN.to_bits();
    assert!(encode_fsom(&model, &Limits::default()).is_err());
}

#[test]
fn fsom_honors_count_output_and_decompression_budgets() {
    let raw = payload(3);
    let model = decode_fsom_payload(&raw, &Limits::default()).unwrap();
    let gzip = encode_fsom(&model, &Limits::default()).unwrap();
    let tiny = Limits {
        max_resource_bytes: raw.len() - 1,
        ..Limits::default()
    };
    assert!(decode_fsom_payload(&raw, &tiny).is_err());
    assert!(encode_fsom_payload(&model, &tiny).is_err());
    let tiny = Limits {
        max_total_decoded_bytes: gzip.len() + raw.len(),
        ..Limits::default()
    };
    assert_eq!(
        decode_fsom(&gzip, &tiny).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    let tiny = Limits {
        max_vertices: 2,
        ..Limits::default()
    };
    assert!(decode_fsom_payload(&raw, &tiny).is_err());
    assert!(encode_fsom_payload(&model, &tiny).is_err());
}

#[test]
fn fsom_reads_independent_compressed_gzip_and_requires_deflate_stream_end() {
    // Python 3 gzip.compress(payload(3), mtime=0), generated independently of
    // the Rust gzip writer; includes Huffman coding and backreferences.
    let hex = "1f8b0800000000000203730bf6cf656680009692d4e2122620831188dd19feff87490041030356d0600fc1c87c747974715436c80e26288f118b05e8f6000052d6f8bdb1000000";
    let mut compressed = Vec::new();
    for pair in hex.as_bytes().chunks_exact(2) {
        compressed.push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap());
    }
    let decoded = decode_fsom(&compressed, &Limits::default()).unwrap();
    assert_eq!(
        encode_fsom_payload(&decoded, &Limits::default()).unwrap(),
        payload(3)
    );
    let mut incomplete = encode_fsom(&decoded, &Limits::default()).unwrap();
    // Clear BFINAL while retaining an otherwise complete stored block and
    // correct CRC/ISIZE. All output bytes exist, but DEFLATE never ended.
    incomplete[10] = 0;
    assert!(decode_fsom(&incomplete, &Limits::default()).is_err());
}

#[test]
fn nbhm_retains_house_order_duplicate_overrides_and_no_model_declaration() {
    let mut bytes = b"NBHm".to_vec();
    bytes.extend(1i32.to_le_bytes());
    bytes.extend(2i32.to_le_bytes());
    for z in [4f32, 7.0] {
        bytes.extend((-5i16).to_le_bytes());
        for f in [1f32, -0.0, z] {
            bytes.extend(f.to_le_bytes());
        }
    }
    bytes.push(0);
    let value = decode_nbhm(&bytes, &Limits::default()).unwrap();
    assert_eq!(value.houses.len(), 2);
    assert_eq!(value.house(-5).unwrap().position[2].get(), 7.0);
    assert_eq!(encode_nbhm(&value, &Limits::default()).unwrap(), bytes);
    for end in 0..bytes.len() {
        assert!(decode_nbhm(&bytes[..end], &Limits::default()).is_err());
    }
}
