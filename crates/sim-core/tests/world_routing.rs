use sim_core::{
    ids::{EntityRef, ObjectId},
    rng::SimRng,
    world::{
        routing::{
            segment_geometry_clear, CallbackOutcome, ReplanReason, RouteCallbackKind, RouteConfig,
            RouteError,
        },
        *,
    },
};

fn entity(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}
fn actor(id: i16, p: TilePos) -> WorldObject {
    let mut object = WorldObject::new(entity(id), p.center());
    object.rules.is_avatar = true;
    object.motion = ObstacleMotion::MovingAvatar;
    object
}
fn world() -> WorldState {
    let mut world = WorldState::new(LotModel::new(8, 8, 1).unwrap());
    world
        .insert_object(actor(1, TilePos::new(1, 1, 1)))
        .unwrap();
    world
}
fn route(world: &WorldState, id: u64, goal: LotPosition) -> RouteContinuation {
    RouteContinuation::new(
        RouteRequest::new(
            id,
            entity(1),
            world.object(entity(1)).unwrap().position,
            vec![RouteGoal::point(goal)],
        ),
        world,
    )
    .unwrap()
}

#[test]
fn route_admission_rejects_missing_seat_and_callback_admission_is_atomic() {
    let mut world = portal_world();
    let mut request = RouteRequest::new(
        90,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![RouteGoal::point(TilePos::new(6, 2, 2).center())],
    );
    request.seated_on = Some(entity(999));
    assert_eq!(
        RouteContinuation::new(request, &world),
        Err(RouteError::MissingTarget)
    );

    let mut route = route(&world, 91, TilePos::new(6, 2, 2).center());
    let mut rng = SimRng::new(10);
    let call = match finish(&mut world, &mut route, &mut rng) {
        RouteStep::Script(call) => call,
        other => panic!("{other:?}"),
    };
    for outcome in [
        CallbackOutcome::failure(Some(EntityRef {
            object_id: ObjectId(0),
            generation: 0,
        })),
        CallbackOutcome {
            success: true,
            position: Some(TilePos::new(99, 99, 1).center()),
            blocker: None,
        },
    ] {
        let before = route.clone();
        assert_eq!(
            route.complete_callback_for(call.route_id, call.token, outcome),
            Err(RouteError::InvalidRequest)
        );
        assert_eq!(route, before);
        route.validate_against(&world).unwrap();
    }
}

#[test]
fn pending_portal_callback_rejects_changed_geometry_revision_or_direction() {
    for change in 0..3 {
        let mut world = portal_world();
        let mut route = route(&world, 92, TilePos::new(6, 2, 2).center());
        let mut rng = SimRng::new(10);
        let call = match finish(&mut world, &mut route, &mut rng) {
            RouteStep::Script(call) => call,
            other => panic!("{other:?}"),
        };
        let traversal = match call.kind {
            RouteCallbackKind::Portal { traversal, .. } => traversal,
            _ => panic!(),
        };
        let mut portal = world.lot.portals()[&traversal.id].clone();
        match change {
            0 => portal.entry = portal.entry.offset(16, 0, 0).unwrap(),
            1 => portal.cost += 1,
            _ => portal.bidirectional = false,
        }
        world.lot.upsert_portal(portal).unwrap();
        route
            .complete_callback(call.token, CallbackOutcome::success())
            .unwrap();
        assert_eq!(
            dispatch(&mut world, &mut route, &mut rng, 128),
            RouteStep::Replan {
                reason: ReplanReason::PortalFailed
            }
        );
        assert_eq!(world.object(entity(1)).unwrap().position.level, 1);
        route.validate_against(&world).unwrap();
    }
}

#[test]
fn moving_an_active_actor_out_of_world_finishes_without_an_invalid_replan() {
    let mut world = world();
    let mut route = route(&world, 93, TilePos::new(6, 6, 1).center());
    let mut rng = SimRng::new(10);
    dispatch(&mut world, &mut route, &mut rng, 0);
    world
        .move_object(entity(1), LotPosition::OUT_OF_WORLD, Facing::NORTH)
        .unwrap();
    assert_eq!(
        route.step(&world, &mut rng, 128),
        RouteStep::Failed {
            code: RouteFailCode::Interrupted,
            blocker: None
        }
    );
    route.validate_against(&world).unwrap();
}

