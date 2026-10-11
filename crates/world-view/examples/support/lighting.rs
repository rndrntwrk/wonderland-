use wonderland_render_core::Vec2;
use wonderland_world_view::*;
pub fn lit_world() -> WorldDocument {
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>3</size><world><floors/><walls/></world><objects/></house>",
        "test:light-recipe",
        "fixture-v1",
    )
    .unwrap();
    world.provenance.kind = WorldSourceKind::TestFixture;
    world.source_counts = None;
    world.revision.lot_id = Some(7);
    world.revision.epoch = 3;
    world.revision.tick = 1;
    world.schema_version = LIGHTING_WORLD_SCHEMA_VERSION;
    world.lighting = Some(WorldLighting {
        source: world.revision.content,
        lot_id: 7,
        epoch: 3,
        revision: 1,
        width: 3,
        height: 3,
        stories: world.lot.levels,
        cells: (0..world.lot.levels)
            .flat_map(|f| {
                vec![
                    WorldLightCell {
                        first: u16::from(f) + 1,
                        second: u16::from(f) + 1,
                        diagonal: LightRoomDiagonal::None,
                        floor_pattern: 0
                    };
                    9
                ]
            })
            .collect(),
        rooms: (0..world.lot.levels)
            .map(|f| WorldLightRoom {
                id: u16::from(f) + 1,
                floor: f,
                outside: false,
                outside_light: 0,
                ambient_light: 0,
            })
            .collect(),
        geometry: vec![WorldRoomShadows {
            room: 1,
            floor: 0,
            walls: vec![[Vec2::new(16., 0.), Vec2::new(16., 32.)]],
            objects: vec![],
        }],
        lights: vec![WorldPointLight {
            id: 1,
            room: 1,
            floor: 0,
            position_sixteenths: Vec2::new(1., 16.),
            radius_sixteenths: 32.,
            falloff_multiplier: 1.,
            color: [255; 3],
            intensity: 1.,
            outdoors_color: false,
            window_room: None,
            height: 2.2125,
        }],
        minimum: [0; 4],
        outside: [255; 4],
    });
    world
}
