use wonderland_render_core::{reference::*, *};

fn surface() -> ReferenceSurface {
    ReferenceSurface::new(4, 4, &RenderLimits::default()).unwrap()
}
fn id(n: u32) -> Option<EntityRef> {
    Some(EntityRef {
        object_id: n,
        generation: 1,
    })
}
fn vertex(x: f32, y: f32, z: f32, color: [f32; 4]) -> RasterVertex {
    RasterVertex {
        position: Vec3::new(x, y, z),
        reciprocal_w: 1.,
        color,
    }
}

#[test]
fn straight_alpha_over_preserves_rgb_and_transparent_fragments_do_not_occlude() {
    let mut s = surface();
    let options = FragmentOptions::default();
    assert!(s
        .write_fragment(1, 1, 0.8, [0, 0, 255, 255], id(1), options)
        .unwrap());
    assert!(!s
        .write_fragment(1, 1, 0.1, [255, 0, 0, 0], id(2), options)
        .unwrap());
    assert_eq!(s.pixel(1, 1), Some([0, 0, 255, 255]));
    assert_eq!(s.id_at(1, 1), id(1));
    assert!(s
        .write_fragment(1, 1, 0.5, [255, 0, 0, 128], id(2), options)
        .unwrap());
    assert_eq!(s.pixel(1, 1), Some([128, 0, 127, 255]));
    assert_eq!(s.id_at(1, 1), id(2));
    assert_eq!(s.depth_at(1, 1), Some(0.5));
}

#[test]
fn depth_and_id_overlap_are_ordered_and_out_of_bounds_is_safe() {
    let mut s = surface();
    let options = FragmentOptions::default();
    s.write_fragment(0, 0, 0.2, [255, 0, 0, 255], id(1), options)
        .unwrap();
    assert!(!s
        .write_fragment(0, 0, 0.3, [0, 255, 0, 255], id(2), options)
        .unwrap());
    assert!(!s
        .write_fragment(0, 0, 0.2, [0, 255, 0, 255], id(2), options)
        .unwrap());
    assert_eq!(s.id_at(0, 0), id(1));
    assert!(!s
        .write_fragment(u32::MAX, 0, 0.1, [0; 4], id(2), options)
        .unwrap());
    assert!(s
        .write_fragment(0, 0, f32::NAN, [0; 4], id(2), options)
        .is_err());
    assert_eq!(s.pixel(0, 0), Some([255, 0, 0, 255]));
}

#[test]
fn alpha_cutoff_and_optional_depth_write_support_masked_and_translucent_draws() {
    let mut s = surface();
    let mut options = FragmentOptions {
        alpha_cutoff: 128,
        ..Default::default()
    };
    assert!(!s
        .write_fragment(0, 0, 0.1, [1, 2, 3, 128], id(1), options)
        .unwrap());
    options.alpha_cutoff = 0;
    options.write_depth = false;
    s.write_fragment(0, 0, 0.2, [255, 0, 0, 128], id(1), options)
        .unwrap();
    s.write_fragment(
        0,
        0,
        0.5,
        [0, 0, 255, 255],
        id(2),
        FragmentOptions::default(),
    )
    .unwrap();
    assert_eq!(s.pixel(0, 0), Some([0, 0, 255, 255]));
    assert_eq!(s.id_at(0, 0), id(2));
}

#[test]
fn triangle_top_left_rule_prevents_shared_edge_double_blending() {
    let mut s = surface();
    let color = [1., 0., 0., 0.5];
    let options = FragmentOptions {
        write_depth: false,
        ..FragmentOptions::default()
    };
    s.draw_triangle(
        [
            vertex(0., 0., 0.5, color),
            vertex(4., 0., 0.5, color),
            vertex(4., 4., 0.5, color),
        ],
        id(1),
        options,
    )
    .unwrap();
    s.draw_triangle(
        [
            vertex(0., 0., 0.5, color),
            vertex(4., 4., 0.5, color),
            vertex(0., 4., 0.5, color),
        ],
        id(1),
        options,
    )
    .unwrap();
    assert!(s.image().pixels.iter().all(|p| *p == [255, 0, 0, 128]));
}

#[test]
fn triangles_interpolate_depth_and_colors_at_pixel_centers() {
    let mut s = surface();
    s.draw_triangle(
        [
            vertex(0., 0., 0.2, [1., 0., 0., 1.]),
            vertex(4., 0., 0.6, [0., 1., 0., 1.]),
            vertex(0., 4., 0.6, [0., 0., 1., 1.]),
        ],
        id(3),
        FragmentOptions::default(),
    )
    .unwrap();
    assert_eq!(s.pixel(0, 0), Some([191, 32, 32, 255]));
    assert!((s.depth_at(0, 0).unwrap() - 0.3).abs() < 1e-6);
    assert_eq!(s.id_at(0, 0), id(3));
    assert_eq!(s.pixel(3, 3), Some([0; 4]));
}

