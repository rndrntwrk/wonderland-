//! Rendering coordinates derive from source simulation poses; no FSOm or avatar
//! appearance is invented by the runtime projection.
use sim_core::{
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::RoutineStore,
};
use wonderland_game_runtime::*;
use wonderland_world_view::WorldDocument;

fn graphic_appearance() -> WorldDocument {
    use wonderland_render_core::{Aabb, AssetKey, Vec3};
    use wonderland_world_view::{ModelContext, WorldModel, WorldObject};
    let mut world = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "tests:source graphic selector",
        "authored-fixture",
    )
    .unwrap();
    for id in [100, 102, 104] {
        world.models.push(WorldModel {
            effective_source: AssetKey([id as u8; 32]),
            effective_content: AssetKey([10; 32]),
            context: ModelContext::Dgrp {
                effective_iff: AssetKey([11; 32]),
                chunk_id: id,
            },
            format_version: 2,
            reconstruction_version: 0,
            groups: vec![],
            textures: vec![],
            bounds: Aabb::new(Vec3::ZERO, Vec3::new(1., 1., 1.)).unwrap(),
            depth_mask: None,
        });
    }
    world.objects.push(WorldObject {
        source_guid: 123,
        blueprint: None,
        snapshot: None,
        entity: None,
        visual_revision: 0,
        position_tiles: Vec3::new(2., 2., 0.),
        yaw_radians: 0.,
        dynamic_flags: [0; 2],
        room: 0,
        level: 1,
        visible: true,
        selectable: true,
        model: Some(0),
    });
    // This appearance adds a binding rather than retaining XML source records.
    world.source_counts = None;
    world
}

fn graphic_game(graphic: i16) -> GameRuntime {
    let mut definition = ObjectDefinition::new(123, 0);
    definition.definition = vec![142, 0, 0, 100];
    definition.object_data[0] = graphic;
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![definition],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut game = GameRuntime::new(
        content,
        LotModel::new(8, 8, 2).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 77, 9, 17),
        RuntimeRole::Authority,
    )
    .unwrap();
    game.advance(vec![AcceptedCommand::Spawn(SpawnSpec {
        guid: 123,
        position: TilePos::new(2, 2, 1).center(),
        facing: Facing::NORTH,
        persistent_id: PersistentId(0),
        avatar: false,
    })])
    .unwrap();
    game
}

#[test]
fn source_base_plus_graphic_selects_exact_dgrp_and_survives_snapshot() {
    use sim_core::vm::{EntityField, MemoryAddress};
    let appearance = graphic_appearance();
    let mut game = graphic_game(2);
    let before = game.snapshot().unwrap();
    assert_eq!(
        game.world_document(&appearance).unwrap().objects[0].model,
        Some(1)
    );
    assert_eq!(before, game.snapshot().unwrap());
    let entity = game.projection().entities[0].reference;
    game.advance(vec![AcceptedCommand::WriteMemory {
        address: MemoryAddress::Entity {
            entity,
            field: EntityField::ObjectData,
            index: 0,
        },
        value: 4,
    }])
    .unwrap();
    let world = game.world_document(&appearance).unwrap();
    assert_eq!(world.objects[0].model, Some(2));
    let saved = game.snapshot().unwrap();
    let mut restored = graphic_game(2);
    restored.restore(&saved).unwrap();
    assert_eq!(
        restored.world_document(&world).unwrap().objects[0].model,
        Some(2)
    );
}

#[test]
fn missing_or_ambiguous_dgrp_never_falls_back_to_a_different_graphic() {
    use wonderland_render_core::AssetKey;
    use wonderland_world_view::ModelContext;
    let mut appearance = graphic_appearance();
    let game = graphic_game(4);
    appearance.models[2].context = ModelContext::Dgrp {
        effective_iff: AssetKey([99; 32]),
        chunk_id: 104,
    };
    assert_eq!(
        game.world_document(&appearance).unwrap().objects[0].model,
        None
    );
    appearance = graphic_appearance();
    appearance.models.push(appearance.models[2].clone());
    assert_eq!(
        game.world_document(&appearance).unwrap().objects[0].model,
        None
    );
    assert_eq!(
        graphic_game(3)
            .world_document(&graphic_appearance())
            .unwrap()
            .objects[0]
            .model,
        None
    );
}

