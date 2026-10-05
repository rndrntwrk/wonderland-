use std::sync::Arc;
use wonderland_render_core::*;
use wonderland_render_iso::*;
fn key(q: u8) -> AssetKey {
    AssetKey([q; 32])
}
fn p() -> Projection {
    Projection::new(
        Zoom::Near,
        Rotation::TopLeft,
        1.,
        Vec3::ZERO,
        Vec2::new(800., 600.),
    )
    .unwrap()
}
fn instance() -> DgrpInstance {
    DgrpInstance {
        reference: EntityRef {
            object_id: 1,
            generation: 2,
        },
        visual_revision: 3,
        direction: 1,
        tile_position: Vec3::ZERO,
        room: 1,
        base_room: None,
        level: 1,
        visible: true,
        selectable: true,
        cutaway_hidden: false,
        dynamic_base: 100,
        dynamic_count: 128,
        dynamic_masks: [u64::MAX; 2],
    }
}
fn layer(id: u32, width: u32, height: u32, depth: DepthInput) -> DgrpLayer {
    DgrpLayer {
        sprite_id: id,
        frame_index: 0,
        sprite_offset: Vec2::new(-4., 3.),
        object_offset: Vec3::ZERO,
        flags: 0,
        asset: Some(Arc::new(SpriteAsset {
            key: key(1),
            rgba: RgbaImage {
                width,
                height,
                pixels: vec![[255; 4]; (width * height) as usize],
            },
            physical_size: [(width + 3) / 4 * 4, (height + 3) / 4 * 4],
            depth,
            mask: None,
        })),
    }
}
fn image(layers: Vec<Option<DgrpLayer>>) -> DgrpImage {
    DgrpImage {
        direction: 1,
        zoom: Zoom::Near,
        layers,
    }
}
fn prepare(layers: Vec<Option<DgrpLayer>>) -> PreparedObject {
    prepare_sprites(
        &p(),
        &instance(),
        &[image(layers)],
        &PreparePolicy::default(),
    )
    .unwrap()
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}

