use sim_core::{
    avatars::{events::TimeProperty, timeline::AnimationMetadata},
    ids::{EntityRef, ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::*,
    state::*,
    vm::*,
    world::{
        routing::{CallbackOutcome, RouteCallbackKind},
        slots::SlotSearch,
        *,
    },
};

fn entity(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}
fn key(owner: u32, id: u16) -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(owner),
        id,
    }
}
fn op(opcode: u16, operand: [u8; 8]) -> VmInstruction {
    VmInstruction::new(opcode, 254, 255, operand)
}
fn expression(lhs: Variable, rhs: Variable, operator: u8, yes: u8, no: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        yes,
        no,
        ExpressionOperand {
            lhs,
            rhs,
            operator,
            is_signed: 0,
        }
        .encode(),
    )
}
fn content(slots: Vec<((u32, u16), RoutingSlot)>) -> ContentSet {
    let mut routines = RoutineStore::new();
    let operands = [
        (4096, 46, [0, 0, 1, 0, 0, 0, 0, 0]),     // contained
        (4097, 46, [0, 0, 1, 1, 0, 0, 0, 0]),     // mode257, not mode1
        (4098, 46, [7, 0, 3, 0, 0, 0, 0, 0]),     // literal slot7
        (4099, 46, [7, 0, 4, 0, 0, 0, 0, 0]),     // global slot7
        (4100, 46, [0, 0, 0, 0, 0, 0, 0, 0]),     // parameter0
        (4101, 27, [0, 0, 254, 255, 0, 0, 2, 0]), // on top, any facing, no failure tree
        (4102, 45, [7, 0, 1, 0, 0, 0, 0, 0]),     // literal, failure tree enabled
        (4103, 22, [0, 0, 0, 0, 0, 0, 0, 0]),     // head
        (4104, 22, [2, 0, 0, 0, 0, 0, 0, 0]),     // body
        (4105, 16, [0, 0, 0, 0, 0, 0, 0, 0]),     // nearest, defer occupied
        (4106, 16, [0, 0, 2, 0, 0, 0, 0, 0]),     // nearest, immediate
        (4108, 16, [5, 0, 2, 0, 0, 0, 0, 0]),     // random
        (4109, 16, [1, 0, 0, 0, 0, 0, 0, 0]),     // out of world
        (4110, 47, [0, 0, 0, 0, 0, 0, 0, 0]),     // reach stack
        (4111, 47, [1, 0, 0, 0, 0, 0, 0, 0]),     // reach slot parameter0
        (4112, 16, [2, 0, 0, 0, 0, 0, 0, 0]),     // smoke midpoint
        (4113, 16, [1, 0, 1, 0, 0, 0, 0, 0]),     // out-of-world, unused null local ref
        (4114, 22, [4, 0, 0, 0, 0, 0, 0, 0]),     // body towards group average
        (4115, 22, [5, 0, 0, 0, 0, 0, 0, 0]),     // body away from group average
    ];
    for (id, opcode, bytes) in operands {
        routines
            .insert(
                key(100, id),
                VmRoutine::new(id, 1, 4, vec![op(opcode, bytes)]).unwrap(),
            )
            .unwrap();
    }
    routines
        .insert(
            key(100, 4107),
            VmRoutine::new(
                4107,
                1,
                4,
                vec![
                    expression(
                        Variable::new(Scope::Local, 0),
                        Variable::new(Scope::Literal, 3),
                        5,
                        1,
                        255,
                    ),
                    op(16, [3, 0, 1, 0, 0, 0, 0, 0]),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(100, 4116),
            VmRoutine::new(
                4116,
                0,
                4,
                vec![
                    expression(
                        Variable::new(Scope::Temps, 0),
                        Variable::new(Scope::Literal, 314),
                        5,
                        1,
                        255,
                    ),
                    op(45, [7, 0, 1, 0, 0, 0, 0, 0]),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    // Real encoded intersection trees validate bound args before an isolated write.
    for (owner, other) in [(100, 2), (200, 1)] {
        routines
            .insert(
                key(owner, 4160),
                VmRoutine::new(
                    4160,
                    0,
                    4,
                    vec![
                        expression(
                            Variable::new(Scope::Parameters, 0),
                            Variable::new(Scope::Literal, other),
                            2,
                            1,
                            255,
                        ),
                        expression(
                            Variable { scope: 0, data: 0 },
                            Variable::new(Scope::Literal, 1),
                            3,
                            254,
                            255,
                        ),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
    }
    // An invalid expression operator proves the second script's fault is propagated.
    routines
        .insert(
            key(200, 4161),
            VmRoutine::new(
                4161,
                0,
                4,
                vec![expression(
                    Variable { scope: 0, data: 0 },
                    Variable::new(Scope::Literal, 0),
                    253,
                    254,
                    255,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    // Entry five can invoke a placement primitive again. The host must retain
    // the outer query's depth and instruction budget across this call chain.
    routines
        .insert(
            key(200, 4162),
            VmRoutine::new(4162, 0, 4, vec![op(46, [7, 0, 3, 0, 0, 0, 0, 0])]).unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(200, 4300),
            VmRoutine::new(
                4300,
                0,
                4,
                vec![expression(
                    Variable::new(Scope::Literal, 1),
                    Variable::new(Scope::Literal, 1),
                    2,
                    254,
                    255,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    let definitions = [100, 200, 300]
        .into_iter()
        .map(|guid| {
            let mut definition = ObjectDefinition::new(guid, 2);
            definition.slot_count = 2;
            definition.object_data[4] = 1;
            definition.object_data[42] = 3;
            definition
        })
        .collect();
    let animations = [
        "a2o-reach-floorht.anim",
        "a2o-reach-seatht.anim",
        "a2o-reach-tableht.anim",
        "a2o-sit-reach-table.anim",
        "a2o-rarm-carry-loop.anim",
    ]
    .into_iter()
    .enumerate()
    .map(|(id, name)| {
        (
            AnimationKey {
                owner: 100,
                scope: 0,
                id: id as u16 + 1,
            },
            AnimationMetadata {
                resource: name.into(),
                num_frames: 5,
                time_properties: if id == 4 {
                    vec![]
                } else {
                    vec![TimeProperty::xevt(33, 0)]
                },
            },
        )
    })
    .collect();
    ContentSet::new(routines, definitions, animations, TuningSet::default())
        .unwrap()
        .with_routing_slots(slots)
        .unwrap()
}
fn replace_definitions(
    content: ContentSet,
    modify: impl FnOnce(&mut Vec<ObjectDefinition>),
) -> ContentSet {
    let mut definitions = [100, 200, 300]
        .into_iter()
        .map(|id| content.object(id).unwrap().clone())
        .collect();
    modify(&mut definitions);
    // Fixtures which customize definitions do not need normalized routing slots.
    let animations = (1..=5)
        .map(|id| {
            let k = AnimationKey {
                owner: 100,
                scope: 0,
                id,
            };
            (k, content.animation(k).unwrap().clone())
        })
        .collect();
    ContentSet::new(
        content.routines().clone(),
        definitions,
        animations,
        TuningSet::default(),
    )
    .unwrap()
}
fn slot(offset_x: i32, facing: i8, snap: bool) -> RoutingSlot {
    RoutingSlot {
        search: SlotSearch {
            min_proximity: 0,
            max_proximity: 0,
            optimal_proximity: 0,
            directions: 0,
            offset_x,
            resolution: 16,
            ..SlotSearch::default()
        },
        facing,
        snap_to_direction: snap,
        snap_target_slot: None,
    }
}
fn spawn(guid: u32, tile: TilePos, avatar: bool) -> AcceptedCommand {
    AcceptedCommand::Spawn(SpawnSpec {
        guid,
        position: tile.center(),
        facing: Facing::NORTH,
        persistent_id: PersistentId(0),
        avatar,
    })
}
fn tick(runtime: &mut SimRuntime, commands: Vec<AcceptedCommand>) -> TickOutcome {
    let input = runtime.next_tick(commands).unwrap();
    let outcome = runtime.step(&input).unwrap();
    assert!(
        !outcome
            .events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ThreadFault { .. })),
        "{:?}",
        outcome.events
    );
    outcome
}
fn runtime(content: ContentSet, spawns: Vec<AcceptedCommand>) -> SimRuntime {
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    tick(&mut runtime, spawns);
    runtime
}
fn start(owner: i16, target: i16, routine: u16, args: Vec<i16>) -> AcceptedCommand {
    let mut context = FrameContext::for_entity(entity(owner), 100);
    context.stack_object = ObjectId(target);
    context.stack_object_ref = Some(entity(target));
    AcceptedCommand::StartBehavior {
        entity: entity(owner),
        routine: key(100, routine),
        context,
        args,
        replace: true,
    }
}
fn default_spawns() -> Vec<AcceptedCommand> {
    vec![
        spawn(100, TilePos::new(1, 1, 1), true),
        spawn(200, TilePos::new(5, 5, 1), false),
    ]
}
fn assert_exit(runtime: &SimRuntime, id: i16, success: bool) {
    assert_eq!(
        runtime.state().threads[&ObjectId(id)].stop,
        VmStop::Completed(if success {
            PrimitiveExit::ReturnTrue
        } else {
            PrimitiveExit::ReturnFalse
        })
    );
}

#[test]
fn snap_decodes_the_full_mode_word_and_occupied_containers_are_not_replaced() {
    let mut runtime = runtime(
        content(vec![]),
        vec![
            spawn(100, TilePos::new(1, 1, 1), true),
            spawn(200, TilePos::new(5, 5, 1), false),
            spawn(300, TilePos::new(7, 7, 1), false),
        ],
    );
    tick(&mut runtime, vec![start(1, 2, 4097, vec![])]);
    assert_exit(&runtime, 1, false);
    assert_eq!(runtime.state().entities[&ObjectId(1)].container, None);
    tick(&mut runtime, vec![start(1, 2, 4096, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].container,
        Some((entity(2), 0))
    );
    tick(&mut runtime, vec![start(3, 2, 4096, vec![])]);
    assert_exit(&runtime, 3, false);
    assert_eq!(
        runtime.state().entities[&ObjectId(2)].slots[0],
        Some(entity(1))
    );
    tick(&mut runtime, vec![start(1, 1, 4109, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(runtime.state().entities[&ObjectId(1)].container, None);
    assert_eq!(runtime.state().entities[&ObjectId(2)].slots[0], None);
    assert_eq!(
        runtime.state().world.object(entity(1)).unwrap().position,
        LotPosition::OUT_OF_WORLD
    );
    runtime.snapshot().unwrap();
}

#[test]
fn snap_resolves_parameter_literal_and_global_slots_and_can_enter_from_out_of_world() {
    for (routine, offset_y) in [(4100, 16), (4098, 16), (4099, -16)] {
        let data = content(vec![
            ((200, 7), slot(16, 0, true)),
            ((0, 7), slot(-16, 0, true)),
        ]);
        let mut actor = match default_spawns().remove(0) {
            AcceptedCommand::Spawn(actor) => actor,
            _ => unreachable!(),
        };
        actor.position = LotPosition::OUT_OF_WORLD;
        let mut target = match default_spawns().remove(1) {
            AcceptedCommand::Spawn(target) => target,
            _ => unreachable!(),
        };
        target.facing = Facing::EAST;
        let mut runtime = runtime(
            data,
            vec![
                AcceptedCommand::Spawn(actor),
                AcceptedCommand::Spawn(target),
            ],
        );
        let before_rng = runtime.state().rng.state();
        tick(&mut runtime, vec![start(1, 2, routine, vec![7])]);
        assert_exit(&runtime, 1, true);
        assert_eq!(
            runtime.state().world.object(entity(1)).unwrap().position,
            TilePos::new(5, 5, 1)
                .center()
                .offset(0, offset_y, 0)
                .unwrap()
        );
        assert_eq!(runtime.state().entities[&ObjectId(1)].info.direction, 2);
        assert_eq!(runtime.state().rng.state(), before_rng.wrapping_add(2)); // on-point has no scoring draw
    }
}

#[test]
fn snap_to_direction_uses_one_point_and_source_mask_heading_without_scoring_draws() {
    for (directions, facing, absolute, expected) in [
        (255, -3, false, 2), // log2(255) rounds to eight, relative to east
        (0, -3, false, 2),   // legacy negative-facing default is north
        (4, 0, false, 4),    // facing bit is ORed with existing flags: log2(5) -> two
        (64, -2, true, 6),   // non-point Absolute retains the zero base rotation
    ] {
        let mut normalized = slot(0, facing, true);
        normalized.search.min_proximity = 16;
        normalized.search.max_proximity = 32;
        normalized.search.optimal_proximity = 16;
        normalized.search.directions = directions;
        normalized.search.absolute = absolute;
        let data = replace_definitions(content(vec![]), |definitions| {
            definitions[1].object_data[8] = 4;
        })
        .with_routing_slots(vec![((200, 7), normalized)])
        .unwrap();
        let mut target = match default_spawns().remove(1) {
            AcceptedCommand::Spawn(target) => target,
            _ => unreachable!(),
        };
        target.facing = Facing::EAST;
        let mut runtime = runtime(
            data,
            vec![
                spawn(100, TilePos::new(1, 1, 1), true),
                AcceptedCommand::Spawn(target),
            ],
        );
        let before_rng = runtime.state().rng.state();
        tick(&mut runtime, vec![start(1, 2, 4098, vec![])]);
        assert_exit(&runtime, 1, true);
        assert_eq!(
            runtime.state().world.object(entity(1)).unwrap().position,
            TilePos::new(5, 5, 1).center()
        );
        assert_eq!(
            runtime.state().entities[&ObjectId(1)].info.direction,
            expected
        );
        assert_eq!(runtime.state().rng.state(), before_rng.wrapping_add(2));
    }
}

#[test]
fn snap_target_slot_works_even_when_no_candidate_survives_bounds() {
    let mut normalized = slot(-256, -3, false);
    normalized.snap_target_slot = Some(1);
    let mut runtime = runtime(content(vec![((200, 7), normalized)]), default_spawns());
    tick(&mut runtime, vec![start(1, 2, 4098, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].container,
        Some((entity(2), 1))
    );
}

#[test]
fn relative_on_top_is_one_exact_goal_and_route_snapshot_replays() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[1].object_data[8] = 4
    });
    let mut authority = runtime(data.clone(), default_spawns());
    tick(&mut authority, vec![start(1, 2, 4101, vec![])]);
    let continuation = authority.state().continuations.values().next().unwrap();
    let ContinuationKind::Route(route) = &continuation.kind else {
        panic!()
    };
    assert_eq!(route.request().goals.len(), 1);
    assert_eq!(
        route.request().goals[0].position,
        TilePos::new(5, 5, 1).center()
    );
    assert_eq!(route.request().goals[0].facing, None);
    let snapshot = authority.snapshot().unwrap();
    let mut replica = SimRuntime::new(
        data,
        LotModel::new(10, 10, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Replica,
    )
    .unwrap();
    replica.restore(&snapshot).unwrap();
    for _ in 0..150 {
        let input = authority.next_tick(vec![]).unwrap();
        let a = authority.step(&input).unwrap();
        let b = replica.step(&input).unwrap();
        assert_eq!(a.state_hash, b.state_hash);
        assert_eq!(a.events, b.events);
        if authority.state().continuations.is_empty() {
            assert_exit(&authority, 1, true);
            return;
        }
    }
    panic!("route did not finish");
}

#[test]
fn unavailable_slot_uses_resumable_failure_script_and_source_route_result() {
    let mut runtime = runtime(
        content(vec![((200, 7), slot(0, -2, false))]),
        default_spawns(),
    );
    tick(&mut runtime, vec![start(1, 2, 4102, vec![])]);
    let outcome = tick(&mut runtime, vec![]);
    let (continuation_id, callback) = outcome
        .events
        .into_iter()
        .find_map(|event| match event {
            RuntimeEvent::RouteScript {
                continuation_id,
                callback,
            } => Some((continuation_id, callback)),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        callback.kind,
        RouteCallbackKind::Failure {
            code: RouteFailCode::DestTileOccupied,
            ..
        }
    ));
    let snapshot = runtime.snapshot().unwrap();
    runtime.restore(&snapshot).unwrap();
    tick(
        &mut runtime,
        vec![AcceptedCommand::RouteCallback {
            continuation_id,
            route_id: callback.route_id,
            token: callback.token,
            outcome: CallbackOutcome::success(),
        }],
    );
    assert_exit(&runtime, 1, false);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)]
            .avatar
            .as_ref()
            .unwrap()
            .person_data
            .values[62],
        RouteFailCode::DestTileOccupied.code()
    );
}

#[test]
fn late_failure_sets_route_result_before_callback_and_preserves_temp_zero() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[2].object_data[8] = 4;
    })
    .with_routing_slots(vec![((300, 7), slot(0, -3, false))])
    .unwrap();
    let mut runtime = runtime(
        data,
        vec![
            spawn(100, TilePos::new(1, 1, 1), true),
            spawn(200, TilePos::new(5, 5, 1), false),
            spawn(300, TilePos::new(7, 7, 1), false),
        ],
    );
    tick(&mut runtime, vec![start(1, 2, 4096, vec![])]);
    tick(&mut runtime, vec![start(1, 3, 4116, vec![])]);
    let outcome = tick(&mut runtime, vec![]);
    let (continuation_id, callback) = outcome
        .events
        .into_iter()
        .find_map(|event| match event {
            RuntimeEvent::RouteScript {
                continuation_id,
                callback,
            } => Some((continuation_id, callback)),
            _ => None,
        })
        .expect("CantStand failure callback before terminal completion");
    assert!(matches!(
        callback.kind,
        RouteCallbackKind::Failure {
            code: RouteFailCode::CantStand,
            ..
        }
    ));
    assert_eq!(
        runtime.state().entities[&ObjectId(1)]
            .avatar
            .as_ref()
            .unwrap()
            .person_data
            .values[62],
        RouteFailCode::CantStand.code()
    );
    assert_eq!(runtime.state().threads[&ObjectId(1)].temps[0], 314);
    let snapshot = runtime.snapshot().unwrap();
    runtime.restore(&snapshot).unwrap();
    tick(
        &mut runtime,
        vec![AcceptedCommand::RouteCallback {
            continuation_id,
            route_id: callback.route_id,
            token: callback.token,
            outcome: CallbackOutcome::success(),
        }],
    );
    assert_exit(&runtime, 1, false);
    assert_eq!(runtime.state().threads[&ObjectId(1)].temps[0], 314);
}

#[test]
fn both_intersection_bhavs_receive_bound_arguments_and_query_writes_stay_isolated() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[0].entry_points.insert(5, key(100, 4160));
        definitions[1].entry_points.insert(5, key(200, 4160));
    })
    .with_routing_slots(vec![((200, 7), slot(0, -3, false))])
    .unwrap();
    let mut runtime = runtime(data, default_spawns());
    tick(&mut runtime, vec![start(1, 2, 4098, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().world.object(entity(1)).unwrap().position,
        runtime.state().world.object(entity(2)).unwrap().position
    );
    assert_eq!(runtime.state().entities[&ObjectId(1)].attributes[0], 0);
    assert_eq!(runtime.state().entities[&ObjectId(2)].attributes[0], 0);
}

#[test]
fn second_intersection_fault_is_propagated_without_partial_move() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[0].entry_points.insert(5, key(100, 4160));
        definitions[1].entry_points.insert(5, key(200, 4161));
    })
    .with_routing_slots(vec![((200, 7), slot(0, -3, false))])
    .unwrap();
    let mut runtime = runtime(data, default_spawns());
    let position = runtime.state().world.object(entity(1)).unwrap().position;
    let input = runtime.next_tick(vec![start(1, 2, 4098, vec![])]).unwrap();
    let outcome = runtime.step(&input).unwrap();
    assert!(
        outcome.events.iter().any(|event| matches!(
            event,
            RuntimeEvent::ThreadFault {
                fault: VmFault::InvalidOperand { opcode: 2, .. },
                ..
            }
        )),
        "events={:?}, stop={:?}",
        outcome.events,
        runtime.state().threads[&ObjectId(1)].stop
    );
    assert_eq!(
        runtime.state().world.object(entity(1)).unwrap().position,
        position
    );
    assert_eq!(runtime.state().entities[&ObjectId(1)].attributes[0], 0);
}

#[test]
fn recursively_reentered_placement_preserves_depth_and_fails_without_partial_edits() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[0].entry_points.insert(5, key(100, 4160));
        definitions[1].entry_points.insert(5, key(200, 4162));
    })
    .with_routing_slots(vec![
        ((100, 7), slot(0, -3, false)),
        ((200, 7), slot(0, -3, false)),
    ])
    .unwrap();
    let mut runtime = runtime(data, default_spawns());
    let position = runtime.state().world.object(entity(1)).unwrap().position;
    let input = runtime.next_tick(vec![start(1, 2, 4098, vec![])]).unwrap();
    let outcome = runtime.step(&input).unwrap();
    assert!(
        outcome.events.iter().any(|event| matches!(
            event,
            RuntimeEvent::ThreadFault {
                fault: VmFault::HostUnsupported(message),
                ..
            } if message.contains("depth") || message.contains("budget")
        )),
        "{:?}",
        outcome.events
    );
    assert!(outcome.instructions > 1 && outcome.instructions < 1_000);
    assert_eq!(
        runtime.state().world.object(entity(1)).unwrap().position,
        position
    );
    assert_eq!(runtime.state().entities[&ObjectId(1)].attributes[0], 0);
    assert_eq!(runtime.state().entities[&ObjectId(2)].attributes[0], 0);
    runtime.snapshot().unwrap();
}

#[test]
fn scoring_and_snap_revalidation_share_the_query_budget_even_on_failure() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[0].entry_points.insert(5, key(100, 4160));
        definitions[1].entry_points.insert(5, key(200, 4160));
    })
    .with_routing_slots(vec![((200, 7), slot(0, -3, false))])
    .unwrap();
    let mut config = RuntimeConfig::new(VmMode::Ts1, 7, 1, 123);
    config.limits.instruction_budget_per_entity = 5;
    let mut runtime = SimRuntime::new(
        data,
        LotModel::new(10, 10, 1).unwrap(),
        config,
        RuntimeRole::Authority,
    )
    .unwrap();
    tick(&mut runtime, default_spawns());
    let position = runtime.state().world.object(entity(1)).unwrap().position;
    let input = runtime.next_tick(vec![start(1, 2, 4098, vec![])]).unwrap();
    let outcome = runtime.step(&input).unwrap();
    // Two two-instruction scripts fit the scoring probe. A fresh budget for
    // relocation would also succeed, so this must fail during revalidation.
    assert!(
        outcome.events.iter().any(|event| matches!(
            event,
            RuntimeEvent::ThreadFault {
                fault: VmFault::HostUnsupported(message),
                ..
            } if message.contains("budget") || message.contains("synchronously")
        )),
        "{:?}",
        outcome.events
    );
    assert!(outcome.instructions >= 5 && outcome.instructions <= 9);
    assert_eq!(
        runtime.state().world.object(entity(1)).unwrap().position,
        position
    );
    assert_eq!(runtime.state().entities[&ObjectId(1)].attributes[0], 0);
    assert_eq!(runtime.state().entities[&ObjectId(2)].attributes[0], 0);
    runtime.snapshot().unwrap();
}

#[test]
fn look_towards_head_and_body_checks_preserve_source_branching() {
    let mut runtime = runtime(content(vec![]), default_spawns());
    tick(
        &mut runtime,
        vec![
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: entity(1),
                    field: EntityField::PersonData,
                    index: 43,
                },
                value: 1,
            },
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: entity(1),
                    field: EntityField::PersonData,
                    index: 44,
                },
                value: 0,
            },
        ],
    );
    let mut context = FrameContext::for_entity(entity(1), 100);
    context.stack_object = ObjectId(2);
    context.stack_object_ref = Some(entity(2));
    let before = runtime.state().clone();
    assert_eq!(
        runtime
            .query_behavior(entity(1), key(100, 4104), context.clone(), vec![], 16)
            .unwrap()
            .stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert_eq!(
        runtime
            .query_behavior(entity(1), key(100, 4103), context, vec![], 16)
            .unwrap()
            .stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(runtime.state(), &before);
    tick(&mut runtime, vec![start(1, 2, 4103, vec![])]);
    assert_eq!(
        &runtime.state().entities[&ObjectId(1)]
            .avatar
            .as_ref()
            .unwrap()
            .person_data
            .values[41..=45],
        &[2, 1, 0, 1, 0]
    );
    tick(&mut runtime, vec![start(1, 2, 4104, vec![])]);
    for _ in 0..10 {
        if runtime.state().continuations.is_empty() {
            break;
        }
        tick(&mut runtime, vec![]);
    }
    assert_exit(&runtime, 1, true);
    assert_eq!(runtime.state().entities[&ObjectId(1)].info.direction, 3);
}

#[test]
fn look_towards_group_average_wraps_each_source_short_addition() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[1].object_data[8] = 4;
    });
    let mut fixture = SimRuntime::new(
        data.clone(),
        LotModel::new(256, 4, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let mut spawns = vec![spawn(100, TilePos::new(0, 1, 1), true)];
    spawns.extend((0..9).map(|_| spawn(200, TilePos::new(255, 1, 1), false)));
    tick(&mut fixture, spawns);
    let mut state = fixture.state().clone();
    let group: Vec<_> = (2..=10).map(ObjectId).collect();
    for id in &group {
        let info = &mut state.entities.get_mut(id).unwrap().info;
        info.group = group.clone();
        info.base_object = ObjectId(2);
        info.multi_tile = true;
        let mut projection = state.world.object(entity(id.0)).unwrap().clone();
        projection.multitile_group = Some(entity(2));
        state.world.replace_object(projection).unwrap();
    }
    // 9 * 4088 wraps from 36792 to -28744 before division by nine.
    // The resulting point is west of the actor; a wide average points east.
    for (routine, expected) in [(4114, Facing::WEST), (4115, Facing::EAST)] {
        let mut runtime =
            SimRuntime::from_state(state.clone(), data.clone(), RuntimeRole::Authority).unwrap();
        tick(&mut runtime, vec![start(1, 2, routine, vec![])]);
        let continuation = runtime.state().continuations.values().next().unwrap();
        let ContinuationKind::Route(route) = &continuation.kind else {
            panic!()
        };
        assert_eq!(route.request().goals[0].facing, Some(expected));
    }
}

#[test]
fn find_location_preserves_deferred_tile_order_local_reference_and_vector_direction() {
    for (routine, expected) in [(4105, TilePos::new(2, 2, 1)), (4106, TilePos::new(3, 3, 1))] {
        let data = replace_definitions(content(vec![]), |definitions| {
            definitions[0].object_data[8] = 4
        });
        let mut runtime = runtime(
            data,
            vec![
                spawn(100, TilePos::new(3, 3, 1), true),
                spawn(200, TilePos::new(7, 7, 1), false),
            ],
        );
        tick(&mut runtime, vec![start(1, 2, routine, vec![])]);
        assert_exit(&runtime, 1, true);
        assert_eq!(
            runtime.state().world.object(entity(2)).unwrap().position,
            expected.center()
        );
    }
    let mut reference = match spawn(300, TilePos::new(4, 4, 1), false) {
        AcceptedCommand::Spawn(value) => value,
        _ => unreachable!(),
    };
    reference.facing = Facing::EAST;
    let mut runtime = runtime(
        content(vec![]),
        vec![
            spawn(100, TilePos::new(1, 1, 1), true),
            spawn(200, TilePos::new(7, 7, 1), false),
            AcceptedCommand::Spawn(reference),
        ],
    );
    tick(&mut runtime, vec![start(1, 2, 4107, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().world.object(entity(2)).unwrap().position,
        TilePos::new(3, 4, 1).center()
    );
    assert_eq!(runtime.state().entities[&ObjectId(2)].info.direction, 2);
}

#[test]
fn find_location_random_consumes_x_then_y_and_stops_after_first_success() {
    let mut runtime = runtime(content(vec![]), default_spawns());
    let mut expected = runtime.state().rng.clone();
    let x = expected.next(8) as i16 + 1;
    let y = expected.next(8) as i16 + 1;
    expected.mix_entity_count(2);
    tick(&mut runtime, vec![start(1, 2, 4108, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().world.object(entity(2)).unwrap().position,
        TilePos::new(x, y, 1).center()
    );
    assert_eq!(runtime.state().rng, expected);
}

#[test]
fn reach_grabs_on_xevt_and_snapshot_replay_preserves_the_event_cursor() {
    let data = content(vec![]);
    let mut authority = runtime(data.clone(), default_spawns());
    tick(&mut authority, vec![start(1, 2, 4110, vec![])]);
    assert_eq!(authority.state().entities[&ObjectId(2)].container, None);
    assert_eq!(
        authority.state().entities[&ObjectId(1)]
            .avatar
            .as_ref()
            .unwrap()
            .animations
            .animations[0]
            .event_queue
            .front(),
        Some(&0)
    );
    let snapshot = authority.snapshot().unwrap();
    let mut replica = SimRuntime::new(
        data,
        LotModel::new(10, 10, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Replica,
    )
    .unwrap();
    replica.restore(&snapshot).unwrap();
    for _ in 0..8 {
        let input = authority.next_tick(vec![]).unwrap();
        assert_eq!(
            authority.step(&input).unwrap().state_hash,
            replica.step(&input).unwrap().state_hash
        );
    }
    assert_exit(&authority, 1, true);
    assert_eq!(
        authority.state().entities[&ObjectId(2)].container,
        Some((entity(1), 0))
    );
    assert!(
        authority
            .state()
            .world
            .object(entity(2))
            .unwrap()
            .rules
            .zero_extent
    );
}

#[test]
fn reach_slot_uses_parameter_index_height_and_drop_event() {
    let mut runtime = runtime(
        content(vec![]),
        vec![
            spawn(100, TilePos::new(1, 1, 1), true),
            spawn(200, TilePos::new(5, 5, 1), false),
            spawn(300, TilePos::new(7, 7, 1), false),
        ],
    );
    tick(
        &mut runtime,
        vec![
            start(3, 1, 4096, vec![]),
            AcceptedCommand::DefineSlot(SlotDefinition {
                key: SlotKey {
                    owner: entity(2),
                    index: 1,
                },
                capacity: 1,
                height: 3,
                support_strength: 100,
                max_size: 100,
                revision: 0,
            }),
        ],
    );
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].slots[0],
        Some(entity(3))
    );
    tick(&mut runtime, vec![start(1, 2, 4111, vec![1])]);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)]
            .avatar
            .as_ref()
            .unwrap()
            .animations
            .animations[0]
            .metadata
            .resource,
        "a2o-reach-tableht.anim"
    );
    tick(&mut runtime, vec![]);
    assert_eq!(
        runtime.state().entities[&ObjectId(2)].slots[1],
        Some(entity(3))
    );
    assert_eq!(runtime.state().entities[&ObjectId(1)].slots[0], None);
    for _ in 0..6 {
        tick(&mut runtime, vec![]);
    }
    assert_exit(&runtime, 1, true);
}

#[test]
fn runtime_multilevel_portal_callback_snapshot_finishes_on_both_replicas() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[1].object_data[8] = 4;
        definitions[1].entry_points.insert(15, key(200, 4300));
    });
    let mut lot = LotModel::new(8, 8, 2).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            lot.set_floor(TilePos::new(x, y, 2), 1).unwrap();
        }
    }
    let mut authority = SimRuntime::new(
        data.clone(),
        lot,
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    tick(
        &mut authority,
        vec![
            spawn(100, TilePos::new(1, 2, 1), true),
            spawn(200, TilePos::new(3, 2, 1), false),
        ],
    );
    let mut state = authority.state().clone();
    let exit = TilePos::new(3, 2, 2).center();
    state
        .world
        .lot
        .upsert_portal(Portal {
            id: PortalId(1),
            entity: entity(2),
            entry: TilePos::new(3, 2, 1).center(),
            exit,
            bidirectional: true,
            enabled: true,
            cost: 16,
            revision: 0,
        })
        .unwrap();
    authority = SimRuntime::from_state(state, data.clone(), RuntimeRole::Authority).unwrap();
    tick(
        &mut authority,
        vec![AcceptedCommand::BeginRoute {
            entity: entity(1),
            target: Some(entity(2)),
            goals: vec![RouteGoal::point(TilePos::new(6, 2, 2).center())],
        }],
    );
    let mut callback = None;
    for _ in 0..100 {
        let outcome = tick(&mut authority, vec![]);
        callback = outcome.events.into_iter().find_map(|e| match e {
            RuntimeEvent::RouteScript {
                continuation_id,
                callback,
            } => Some((continuation_id, callback)),
            _ => None,
        });
        if callback.is_some() {
            break;
        }
    }
    let (continuation_id, callback) = callback.expect("portal callback");
    assert!(matches!(callback.kind, RouteCallbackKind::Portal { .. }));
    let snapshot = authority.snapshot().unwrap();
    let mut replica =
        SimRuntime::from_state(authority.state().clone(), data, RuntimeRole::Replica).unwrap();
    replica.restore(&snapshot).unwrap();
    let input = authority
        .next_tick(vec![AcceptedCommand::RouteCallback {
            continuation_id,
            route_id: callback.route_id,
            token: callback.token,
            outcome: CallbackOutcome {
                success: true,
                position: Some(exit),
                blocker: None,
            },
        }])
        .unwrap();
    assert_eq!(
        authority.step(&input).unwrap().state_hash,
        replica.step(&input).unwrap().state_hash
    );
    for _ in 0..100 {
        let input = authority.next_tick(vec![]).unwrap();
        assert_eq!(
            authority.step(&input).unwrap().state_hash,
            replica.step(&input).unwrap().state_hash
        );
        if authority.state().continuations.is_empty() {
            assert_eq!(
                authority.state().world.object(entity(1)).unwrap().position,
                TilePos::new(6, 2, 2).center()
            );
            return;
        }
    }
    panic!("portal route did not finish");
}