#[test]
fn later_failure_yields_its_failure_tree_before_terminal_result_and_survives_restore() {
    for (seated, complete) in [(true, false), (true, true), (false, false), (false, true)] {
        let mut world = world();
        let (code, blocker) = if seated {
            let chair = WorldObject::new(entity(2), TilePos::new(1, 1, 1).center());
            world.insert_object(chair).unwrap();
            (RouteFailCode::CantStand, Some(entity(2)))
        } else {
            (RouteFailCode::NoPath, None)
        };
        let mut request = RouteRequest::new(
            94,
            entity(1),
            world.object(entity(1)).unwrap().position,
            vec![RouteGoal::point(TilePos::new(6, 6, 1).center())],
        );
        request.seated_on = blocker;
        request.config.call_failure_tree = true;
        request.config.wait_timeout = 2;
        request.config.collision_wait = 1;
        request.config.shoo_wait = 1;
        if !seated {
            request.config.max_dispatches = 1;
        }
        let mut route = RouteContinuation::new(request, &world).unwrap();
        let mut rng = SimRng::new(91);
        if !seated {
            assert!(matches!(
                route.step(&world, &mut rng, 0),
                RouteStep::Searching { .. }
            ));
        }
        let call = match route.step(&world, &mut rng, 128) {
            RouteStep::Script(call) => call,
            other => panic!("route failure must yield its failure tree first: {other:?}"),
        };
        assert_eq!(call.kind, RouteCallbackKind::Failure { code, blocker });
        assert_eq!(call.entrypoint, 398);
        assert_eq!(call.target, entity(1));
        if seated {
            world.remove_object(entity(2)).unwrap();
        }
        route = bincode::deserialize(&bincode::serialize(&route).unwrap()).unwrap();
        route.validate_against(&world).unwrap();
        if complete {
            route
                .complete_callback_for(call.route_id, call.token, CallbackOutcome::success())
                .unwrap();
        } else {
            assert!(matches!(
                route.step(&world, &mut rng, 128),
                RouteStep::Wait { .. }
            ));
        }
        assert_eq!(
            route.step(&world, &mut rng, 128),
            RouteStep::Failed { code, blocker }
        );
        route.validate_against(&world).unwrap();
    }
}

fn dispatch(
    world: &mut WorldState,
    route: &mut RouteContinuation,
    rng: &mut SimRng,
    budget: u32,
) -> RouteStep {
    let step = route.step(world, rng, budget);
    if let RouteStep::Progress { from, to, facing } = &step {
        assert_eq!(world.object(entity(1)).unwrap().position, *from);
        world.move_object(entity(1), *to, *facing).unwrap();
    }
    step
}
fn finish(world: &mut WorldState, route: &mut RouteContinuation, rng: &mut SimRng) -> RouteStep {
    for _ in 0..4_000 {
        let step = dispatch(world, route, rng, 128);
        if matches!(
            step,
            RouteStep::Arrived { .. } | RouteStep::Failed { .. } | RouteStep::Script(_)
        ) {
            return step;
        }
    }
    panic!("route exceeded fixture dispatch bound")
}

#[test]
fn source_route_failure_codes_and_dispatch_rng_cycle_are_preserved() {
    for code in 0..=13 {
        assert_eq!(RouteFailCode::from_code(code).unwrap().code(), code);
    }
    assert_eq!(RouteFailCode::from_code(-1), None);
    let mut world = world();
    let mut route = route(&world, 1, TilePos::new(6, 6, 1).center());
    let mut rng = SimRng::new(1234);
    let mut expected = rng.clone();
    for _ in 0..9 {
        expected.next(1);
        assert!(matches!(
            dispatch(&mut world, &mut route, &mut rng, 0),
            RouteStep::Searching { expanded: 0, .. }
        ));
        assert_eq!(rng.state(), expected.state());
    }
    route.validate_against(&world).unwrap();
}

