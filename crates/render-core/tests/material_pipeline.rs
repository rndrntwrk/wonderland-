use wonderland_render_core::{reference::*, *};
fn id(n: u32) -> Option<EntityRef> {
    Some(EntityRef {
        object_id: n,
        generation: 1,
    })
}
fn triangle(ccw: bool, depth: f32, color: [f32; 4]) -> [RasterVertex; 3] {
    let mut t = [(0., 0.), (2., 0.), (0., 2.)].map(|(x, y)| RasterVertex {
        position: Vec3::new(x, y, depth),
        reciprocal_w: 1.,
        color,
    });
    if ccw {
        t.swap(1, 2);
    }
    t
}
fn face(pass: StencilOperation) -> StencilFace {
    StencilFace {
        compare: StencilComparison::Always,
        pass,
        fail: StencilOperation::Keep,
        depth_fail: StencilOperation::Keep,
    }
}
fn pipeline() -> FragmentPipeline {
    FragmentPipeline {
        depth_compare: DepthComparison::LessEqual,
        depth_write: true,
        forced_depth: None,
        stencil: Some(StencilState {
            reference: 1,
            clockwise: face(StencilOperation::Zero),
            counterclockwise: face(StencilOperation::Replace),
        }),
        blend: FragmentBlend::NoColor,
    }
}
#[test]
fn projected_winding_survives_triangle_fill_canonicalization() {
    for ccw in [false, true] {
        let mut s = ReferenceSurface::new(2, 2, &RenderLimits::default()).unwrap();
        s.write_fragment(0, 0, 0.8, [10, 20, 30, 255], id(1), Default::default())
            .unwrap();
        s.set_pipeline(Some(pipeline())).unwrap();
        s.draw_triangle(triangle(ccw, 0.4, [1.; 4]), id(2), Default::default())
            .unwrap();
        assert_eq!(s.stencil_at(0, 0), Some(u8::from(ccw)));
        assert_eq!(s.depth_at(0, 0), Some(0.4));
        assert_eq!(s.pixel(0, 0), Some([10, 20, 30, 255]));
        assert_eq!(s.id_at(0, 0), id(1));
    }
}
#[test]
fn stencil_test_depth_failure_alpha_discard_and_cleanup_have_distinct_effects() {
    let mut s = ReferenceSurface::new(2, 2, &RenderLimits::default()).unwrap();
    s.write_fragment(0, 0, 0.3, [10, 20, 30, 255], id(1), Default::default())
        .unwrap();
    let mut p = pipeline();
    p.stencil.as_mut().unwrap().clockwise.depth_fail = StencilOperation::Replace;
    s.set_pipeline(Some(p)).unwrap();
    assert!(!s
        .write_fragment(0, 0, 0.5, [255; 4], id(2), Default::default())
        .unwrap());
    assert_eq!(s.stencil_at(0, 0), Some(1));
    assert_eq!(s.depth_at(0, 0), Some(0.3));
    p.stencil.as_mut().unwrap().reference = 2;
    p.stencil.as_mut().unwrap().clockwise.compare = StencilComparison::Equal;
    p.stencil.as_mut().unwrap().clockwise.fail = StencilOperation::Zero;
    s.set_pipeline(Some(p)).unwrap();
    assert!(!s
        .write_fragment(0, 0, 0.2, [1, 1, 1, 0], id(2), Default::default())
        .unwrap());
    assert_eq!(
        s.stencil_at(0, 0),
        Some(1),
        "discard must happen before stencil fail"
    );
    assert!(!s
        .write_fragment(0, 0, 0.2, [255; 4], id(2), Default::default())
        .unwrap());
    assert_eq!(s.stencil_at(0, 0), Some(0));
    assert_eq!(s.depth_at(0, 0), Some(0.3));
    p.depth_compare = DepthComparison::Always;
    p.depth_write = false;
    p.forced_depth = Some(1.);
    p.stencil.as_mut().unwrap().reference = 0;
    p.stencil.as_mut().unwrap().clockwise.pass = StencilOperation::Replace;
    s.set_pipeline(Some(p)).unwrap();
    assert!(s
        .write_fragment(0, 0, 0.2, [255; 4], id(2), Default::default())
        .unwrap());
    assert_eq!(s.depth_at(0, 0), Some(0.3));
    assert_eq!(s.id_at(0, 0), id(1));
}
#[test]
fn forced_mask_depth_is_applied_before_homogeneous_clipping() {
    let mut s = ReferenceSurface::new(4, 4, &RenderLimits::default()).unwrap();
    let mut p = pipeline();
    p.depth_compare = DepthComparison::Always;
    p.forced_depth = Some(1.);
    p.stencil.as_mut().unwrap().clockwise.pass = StencilOperation::Replace;
    s.set_pipeline(Some(p)).unwrap();
    let mesh = Mesh {
        vertices: [(-1., -1.), (1., -1.), (0., 1.)]
            .map(|(x, y)| Vertex {
                position: Vec3::new(x, y, 2.),
                normal: Vec3::Y,
                uv: Vec2::ZERO,
                color: [1.; 4],
            })
            .to_vec(),
        indices: vec![0, 1, 2],
    };
    assert!(
        s.draw_mesh(
            &mesh,
            Mat4::IDENTITY,
            None,
            Default::default(),
            &RenderLimits::default()
        )
        .unwrap()
            > 0
    );
    assert_eq!(s.stencil_at(2, 2), Some(1));
    assert_eq!(s.depth_at(2, 2), Some(1.));
    let before = s.digest();
    p.forced_depth = Some(f32::NAN);
    assert!(s.set_pipeline(Some(p)).is_err());
    assert_eq!(s.digest(), before);
}
#[test]
fn nonpremultiplied_source_alpha_is_not_replaced_by_normal_alpha_over() {
    let mut s = ReferenceSurface::new(1, 1, &RenderLimits::default()).unwrap();
    s.clear([0, 0, 255, 255]);
    let mut p = pipeline();
    p.stencil = None;
    p.blend = FragmentBlend::NonPremultiplied;
    s.set_pipeline(Some(p)).unwrap();
    s.write_fragment(0, 0, 0.5, [255, 0, 0, 128], id(2), Default::default())
        .unwrap();
    assert_eq!(s.pixel(0, 0), Some([128, 0, 127, 191]));
    s.set_pipeline(None).unwrap();
    s.clear([0, 0, 255, 255]);
    s.write_fragment(0, 0, 0.5, [255, 0, 0, 128], id(2), Default::default())
        .unwrap();
    assert_eq!(s.pixel(0, 0), Some([128, 0, 127, 255]));
    assert_eq!(s.stencil_at(0, 0), Some(0));
}