#[test]
fn invalid_triangle_is_rejected_without_any_partial_write() {
    let mut s = surface();
    let before = s.digest();
    let color = [1.; 4];
    assert!(s
        .draw_triangle(
            [
                vertex(0., 0., 0.2, color),
                vertex(f32::NAN, 0., 0.2, color),
                vertex(0., 4., 0.2, color)
            ],
            id(1),
            FragmentOptions::default()
        )
        .is_err());
    assert_eq!(s.digest(), before);
    assert_eq!(
        s.draw_triangle(
            [vertex(0., 0., 0.2, color); 3],
            id(1),
            FragmentOptions::default()
        )
        .unwrap(),
        0
    );
}

fn mesh(positions: [Vec3; 3]) -> Mesh {
    Mesh {
        vertices: positions
            .into_iter()
            .map(|position| Vertex {
                position,
                normal: Vec3::Z,
                uv: Vec2::ZERO,
                color: [1., 1., 1., 1.],
            })
            .collect(),
        indices: vec![0, 1, 2],
    }
}

#[test]
fn wrap_addresses_interpolated_fragments_and_preserves_default_clamp() {
    let image = RgbaImage {
        width: 4,
        height: 2,
        pixels: vec![
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
            [0, 255, 255, 255],
            [255, 0, 255, 255],
            [128, 0, 0, 255],
            [0, 128, 0, 255],
        ],
    };
    let mut triangle = mesh([
        Vec3::new(-1., 1., 0.5),
        Vec3::new(1., 1., 0.5),
        Vec3::new(-1., -1., 0.5),
    ]);
    for (vertex, u) in triangle.vertices.iter_mut().zip([0.75, 1.75, 0.75]) {
        vertex.uv = Vec2::new(u, -0.25);
    }
    let mut wrapped = surface();
    let options = FragmentOptions {
        texture_address: TextureAddress::Wrap,
        ..Default::default()
    };
    wrapped
        .draw_textured_mesh(
            &triangle,
            Mat4::IDENTITY,
            &image,
            id(4),
            options,
            &Default::default(),
        )
        .unwrap();
    // Top-row barycentric interpolation yields u=.875,1.125,1.375. Wrapping
    // the vertices before interpolation would collapse the span to constant .75.
    assert_eq!(wrapped.pixel(0, 0), Some([0, 128, 0, 255]));
    assert_eq!(wrapped.pixel(1, 0), Some([0, 255, 255, 255]));
    assert_eq!(wrapped.pixel(2, 0), Some([255, 0, 255, 255]));
    let mut clamped = surface();
    clamped
        .draw_textured_mesh(
            &triangle,
            Mat4::IDENTITY,
            &image,
            id(4),
            Default::default(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(clamped.pixel(1, 0), Some([255, 255, 0, 255]));
    assert_eq!(clamped.pixel(2, 0), Some([255, 255, 0, 255]));
    // The same wrapped alpha sample controls depth/ID, rather than an invisible
    // clamped-edge hit: source integer u=1 wraps back to its transparent origin.
    for vertex in &mut triangle.vertices {
        vertex.uv = Vec2::new(1., 0.);
    }
    let mut alpha = image;
    alpha.pixels[0][3] = 0;
    wrapped.clear([0; 4]);
    wrapped
        .draw_textured_mesh(
            &triangle,
            Mat4::IDENTITY,
            &alpha,
            id(4),
            options,
            &Default::default(),
        )
        .unwrap();
    assert_eq!(wrapped.id_at(0, 0), None);
    assert!(wrapped.depth_at(0, 0).unwrap().is_infinite());
}

#[test]
fn mesh_reference_maps_clip_coordinates_and_clips_near_plane() {
    let mut s = surface();
    let m = mesh([
        Vec3::new(-1., 1., 0.5),
        Vec3::new(1., 1., 0.5),
        Vec3::new(-1., -1., 0.5),
    ]);
    let count = s
        .draw_mesh(
            &m,
            Mat4::IDENTITY,
            id(4),
            FragmentOptions::default(),
            &RenderLimits::default(),
        )
        .unwrap();
    assert!(count > 0);
    assert_eq!(s.id_at(0, 0), id(4));
    assert_eq!(s.id_at(3, 3), None);
    s.clear([0; 4]);
    let clipped = mesh([
        Vec3::new(-1., 1., -0.5),
        Vec3::new(1., 1., 0.5),
        Vec3::new(-1., -1., 0.5),
    ]);
    s.draw_mesh(
        &clipped,
        Mat4::IDENTITY,
        id(5),
        FragmentOptions::default(),
        &RenderLimits::default(),
    )
    .unwrap();
    assert_eq!(s.id_at(0, 0), None);
    assert_eq!(s.id_at(2, 0), id(5));
    assert!(s
        .depths()
        .iter()
        .all(|z| z.is_infinite() || (0. ..=1.).contains(z)));
}

#[test]
fn blit_clips_signed_origin_and_validates_image_before_writing() {
    let mut s = surface();
    let image = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![[10, 20, 30, 255]; 4],
    };
    assert_eq!(
        s.blit(
            &image,
            [-1, -1],
            0.4,
            id(6),
            FragmentOptions::default(),
            &RenderLimits::default()
        )
        .unwrap(),
        1
    );
    assert_eq!(s.pixel(0, 0), Some([10, 20, 30, 255]));
    assert_eq!(s.pixel(1, 0), Some([0; 4]));
    let before = s.digest();
    let invalid = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![],
    };
    assert!(s
        .blit(
            &invalid,
            [0, 0],
            0.2,
            id(7),
            FragmentOptions::default(),
            &RenderLimits::default()
        )
        .is_err());
    assert_eq!(s.digest(), before);
}

