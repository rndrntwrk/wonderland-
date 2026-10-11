use wonderland_render_core::reference::{FragmentOptions, ReferenceLightmap, ReferenceSurface};
use wonderland_render_core::{Mat4, Mesh, RenderLimits, RgbaImage, Vec2, Vec3, Vertex};

#[test]
fn malformed_image_is_rejected_without_panicking() {
    for image in [
        RgbaImage {
            width: 0,
            height: 0,
            pixels: vec![],
        },
        RgbaImage {
            width: 0,
            height: 2,
            pixels: vec![],
        },
        RgbaImage {
            width: 2,
            height: 2,
            pixels: vec![[255; 4]],
        },
        RgbaImage {
            width: u32::MAX,
            height: u32::MAX,
            pixels: vec![],
        },
    ] {
        let result = std::panic::catch_unwind(|| {
            ReferenceLightmap::new(&image, Mat4::IDENTITY, &RenderLimits::default())
        });
        assert!(result.is_ok(), "public lightmap admission panicked");
        assert!(result.unwrap().is_err());
    }
}

#[test]
fn bilinear_centers_edges_and_nonfinite_queries_are_explicit() {
    let image = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![
            [0, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [255, 255, 255, 255],
        ],
    };
    let sample = ReferenceLightmap::new(&image, Mat4::IDENTITY, &RenderLimits::default()).unwrap();
    assert_eq!(sample.sample(Vec2::new(0.25, 0.25)).unwrap(), [0., 0., 0.]);
    assert_eq!(sample.sample(Vec2::new(0.75, 0.75)).unwrap(), [1., 1., 1.]);
    assert_eq!(
        sample.sample(Vec2::new(0.5, 0.5)).unwrap(),
        [0.5, 0.5, 0.25]
    );
    assert_eq!(
        sample.sample(Vec2::new(f32::MAX, f32::MAX)).unwrap(),
        [1.; 3]
    );
    assert_eq!(
        sample.sample(Vec2::new(-f32::MAX, -f32::MAX)).unwrap(),
        [0.; 3]
    );
    for uv in [Vec2::new(f32::NAN, 0.), Vec2::new(0., f32::INFINITY)] {
        assert!(sample.sample(uv).is_err());
    }
}

fn quad() -> Mesh {
    Mesh {
        vertices: [
            (-1., -1., 0., 0.),
            (1., -1., 1., 0.),
            (1., 1., 1., 1.),
            (-1., 1., 0., 1.),
        ]
        .into_iter()
        .map(|(x, y, u, v)| Vertex {
            position: Vec3::new(x, y, 0.5),
            normal: Vec3::Z,
            uv: Vec2::new(u, v),
            color: [1.; 4],
        })
        .collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

#[test]
fn lightmap_changes_fragment_color_not_coverage_depth_or_alpha() {
    let limits = RenderLimits::default();
    let mut unlit = ReferenceSurface::new(8, 8, &limits).unwrap();
    let mut lit = ReferenceSurface::new(8, 8, &limits).unwrap();
    let image = RgbaImage {
        width: 2,
        height: 1,
        pixels: vec![[0, 0, 0, 0], [255, 255, 255, 0]],
    };
    let mapping = Mat4 {
        cols: [
            [0.5, 0., 0., 0.],
            [0., 0.5, 0., 0.],
            [0., 0., 0., 0.],
            [0.5, 0.5, 0., 1.],
        ],
    };
    let lighting = ReferenceLightmap::new(&image, mapping, &limits).unwrap();
    unlit
        .draw_mesh(
            &quad(),
            Mat4::IDENTITY,
            None,
            FragmentOptions::default(),
            &limits,
        )
        .unwrap();
    lit.draw_lit_mesh(
        &quad(),
        Mat4::IDENTITY,
        None,
        lighting,
        None,
        FragmentOptions::default(),
        &limits,
    )
    .unwrap();
    assert_eq!(lit.depths(), unlit.depths());
    assert_eq!(lit.ids(), unlit.ids());
    assert!(lit.pixel(0, 4).unwrap()[0] < lit.pixel(7, 4).unwrap()[0]);
    assert!(lit.image().pixels.iter().all(|p| p[3] == 255));
}

#[test]
fn transformed_light_overflow_rejects_before_surface_mutation() {
    let limits = RenderLimits::default();
    let image = RgbaImage {
        width: 1,
        height: 1,
        pixels: vec![[255; 4]],
    };
    let mut mapping = Mat4::IDENTITY;
    mapping.cols[0][0] = f32::MAX;
    let lighting = ReferenceLightmap::new(&image, mapping, &limits).unwrap();
    let mut mesh = quad();
    mesh.vertices[2].position.x = 2.;
    let mut surface = ReferenceSurface::new(8, 8, &limits).unwrap();
    let before = surface.digest();
    assert!(surface
        .draw_lit_mesh(
            &mesh,
            Mat4::IDENTITY,
            None,
            lighting,
            None,
            FragmentOptions::default(),
            &limits
        )
        .is_err());
    assert_eq!(surface.digest(), before);
}