#[test]
fn dgrp_selects_first_exact_rotated_direction_and_handles_missing() {
    let mut a = image(vec![]);
    a.direction = 4;
    let mut b = a.clone();
    b.layers = vec![None];
    let images = [a, b, image(vec![])];
    assert_eq!(
        select_image(&images, 1, Zoom::Near, Rotation::TopRight)
            .unwrap()
            .layers
            .len(),
        0
    );
    assert!(select_image(&images, 1, Zoom::Far, Rotation::TopRight).is_none());
    assert!(select_image(&images, 1, Zoom::Near, Rotation::BottomRight).is_none());
}
#[test]
fn local_cadge_destination_and_resolved_position_are_separate() {
    let obj = prepare(vec![Some(layer(1, 8, 8, DepthInput::Constant(128)))]);
    let s = &obj.sprites[0];
    assert_eq!(
        s.local_rect,
        Rect {
            x: 64.,
            y: 343.,
            width: 8.,
            height: 8.
        }
    );
    assert_eq!(
        s.rect,
        Rect {
            x: 396.,
            y: 327.,
            width: 8.,
            height: 8.
        }
    );
    let mut i = instance();
    i.tile_position = Vec3::new(1., 0., 0.);
    let moved = prepare_sprites(
        &p(),
        &i,
        &[image(vec![Some(layer(1, 8, 8, DepthInput::Constant(128)))])],
        &PreparePolicy::default(),
    )
    .unwrap();
    near(moved.sprites[0].rect.x, 460.);
    near(moved.sprites[0].rect.y, 359.);
    assert_eq!(moved.sprites[0].world_anchor, Vec3::new(3., 0., 0.));
}
#[test]
fn padded_uvs_do_not_enlarge_logical_destination_and_flip_depth_together() {
    let mut l = layer(
        1,
        5,
        7,
        DepthInput::Bytes {
            key: key(2),
            width: 5,
            height: 7,
            values: vec![153; 35],
        },
    );
    l.flags = 1;
    let obj = prepare(vec![Some(l)]);
    let s = &obj.sprites[0];
    assert_eq!(s.rect.width, 5.);
    assert_eq!(s.rect.height, 7.);
    assert_eq!(s.mesh.vertices[0].uv, Vec2::new(0.625, 0.));
    assert_eq!(s.mesh.vertices[2].uv, Vec2::new(0., 0.875));
    near(
        s.fragment(0, 0, [1.; 3], AlphaPass::ColorDepth)
            .unwrap()
            .depth
            .unwrap(),
        s.anchors.unwrap().front.depth,
    );
}
#[test]
fn cardinal_offsets_use_sixteenths_and_fifths_without_frame_offset() {
    let mut l = layer(1, 8, 8, DepthInput::Constant(128));
    l.object_offset = Vec3::new(16., 0., 5.);
    let mut i = instance();
    i.direction = 4;
    let mut im = image(vec![Some(l)]);
    im.direction = 4;
    let obj = prepare_sprites(&p(), &i, &[im], &PreparePolicy::default()).unwrap();
    assert_eq!(obj.sprites[0].world_anchor, Vec3::new(0., 3., 3.));
    assert_eq!(obj.sprites[0].local_rect.x, 0.);
    near(obj.sprites[0].local_rect.y, 296.);
}
#[test]
fn dynamic_and_null_layers_produce_empty_bounds_without_stale_depth() {
    let mut i = instance();
    i.dynamic_masks = [0; 2];
    let im = image(vec![
        None,
        Some(layer(100, 8, 8, DepthInput::Constant(128))),
        Some(DgrpLayer {
            asset: None,
            ..layer(1, 8, 8, DepthInput::None)
        }),
    ]);
    let obj = prepare_sprites(&p(), &i, &[im], &PreparePolicy::default()).unwrap();
    assert!(obj.sprites.is_empty());
    assert_eq!(obj.bounds, None);
    let obj = prepare(vec![
        Some(layer(1, 8, 8, DepthInput::Constant(128))),
        Some(layer(2, 8, 8, DepthInput::None)),
    ]);
    assert_eq!(obj.sprites[1].material.depth, DepthKey::None);
    assert_eq!(obj.sprites[1].anchors, None);
    assert_eq!(
        obj.sprites[1]
            .fragment(0, 0, [1.; 3], AlphaPass::Basic)
            .unwrap()
            .depth,
        None
    );
}
#[test]
fn reserved_luminous_rooms_and_hidden_floor_policy_are_explicit() {
    let mut l = layer(1, 8, 8, DepthInput::None);
    l.flags = 4;
    for (room, want) in [(5, 65535), (65534, 65534), (65533, 65533)] {
        let mut i = instance();
        i.room = room;
        let o = prepare_sprites(
            &p(),
            &i,
            &[image(vec![Some(l.clone())])],
            &PreparePolicy::default(),
        )
        .unwrap();
        assert_eq!(o.sprites[0].room, want);
    }
    for alter in [0, 1, 2, 3] {
        let mut i = instance();
        match alter {
            0 => i.visible = false,
            1 => i.cutaway_hidden = true,
            2 => i.level = 7,
            _ => i.room = 0,
        }
        assert!(prepare_sprites(
            &p(),
            &i,
            &[image(vec![Some(l.clone())])],
            &PreparePolicy::default()
        )
        .unwrap()
        .sprites
        .is_empty());
    }
}
#[test]
fn malformed_images_and_depth_dimensions_fail_before_preparation() {
    let mut l = layer(1, 8, 8, DepthInput::None);
    Arc::make_mut(l.asset.as_mut().unwrap()).physical_size = [4, 4];
    assert!(prepare_sprites(
        &p(),
        &instance(),
        &[image(vec![Some(l)])],
        &PreparePolicy::default()
    )
    .is_err());
    let l = layer(
        1,
        8,
        8,
        DepthInput::Bytes {
            key: key(2),
            width: 8,
            height: 8,
            values: vec![0; 63],
        },
    );
    assert!(prepare_sprites(
        &p(),
        &instance(),
        &[image(vec![Some(l)])],
        &PreparePolicy::default()
    )
    .is_err());
    let mut l = layer(1, 8, 8, DepthInput::None);
    l.object_offset.x = f32::NAN;
    assert!(prepare_sprites(
        &p(),
        &instance(),
        &[image(vec![Some(l)])],
        &PreparePolicy::default()
    )
    .is_err());
    let mut policy = PreparePolicy::default();
    policy.max_sprites = 0;
    assert!(prepare_sprites(&p(), &instance(), &[image(vec![None])], &policy).is_err());
}
#[test]
fn batching_includes_depth_preserves_equal_order_and_splits_portable_indices() {
    let o = prepare(vec![
        Some(layer(1, 8, 8, DepthInput::Constant(128))),
        Some(layer(2, 8, 8, DepthInput::Constant(128))),
        Some(layer(3, 8, 8, DepthInput::Constant(153))),
    ]);
    let b = make_batches(&o.sprites, 2).unwrap();
    assert_eq!(b.len(), 2);
    assert_eq!(b[0].sprite_indices, [0, 1]);
    assert_eq!(b[1].sprite_indices, [2]);
    assert_eq!(b[0].mesh.indices, [0, 1, 3, 1, 2, 3, 4, 5, 7, 5, 6, 7]);
    assert_eq!(make_batches(&o.sprites, 1).unwrap().len(), 3);
    assert!(make_batches(&o.sprites, 0).is_err());
    assert!(make_batches(&o.sprites, 16384).is_err());
}
#[test]
fn material_switching_never_reorders_transparent_equal_depth_sprites() {
    let mut o = prepare(vec![
        Some(layer(1, 8, 8, DepthInput::Constant(128))),
        Some(layer(2, 8, 8, DepthInput::Constant(153))),
        Some(layer(3, 8, 8, DepthInput::Constant(128))),
    ]);
    o.sprites[0].draw_order = 1.;
    o.sprites[1].draw_order = 1.;
    o.sprites[2].draw_order = 1.;
    let b = make_batches(&o.sprites, 16383).unwrap();
    assert_eq!(b.len(), 3);
    assert_eq!(
        b.iter()
            .flat_map(|b| b.sprite_indices.clone())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}
#[test]
fn presentation_dynamic_residency_expires_without_advancing_simulation() {
    let r = DynamicResidency {
        reference: instance().reference,
        changed_at: 10.,
        force_dynamic: false,
    };
    assert_eq!(r.layer(12.49).unwrap(), SpriteLayer::Dynamic);
    assert_eq!(r.layer(12.5).unwrap(), SpriteLayer::Static);
    assert_eq!(
        DynamicResidency {
            force_dynamic: true,
            ..r
        }
        .layer(100.)
        .unwrap(),
        SpriteLayer::Dynamic
    );
    assert!(r.layer(9.).is_err());
    assert!(r.layer(f64::NAN).is_err());
}

#[test]
fn different_lighting_resources_cannot_alias_one_material_batch() {
    let a = prepare(vec![Some(layer(1, 8, 8, DepthInput::Constant(128)))]);
    let mut policy = PreparePolicy::default();
    policy.lighting.advanced = Some(key(9));
    policy.lighting.revision = 7;
    let b = prepare_sprites(
        &p(),
        &instance(),
        &[image(vec![Some(layer(2, 8, 8, DepthInput::Constant(128)))])],
        &policy,
    )
    .unwrap();
    assert_eq!(
        make_batches(&[a.sprites[0].clone(), b.sprites[0].clone()], 10)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn shader_vertex_abi_preserves_room_floor_and_pick_code() {
    let object = prepare(vec![Some(layer(1, 8, 8, DepthInput::Constant(128)))]);
    let s = &object.sprites[0];
    let v = s.shader_vertices(257).unwrap();
    assert_eq!(v[0].position, [396., 327., 0.]);
    assert_eq!(v[0].world_anchor, [0.; 3]);
    near(v[0].object_id_and_floor[0], 257. / 65535.);
    assert_eq!(v[0].object_id_and_floor[1], 0.);
    assert_eq!(v[0].room, [1. / 256., 0.]);
    assert_eq!(v[0].floats()[0], 396.);
    assert_eq!(v[0].floats().len(), 12);
    assert!(s.shader_vertices(65535).is_err());
}

#[test]
fn software_passes_split_overlapping_rectangles_without_reordering() {
    let o = prepare(vec![
        Some(layer(1, 8, 8, DepthInput::Constant(128))),
        Some(layer(2, 8, 8, DepthInput::Constant(128))),
        Some(layer(3, 8, 8, DepthInput::Constant(128))),
    ]);
    let batches = make_software_batches(&o.sprites, 16383).unwrap();
    assert_eq!(batches.len(), 3);
    assert_eq!(
        batches
            .iter()
            .flat_map(|b| b.sprite_indices.clone())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn aggregate_batch_limits_are_checked_before_geometry_copy() {
    let o = prepare(vec![
        Some(layer(1, 8, 8, DepthInput::Constant(128))),
        Some(layer(2, 8, 8, DepthInput::Constant(128))),
    ]);
    let mut limits = RenderLimits::default();
    limits.max_vertices = 4;
    assert!(make_batches_with_limits(&o.sprites, 10, &limits).is_err());
}

#[test]
fn mirrored_color_and_depth_sample_same_logical_pixel() {
    let mut l = layer(
        1,
        2,
        1,
        DepthInput::Bytes {
            key: key(2),
            width: 2,
            height: 1,
            values: vec![153, 255],
        },
    );
    l.flags = 1;
    Arc::make_mut(l.asset.as_mut().unwrap()).rgba.pixels = vec![[255, 0, 0, 255], [0, 255, 0, 255]];
    let o = prepare(vec![Some(l)]);
    let s = &o.sprites[0];
    let f = s.fragment(0, 0, [1.; 3], AlphaPass::ColorDepth).unwrap();
    near(f.premultiplied_color[0], 0.);
    near(f.premultiplied_color[1], 1.);
    near(f.depth.unwrap(), s.anchors.unwrap().back.depth);
}

#[test]
fn fragment_oracle_rejects_malformed_quad_without_panicking() {
    let mut o = prepare(vec![Some(layer(1, 8, 8, DepthInput::Constant(128)))]);
    o.sprites[0].mesh.vertices.clear();
    assert!(o.sprites[0]
        .fragment(0, 0, [1.; 3], AlphaPass::ColorDepth)
        .is_none());
}
