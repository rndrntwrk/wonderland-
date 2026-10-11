use wonderland_render_3d::city::facade::*;
use wonderland_render_core::derivatives::fsof::*;
use wonderland_render_core::{Vec2, Vec3};
#[test]
fn original_fsof_geometry_and_textures_reach_city_space_without_a_second_tile_conversion() {
    let facade = Fsof {
        compression: TextureCompression::Rgba8,
        floor_width: 1,
        floor_height: 1,
        wall_width: 1,
        wall_height: 1,
        floor_texture: vec![1, 2, 3, 255],
        wall_texture: vec![4, 5, 6, 255],
        night: Some(FsofNight {
            floor_texture: vec![7, 8, 9, 255],
            wall_texture: vec![10, 11, 12, 255],
            light_color: [30, 40, 50, 255],
        }),
        geometry: FacadeGeometry {
            floor: FsofMesh {
                vertices: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(64., 0., 0.),
                    Vec3::new(0., 0., 64.),
                ]
                .into_iter()
                .map(|position| FsofVertex {
                    position,
                    uv: Vec2::ZERO,
                    normal: Vec3::Y,
                })
                .collect(),
                indices: vec![0, 1, 2],
            },
            wall: FsofMesh::default(),
        },
    };
    let bytes = facade.encode(true, FsofLimits::default()).unwrap();
    let loaded = load_source_facade(&bytes, (10, 20), [0; 4], 1., FsofLimits::default()).unwrap();
    assert_eq!(loaded.floor_day.pixels, [[1, 2, 3, 255]]);
    assert_eq!(
        loaded.night.as_ref().unwrap().light_color,
        [30, 40, 50, 255]
    );
    let distance = (loaded.floor.vertices[1].position - loaded.floor.vertices[0].position).length();
    assert!(
        (distance - 64. / 77.).abs() < 1e-5,
        "source city facade width {distance}"
    );
    assert_eq!(loaded.floor.indices, [0, 1, 2]);
    assert!(loaded.bounds.min.is_finite());
    assert!(load_source_facade(&bytes, (10, 20), [0; 4], 0., FsofLimits::default()).is_err());
}
