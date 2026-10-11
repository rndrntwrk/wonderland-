use wonderland_engine_fixture::*;
use wonderland_render_core::*;

fn empty_scene() -> FixtureScene {
    FixtureScene {
        frame: RenderFrame {
            stamp: FrameStamp {
                lot_id: 1,
                epoch: 1,
                tick: 0,
                architecture_revision: 0,
                content: AssetKey([0; 32]),
            },
            entities: vec![],
            selected: None,
        },
        camera: CameraSpec {
            eye: Vec3::new(0., 0., 1.),
            target: Vec3::ZERO,
            up: Vec3::new(0., 1., 0.),
            vertical_size: 2.,
            fov_y_radians: 1.,
            near: 0.,
            far: 2.,
            orthographic: true,
        },
        draws: vec![],
        sprites: vec![],
        hash: [0; 32],
    }
}
fn sprite(rect: [f32; 4], object_id: u32) -> SpriteDraw {
    SpriteDraw {
        name: "physical-pixel-sprite".into(),
        owner: EntityRef {
            object_id,
            generation: 1,
        },
        rect,
        image: RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![[255; 4]],
        },
        depth: vec![128],
        mask: None,
        back_depth: 0.5,
        front_depth: 0.5,
        lighting: [1.; 3],
        room: 1,
        mirror: false,
    }
}
fn screen_quad([left, top, width, height]: [f32; 4], object_id: u32, color: [f32; 4]) -> DrawMesh {
    let vertex = |x: f32, y: f32| Vertex {
        position: Vec3::new(
            (x / WIDTH as f32 * 2. - 1.) * WIDTH as f32 / HEIGHT as f32,
            1. - y / HEIGHT as f32 * 2.,
            0.,
        ),
        normal: Vec3::new(0., 0., 1.),
        uv: Vec2::ZERO,
        color,
    };
    DrawMesh {
        name: "physical-pixel-quad".into(),
        owner: Some(EntityRef {
            object_id,
            generation: 1,
        }),
        model: Mat4::IDENTITY,
        mesh: Mesh {
            vertices: vec![
                vertex(left, top),
                vertex(left + width, top),
                vertex(left + width, top + height),
                vertex(left, top + height),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        },
        material: MaterialSpec {
            color: [1.; 4],
            image: None,
            unlit: true,
            alpha_cutoff: None,
            double_sided: true,
        },
    }
}

#[test]
fn default_reference_keeps_the_existing_640_by_480_result() {
    let scene = representative_scene(ViewMode::Hybrid2D, 0, 30).unwrap();
    let original = reference_frame(&scene).unwrap();
    let sized = reference_frame_at_size(&scene, WIDTH, HEIGHT).unwrap();
    assert_eq!(original.image, sized.image);
    assert_eq!(original.ids, sized.ids);
    assert_eq!(original.depths, sized.depths);
    assert_eq!(original.digest, sized.digest);
}

#[test]
fn physical_pixel_centers_reveal_a_thin_sprite_without_resampling_encoded_ids() {
    let mut scene = empty_scene();
    scene.sprites.push(sprite([100.6, 100.6, 0.3, 0.3], 37));
    let logical = reference_frame(&scene).unwrap();
    assert!(logical.ids.iter().all(Option::is_none));
    let physical = reference_frame_at_size(&scene, 1280, 960).unwrap();
    assert_eq!(physical.image.width, 1280);
    assert_eq!(physical.image.height, 960);
    assert_eq!(physical.ids.iter().flatten().count(), 1);
    assert_eq!(
        physical.ids[201 * 1280 + 201],
        Some(EntityRef {
            object_id: 37,
            generation: 1
        })
    );
    assert_eq!(physical.depths[201 * 1280 + 201], 0.5);
}

#[test]
fn physical_pixel_centers_rerasterize_thin_mesh_geometry_with_the_same_aspect() {
    let mut scene = empty_scene();
    scene
        .draws
        .push(screen_quad([100.6, 100.6, 0.3, 0.3], 37, [1.; 4]));
    let logical = reference_frame(&scene).unwrap();
    assert!(logical.ids.iter().all(Option::is_none));
    let physical = reference_frame_at_size(&scene, 1280, 960).unwrap();
    assert_eq!(physical.ids.iter().flatten().count(), 1);
    assert_eq!(
        physical.ids[201 * 1280 + 201],
        Some(EntityRef {
            object_id: 37,
            generation: 1
        })
    );
    assert_eq!(physical.depths[201 * 1280 + 201], 0.5);
}

#[test]
fn scaled_sprite_extent_preserves_nearest_mask_depth_and_mirror_sampling() {
    let mut scene = empty_scene();
    let mut draw = sprite([10., 20., 4., 2.], 37);
    draw.image = RgbaImage {
        width: 2,
        height: 1,
        pixels: vec![[255, 0, 0, 255], [0, 0, 255, 255]],
    };
    draw.depth = vec![10, 240];
    draw.mask = Some(vec![255, 0]);
    draw.mirror = true;
    draw.back_depth = 0.8;
    draw.front_depth = 0.6;
    scene.sprites.push(draw);
    let logical = reference_frame(&scene).unwrap();
    let physical = reference_frame_at_size(&scene, 1280, 960).unwrap();
    assert_eq!(logical.ids.iter().flatten().count(), 4);
    assert_eq!(physical.ids.iter().flatten().count(), 16);
    for y in 39..45 {
        for x in 19..29 {
            let selected = (24..28).contains(&x) && (40..44).contains(&y);
            assert_eq!(physical.ids[y * 1280 + x].is_some(), selected, "{x},{y}");
            if selected {
                assert_eq!(physical.image.pixels[y * 1280 + x], [255, 0, 0, 255]);
                assert_eq!(
                    physical.depths[y * 1280 + x],
                    logical.depths[20 * WIDTH as usize + 12]
                );
            }
        }
    }
}

#[test]
fn high_dpi_coplanar_color_and_selection_keep_the_later_source_draw() {
    let mut scene = empty_scene();
    scene.draws = vec![
        screen_quad([100., 100., 2., 2.], 37, [1., 0., 0., 1.]),
        screen_quad([100., 100., 2., 2.], 38, [0., 0., 1., 1.]),
    ];
    let physical = reference_frame_at_size(&scene, 1280, 960).unwrap();
    assert_eq!(physical.ids.iter().flatten().count(), 16);
    assert!(physical.ids.iter().flatten().all(|id| id.object_id == 38));
    for y in 200..204 {
        for x in 200..204 {
            assert_eq!(physical.image.pixels[y * 1280 + x], [0, 0, 255, 255]);
            assert_eq!(physical.depths[y * 1280 + x], 0.5);
        }
    }
}

#[test]
fn malformed_dimensions_and_late_sprite_errors_return_no_partial_frame() {
    let mut scene = empty_scene();
    for (width, height) in [(0, 480), (640, 0), (u32::MAX, 960), (4096, 4096)] {
        assert!(reference_frame_at_size(&scene, width, height).is_err());
    }
    scene.sprites.push(sprite([10., 10., 8., 8.], 37));
    let expected = reference_frame_at_size(&scene, 1280, 960).unwrap();
    scene.sprites.push(sprite([f32::MAX, 0., f32::MAX, 8.], 38));
    let before = bincode::serialize(&scene).unwrap();
    assert!(reference_frame_at_size(&scene, 1280, 960).is_err());
    assert_eq!(bincode::serialize(&scene).unwrap(), before);
    scene.sprites.pop();
    assert_eq!(
        reference_frame_at_size(&scene, 1280, 960).unwrap().digest,
        expected.digest
    );
}
