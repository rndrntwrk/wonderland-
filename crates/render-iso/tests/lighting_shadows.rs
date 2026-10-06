use wonderland_render_core::{AssetKey, Vec2, Vec3};
use wonderland_render_iso::*;

fn point_light() -> ShadowLight {
    ShadowLight {
        kind: ShadowLightKind::Point,
        position_sixteenths: Vec2::new(0., 16.),
        direction: Vec2::new(1., 0.),
        radius_sixteenths: 128.,
        falloff_multiplier: 1.,
    }
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}

// Catches reversed silhouettes, missing solid volumes, and incompatible GPU vertex packing.
#[test]
fn source_wall_volume_occludes_behind_the_wall_and_uploads_64_byte_vertices() {
    let mesh = generate_wall_shadows(
        &[[Vec2::new(16., 0.), Vec2::new(16., 32.)]],
        point_light(),
        LightingBudget::default(),
    )
    .unwrap();
    assert_eq!(mesh.vertices().len(), 12);
    assert_eq!(mesh.indices().len(), 18);
    assert_eq!(mesh.vertices()[0].position, Vec2::new(16., 0.));
    close(mesh.sample_occlusion(Vec2::new(24., 16.), 100).unwrap(), 1.);
    close(mesh.sample_occlusion(Vec2::new(8., 16.), 100).unwrap(), 0.);
    assert_eq!(mesh.vertex_bytes().len(), 12 * 64);
    assert_eq!(&mesh.vertex_bytes()[8..12], &[255; 4]);
    let target = rasterize_shadow(
        &mesh,
        [32, 32],
        Vec2::new(32., 32.),
        LightingBudget::default(),
    )
    .unwrap();
    assert_eq!(target.pixels[16 * 32 + 24], [255, 0, 0, 255]);
    assert_eq!(target.pixels[16 * 32 + 8], [0, 0, 0, 255]);
}

// Catches infinite directional extrusion or ignoring source linear distance attenuation.
#[test]
fn directional_wall_shadow_has_the_source_32_unit_falloff() {
    let light = ShadowLight {
        kind: ShadowLightKind::Directional,
        ..point_light()
    };
    let mesh = generate_wall_shadows(
        &[[Vec2::new(16., 0.), Vec2::new(16., 32.)]],
        light,
        LightingBudget::default(),
    )
    .unwrap();
    close(
        mesh.sample_occlusion(Vec2::new(24., 16.), 100).unwrap(),
        0.75,
    );
    close(mesh.sample_occlusion(Vec2::new(56., 16.), 100).unwrap(), 0.);
}

// Catches replacing the finite object ellipse with the wall's infinite shadow, and double adding objects.
#[test]
fn object_ellipse_uses_max_green_and_preserves_the_light_inside_exclusion() {
    let r = Rect {
        x: 16.,
        y: 0.,
        width: 16.,
        height: 32.,
    };
    let mesh = generate_object_shadows(&[r, r], point_light(), LightingBudget::default()).unwrap();
    close(mesh.sample_occlusion(Vec2::new(24., 16.), 100).unwrap(), 1.);
    close(mesh.sample_occlusion(Vec2::new(60., 16.), 100).unwrap(), 0.);
    let target = rasterize_shadow(
        &mesh,
        [32, 32],
        Vec2::new(32., 32.),
        LightingBudget::default(),
    )
    .unwrap();
    assert_eq!(target.pixels[16 * 32 + 24][0], 0);
    assert!(target.pixels[16 * 32 + 24][1] > 230);
    let inside = ShadowLight {
        position_sixteenths: Vec2::new(24., 16.),
        ..point_light()
    };
    assert!(
        generate_object_shadows(&[r], inside, LightingBudget::default())
            .unwrap()
            .vertices()
            .is_empty()
    );
}

// Catches nonfinite geometry, unchecked output growth, and unbounded offscreen raster work.
#[test]
fn malformed_or_over_budget_shadows_fail_before_publication() {
    let wall = [Vec2::new(16., 0.), Vec2::new(16., 32.)];
    let mut b = LightingBudget::default();
    b.max_vertices = 11;
    assert!(generate_wall_shadows(&[wall], point_light(), b).is_err());
    assert!(generate_wall_shadows(
        &[[Vec2::ZERO, Vec2::ZERO]],
        point_light(),
        LightingBudget::default()
    )
    .is_err());
    assert!(generate_wall_shadows(
        &[wall],
        ShadowLight {
            radius_sixteenths: f32::NAN,
            ..point_light()
        },
        LightingBudget::default()
    )
    .is_err());
    let mesh = generate_wall_shadows(&[wall], point_light(), LightingBudget::default()).unwrap();
    b = LightingBudget::default();
    b.max_raster_samples = 10;
    assert!(rasterize_shadow(&mesh, [32, 32], Vec2::new(32., 32.), b).is_err());
    assert!(mesh.sample_occlusion(Vec2::new(24., 16.), 0).is_err());
}

// Catches affine projection of point shadows and loss of source five-story height attenuation.
#[test]
fn projected_mesh_shadows_execute_source_point_and_directional_transforms() {
    let input = MeshShadowInput {
        source: AssetKey([6; 32]),
        vertices: vec![
            Vec3::new(3., 3., 0.),
            Vec3::new(6., 3., 0.),
            Vec3::new(3., 3., 3.),
        ],
        indices: vec![0, 1, 2],
    };
    let light = ShadowLight {
        position_sixteenths: Vec2::ZERO,
        ..point_light()
    };
    let mesh = project_object_shadow(&input, light, 2., LightingBudget::default()).unwrap();
    assert_eq!(mesh.vertices()[0].position, Vec2::new(32., 0.));
    close(mesh.vertices()[0].attenuation, 0.9322034);
    let target = rasterize_projected_shadow(
        &mesh,
        [64, 64],
        Vec2::new(64., 64.),
        LightingBudget::default(),
    )
    .unwrap();
    assert_eq!(target.pixels[8 * 64 + 40], [0, 238, 0, 255]);
    let directional = project_object_shadow(
        &input,
        ShadowLight {
            kind: ShadowLightKind::Directional,
            ..light
        },
        2.,
        LightingBudget::default(),
    )
    .unwrap();
    close(directional.vertices()[0].position.x, 26.666666);
    let mut crossing = input.clone();
    crossing.vertices[2].y = 7.;
    let clipped = project_object_shadow(&crossing, light, 2., LightingBudget::default()).unwrap();
    assert!(clipped.vertices().len() > 3);
    assert!(clipped.vertices().iter().all(|v| v.position.is_finite()));
    crossing.indices[0] = 99;
    assert!(project_object_shadow(&crossing, light, 2., LightingBudget::default()).is_err());
}

// A single triangle grows the owned Vecs to four elements; upload length is
// three. Accounting must include that still-resident fourth element.
#[test]
fn retained_shadow_bytes_include_spare_vector_capacity() {
    let mesh = project_object_shadow(
        &MeshShadowInput {
            source: AssetKey([7; 32]),
            vertices: vec![
                Vec3::new(0., 1., 0.),
                Vec3::new(3., 1., 0.),
                Vec3::new(0., 1., 3.),
            ],
            indices: vec![0, 1, 2],
        },
        point_light(),
        2.,
        LightingBudget::default(),
    )
    .unwrap();
    assert!(mesh.resident_bytes() >= 4 * 16 + 4 * 4);
}
