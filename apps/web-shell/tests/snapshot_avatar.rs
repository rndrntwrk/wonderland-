//! Synthetic binary-content fixtures exercise the source restore adapter.
//! Real original resources are verified only by the separately opted-in test.
use std::{collections::BTreeMap, sync::Arc};
use wonderland_avatar_content::{self as content, ImportedContent};
use wonderland_render_core::{AssetKey, RgbaImage, Vec3};
use wonderland_vm_protocol::snapshot::*;
use wonderland_web_shell::{
    snapshot_avatar::restore_snapshot_avatars, snapshot_world::snapshot_world,
};
use wonderland_world_view::*;
#[path = "support/snapshot.rs"]
mod support;

const HEAD: u64 = 0x002000010000000d;
const BODY: u64 = 0x002000020000000d;
fn outfit(id: u64) -> Outfit {
    Outfit { id, name: None }
}
fn avatar_snapshot() -> Snapshot {
    let mut source = support::fixture();
    let mut person_data = vec![0; 101];
    person_data[63] = 100;
    source.entities[0].guid = 0x7FD96B54;
    source.entities[0].object_data = vec![0; 80];
    source.entities[0].appearance = Appearance::Avatar(Box::new(Avatar {
        animations: vec![],
        carry_animation: None,
        message: String::new(),
        message_timeout: 0,
        motive_changes: vec![],
        decay_last_minute: 0,
        decay_fractions: [0; 7],
        person_data,
        motives: vec![0; 16],
        old_hand_object: 0,
        yaw: std::f32::consts::FRAC_PI_2,
        kill_timeout: 0,
        default_suits: [outfit(999), outfit(999), outfit(999)],
        dynamic_suits: [999; 4],
        decoration: [0; 4],
        bound_appearances: vec![],
        body: outfit(BODY),
        head: outfit(HEAD),
        skin_tone: 0,
    }));
    source
}
fn avatar(source: &mut Snapshot) -> &mut Avatar {
    match &mut source.entities[0].appearance {
        Appearance::Avatar(avatar) => avatar,
        _ => panic!("test avatar"),
    }
}
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
fn bank() -> ImportedContent {
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
        0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
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
fn pixels(content: &ImportedContent) -> BTreeMap<AssetKey, RgbaImage> {
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
fn restored(source: &Snapshot, bank: &ImportedContent) -> WorldDocument {
    let mut world = snapshot_world(source, 9, 123, 27).unwrap();
    restore_snapshot_avatars(source, &mut world, bank, &pixels(bank)).unwrap();
    world
}

#[test]
fn exact_source_outfits_ground_contact_scale_and_identity_survive_restore() {
    let content = bank();
    let mut source = avatar_snapshot();
    avatar(&mut source).person_data[63] = 50;
    let world = restored(&source, &content);
    let object = &world.objects[0];
    assert_eq!(object.position_tiles, Vec3::new(1.5, 1.5, 3.70));
    assert_eq!(object.entity, None);
    assert_eq!(object.snapshot.unwrap().persistent_id, 4_000_000_001);
    assert_eq!(object.snapshot.unwrap().object_id, 7);
    let model = &world.models[object
        .model
        .expect("complete exact source outfit must render")];
    assert_eq!(model.context, ModelContext::Vitaboy);
    assert_eq!(model.bounds.max.y, 2.);
    assert!(
        world
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "snapshot_avatar_restore_pose")
    );
    assert!(
        !world
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_avatar_visual")
    );
    assert_eq!(avatar(&mut source).head.id, HEAD);
    avatar(&mut source).head.id += 1;
    let missing = restored(&source, &content);
    assert!(
        missing.objects[0].model.is_none(),
        "nearby 64-bit source ID must never resolve by numeric rounding or selected appearance"
    );
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_avatar_resource")
    );
}

#[test]
fn hidden_off_world_contained_and_unknown_rig_avatars_never_invent_positions() {
    let content = bank();
    for kind in 0..4 {
        let mut source = avatar_snapshot();
        match kind {
            0 => source.entities[0].object_data[34] = -1,
            1 => {
                source.entities[0].position.x = i16::MIN;
                source.entities[0].position.y = i16::MIN;
            }
            2 => source.entities[0].container = 8,
            _ => source.entities[0].guid = 0x12345678,
        }
        let world = restored(&source, &content);
        assert!(world.objects[0].model.is_none());
        if kind < 2 {
            assert!(!world.objects[0].visible);
        } else {
            assert!(world.diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.code.as_str(),
                "unresolved_avatar_container" | "unresolved_avatar_rig"
            )));
        }
    }
}

