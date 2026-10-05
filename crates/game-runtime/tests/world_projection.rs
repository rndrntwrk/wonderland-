//! Rendering coordinates derive from source simulation poses; no FSOm or avatar
//! appearance is invented by the runtime projection.
use sim_core::{
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::RoutineStore,
};
use wonderland_game_runtime::*;
use wonderland_world_view::WorldDocument;

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
}
