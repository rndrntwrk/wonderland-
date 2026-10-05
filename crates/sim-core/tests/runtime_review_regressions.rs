//! Independent runtime review regressions. These exercise the integrated host
//! through public APIs, plus one borrowed thread-overlay adapter boundary.
use sim_core::{
    ids::{ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::*,
    state::*,
    vm::*,
    world::{Facing, LotModel, TilePos},
};

const CALLER_GUID: u32 = 100;
const CANDIDATE_GUID: u32 = 200;
const FIND_ROUTINE: RoutineKey = RoutineKey {
    scope: RoutineScope::Private(CALLER_GUID),
    id: 4100,
};
const BURN_ROUTINE: RoutineKey = RoutineKey {
    scope: RoutineScope::Private(CALLER_GUID),
    id: 4101,
};

fn review_content() -> ContentSet {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            FIND_ROUTINE,
            VmRoutine::new(4100, 0, 4, vec![VmInstruction::new(14, 254, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    routines
        .insert(
            BURN_ROUTINE,
            VmRoutine::new(4101, 0, 4, vec![VmInstruction::new(9, 254, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    let action = RoutineKey {
        scope: RoutineScope::Private(CANDIDATE_GUID),
        id: 4096,
    };
    let set = ExpressionOperand {
        lhs: Variable::new(Scope::Temps, 0),
        rhs: Variable::new(Scope::Literal, 0),
        is_signed: 0,
        operator: 5,
    }
    .encode();
    routines
        .insert(
            action,
            VmRoutine::new(4096, 0, 4, vec![VmInstruction::new(2, 254, 255, set)]).unwrap(),
        )
        .unwrap();
    let caller = ObjectDefinition::new(CALLER_GUID, 1);
    let mut candidate = ObjectDefinition::new(CANDIDATE_GUID, 1);
    candidate.object_data[31] = 50; // PrepValue: valid prepare-food candidate.
    candidate.entry_points.insert(18, action);
    ContentSet::new(
        routines,
        vec![caller, candidate],
        vec![],
        TuningSet::default(),
    )
    .unwrap()
}

fn runtime_with_two_objects() -> SimRuntime {
    runtime_with_content(review_content())
}

fn runtime_with_content(content: ContentSet) -> SimRuntime {
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawns = [(CALLER_GUID, 3), (CANDIDATE_GUID, 6)]
        .into_iter()
        .map(|(guid, x)| {
            AcceptedCommand::Spawn(SpawnSpec {
                guid,
                position: TilePos::new(x, 3, 1).center(),
                facing: Facing::NORTH,
                persistent_id: PersistentId(0),
                avatar: false,
            })
        })
        .collect();
    let input = runtime.next_tick(spawns).unwrap();
    runtime.step(&input).unwrap();
    runtime
}

fn write_object_data(runtime: &mut SimRuntime, index: u16, value: i16) {
    let candidate = runtime.state().entities[&ObjectId(2)].info.reference;
    let input = runtime
        .next_tick(vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: candidate,
                field: EntityField::ObjectData,
                index,
            },
            value,
        }])
        .unwrap();
    runtime.step(&input).unwrap();
}

fn find_stop(runtime: &SimRuntime) -> VmStop {
    let caller = runtime.state().entities[&ObjectId(1)].info.reference;
    runtime
        .query_behavior(
            caller,
            FIND_ROUTINE,
            FrameContext::for_entity(caller, CALLER_GUID),
            vec![0; 4],
            32,
        )
        .unwrap()
        .stop
}

#[test]
fn source_attribute_growth_survives_tick_validation_and_snapshot_restore() {
    let mut runtime = runtime_with_two_objects();
    let caller = runtime.state().entities[&ObjectId(1)].info.reference;
    let input = runtime
        .next_tick(vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: caller,
                field: EntityField::Attribute,
                index: 3,
            },
            value: 42,
        }])
        .unwrap();
    let result = runtime.step(&input);
    assert!(
        result.is_ok(),
        "valid attribute growth rejected: {result:?}"
    );
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].attributes,
        [0, 0, 0, 42]
    );
    let bytes = runtime.snapshot().unwrap();
    let mut restored = runtime_with_two_objects();
    restored.restore(&bytes).unwrap();
    assert_eq!(restored.state(), runtime.state());
}

