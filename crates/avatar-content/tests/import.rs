use wonderland_avatar_content::readers::*;
use wonderland_avatar_content::*;
use wonderland_legacy_formats::Limits;

fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|x| x.to_be_bytes()).collect()
}
fn key(file_id: u32, type_id: u32) -> ResourceKey {
    ResourceKey {
        group_id: 0,
        file_id,
        type_id,
    }
}
fn file(file_id: u32, type_id: u32) -> FileKey {
    FileKey { file_id, type_id }
}
fn skeleton() -> Vec<u8> {
    let mut v = u32s(&[1]);
    v.extend([5]);
    v.extend(b"adult");
    v.extend(1i16.to_be_bytes());
    v.extend(u32s(&[0]));
    v.push(4);
    v.extend(b"ROOT");
    v.push(4);
    v.extend(b"NULL");
    v.push(0);
    for x in [2f32, 0., 0., 0., 0., 0., 1.] {
        v.extend(x.to_le_bytes());
    }
    v.extend(u32s(&[1, 1, 1]));
    v.extend([0; 8]);
    v
}
fn mesh(x: f32) -> Vec<u8> {
    let mut v = u32s(&[2, 1]);
    v.push(4);
    v.extend(b"ROOT");
    v.extend(u32s(&[1, 0, 1, 2, 1, 0, 0, 3, 0, 0, 3]));
    for uv in [[0f32, 0.], [1., 0.], [0., 1.]] {
        for x in uv {
            v.extend(x.to_le_bytes());
        }
    }
    v.extend(u32s(&[0, 3]));
    for p in [[x, 0., 0.], [x, 1., 0.], [x + 1., 0., 0.]] {
        for x in p.into_iter().chain([0., 0., 1.]) {
            v.extend(x.to_le_bytes());
        }
    }
    v
}
fn png() -> Vec<u8> {
    // Valid original encoded texture bytes, decoded by browser/native contract.
    vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 240, 31,
        0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
}
struct Owned {
    name: String,
    bytes: Vec<u8>,
    key: Option<ResourceKey>,
}
fn original_set() -> Vec<Owned> {
    let mut r = vec![Owned {
        name: "adult.skel".into(),
        bytes: skeleton(),
        key: None,
    }];
    for (name, k, bytes) in [
        ("heads.col", None, u32s(&[1, 7, 100, 10])),
        ("bodies.col", None, u32s(&[1, 9, 101, 10])),
        ("head.po", Some(key(100, 10)), u32s(&[1, 0, 8, 0, 200, 11])),
        ("body.po", Some(key(101, 10)), u32s(&[1, 1, 8, 0, 201, 11])),
        (
            "head.oft",
            Some(key(200, 11)),
            u32s(&[1, 0, 300, 12, 301, 12, 302, 12, 37, 0]),
        ),
        (
            "body.oft",
            Some(key(201, 11)),
            u32s(&[1, 0, 400, 12, 401, 12, 402, 12, 37, 0]),
        ),
    ] {
        r.push(Owned {
            name: name.into(),
            key: k,
            bytes,
        });
    }
    for id in [300, 301, 302, 400, 401, 402] {
        r.push(Owned {
            name: format!("{id}.apr"),
            key: Some(key(id, 12)),
            bytes: u32s(&[1, 0, 0, 1, id, 13]),
        });
        let mut b = u32s(&[1]);
        b.push(4);
        b.extend(b"ROOT");
        b.extend(u32s(&[8, 0, id, 14, 8, 0, 500, 15]));
        r.push(Owned {
            name: format!("{id}.bnd"),
            key: Some(key(id, 13)),
            bytes: b,
        });
        r.push(Owned {
            name: format!("{id}.mesh"),
            key: Some(key(id, 14)),
            bytes: mesh(id as f32),
        });
    }
    let mut h = u32s(&[1]);
    for skin in 0..3 {
        for _ in 0..6 {
            h.extend(u32s(&[400 + skin, 12]));
        }
    }
    r.push(Owned {
        name: "hands.hag".into(),
        key: Some(key(37, 18)),
        bytes: h,
    });
    r.push(Owned {
        name: "skin.png".into(),
        key: Some(key(500, 15)),
        bytes: png(),
    });
    r
}
fn load(r: &[Owned]) -> ImportedContent {
    import(
        ImportRequest {
            files: r
                .iter()
                .map(|r| NamedBytes {
                    name: &r.name,
                    bytes: &r.bytes,
                    key: r.key,
                })
                .collect(),
            skeleton_name: "adult.skel",
            collections: vec![
                CollectionSpec {
                    name: "heads.col".into(),
                    role: CollectionRole::Head,
                },
                CollectionSpec {
                    name: "bodies.col".into(),
                    role: CollectionRole::Body,
                },
            ],
        },
        &ImportLimits::default(),
    )
    .unwrap()
}

