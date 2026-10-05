use sim_core::{
    ids::{EntityRef, ObjectId},
    rng::SimRng,
    world::{
        placement::{flags, PlacementContinuationError},
        slots::{score_slots, SlotGoalValidity, SlotSearch},
        *,
    },
};

fn entity(id: i16, generation: u32) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation,
    }
}
fn object(id: i16, tile: TilePos) -> WorldObject {
    WorldObject::new(entity(id, 1), tile.center())
}
fn check(world: &WorldState, object: WorldObject) -> PlacementResult {
    validate_placement(
        PlacementRequest::new(object.clone(), object.position, object.facing),
        world,
        &mut |_call: &IntersectionCall| false,
    )
    .unwrap()
}

#[test]
fn source_failure_codes_are_stable_and_unknown_values_remain_unknown() {
    for code in -1..=50 {
        assert_eq!(PlacementError::from_code(code).unwrap().code(), code);
    }
    assert_eq!(PlacementError::CantSupportWeight.code(), 13);
    assert_eq!(PlacementError::MustBeOnDiagonal.code(), 22);
    assert_eq!(PlacementError::CannotPlaceComputerOnEndTable.code(), 44);
    assert_eq!(PlacementError::from_code(51), None);
}

#[test]
fn source_allow_intersection_signed_height_and_avatar_flag_precedence_are_preserved() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    let p = TilePos::new(2, 2, 1);
    let mut proposed = object(7, p);
    proposed.rules.allowed_heights = 8;
    let mut request = PlacementRequest::new(proposed.clone(), p.center(), Facing::NORTH);
    request.flags.allow_intersection = true;
    assert!(
        validate_placement(request, &world, &mut |_: &IntersectionCall| false)
            .unwrap()
            .is_success()
    );

    world.insert_object(object(3, p)).unwrap();
    world
        .define_slot(SlotDefinition {
            key: SlotKey {
                owner: entity(3, 1),
                index: 0,
            },
            capacity: 1,
            height: 4,
            support_strength: 100,
            max_size: 100,
            revision: 0,
        })
        .unwrap();
    proposed.rules.allowed_heights = u16::MAX;
    let mut request = PlacementRequest::new(proposed.clone(), p.center(), Facing::NORTH);
    request.flags.accept_slots = true;
    assert_eq!(
        validate_placement(request, &world, &mut |_: &IntersectionCall| false)
            .unwrap()
            .status,
        PlacementError::CantIntersectOtherObjects
    );

    let mut other = world.object(entity(3, 1)).unwrap().clone();
    other.rules.is_avatar = true;
    world.replace_object(other).unwrap();
    proposed.rules.is_avatar = true;
    proposed.rules.allow_person_intersection = true;
    proposed.rules.disallow_person_intersection = true;
    assert!(check(&world, proposed.clone()).is_success());
    let mut request = PlacementRequest::new(proposed, p.center(), Facing::NORTH);
    request.flags.all_avatars_solid = true;
    assert_eq!(
        validate_placement(request, &world, &mut |_: &IntersectionCall| false)
            .unwrap()
            .status,
        PlacementError::CantIntersectOtherObjects
    );
}

#[test]
fn diagonal_wall_attachment_uses_source_cardinal_only_fallback() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    let p = TilePos::new(2, 2, 1);
    world
        .lot
        .set_wall(p, WallTile::solid(Cardinal::North.wall_bit()))
        .unwrap();
    let mut o = object(7, p);
    o.rules.wall_flags = 1;
    for direction in [0, 1, 3, 5, 7] {
        o.facing = Facing(direction);
        assert!(
            check(&world, o.clone()).is_success(),
            "direction {direction}"
        );
    }
    o.facing = Facing::EAST;
    assert_eq!(check(&world, o).status, PlacementError::MustBeAgainstWall);
}

