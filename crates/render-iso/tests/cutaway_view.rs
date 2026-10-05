use wonderland_render_core::*;
use wonderland_render_iso::*;
fn map() -> CutawayMap {
    CutawayMap::new(3, 3, vec![true; 9], 9).unwrap()
}
fn wall() -> WallCutInput {
    WallCutInput {
        top_left_thick: true,
        top_left_style: 1,
        top_right_style: 1,
        shape: WallShape::Cardinal,
    }
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.00001, "{a} != {b}");
}
#[test]
fn cutaways_use_interior_edges_and_only_displayed_top_floor() {
    let mut m = map();
    assert_eq!(
        m.wall_cuts(1, 1, 2, 2, wall()).unwrap(),
        WallCuts {
            top_left: WallCut::Down,
            top_right: WallCut::Down
        }
    );
    assert_eq!(
        m.wall_cuts(1, 1, 1, 2, wall()).unwrap(),
        WallCuts::default()
    );
    m.down[3] = false;
    assert_eq!(
        m.wall_cuts(1, 1, 2, 2, wall()).unwrap().top_left,
        WallCut::Up
    );
    let mut m = map();
    m.down[7] = false;
    assert_eq!(
        m.wall_cuts(1, 1, 2, 2, wall()).unwrap().top_left,
        WallCut::DownRightUpLeft
    );
    assert!(m.wall_cuts(-1, 0, 2, 2, wall()).is_err());
    assert!(CutawayMap::new(3, 2, vec![true; 5], 10).is_err());
    assert!(CutawayMap::new(3, 2, vec![true; 6], 5).is_err());
}
#[test]
fn object_cutaways_require_both_adjacent_tiles_and_nearest_even() {
    let mut m = map();
    m.down[0] = false;
    assert!(m
        .object_hidden(Vec3::new(1., 1., 0.), 1, 1, 0, WallSide::TopLeft)
        .unwrap());
    assert!(!m
        .object_hidden(Vec3::new(0.5, 0.5, 0.), 1, 1, 0, WallSide::None)
        .unwrap());
    assert!(!m
        .object_hidden(Vec3::new(0., 1., 0.), 1, 1, 0, WallSide::TopLeft)
        .unwrap());
    assert!(!m
        .object_hidden(Vec3::new(1., 1., 0.), 1, 2, 0, WallSide::TopLeft)
        .unwrap());
    assert!(!m
        .object_hidden(Vec3::new(1., 1., 0.), 1, 1, 2, WallSide::TopLeft)
        .unwrap());
}
#[test]
fn rotated_cuts_read_neighbors_and_swap_slopes() {
    let m = map();
    let mut c = vec![WallCuts::default(); 9];
    c[4] = WallCuts {
        top_left: WallCut::Down,
        top_right: WallCut::Up,
    };
    c[5] = WallCuts {
        top_left: WallCut::DownLeftUpRight,
        top_right: WallCut::Up,
    };
    c[7] = WallCuts {
        top_left: WallCut::Up,
        top_right: WallCut::DownRightUpLeft,
    };
    assert_eq!(
        m.rotated_cuts(&c, 1, 1, Rotation::TopRight, false).unwrap(),
        WallCuts {
            top_left: WallCut::DownLeftUpRight,
            top_right: WallCut::Down
        }
    );
    assert_eq!(
        m.rotated_cuts(&c, 1, 1, Rotation::BottomRight, false)
            .unwrap(),
        WallCuts {
            top_left: WallCut::DownRightUpLeft,
            top_right: WallCut::DownLeftUpRight
        }
    );
    assert_eq!(
        m.rotated_cuts(&c, 1, 1, Rotation::BottomLeft, false)
            .unwrap(),
        WallCuts {
            top_left: WallCut::Up,
            top_right: WallCut::DownRightUpLeft
        }
    );
}
#[test]
fn cut_styles_select_overlays_omit_doors_and_preserve_down_masks() {
    assert_eq!(
        cut_style(WallCut::DownLeftUpRight, 12, false),
        CutStyle {
            style: 7,
            overlay: Some(252),
            down_mask: false,
            omit: false
        }
    );
    assert_eq!(
        cut_style(WallCut::DownRightUpLeft, 12, true),
        CutStyle {
            style: 8,
            overlay: Some(253),
            down_mask: false,
            omit: true
        }
    );
    assert_eq!(
        cut_style(WallCut::Down, 12, false),
        CutStyle {
            style: 12,
            overlay: None,
            down_mask: true,
            omit: false
        }
    );
}
#[test]
fn generated_wall_depth_preserves_even_rounding_and_logical_height_quirk() {
    let (w, h, q) =
        generated_wall_depth(Zoom::Near, WallShape::Cardinal, WallSide::TopLeft).unwrap();
    assert_eq!((w, h), (64, 271));
    assert_eq!(q[0], 74);
    assert_eq!(q[64], 74);
    assert_eq!(q[65], 76);
    assert_eq!(q[270 * 64 + 63], 255);
    assert_eq!(
        generated_wall_depth(Zoom::Near, WallShape::HorizontalDiagonal, WallSide::None)
            .unwrap()
            .2[0],
        90
    );
    let (w, h, _) =
        generated_wall_depth(Zoom::Medium, WallShape::Cardinal, WallSide::TopLeft).unwrap();
    assert_eq!((w, h), (32, 135));
}
fn intent() -> CameraIntent {
    CameraIntent {
        center_tile: Vec3::new(2., 3., 1.),
        zoom: Zoom::Far,
        precise_zoom: 1.25,
        rotation: Rotation::BottomLeft,
        selected_level: 2,
        selected: Some(EntityRef {
            object_id: 7,
            generation: 3,
        }),
    }
}
#[test]
fn all_modes_preserve_camera_selection_and_keep_view_policy_distinct() {
    let mut state = ViewState::new(
        ViewMode::Full2D,
        intent(),
        Transform::IDENTITY,
        Mat4::IDENTITY,
    )
    .unwrap();
    assert_eq!(state.policy().architecture, Representation::Sprites);
    state
        .switch_to(ViewMode::Hybrid2D, Transform::IDENTITY, Mat4::IDENTITY, 0.)
        .unwrap();
    assert_eq!(state.policy().objects, Representation::Sprites);
    assert_eq!(state.policy().architecture, Representation::Geometry);
    state
        .switch_to(ViewMode::Full3D, Transform::IDENTITY, Mat4::IDENTITY, 0.)
        .unwrap();
    assert_eq!(state.policy().objects, Representation::Geometry);
    assert_eq!(state.sprite_zoom(), Zoom::Near);
    state
        .switch_to(ViewMode::Full2D, Transform::IDENTITY, Mat4::IDENTITY, 0.)
        .unwrap();
    assert_eq!(state.intent, intent());
    assert_eq!(state.sprite_zoom(), Zoom::Far);
}
#[test]
fn transitions_retain_continuity_on_interrupt_and_enforce_safe_2d() {
    let mut state = ViewState::new(
        ViewMode::Full2D,
        intent(),
        Transform::IDENTITY,
        Mat4::IDENTITY,
    )
    .unwrap();
    let mut end = Transform::IDENTITY;
    end.translation = Vec3::new(10., 0., 0.);
    let proj = Mat4::from_scale(Vec3::new(2., 2., 2.));
    state.switch_to(ViewMode::Full3D, end, proj, 0.66).unwrap();
    near(state.sample_camera().pose.translation.x, 0.);
    state.advance(0.33).unwrap();
    near(state.sample_camera().pose.translation.x, 5.);
    near(state.sample_camera().projection.cols[0][0], 1.0137672);
    let before = state.sample_camera();
    state
        .switch_to(ViewMode::Full2D, Transform::IDENTITY, Mat4::IDENTITY, 0.66)
        .unwrap();
    assert_eq!(state.sample_camera().pose, before.pose);
    assert_eq!(state.sample_camera().projection, before.projection);
    assert!(!state.policy().safe_2d);
    assert!(state.policy().immediate);
    assert_eq!(state.policy().objects, Representation::Geometry);
    state.advance(0.66).unwrap();
    assert!(state.policy().safe_2d);
    state.set_rotation_offset(1.).unwrap();
    assert!(!state.policy().safe_2d);
    let before = state.clone();
    assert!(state.advance(-1.).is_err());
    assert_eq!(state, before);
}
#[test]
fn source_rotation_and_zoom_easing_use_presentation_seconds() {
    near(rotation_offset(270., 0.).unwrap(), -90.);
    near(rotation_offset(270., 1. / 6.).unwrap(), -45.);
    near(rotation_offset(270., 1. / 3.).unwrap(), 0.);
    near(smooth_zoom_scale(2., 0.).unwrap(), 2.);
    near(smooth_zoom_scale(2., 0.125).unwrap(), 1.2928932);
    near(smooth_zoom_scale(2., 0.25).unwrap(), 1.);
    assert_eq!(nearest_2d_rotation(0.).unwrap(), Rotation::TopLeft);
    assert_eq!(
        nearest_2d_rotation(std::f32::consts::FRAC_PI_4).unwrap(),
        Rotation::TopRight
    );
    assert_eq!(
        nearest_2d_rotation(std::f32::consts::FRAC_PI_2).unwrap(),
        Rotation::BottomRight
    );
    assert!(nearest_2d_rotation(f32::NAN).is_err());
}