#[test]
fn suspended_search_and_walk_roundtrip_replay_matches_every_subsequent_dispatch() {
    let mut a = world();
    let mut ra = route(&a, 1, TilePos::new(6, 5, 1).center());
    let mut rnga = SimRng::new(777);
    for _ in 0..7 {
        dispatch(&mut a, &mut ra, &mut rnga, 2);
    }
    let bytes = bincode::serialize(&(a.clone(), ra.clone(), rnga.clone())).unwrap();
    let (mut b, mut rb, mut rngb): (WorldState, RouteContinuation, SimRng) =
        bincode::deserialize(&bytes).unwrap();
    rb.validate_against(&b).unwrap();
    let mut reached = false;
    for _ in 0..2_000 {
        let sa = dispatch(&mut a, &mut ra, &mut rnga, 4);
        let sb = dispatch(&mut b, &mut rb, &mut rngb, 4);
        assert_eq!(sa, sb);
        assert_eq!((&a, &ra, &rnga), (&b, &rb, &rngb));
        if matches!(sa, RouteStep::Arrived { .. }) {
            reached = true;
            break;
        }
        assert!(!matches!(sa, RouteStep::Failed { .. }));
    }
    assert!(reached);
    assert_eq!(
        a.object(entity(1)).unwrap().position,
        TilePos::new(6, 5, 1).center()
    );
}

#[test]
fn diagonal_barriers_prevent_corner_cutting_but_route_around_their_end() {
    let mut world = world();
    let tile = TilePos::new(2, 2, 1);
    let wall = WallTile {
        diagonal: Diagonal::Vertical,
        diagonal_solid: true,
        ..WallTile::default()
    };
    world.lot.set_wall(tile, wall).unwrap();
    let mut object = world.object(entity(1)).unwrap().clone();
    object.footprint = Footprint::rectangle(-1, -1, 1, 1);
    object.position = LotPosition::new(44, 36, 1);
    world.replace_object(object).unwrap();
    let destination = LotPosition::new(36, 44, 1);
    assert!(!segment_geometry_clear(
        world.object(entity(1)).unwrap(),
        LotPosition::new(40, 36, 1),
        LotPosition::new(36, 40, 1),
        &world
    ));
    let mut route = route(&world, 2, destination);
    let mut rng = SimRng::new(23);
    let mut walked = 0;
    for _ in 0..1_000 {
        let step = route.step(&world, &mut rng, 64);
        match step {
            RouteStep::Progress { from, to, facing } => {
                assert!(segment_geometry_clear(
                    world.object(entity(1)).unwrap(),
                    from,
                    to,
                    &world
                ));
                world.move_object(entity(1), to, facing).unwrap();
                walked += 1;
            }
            RouteStep::Arrived { position, .. } => {
                assert_eq!(position, destination);
                assert!(walked > 4);
                return;
            }
            RouteStep::Failed { code, .. } => panic!("failed {code:?}"),
            _ => {}
        }
    }
    panic!("no route")
}

fn portal_world() -> WorldState {
    let mut world = WorldState::new(LotModel::new(8, 6, 2).unwrap());
    for y in 0..6 {
        for x in 0..8 {
            world
                .lot
                .set_object_support(TilePos::new(x, y, 1), true)
                .unwrap();
            world.lot.set_floor(TilePos::new(x, y, 2), 1).unwrap();
        }
    }
    world
        .insert_object(actor(1, TilePos::new(1, 2, 1)))
        .unwrap();
    for (id, y) in [(10, 2), (11, 4)] {
        let mut object = WorldObject::new(entity(id), TilePos::new(3, y, 1).center());
        object.rules.zero_extent = true;
        object.entrypoints.insert(15);
        world.insert_object(object).unwrap();
        world
            .lot
            .upsert_portal(Portal {
                id: PortalId((id - 9) as u32),
                entity: entity(id),
                entry: TilePos::new(3, y, 1).center(),
                exit: TilePos::new(3, y, 2).center(),
                bidirectional: true,
                enabled: true,
                cost: 16,
                revision: 0,
            })
            .unwrap();
    }
    world
}

