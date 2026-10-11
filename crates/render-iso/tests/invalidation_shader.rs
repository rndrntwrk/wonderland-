use wonderland_render_iso::*;
#[test]
fn standalone_room_and_unimportant_light_changes_are_not_lost() {
    let room = plan_invalidation(IsoChanges {
        rooms: true,
        ..IsoChanges::default()
    });
    assert!(room.room_maps);
    assert!(room.lighting);
    assert!(room.roofs);
    assert!(room.static_surface);
    let light = plan_invalidation(IsoChanges {
        lighting_unimportant: true,
        ..IsoChanges::default()
    });
    assert!(light.static_surface);
    assert!(light.lighting);
    assert_eq!(
        plan_invalidation(IsoChanges {
            wall_cut: true,
            ..IsoChanges::default()
        })
        .walls,
        WallRecache::CutOnly
    );
    assert_eq!(
        plan_invalidation(IsoChanges {
            wall_cut: true,
            walls: true,
            ..IsoChanges::default()
        })
        .walls,
        WallRecache::Full
    );
    let view = plan_invalidation(IsoChanges {
        rotation: true,
        ..IsoChanges::default()
    });
    assert!(view.sprites);
    assert!(view.floors);
    assert_eq!(view.walls, WallRecache::Full);
    assert!(
        plan_invalidation(IsoChanges {
            precise_zoom: true,
            ..IsoChanges::default()
        })
        .immediate
    );
}
#[test]
fn source_time_and_color_thresholds_preserve_exact_boundaries() {
    assert!(!time_light_changed(0., 0.001).unwrap());
    assert!(time_light_changed(0., 0.001001).unwrap());
    assert!(!outside_light_changed([0, 0, 0, 255], [20, 0, 0, 255]));
    assert!(outside_light_changed([0, 0, 0, 255], [21, 0, 0, 255]));
    assert!(outside_light_changed([254; 4], [255; 4]));
    assert!(time_light_changed(0., f64::NAN).is_err());
}
#[test]
fn missing_light_bindings_have_explicit_source_fallbacks_and_sampler_policies() {
    let b = lighting_bindings(LightingResources::default(), [0.2, 0.3, 0.4, 1.]).unwrap();
    assert_eq!(b.ambient, LightBinding::Constant([1.; 4]));
    assert_eq!(b.advanced, LightBinding::Constant([0.2, 0.3, 0.4, 1.]));
    assert_eq!(
        b.direction,
        LightBinding::Constant([128. / 255., 0., 0., 1.])
    );
    assert_eq!(b.advanced_sampling, Sampling::LinearClamp);
    assert_eq!(b.ambient_sampling, Sampling::PointClamp);
}
#[test]
fn legacy_dynamic_blend_difference_is_visible_without_double_premultiplying_input() {
    let src = [0.5, 0., 0., 0.5];
    let dst = [0., 0., 0., 1.];
    assert_eq!(
        blend_over(src, dst, BlendPolicy::Premultiplied).unwrap(),
        [0.5, 0., 0., 1.]
    );
    assert_eq!(
        blend_over(src, dst, BlendPolicy::LegacyDynamicNonPremultiplied).unwrap(),
        [0.25, 0., 0., 0.75]
    );
}