#[test]
fn both_intersection_scripts_run_in_object_id_order_even_if_first_allows() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    let p = TilePos::new(2, 2, 1);
    for id in [9, 3] {
        let mut o = object(id, p);
        o.entrypoints.insert(5);
        world.insert_object(o).unwrap();
    }
    let mut proposed = object(7, p);
    proposed.entrypoints.insert(5);
    proposed.rules.ghost = true;
    let mut calls = Vec::new();
    let result = validate_placement(
        PlacementRequest::new(proposed, p.center(), Facing::EAST),
        &world,
        &mut |call: &IntersectionCall| {
            calls.push(call.clone());
            call.caller.object_id == ObjectId(7)
        },
    )
    .unwrap();
    assert!(result.is_success());
    let order: Vec<_> = calls
        .iter()
        .map(|c| (c.caller.object_id.0, c.other.object_id.0))
        .collect();
    assert_eq!(order, vec![(7, 3), (3, 7), (7, 9), (9, 7)]);
    assert!(calls
        .iter()
        .all(|c| c.ghost && c.proposed_facing == Facing::EAST));
}

#[test]
fn placement_continuation_roundtrip_and_revision_guard_preserve_script_cursor() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    let p = TilePos::new(2, 2, 1);
    let mut other = object(3, p);
    other.entrypoints.insert(5);
    world.insert_object(other).unwrap();
    let mut proposed = object(7, p);
    proposed.entrypoints.insert(5);
    let mut c = PlacementContinuation::new(
        PlacementRequest::new(proposed, p.center(), Facing::NORTH),
        &world,
    )
    .unwrap();
    let call = match c.step(&world) {
        PlacementStep::Script(call) => call,
        _ => panic!("expected script"),
    };
    c.complete_callback(call.token, true).unwrap();
    let bytes = bincode::serialize(&c).unwrap();
    let mut restored: PlacementContinuation = bincode::deserialize(&bytes).unwrap();
    restored.validate().unwrap();
    assert_eq!(c.step(&world), restored.step(&world));
    let call = restored.pending_callback().unwrap().clone();
    assert_eq!(call.caller, entity(3, 1));
    assert_eq!(
        restored.complete_callback(call.token + 1, false),
        Err(PlacementContinuationError::CallbackMismatch)
    );
    restored.complete_callback(call.token, false).unwrap();
    assert!(matches!(restored.step(&world),PlacementStep::Complete(ref r)if r.is_success()));
    world
        .move_object(entity(3, 1), TilePos::new(3, 3, 1).center(), Facing::NORTH)
        .unwrap();
    assert_eq!(c.step(&world), PlacementStep::Stale);
}

#[test]
fn placement_checks_floor_water_slope_support_and_build_limits() {
    let mut world = WorldState::new(LotModel::new(6, 6, 2).unwrap());
    let p = TilePos::new(2, 2, 1);
    let base = object(1, p);
    assert!(check(&world, base.clone()).is_success());
    world.lot.set_floor(p, tiles::FLOOR_POOL).unwrap();
    assert_eq!(
        check(&world, base.clone()).status,
        PlacementError::CantPlaceOnWater
    );
    let mut pool = base.clone();
    pool.rules.flags |= flags::REQUIRE_POOL;
    assert!(check(&world, pool.clone()).is_success());
    world.lot.set_floor(p, 0).unwrap();
    assert_eq!(check(&world, pool).status, PlacementError::MustPlaceOnPool);
    let mut water = base.clone();
    water.rules.flags |= flags::REQUIRE_WATER;
    assert_eq!(
        check(&world, water).status,
        PlacementError::MustPlaceOnWater
    );
    world.lot.set_terrain_vertex(2, 2, 4).unwrap();
    assert_eq!(
        check(&world, base.clone()).status,
        PlacementError::CantPlaceOnSlope
    );
    let mut slopes = base.clone();
    slopes.rules.flags |= flags::ON_SLOPE;
    assert!(check(&world, slopes).is_success());
    let upstairs = object(1, TilePos::new(3, 3, 2));
    assert_eq!(
        check(&world, upstairs.clone()).status,
        PlacementError::CantPlaceInAir
    );
    world
        .lot
        .set_object_support(TilePos::new(3, 3, 1), true)
        .unwrap();
    world.lot.set_floor(TilePos::new(3, 3, 2), 1).unwrap();
    assert!(check(&world, upstairs).is_success());
    let mut request = PlacementRequest::new(base, TilePos::new(0, 0, 1).center(), Facing::NORTH);
    request.flags.user_buildable_limit = true;
    world
        .lot
        .set_build_bounds(
            BuildBounds {
                min_x: 1,
                min_y: 1,
                max_x: 5,
                max_y: 5,
                max_level: 2,
            },
            None,
        )
        .unwrap();
    assert_eq!(
        validate_placement(request, &world, &mut |_c: &IntersectionCall| false)
            .unwrap()
            .status,
        PlacementError::LocationOutOfBounds
    );
}