#[test]
fn source_yaw_boundaries_keep_double_precision_after_promoting_f32_input() {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    // These f32 constants lie just above their exact mathematical boundaries.
    // The source promotes them to double before PosMod/division and rounding.
    for (boundary, below, at, above) in [
        (FRAC_PI_2, Rotation::TopRight, Rotation::BottomRight, Rotation::BottomRight),
        (PI, Rotation::BottomRight, Rotation::BottomLeft, Rotation::BottomLeft),
        (3. * FRAC_PI_2, Rotation::BottomLeft, Rotation::TopLeft, Rotation::TopLeft),
        (TAU, Rotation::TopLeft, Rotation::TopRight, Rotation::TopRight),
    ] {
        let bits = boundary.to_bits();
        assert_eq!(nearest_2d_rotation(f32::from_bits(bits - 1)).unwrap(), below);
        assert_eq!(nearest_2d_rotation(boundary).unwrap(), at);
        assert_eq!(nearest_2d_rotation(f32::from_bits(bits + 1)).unwrap(), above);
    }
    assert_eq!(nearest_2d_rotation(0.).unwrap(), Rotation::TopLeft);
    assert_eq!(nearest_2d_rotation(-0.).unwrap(), Rotation::TopLeft);
    assert_eq!(nearest_2d_rotation(-0.0001).unwrap(), Rotation::TopLeft);
    assert_eq!(nearest_2d_rotation(0.0001).unwrap(), Rotation::TopRight);
    assert_eq!(nearest_2d_rotation(-FRAC_PI_2).unwrap(), Rotation::BottomLeft);
    assert_eq!(nearest_2d_rotation(-PI).unwrap(), Rotation::BottomRight);
    assert!(nearest_2d_rotation(f32::INFINITY).is_err());
}
