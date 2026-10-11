use sim_core::{
    ids::{EntityRef, ObjectId, PersistentId},
    world::{build::*, *},
};
use std::collections::BTreeMap;

fn entity(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}
fn setup() -> (WorldState, BuildAuthority) {
    let mut world = WorldState::new(LotModel::new(8, 8, 2).unwrap());
    let mut actor = WorldObject::new(entity(1), TilePos::new(1, 1, 1).center());
    actor.rules.is_avatar = true;
    world.insert_object(actor).unwrap();
    let mut authority = BuildAuthority::new(entity(1), PersistentId(50), 1_000);
    authority.can_build = true;
    authority.permissions_revision = 4;
    authority.account_revision = 7;
    authority.catalog_revision = 12;
    authority.prices.objects.insert(77, 100);
    authority.prices.floor = 5;
    authority.prices.wall_side = 10;
    authority.undo_policy = UndoPolicy::Compensating {
        version: 1,
        max_records: 8,
    };
    (world, authority)
}
fn purchase(world: &WorldState, operation: u64, id: i16) -> BuildIntent {
    let mut object = WorldObject::new(entity(id), TilePos::new(4, 4, 1).center());
    object.owner = Some(PersistentId(50));
    BuildIntent::new(
        operation,
        entity(1),
        world.lot.revision().architecture,
        vec![BuildEdit::PlaceObject {
            catalog_id: 77,
            object,
        }],
    )
}
fn preview(world: &WorldState, intent: BuildIntent, authority: &BuildAuthority) -> BuildPreview {
    world
        .preview_build(intent, authority, &mut |_c: &IntersectionCall| false)
        .unwrap()
}
fn begin(
    world: &mut WorldState,
    preview: BuildPreview,
    authority: &BuildAuthority,
) -> DurableBuildRequest {
    match world.begin_build_commit(preview, authority).unwrap() {
        BuildBegin::Effect(effect) => effect,
        other => panic!("{other:?}"),
    }
}
fn confirmed(effect: &DurableBuildRequest) -> ServerBuildConfirmation {
    ServerBuildConfirmation {
        operation: effect.operation,
        actor: effect.actor,
        owner: effect.owner,
        preview_hash: effect.preview_hash,
        charged_cost: effect.cost,
        durable_receipt: effect.operation + 100,
        status: DurableBuildStatus::Committed,
        created_objects: effect
            .purchases
            .iter()
            .map(|p| (p.entity, PersistentId(1_000 + p.entity.object_id.0 as u32)))
            .collect(),
    }
}

#[test]
fn replacing_a_portal_authorizes_and_locks_the_previous_entity_and_endpoints() {
    let (mut world, mut authority) = setup();
    for (id, owner, tile) in [
        (10, 50, TilePos::new(5, 5, 1)),
        (11, 99, TilePos::new(2, 2, 1)),
    ] {
        let mut object = WorldObject::new(entity(id), tile.center());
        object.owner = Some(PersistentId(owner));
        object.entrypoints.insert(15);
        world.insert_object(object).unwrap();
    }
    let old = Portal {
        id: PortalId(1),
        entity: entity(11),
        entry: TilePos::new(2, 2, 1).center(),
        exit: TilePos::new(2, 3, 1).center(),
        bidirectional: true,
        enabled: true,
        cost: 16,
        revision: 0,
    };
    world.lot.upsert_portal(old.clone()).unwrap();
    let replacement = Portal {
        entity: entity(10),
        entry: TilePos::new(5, 5, 1).center(),
        exit: TilePos::new(5, 6, 1).center(),
        ..old.clone()
    };
    let intent = BuildIntent::new(
        90,
        entity(1),
        world.lot.revision().architecture,
        vec![BuildEdit::SetPortal {
            portal: replacement,
        }],
    );
    let before = world.clone();
    assert_eq!(
        world.preview_build(intent.clone(), &authority, &mut |_: &IntersectionCall| {
            false
        }),
        Err(BuildError::NotAuthorized)
    );
    assert_eq!(world, before);
    authority.can_manage_others = true;
    let preview = preview(&world, intent, &authority);
    assert!(preview.changes.entities.contains(&entity(11)));
    assert!(preview.changes.tiles.contains(&old.entry.tile().unwrap()));
    assert!(preview.changes.tiles.contains(&old.exit.tile().unwrap()));
    begin(&mut world, preview, &authority);
    assert_eq!(
        world.move_object(entity(11), TilePos::new(7, 7, 1).center(), Facing::NORTH),
        Err(WorldError::BuildLocked)
    );
}

