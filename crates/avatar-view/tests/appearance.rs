use std::sync::Arc;
use wonderland_avatar_view::*;
use wonderland_render_core::{AssetKey, EntityRef};
fn catalog() -> (Rig, AppearanceCatalog, FileKey) {
    let rig = fixtures::synthetic_rig();
    let mut c = AppearanceCatalog::default();
    let id = FileKey {
        file_id: 100,
        type_id: 16,
    };
    let ap = FileKey {
        file_id: 200,
        type_id: 17,
    };
    let binding = FileKey {
        file_id: 300,
        type_id: 15,
    };
    let mesh = ResourceKey {
        group_id: 42,
        file_id: 400,
        type_id: 3,
    };
    let texture = ResourceKey {
        group_id: 42,
        file_id: 500,
        type_id: 9,
    };
    c.outfits.insert(
        id,
        Outfit {
            version: 1,
            unknown: 0,
            light_appearance: ap,
            medium_appearance: FileKey { file_id: 201, ..ap },
            dark_appearance: FileKey { file_id: 202, ..ap },
            hand_group: 0,
            region: 0,
        },
    );
    c.appearances.insert(
        ap,
        Appearance {
            version: 1,
            thumbnail: ap,
            bindings: vec![binding],
        },
    );
    c.bindings.insert(
        binding,
        Binding {
            version: 1,
            bone: "HEAD".into(),
            mesh_selector: 8,
            mesh: Some(mesh),
            texture_selector: 8,
            texture: Some(texture),
        },
    );
    c.meshes.insert(
        mesh,
        (AssetKey([40; 32]), Arc::new(fixtures::source_mesh())),
    );
    c.textures.insert(texture, AssetKey([50; 32]));
    for file_id in [201, 202, 203, 204] {
        c.appearances.insert(
            FileKey { file_id, ..ap },
            Appearance {
                version: 1,
                thumbnail: ap,
                bindings: vec![binding],
            },
        );
    }
    let set = HandSet {
        idle: ap,
        fist: FileKey { file_id: 203, ..ap },
        pointing: FileKey { file_id: 204, ..ap },
    };
    let pair = HandPair {
        right: set.clone(),
        left: set,
    };
    c.hand_groups.insert(
        FileKey {
            file_id: 37,
            type_id: 18,
        },
        HandGroup {
            skins: [pair.clone(), pair.clone(), pair],
        },
    );
    (rig, c, id)
}
#[test]
fn three_skins_hands_and_deduplicated_accessories_keep_resource_ids() {
    let (rig, c, id) = catalog();
    for skin in [Skin::Light, Skin::Medium, Skin::Dark] {
        let s = AppearanceSelection {
            body: Some(id),
            head: Some(id),
            skin,
            accessories: vec![
                FileKey {
                    file_id: 200,
                    type_id: 17
                };
                2
            ],
            ..AppearanceSelection::default()
        };
        let bundle = c.compose(&rig, &s, AvatarLimits::default()).unwrap();
        assert_eq!(bundle.parts.len(), 5);
        assert_eq!(bundle.parts[0].mesh_resource.group_id, 42);
        assert_eq!(bundle.parts[0].mesh.rig_key, rig.key);
        assert_eq!(bundle.parts[0].mesh.vertices[0].primary_joint, 0);
        assert_eq!(bundle.skin, skin);
        assert_eq!(bundle.parts[0].appearance.file_id, 200 + skin as u32);
    }
    let s = AppearanceSelection {
        body: Some(id),
        left: Gesture::None,
        right: Gesture::None,
        ..AppearanceSelection::default()
    };
    assert_eq!(
        c.compose(&rig, &s, AvatarLimits::default())
            .unwrap()
            .parts
            .len(),
        1
    );
    let fists = AppearanceSelection {
        body: Some(id),
        left: Gesture::Pointing,
        right: Gesture::Fist,
        ..AppearanceSelection::default()
    };
    let hands = c.compose(&rig, &fists, AvatarLimits::default()).unwrap();
    assert_eq!(hands.parts[1].appearance.file_id, 203);
    assert_eq!(hands.parts[2].appearance.file_id, 204);
    assert_eq!(Gesture::Pointing as u8, 1);
    assert_eq!(Gesture::Fist as u8, 2);
}
#[test]
fn failed_and_late_outfits_do_not_partially_replace_live_resources() {
    let (rig, mut c, id) = catalog();
    let s = AppearanceSelection {
        body: Some(id),
        ..AppearanceSelection::default()
    };
    let initial = c.compose(&rig, &s, AvatarLimits::default()).unwrap();
    let shared = initial.parts[0].mesh.clone();
    let reference = EntityRef {
        object_id: 1,
        generation: 1,
    };
    let mut state = AppearanceState::new(reference);
    let first = state.request().unwrap();
    state.install(first, initial).unwrap();
    let before = Arc::strong_count(&shared);
    let late = state.request().unwrap();
    let current = state.request().unwrap();
    assert!(state
        .install(late, c.compose(&rig, &s, AvatarLimits::default()).unwrap())
        .is_err());
    assert_eq!(Arc::strong_count(&shared), before);
    c.textures.clear();
    assert!(c.compose(&rig, &s, AvatarLimits::default()).is_err());
    assert_eq!(state.current().unwrap().parts.len(), 3);
    state.clear();
    assert_eq!(Arc::strong_count(&shared), 1);
    state.reset(EntityRef {
        object_id: 1,
        generation: 2,
    });
    assert!(state
        .install(
            current,
            AppearanceBundle {
                rig_key: rig.key,
                skin: Skin::Light,
                parts: vec![]
            }
        )
        .is_err());
}
#[test]
fn literal_ts1_right_hand_keeps_idle_texture_and_no_hand_is_removed() {
    let set = LiteralHandSet {
        idle: LiteralHand {
            mesh: "idle".into(),
            texture: "idle_tex".into(),
        },
        fist: LiteralHand {
            mesh: "fist".into(),
            texture: "fist_tex".into(),
        },
        pointing: LiteralHand {
            mesh: "point".into(),
            texture: "point_tex".into(),
        },
    };
    let right = set.select(Gesture::Fist, true).unwrap();
    assert_eq!(right.mesh, "fist");
    assert_eq!(right.texture, "idle_tex");
    assert_eq!(
        set.select(Gesture::Pointing, false).unwrap().texture,
        "point_tex"
    );
    assert!(set.select(Gesture::None, true).is_none());
}
#[test]
fn normalized_hand_group_file_order_differs_from_gesture_enum() {
    let refs: Vec<_> = (0..18)
        .map(|i| FileKey {
            file_id: 100 + i,
            type_id: 17,
        })
        .collect();
    let group = HandGroup::from_source_refs(&refs).unwrap();
    assert_eq!(
        group.skins[2].right.select(Gesture::Fist).unwrap().file_id,
        113
    );
    assert_eq!(
        group.skins[2]
            .right
            .select(Gesture::Pointing)
            .unwrap()
            .file_id,
        114
    );
    assert_eq!(
        group.skins[2]
            .left
            .select(Gesture::Pointing)
            .unwrap()
            .file_id,
        117
    );
    assert!(HandGroup::from_source_refs(&refs[..17]).is_err());
}