#[test]
fn missing_source_hidden_field_blocks_only_that_avatar_visual() {
    let content = bank();
    let mut source = avatar_snapshot();
    let mut other = source.entities[0].clone();
    other.object_id = 8;
    other.persist_id += 1;
    source.entities.push(other);
    source.entities[0].object_data.clear();
    let world = restored(&source, &content);
    assert!(world.objects[0].model.is_none());
    assert!(world.objects[1].model.is_some());
    assert!(
        world
            .diagnostics
            .iter()
            .any(|issue| issue.code == "unsupported_avatar_state"
                && issue.message.contains("visibility"))
    );
}

#[test]
fn missing_animation_or_decoded_texture_is_explicit_and_never_a_bind_pose_fallback() {
    let content = bank();
    let mut source = avatar_snapshot();
    avatar(&mut source).animations.push(Animation {
        name: "unloaded".into(),
        frame: 12.5,
        event_queue: vec![3],
        events_run: 1,
        end_reached: false,
        backwards: false,
        speed: 1.,
        weight: 1.,
        looped: true,
    });
    let world = restored(&source, &content);
    assert!(world.objects[0].model.is_none());
    assert!(
        world
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_avatar_animation"
                && diagnostic.message.contains("unloaded.anim"))
    );
    let source = avatar_snapshot();
    let mut world = snapshot_world(&source, 9, 123, 27).unwrap();
    restore_snapshot_avatars(&source, &mut world, &content, &BTreeMap::new()).unwrap();
    assert!(world.objects[0].model.is_none());
    assert!(
        world
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_avatar_texture")
    );
}

#[test]
fn restored_avatar_geometry_has_real_depth_picks_and_refresh_expires_them() {
    let content = bank();
    let source = avatar_snapshot();
    let mut world = restored(&source, &content);
    let controls = ViewportControls {
        visible_level: 2,
        ..Default::default()
    };
    let mut renderer = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    renderer.render(controls, 240, 240).unwrap();
    let pick = (0..240)
        .flat_map(|y| (0..240).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            renderer.pick(x, y).filter(|pick| {
                matches!(
                    pick.target,
                    WorldPickTarget::Object {
                        source_record: Some(0),
                        entity: None,
                        ..
                    }
                )
            })
        })
        .expect("avatar pixels are selectable");
    assert_eq!(pick.revision, world.revision);
    world.revision.architecture_revision += 1;
    world.objects[0]
        .snapshot
        .as_mut()
        .unwrap()
        .presentation_generation += 1;
    renderer.replace_document(Arc::new(world)).unwrap();
    assert!(renderer.resolve_pick(&pick).is_none());
}

fn animation(name: &str, frame: f32, weight: f32) -> Animation {
    Animation {
        name: name.into(),
        frame,
        event_queue: vec![9],
        events_run: 1,
        end_reached: false,
        backwards: false,
        speed: 1.,
        weight,
        looped: true,
    }
}

#[test]
fn stored_layers_and_integer_carry_sample_once_without_events_or_visual_progression() {
    let content = bank();
    let mut source = avatar_snapshot();
    avatar(&mut source).animations = vec![animation("base", 0.5, 1.), animation("blend", 0., 3.)];
    let world = restored(&source, &content);
    assert_eq!(world.models[0].bounds.max.x, -6.);
    // Ended layers still contribute prefix weight, exactly like FractionalAnim.
    avatar(&mut source).animations[0].end_reached = true;
    assert_eq!(restored(&source, &content).models[0].bounds.max.x, -5.5);
    let mut carry = animation("carry", 1.9, -99.);
    carry.end_reached = true;
    carry.backwards = true;
    carry.speed = 400.;
    avatar(&mut source).carry_animation = Some(carry);
    let carried = restored(&source, &content);
    assert_eq!(carried.models[0].bounds.max.x, -5.5);
    assert_eq!(restored(&source, &content), carried);
    assert_eq!(avatar(&mut source).animations[0].frame, 0.5);
    assert_eq!(avatar(&mut source).animations[0].event_queue, vec![9]);
    assert_eq!(
        avatar(&mut source).carry_animation.as_ref().unwrap().frame,
        1.9
    );
}