#[test]
fn find_best_excludes_occupied_objects_without_requiring_an_engine_query_write() {
    let mut runtime = runtime_with_two_objects();
    write_object_data(&mut runtime, 8, 1 << 5); // Occupied.
    assert_eq!(runtime.state().entities[&ObjectId(2)].object_data[79], 0);
    assert_eq!(
        find_stop(&runtime),
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
}

#[test]
fn find_best_does_not_treat_a_successful_safe_delete_query_as_in_use() {
    let mut runtime = runtime_with_two_objects();
    write_object_data(&mut runtime, 79, 1); // Query safe-to-delete: true.
    assert_eq!(runtime.state().entities[&ObjectId(2)].object_data[79], 1);
    assert_eq!(
        find_stop(&runtime),
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
}

#[test]
fn restored_creation_main_inputs_are_consumed_once_before_main_restarts() {
    let original = review_content();
    let mut routines = original.routines().clone();
    let main = RoutineKey {
        scope: RoutineScope::Private(CALLER_GUID),
        id: 4200,
    };
    let set = |attribute, source| {
        ExpressionOperand {
            lhs: Variable::new(Scope::MyObjectAttributes, attribute),
            rhs: source,
            is_signed: 0,
            operator: 5,
        }
        .encode()
    };
    routines
        .insert(
            main,
            VmRoutine::new(
                4200,
                0,
                4,
                vec![
                    VmInstruction::new(2, 1, 255, set(0, Variable::new(Scope::Parameters, 0))),
                    VmInstruction::new(2, 254, 255, set(1, Variable::new(Scope::StackObjectId, 0))),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut caller_definition = original.object(CALLER_GUID).unwrap().clone();
    caller_definition.attributes.resize(2, 0);
    caller_definition.entry_points.insert(1, main);
    let content = ContentSet::new(
        routines,
        vec![
            caller_definition,
            original.object(CANDIDATE_GUID).unwrap().clone(),
        ],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = SimRuntime::new(
        content.clone(),
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawn = |guid, x| {
        AcceptedCommand::Spawn(SpawnSpec {
            guid,
            position: TilePos::new(x, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })
    };
    let tick = runtime
        .next_tick(vec![spawn(CALLER_GUID, 3), spawn(CANDIDATE_GUID, 6)])
        .unwrap();
    runtime.step(&tick).unwrap();

    // A valid restored creation whose Init finished and first Main is pending.
    let mut restored_state = runtime.state().clone();
    let entity = restored_state.entities.get_mut(&ObjectId(1)).unwrap();
    let reference = entity.info.reference;
    entity.pending_entrypoints = [1].into();
    entity.lifecycle = LifecyclePhase::Initializing;
    entity.main_parameter = ObjectId(77);
    entity.main_stack_object = ObjectId(2);
    restored_state
        .threads
        .insert(ObjectId(1), VmThread::new(reference, VmMode::Ts1));
    let mut restored =
        SimRuntime::from_state(restored_state, content, RuntimeRole::Authority).unwrap();
    let first = restored.next_tick(vec![]).unwrap();
    restored.step(&first).unwrap();
    let entity = &restored.state().entities[&ObjectId(1)];
    assert_eq!(entity.attributes, [77, 2]);
    assert_eq!(
        entity.main_parameter,
        ObjectId::NULL,
        "creation parameter was not consumed"
    );
    assert_eq!(
        entity.main_stack_object,
        ObjectId::NULL,
        "creation stack-object override was not consumed"
    );

    let second = restored.next_tick(vec![]).unwrap();
    restored.step(&second).unwrap();
    assert_eq!(restored.state().entities[&ObjectId(1)].attributes, [0, 0]);
}

#[test]
fn deleting_a_multitile_base_preserves_one_live_group_and_its_projections() {
    let mut runtime = runtime_with_two_objects();
    let spawn_third = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: CANDIDATE_GUID,
            position: TilePos::new(8, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&spawn_third).unwrap();
    let content = runtime.content().clone();
    let mut state = runtime.state().clone();
    let old_base = state.entities[&ObjectId(1)].info.reference;
    let members = vec![ObjectId(1), ObjectId(2), ObjectId(3)];
    for id in &members {
        let entity = state.entities.get_mut(id).unwrap();
        entity.info.base_object = old_base.object_id;
        entity.info.multi_tile = true;
        entity.info.group = members.clone();
        let mut object = state.world.object(entity.info.reference).unwrap().clone();
        object.multitile_group = Some(old_base);
        state.world.replace_object(object).unwrap();
    }
    let mut restored = SimRuntime::from_state(state, content, RuntimeRole::Authority).unwrap();
    let deletion = restored
        .next_tick(vec![AcceptedCommand::Delete { entity: old_base }])
        .unwrap();
    let result = restored.step(&deletion);
    assert!(result.is_ok(), "valid base deletion rejected: {result:?}");
    assert!(!restored.state().ids.is_live(old_base));
    let second = &restored.state().entities[&ObjectId(2)];
    let third = &restored.state().entities[&ObjectId(3)];
    assert_eq!(second.info.group, [ObjectId(2), ObjectId(3)]);
    assert_eq!(third.info.group, second.info.group);
    assert_eq!(third.info.base_object, second.info.base_object);
    let new_base = restored
        .state()
        .ids
        .resolve(second.info.base_object)
        .unwrap();
    for member in [&second.info, &third.info] {
        assert_eq!(
            restored
                .state()
                .world
                .object(member.reference)
                .unwrap()
                .multitile_group,
            Some(new_base)
        );
    }
    restored.snapshot().unwrap();
}

#[test]
fn burn_treats_source_water_rooms_as_pool_rooms() {
    let mut lot = LotModel::new(12, 12, 1).unwrap();
    lot.set_floor(TilePos::new(3, 3, 1), 65_534).unwrap();
    let mut runtime = SimRuntime::new(
        review_content(),
        lot,
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let input = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: CALLER_GUID,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&input).unwrap();
    let caller = runtime.state().entities[&ObjectId(1)].info.reference;
    let physical_room = runtime
        .state()
        .world
        .lot
        .room_at(TilePos::new(3, 3, 1).center())
        .unwrap();
    assert!(runtime.state().world.lot.rooms().rooms[&physical_room].is_water);
    assert!(!runtime.state().world.lot.rooms().rooms[&physical_room].is_pool);
    let before = runtime.state().clone();
    let result = runtime
        .query_behavior(
            caller,
            BURN_ROUTINE,
            FrameContext::for_entity(caller, CALLER_GUID),
            vec![0; 4],
            32,
        )
        .unwrap();
    assert_eq!(result.stop, VmStop::Completed(PrimitiveExit::ReturnFalse));
    assert_eq!(runtime.state(), &before);
}

#[test]
fn finite_sleep_in_a_synchronous_condition_finishes_without_advancing_lot_time() {
    let original = review_content();
    let mut routines = original.routines().clone();
    let condition = RoutineKey {
        scope: RoutineScope::Private(CANDIDATE_GUID),
        id: 4097,
    };
    let countdown = ExpressionOperand {
        lhs: Variable::new(Scope::Parameters, 0),
        rhs: Variable::new(Scope::Literal, 2),
        is_signed: 0,
        operator: 5,
    }
    .encode();
    let shared_temp = ExpressionOperand {
        lhs: Variable::new(Scope::Temps, 0),
        rhs: Variable::new(Scope::Literal, 88),
        is_signed: 0,
        operator: 5,
    }
    .encode();
    routines
        .insert(
            condition,
            VmRoutine::new(
                4097,
                0,
                4,
                vec![
                    VmInstruction::new(2, 1, 255, countdown),
                    VmInstruction::new(0, 2, 255, [0; 8]),
                    VmInstruction::new(2, 254, 255, shared_temp),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut candidate = original.object(CANDIDATE_GUID).unwrap().clone();
    candidate.entry_conditions.insert(18, condition);
    let content = ContentSet::new(
        routines,
        vec![original.object(CALLER_GUID).unwrap().clone(), candidate],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = runtime_with_content(content);
    let before = runtime.state().clone();
    assert_eq!(
        find_stop(&runtime),
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(runtime.state(), &before);
    let caller = runtime.state().entities[&ObjectId(1)].info.reference;
    let tick = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: caller,
            routine: FIND_ROUTINE,
            context: FrameContext::for_entity(caller, CALLER_GUID),
            args: vec![0; 4],
            replace: true,
        }])
        .unwrap();
    let result = runtime.step(&tick).unwrap();
    assert!(result
        .events
        .iter()
        .all(|event| !matches!(event, RuntimeEvent::ThreadFault { .. })));
    assert_eq!(runtime.state().threads[&ObjectId(1)].temps[0], 88);
    assert_eq!(
        runtime.state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(runtime.state().clock.ticks, before.clock.ticks + 1);
    assert_eq!(
        runtime.state().scheduler.current_tick(),
        before.scheduler.current_tick() + 1
    );
    let mut expected_rng = before.rng;
    expected_rng.next(1); // The one Sleep completion draw.
    expected_rng.mix_entity_count(before.entities.len());
    assert_eq!(runtime.state().rng, expected_rng);
}

#[test]
fn condition_engine_queries_preserve_the_real_executing_avatar_frames() {
    let original = review_content();
    let mut routines = original.routines().clone();
    let condition = RoutineKey {
        scope: RoutineScope::Private(CANDIDATE_GUID),
        id: 4097,
    };
    let set = |lhs: Scope, index, rhs: Scope, value, next| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs: Variable::new(lhs, index),
                rhs: Variable::new(rhs, value),
                is_signed: 0,
                operator: 5,
            }
            .encode(),
        )
    };
    routines
        .insert(
            condition,
            VmRoutine::new(
                4097,
                0,
                4,
                vec![
                    set(Scope::StackObjectId, 0, Scope::Literal, 2, 1),
                    set(Scope::StackObject, 79, Scope::Literal, 1, 2),
                    set(Scope::Temps, 0, Scope::StackObject, 79, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut candidate = original.object(CANDIDATE_GUID).unwrap().clone();
    candidate.entry_conditions.insert(18, condition);
    let content = ContentSet::new(
        routines,
        vec![
            original.object(CALLER_GUID).unwrap().clone(),
            ObjectDefinition::new(300, 1),
            candidate,
        ],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawn = |guid, avatar, x| {
        AcceptedCommand::Spawn(SpawnSpec {
            guid,
            position: TilePos::new(x, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar,
        })
    };
    let input = runtime
        .next_tick(vec![
            spawn(CALLER_GUID, true, 3),
            spawn(300, false, 4),
            spawn(CANDIDATE_GUID, false, 6),
        ])
        .unwrap();
    runtime.step(&input).unwrap();
    let caller = runtime.state().entities[&ObjectId(1)].info.reference;
    let busy_callee = runtime.state().entities[&ObjectId(2)].info.reference;
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: caller,
            routine: FIND_ROUTINE,
            context: FrameContext {
                caller,
                callee: busy_callee,
                stack_object: ObjectId::NULL,
                stack_object_ref: None,
                code_owner: CALLER_GUID,
            },
            args: vec![0; 4],
            replace: true,
        }])
        .unwrap();
    let result = runtime.step(&input).unwrap();
    assert!(result
        .events
        .iter()
        .all(|event| !matches!(event, RuntimeEvent::ThreadFault { .. })));
    assert_eq!(
        runtime.state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(runtime.state().entities[&ObjectId(2)].object_data[79], 0,
        "the real avatar frame still uses object2 while the temporary condition's callee is object3");
    assert_eq!(runtime.state().threads[&ObjectId(1)].temps[0], 0);
}

#[test]
fn thread_overlay_preserves_multiple_removed_owners_and_rejects_stale_views() {
    use sim_core::runtime_memory::{
        is_in_use_with_threads, read_memory_with_threads, sync_projection_with_threads,
        write_memory_with_threads, ThreadView,
    };
    let content = review_content();
    let mut runtime = SimRuntime::new(
        content.clone(),
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawn = |guid, avatar, x| {
        AcceptedCommand::Spawn(SpawnSpec {
            guid,
            position: TilePos::new(x, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar,
        })
    };
    let input = runtime
        .next_tick(vec![
            spawn(CALLER_GUID, true, 3),
            spawn(CALLER_GUID, true, 4),
            spawn(CANDIDATE_GUID, false, 6),
        ])
        .unwrap();
    runtime.step(&input).unwrap();
    let mut state = runtime.state().clone();
    let first = state.entities[&ObjectId(1)].info.reference;
    let second = state.entities[&ObjectId(2)].info.reference;
    let target = state.entities[&ObjectId(3)].info.reference;
    let first_thread = state.threads.remove(&ObjectId(1)).unwrap();
    let mut second_thread = state.threads.remove(&ObjectId(2)).unwrap();
    second_thread.temps[0] = 444;
    second_thread
        .push_entry(
            content.routines(),
            FIND_ROUTINE,
            FrameContext {
                caller: second,
                callee: target,
                stack_object: target.object_id,
                stack_object_ref: Some(target),
                code_owner: CALLER_GUID,
            },
            vec![0; 4],
        )
        .unwrap();
    let mut current_check = VmThread::new(first, VmMode::Ts1);
    current_check.is_check = true;
    current_check
        .push_entry(
            content.routines(),
            FIND_ROUTINE,
            FrameContext::for_entity(first, CALLER_GUID),
            vec![0; 4],
        )
        .unwrap();
    let mut overlay = std::collections::BTreeMap::from([
        (first.object_id, first_thread),
        (second.object_id, second_thread),
    ]);
    let view = ThreadView {
        current: Some(&current_check),
        entity_threads: Some(&overlay),
    };
    assert!(is_in_use_with_threads(&state, target, view).unwrap());
    let temp = MemoryAddress::Entity {
        entity: second,
        field: EntityField::Temp,
        index: 0,
    };
    assert_eq!(
        read_memory_with_threads(&state, &content, &temp, view).unwrap(),
        444
    );
    let before = state.clone();
    assert!(matches!(
        write_memory_with_threads(&mut state, &content, &temp, 555, view),
        Err(VmFault::HostUnsupported(_))
    ));
    assert_eq!(
        state, before,
        "immutable overlay writes must not silently mutate stale storage"
    );
    sync_projection_with_threads(&mut state, target, view).unwrap();
    assert!(state.world.object(target).unwrap().in_use);

    // A stale overlay must not silently fall back to a valid stored thread.
    state
        .threads
        .insert(second.object_id, overlay[&second.object_id].clone());
    overlay.get_mut(&second.object_id).unwrap().owner.generation += 1;
    let view = ThreadView {
        current: Some(&current_check),
        entity_threads: Some(&overlay),
    };
    assert_eq!(
        is_in_use_with_threads(&state, target, view),
        Err(VmFault::StaleEntity(second))
    );
}

#[test]
fn sequential_conditions_in_one_primitive_share_the_latest_temp_banks() {
    let original = review_content();
    let mut routines = original.routines().clone();
    let condition = RoutineKey {
        scope: RoutineScope::Private(CANDIDATE_GUID),
        id: 4097,
    };
    let increment = |scope, amount, next| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs: Variable::new(scope, 0),
                rhs: Variable::new(Scope::Literal, amount),
                is_signed: 0,
                operator: 3,
            }
            .encode(),
        )
    };
    routines
        .insert(
            condition,
            VmRoutine::new(
                4097,
                0,
                4,
                vec![
                    increment(Scope::Temps, 1, 1),
                    increment(Scope::TempXl, 3, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut candidate = original.object(CANDIDATE_GUID).unwrap().clone();
    candidate.entry_conditions.insert(18, condition);
    let content = ContentSet::new(
        routines,
        vec![original.object(CALLER_GUID).unwrap().clone(), candidate],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = runtime_with_content(content);
    let input = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: CANDIDATE_GUID,
            position: TilePos::new(8, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&input).unwrap();
    let caller = runtime.state().entities[&ObjectId(1)].info.reference;
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: caller,
            routine: FIND_ROUTINE,
            context: FrameContext::for_entity(caller, CALLER_GUID),
            args: vec![0; 4],
            replace: true,
        }])
        .unwrap();
    let result = runtime.step(&input).unwrap();
    assert!(result
        .events
        .iter()
        .all(|event| !matches!(event, RuntimeEvent::ThreadFault { .. })));
    assert_eq!(
        runtime.state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(
        runtime.state().threads[&ObjectId(1)].temps[0],
        2,
        "both candidate checks must share the current opcode's evolving temp bank"
    );
    assert_eq!(runtime.state().threads[&ObjectId(1)].temp_xl[0], 6);
}

#[test]
fn created_child_init_reset_stops_the_removed_parent_thread() {
    let parent_routine = RoutineKey {
        scope: RoutineScope::Private(CALLER_GUID),
        id: 4102,
    };
    let child_init = RoutineKey {
        scope: RoutineScope::Private(CANDIDATE_GUID),
        id: 4096,
    };
    let set = |scope, index, value, next| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs: Variable::new(scope, index),
                rhs: Variable::new(Scope::Literal, value),
                is_signed: 0,
                operator: 5,
            }
            .encode(),
        )
    };
    let mut create = [0; 8];
    create[..4].copy_from_slice(&CANDIDATE_GUID.to_le_bytes());
    create[4] = 6; // Source out-of-world mode avoids placement/collision effects.
    let mut routines = RoutineStore::new();
    routines
        .insert(
            parent_routine,
            VmRoutine::new(
                4102,
                0,
                4,
                vec![
                    VmInstruction::new(42, 1, 255, create),
                    set(Scope::MyObjectAttributes, 0, 99, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    routines
        .insert(
            child_init,
            VmRoutine::new(
                4096,
                0,
                4,
                vec![
                    set(Scope::StackObjectId, 0, 1, 1),
                    set(Scope::StackObject, 8, 1 << 9, 254),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut child = ObjectDefinition::new(CANDIDATE_GUID, 1);
    child.entry_points.insert(0, child_init);
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(CALLER_GUID, 1), child],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let input = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: CALLER_GUID,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: true,
        })])
        .unwrap();
    runtime.step(&input).unwrap();
    let parent = runtime.state().entities[&ObjectId(1)].info.reference;
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: parent,
            routine: parent_routine,
            context: FrameContext::for_entity(parent, CALLER_GUID),
            args: vec![0; 4],
            replace: true,
        }])
        .unwrap();
    let result = runtime.step(&input).unwrap();
    assert!(result
        .events
        .iter()
        .all(|event| !matches!(event, RuntimeEvent::ThreadFault { .. })));
    assert_eq!(
        runtime.state().entities.len(),
        2,
        "the child must be created"
    );
    assert_ne!(
        runtime.state().entities[&ObjectId(1)].object_data[8] & (1 << 9),
        0
    );
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].attributes[0],
        0,
        "the child's Burning reset must prevent the parent’s next instruction"
    );
}

fn control_set(scope: Scope, index: i16, value: i16, next: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        ExpressionOperand {
            lhs: Variable::new(scope, index),
            rhs: Variable::new(Scope::Literal, value),
            is_signed: 0,
            operator: 5,
        }
        .encode(),
    )
}

fn named_control_runtime(
    parent: Vec<VmInstruction>,
    child: Vec<VmInstruction>,
) -> (SimRuntime, RoutineKey) {
    let parent_key = RoutineKey {
        scope: RoutineScope::Private(CALLER_GUID),
        id: 4096,
    };
    let child_key = RoutineKey {
        scope: RoutineScope::Private(CALLER_GUID),
        id: 4097,
    };
    let mut routines = RoutineStore::new();
    routines
        .insert(parent_key, VmRoutine::new(4096, 0, 4, parent).unwrap())
        .unwrap();
    routines
        .insert(child_key, VmRoutine::new(4097, 0, 4, child).unwrap())
        .unwrap();
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(CALLER_GUID, 1)],
        vec![],
        TuningSet::default(),
    )
    .unwrap()
    .with_strings(vec![((CALLER_GUID, 1), vec!["control child".into()])])
    .unwrap()
    .with_named_trees(vec![((CALLER_GUID, "control child".into()), child_key)])
    .unwrap();
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let input = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: CALLER_GUID,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&input).unwrap();
    (runtime, parent_key)
}

#[test]
fn named_child_sleep_consumes_the_selected_threads_existing_interrupt() {
    let named = VmInstruction::new(28, 2, 255, [1, 0, 0, 0, 1, 0, 0, 0]);
    let (mut runtime, parent_key) = named_control_runtime(
        vec![
            control_set(Scope::Temps, 0, 2, 1),
            named,
            control_set(Scope::Temps, 1, 77, 254),
        ],
        vec![
            VmInstruction::new(0, 1, 255, [0; 8]),
            control_set(Scope::Temps, 2, 88, 254),
        ],
    );
    let parent = runtime.state().entities[&ObjectId(1)].info.reference;
    let rng_before = runtime.state().rng.state();
    let input = runtime
        .next_tick(vec![
            AcceptedCommand::StartBehavior {
                entity: parent,
                routine: parent_key,
                context: FrameContext::for_entity(parent, CALLER_GUID),
                args: vec![0; 4],
                replace: true,
            },
            AcceptedCommand::Interrupt { entity: parent },
        ])
        .unwrap();
    let result = runtime.step(&input).unwrap();
    assert!(result
        .events
        .iter()
        .all(|event| !matches!(event, RuntimeEvent::ThreadFault { .. })));
    let thread = &runtime.state().threads[&ObjectId(1)];
    assert_eq!(thread.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(&thread.temps[1..3], &[77, 88]);
    assert!(
        !thread.interrupt,
        "the named child consumed the shared interrupt"
    );
    assert_eq!(thread.schedule_idle_start, 0);
    assert_eq!(
        runtime.state().rng.state(),
        rng_before.wrapping_add(1),
        "interrupted Sleep does not draw RNG; only one entity-count mix remains"
    );
}

#[test]
fn named_child_notify_remains_visible_to_the_parent_sleep() {
    let named = VmInstruction::new(28, 2, 255, [1, 0, 0, 0, 1, 0, 0, 0]);
    let (mut runtime, parent_key) = named_control_runtime(
        vec![
            control_set(Scope::Parameters, 0, 2, 1),
            named,
            VmInstruction::new(0, 3, 255, [0; 8]),
            control_set(Scope::Temps, 3, 99, 254),
        ],
        vec![VmInstruction::new(49, 254, 255, [0; 8])],
    );
    let parent = runtime.state().entities[&ObjectId(1)].info.reference;
    let rng_before = runtime.state().rng.state();
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: parent,
            routine: parent_key,
            context: FrameContext::for_entity(parent, CALLER_GUID),
            args: vec![0; 4],
            replace: true,
        }])
        .unwrap();
    let result = runtime.step(&input).unwrap();
    assert!(result
        .events
        .iter()
        .all(|event| !matches!(event, RuntimeEvent::ThreadFault { .. })));
    let thread = &runtime.state().threads[&ObjectId(1)];
    assert_eq!(
        thread.stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue),
        "the parent Sleep must observe the interrupt set by its named child"
    );
    assert_eq!(thread.temps[3], 99);
    assert!(!thread.interrupt);
    assert_eq!(thread.schedule_idle_start, 0);
    assert_eq!(runtime.state().rng.state(), rng_before.wrapping_add(1));
}
