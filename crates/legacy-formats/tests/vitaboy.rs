use wonderland_legacy_formats::{vitaboy::*, ErrorKind, Limits};

fn be(n: u32, out: &mut Vec<u8>) {
    out.extend(n.to_be_bytes());
}
fn float(n: f32, out: &mut Vec<u8>) {
    out.extend(n.to_le_bytes());
}
fn string(s: &str, out: &mut Vec<u8>) {
    out.push(s.len() as u8);
    out.extend(s.as_bytes());
}

// Built from Animation.Read's field order, not from a Rust writer. Integers are
// BE, but IoBuffer.ReadFloat calls BinaryReader.ReadSingle and is always LE.
fn animation() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 2, 0, 4, b't', b'e', b's', b't'];
    float(1000.0, &mut b);
    float(2.5, &mut b);
    b.push(1);
    be(1, &mut b);
    for f in [1.0, -0.0, 3.0] {
        float(f, &mut b);
    }
    be(1, &mut b);
    for f in [0.0, 0.0, 0.0, 1.0] {
        float(f, &mut b);
    }
    be(1, &mut b);
    be(123, &mut b);
    string("ROOT", &mut b);
    be(1, &mut b);
    float(1000.0, &mut b);
    b.extend([1, 1]);
    be(0, &mut b);
    be(0, &mut b);
    b.push(0);
    b.push(1);
    be(1, &mut b);
    be(3, &mut b);
    for (time, val) in [(200u32, "9"), (50, "3"), (50, "4")] {
        be(time, &mut b);
        be(1, &mut b);
        be(2, &mut b);
        string("xevt", &mut b);
        string(val, &mut b);
        string("xevt", &mut b);
        string("99", &mut b);
    }
    b
}

fn skeleton(parents: &[(&str, &str)]) -> Vec<u8> {
    let mut b = vec![0, 0, 0, 1, 3, b'r', b'i', b'g'];
    b.extend((parents.len() as i16).to_be_bytes());
    for (i, (name, parent)) in parents.iter().enumerate() {
        be(i as u32, &mut b);
        string(name, &mut b);
        string(parent, &mut b);
        b.push(0);
        for f in [i as f32 + 1.0, -0.0, 2.0, 0.0, 0.0, 0.0, 1.0] {
            float(f, &mut b);
        }
        for n in [1, 1, 0] {
            be(n, &mut b);
        }
        float(0.0, &mut b);
        float(0.5, &mut b);
    }
    b
}

#[test]
fn animation_preserves_mixed_endian_float_bits_and_time_property_order() {
    let a = decode_animation(&animation(), &Limits::default()).unwrap();
    assert_eq!(a.duration_ms.0, 0x447a0000);
    assert_eq!(a.distance.0, 0x40200000);
    assert_eq!(
        a.translations[0].map(|f| f.0),
        [0xbf800000, 0x80000000, 0x40400000]
    );
    assert_eq!(
        a.rotations[0].map(|f| f.0),
        [0, 0x80000000, 0x80000000, 0xbf800000]
    );
    assert_eq!(a.num_frames, 1);
    let t: Vec<_> = a.time_properties_in_source_order().collect();
    assert_eq!(t.iter().map(|p| p.id).collect::<Vec<_>>(), [200, 50, 50]);
    assert_eq!(
        t[0].properties.items[0].pairs,
        [("xevt".into(), "9".into()), ("xevt".into(), "99".into())]
    );
    assert_eq!(a.coordinate_policy, CoordinatePolicy::FreeSo);
}

