//! Authored normalized Vitaboy geometry. Not original human artwork.
use wonderland_render_core::{Aabb, AssetKey, EntityRef, Mesh, RgbaImage, Vec2, Vec3, Vertex};
use wonderland_world_view::*;
pub fn wrapped_avatar() -> WorldDocument {
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>4</size><world><floors/><walls/></world><objects/></house>",
        "tests:native GPU avatar wrap",
        "native-gpu-v1",
    )
    .unwrap();
    world.provenance.kind = WorldSourceKind::TestFixture;
    world.source_counts = None;
    world.revision.lot_id = Some(777);
    world.revision.epoch = 12;
    world.revision.tick = 9_007_199_254_740_993;
    let mesh = Mesh {
        vertices: [
            (-0.8, 0., -0.4),
            (0.8, 0., -0.4),
            (0.8, 2.4, -0.4),
            (-0.8, 2.4, -0.4),
            (-0.8, 0., 0.4),
            (0.8, 0., 0.4),
            (0.8, 2.4, 0.4),
            (-0.8, 2.4, 0.4),
        ]
        .into_iter()
        .map(|(x, y, z)| Vertex {
            position: Vec3::new(x, y, z),
            normal: Vec3::Z,
            uv: Vec2::new(1.25, -0.75),
            color: [1.; 4],
        })
        .collect(),
        indices: vec![
            0, 1, 2, 2, 3, 0, 4, 6, 5, 6, 4, 7, 0, 4, 5, 5, 1, 0, 3, 2, 6, 6, 7, 3, 1, 5, 6, 6, 2,
            1, 0, 3, 7, 7, 4, 0,
        ],
    };
    world.models.push(WorldModel {
        effective_source: AssetKey([27; 32]),
        effective_content: world.revision.content,
        context: ModelContext::Vitaboy,
        format_version: 1,
        reconstruction_version: 0,
        bounds: Aabb::new(Vec3::new(-0.8, 0., -0.4), Vec3::new(0.8, 2.4, 0.4)).unwrap(),
        groups: vec![vec![ModelPart { texture: 0, mesh }]],
        textures: vec![ModelTexture {
            selector: ModelTextureSelector::Custom { id: 1 },
            effective_asset: AssetKey([28; 32]),
            uv_scale: Vec2::new(1., 1.),
            image: RgbaImage {
                width: 2,
                height: 2,
                pixels: vec![
                    [10, 240, 240, 255],
                    [240, 10, 240, 255],
                    [240, 240, 10, 255],
                    [10, 10, 240, 255],
                ],
            },
        }],
        depth_mask: None,
    });
    world.objects.push(WorldObject {
        source_guid: 0x313D2F9A,
        blueprint: None,
        snapshot: None,
        entity: Some(EntityRef {
            object_id: 42,
            generation: 7,
        }),
        visual_revision: 1,
        position_tiles: Vec3::new(1.5, 2., 0.25),
        yaw_radians: 1.0,
        dynamic_flags: [0; 2],
        room: 0,
        level: 1,
        visible: true,
        selectable: true,
        model: Some(0),
    });
    world
}