#[test]
fn purchase_is_pure_until_durable_confirmation_and_duplicates_commit_geometry_once() {
    let (mut world, authority) = setup();
    let before = world.clone();
    let preview = preview(&world, purchase(&world, 1, 2), &authority);
    assert_eq!(world, before);
    assert_eq!(preview.cost, 100);
    let effect = begin(&mut world, preview.clone(), &authority);
    assert!(world.object(entity(2)).is_none());
    assert_eq!(world.lot, before.lot);
    assert_eq!(authority.balance, 1_000);
    assert_eq!(
        world
            .begin_build_commit(preview.clone(), &authority)
            .unwrap(),
        BuildBegin::Effect(effect.clone())
    );
    let bytes = bincode::serialize(&world).unwrap();
    let mut restored: WorldState = bincode::deserialize(&bytes).unwrap();
    restored.validate().unwrap();
    let confirmation = confirmed(&effect);
    let result = world.complete_build_commit(confirmation.clone()).unwrap();
    let resumed = restored
        .complete_build_commit(confirmation.clone())
        .unwrap();
    assert_eq!(result, resumed);
    assert_eq!(world, restored);
    assert!(matches!(result.status, BuildCommitStatus::Committed { .. }));
    assert_eq!(
        world.object(entity(2)).unwrap().owner,
        Some(PersistentId(50))
    );
    let committed = world.clone();
    assert_eq!(world.complete_build_commit(confirmation).unwrap(), result);
    assert_eq!(world, committed);
    assert_eq!(authority.balance, 1_000);
    assert_eq!(
        world.begin_build_commit(preview, &authority).unwrap(),
        BuildBegin::AlreadyCompleted(result)
    );
    world.validate().unwrap();
}

#[test]
fn competing_previews_cannot_both_purchase_into_one_occupied_tile() {
    let (mut world, authority) = setup();
    let a = preview(&world, purchase(&world, 1, 2), &authority);
    let b = preview(&world, purchase(&world, 2, 3), &authority);
    let effect = begin(&mut world, a, &authority);
    assert_eq!(
        world.begin_build_commit(b.clone(), &authority),
        Err(BuildError::Busy)
    );
    world.complete_build_commit(confirmed(&effect)).unwrap();
    assert_eq!(
        world.begin_build_commit(b, &authority),
        Err(BuildError::StaleObject)
    );
    assert!(world.object(entity(3)).is_none());
    assert_eq!(world.builds.outcomes().len(), 1);
}

#[test]
fn insufficient_funds_permissions_and_disconnect_are_checked_before_effect_creation() {
    let (mut world, mut authority) = setup();
    authority.balance = 99;
    assert_eq!(
        world.preview_build(
            purchase(&world, 1, 2),
            &authority,
            &mut |_c: &IntersectionCall| false
        ),
        Err(BuildError::InsufficientFunds)
    );
    assert!(world.builds.pending_effect().is_none());
    authority.balance = 1_000;
    let p = preview(&world, purchase(&world, 1, 2), &authority);
    authority.can_build = false;
    assert_eq!(
        world.begin_build_commit(p.clone(), &authority),
        Err(BuildError::NotAuthorized)
    );
    authority.can_build = true;
    authority.connected = false;
    assert_eq!(
        world.begin_build_commit(p.clone(), &authority),
        Err(BuildError::Disconnected)
    );
    authority.connected = true;
    authority.permissions_revision += 1;
    assert_eq!(
        world.begin_build_commit(p, &authority),
        Err(BuildError::StalePreview)
    );
    assert!(world.builds.pending_effect().is_none());
    assert!(world.object(entity(2)).is_none());
}

#[test]
fn architecture_and_object_revisions_invalidate_stale_previews() {
    let (mut world, authority) = setup();
    let p = preview(&world, purchase(&world, 1, 2), &authority);
    world.lot.set_floor(TilePos::new(6, 6, 1), 3).unwrap();
    assert_eq!(
        world.begin_build_commit(p, &authority),
        Err(BuildError::StalePreview)
    );
    let mut item = WorldObject::new(entity(2), TilePos::new(4, 4, 1).center());
    item.owner = Some(authority.owner);
    world.insert_object(item).unwrap();
    let item = world.object(entity(2)).unwrap().clone();
    let intent = BuildIntent::new(
        2,
        entity(1),
        world.lot.revision().architecture,
        vec![BuildEdit::MoveObject {
            entity: item.entity,
            expected_revision: item.revision,
            position: TilePos::new(5, 4, 1).center(),
            facing: Facing::NORTH,
        }],
    );
    let p = preview(&world, intent, &authority);
    world
        .move_object(entity(2), TilePos::new(3, 4, 1).center(), Facing::NORTH)
        .unwrap();
    assert_eq!(
        world.begin_build_commit(p, &authority),
        Err(BuildError::StaleObject)
    );
}

