//! Construction uses A's source geometry/quote/receipt engine inside accepted
//! ticks. The authority values here are explicit fixtures, not an account service.
use sim_core::{
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::RoutineStore,
    world::{WorldObject, build::*},
};
use std::collections::BTreeMap;
use wonderland_game_runtime::*;
fn setup() -> (GameRuntime, EntityRef, BuildAuthority) {
    let mut definition = ObjectDefinition::new(123, 0);
    definition.object_data[42] = 3;
    definition.object_data[4] = 1;
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![definition],
        vec![],
        TuningSet::default(),
    )
    .unwrap()
    .with_build_catalog(vec![(987, 123)])
    .unwrap();
    let mut game = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 77, 1, 15),
        RuntimeRole::Authority,
    )
    .unwrap();
    let out = game
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(9),
            avatar: false,
        })])
        .unwrap();
    let actor = out
        .events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::Spawned(e) = e {
                Some(*e)
            } else {
                None
            }
        })
        .unwrap();
    let mut authority = BuildAuthority::new(actor, PersistentId(9), 1000);
    authority.can_build = true;
    authority.can_manage_others = true;
    authority.prices.floor = 7;
    authority.prices.objects.insert(987, 200);
    (game, actor, authority)
}
fn confirmation(request: &DurableBuildRequest) -> ServerBuildConfirmation {
    ServerBuildConfirmation {
        operation: request.operation,
        actor: request.actor,
        owner: request.owner,
        preview_hash: request.preview_hash,
        charged_cost: request.cost,
        durable_receipt: 99,
        status: DurableBuildStatus::Committed,
        created_objects: BTreeMap::new(),
    }
}
#[test]
fn architecture_changes_only_after_matching_accepted_confirmation() {
    let (mut game, actor, authority) = setup();
    let tile = TilePos::new(2, 2, 1);
    let intent = BuildIntent::new(
        1,
        actor,
        game.sim().state().world.lot.revision().architecture,
        vec![BuildEdit::SetFloor { tile, pattern: 1 }],
    );
    let preview = game.preview_build(&intent, &authority).unwrap();
    assert_eq!(preview.cost, 7);
    assert_eq!(game.sim().state().world.lot.tile(tile).unwrap().floor, 0);
    let out = game
        .advance(vec![AcceptedCommand::BeginBuild {
            preview,
            authority: authority.clone(),
        }])
        .unwrap();
    let request = out
        .events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::BuildRequested(r) = e {
                Some(r.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(game.sim().state().world.lot.tile(tile).unwrap().floor, 0);
    let mut bad = confirmation(&request);
    bad.preview_hash[0] ^= 1;
    let before = game.snapshot().unwrap();
    assert!(
        game.advance(vec![AcceptedCommand::CompleteBuild(bad)])
            .is_err()
    );
    assert_eq!(game.snapshot().unwrap(), before);
    game.advance(vec![AcceptedCommand::CompleteBuild(confirmation(&request))])
        .unwrap();
    assert_eq!(game.sim().state().world.lot.tile(tile).unwrap().floor, 1);
    let revision = game.sim().state().world.lot.revision();
    game.advance(vec![AcceptedCommand::CompleteBuild(confirmation(&request))])
        .unwrap();
    assert_eq!(game.sim().state().world.lot.revision(), revision);
}
#[test]
fn committed_move_updates_simulation_pose_and_world_in_one_tick() {
    let (mut game, actor, authority) = setup();
    let world = game.sim().state().world.object(actor).unwrap();
    let position = TilePos::new(5, 4, 1).center();
    let intent = BuildIntent::new(
        2,
        actor,
        game.sim().state().world.lot.revision().architecture,
        vec![BuildEdit::MoveObject {
            entity: actor,
            expected_revision: world.revision,
            position,
            facing: Facing::EAST,
        }],
    );
    let preview = game.preview_build(&intent, &authority).unwrap();
    let out = game
        .advance(vec![AcceptedCommand::BeginBuild { preview, authority }])
        .unwrap();
    let request = out
        .events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::BuildRequested(r) = e {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    game.advance(vec![AcceptedCommand::CompleteBuild(confirmation(request))])
        .unwrap();
    let projected = game.projection();
    assert_eq!(projected.entities[0].position, position);
    assert_eq!(projected.entities[0].facing, Facing::EAST);
    assert_eq!(projected.world.object(actor).unwrap().position, position);
}
#[test]
fn durable_purchase_creates_real_entity_and_thread_once() {
    let (mut game, actor, authority) = setup();
    let reference = game.sim().state().ids.clone().allocate().unwrap();
    let object = WorldObject::new(reference, TilePos::new(5, 5, 1).center());
    let intent = BuildIntent::new(
        3,
        actor,
        game.sim().state().world.lot.revision().architecture,
        vec![BuildEdit::PlaceObject {
            catalog_id: 987,
            object,
        }],
    );
    let preview = game.preview_build(&intent, &authority).unwrap();
    let out = game
        .advance(vec![AcceptedCommand::BeginBuild { preview, authority }])
        .unwrap();
    let request = out
        .events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::BuildRequested(r) = e {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    let mut completed = confirmation(request);
    completed
        .created_objects
        .insert(reference, PersistentId(999));
    let out = game
        .advance(vec![AcceptedCommand::CompleteBuild(completed.clone())])
        .unwrap();
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e,RuntimeEvent::Spawned(e) if *e==reference))
    );
    assert_eq!(
        game.sim().state().entities[&reference.object_id].info.guid,
        123
    );
    assert_eq!(
        game.sim().state().entities[&reference.object_id]
            .info
            .persistent_id,
        999
    );
    assert!(
        game.sim()
            .state()
            .threads
            .contains_key(&reference.object_id)
    );
    let out = game
        .advance(vec![AcceptedCommand::CompleteBuild(completed)])
        .unwrap();
    assert!(
        !out.events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::Spawned(_)))
    );
    assert_eq!(game.sim().state().entities.len(), 2);
}
#[test]
fn unauthorized_build_has_no_prepared_effect() {
    let (game, actor, mut authority) = setup();
    authority.can_build = false;
    let intent = BuildIntent::new(
        4,
        actor,
        game.sim().state().world.lot.revision().architecture,
        vec![BuildEdit::SetFloor {
            tile: TilePos::new(2, 2, 1),
            pattern: 1,
        }],
    );
    let before = game.snapshot().unwrap();
    assert!(game.preview_build(&intent, &authority).is_err());
    assert_eq!(game.snapshot().unwrap(), before);
}