#[test]
fn wall_diagonal_and_exclusive_attachment_errors_have_source_codes() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    let p = TilePos::new(2, 2, 1);
    let mut o = object(1, p);
    o.rules.wall_flags = 1;
    assert_eq!(
        check(&world, o.clone()).status,
        PlacementError::MustBeAgainstWall
    );
    world
        .lot
        .set_wall(p, WallTile::solid(Cardinal::North.wall_bit()))
        .unwrap();
    assert!(check(&world, o.clone()).is_success());
    let mut wall = world.lot.tile(p).unwrap().wall.clone();
    wall.occupied = Cardinal::North.wall_bit();
    world.lot.set_wall(p, wall).unwrap();
    o.rules.exclusive_wall = true;
    assert_eq!(
        check(&world, o.clone()).status,
        PlacementError::MustBeAgainstUnusedWall
    );
    o.rules.wall_flags = flags::DIAGONAL_REQUIRED | flags::DIAGONAL_ALLOWED;
    assert_eq!(
        check(&world, o.clone()).status,
        PlacementError::MustBeOnDiagonal
    );
    let wall = WallTile {
        diagonal: Diagonal::Vertical,
        diagonal_solid: true,
        ..WallTile::default()
    };
    world.lot.set_wall(p, wall).unwrap();
    assert!(check(&world, o).is_success());
    assert_eq!(
        check(&world, object(2, p)).status,
        PlacementError::CantBeThroughWall
    );
    let mut crossing = object(2, p);
    crossing.position = LotPosition::new(32, 40, 1);
    world
        .lot
        .set_wall(p, WallTile::solid(Cardinal::West.wall_bit()))
        .unwrap();
    assert_eq!(
        check(&world, crossing).status,
        PlacementError::CantBeThroughWall
    );
}

#[test]
fn surface_weight_is_strict_height_mapping_and_slot_capacity_are_observable() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    let p = TilePos::new(2, 2, 1);
    world.insert_object(object(2, p)).unwrap();
    let key = SlotKey {
        owner: entity(2, 1),
        index: 0,
    };
    world
        .define_slot(SlotDefinition {
            key,
            capacity: 1,
            height: 4,
            support_strength: 10,
            max_size: 16,
            revision: 0,
        })
        .unwrap();
    let mut item = object(3, p);
    item.rules.allowed_heights = 1 << 3;
    item.rules.weight = 10;
    let mut request = PlacementRequest::new(item.clone(), p.center(), Facing::NORTH);
    request.flags.accept_slots = true;
    assert_eq!(
        validate_placement(request.clone(), &world, &mut |_c: &IntersectionCall| false)
            .unwrap()
            .status,
        PlacementError::CantSupportWeight
    );
    request.object.rules.weight = 9;
    let result =
        validate_placement(request.clone(), &world, &mut |_c: &IntersectionCall| false).unwrap();
    assert!(result.is_success());
    assert_eq!(result.support_slot, Some(key));
    request.object.rules.allowed_heights = 1 << 7;
    assert_eq!(
        validate_placement(request.clone(), &world, &mut |_c: &IntersectionCall| false)
            .unwrap()
            .status,
        PlacementError::HeightNotAllowed
    );
    request.flags.accept_slots = false;
    assert_eq!(
        validate_placement(request, &world, &mut |_c: &IntersectionCall| false)
            .unwrap()
            .status,
        PlacementError::CantIntersectOtherObjects
    );
}

