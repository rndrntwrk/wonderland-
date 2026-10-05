use wonderland_render_core::reference::{DepthComparison, FragmentOptions, ReferenceSurface};
use wonderland_render_core::{EntityRef, RenderLimits};

#[test]
fn default_strict_depth_keeps_the_first_equal_fragment() {
    let mut surface = ReferenceSurface::new(1, 1, &RenderLimits::default()).unwrap();
    let options = FragmentOptions::default();
    assert!(surface
        .write_fragment(0, 0, 0.5, [255, 0, 0, 255], None, options)
        .unwrap());
    assert!(!surface
        .write_fragment(0, 0, 0.5, [0, 0, 255, 255], None, options)
        .unwrap());
    assert_eq!(surface.pixel(0, 0), Some([255, 0, 0, 255]));
}

#[test]
fn source_less_equal_replaces_ties_without_accepting_farther_depths() {
    let mut surface = ReferenceSurface::new(1, 1, &RenderLimits::default()).unwrap();
    surface.set_depth_comparison(DepthComparison::LessEqual);
    let options = FragmentOptions::default();
    let owner = EntityRef {
        object_id: 7,
        generation: 1,
    };
    assert!(surface
        .write_fragment(0, 0, 0.5, [255, 0, 0, 255], Some(owner), options)
        .unwrap());
    assert!(surface
        .write_fragment(0, 0, 0.5, [0, 0, 255, 255], None, options)
        .unwrap());
    assert_eq!(surface.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(surface.id_at(0, 0), None);
    let farther = f32::from_bits(0.5_f32.to_bits() + 1);
    assert!(!surface
        .write_fragment(0, 0, farther, [0, 255, 0, 255], Some(owner), options)
        .unwrap());
    assert_eq!(surface.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(surface.depth_at(0, 0), Some(0.5));
}
