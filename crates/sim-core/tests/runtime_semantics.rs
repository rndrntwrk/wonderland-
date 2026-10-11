use sim_core::{
    ids::{ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::*,
    state::*,
    vm::*,
    world::{Facing, LotModel, TilePos},
};

fn expr(lhs: Scope, index: i16, rhs: Scope, value: i16, op: u8, next: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        ExpressionOperand {
            lhs: Variable::new(lhs, index),
            rhs: Variable::new(rhs, value),
            is_signed: 0,
            operator: op,
        }
        .encode(),
    )
}
fn key(guid: u32, id: u16) -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(guid),
        id,
    }
}
fn rt(content: ContentSet) -> SimRuntime {
    SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 8, 1, 99),
        RuntimeRole::Authority,
    )
    .unwrap()
}
fn spawn(guid: u32, avatar: bool, x: i16) -> AcceptedCommand {
    AcceptedCommand::Spawn(SpawnSpec {
        guid,
        position: TilePos::new(x, 3, 1).center(),
        facing: Facing::NORTH,
        persistent_id: PersistentId(0),
        avatar,
    })
}
fn step(r: &mut SimRuntime, commands: Vec<AcceptedCommand>) -> TickOutcome {
    let tick = r.next_tick(commands).unwrap();
    r.step(&tick).unwrap()
}
fn reference(r: &SimRuntime, id: i16) -> sim_core::ids::EntityRef {
    r.state().entities[&ObjectId(id)].info.reference
}
fn start(r: &SimRuntime, id: i16, guid: u32, routine: u16) -> AcceptedCommand {
    let entity = reference(r, id);
    AcceptedCommand::StartBehavior {
        entity,
        routine: key(guid, routine),
        context: FrameContext::for_entity(entity, guid),
        args: vec![0; 4],
        replace: true,
    }
}