#[test]
fn failed_first_portal_uses_alternate_stairs_and_mid_portal_snapshot_resumes_identically() {
    let mut world = portal_world();
    let goal = TilePos::new(6, 2, 2).center();
    let mut route = route(&world, 10, goal);
    let mut rng = SimRng::new(7654);
    let first = match finish(&mut world, &mut route, &mut rng) {
        RouteStep::Script(call) => call,
        other => panic!("{other:?}"),
    };
    assert_eq!(first.target, entity(10));
    route
        .complete_callback(first.token, CallbackOutcome::failure(Some(first.target)))
        .unwrap();
    assert_eq!(
        dispatch(&mut world, &mut route, &mut rng, 128),
        RouteStep::Replan {
            reason: ReplanReason::PortalFailed
        }
    );
    let second = match finish(&mut world, &mut route, &mut rng) {
        RouteStep::Script(call) => call,
        other => panic!("{other:?}"),
    };
    assert_eq!(second.target, entity(11));
    assert!(matches!(second.kind, RouteCallbackKind::Portal { .. }));
    let bytes = bincode::serialize(&(world.clone(), route.clone(), rng.clone())).unwrap();
    let (mut restored, mut resumed, mut rng2): (WorldState, RouteContinuation, SimRng) =
        bincode::deserialize(&bytes).unwrap();
    resumed.validate_against(&restored).unwrap();
    route
        .complete_callback(second.token, CallbackOutcome::success())
        .unwrap();
    resumed
        .complete_callback(second.token, CallbackOutcome::success())
        .unwrap();
    for _ in 0..1_000 {
        let a = dispatch(&mut world, &mut route, &mut rng, 128);
        let b = dispatch(&mut restored, &mut resumed, &mut rng2, 128);
        assert_eq!(a, b);
        assert_eq!((&world, &route, &rng), (&restored, &resumed, &rng2));
        if let RouteStep::Arrived { position, .. } = a {
            assert_eq!(position, goal);
            return;
        }
        assert!(!matches!(
            a,
            RouteStep::Failed { .. } | RouteStep::Script(_)
        ));
    }
    panic!("route did not complete")
}

#[test]
fn nested_route_keeps_parent_portal_callback_and_only_dispatches_active_rng() {
    let mut world = portal_world();
    let mut parent = route(&world, 20, TilePos::new(6, 2, 2).center());
    let mut rng = SimRng::new(321);
    let call = match finish(&mut world, &mut parent, &mut rng) {
        RouteStep::Script(call) => call,
        other => panic!("{other:?}"),
    };
    let start = parent.position();
    let child_goal = start.offset(-8, 0, 0).unwrap();
    parent
        .push_nested(
            RouteRequest::new(21, entity(1), start, vec![RouteGoal::point(child_goal)]),
            &world,
        )
        .unwrap();
    let bytes = bincode::serialize(&parent).unwrap();
    parent = bincode::deserialize(&bytes).unwrap();
    parent.validate_against(&world).unwrap();
    let mut completed = false;
    for _ in 0..1_000 {
        let mut expected = rng.clone();
        expected.next(1);
        let step = dispatch(&mut world, &mut parent, &mut rng, 128);
        assert_eq!(expected.state(), rng.state());
        if let RouteStep::NestedFinished { route_id, result } = step {
            assert_eq!(route_id, 21);
            assert_eq!(result, Ok(child_goal));
            completed = true;
            break;
        }
    }
    assert!(completed);
    assert_eq!(parent.pending_callback().unwrap(), &call);
    parent
        .complete_callback_for(20, call.token, CallbackOutcome::success())
        .unwrap();
    assert!(matches!(
        finish(&mut world, &mut parent, &mut rng),
        RouteStep::Arrived { .. }
    ));
}