#[test]
fn nonfinite_zero_prefix_negative_scale_and_stale_preparation_are_explicit() {
    let content = bank();
    for variant in 0..3 {
        let mut source = avatar_snapshot();
        match variant {
            0 => avatar(&mut source).animations = vec![animation("base", 0., 0.)],
            1 => avatar(&mut source).animations = vec![animation("base", f32::NAN, 1.)],
            _ => avatar(&mut source).person_data[63] = -100,
        }
        let world = restored(&source, &content);
        assert!(world.objects[0].model.is_none());
        assert!(world.diagnostics.iter().any(|issue| matches!(
            issue.code.as_str(),
            "unsupported_avatar_animation" | "unsupported_avatar_scale"
        )));
    }
    let source = avatar_snapshot();
    let mut world = snapshot_world(&source, 9, 123, 27).unwrap();
    let prepared = wonderland_web_shell::snapshot_avatar::SnapshotAvatarProjection::prepare(
        &source, &world, &content,
    )
    .unwrap();
    world.revision.architecture_revision += 1;
    assert!(
        prepared
            .apply(&mut world, &content, &pixels(&content))
            .is_err()
    );
    assert!(world.models.is_empty());
}

#[test]
fn named_outfits_bound_appearances_decorations_and_source_ghost_state_are_preserved() {
    let mut content = bank();
    let mut source = avatar_snapshot();
    let named = avatar(&mut source);
    named.head = Outfit {
        id: u64::from(u32::MAX),
        name: Some("HEAD.oft".into()),
    };
    named.bound_appearances = vec!["PART.apr".into()];
    named.skin_tone = 2;
    named.person_data[74] = 1;
    let decoration_key = content::FileKey {
        file_id: 0x200003,
        type_id: 13,
    };
    let mut decoration = content.catalog.outfits[&content::FileKey {
        file_id: (HEAD >> 32) as u32,
        type_id: HEAD as u32,
    }]
        .clone();
    // Only Light exists. The original restore attaches decorations before it
    // applies saved skin; choosing the saved Dark here would fail composition.
    decoration.dark_appearance = content::FileKey {
        file_id: 999,
        type_id: 12,
    };
    content.catalog.outfits.insert(decoration_key, decoration);
    named.decoration[2] = decoration_key.packed();
    let world = restored(&source, &content);
    // Source Avatar.AddAccessory deduplicates the shoes/bound-appearance APR,
    // without removing the same appearance from independent head/body/hands.
    assert_eq!(world.models[0].groups[0].len(), 5);
    assert!(world.models[0].groups[0].iter().all(|part| {
        part.mesh
            .vertices
            .iter()
            .all(|v| v.color == [21. / 255., 168. / 255., 63. / 255., 168. / 255.])
    }));
    assert!(
        build_scene(
            &world,
            ViewportControls {
                visible_level: 2,
                ..Default::default()
            }
        )
        .unwrap()
        .diagnostics
        .iter()
        .any(|issue| issue.code == "software_alpha_approximation")
    );
}

#[test]
fn source_accessory_insertion_order_is_bound_then_back_head_tail_shoes() {
    use wonderland_avatar_view::F32Bits;
    let mut content = bank();
    let mut source = avatar_snapshot();
    let original_outfit = content.catalog.outfits[&content::FileKey {
        file_id: (HEAD >> 32) as u32,
        type_id: HEAD as u32,
    }]
        .clone();
    let original_appearance = content.catalog.appearances[&content::FileKey {
        file_id: 500,
        type_id: 12,
    }]
        .clone();
    let original_binding = content.catalog.bindings[&content::FileKey {
        file_id: 501,
        type_id: 11,
    }]
        .clone();
    let original_mesh = content.catalog.meshes[&content::ResourceKey {
        group_id: 0,
        file_id: 600,
        type_id: 9,
    }]
        .1
        .as_ref()
        .clone();
    for index in 0..4 {
        let key = |type_id| content::FileKey {
            file_id: 900 + index,
            type_id,
        };
        let mesh_key = content::ResourceKey {
            group_id: 0,
            file_id: 900 + index,
            type_id: 9,
        };
        let mut mesh = original_mesh.clone();
        for vertex in &mut mesh.vertices {
            vertex.position[0] = F32Bits::from_f32(10. + index as f32);
        }
        content
            .catalog
            .meshes
            .insert(mesh_key, (AssetKey([index as u8; 32]), Arc::new(mesh)));
        let mut binding = original_binding.clone();
        binding.mesh = Some(mesh_key);
        content.catalog.bindings.insert(key(11), binding);
        let mut appearance = original_appearance.clone();
        appearance.bindings = vec![key(11)];
        content.catalog.appearances.insert(key(12), appearance);
        let mut outfit = original_outfit.clone();
        outfit.light_appearance = key(12);
        content.catalog.outfits.insert(key(13), outfit);
        avatar(&mut source).decoration[index as usize] = key(13).packed();
    }
    avatar(&mut source).bound_appearances = vec!["part.apr".into()];
    let world = restored(&source, &content);
    let x = world.models[0].groups[0][4..]
        .iter()
        .map(|part| part.mesh.vertices[0].position.x)
        .collect::<Vec<_>>();
    assert_eq!(x, vec![0.5, 11., 10., 13., 12.]);
}