#[test]
fn pending_geometry_locks_block_conflicts_but_keep_unrelated_avatar_movement() {
    let (mut world, authority) = setup();
    let p = preview(&world, purchase(&world, 1, 2), &authority);
    let effect = begin(&mut world, p, &authority);
    let original = world.object(entity(1)).unwrap().position;
    assert_eq!(
        world.move_object(entity(1), TilePos::new(4, 4, 1).center(), Facing::NORTH),
        Err(WorldError::BuildLocked)
    );
    assert_eq!(world.object(entity(1)).unwrap().position, original);
    world
        .move_object(entity(1), TilePos::new(1, 2, 1).center(), Facing::NORTH)
        .unwrap();
    let changed = world.object(entity(1)).unwrap().clone();
    world.complete_build_commit(confirmed(&effect)).unwrap();
    assert_eq!(world.object(entity(1)), Some(&changed));
    assert!(world.object(entity(2)).is_some());
}

#[test]
fn receipt_mismatches_and_conflicting_duplicate_payloads_are_rejected_without_mutation() {
    let (mut world, authority) = setup();
    let p = preview(&world, purchase(&world, 1, 2), &authority);
    let effect = begin(&mut world, p, &authority);
    let pending = world.clone();
    let mut wrong = confirmed(&effect);
    wrong.charged_cost = 99;
    assert_eq!(
        world.complete_build_commit(wrong),
        Err(BuildError::InvalidConfirmation)
    );
    assert_eq!(world, pending);
    let mut wrong = confirmed(&effect);
    wrong.actor.generation = 2;
    assert_eq!(
        world.complete_build_commit(wrong),
        Err(BuildError::InvalidConfirmation)
    );
    assert_eq!(world, pending);
    let mut wrong = confirmed(&effect);
    wrong.created_objects.clear();
    assert_eq!(
        world.complete_build_commit(wrong),
        Err(BuildError::InvalidConfirmation)
    );
    assert_eq!(world, pending);
    world.complete_build_commit(confirmed(&effect)).unwrap();
    let committed = world.clone();
    let mut conflicting = confirmed(&effect);
    conflicting.durable_receipt += 1;
    assert_eq!(
        world.complete_build_commit(conflicting),
        Err(BuildError::InvalidConfirmation)
    );
    assert_eq!(world, committed);
}

#[test]
fn rejected_durable_transaction_releases_locks_and_never_places_or_debits() {
    let (mut world, authority) = setup();
    let p = preview(&world, purchase(&world, 1, 2), &authority);
    let effect = begin(&mut world, p, &authority);
    let confirmation = ServerBuildConfirmation {
        charged_cost: 0,
        durable_receipt: 0,
        status: DurableBuildStatus::Rejected {
            reason: BuildError::InsufficientFunds,
        },
        created_objects: BTreeMap::new(),
        ..confirmed(&effect)
    };
    let result = world.complete_build_commit(confirmation.clone()).unwrap();
    assert_eq!(result.cost, 0);
    assert!(matches!(
        result.status,
        BuildCommitStatus::Rejected {
            reason: BuildError::InsufficientFunds
        }
    ));
    assert!(world.object(entity(2)).is_none());
    assert!(world.builds.pending_effect().is_none());
    assert_eq!(world.complete_build_commit(confirmation).unwrap(), result);
    assert_eq!(authority.balance, 1_000);
    world
        .move_object(entity(1), TilePos::new(4, 4, 1).center(), Facing::NORTH)
        .unwrap();
}

#[test]
fn durable_commit_after_out_of_band_architecture_change_requires_explicit_reconciliation() {
    let (mut world, authority) = setup();
    let p = preview(&world, purchase(&world, 1, 2), &authority);
    let effect = begin(&mut world, p, &authority);
    // Trusted raw architecture imports can bypass the build lock; a receipt must not silently overwrite them.
    world.lot.set_floor(TilePos::new(7, 7, 1), 99).unwrap();
    let result = world.complete_build_commit(confirmed(&effect)).unwrap();
    assert!(matches!(
        result.status,
        BuildCommitStatus::NeedsReconciliation {
            reason: BuildError::StalePreview,
            ..
        }
    ));
    assert_eq!(result.cost, 100);
    assert!(world.object(entity(2)).is_none());
    assert_eq!(world.lot.tile(TilePos::new(7, 7, 1)).unwrap().floor, 99);
    world.validate().unwrap();
}