#[test]
fn moved_target_replans_relative_goal_and_reused_target_generation_is_rejected() {
    let mut world = world();
    let mut target = WorldObject::new(entity(2), TilePos::new(5, 2, 1).center());
    target.rules.zero_extent = true;
    world.insert_object(target).unwrap();
    let mut request = RouteRequest::new(
        30,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![RouteGoal {
            follows_target: true,
            ..RouteGoal::point(TilePos::new(4, 2, 1).center())
        }],
    );
    request.target = Some(entity(2));
    let mut route = RouteContinuation::new(request, &world).unwrap();
    let mut rng = SimRng::new(43);
    dispatch(&mut world, &mut route, &mut rng, 2);
    world
        .move_object(entity(2), TilePos::new(6, 2, 1).center(), Facing::NORTH)
        .unwrap();
    assert_eq!(
        dispatch(&mut world, &mut route, &mut rng, 2),
        RouteStep::Replan {
            reason: ReplanReason::TargetMoved
        }
    );
    assert!(
        matches!(finish(&mut world,&mut route,&mut rng),RouteStep::Arrived{position,..}if position==TilePos::new(5,2,1).center())
    );
    let mut request = RouteRequest::new(
        31,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![RouteGoal::point(TilePos::new(3, 2, 1).center())],
    );
    request.target = Some(entity(2));
    let mut second = RouteContinuation::new(request, &world).unwrap();
    world.remove_object(entity(2)).unwrap();
    let mut replacement = WorldObject::new(
        EntityRef {
            generation: 2,
            ..entity(2)
        },
        TilePos::new(6, 2, 1).center(),
    );
    replacement.rules.zero_extent = true;
    world.insert_object(replacement).unwrap();
    assert!(matches!(
        dispatch(&mut world, &mut second, &mut rng, 16),
        RouteStep::Failed {
            code: RouteFailCode::NoValidGoals,
            ..
        }
    ));
}

#[test]
fn chair_capacity_sit_failure_alternative_and_stand_failure_are_explicit() {
    let mut world = world();
    let chair_ref = entity(2);
    let chair_position = TilePos::new(4, 4, 1).center();
    let mut chair = WorldObject::new(chair_ref, chair_position);
    chair.entrypoints.extend([26, 27]);
    world.insert_object(chair).unwrap();
    world
        .insert_object(actor(3, TilePos::new(5, 4, 1)))
        .unwrap();
    let slot = SlotKey {
        owner: chair_ref,
        index: 0,
    };
    world
        .define_slot(SlotDefinition {
            key: slot,
            capacity: 1,
            height: 1,
            support_strength: 100,
            max_size: 16,
            revision: 0,
        })
        .unwrap();
    let token = world
        .reserve_slot(ReservationRequest {
            slot,
            actor: entity(3),
            operation: 10,
            expected_revision: world
                .slots
                .get(slot)
                .unwrap()
                .definition
                .as_ref()
                .unwrap()
                .revision,
            tick: 0,
            duration_ticks: 300,
        })
        .unwrap();
    world.occupy_slot(token, 0).unwrap();
    let goal = RouteGoal {
        chair: Some(chair_ref),
        slot: Some(slot),
        ..RouteGoal::point(chair_position)
    };
    let request = RouteRequest::new(
        40,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![goal.clone()],
    );
    let mut first = RouteContinuation::new(request, &world).unwrap();
    let mut rng = SimRng::new(1);
    assert!(matches!(
        first.step(&world, &mut rng, 16),
        RouteStep::Failed {
            code: RouteFailCode::DestChairOccupied,
            ..
        }
    ));
    world.slots.release(token).unwrap();
    let request = RouteRequest::new(
        41,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![goal, RouteGoal::point(TilePos::new(2, 2, 1).center())],
    );
    let mut second = RouteContinuation::new(request, &world).unwrap();
    let call = match second.step(&world, &mut rng, 16) {
        RouteStep::Script(call) => call,
        other => panic!("{other:?}"),
    };
    assert_eq!(call.entrypoint, 26);
    second
        .complete_callback(call.token, CallbackOutcome::failure(Some(chair_ref)))
        .unwrap();
    assert_eq!(
        dispatch(&mut world, &mut second, &mut rng, 16),
        RouteStep::Replan {
            reason: ReplanReason::AlternativeGoal
        }
    );
    assert!(matches!(
        finish(&mut world, &mut second, &mut rng),
        RouteStep::Arrived { goal_index: 1, .. }
    ));
    let mut request = RouteRequest::new(
        42,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![RouteGoal::point(TilePos::new(3, 2, 1).center())],
    );
    request.seated_on = Some(chair_ref);
    let mut third = RouteContinuation::new(request, &world).unwrap();
    let call = match third.step(&world, &mut rng, 16) {
        RouteStep::Script(call) => call,
        other => panic!("{other:?}"),
    };
    assert_eq!(call.entrypoint, 27);
    third
        .complete_callback(call.token, CallbackOutcome::failure(Some(chair_ref)))
        .unwrap();
    assert!(matches!(
        third.step(&world, &mut rng, 16),
        RouteStep::Failed {
            code: RouteFailCode::CantStand,
            ..
        }
    ));
}