#[test]
fn source_graphic_uses_ushort_wrap_and_standalone_binding_is_unchanged() {
    use wonderland_world_view::ModelContext;
    let mut appearance = graphic_appearance();
    let ModelContext::Dgrp { effective_iff, .. } = appearance.models[1].context else {
        unreachable!()
    };
    appearance.models[1].context = ModelContext::Dgrp {
        effective_iff,
        chunk_id: 65535,
    };
    assert_eq!(
        graphic_game(-101)
            .world_document(&appearance)
            .unwrap()
            .objects[0]
            .model,
        Some(1)
    );
    appearance.models[0].context = ModelContext::Standalone;
    assert_eq!(
        graphic_game(4).world_document(&appearance).unwrap().objects[0].model,
        Some(0)
    );
}

#[test]
fn live_geometry_and_generations_reach_normalized_world_without_guessed_assets() {
    let appearance = WorldDocument::from_blueprint_xml(
        "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
        "tests:appearance projection",
        "authored-fixture",
    )
    .unwrap();
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(123, 0)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut lot = LotModel::new(8, 8, 2).unwrap();
    lot.set_floor(TilePos::new(3, 3, 1), 21).unwrap();
    lot.set_terrain_vertex(2, 2, 12).unwrap();
    let mut game = GameRuntime::new(
        content,
        lot,
        RuntimeConfig::new(VmMode::Ts1, 77, 9, 17),
        RuntimeRole::Authority,
    )
    .unwrap();
    game.advance(vec![
        AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::EAST,
            persistent_id: PersistentId(0),
            avatar: false,
        }),
        AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position: TilePos::new(4, 4, 2).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(2),
            avatar: true,
        }),
    ])
    .unwrap();
    let before = game.snapshot().unwrap();
    let world = game.world_document(&appearance).unwrap();
    world.validate().unwrap();
    assert_eq!(world.revision.lot_id, Some(77));
    assert_eq!(world.revision.epoch, 9);
    assert_eq!(world.revision.tick, 1);
    assert_eq!(world.lot.levels, 2);
    assert_eq!(world.lot.tiles.len(), 128);
    assert_eq!(world.lot.tiles[3 * 8 + 3].floor, 21);
    assert_eq!(world.lot.terrain.corners[2 * 9 + 2], 12);
    assert_eq!(
        world.objects[0].position_tiles,
        wonderland_render_core::Vec3::new(3., 3., 0.)
    );
    assert_eq!(
        world.objects[1].position_tiles,
        wonderland_render_core::Vec3::new(4.5, 4.5, 2.95)
    );
    assert_eq!(world.objects[0].entity.unwrap().generation, 1);
    assert!(
        world
            .objects
            .iter()
            .all(|o| o.model.is_none() && o.snapshot.is_none() && o.blueprint.is_none())
    );
    assert!(
        world
            .diagnostics
            .iter()
            .any(|d| d.code == "runtime_model_unavailable")
    );
    assert_eq!(game.snapshot().unwrap(), before);

    // The projection consumes authoritative fractional source motion and yaw,
    // without inventing an appearance or running an independent UI clock.
    let mut state = game.sim().state().clone();
    let actor = state.entities[&ObjectId(2)].info.reference;
    state.source_routing.avatars.insert(
        actor,
        sim_core::route_motion::SourceAvatarMotion {
            position: LotPosition::new(72, 72, 2),
            facing: Facing::NORTH,
            radians: 0.2,
            visual_position: [72.5 / 16., 72.25 / 16., 2.95],
            velocity: [0.01, 0., 0.],
            turn_velocity: 0.02,
        },
    );
    let snapshot = sim_core::snapshot::encode(&state, game.sim().content()).unwrap();
    game.restore(&snapshot).unwrap();
    let projected = game.world_document(&appearance).unwrap();
    assert_eq!(projected.objects[1].position_tiles.x, 72.5 / 16.);
    assert_eq!(projected.objects[1].position_tiles.y, 72.25 / 16.);
    assert_eq!(projected.objects[1].yaw_radians, 0.2);
    assert!(projected.objects[1].model.is_none());
    assert_eq!(game.snapshot().unwrap(), snapshot);
}