#[test]
#[ignore = "requires an explicitly supplied original resource pack, decoded pixels, and synthetic FSOv fixture"]
fn supplied_original_pack_renders_original_avatar_through_the_snapshot_world() {
    use std::{fs, path::PathBuf};
    let pack =
        PathBuf::from(std::env::var_os("WONDERLAND_AVATAR_PACK").expect("original pack path"));
    let decoded_path =
        PathBuf::from(std::env::var_os("WONDERLAND_AVATAR_RGBA").expect("original RGBA path"));
    let source_path = std::env::var_os("WONDERLAND_AVATAR_SNAPSHOT").expect("test-only FSOv path");
    let mut files = fs::read_dir(&pack)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let content = content::import(
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
    .unwrap();
    let source = wonderland_vm_protocol::decode_snapshot(
        &fs::read(source_path).unwrap(),
        &Default::default(),
    )
    .unwrap();
    let mut world = snapshot_world(&source, 9, 123, 27).unwrap();
    let projection = wonderland_web_shell::snapshot_avatar::SnapshotAvatarProjection::prepare(
        &source, &world, &content,
    )
    .unwrap();
    assert!(projection.texture_keys().len() >= 3);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(decoded_path.join("manifest.json")).unwrap()).unwrap();
    let mut decoded = BTreeMap::new();
    for &key in projection.texture_keys() {
        let key_text = key
            .0
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let metadata = &manifest[&key_text];
        let bytes = fs::read(decoded_path.join(metadata["file"].as_str().unwrap())).unwrap();
        let image = RgbaImage {
            width: metadata["width"].as_u64().unwrap() as u32,
            height: metadata["height"].as_u64().unwrap() as u32,
            pixels: bytes.as_chunks::<4>().0.to_vec(),
        };
        content.validate_decoded_texture(key, &image).unwrap();
        decoded.insert(key, image);
    }
    projection.apply(&mut world, &content, &decoded).unwrap();
    assert_eq!(world.models.len(), 1, "{:?}", world.diagnostics);
    let model = &world.models[0];
    assert_eq!(model.groups[0].len(), 4);
    assert!(
        model.groups[0]
            .iter()
            .map(|part| part.mesh.indices.len() / 3)
            .sum::<usize>()
            > 500
    );
    assert!(model.bounds.max.y > 4. && model.bounds.max.y < 7.);
    let mut renderer = WorldRenderer::new(Arc::new(world.clone())).unwrap();
    renderer
        .render(ViewportControls::default(), 400, 320)
        .unwrap();
    let pick = (0..320)
        .flat_map(|y| (0..400).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            renderer.pick(x, y).filter(|pick| {
                matches!(
                    pick.target,
                    WorldPickTarget::Object {
                        source_record: Some(0),
                        entity: None,
                        ..
                    }
                )
            })
        })
        .expect("original avatar has depth-tested source picks");
    assert_eq!(world.objects[0].snapshot.unwrap().persistent_id, 42);
    eprintln!(
        "Original source snapshot avatar: {} parts, {} triangles, {} textures, bounds {:?}, picked {:?}",
        model.groups[0].len(),
        model.groups[0]
            .iter()
            .map(|part| part.mesh.indices.len() / 3)
            .sum::<usize>(),
        model.textures.len(),
        model.bounds,
        pick.target
    );
}
