use serde_json::json;
use wonderland_creator::city::{CityImage, MapLayer, NeighborhoodDocument};
use wonderland_creator::editors::upgrades::UpgradeDocument;
use wonderland_creator::json_support::JsonEdit;
use wonderland_creator::{default_limits, sha256};

fn upgrades() -> Vec<u8> {
    br#"{ "Version":2,"Files":[{"Name":"chair.iff","Subs":[{"Old":"4096:0","New":"V11"}],"Groups":[{"Name":"comfort","Tuning":["4096:0","4096:1"],"DefaultValue":"V5"}],"Upgrades":[{"Name":"1","Price":"$50","Ad":"comfort:50","Subs":[{"Old":"G0","New":"V30"},{"Old":"4096:0","New":"V20"},{"Old":"4096:2","New":"C4096:9"}]}],"Config":[{"GUID":"deadbeef","Level":0,"Limit":0,"Special":false,"Reinit":true}],"Future":{"retain":[null,2,"x"]}}]}"#.to_vec()
}

#[test]
fn upgrade_source_groups_overwrite_direct_values_and_missing_constants_are_zero() {
    let limits = default_limits();
    let input = upgrades();
    let doc = UpgradeDocument::import(&input, &limits).unwrap();
    assert_eq!(doc.export(&limits).unwrap(), input);
    let resolved = doc
        .resolve_substitutions("chair.iff", Some(0), |_, _| None)
        .unwrap();
    assert_eq!(resolved[&(4096, 0)], 30);
    assert_eq!(resolved[&(4096, 1)], 30);
    assert_eq!(resolved[&(4096, 2)], 0);
    let defaults = doc
        .resolve_substitutions("chair.iff", None, |_, _| None)
        .unwrap();
    assert_eq!(defaults[&(4096, 0)], 11);
    assert_eq!(defaults[&(4096, 1)], 5);
}

#[test]
fn upgrade_guarded_edit_preserves_unknown_data_and_failed_candidate() {
    let limits = default_limits();
    let input = upgrades();
    let mut doc = UpgradeDocument::import(&input, &limits).unwrap();
    let edit = JsonEdit::set(["Files", "0", "Upgrades", "0", "Price"], json!("R100"));
    assert!(doc
        .apply(&"0".repeat(64), std::slice::from_ref(&edit), &limits)
        .is_err());
    doc.apply(&sha256(&input), &[edit], &limits).unwrap();
    let changed = doc.export(&limits).unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&changed).unwrap();
    assert_eq!(tree["Files"][0]["Future"], json!({"retain":[null,2,"x"]}));
    assert_eq!(tree["Files"][0]["Upgrades"][0]["Price"], "R100");
    assert!(doc
        .apply(
            &sha256(&changed),
            &[JsonEdit::set(
                ["Files", "0", "Upgrades", "0", "Subs", "0", "New"],
                json!("V32768")
            )],
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), changed);
    UpgradeDocument::import(&changed, &limits).unwrap();
}