#[test]
fn contained_route_runs_stand_callback_and_detaches_before_walking() {
    let data = replace_definitions(content(vec![]), |definitions| {
        definitions[1].entry_points.insert(27, key(200, 4300));
        definitions[2].object_data[8] = 4;
    });
    let mut runtime = runtime(
        data,
        vec![
            spawn(100, TilePos::new(1, 1, 1), true),
            spawn(200, TilePos::new(5, 5, 1), false),
            spawn(300, TilePos::new(7, 7, 1), false),
        ],
    );
    tick(&mut runtime, vec![start(1, 2, 4096, vec![])]);
    tick(
        &mut runtime,
        vec![
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: entity(1),
                    field: EntityField::PersonData,
                    index: 0,
                },
                value: 1,
            },
            start(1, 3, 4101, vec![]),
        ],
    );
    let outcome = tick(&mut runtime, vec![]);
    let (continuation_id, callback) = outcome
        .events
        .into_iter()
        .find_map(|event| match event {
            RuntimeEvent::RouteScript {
                continuation_id,
                callback,
            } => Some((continuation_id, callback)),
            _ => None,
        })
        .unwrap();
    assert_eq!(callback.kind, RouteCallbackKind::Stand);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].container,
        Some((entity(2), 0))
    );
    tick(
        &mut runtime,
        vec![AcceptedCommand::RouteCallback {
            continuation_id,
            route_id: callback.route_id,
            token: callback.token,
            outcome: CallbackOutcome {
                success: true,
                position: Some(TilePos::new(6, 5, 1).center()),
                blocker: None,
            },
        }],
    );
    assert_eq!(runtime.state().entities[&ObjectId(1)].container, None);
    assert_eq!(runtime.state().entities[&ObjectId(2)].slots[0], None);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)]
            .avatar
            .as_ref()
            .unwrap()
            .person_data
            .values[0],
        0
    );
    assert!(
        !runtime
            .state()
            .world
            .object(entity(1))
            .unwrap()
            .rules
            .zero_extent
    );
    for _ in 0..100 {
        if runtime.state().continuations.is_empty() {
            assert_exit(&runtime, 1, true);
            return;
        }
        tick(&mut runtime, vec![]);
    }
    panic!("standing route did not finish");
}