// Original .anim layout, deliberately distinct internal and provider names.
fn animation(name: &str, first: f32, last: f32) -> Vec<u8> {
    let mut bytes = u32s(&[2]);
    bytes.extend((name.len() as i16).to_be_bytes());
    bytes.extend(name.as_bytes());
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend(0f32.to_le_bytes());
    bytes.push(0);
    bytes.extend(u32s(&[2]));
    for x in [first, last] {
        for value in [x, 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes.extend(u32s(&[0, 1, 0]));
    bytes.push(4);
    bytes.extend(b"ROOT");
    bytes.extend(u32s(&[2]));
    bytes.extend(1000f32.to_le_bytes());
    bytes.extend([1, 0]);
    bytes.extend(0i32.to_be_bytes());
    bytes.extend((-1i32).to_be_bytes());
    bytes.extend([0, 0]);
    bytes
}

#[test]
fn original_named_animation_import_and_posed_composition_preserve_source_frame() {
    use wonderland_avatar_view::{sample_timeline, Timeline, TimelineLayer};
    let mut resources = original_set();
    resources.push(Owned {
        name: "Folder/Provider_Name.0000000100000007.anim".into(),
        bytes: animation("internal", 2., 6.),
        key: None,
    });
    let content = load(&resources);
    let clip = content.animation("pROVIDER_nAME.anim").unwrap();
    assert_eq!(clip.source().name, "internal");
    assert!(content.animation("internal.anim").is_err());
    let rig = content.rig.as_ref().unwrap();
    let mut pose = rig.bind_pose();
    sample_timeline(
        rig,
        &mut pose,
        &Timeline {
            layers: vec![TimelineLayer {
                clip,
                current_frame: 0.5,
                speed: 1.,
                weight: 1.,
                backwards: false,
                end_reached: false,
                looping: false,
            }],
            carry: None,
        },
        0.,
    )
    .unwrap();
    let selection = AppearanceSelection {
        head: Some(file(200, 11)),
        left: Gesture::None,
        right: Gesture::None,
        ..Default::default()
    };
    let bind = content.compose(&selection).unwrap();
    let posed = content.compose_at(&selection, &pose).unwrap();
    assert_eq!(bind[0].mesh.vertices[0].position.x, -302.);
    assert_eq!(posed[0].mesh.vertices[0].position.x, -304.);
    assert_eq!(posed[0].texture, bind[0].texture);
    assert_eq!(pose.locals[0].translation.x, -4.);
}

#[test]
fn named_animation_ambiguity_and_aggregate_limits_never_choose_an_arbitrary_clip() {
    let mut resources = original_set();
    for (folder, x) in [("one", 2.), ("two", 9.)] {
        resources.push(Owned {
            name: format!("{folder}/Duplicate.anim"),
            bytes: animation("duplicate", x, x + 1.),
            key: None,
        });
    }
    let content = load(&resources);
    assert!(content.animation("duplicate.anim").is_err());
    assert!(content
        .issues
        .iter()
        .any(|issue| issue.kind == IssueKind::Ambiguous && issue.resource == "duplicate.anim"));
    resources.pop();
    let content = import(
        ImportRequest {
            files: resources
                .iter()
                .map(|r| NamedBytes {
                    name: &r.name,
                    bytes: &r.bytes,
                    key: r.key,
                })
                .collect(),
            skeleton_name: "adult.skel",
            collections: vec![],
        },
        &ImportLimits {
            max_total_animation_samples: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(content.animation("duplicate.anim").is_err());
    assert!(content
        .issues
        .iter()
        .any(|issue| issue.kind == IssueKind::Limit && issue.resource.contains("animation")));
}
#[test]
fn source_readers_retain_order_ids_prefix_and_hand_order() {
    let col = decode_collection(
        &u32s(&[2, 9, 0xaabbccdd, 0x11223344, 3, 1, 2]),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(col[0].index, 9);
    assert_eq!(col[1].index, 3);
    assert_eq!(col[0].purchasable.packed(), 0xaabbccdd11223344);
    let po = decode_purchasable(
        &u32s(&[1, 1, 8, 0xdeadbeef, 0xaabbccdd, 0x11223344]),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(po.prefix, [0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(po.outfit, col[0].purchasable);
    assert_eq!(po.trailer, None);
    let po = decode_purchasable(
        &u32s(&[2, 0, 8, 0xa96f6d42, 0x185, 13, 0]),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(po.outfit, file(0x185, 13));
    assert_eq!(po.trailer, Some([0; 4]));
    assert!(decode_purchasable(
        &u32s(&[2, 0, 8, 0xa96f6d42, 0x185, 13, 0, 0]),
        &Limits::default()
    )
    .is_err());
    let mut bytes = u32s(&[1]);
    for i in 0..18 {
        bytes.extend(u32s(&[i + 100, 12]));
    }
    let h = decode_hand_group(&bytes, &Limits::default()).unwrap();
    assert_eq!(h.skins[0].right.idle.file_id, 100);
    assert_eq!(h.skins[0].right.fist.file_id, 101);
    assert_eq!(h.skins[0].right.pointing.file_id, 102);
    assert_eq!(h.skins[0].left.idle.file_id, 103);
    assert_eq!(h.skins[2].left.pointing.file_id, 117);
}
#[test]
fn standalone_identity_uses_only_native_packed_id_filename_convention() {
    assert_eq!(
        source_filename_key("Avatar/Outfits/original-name.aabbccdd11223344.oft"),
        Some(key(0xaabbccdd, 0x11223344))
    );
    assert_eq!(source_filename_key("original-name.oft"), None);
    assert_eq!(source_filename_key("300.oft"), None);
    assert_eq!(source_filename_key("original-name.nothex.oft"), None);
    let mut r = original_set();
    for resource in &mut r {
        if let Some(k) = resource.key {
            let ext = resource.name.rsplit('.').next().unwrap();
            resource.name = format!(
                "original.{:016x}.{ext}",
                ((k.file_id as u64) << 32) | k.type_id as u64
            );
            resource.key = None;
        }
    }
    let c = load(&r);
    assert!(
        c.choices.iter().flat_map(|c| &c.skins).all(|s| s.ready),
        "{:?}",
        c.issues
    );
}
#[test]
fn original_chains_compose_independent_head_body_and_skin_once() {
    let r = original_set();
    let c = load(&r);
    assert!(c.issues.is_empty(), "{:?}", c.issues);
    assert_eq!(c.choices.len(), 2);
    assert_eq!(c.choices[0].key, "vitaboy:000000640000000a");
    assert!(c.choices.iter().flat_map(|c| &c.skins).all(|s| s.ready));
    let s = AppearanceSelection {
        head: Some(file(200, 11)),
        body: Some(file(201, 11)),
        skin: Skin::Dark,
        left: Gesture::None,
        right: Gesture::None,
        ..Default::default()
    };
    let parts = c.compose(&s).unwrap();
    assert_eq!(parts.len(), 2);
    let head = parts.iter().find(|p| p.role == PartRole::Head).unwrap();
    let body = parts.iter().find(|p| p.role == PartRole::Body).unwrap();
    assert_eq!(head.mesh.vertices[0].position.x, -304.);
    assert_eq!(body.mesh.vertices[0].position.x, -404.);
    assert_eq!(head.mesh.vertices[1].uv.x, 1.);
    assert_eq!(head.mesh.indices, vec![0, 1, 2]);
    let only_head = c
        .compose(&AppearanceSelection {
            body: None,
            ..s.clone()
        })
        .unwrap();
    assert_eq!(only_head.len(), 1);
    assert_eq!(only_head[0].mesh, head.mesh);
    assert_eq!(c.textures[&head.texture].bytes, png());
}
#[test]
fn missing_dependencies_are_detailed_and_never_replaced() {
    let mut r = original_set();
    r.retain(|r| r.name != "300.mesh" && r.name != "skin.png");
    let c = load(&r);
    assert!(!c.choices[0].skins[0].ready);
    let issues = &c.choices[0].skins[0].issues;
    assert!(issues.iter().any(|i| i.resource.contains("mesh")));
    assert!(issues.iter().any(|i| i.resource.contains("texture")));
    assert!(c
        .compose(&AppearanceSelection {
            head: Some(file(200, 11)),
            ..Default::default()
        })
        .is_err());
    assert!(!c.catalog.meshes.contains_key(&key(300, 14)));
    assert!(c.textures.is_empty());
}
#[test]
fn ambiguous_original_ids_are_rejected_without_precedence() {
    let mut r = original_set();
    r.push(Owned {
        name: "duplicate.mesh".into(),
        key: Some(key(300, 14)),
        bytes: mesh(1.),
    });
    let c = load(&r);
    assert!(c.issues.iter().any(|i| i.kind == IssueKind::Ambiguous));
    assert!(!c.choices[0].skins[0].ready);
}
#[test]
fn source_lookup_preserves_binding_group_but_resolves_by_file_type() {
    let mut r = original_set();
    let binding = r.iter_mut().find(|r| r.name == "300.bnd").unwrap();
    binding.bytes[13..17].copy_from_slice(&7u32.to_be_bytes());
    binding.bytes[29..33].copy_from_slice(&9u32.to_be_bytes());
    let c = load(&r);
    assert!(c.choices[0].skins[0].ready, "{:?}", c.issues);
    assert_eq!(c.catalog.bindings[&file(300, 13)].mesh.unwrap().group_id, 7);
    assert!(c.catalog.meshes.contains_key(&ResourceKey {
        group_id: 7,
        ..key(300, 14)
    }));
    r.push(Owned {
        name: "another-group.mesh".into(),
        key: Some(ResourceKey {
            group_id: 7,
            ..key(300, 14)
        }),
        bytes: mesh(300.),
    });
    let c = load(&r);
    assert!(!c.choices[0].skins[0].ready);
    assert!(c.issues.iter().any(|i| i.kind == IssueKind::Ambiguous));
}
#[test]
fn aggregate_choice_budget_and_repeated_collection_specs_are_rejected() {
    let r = original_set();
    let run = |collections: Vec<CollectionSpec>, max_choices| {
        let limits = ImportLimits {
            max_choices,
            ..Default::default()
        };
        import(
            ImportRequest {
                files: r
                    .iter()
                    .map(|r| NamedBytes {
                        name: &r.name,
                        bytes: &r.bytes,
                        key: r.key,
                    })
                    .collect(),
                skeleton_name: "adult.skel",
                collections,
            },
            &limits,
        )
        .err()
        .unwrap()
    };
    let head = || CollectionSpec {
        name: "heads.col".into(),
        role: CollectionRole::Head,
    };
    assert_eq!(run(vec![head(), head()], 100).kind, IssueKind::Ambiguous);
    assert_eq!(
        run(
            vec![
                head(),
                CollectionSpec {
                    name: "bodies.col".into(),
                    role: CollectionRole::Body
                }
            ],
            1
        )
        .kind,
        IssueKind::Limit
    );
}
#[test]
fn corrupt_and_bounded_resources_fail_before_allocation() {
    assert!(decode_collection(&u32s(&[u32::MAX]), &Limits::default()).is_err());
    assert!(decode_collection(&u32s(&[100_001]), &Limits::default()).is_err());
    assert!(decode_purchasable(&u32s(&[1, 0, 4, 0, 100, 11]), &Limits::default()).is_err());
    assert!(decode_hand_group(&u32s(&[1, 1, 12]), &Limits::default()).is_err());
    let r = original_set();
    let limits = ImportLimits {
        max_total_resource_bytes: 5,
        ..Default::default()
    };
    assert_eq!(
        import(
            ImportRequest {
                files: r
                    .iter()
                    .map(|r| NamedBytes {
                        name: &r.name,
                        bytes: &r.bytes,
                        key: r.key
                    })
                    .collect(),
                skeleton_name: "adult.skel",
                collections: vec![]
            },
            &limits
        )
        .err()
        .unwrap()
        .kind,
        IssueKind::Limit
    );
}
#[test]
fn texture_decoder_contract_rejects_unknown_and_wrong_pixel_length() {
    let c = load(&original_set());
    let k = *c.textures.keys().next().unwrap();
    assert!(c
        .validate_decoded_texture(
            k,
            &RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![[255, 0, 0, 255]]
            }
        )
        .is_ok());
    assert!(c
        .validate_decoded_texture(
            k,
            &RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![]
            }
        )
        .is_err());
    assert!(c
        .validate_decoded_texture(
            AssetKey([0; 32]),
            &RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![[0; 4]]
            }
        )
        .is_err());
}
#[test]
fn thumbnails_use_original_appearance_reference_separate_from_uv_texture() {
    let mut r = original_set();
    let a = r.iter_mut().find(|r| r.name == "300.apr").unwrap();
    a.bytes[4..12].copy_from_slice(&u32s(&[700, 16]));
    let mut bytes = png();
    bytes.push(0); // Distinct encoded source asset; decoder owns payload validity.
    r.push(Owned {
        name: "head-thumbnail.png".into(),
        key: Some(key(700, 16)),
        bytes: bytes.clone(),
    });
    let c = load(&r);
    let choice = &c.choices[0];
    assert_eq!(choice.thumbnail_keys[0], Some(file(700, 16)));
    let asset = choice.thumbnails[0].unwrap();
    assert_eq!(c.textures[&asset].name, "head-thumbnail.png");
    assert_eq!(c.textures[&asset].bytes, bytes);
    assert_ne!(Some(asset), c.catalog.textures.get(&key(500, 15)).copied());
    assert!(choice.thumbnails[1].is_none());
}
#[test]
fn far3_original_ids_extract_without_filename_mapping() {
    let source = original_set();
    let mut archive = b"FAR!byAZ".to_vec();
    archive.extend(3u32.to_le_bytes());
    archive.extend([0; 4]);
    let mut manifest = Vec::new();
    manifest.extend((source.len() as u32).to_le_bytes());
    for (i, r) in source.iter().enumerate() {
        let offset = archive.len();
        archive.extend(&r.bytes);
        let k = r.key.unwrap_or(key(900 + i as u32, 99));
        manifest.extend((r.bytes.len() as u32).to_le_bytes());
        manifest.extend([0; 4]);
        manifest.extend((offset as u32).to_le_bytes());
        manifest.extend([0, 0]);
        manifest.extend((r.name.len() as u16).to_le_bytes());
        manifest.extend(k.type_id.to_le_bytes());
        manifest.extend(k.file_id.to_le_bytes());
        manifest.extend(r.name.as_bytes());
    }
    let offset = archive.len() as u32;
    archive[12..16].copy_from_slice(&offset.to_le_bytes());
    archive.extend(manifest);
    let list = inventory(
        &[NamedBytes {
            name: "original.dat",
            bytes: &archive,
            key: None,
        }],
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(list.len(), source.len());
    assert!(list
        .iter()
        .any(|r| r.name == "adult.skel" && r.kind == ResourceKind::Skeleton));
    assert!(list
        .iter()
        .any(|r| r.name == "heads.col" && r.kind == ResourceKind::Collection));
    let c = import(
        ImportRequest {
            files: vec![NamedBytes {
                name: "original.dat",
                bytes: &archive,
                key: None,
            }],
            skeleton_name: "adult.skel",
            collections: vec![CollectionSpec {
                name: "heads.col".into(),
                role: CollectionRole::Head,
            }],
        },
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(c.choices.len(), 1);
    assert!(c.choices[0].skins.iter().all(|s| s.ready));
    assert!(c.issues.is_empty(), "{:?}", c.issues);
    archive[12..16].copy_from_slice(&0u32.to_le_bytes());
    assert!(import(
        ImportRequest {
            files: vec![NamedBytes {
                name: "bad.dat",
                bytes: &archive,
                key: None
            }],
            skeleton_name: "adult.skel",
            collections: vec![]
        },
        &ImportLimits::default()
    )
    .is_err());
}
