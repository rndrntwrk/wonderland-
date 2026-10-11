use std::sync::Arc;
use wonderland_avatar_view::*;
use wonderland_render_core::{math::*, AssetKey, EntityRef};
fn bundle(rig: &Rig, mesh: Arc<PreparedMesh>) -> Arc<AppearanceBundle> {
    let file = FileKey {
        file_id: 1,
        type_id: 1,
    };
    let resource = ResourceKey {
        group_id: 1,
        file_id: 2,
        type_id: 3,
    };
    Arc::new(AppearanceBundle {
        rig_key: rig.key,
        skin: Skin::Light,
        parts: vec![AppearancePart {
            role: PartRole::Body,
            appearance: file,
            binding: file,
            mesh_resource: resource,
            texture_resource: resource,
            texture: AssetKey([1; 32]),
            mesh,
        }],
    })
}
#[test]
fn preview_renders_fitted_bounds_and_releases_targets_without_shared_mesh_loss() {
    let rig = fixtures::representative_rig();
    let mesh = Arc::new(fixtures::representative_mesh(&rig));
    let appearance = bundle(&rig, mesh.clone());
    let baseline = Arc::strong_count(&mesh);
    let mut previews = PreviewPool::new(2, 2 * 1024 * 1024);
    for cycle in 0..20 {
        let a = previews
            .open(appearance.clone(), &rig, &rig.bind_pose(), 96, 96)
            .unwrap();
        let b = previews
            .open(appearance.clone(), &rig, &rig.bind_pose(), 96, 96)
            .unwrap();
        let key = previews.key(a).unwrap();
        assert!(previews.image(a).unwrap().pixels.iter().any(|p| p[3] > 0));
        assert!(previews
            .open(appearance.clone(), &rig, &rig.bind_pose(), 96, 96)
            .is_err());
        previews.close(a).unwrap();
        assert!(previews.image(b).is_ok());
        previews.device_reset(cycle + 1).unwrap();
        assert!(previews.image(b).is_err());
        previews.render(b).unwrap();
        assert_ne!(key, previews.key(b).unwrap());
        previews.close(b).unwrap();
        assert_eq!(previews.stats().targets, 0);
        assert_eq!(previews.stats().cpu_bytes, 0);
    }
    assert_eq!(Arc::strong_count(&mesh), baseline);
    assert!(previews
        .open(appearance, &rig, &rig.bind_pose(), 8192, 8192)
        .is_err());
}
#[test]
fn bounded_32_and_64_actor_lod_keeps_visible_dependents_endpoints() {
    let rig = fixtures::representative_rig();
    let mesh = fixtures::representative_mesh(&rig);
    let base = mesh.bounds(&rig.bind_pose(), Mat4::IDENTITY).unwrap();
    for n in [32, 64] {
        let actors: Vec<AvatarCandidate> = (0..n)
            .map(|i| AvatarCandidate {
                reference: EntityRef {
                    object_id: i + 1,
                    generation: 1,
                },
                bounds: base
                    .transformed(Mat4::from_translation(Vec3::new(i as f32 * 4.0, 0.0, 0.0)))
                    .unwrap(),
                visible: true,
                level: 0,
                requires_endpoint: i % 8 == 0,
            })
            .collect();
        let view = Aabb::new(Vec3::new(-3.0, -3.0, -3.0), Vec3::new(40.0, 4.0, 3.0)).unwrap();
        let policy = LodPolicy {
            max_pose_updates: 16,
            ..LodPolicy::default()
        };
        let plan = plan_lod(3, &actors, Vec3::ZERO, view, 0, policy).unwrap();
        assert!(plan.iter().filter(|p| p.update_pose).count() <= 16);
        assert_eq!(plan.iter().filter(|p| p.draw).count(), 11);
        for p in &plan {
            if (p.reference.object_id - 1) % 8 == 0 {
                assert!(p.update_pose);
            }
        }
        let reversed: Vec<_> = actors.iter().copied().rev().collect();
        assert_eq!(
            plan,
            plan_lod(3, &reversed, Vec3::ZERO, view, 0, policy).unwrap()
        );
    }
}
#[test]
fn cook_roundtrip_preserves_bits_ids_dual_positions_and_ordered_properties() {
    let mut animation = fixtures::animation("clip", 0.0, 8.0);
    animation.translations[0][0] = F32Bits((-0.0f32).to_bits());
    animation.motions[0].time_properties = vec![TimePropertyList {
        items: vec![
            TimeProperty {
                id: 10,
                properties: PropertyList {
                    items: vec![PropertyItem {
                        pairs: vec![("xevt".into(), "2".into()), ("xevt".into(), "1".into())],
                    }],
                },
            },
            TimeProperty {
                id: -1,
                properties: PropertyList::default(),
            },
            TimeProperty {
                id: 10,
                properties: PropertyList::default(),
            },
        ],
    }];
    let source = CookedAvatar {
        version: 1,
        source_digest: AssetKey([90; 32]),
        rig_resource: ResourceKey {
            group_id: 9,
            file_id: 8,
            type_id: 7,
        },
        skeleton: fixtures::skeleton(),
        meshes: vec![(
            ResourceKey {
                group_id: 3,
                file_id: 4,
                type_id: 5,
            },
            fixtures::source_mesh(),
        )],
        animations: vec![(
            ResourceKey {
                group_id: 6,
                file_id: 7,
                type_id: 8,
            },
            animation,
        )],
    };
    let encoded = source.encode(AvatarLimits::default()).unwrap();
    let recovered = CookedAvatar::decode(&encoded, AvatarLimits::default()).unwrap();
    assert_eq!(source, recovered);
    assert_eq!(recovered.animations[0].1.translations[0][0].0, 0x80000000);
    assert_eq!(
        recovered.animations[0].1.motions[0].time_properties[0].items[0]
            .properties
            .first("xevt"),
        Some("2")
    );
    assert!(CookedAvatar::decode(&encoded[..encoded.len() - 1], AvatarLimits::default()).is_err());
    let mut corrupt = encoded.clone();
    corrupt[20] ^= 1;
    assert!(CookedAvatar::decode(&corrupt, AvatarLimits::default()).is_err());
    assert!(CookedAvatar::decode(
        &encoded,
        AvatarLimits {
            max_metadata_bytes: 32,
            ..AvatarLimits::default()
        }
    )
    .is_err());
}