#[test]
fn skeleton_retains_file_order_and_resolves_forward_parents() {
    let s = decode_skeleton(
        &skeleton(&[("hand", "ROOT"), ("ROOT", "NULL")]),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(s.root, 1);
    assert_eq!(s.bones[0].parent, Some(1));
    assert_eq!(s.bones[1].children, [0]);
    assert_eq!(s.bones[0].translation[0].0, 0xbf800000);
}

#[test]
fn skeleton_rejects_cycles_missing_parents_and_duplicates() {
    for pairs in [
        vec![("a", "b"), ("b", "a")],
        vec![("a", "NULL"), ("b", "missing")],
        vec![("a", "NULL"), ("a", "a")],
    ] {
        assert!(decode_skeleton(&skeleton(&pairs), &Limits::default()).is_err());
    }
}

#[test]
fn animation_rejects_nonfinite_counts_and_all_truncated_prefixes() {
    let b = animation();
    for end in 0..b.len() {
        assert!(
            decode_animation(&b[..end], &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut nan = b.clone();
    nan[10..14].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(
        decode_animation(&nan, &Limits::default()).unwrap_err().kind,
        ErrorKind::InvalidData
    );
    let limits = Limits {
        max_frames: 0,
        ..Limits::default()
    };
    assert_eq!(
        decode_animation(&b, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn binding_and_appearance_keep_identifier_order() {
    let binding = [
        0, 0, 0, 1, 1, b'b', 0, 0, 0, 8, 0, 0, 0, 11, 0, 0, 0, 12, 0, 0, 0, 13, 0, 0, 0, 8, 0, 0,
        0, 21, 0, 0, 0, 22, 0, 0, 0, 23,
    ];
    let b = decode_binding(&binding, &Limits::default()).unwrap();
    assert_eq!(
        b.mesh.unwrap(),
        ResourceKey {
            group_id: 11,
            file_id: 12,
            type_id: 13
        }
    );
    assert_eq!(b.texture.unwrap().type_id, 23);
    let appearance = [
        0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0, 2, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0, 0, 6, 0, 0,
        0, 7,
    ];
    let a = decode_appearance(&appearance, &Limits::default()).unwrap();
    assert_eq!(
        a.thumbnail,
        FileKey {
            file_id: 4,
            type_id: 5
        }
    );
    assert_eq!(
        a.bindings,
        [
            FileKey {
                file_id: 8,
                type_id: 9
            },
            FileKey {
                file_id: 6,
                type_id: 7
            }
        ]
    );
}

fn mesh() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 2, 0, 0, 0, 1];
    string("ROOT", &mut b);
    be(1, &mut b);
    for n in [2, 0, 1] {
        be(n, &mut b);
    }
    be(1, &mut b);
    for n in [0, 0, 3, 0, 1] {
        be(n, &mut b);
    }
    be(3, &mut b);
    for n in [0.0, 0.0, 1.0, 0.0, 0.0, 1.0] {
        float(n, &mut b);
    }
    for n in [1, 16384, 2, 3] {
        be(n, &mut b);
    }
    for n in [
        1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 2.0, 3.0, 4.0, 0.0, 1.0, 0.0, 3.0, 4.0, 5.0, 1.0, 0.0, 0.0,
        4.0, 5.0, 6.0, 0.0, 0.0, 1.0,
    ] {
        float(n, &mut b);
    }
    b
}

#[test]
fn mesh_keeps_triangle_order_bone_ranges_blend_weights_and_zero_normals() {
    let m = decode_mesh(&mesh(), &Limits::default()).unwrap();
    assert_eq!(m.faces, [[2, 0, 1]]);
    assert_eq!(m.bone_names, ["ROOT"]);
    assert_eq!(m.vertices[0].position.map(|f| f.get()), [-1.0, 2.0, 3.0]);
    assert_eq!(m.vertices[0].normal.map(|f| f.0), [0x80000000, 0, 0]);
    assert_eq!(
        m.vertices[0].effective_normal().map(|f| f.get()),
        [0.0, 1.0, 0.0]
    );
    assert_eq!(m.blend_vertices[0].raw_weight, 16384);
    assert_eq!(m.blend_vertices[0].weight().get(), 0.5);
    assert_eq!(m.blend_vertices[0].other_vertex, 2);
    assert_eq!(m.bindings[0].real_vertex_count, 3);
}

#[test]
fn mesh_rejects_bad_indices_bindings_counts_and_all_truncated_prefixes() {
    let b = mesh();
    for end in 0..b.len() {
        assert!(
            decode_mesh(&b[..end], &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    for (at, value) in [(17, 3u32), (33, 1), (37, 1), (4, u32::MAX)] {
        let mut bad = b.clone();
        bad[at..at + 4].copy_from_slice(&value.to_be_bytes());
        assert!(decode_mesh(&bad, &Limits::default()).is_err(), "field {at}");
    }
    let limits = Limits {
        max_vertices: 3,
        ..Limits::default()
    };
    assert_eq!(
        decode_mesh(&b, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn animation_invalid_indices_and_float_policy_are_explicit() {
    for value in [1u32, u32::MAX] {
        let mut b = animation();
        b[78..82].copy_from_slice(&value.to_be_bytes());
        assert!(decode_animation(&b, &Limits::default()).is_err());
    }
    let mut b = animation();
    b[10..14].copy_from_slice(&(-1.0f32).to_le_bytes());
    assert!(decode_animation(&b, &Limits::default()).is_err());
    assert_eq!(
        decode_animation(&animation(), &Limits::default())
            .unwrap()
            .frames_per_second(),
        Some(1)
    );
    let s = skeleton(&[("ROOT", "NULL"), ("hand", "ROOT")]);
    let limits = Limits {
        max_depth: 1,
        ..Limits::default()
    };
    assert_eq!(
        decode_skeleton(&s, &limits).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
}

#[test]
fn coordinate_conversion_is_bit_exact_involution_including_signed_zero() {
    let vector = [F32Bits(0x80000000), F32Bits(1), F32Bits(0x7f7fffff)];
    assert_eq!(
        CoordinatePolicy::FreeSo.vector(CoordinatePolicy::FreeSo.vector(vector)),
        vector
    );
    let q = [
        F32Bits(0),
        F32Bits(0x80000000),
        F32Bits(0x3f800000),
        F32Bits(0xbf800000),
    ];
    assert_eq!(
        CoordinatePolicy::FreeSo.quaternion(CoordinatePolicy::FreeSo.quaternion(q)),
        q
    );
    assert_eq!(CoordinatePolicy::Source.vector(vector), vector);
}

#[test]
fn outfit_resource_order_and_unknown_field_are_retained() {
    let mut b = Vec::new();
    for n in [1u32, 0x12345678, 10, 11, 20, 21, 30, 31, 40, 50] {
        be(n, &mut b);
    }
    let o = decode_outfit(&b, &Limits::default()).unwrap();
    assert_eq!(o.unknown, 0x12345678);
    assert_eq!(
        o.light_appearance,
        FileKey {
            file_id: 10,
            type_id: 11
        }
    );
    assert_eq!(
        o.medium_appearance,
        FileKey {
            file_id: 20,
            type_id: 21
        }
    );
    assert_eq!(
        o.dark_appearance,
        FileKey {
            file_id: 30,
            type_id: 31
        }
    );
    assert_eq!((o.hand_group, o.region), (40, 50));
    for end in 0..b.len() {
        assert!(decode_outfit(&b[..end], &Limits::default()).is_err());
    }
}
