use wonderland_legacy_formats::{vitaboy::*, Limits};

fn int(b: &mut Vec<u8>, n: i32) {
    b.extend(n.to_le_bytes());
}
fn float(b: &mut Vec<u8>, n: f32) {
    b.extend(n.to_le_bytes());
}
fn string(b: &mut Vec<u8>, s: &str) {
    b.push(s.len() as u8);
    b.extend(s.as_bytes());
}
fn bcf_bytes() -> Vec<u8> {
    let mut b = Vec::new();
    int(&mut b, 1);
    string(&mut b, "rig");
    b.extend(1i16.to_le_bytes());
    string(&mut b, "");
    string(&mut b, "skipped");
    string(&mut b, "ROOT");
    string(&mut b, "NULL");
    int(&mut b, 1);
    int(&mut b, 1);
    string(&mut b, "kind");
    string(&mut b, "body");
    for n in [1.0, -0.0, 2.0, 0.0, 0.0, 0.0, 1.0] {
        float(&mut b, n);
    }
    for n in [1, 1, 0] {
        int(&mut b, n);
    }
    float(&mut b, 0.0);
    float(&mut b, 0.5);
    int(&mut b, 1);
    string(&mut b, "look");
    int(&mut b, 7);
    int(&mut b, 9);
    int(&mut b, 1);
    string(&mut b, "ROOT");
    string(&mut b, "body");
    int(&mut b, 3);
    int(&mut b, 0);
    int(&mut b, 1);
    string(&mut b, "wave");
    string(&mut b, "skill.cfp");
    float(&mut b, 1000.0);
    float(&mut b, 2.5);
    int(&mut b, 258);
    int(&mut b, 2);
    int(&mut b, 1);
    int(&mut b, 1);
    string(&mut b, "ROOT");
    int(&mut b, 1);
    float(&mut b, 1000.0);
    int(&mut b, 1);
    int(&mut b, 1);
    int(&mut b, 0);
    int(&mut b, 0);
    int(&mut b, 1);
    int(&mut b, 2);
    string(&mut b, "xevt");
    string(&mut b, "9");
    string(&mut b, "xevt");
    string(&mut b, "8");
    int(&mut b, 1);
    int(&mut b, 2);
    for (id, value) in [(200, "7"), (50, "3")] {
        int(&mut b, id);
        int(&mut b, 1);
        string(&mut b, "xevt");
        string(&mut b, value);
    }
    b
}
fn cfp_bytes() -> Vec<u8> {
    let mut b = vec![0xff];
    float(&mut b, 1.0);
    b.extend([0xfe, 0, 0]);
    b.extend([0xfe, 1, 0, 0xfe, 1, 0]);
    b.extend([0xfe, 0, 0, 0xfe, 0, 0, 0xfe, 0, 0, 0xff]);
    float(&mut b, 1.0);
    b
}
#[test]
fn bcf_preserves_binary_headers_skipped_bones_and_animation_metadata() {
    let bytes = bcf_bytes();
    let b = decode_bcf(&bytes, LegacyEncoding::Binary, &Limits::default()).unwrap();
    assert_eq!(b.skeletons[0].skipped_bones[0].before_index, 0);
    assert_eq!(b.skeletons[0].skeleton.bones[0].translation[0].get(), -1.0);
    assert_eq!(b.appearances[0].bindings[0].mesh_name, "body");
    assert_eq!(b.animations[0].is_moving, 258);
    assert_eq!(
        b.animations[0].motions[0].time_properties[0]
            .items
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [200, 50]
    );
    assert_eq!(
        encode_bcf(&b, LegacyEncoding::Binary, &Limits::default()).unwrap(),
        bytes
    );
    assert!(encode_bcf(&b, LegacyEncoding::Text, &Limits::default()).is_err());
    let cmx = encode_bcf_text(
        &b,
        LegacyTextPolicy::NormalizeSignedZero,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(cmx.normalized_signed_zeros, 1);
    let c = decode_bcf(&cmx.bytes, LegacyEncoding::Text, &Limits::default()).unwrap();
    let mut normalized_skeletons = b.skeletons.clone();
    normalized_skeletons[0].skeleton.bones[0].translation[1] = F32Bits(0);
    assert_eq!(c.skeletons, normalized_skeletons);
    assert_eq!(c.appearances, b.appearances);
    assert_eq!(c.animations, b.animations);
    assert_eq!(c.text_version, Some(300));
    for end in 0..bytes.len() {
        assert!(
            decode_bcf(&bytes[..end], LegacyEncoding::Binary, &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
}

#[test]
fn cmx_uses_original_single_parse_zero_and_requires_explicit_lossy_conversion() {
    // Literal source text; BCFReadString.ReadFloat uses the original framework's
    // Single.Parse, which normalizes textual negative zero before coordinates.
    let text = b"header\nversion 300\n0\n0\n1\npause\nsamples\n-0 -0.0 0 0 0 0\n";
    let mut bcf = decode_bcf(text, LegacyEncoding::Text, &Limits::default()).unwrap();
    assert_eq!(bcf.animations[0].duration_ms.0, 0);
    assert_eq!(bcf.animations[0].distance.0, 0);
    bcf.animations[0].distance = F32Bits(0x80000000);
    assert!(encode_bcf(&bcf, LegacyEncoding::Text, &Limits::default()).is_err());
    assert!(encode_bcf(&bcf, LegacyEncoding::Binary, &Limits::default()).is_ok());
}

#[test]
fn cmx_matches_pinned_framework_numeric_vectors_and_rejects_malformed_groups() {
    // Expected bits obtained by compiling the original BCFReadProxy.cs and
    // calling BCFReadString.ReadFloat under the original Mono framework.
    for (token, expected) in [
        ("-0", 0),
        ("-1e-999", 0),
        ("-1e-45", 0x80000001),
        ("1.17549435e-38", 0x00800000),
        ("1.0000000596046448", 0x3f800000),
        ("1,234.5", 0x449a5000),
        ("1,,2", 0x41400000),
        ("12,34", 0x449a4000),
        ("1,", 0x3f800000),
        ("\t1.25\t", 0x3fa00000),
    ] {
        let text = format!("version 300\n0\n0\n1\npause\nsamples\n0 {token} 0 0 0 0\n");
        let bcf = decode_bcf(text.as_bytes(), LegacyEncoding::Text, &Limits::default())
            .unwrap_or_else(|e| panic!("{token}: {e}"));
        assert_eq!(bcf.animations[0].distance.0, expected, "{token}");
    }
    for token in [
        "3.4028235677973366e38",
        "NaN",
        "Infinity",
        ",1",
        "1.2,3",
        "1e1,0",
    ] {
        let text = format!("version 300\n0\n0\n1\npause\nsamples\n0 {token} 0 0 0 0\n");
        assert!(
            decode_bcf(text.as_bytes(), LegacyEncoding::Text, &Limits::default()).is_err(),
            "{token}"
        );
    }
}

#[test]
fn cfp_delta_table_matches_independent_original_csharp_bits() {
    for (code, expected) in [
        (0u8, 0xbdccce04),
        (1, 0xbdc66148),
        (125, 0xafda1f02),
        (126, 0),
        (127, 0x2fda1f02),
        (251, 0x3dc66148),
        (252, 0x3dccce04),
    ] {
        let frames = decode_cfp(&[code, 126, 126], 1, 0, &Limits::default()).unwrap();
        assert_eq!(frames.translations[0][0].0 ^ 0x80000000, expected);
    }
}

#[test]
fn text_skn_retains_utf8_bone_names_without_binary_pascal_restrictions() {
    let source = "皮膚\n肌\n1\n根\n0\n0\n0\n0\n0\n";
    let mesh = decode_bmf(source.as_bytes(), LegacyEncoding::Text, &Limits::default()).unwrap();
    assert_eq!(mesh.mesh.bone_names, ["根"]);
    let encoded = encode_bmf(&mesh, LegacyEncoding::Text, &Limits::default()).unwrap();
    assert_eq!(
        decode_bmf(&encoded, LegacyEncoding::Text, &Limits::default()).unwrap(),
        mesh
    );
    assert!(encode_bmf(&mesh, LegacyEncoding::Binary, &Limits::default()).is_err());
}

#[test]
fn legacy_nested_allocations_ranges_and_text_precision_are_bounded() {
    let bcf = decode_bcf(&bcf_bytes(), LegacyEncoding::Binary, &Limits::default()).unwrap();
    let bytes = cfp_bytes();
    let limits = Limits {
        max_total_decoded_bytes: 15,
        ..Limits::default()
    };
    assert!(decode_cfp(&bytes, 2, 1, &limits).is_err());
    assert!(bcf.animations[0].enrich(&bytes, &limits).is_err());
    let limits = Limits {
        max_frames: 1,
        ..Limits::default()
    };
    assert!(decode_cfp(&bytes, 2, 1, &limits).is_err());
    assert!(decode_bcf(&bcf_bytes(), LegacyEncoding::Binary, &limits).is_err());
    let mut invalid = bcf.clone();
    invalid.animations[0].motions[0].first_translation_index = 2;
    assert!(encode_bcf(&invalid, LegacyEncoding::Binary, &Limits::default()).is_err());
    for bits in [
        1, 0x80000001, 0x00800000, 0x3f800001, 0x7f7fffff, 0xff7fffff,
    ] {
        let mut value = bcf.clone();
        value.skeletons.clear();
        value.animations[0].distance = F32Bits(bits);
        let text = encode_bcf(&value, LegacyEncoding::Text, &Limits::default()).unwrap();
        let read = decode_bcf(&text, LegacyEncoding::Text, &Limits::default()).unwrap();
        assert_eq!(read.animations[0].distance.0, bits);
    }
}
#[test]
fn cfp_uses_original_repeat_plus_one_component_reset_and_exact_coordinate_bits() {
    let raw = cfp_bytes();
    let frames = decode_cfp(&raw, 2, 1, &Limits::default()).unwrap();
    assert_eq!(frames.translations[0].map(|v| v.get()), [-1.0, 0.0, 0.0]);
    assert_eq!(frames.translations[1], frames.translations[0]);
    assert_eq!(
        frames.rotations[0].map(|v| v.0),
        [0, 0x80000000, 0x80000000, 0xbf800000]
    );
    let b = decode_bcf(&bcf_bytes(), LegacyEncoding::Binary, &Limits::default()).unwrap();
    let a = b.animations[0].enrich(&raw, &Limits::default()).unwrap();
    assert_eq!(a.is_moving, 2);
    assert_eq!(a.motions[0].unknown, 0);
    assert_eq!(
        a.time_properties_in_source_order()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [200, 50]
    );
    let encoded = encode_cfp(&frames, &Limits::default()).unwrap();
    assert_eq!(
        decode_cfp(&encoded, 2, 1, &Limits::default()).unwrap(),
        frames
    );
    assert!(decode_cfp(&[0xfe, 2, 0], 2, 0, &Limits::default()).is_err());
    assert!(decode_cfp(&[0xfd], 1, 0, &Limits::default()).is_err());
    for end in 0..raw.len() {
        assert!(decode_cfp(&raw[..end], 2, 1, &Limits::default()).is_err());
    }
}
#[test]
fn bmf_and_text_skn_preserve_source_weight_order_and_mesh_identity() {
    let mut b = Vec::new();
    string(&mut b, "body");
    string(&mut b, "skin");
    int(&mut b, 1);
    string(&mut b, "ROOT");
    int(&mut b, 1);
    for i in [2, 0, 1] {
        int(&mut b, i);
    }
    int(&mut b, 1);
    for i in [0, 0, 3, 0, 1] {
        int(&mut b, i);
    }
    int(&mut b, 3);
    for f in [0.0, 0.0, 1.0, 0.0, 0.0, 1.0] {
        float(&mut b, f);
    }
    for i in [1, 16384, 2, 3] {
        int(&mut b, i);
    }
    for _ in 0..4 {
        for f in [1.0, -0.0, 3.0, 0.0, 1.0, 0.0] {
            float(&mut b, f);
        }
    }
    let m = decode_bmf(&b, LegacyEncoding::Binary, &Limits::default()).unwrap();
    assert_eq!(m.skin_name, "body");
    assert_eq!(m.mesh.blend_vertices[0].raw_weight, 16384);
    assert_eq!(
        encode_bmf(&m, LegacyEncoding::Binary, &Limits::default()).unwrap(),
        b
    );
    assert!(encode_bmf(&m, LegacyEncoding::Text, &Limits::default()).is_err());
    let text = encode_bmf_text(
        &m,
        LegacyTextPolicy::NormalizeSignedZero,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(text.normalized_signed_zeros, 4);
    let mut normalized = m.clone();
    for v in &mut normalized.mesh.vertices {
        v.position[1] = F32Bits(0);
    }
    for v in &mut normalized.mesh.blend_vertices {
        v.position[1] = F32Bits(0);
    }
    assert_eq!(
        decode_bmf(&text.bytes, LegacyEncoding::Text, &Limits::default()).unwrap(),
        normalized
    );
}