#[test]
fn surface_budget_is_checked_and_digest_includes_color_depth_and_game_identity() {
    let limits = RenderLimits {
        max_texture_pixels: 8,
        ..Default::default()
    };
    assert!(ReferenceSurface::new(4, 4, &limits).is_err());
    let mut a = surface();
    let mut b = surface();
    assert_eq!(a.digest(), b.digest());
    a.write_fragment(0, 0, 0.1, [1, 2, 3, 255], id(1), FragmentOptions::default())
        .unwrap();
    b.write_fragment(0, 0, 0.1, [1, 2, 3, 255], id(2), FragmentOptions::default())
        .unwrap();
    assert_ne!(a.digest(), b.digest());
    b.clear([0; 4]);
    b.write_fragment(0, 0, 0.2, [1, 2, 3, 255], id(1), FragmentOptions::default())
        .unwrap();
    assert_ne!(a.digest(), b.digest());
}

#[test]
fn perspective_color_interpolation_uses_reciprocal_w_but_depth_is_screen_linear() {
    let mut s = surface();
    let a = vertex(0., 0., 0.2, [1., 0., 0., 1.]);
    let mut b = vertex(4., 0., 0.6, [0., 1., 0., 1.]);
    let mut c = vertex(0., 4., 0.6, [0., 0., 1., 1.]);
    b.reciprocal_w = 2.;
    c.reciprocal_w = 4.;
    s.draw_triangle([a, b, c], id(1), FragmentOptions::default())
        .unwrap();
    assert_eq!(s.pixel(0, 0), Some([128, 43, 85, 255]));
    assert!((s.depth_at(0, 0).unwrap() - 0.3).abs() < 1e-6);
}

#[test]
fn textured_mesh_uses_nearest_clamp_and_transparent_texel_does_not_claim_id() {
    let mut s = surface();
    let positions = [
        Vec3::new(-1., 1., 0.5),
        Vec3::new(1., 1., 0.5),
        Vec3::new(1., -1., 0.5),
        Vec3::new(-1., -1., 0.5),
    ];
    let uvs = [
        Vec2::new(-0.1, -0.1),
        Vec2::new(1.1, -0.1),
        Vec2::new(1.1, 1.1),
        Vec2::new(-0.1, 1.1),
    ];
    let mesh = Mesh {
        vertices: positions
            .iter()
            .zip(uvs)
            .map(|(&position, uv)| Vertex {
                position,
                normal: Vec3::Z,
                uv,
                color: [1.; 4],
            })
            .collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    let image = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [1, 2, 3, 0],
        ],
    };
    s.draw_textured_mesh(
        &mesh,
        Mat4::IDENTITY,
        &image,
        id(7),
        FragmentOptions::default(),
        &RenderLimits::default(),
    )
    .unwrap();
    assert_eq!(s.pixel(0, 0), Some([255, 0, 0, 255]));
    assert_eq!(s.pixel(3, 0), Some([0, 255, 0, 255]));
    assert_eq!(s.pixel(0, 3), Some([0, 0, 255, 255]));
    assert_eq!(s.id_at(3, 3), None);
    assert_eq!(s.depth_at(3, 3), Some(f32::INFINITY));
}

#[test]
fn invalid_late_mesh_vertex_and_invalid_texture_are_atomic() {
    let mut s = surface();
    let mut m = mesh([
        Vec3::new(-1., 1., 0.5),
        Vec3::new(1., 1., 0.5),
        Vec3::new(-1., -1., 0.5),
    ]);
    m.vertices.push(Vertex {
        position: Vec3::new(f32::MAX, 0., 0.),
        normal: Vec3::X,
        uv: Vec2::ZERO,
        color: [1.; 4],
    });
    let before = s.digest();
    assert!(s
        .draw_mesh(
            &m,
            Mat4::from_scale(Vec3::new(2., 1., 1.)),
            id(1),
            FragmentOptions::default(),
            &RenderLimits::default()
        )
        .is_err());
    assert_eq!(s.digest(), before);
    let invalid = RgbaImage {
        width: 1,
        height: 1,
        pixels: vec![],
    };
    assert!(s
        .draw_textured_mesh(
            &m,
            Mat4::IDENTITY,
            &invalid,
            id(1),
            FragmentOptions::default(),
            &RenderLimits::default()
        )
        .is_err());
    assert_eq!(s.digest(), before);
}