#[test]
fn upgrades_reject_duplicate_keys_wrong_shapes_and_memory_overcommit() {
    let limits = default_limits();
    assert!(UpgradeDocument::import(br#"{"Version":2,"Version":1,"Files":[]}"#, &limits).is_err());
    assert!(UpgradeDocument::import(br#"[2,[]]"#, &limits).is_err());
    let mut tight = limits;
    tight.max_total_decoded_bytes = upgrades().len() * 2;
    assert!(UpgradeDocument::import(&upgrades(), &tight).is_err());
}

#[test]
fn neighbourhood_source_order_ties_and_distance_multiplier_are_preserved() {
    let input = br#"[{"GUID":"a","Name":"A","Location":{"X":10,"Y":10},"DistanceMul":9.0,"Future":7},{"GUID":"b","Name":"B","Location":{"X":12,"Y":10},"DistanceMul":0.01}]"#;
    let limits = default_limits();
    let mut doc = NeighborhoodDocument::import(input, &limits).unwrap();
    assert_eq!(doc.nearest(11, 10).unwrap(), Some(0));
    assert_eq!(doc.export(&limits).unwrap(), input);
    doc.apply(
        &sha256(input),
        &[JsonEdit::set(["1", "Location", "X"], json!(11))],
        &limits,
    )
    .unwrap();
    assert_eq!(doc.nearest(11, 10).unwrap(), Some(1));
    assert_eq!(doc.entries()[0]["Future"], 7);
    let before = doc.export(&limits).unwrap();
    assert!(doc
        .apply(
            &sha256(&before),
            &[JsonEdit::set(["1", "GUID"], json!("a"))],
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), before);
}

#[test]
fn city_png_pixel_roundtrip_road_reciprocity_and_atomic_boundary_failure() {
    let limits = default_limits();
    let mut map = CityImage::from_rgba(8, 8, vec![[0, 0, 0, 255]; 64], &limits).unwrap();
    map.road_stroke(3, 3, 2, 0, false, &limits).unwrap();
    assert_eq!(map.pixel(3, 3).unwrap()[0], 8);
    assert_eq!(map.pixel(3, 2).unwrap()[0], 2);
    assert_eq!(map.pixel(2, 3).unwrap()[0], 128);
    assert_eq!(map.pixel(2, 2).unwrap()[0], 64);
    assert_eq!(map.pixel(5, 3).unwrap()[0], 16);
    assert_eq!(map.pixel(5, 2).unwrap()[0], 32);
    let png = map.encode_png(&limits).unwrap();
    let restored = CityImage::decode(&png, &limits).unwrap();
    assert_eq!(restored.pixels(), map.pixels());
    let before = map.pixels().to_vec();
    assert!(map.road_stroke(0, 0, 2, 0, false, &limits).is_err());
    assert_eq!(map.pixels(), before);
    map.road_stroke(3, 3, 2, 0, true, &limits).unwrap();
    assert!(map.pixels().iter().all(|p| p[0] == 0));
}

#[test]
fn city_brush_clips_and_preserves_opaque_palette_channels() {
    let limits = default_limits();
    let mut map = CityImage::from_rgba(3, 3, vec![[0, 0, 0, 255]; 9], &limits).unwrap();
    map.brush(0, 0, 1, [255, 0, 0], MapLayer::Terrain, &limits)
        .unwrap();
    assert_eq!(map.pixel(0, 0).unwrap(), [255, 0, 0, 255]);
    assert_eq!(map.pixel(1, 1).unwrap(), [255, 0, 0, 255]);
    assert_eq!(map.pixel(2, 2).unwrap(), [0, 0, 0, 255]);
    assert!(map
        .brush(0, 0, 1, [13, 14, 15], MapLayer::Terrain, &limits)
        .is_err());
    let bmp = map.encode_bmp(&limits).unwrap();
    assert_eq!(
        CityImage::decode(&bmp, &limits).unwrap().pixels(),
        map.pixels()
    );
}

#[test]
fn vitaboy_mesh_edit_preserves_other_vertices_and_reopens_actual_source_binary() {
    use wonderland_creator::editors::assets::{AssetDocument, AssetKind};
    use wonderland_legacy_formats::vitaboy;
    let input=include_bytes!("../../TSOClient/FSO.Content.TSO/Content/Avatar/Meshes/fso-heart-mark.c72160c000000009.mesh");
    let limits = default_limits();
    let source = vitaboy::decode_mesh(input, &limits).unwrap();
    let mut doc = AssetDocument::import(AssetKind::Mesh, input, &limits).unwrap();
    assert_eq!(doc.export(&limits).unwrap(), input);
    let edit = JsonEdit::set(["vertices", "0", "position", "0"], json!(42.5f32.to_bits()));
    doc.apply(&sha256(input), &[edit], &limits).unwrap();
    let output = doc.export(&limits).unwrap();
    let changed = vitaboy::decode_mesh(&output, &limits).unwrap();
    assert_eq!(changed.vertices[0].position[0].get(), 42.5);
    assert_eq!(&changed.vertices[1..], &source.vertices[1..]);
    assert_eq!(changed.faces, source.faces);
    assert_eq!(changed.bindings, source.bindings);
    assert_eq!(changed.blend_vertices, source.blend_vertices);
    let differences = input.iter().zip(&output).filter(|(a, b)| a != b).count();
    assert!(differences <= 4 && differences > 0);
    assert!(doc
        .apply(
            &sha256(&output),
            &[JsonEdit::set(["surprise"], json!(7))],
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), output);
}

#[test]
fn animation_edit_keeps_contact_event_order_and_rejects_nonfinite_transform() {
    use wonderland_creator::editors::assets::{AssetDocument, AssetKind};
    use wonderland_legacy_formats::vitaboy;
    let input=include_bytes!("../../TSOClient/FSO.Content.TSO/Content/Avatar/Animations/a2o-kart-ride.985c76ca00000007.anim");
    let limits = default_limits();
    let source = vitaboy::decode_animation(input, &limits).unwrap();
    let mut doc = AssetDocument::import(AssetKind::Animation, input, &limits).unwrap();
    doc.apply(
        &sha256(input),
        &[JsonEdit::set(
            ["translations", "0", "1"],
            json!(12.25f32.to_bits()),
        )],
        &limits,
    )
    .unwrap();
    let bytes = doc.export(&limits).unwrap();
    let changed = vitaboy::decode_animation(&bytes, &limits).unwrap();
    assert_eq!(changed.translations[0][1].get(), 12.25);
    assert_eq!(changed.motions, source.motions);
    assert_eq!(changed.rotations, source.rotations);
    assert!(doc
        .apply(
            &sha256(&bytes),
            &[JsonEdit::set(
                ["translations", "0", "1"],
                json!(f32::INFINITY.to_bits())
            )],
            &limits
        )
        .is_err());
    assert_eq!(doc.export(&limits).unwrap(), bytes);
}

#[test]
fn checked_in_upgrade_document_is_inspectable_and_preserves_ignored_group_references() {
    let limits = default_limits();
    let source = include_bytes!("../../TSOClient/FSO.Content.TSO/Content/upgrades.json");
    let doc = UpgradeDocument::import(source, &limits).unwrap();
    assert_eq!(doc.export(&limits).unwrap(), source);
    let input =
        br#"{"Version":2,"Files":[{"Name":"x.iff","Subs":[{"Old":"G99","New":"V60000"}]}]}"#;
    let doc = UpgradeDocument::import(input, &limits).unwrap();
    assert!(doc
        .resolve_substitutions("x.iff", None, |_, _| None)
        .unwrap()
        .is_empty());
}

fn triangle_obj() -> &'static [u8] {
    b"# source-style fixture\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0.1 0.1\nvt 1 0\nvt 0 1\no 0_TEX_7\nf 1/1 2/2 3/3\n"
}

#[test]
fn source_obj_materials_and_fsom_guarded_edit_preserve_unedited_uv_bits() {
    use wonderland_asset_cooker::interchange::obj::{decode_mtl, ObjModel};
    use wonderland_creator::editors::meshes::MeshOverrideDocument;
    use wonderland_legacy_formats::reconstruction;
    let limits = default_limits();
    let obj = ObjModel::decode(triangle_obj(), &limits).unwrap();
    let materials = decode_mtl(&obj.source_materials(&limits).unwrap(), &limits).unwrap();
    assert_eq!(materials[0].name, "0_TEX_7");
    assert_eq!(materials[0].diffuse_map.as_deref(), Some("0_TEX_7.png"));
    let mesh = obj.to_fsom("fixture_1", &limits).unwrap();
    assert_eq!(mesh.groups[0][0].pixel_direction, u16::MAX);
    assert_eq!(mesh.groups[0][0].pixel_sprite, 7);
    assert_eq!(
        mesh.groups[0][0].vertices[0].normal.map(|v| v.get()),
        [0.0, 0.0, 1.0]
    );
    let bytes = reconstruction::encode_fsom(&mesh, &limits).unwrap();
    let mut doc = MeshOverrideDocument::import(&bytes, &limits).unwrap();
    let exported = doc.export_obj(&limits).unwrap();
    doc.import_obj(&sha256(&bytes), &exported, &limits).unwrap();
    assert_eq!(doc.export(), bytes);
    let mut edited = ObjModel::decode(&exported, &limits).unwrap();
    edited.set_position(0, [2.5, 0.0, 0.0]).unwrap();
    doc.import_obj(&sha256(&bytes), &edited.encode(&limits).unwrap(), &limits)
        .unwrap();
    assert_eq!(doc.mesh().groups[0][0].vertices[0].position[0].get(), 2.5);
    for (before, after) in mesh.groups[0][0]
        .vertices
        .iter()
        .zip(&doc.mesh().groups[0][0].vertices)
    {
        assert_eq!(before.texture_coordinate, after.texture_coordinate);
        assert_eq!(before.normal, after.normal);
    }
    assert_eq!(doc.mesh().bounds[1][0].get(), 2.5);
    let published = doc.export().to_vec();
    assert!(doc.import_obj(&"0".repeat(64), &exported, &limits).is_err());
    assert_eq!(doc.export(), published);
    assert!(ObjModel::decode(b"v 0 0 0\nvt 0 0\nf 1/1 1/1 1/1 1/1\n", &limits).is_err());
    assert!(decode_mtl(b"newmtl a\nmap_Kd ../secret.png", &limits).is_err());
}

#[test]
fn glb_and_gltf_actual_fsom_roundtrip_reject_changed_indices_truncation_and_nan() {
    use wonderland_asset_cooker::interchange::obj::ObjModel;
    use wonderland_creator::editors::gltf::GltfPackage;
    let limits = default_limits();
    let mesh = ObjModel::decode(triangle_obj(), &limits)
        .unwrap()
        .to_fsom("test", &limits)
        .unwrap();
    let hash = "5".repeat(64);
    let package = GltfPackage::from_fsom(&mesh, &hash, &limits).unwrap();
    let glb = package.to_glb(&limits).unwrap();
    let decoded = GltfPackage::from_glb(&glb, &limits).unwrap();
    assert_eq!(decoded.apply_fsom(&mesh, &hash, &limits).unwrap(), mesh);
    let text = package.to_gltf(&limits).unwrap();
    assert_eq!(
        GltfPackage::from_gltf(&text, &limits)
            .unwrap()
            .apply_fsom(&mesh, &hash, &limits)
            .unwrap(),
        mesh
    );
    for length in 0..glb.len() {
        assert!(GltfPackage::from_glb(&glb[..length], &limits).is_err());
    }
    let mut edited = decoded.clone();
    edited.binary[0..4].copy_from_slice(&3.0f32.to_le_bytes());
    let changed = edited.apply_fsom(&mesh, &hash, &limits).unwrap();
    assert_eq!(changed.groups[0][0].vertices[0].position[0].get(), 3.0);
    assert_eq!(
        changed.groups[0][0].vertices[0].texture_coordinate,
        mesh.groups[0][0].vertices[0].texture_coordinate
    );
    let last = edited.binary.len() - 4;
    edited.binary[last..].copy_from_slice(&0u32.to_le_bytes());
    assert!(edited.apply_fsom(&mesh, &hash, &limits).is_err());
    let mut nan = decoded;
    nan.binary[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(nan.apply_fsom(&mesh, &hash, &limits).is_err());
}

#[test]
fn gltf_animation_uses_source_axes_36hz_and_preserves_contact_events() {
    use wonderland_creator::editors::gltf::GltfPackage;
    use wonderland_legacy_formats::vitaboy::*;
    let limits = default_limits();
    let f = F32Bits::from_f32;
    let skeleton = Skeleton {
        version: 1,
        name: "rig".into(),
        bones: vec![Bone {
            unknown: 0,
            name: "ROOT".into(),
            parent_name: "NULL".into(),
            properties_flag: 0,
            properties: PropertyList { items: vec![] },
            translation: [f(0.0); 3],
            rotation: [f(0.0), f(0.0), f(0.0), f(1.0)],
            can_translate: 1,
            can_rotate: 1,
            can_blend: 1,
            wiggle_value: f(0.0),
            wiggle_power: f(0.0),
            index: 0,
            parent: None,
            children: vec![],
        }],
        root: 0,
        coordinate_policy: CoordinatePolicy::FreeSo,
    };
    let events = vec![TimePropertyList {
        items: vec![
            TimeProperty {
                id: 200,
                properties: PropertyList {
                    items: vec![PropertyItem {
                        pairs: vec![("xevt".into(), "9".into()), ("xevt".into(), "99".into())],
                    }],
                },
            },
            TimeProperty {
                id: 50,
                properties: PropertyList { items: vec![] },
            },
        ],
    }];
    let animation = Animation {
        version: 2,
        name: "contact".into(),
        duration_ms: f(1000.0),
        distance: f(0.0),
        is_moving: 0,
        translations: vec![[f(3.0), f(6.0), f(9.0)], [f(6.0), f(3.0), f(0.0)]],
        rotations: vec![[f(0.0), f(0.0), f(0.0), f(1.0)]; 2],
        motions: vec![Motion {
            unknown: 7,
            bone_name: "ROOT".into(),
            frame_count: 2,
            duration_ms: f(1000.0),
            translation_flag: 1,
            rotation_flag: 1,
            first_translation_index: 0,
            first_rotation_index: 0,
            properties_flag: 0,
            properties: vec![],
            time_properties_flag: 1,
            time_properties: events.clone(),
        }],
        num_frames: 2,
        coordinate_policy: CoordinatePolicy::FreeSo,
    };
    let hash = "a".repeat(64);
    let skelhash = "b".repeat(64);
    let mut package =
        GltfPackage::from_animation(&animation, &skeleton, &hash, &skelhash, &limits).unwrap();
    assert_eq!(
        f32::from_le_bytes(package.binary[4..8].try_into().unwrap()),
        1.0f32 / 36.0
    );
    assert_eq!(
        f32::from_le_bytes(package.binary[8..12].try_into().unwrap()),
        -1.0
    );
    assert_eq!(
        f32::from_le_bytes(package.binary[12..16].try_into().unwrap()),
        2.0
    );
    assert_eq!(
        f32::from_le_bytes(package.binary[16..20].try_into().unwrap()),
        -3.0
    );
    assert_eq!(
        package
            .apply_animation(&animation, &skeleton, &hash, &skelhash, &limits)
            .unwrap(),
        animation
    );
    package.binary[8..12].copy_from_slice(&(-4.0f32).to_le_bytes());
    let changed = package
        .apply_animation(&animation, &skeleton, &hash, &skelhash, &limits)
        .unwrap();
    assert_eq!(changed.translations[0][0].get(), 12.0);
    assert_eq!(changed.motions[0].time_properties, events);
    assert_eq!(changed.rotations, animation.rotations);
    package.binary[4..8].copy_from_slice(&0.0f32.to_le_bytes());
    assert!(package
        .apply_animation(&animation, &skeleton, &hash, &skelhash, &limits)
        .is_err());
    let mut scaled = animation.clone();
    scaled.translations[0][0] = f(5.0);
    let converted =
        GltfPackage::from_animation(&scaled, &skeleton, &hash, &skelhash, &limits).unwrap();
    // Original Vec3Convert multiplies by the binary32 constant (1f / 3f).
    assert_eq!(
        u32::from_le_bytes(converted.binary[8..12].try_into().unwrap()),
        0xbfd55556
    );
}

struct TempWork(std::path::PathBuf);
impl TempWork {
    fn new() -> Self {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "creator-extended-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(name), data).unwrap();
    }
}
impl Drop for TempWork {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn cook_spec(sources: serde_json::Value) -> wonderland_asset_cooker::registry::CookSpec {
    wonderland_asset_cooker::registry::CookSpec::from_json(&serde_json::to_vec(&json!({"schema_version":1,"fixture_only":true,"source_baseline":"4c6b3e8f5835b228723caea3c9f683c62f244f73","tuning_version":"aa96701f216ffebc6514d80cf8a491eaa3ff56926360f7eeced842fb5e594dfa","sources":sources,"overrides":[]})).unwrap(),&Default::default()).unwrap()
}
fn cook_source(id: &str, path: &str, format: &str) -> serde_json::Value {
    json!({"id":id,"path":path,"format":format,"pack_group":id,"provenance":{"origin":"authored","license":"CC0-1.0","redistribution":"allowed"}})
}
fn triangle_fsom() -> Vec<u8> {
    let mesh = wonderland_asset_cooker::interchange::obj::ObjModel::decode(
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\no 0_TEX_7\nf 1/1 2/2 3/3\n",
        &default_limits(),
    )
    .unwrap()
    .to_fsom("authored", &default_limits())
    .unwrap();
    wonderland_legacy_formats::reconstruction::encode_fsom(&mesh, &default_limits()).unwrap()
}

#[test]
fn new_source_asset_codecs_are_validated_visual_and_reject_invalid_payloads() {
    use wonderland_asset_cooker::{interchange::import_spec, manifest::CookLimits};
    use wonderland_content_ir::manifest::{ResourceCodec, ResourceKind};
    let work = TempWork::new();
    let limits = CookLimits::default();
    let nbhm = wonderland_legacy_formats::reconstruction::encode_nbhm(
        &wonderland_legacy_formats::reconstruction::Nbhm {
            version: 1,
            houses: vec![],
            has_model: 0,
        },
        &limits.legacy,
    )
    .unwrap();
    let png = CityImage::from_rgba(1, 1, vec![[11, 22, 33, 255]], &limits.legacy)
        .unwrap()
        .encode_png(&limits.legacy)
        .unwrap();
    let cases = [
        (
            "vitaboy_purchasable_outfit",
            ResourceCodec::VitaboyPurchasableOutfit,
            vec![0; 24],
        ),
        (
            "vitaboy_hand_group",
            ResourceCodec::VitaboyHandGroup,
            vec![0; 148],
        ),
        (
            "vitaboy_collection",
            ResourceCodec::VitaboyCollection,
            vec![0; 4],
        ),
        ("fsom", ResourceCodec::Fsom, triangle_fsom()),
        ("nbhm", ResourceCodec::Nbhm, nbhm),
        ("png_texture", ResourceCodec::PngTexture, png),
    ];
    for (format, codec, bytes) in cases {
        work.write("asset.bin", &bytes);
        let spec = cook_spec(json!([cook_source("asset", "asset.bin", format)]));
        let imported = import_spec(&spec, &work.0, &limits).unwrap();
        assert_eq!(imported.resources[0].kind, ResourceKind::Visual, "{format}");
        assert_eq!(imported.resources[0].codec, codec, "{format}");
        assert_eq!(imported.resources[0].payload, bytes);
        assert!(!imported.resources[0].simulation_critical);
        work.write("asset.bin", [0xff]);
        assert!(
            import_spec(&spec, &work.0, &limits).is_err(),
            "{format} silently became opaque"
        );
    }
}

fn iff_chunk(kind: [u8; 4], id: u16, data: Vec<u8>) -> wonderland_legacy_formats::iff::IffChunk {
    wonderland_legacy_formats::iff::IffChunk {
        key: wonderland_legacy_formats::iff::ChunkKey { kind, id },
        flags: 0,
        label: [0; 64],
        data,
    }
}
fn iff_file(
    chunks: Vec<wonderland_legacy_formats::iff::IffChunk>,
) -> wonderland_legacy_formats::iff::IffFile {
    let mut header = [0; 64];
    let magic = b"IFF FILE 2.5:TYPE FOLLOWED BY SIZE\0 JAMIE DOORNBOS & MAXIS 1";
    header[..magic.len()].copy_from_slice(magic);
    wonderland_legacy_formats::iff::IffFile { header, chunks }
}

#[test]
fn fsom_texture_closure_recooks_real_visuals_and_changes_only_dependent_control_group() {
    use wonderland_asset_cooker::{
        interchange::import_spec,
        manifest::{cook_resources, CookLimits},
    };
    use wonderland_content_ir::manifest::{LoadPhase, ResourceKind};
    use wonderland_legacy_formats::{iff, sprites};
    let work = TempWork::new();
    let limits = CookLimits::default();
    let png = CityImage::from_rgba(1, 1, vec![[11, 22, 33, 255]], &limits.legacy)
        .unwrap()
        .encode_png(&limits.legacy)
        .unwrap();
    let drawing = sprites::encode_dgrp(
        &sprites::DrawingGroup {
            version: 20004,
            images: vec![],
        },
        &limits.legacy,
    )
    .unwrap();
    let mut source = iff_file(vec![
        iff_chunk(*b"DGRP", 12, drawing),
        iff_chunk(*b"FSOM", 12, triangle_fsom()),
        iff_chunk(*b"MTEX", 7, png),
    ]);
    work.write("source.iff", iff::encode(&source, &limits.legacy).unwrap());
    work.write("control.bin", b"independent control");
    let spec = cook_spec(json!([
        cook_source("source", "source.iff", "iff"),
        cook_source("control", "control.bin", "opaque")
    ]));
    let cook = || {
        let imported = import_spec(&spec, &work.0, &limits).unwrap();
        cook_resources(
            &spec.source_baseline,
            spec.tuning_version.clone(),
            &imported.resources,
            &limits,
        )
        .unwrap()
    };
    let first = cook();
    let repeated = cook();
    assert_eq!(first.packs, repeated.packs);
    let mesh = "source/chunk-46534f4d-000c";
    let texture = "source/chunk-4d544558-0007";
    let drawing = "source/chunk-44475250-000c";
    assert_eq!(first.manifest.resources[mesh].kind, ResourceKind::Visual);
    assert_eq!(first.manifest.resources[texture].kind, ResourceKind::Visual);
    let closure = first
        .manifest
        .load_plan(
            &[mesh.into()],
            LoadPhase::All,
            None,
            &Default::default(),
            &limits.manifest,
        )
        .unwrap();
    assert_eq!(
        closure
            .resources
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        [mesh.to_string(), texture.to_string(), drawing.to_string()]
            .into_iter()
            .collect()
    );
    source.chunks[2].data = CityImage::from_rgba(1, 1, vec![[99, 88, 77, 255]], &limits.legacy)
        .unwrap()
        .encode_png(&limits.legacy)
        .unwrap();
    work.write("source.iff", iff::encode(&source, &limits.legacy).unwrap());
    let edited = cook();
    assert_eq!(
        first.manifest.resources[mesh].content_hash,
        edited.manifest.resources[mesh].content_hash
    );
    assert_ne!(
        first.manifest.resources[texture].content_hash,
        edited.manifest.resources[texture].content_hash
    );
    let key = |manifest: &wonderland_content_ir::manifest::AssetManifest, id| {
        manifest
            .derived_cache_key(id, "source-mesh-editor-v1", "mesh3d", &limits.manifest)
            .unwrap()
    };
    assert_ne!(key(&first.manifest, mesh), key(&edited.manifest, mesh));
    assert_eq!(
        key(&first.manifest, "control"),
        key(&edited.manifest, "control")
    );
    source.chunks.pop();
    work.write("source.iff", iff::encode(&source, &limits.legacy).unwrap());
    assert!(import_spec(&spec, &work.0, &limits).is_err());
}

#[test]
fn asset_json_admits_the_decompressed_model_before_owning_a_json_tree() {
    use wonderland_creator::editors::assets::{AssetDocument, AssetKind};
    // Literal gzip fixture generated from documented little-endian FSOm v3,
    // 128 repeated vertices, one triangle and no mask; not an encoder oracle.
    let bytes = include_bytes!("fixtures/creator-json-budget.fsom");
    let mut limits = default_limits();
    assert!(AssetDocument::import(AssetKind::Fsom, bytes, &limits).is_ok());
    limits.max_total_decoded_bytes = 400_000;
    assert!(AssetDocument::import(AssetKind::Fsom, bytes, &limits).is_err());
}

#[test]
fn retained_unknown_json_numbers_cannot_be_silently_rounded_by_an_edit() {
    use wonderland_creator::json_support;
    let limits = default_limits();
    // These tokens would round into a different number in an f64-backed Value.
    for number in ["18446744073709551617", "0.123456789012345678901", "1e-999"] {
        let bytes = format!("{{\"Version\":2,\"Files\":[],\"future\":{number}}}");
        assert!(
            UpgradeDocument::import(bytes.as_bytes(), &limits).is_err(),
            "{number}"
        );
    }
    let parsed = json_support::parse(
        br#"{"fraction":0.125,"exponent":1e3,"escaped":"\"-1e99"}"#,
        &limits,
    )
    .unwrap();
    assert_eq!(parsed["fraction"], 0.125);
    assert_eq!(parsed["exponent"], 1000.0);
}

#[test]
fn public_obj_models_validate_bounds_before_allocating_conversion_outputs() {
    use wonderland_asset_cooker::interchange::obj::ObjModel;
    let limits = default_limits();
    let mut model =
        ObjModel::decode(b"v 0 0 0\nvt 0 0\no 0_TEX_1\nf 1/1 1/1 1/1\n", &limits).unwrap();
    let mut small = limits;
    small.max_entries = 0;
    assert!(model.source_materials(&small).is_err());
    model.groups.get_mut("0_TEX_1").unwrap()[0][0].position = usize::MAX;
    assert!(model.encode(&limits).is_err());
    assert!(model.to_fsom("test", &limits).is_err());
}

#[test]
fn patch_view_uses_actual_user_patch_precedence_and_exact_source_name() {
    use wonderland_creator::patch_view::{PatchInput, PatchView};
    use wonderland_legacy_formats::{iff, semantic::*};
    let limits = default_limits();
    let source = iff_file(vec![
        iff_chunk(*b"ZZZZ", 1, b"abc".to_vec()),
        iff_chunk(*b"ZZZZ", 2, b"untouched".to_vec()),
    ]);
    let bytes = iff::encode(&source, &limits).unwrap();
    let text = |s: &str| LegacyString::from_text(s, TextEncoding::Utf8).unwrap();
    let patch = |value: &[u8]| {
        let descriptor = Piff {
            version: 2,
            source: text("Source.iff"),
            comment: text("authored"),
            trailing: vec![],
            entries: vec![PiffEntry {
                kind: *b"ZZZZ",
                id: 1,
                comment: text(""),
                operation: PiffOperation::Patch {
                    label: text("patched"),
                    flags: 17,
                    new_id: 1,
                    new_size: value.len() as u32,
                    patches: vec![
                        PiffPatch {
                            offset: 0,
                            size: 3,
                            mode: PiffPatchMode::Remove,
                            data: vec![],
                        },
                        PiffPatch {
                            offset: 0,
                            size: value.len() as u32,
                            mode: PiffPatchMode::Add,
                            data: value.to_vec(),
                        },
                    ],
                },
            }],
        };
        iff::encode(
            &iff_file(vec![iff_chunk(
                *b"PIFF",
                256,
                encode_piff(&descriptor, &limits).unwrap(),
            )]),
            &limits,
        )
        .unwrap()
    };
    let official = patch(b"official");
    let user = patch(b"user-authored");
    let inputs = [
        PatchInput {
            name: "official.piff",
            is_user: false,
            bytes: &official,
        },
        PatchInput {
            name: "user.piff",
            is_user: true,
            bytes: &user,
        },
    ];
    let view = PatchView::resolve("Source.iff", &bytes, &inputs, &limits).unwrap();
    assert_eq!(view.applied.len(), 1);
    assert_eq!(view.applied[0].name, "user.piff");
    assert_eq!(view.applied[0].order, 1);
    assert_eq!(view.suppressed.len(), 1);
    assert_eq!(view.suppressed[0].name, "official.piff");
    let effective = iff::decode(view.effective_bytes(), &limits).unwrap();
    assert_eq!(effective.chunks[0].data, b"user-authored");
    // Original IffFile.ApplyPatch reads descriptor flags but retains the
    // existing chunk flags; the effective view must expose the retained value.
    assert_eq!(effective.chunks[0].flags, 0);
    assert_eq!(effective.chunks[1], source.chunks[1]);
    assert!(view.resources[0].changed);
    assert!(!view.resources[1].changed);
    let metadata: serde_json::Value =
        serde_json::from_slice(&view.metadata_json(&limits).unwrap()).unwrap();
    assert_eq!(metadata["source_sha256"], sha256(&bytes));
    let skipped = PatchView::resolve("source.iff", &bytes, &inputs, &limits).unwrap();
    assert!(skipped.applied.is_empty());
    assert_eq!(skipped.effective_bytes(), bytes);
    assert_eq!(iff::encode(&source, &limits).unwrap(), bytes);
}

#[test]
fn gltf_rejects_nonunit_normals_without_normalizing_source_bits() {
    use wonderland_creator::editors::gltf::GltfPackage;
    use wonderland_legacy_formats::{reconstruction, vitaboy::F32Bits};
    let limits = default_limits();
    let bytes = triangle_fsom();
    let mut mesh = reconstruction::decode_fsom(&bytes, &limits).unwrap();
    let hash = sha256(&bytes);
    let mut package = GltfPackage::from_fsom(&mesh, &hash, &limits).unwrap();
    let normal_offset = package.json["bufferViews"][1]["byteOffset"]
        .as_u64()
        .unwrap() as usize;
    package.binary[normal_offset..normal_offset + 4].copy_from_slice(&2.0f32.to_le_bytes());
    assert!(package.apply_fsom(&mesh, &hash, &limits).is_err());
    mesh.groups[0][0].vertices[0].normal[0] = F32Bits::from_f32(2.0);
    assert!(GltfPackage::from_fsom(&mesh, &hash, &limits).is_err());
}

#[test]
fn gltf_admits_all_graphs_before_building_many_small_geometries() {
    use wonderland_creator::editors::gltf::GltfPackage;
    use wonderland_legacy_formats::{reconstruction::*, vitaboy::F32Bits};
    let mut limits = default_limits();
    limits.max_total_decoded_bytes = 8 * 1024 * 1024;
    let mut mesh = decode_fsom(&triangle_fsom(), &limits).unwrap();
    mesh.groups = vec![mesh.groups[0].clone(); 1_000];
    // Geometry and binary data fit. The many JSON nodes and simultaneous graph
    // copies do not; rejection must precede constructing those owned graphs.
    assert!(encode_fsom_payload(&mesh, &limits).is_ok());
    let error = GltfPackage::from_fsom(&mesh, &"a".repeat(64), &limits)
        .err()
        .expect("graph admission must fail");
    assert!(error.contains("graph admission"), "{error}");
    mesh.groups.truncate(1);
    mesh.groups[0][0].vertices[0].normal = [F32Bits(0), F32Bits(0), F32Bits::from_f32(1.0)];
    assert!(GltfPackage::from_fsom(&mesh, &"a".repeat(64), &limits).is_ok());
}

#[test]
fn gltf_shared_source_samples_reject_conflicting_and_implicit_channel_edits() {
    use wonderland_creator::editors::gltf::GltfPackage;
    use wonderland_legacy_formats::vitaboy::*;
    let limits = default_limits();
    let f = F32Bits::from_f32;
    let skeleton = Skeleton {
        version: 1,
        name: "rig".into(),
        root: 0,
        coordinate_policy: CoordinatePolicy::FreeSo,
        bones: (0..3)
            .map(|i| Bone {
                unknown: 0,
                name: ["ROOT", "BoneA", "BoneB"][i].into(),
                parent_name: if i == 0 { "NULL" } else { "ROOT" }.into(),
                properties_flag: 0,
                properties: PropertyList { items: vec![] },
                translation: [f(0.0); 3],
                rotation: [f(0.0), f(0.0), f(0.0), f(1.0)],
                can_translate: 1,
                can_rotate: 1,
                can_blend: 1,
                wiggle_value: f(0.0),
                wiggle_power: f(0.0),
                index: i,
                parent: if i == 0 { None } else { Some(0) },
                children: if i == 0 { vec![1, 2] } else { vec![] },
            })
            .collect(),
    };
    let source = Animation {
        version: 2,
        name: "shared".into(),
        duration_ms: f(1000.0),
        distance: f(0.0),
        is_moving: 0,
        translations: vec![[f(1.0), f(2.0), f(3.0)]],
        rotations: vec![[f(0.0), f(0.0), f(0.0), f(1.0)]],
        motions: ["BoneA", "BoneB"]
            .map(|bone| Motion {
                unknown: 0,
                bone_name: bone.into(),
                frame_count: 1,
                duration_ms: f(1000.0),
                translation_flag: 1,
                rotation_flag: 1,
                first_translation_index: 0,
                first_rotation_index: 0,
                properties_flag: 0,
                properties: vec![],
                time_properties_flag: 0,
                time_properties: vec![],
            })
            .to_vec(),
        num_frames: 1,
        coordinate_policy: CoordinatePolicy::FreeSo,
    };
    let baseline = GltfPackage::from_animation(&source, &skeleton, "a", "s", &limits).unwrap();
    let offset = |package: &GltfPackage, sampler: usize| {
        let accessor = package.json["animations"][0]["samplers"][sampler]["output"]
            .as_u64()
            .unwrap() as usize;
        let view = package.json["accessors"][accessor]["bufferView"]
            .as_u64()
            .unwrap() as usize;
        package.json["bufferViews"][view]["byteOffset"]
            .as_u64()
            .unwrap() as usize
    };
    assert_eq!(
        baseline
            .apply_animation(&source, &skeleton, "a", "s", &limits)
            .unwrap(),
        source
    );
    for second in [None, Some(5.0f32)] {
        let mut edited = baseline.clone();
        let first = offset(&edited, 0);
        edited.binary[first..first + 4].copy_from_slice(&4.0f32.to_le_bytes());
        if let Some(second) = second {
            let at = offset(&edited, 2);
            edited.binary[at..at + 4].copy_from_slice(&second.to_le_bytes());
        }
        assert!(
            edited
                .apply_animation(&source, &skeleton, "a", "s", &limits)
                .is_err(),
            "shared translations must agree"
        );
    }
    let mut consistent = baseline.clone();
    for sampler in [0, 2] {
        let at = offset(&consistent, sampler);
        consistent.binary[at..at + 4].copy_from_slice(&4.0f32.to_le_bytes());
    }
    let applied = consistent
        .apply_animation(&source, &skeleton, "a", "s", &limits)
        .unwrap();
    let exported = GltfPackage::from_animation(&applied, &skeleton, "a", "s", &limits).unwrap();
    assert_eq!(consistent.binary, exported.binary);
    let mut rotations = baseline.clone();
    let at = offset(&rotations, 1);
    for (i, value) in [1.0f32, 0.0, 0.0, 0.0].iter().enumerate() {
        rotations.binary[at + i * 4..at + (i + 1) * 4].copy_from_slice(&value.to_le_bytes());
    }
    assert!(
        rotations
            .apply_animation(&source, &skeleton, "a", "s", &limits)
            .is_err(),
        "shared rotations must agree"
    );
}

#[test]
fn city_bi_rgb32_uses_opaque_color_and_refuses_unrepresentable_alpha() {
    let limits = default_limits();
    // Independent BITMAPINFOHEADER vector: BI_RGB32 stores BGR plus an unused
    // high byte, per Microsoft's format specification. It has no alpha mask.
    let mut bytes = vec![0; 58];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&1i32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1i32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&32u16.to_le_bytes());
    bytes[34..38].copy_from_slice(&4u32.to_le_bytes());
    for unused in [0, 17, 255] {
        bytes[54..].copy_from_slice(&[30, 20, 10, unused]);
        let image = CityImage::decode(&bytes, &limits).unwrap();
        assert_eq!(image.pixel(0, 0).unwrap(), [10, 20, 30, 255]);
        let encoded = image.encode_bmp(&limits).unwrap();
        assert_eq!(&encoded[54..], &[30, 20, 10, 0]);
        assert_eq!(
            CityImage::decode(&encoded, &limits).unwrap().pixels(),
            image.pixels()
        );
    }
    let image = CityImage::from_rgba(1, 1, vec![[10, 20, 30, 17]], &limits).unwrap();
    assert!(image.encode_bmp(&limits).is_err());
    let png = image.encode_png(&limits).unwrap();
    assert_eq!(
        CityImage::decode(&png, &limits).unwrap().pixels(),
        image.pixels()
    );
}

#[test]
fn obj_sparse_dynamic_id_is_admitted_before_dense_group_allocation() {
    use wonderland_asset_cooker::interchange::obj::ObjModel;
    let mut limits = default_limits();
    limits.max_total_decoded_bytes = 512 * 1024;
    limits.max_entries = 100_000;
    let source = b"v 0 0 0\nvt 0 0\nvn 0 0 1\no 99999_TEX_1\nf 1/1/1 1/1/1 1/1/1\n";
    let model = ObjModel::decode(source, &limits).unwrap();
    assert!(model.encode(&limits).is_ok());
    let error = model.to_fsom("sparse", &limits).unwrap_err();
    assert!(error.contains("conversion admission"), "{error}");
    let dense = ObjModel::decode(
        b"v 0 0 0\nvt 0 0\nvn 0 0 1\no 0_TEX_1\nf 1/1/1 1/1/1 1/1/1\n",
        &limits,
    )
    .unwrap();
    assert!(dense.to_fsom("dense", &limits).is_ok());
}

#[test]
fn fsom_to_obj_admits_expanded_corner_arrays_before_export_allocation() {
    use wonderland_asset_cooker::interchange::obj::ObjModel;
    use wonderland_legacy_formats::reconstruction;
    let mut limits = default_limits();
    limits.max_total_decoded_bytes = 512 * 1024;
    limits.max_entries = 100_000;
    let mut mesh = reconstruction::decode_fsom(&triangle_fsom(), &default_limits()).unwrap();
    mesh.groups[0][0].indices = vec![0; 30_000];
    assert!(reconstruction::encode_fsom_payload(&mesh, &limits).is_ok());
    let error = ObjModel::from_fsom(&mesh, &limits).unwrap_err();
    assert!(error.contains("conversion admission"), "{error}");
}

#[test]
fn animation_gltf_refuses_duplicate_targets_and_nonunit_rest_rotations() {
    use wonderland_creator::editors::gltf::GltfPackage;
    use wonderland_legacy_formats::vitaboy::*;
    let limits = default_limits();
    let f = F32Bits::from_f32;
    let skeleton = Skeleton {
        version: 1,
        name: "rig".into(),
        root: 0,
        coordinate_policy: CoordinatePolicy::FreeSo,
        bones: vec![Bone {
            unknown: 0,
            name: "ROOT".into(),
            parent_name: "NULL".into(),
            properties_flag: 0,
            properties: PropertyList { items: vec![] },
            translation: [f(0.0); 3],
            rotation: [f(0.0), f(0.0), f(0.0), f(1.0)],
            can_translate: 1,
            can_rotate: 1,
            can_blend: 1,
            wiggle_value: f(0.0),
            wiggle_power: f(0.0),
            index: 0,
            parent: None,
            children: vec![],
        }],
    };
    let mut source = Animation {
        version: 2,
        name: "duplicate".into(),
        duration_ms: f(1000.0),
        distance: f(0.0),
        is_moving: 0,
        translations: vec![[f(1.0), f(2.0), f(3.0)]],
        rotations: vec![],
        motions: vec![Motion {
            unknown: 0,
            bone_name: "ROOT".into(),
            frame_count: 1,
            duration_ms: f(1000.0),
            translation_flag: 1,
            rotation_flag: 0,
            first_translation_index: 0,
            first_rotation_index: -1,
            properties_flag: 0,
            properties: vec![],
            time_properties_flag: 0,
            time_properties: vec![],
        }],
        num_frames: 1,
        coordinate_policy: CoordinatePolicy::FreeSo,
    };
    source.motions.push(source.motions[0].clone());
    assert!(GltfPackage::from_animation(&source, &skeleton, "a", "s", &limits).is_err());
    source.motions.pop();
    let mut invalid = skeleton.clone();
    invalid.bones[0].rotation[3] = f(2.0);
    assert!(GltfPackage::from_animation(&source, &invalid, "a", "s", &limits).is_err());
}