#[test]
fn reach_completion_precedes_a_queued_grab_event() {
    let base = content(vec![]);
    let definitions = [100, 200, 300]
        .into_iter()
        .map(|id| base.object(id).unwrap().clone())
        .collect();
    let animations = (1..=5)
        .map(|id| {
            let key = AnimationKey {
                owner: 100,
                scope: 0,
                id,
            };
            let mut metadata = base.animation(key).unwrap().clone();
            if id == 1 {
                metadata.num_frames = 1;
            }
            (key, metadata)
        })
        .collect();
    let data = ContentSet::new(
        base.routines().clone(),
        definitions,
        animations,
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = runtime(data, default_spawns());
    tick(&mut runtime, vec![start(1, 2, 4110, vec![])]);
    let current = &runtime.state().entities[&ObjectId(1)]
        .avatar
        .as_ref()
        .unwrap()
        .animations
        .animations[0];
    assert!(current.end_reached);
    assert_eq!(current.event_queue.front(), Some(&0));
    tick(&mut runtime, vec![]);
    assert_exit(&runtime, 1, true);
    assert_eq!(runtime.state().entities[&ObjectId(2)].container, None);
}

#[test]
fn smoke_find_location_uses_callee_caller_midpoint_and_half_tile_offset() {
    let mut runtime = runtime(
        content(vec![]),
        vec![
            spawn(100, TilePos::new(1, 1, 1), true),
            spawn(200, TilePos::new(7, 7, 1), false),
            spawn(300, TilePos::new(5, 5, 1), false),
        ],
    );
    let mut command = start(1, 2, 4112, vec![]);
    if let AcceptedCommand::StartBehavior { context, .. } = &mut command {
        context.callee = entity(3);
    }
    tick(&mut runtime, vec![command]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().world.object(entity(2)).unwrap().position,
        LotPosition::new(48, 48, 1)
    );
}

#[test]
fn out_of_world_find_location_does_not_dereference_an_unused_null_local_reference() {
    let mut runtime = runtime(content(vec![]), default_spawns());
    tick(&mut runtime, vec![start(1, 2, 4113, vec![])]);
    assert_exit(&runtime, 1, true);
    assert_eq!(
        runtime.state().world.object(entity(2)).unwrap().position,
        LotPosition::OUT_OF_WORLD
    );
}
