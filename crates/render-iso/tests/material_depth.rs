use wonderland_render_iso::*;

fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-6, "{a} != {b}");
}

#[test]
fn raw_byte_depth_extrapolates_instead_of_clamping() {
    for (q, want) in [(255, 0.0), (153, 1.0), (0, 2.5), (128, 1.245098)] {
        near(sprite_depth_fraction(q), want);
    }
}

#[test]
fn dynamic_masks_keep_two_words_and_reject_unrepresentable_ranges() {
    for id in [100, 163, 164, 227] {
        assert!(!dynamic_sprite_visible(id, 100, 128, [0, 0]).unwrap());
    }
    for id in [100, 163, 164, 227] {
        assert!(dynamic_sprite_visible(id, 100, 128, [0x8000000000000001; 2]).unwrap());
    }
    assert!(dynamic_sprite_visible(99, 100, 128, [0, 0]).unwrap());
    assert!(dynamic_sprite_visible(228, 100, 128, [0, 0]).unwrap());
    assert!(dynamic_sprite_visible(128, 0, 129, [0, 0]).is_err());
    assert!(dynamic_sprite_visible(u32::MAX, u32::MAX, 2, [0, 0]).is_err());
}

#[test]
fn source_alpha_passes_disagree_at_low_alpha_bytes() {
    for (q, basic, color, id) in [
        (0, false, false, false),
        (1, true, false, false),
        (2, true, false, false),
        (3, true, true, false),
        (25, true, true, false),
        (26, true, true, true),
        (255, true, true, true),
    ] {
        assert_eq!(alpha_survives(q, AlphaPass::Basic), basic);
        assert_eq!(alpha_survives(q, AlphaPass::ColorDepth), color);
        assert_eq!(alpha_survives(q, AlphaPass::ObjectIdDepth), id);
    }
    assert!(!alpha_survives(2, AlphaPass::Restore));
}

#[test]
fn wall_mask_replaces_color_alpha_and_premultiplies_once() {
    let shaded = shade_fragment(
        [255, 0, 0, 0],
        Some(128),
        [1.; 3],
        65535,
        GammaMode::Basic,
        AlphaPass::Wall,
    )
    .unwrap();
    near(shaded[0], 128.0 / 255.0);
    near(shaded[3], 128.0 / 255.0);
    assert!(shade_fragment(
        [255; 4],
        Some(0),
        [1.; 3],
        65535,
        GammaMode::Basic,
        AlphaPass::Wall
    )
    .is_none());
}

#[test]
fn sentinel_rooms_apply_inversion_grayscale_and_fullbright() {
    let bright = shade_fragment(
        [255, 128, 0, 255],
        None,
        [0.; 3],
        65535,
        GammaMode::Basic,
        AlphaPass::Basic,
    )
    .unwrap();
    near(bright[0], 1.0);
    near(bright[1], 128.0 / 255.0);
    let invert = shade_fragment(
        [255, 0, 0, 255],
        None,
        [1.; 3],
        65534,
        GammaMode::Basic,
        AlphaPass::Basic,
    )
    .unwrap();
    assert_eq!(invert, [0., 1., 1., 1.]);
    let gray = shade_fragment(
        [255, 0, 0, 255],
        None,
        [1.; 3],
        65533,
        GammaMode::Basic,
        AlphaPass::Basic,
    )
    .unwrap();
    for c in &gray[..3] {
        near(*c, 0.2989);
    }
}

#[test]
fn room_diagonals_retain_source_equality_rules() {
    assert_eq!(room_at(0x8011002A, [0.4, 0.6]), 17);
    assert_eq!(room_at(0x8011002A, [0.4, 0.5]), 42);
    assert_eq!(room_at(0x0011002A, [0.5, 0.5]), 17);
    assert_eq!(room_at(0x0011002A, [0.6, 0.5]), 42);
}

#[test]
fn wall_and_floor_occlusion_modify_separate_light_channels() {
    let half = point_light([1.; 4], 0.5, 0., 0., [1.; 2]);
    for c in half {
        near(c, 0.21763764);
    }
    let occ = point_light([1.; 4], 0.5, 0.25, 0.5, [1.; 2]);
    near(occ[0], 0.16322823);
    near(occ[3], 0.081614115);
}

#[test]
fn packed_source_depth_wraps_one_and_roundtrips_short_ids() {
    for id in [1u16, 255, 256, 257, 32767] {
        let decoded = (unpack_depth(pack_depth(f32::from(id) / 65535.)) * 65535.).round() as u16;
        assert_eq!(decoded, id);
    }
    near(unpack_depth(pack_depth(1.0)), 0.0);
    near(unpack_depth([1.; 4]), 1.0039369);
}

#[test]
fn gamma_modes_use_source_transfer_without_automatic_srgb() {
    near(gamma_multiply(0.5, 0.5, GammaMode::Basic), 0.36487004);
    // Independent double-precision source-equation calculation: .3614746471584.
    near(gamma_multiply(0.5, 0.5, GammaMode::Advanced), 0.36147465);
    let restored = shade_fragment(
        [64, 0, 0, 128],
        None,
        [0.; 3],
        1,
        GammaMode::Basic,
        AlphaPass::Restore,
    )
    .unwrap();
    near(restored[0], 64. / 255.);
    assert!(shade_fragment(
        [255; 4],
        None,
        [f32::NAN, 1., 1.],
        1,
        GammaMode::Basic,
        AlphaPass::Basic
    )
    .is_none());
}

#[test]
fn wall_shader_does_not_apply_sprite_highlight_sentinels() {
    let wall = shade_fragment(
        [255, 0, 0, 255],
        Some(255),
        [1.; 3],
        65534,
        GammaMode::Basic,
        AlphaPass::Wall,
    )
    .unwrap();
    assert_eq!(wall, [1., 0., 0., 1.]);
}