#[test]
fn stationary_avatar_blocking_corridor_is_shooed_and_route_retries_after_wait() {
    let mut world = WorldState::new(LotModel::new(5, 3, 1).unwrap());
    for x in 0..5 {
        world
            .lot
            .set_wall(
                TilePos::new(x, 1, 1),
                WallTile::solid(Cardinal::North.wall_bit() | Cardinal::South.wall_bit()),
            )
            .unwrap();
    }
    world
        .insert_object(actor(1, TilePos::new(1, 1, 1)))
        .unwrap();
    let mut blocker = actor(2, TilePos::new(2, 1, 1));
    blocker.motion = ObstacleMotion::StationaryAvatar;
    blocker.footprint = Footprint::rectangle(-8, -8, 8, 8);
    world.insert_object(blocker).unwrap();
    let mut request = RouteRequest::new(
        50,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![RouteGoal::point(TilePos::new(3, 1, 1).center())],
    );
    request.config.shoo_wait = 3;
    let mut route = RouteContinuation::new(request, &world).unwrap();
    let mut rng = SimRng::new(5);
    let call = match finish(&mut world, &mut route, &mut rng) {
        RouteStep::Script(call) => call,
        other => panic!("expected shoo: {other:?}"),
    };
    assert!(matches!(call.kind, RouteCallbackKind::Shoo));
    assert_eq!(call.target, entity(2));
    world
        .move_object(entity(2), LotPosition::OUT_OF_WORLD, Facing::NORTH)
        .unwrap();
    route
        .complete_callback(call.token, CallbackOutcome::success())
        .unwrap();
    assert!(matches!(
        dispatch(&mut world, &mut route, &mut rng, 128),
        RouteStep::Wait { remaining: 3, .. }
    ));
    assert!(matches!(
        finish(&mut world, &mut route, &mut rng),
        RouteStep::Arrived { .. }
    ));
}

#[test]
fn route_timeout_and_out_of_world_actor_do_not_hang_or_teleport() {
    let mut world = world();
    let mut request = RouteRequest::new(
        60,
        entity(1),
        world.object(entity(1)).unwrap().position,
        vec![RouteGoal::point(TilePos::new(5, 5, 1).center())],
    );
    request.config = RouteConfig {
        max_dispatches: 3,
        ..RouteConfig::default()
    };
    let mut route = RouteContinuation::new(request, &world).unwrap();
    let mut rng = SimRng::new(8);
    for _ in 0..3 {
        assert!(matches!(
            route.step(&world, &mut rng, 0),
            RouteStep::Searching { .. }
        ));
    }
    assert!(matches!(
        route.step(&world, &mut rng, 0),
        RouteStep::Failed {
            code: RouteFailCode::NoPath,
            ..
        }
    ));
    world
        .move_object(entity(1), LotPosition::OUT_OF_WORLD, Facing::NORTH)
        .unwrap();
    let invalid = RouteRequest::new(
        61,
        entity(1),
        TilePos::new(1, 1, 1).center(),
        vec![RouteGoal::point(TilePos::new(2, 2, 1).center())],
    );
    assert_eq!(
        RouteContinuation::new(invalid, &world),
        Err(RouteError::InvalidRequest)
    );
}