#[test]
fn versioned_architecture_undo_is_another_durable_transaction() {
    let (mut world, mut authority) = setup();
    let tile = TilePos::new(4, 4, 1);
    let intent = BuildIntent::new(
        1,
        entity(1),
        world.lot.revision().architecture,
        vec![BuildEdit::SetFloor { tile, pattern: 20 }],
    );
    let p = preview(&world, intent, &authority);
    assert_eq!(p.cost, 5);
    let effect = begin(&mut world, p, &authority);
    world.complete_build_commit(confirmed(&effect)).unwrap();
    assert_eq!(world.lot.tile(tile).unwrap().floor, 20);
    authority.undo_policy = UndoPolicy::Compensating {
        version: 2,
        max_records: 8,
    };
    assert_eq!(
        world
            .builds
            .preview_undo(&world, 1, 2, &authority, &mut |_c: &IntersectionCall| false),
        Err(BuildError::UndoVersion)
    );
    authority.undo_policy = UndoPolicy::Compensating {
        version: 1,
        max_records: 8,
    };
    let undo = world
        .builds
        .preview_undo(&world, 1, 2, &authority, &mut |_c: &IntersectionCall| false)
        .unwrap();
    let effect = begin(&mut world, undo, &authority);
    assert_eq!(world.lot.tile(tile).unwrap().floor, 20);
    world.complete_build_commit(confirmed(&effect)).unwrap();
    assert_eq!(world.lot.tile(tile).unwrap().floor, 0);
    assert_eq!(authority.balance, 1_000);
    assert_eq!(
        world
            .builds
            .preview_undo(&world, 1, 3, &authority, &mut |_c: &IntersectionCall| false),
        Err(BuildError::NoUndo)
    );
    world.validate().unwrap();
}

#[test]
fn ownership_and_live_object_revisions_gate_moves_and_deletion() {
    let (mut world, authority) = setup();
    let mut item = WorldObject::new(entity(2), TilePos::new(4, 4, 1).center());
    item.owner = Some(PersistentId(99));
    world.insert_object(item).unwrap();
    let item = world.object(entity(2)).unwrap().clone();
    let intent = BuildIntent::new(
        1,
        entity(1),
        world.lot.revision().architecture,
        vec![BuildEdit::DeleteObject {
            entity: item.entity,
            expected_revision: item.revision,
        }],
    );
    assert_eq!(
        world.preview_build(intent, &authority, &mut |_c: &IntersectionCall| false),
        Err(BuildError::Placement(PlacementError::ObjectNotOwnedByYou))
    );
    let intent = BuildIntent::new(
        2,
        entity(1),
        world.lot.revision().architecture,
        vec![BuildEdit::MoveObject {
            entity: EntityRef {
                generation: 2,
                ..item.entity
            },
            expected_revision: item.revision,
            position: TilePos::new(5, 5, 1).center(),
            facing: Facing::NORTH,
        }],
    );
    assert_eq!(
        world.preview_build(intent, &authority, &mut |_c: &IntersectionCall| false),
        Err(BuildError::StaleObject)
    );
}

#[test]
fn build_preview_records_both_intersection_decisions_and_commits_without_rerunning_scripts() {
    let (mut world, authority) = setup();
    let mut table = WorldObject::new(entity(3), TilePos::new(4, 4, 1).center());
    table.entrypoints.insert(5);
    world.insert_object(table).unwrap();
    let mut intent = purchase(&world, 1, 2);
    if let BuildEdit::PlaceObject { object, .. } = &mut intent.edits[0] {
        object.entrypoints.insert(5);
    }
    let mut calls = Vec::new();
    let p = world
        .preview_build(intent, &authority, &mut |call: &IntersectionCall| {
            calls.push((call.caller.object_id.0, call.other.object_id.0));
            true
        })
        .unwrap();
    assert_eq!(calls, vec![(2, 3), (3, 2)]);
    let effect = begin(&mut world, p, &authority);
    let result = world.complete_build_commit(confirmed(&effect)).unwrap();
    assert!(matches!(result.status, BuildCommitStatus::Committed { .. }));
    assert_eq!(calls.len(), 2);
}