#[test]
fn in_tick_function_condition_preserves_shared_temps_and_mutations() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    expr(Scope::StackObjectId, 0, Scope::Literal, 2, 5, 1),
                    VmInstruction::new(20, 2, 255, [0; 8]),
                    expr(Scope::MyObjectAttributes, 0, Scope::Temps, 0, 5, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(200, 4096),
            VmRoutine::new(4096, 0, 4, vec![VmInstruction::new(255, 254, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(200, 4097),
            VmRoutine::new(
                4097,
                0,
                4,
                vec![
                    expr(Scope::Temps, 0, Scope::Literal, 77, 5, 1),
                    expr(Scope::StackObjectAttributes, 0, Scope::Literal, 1, 3, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let caller = ObjectDefinition::new(100, 1);
    let mut target = ObjectDefinition::new(200, 1);
    target.entry_points.insert(18, key(200, 4096));
    target.entry_conditions.insert(18, key(200, 4097));
    let content =
        ContentSet::new(routines, vec![caller, target], vec![], TuningSet::default()).unwrap();
    let mut r = rt(content);
    step(&mut r, vec![spawn(100, false, 3), spawn(200, false, 4)]);
    let command = start(&r, 1, 100, 4096);
    step(&mut r, vec![command]);
    assert_eq!(r.state().entities[&ObjectId(1)].attributes[0], 77);
    assert_eq!(r.state().entities[&ObjectId(2)].attributes[0], 1);
    assert_eq!(r.state().threads[&ObjectId(1)].temps[0], 77);
    let bytes = r.snapshot().unwrap();
    r.restore(&bytes).unwrap();
}

#[test]
fn world_usage_tracks_avatar_callee_frames_and_clears_when_they_finish() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    expr(Scope::StackObjectId, 0, Scope::Literal, 2, 5, 1),
                    VmInstruction::new(20, 254, 255, [0; 8]),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(100, 4097),
            VmRoutine::new(4097, 0, 4, vec![VmInstruction::new(255, 254, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(200, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    expr(Scope::Parameters, 0, Scope::Literal, 10, 5, 1),
                    VmInstruction::new(0, 254, 255, [0; 8]),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut target = ObjectDefinition::new(200, 1);
    target.entry_points.insert(18, key(200, 4096));
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(100, 1), target],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut r = rt(content);
    step(&mut r, vec![spawn(100, true, 3), spawn(200, false, 4)]);
    let target = reference(&r, 2);
    let command = start(&r, 1, 100, 4096);
    step(&mut r, vec![command]);
    assert!(r.state().world.object(target).unwrap().in_use);
    let command = start(&r, 1, 100, 4097);
    step(&mut r, vec![command]);
    assert!(!r.state().world.object(target).unwrap().in_use);
}

#[test]
fn reset_runs_object_reset_entry_and_preserves_thread_registers() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![expr(
                    Scope::MyObjectAttributes,
                    0,
                    Scope::Literal,
                    1,
                    3,
                    254,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    routines
        .insert(
            key(100, 4097),
            VmRoutine::new(
                4097,
                0,
                4,
                vec![
                    expr(Scope::MyObjectAttributes, 1, Scope::Temps, 0, 5, 1),
                    expr(Scope::Temps, 0, Scope::Literal, 1, 3, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(100, 2);
    object.entry_points.insert(1, key(100, 4096));
    object.entry_points.insert(3, key(100, 4097));
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap();
    let mut r = rt(content);
    step(&mut r, vec![spawn(100, false, 3)]);
    let mut saved = r.state().clone();
    saved.threads.get_mut(&ObjectId(1)).unwrap().temps[0] = 70;
    saved.threads.get_mut(&ObjectId(1)).unwrap().temp_xl[0] = 90000;
    r = SimRuntime::from_state(saved, r.content().clone(), RuntimeRole::Authority).unwrap();
    let entity = reference(&r, 1);
    step(&mut r, vec![AcceptedCommand::Reset { entity }]);
    assert_eq!(r.state().entities[&ObjectId(1)].attributes, [2, 70]);
    assert_eq!(r.state().threads[&ObjectId(1)].temps[0], 70);
    assert_eq!(r.state().threads[&ObjectId(1)].temp_xl[0], 90000);
    assert!(!r.state().threads[&ObjectId(1)].is_check);
    assert!(r.pending_effects().unwrap().is_empty());
}

#[test]
fn human_avatar_reset_skips_object_reset_entry_and_clears_its_headline() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4097),
            VmRoutine::new(
                4097,
                0,
                4,
                vec![expr(
                    Scope::MyObjectAttributes,
                    0,
                    Scope::Literal,
                    99,
                    5,
                    254,
                )],
            )
            .unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(100, 1);
    object.entry_points.insert(3, key(100, 4097));
    let mut r = rt(ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap());
    step(&mut r, vec![spawn(100, true, 3)]);
    let mut saved = r.state().clone();
    saved.entities.get_mut(&ObjectId(1)).unwrap().headline = Some(HeadlineState {
        duration: 60,
        anim: 0,
        kind: HeadlineKind::Money(12),
    });
    r = SimRuntime::from_state(saved, r.content().clone(), RuntimeRole::Authority).unwrap();
    let entity = reference(&r, 1);
    step(&mut r, vec![AcceptedCommand::Reset { entity }]);
    assert_eq!(r.state().entities[&ObjectId(1)].attributes, [0]);
    assert!(r.state().entities[&ObjectId(1)].headline.is_none());
}

#[test]
fn aggregate_instruction_exhaustion_rolls_back_the_entire_tick() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    expr(Scope::MyObjectAttributes, 0, Scope::Literal, 1, 3, 1),
                    expr(Scope::MyObjectAttributes, 0, Scope::Literal, 1, 3, 2),
                    expr(Scope::MyObjectAttributes, 0, Scope::Literal, 1, 3, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(100, 1);
    object.entry_points.insert(1, key(100, 4096));
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap();
    let mut config = RuntimeConfig::new(VmMode::Ts1, 8, 1, 99);
    config.limits.instruction_budget_per_entity = 4;
    config.limits.max_tick_instructions = 5;
    let mut r = SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).unwrap(),
        config,
        RuntimeRole::Authority,
    )
    .unwrap();
    let before = r.state().clone();
    let input = r
        .next_tick(vec![spawn(100, false, 3), spawn(100, false, 4)])
        .unwrap();
    assert_eq!(r.step(&input), Err(RuntimeError::InstructionLimit));
    assert_eq!(r.state(), &before);
}

#[test]
fn runaway_entity_is_quarantined_without_retrying_each_tick() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(4096, 0, 4, vec![VmInstruction::new(255, 0, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(100, 1);
    object.entry_points.insert(1, key(100, 4096));
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap();
    let mut config = RuntimeConfig::new(VmMode::Ts1, 8, 1, 99);
    config.limits.instruction_budget_per_entity = 4;
    let mut r = SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).unwrap(),
        config,
        RuntimeRole::Authority,
    )
    .unwrap();
    let first = step(&mut r, vec![spawn(100, false, 3)]);
    assert_eq!(first.instructions, 4);
    assert_eq!(
        r.state().entities[&ObjectId(1)].lifecycle,
        LifecyclePhase::Faulted
    );
    let entity = reference(&r, 1);
    assert!(r.state().scheduler.scheduled_tick(entity).is_none());
    let second = step(&mut r, vec![]);
    assert_eq!(second.instructions, 0);
}

#[test]
fn notify_current_owner_interrupts_the_following_sleep_without_rng_draw() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    expr(Scope::Parameters, 0, Scope::Literal, 5, 5, 1),
                    VmInstruction::new(49, 2, 255, [0; 8]),
                    VmInstruction::new(0, 254, 255, [0; 8]),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut r = rt(ContentSet::new(
        routines,
        vec![ObjectDefinition::new(100, 1)],
        vec![],
        TuningSet::default(),
    )
    .unwrap());
    step(&mut r, vec![spawn(100, false, 3)]);
    let seed = r.state().rng.state();
    let command = start(&r, 1, 100, 4096);
    step(&mut r, vec![command]);
    assert_eq!(
        r.state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert!(!r.state().threads[&ObjectId(1)].interrupt);
    assert_eq!(r.state().rng.state(), seed.wrapping_add(1));
}

#[test]
fn ts1_family_and_inventory_projection_round_trip_and_budget_source_quirk() {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key(100, 4096),
            VmRoutine::new(
                4096,
                0,
                4,
                vec![VmInstruction::new(25, 254, 255, [0, 0, 20, 0, 0, 0, 0, 0])],
            )
            .unwrap(),
        )
        .unwrap();
    let mut r = rt(ContentSet::new(
        routines,
        vec![ObjectDefinition::new(100, 1)],
        vec![],
        TuningSet::default(),
    )
    .unwrap());
    step(
        &mut r,
        vec![
            spawn(100, false, 3),
            AcceptedCommand::SetTs1FamilyBudget(Some(10)),
            AcceptedCommand::SetTs1Inventory {
                neighbor: -2,
                items: vec![Ts1InventoryItem {
                    token_type: 2,
                    guid: 0x1234,
                    count: 3,
                }],
            },
        ],
    );
    let command = start(&r, 1, 100, 4096);
    step(&mut r, vec![command]);
    assert_eq!(r.state().ts1_family_budget, Some(-10));
    assert_eq!(
        r.state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    let before = r.state().clone();
    let bytes = r.snapshot().unwrap();
    r.restore(&bytes).unwrap();
    assert_eq!(r.state(), &before);
    let command = start(&r, 1, 100, 4096);
    step(&mut r, vec![command]);
    assert_eq!(
        r.state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert_eq!(r.state().ts1_family_budget, Some(-10));
}