#[test]
fn reservation_tokens_are_capacity_generation_and_expiry_aware() {
    let mut world = WorldState::new(LotModel::new(5, 5, 1).unwrap());
    for id in 1..=3 {
        world
            .insert_object(object(id, TilePos::new(id, 1, 1)))
            .unwrap();
    }
    let key = SlotKey {
        owner: entity(1, 1),
        index: 0,
    };
    world
        .define_slot(SlotDefinition {
            key,
            capacity: 1,
            height: 1,
            support_strength: 10,
            max_size: 16,
            revision: 0,
        })
        .unwrap();
    let revision = world
        .slots
        .get(key)
        .unwrap()
        .definition
        .as_ref()
        .unwrap()
        .revision;
    let request = ReservationRequest {
        slot: key,
        actor: entity(2, 1),
        operation: 10,
        expected_revision: revision,
        tick: 5,
        duration_ticks: 3,
    };
    let token = world.reserve_slot(request.clone()).unwrap();
    assert_eq!(world.reserve_slot(request.clone()).unwrap(), token);
    let other = ReservationRequest {
        actor: entity(3, 1),
        operation: 11,
        ..request.clone()
    };
    assert_eq!(world.reserve_slot(other.clone()), Err(SlotError::Full));
    assert_eq!(world.occupy_slot(token, 8), Err(SlotError::Expired));
    world.expire_slots(8).unwrap();
    let next = world
        .reserve_slot(ReservationRequest { tick: 8, ..other })
        .unwrap();
    world.occupy_slot(next, 9).unwrap();
    assert!(!world.occupy_slot(next, 9).unwrap());
    let encoded = bincode::serialize(&world).unwrap();
    let restored: WorldState = bincode::deserialize(&encoded).unwrap();
    assert_eq!(restored, world);
    restored.validate().unwrap();
    world.remove_object(entity(3, 1)).unwrap();
    let mut replacement = object(3, TilePos::new(3, 1, 1));
    replacement.entity.generation = 2;
    world.insert_object(replacement).unwrap();
    assert!(!world.slots.release(next).unwrap());
    assert_eq!(world.occupy_slot(next, 9), Err(SlotError::MissingActor));
    world.validate().unwrap();
}

#[test]
fn slot_scoring_consumes_rng_before_validation_and_equal_scores_preserve_enumeration() {
    let lot = LotModel::new(8, 8, 1).unwrap();
    let target = TilePos::new(3, 3, 1).center();
    let caller = target;
    let mut rng = SimRng::new(123);
    let mut expected = rng.clone();
    for _ in 0..8 {
        expected.next(1024);
    }
    let search = SlotSearch::default();
    let result = score_slots(
        &search,
        target,
        Facing::NORTH,
        caller,
        &lot,
        &mut rng,
        |_| SlotGoalValidity::Blocked {
            code: RouteFailCode::DestTileOccupied,
            blocker: None,
        },
    )
    .unwrap();
    assert_eq!(result.scoring_draws, 8);
    assert_eq!(rng.state(), expected.state());
    assert!(result.choices.is_empty());
    let mut equal = search;
    equal.equal_proximity_score = true;
    equal.random_scoring = true;
    let before = rng.state();
    let result = score_slots(
        &equal,
        target,
        Facing::NORTH,
        caller,
        &lot,
        &mut rng,
        |_| SlotGoalValidity::Standing { congested: false },
    )
    .unwrap();
    assert_eq!(rng.state(), before);
    assert_eq!(result.scoring_draws, 0);
    assert_eq!(
        result
            .choices
            .iter()
            .map(|g| g.enumeration_index)
            .collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );
}

#[test]
fn source_slot_search_sectors_overlap_and_quantize_positions_before_angles() {
    use sim_core::world::slots::slot_search_directions;
    let center = LotPosition::new(40, 40, 1);
    assert_eq!(
        slot_search_directions(center, LotPosition::new(56, 8, 1), Facing::NORTH),
        1 | 2
    );
    assert_eq!(
        slot_search_directions(center, LotPosition::new(56, 24, 1), Facing::NORTH),
        2
    );
    assert_eq!(
        slot_search_directions(center, LotPosition::new(41, 41, 1), Facing::NORTH),
        1
    );
    assert_eq!(
        slot_search_directions(center, LotPosition::new(41, 41, 1), Facing::EAST),
        1 << 6
    );
    let lot = LotModel::new(8, 8, 1).unwrap();
    let mut rng = SimRng::new(100);
    let invalid = SlotSearch {
        offset_x: i32::MIN,
        ..SlotSearch::default()
    };
    assert!(score_slots(
        &invalid,
        center,
        Facing::NORTH,
        center,
        &lot,
        &mut rng,
        |_| SlotGoalValidity::Standing { congested: false }
    )
    .is_err());
}
