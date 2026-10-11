use wonderland_render_core::{frame::FrameStore, *};

fn fixture_frame(tick: u64, content: u8) -> RenderFrame {
    RenderFrame {
        stamp: FrameStamp {
            lot_id: 1,
            epoch: 1,
            tick,
            architecture_revision: 1,
            content: AssetKey([content; 32]),
        },
        entities: vec![EntityProjection {
            reference: EntityRef {
                object_id: 1,
                generation: 1,
            },
            visual_revision: 1,
            transform: Transform::IDENTITY,
            previous_transform: None,
            asset: AssetKey([8; 32]),
            level: 1,
            visible: true,
            selectable: true,
        }],
        selected: None,
    }
}

#[test]
fn changing_content_away_and_back_cannot_revive_old_pick_tickets() {
    let mut store = FrameStore::new(RenderLimits::default());
    store.reset(1, 1);
    store.admit(fixture_frame(1, 1)).unwrap();
    let id = EntityRef {
        object_id: 1,
        generation: 1,
    };
    let ticket = store.pick_ticket(id).unwrap();
    assert!(store.admit(fixture_frame(1, 2)).is_err());
    assert_eq!(
        store.resolve_pick(&ticket),
        Some(id),
        "rejected content must not mutate its generation"
    );
    store.admit(fixture_frame(2, 2)).unwrap();
    assert_eq!(store.resolve_pick(&ticket), None);
    store.admit(fixture_frame(3, 1)).unwrap();
    assert_eq!(store.resolve_pick(&ticket), None);
    assert_eq!(
        store.resolve_pick(&store.pick_ticket(id).unwrap()),
        Some(id)
    );
}

#[test]
fn large_finite_orthographic_range_still_maps_its_boundary_and_inverts() {
    let matrix = Mat4::orthographic_rh(-3e38, 3e38, -1., 1., 0., 1.).unwrap();
    assert!((matrix.transform_point3(Vec3::new(3e38, 0., 0.)).x - 1.).abs() < 1e-5);
    assert!(matrix.inverse().is_some());
}

#[test]
fn exactly_dependent_columns_have_no_inverse() {
    let matrix = Mat4 {
        cols: [
            [1., 3., 5., 0.],
            [2., 4., 6., 0.],
            [3., 7., 11., 0.],
            [0., 0., 0., 1.],
        ],
    };
    assert_eq!(matrix.transform_vector3(Vec3::new(1., 1., -1.)), Vec3::ZERO);
    assert!(matrix.inverse().is_none());
    assert!(
        Mat4::from_scale(Vec3::new(1e-30, 1e30, 1e-20))
            .inverse()
            .is_some(),
        "representable small pivots remain valid"
    );
}

#[test]
fn unit_quaternion_large_vector_rotation_does_not_overflow_intermediate_cross_product() {
    let rotation = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2).unwrap();
    let vector = Vec3::new(3e38, 0., 0.);
    let result = rotation.rotate_vec3(vector);
    assert!(result.is_finite());
    assert!((result.y / 3e38 - 1.).abs() < 1e-5);
}

#[test]
fn clipping_either_diagonal_of_a_large_quad_covers_each_pixel_once() {
    use wonderland_render_core::reference::{FragmentOptions, ReferenceSurface};
    for extent in [2., 10., 100000.] {
        for indices in [vec![0, 1, 2, 0, 2, 3], vec![0, 1, 3, 1, 2, 3]] {
            let mesh = Mesh {
                vertices: [
                    (-extent, -extent),
                    (extent, -extent),
                    (extent, extent),
                    (-extent, extent),
                ]
                .into_iter()
                .map(|(x, y)| Vertex {
                    position: Vec3::new(x, y, 0.5),
                    normal: Vec3::Z,
                    uv: Vec2::ZERO,
                    color: [1., 0., 0., 0.5],
                })
                .collect(),
                indices,
            };
            let mut image = ReferenceSurface::new(4, 4, &RenderLimits::default()).unwrap();
            let count = image
                .draw_mesh(
                    &mesh,
                    Mat4::IDENTITY,
                    None,
                    FragmentOptions {
                        depth_test: false,
                        write_depth: false,
                        ..FragmentOptions::default()
                    },
                    &RenderLimits::default(),
                )
                .unwrap();
            assert_eq!(count, 16, "extent {extent}");
            assert!(
                image.image().pixels.iter().all(|p| p[3] == 128),
                "extent {extent}: {:?}",
                image.image().pixels
            );
        }
    }
}

#[test]
fn tiny_shared_edge_roundoff_cannot_change_triangle_coverage() {
    use wonderland_render_core::reference::{FragmentOptions, RasterVertex, ReferenceSurface};
    for epsilon in [2.220446e-16, 1e-15] {
        let vertices = [(0., 4.), (4., 4.), (4., epsilon), (0., 0.)].map(|(x, y)| RasterVertex {
            position: Vec3::new(x, y, 0.5),
            reciprocal_w: 1.,
            color: [1., 0., 0., 0.5],
        });
        let mut image = ReferenceSurface::new(4, 4, &RenderLimits::default()).unwrap();
        let options = FragmentOptions {
            depth_test: false,
            write_depth: false,
            ..FragmentOptions::default()
        };
        let a = image
            .draw_triangle([vertices[0], vertices[1], vertices[2]], None, options)
            .unwrap();
        let b = image
            .draw_triangle([vertices[0], vertices[2], vertices[3]], None, options)
            .unwrap();
        assert_eq!(a + b, 16);
        assert!(image.image().pixels.iter().all(|p| p[3] == 128));
    }
}
