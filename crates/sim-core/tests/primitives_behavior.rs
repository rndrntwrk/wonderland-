#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::primitives::{behavior::*, flow::idle_for_input};
use sim_core::vm::*;
use support::*;

fn bind(id: u16, args: u16) -> BoundRoutine {
    BoundRoutine {
        routine: RoutineKey {
            scope: RoutineScope::Private(0x5678),
            id,
        },
        code_owner: 0x5678,
        arguments: args,
    }
}
fn set_stack(thread: &mut VmThread, host: &mut Host, id: i16) {
    write_variable(thread, host, Variable::new(Scope::StackObjectId, 0), id).unwrap();
}
fn entry(action: Option<BoundRoutine>, condition: Option<BoundRoutine>) -> BehaviorEntry {
    BehaviorEntry {
        action_declared: true,
        action,
        condition_declared: condition.is_some(),
        condition,
    }
}
fn result(exit: PrimitiveExit, temp0: i16, copy_back: bool) -> CheckResult {
    let mut temps = [0; 20];
    temps[0] = temp0;
    CheckResult {
        accepted: exit == PrimitiveExit::ReturnTrue,
        exit,
        aborting: false,
        temps,
        temp_xl: [90000, -1],
        copy_back,
        thread_control: None,
    }
}

#[test]
fn functional_tree_pushes_exact_declared_args_and_changes_callee_owner_and_icon() {
    let (mut store, mut thread, mut host) = setup(vec![VmInstruction::new(
        20,
        254,
        255,
        [0, 0, 128, 0, 0, 0, 0, 0],
    )]);
    let action = bind(4096, 0);
    let condition = bind(4097, 20);
    store
        .insert(
            action.routine,
            VmRoutine::new(4096, 3, 0, vec![instruction(255)]).unwrap(),
        )
        .unwrap();
    host.behavior_entries.insert(
        (ObjectId(2), 18),
        entry(Some(action.clone()), Some(condition)),
    );
    set_stack(&mut thread, &mut host, 2);
    thread.top_mut().unwrap().action_tree = true;
    host.check_results
        .push_back(result(PrimitiveExit::ReturnTrue, 88, true));
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::BudgetExhausted
    );
    let frame = thread.top().unwrap();
    assert!(frame.args.is_empty());
    assert_eq!(frame.locals.len(), 3);
    assert_eq!(frame.context.caller, reference(1));
    assert_eq!(frame.context.callee, reference(2));
    assert_eq!(frame.context.code_owner, 0x5678);
    assert!(frame.action_tree);
    assert_eq!(thread.temps[0], 88);
    assert_eq!(thread.temp_xl[0], 90000);
    assert_eq!(host.checks[0].args, vec![0; 4]);
    assert_eq!(host.checks[0].run_in_owner, None);
    assert!(!host.checks[0].use_current_thread);
    assert_eq!(host.icons, vec![(reference(1), reference(2))]);
    let mut restored: VmThread =
        bincode::deserialize(&bincode::serialize(&thread).unwrap()).unwrap();
    restored.validate(&store).unwrap();
    assert_eq!(
        restored.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
}

#[test]
fn functional_missing_condition_falls_through_but_failed_check_and_missing_action_branch_false() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    host.behavior_entries.insert(
        (ObjectId(2), 18),
        BehaviorEntry {
            condition_declared: true,
            ..entry(Some(bind(4096, 4)), None)
        },
    );
    assert!(matches!(
        run_functional(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveOutcome::Call(_)
    ));
    assert!(host.checks.is_empty());
    host.behavior_entries
        .get_mut(&(ObjectId(2), 18))
        .unwrap()
        .condition = Some(bind(4097, 4));
    host.check_results
        .push_back(result(PrimitiveExit::ReturnFalse, 55, true));
    assert_eq!(
        run_functional(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse)
    );
    assert_eq!(thread.temps[0], 55);
    host.behavior_entries
        .get_mut(&(ObjectId(2), 18))
        .unwrap()
        .action = None;
    assert_eq!(
        run_functional(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse)
    );
    set_stack(&mut thread, &mut host, 0);
    assert_eq!(
        run_functional(&mut thread, &mut host, [255, 255, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse)
    );
    assert!(matches!(
        run_functional(&mut thread, &mut host, [15, 0, 0, 0, 0, 0, 0, 0]),
        Err(VmFault::Bounds { .. })
    ));
}

#[test]
fn named_subroutine_preserves_callee_and_first_four_temps_without_substituting_extra_minus_ones() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    thread.temps[..6].copy_from_slice(&[-1, 2, 3, 4, 500, 600]);
    host.named_result = Some(bind(4096, 6));
    let PrimitiveOutcome::Call(call) =
        run_named(&mut thread, &mut host, [0x23, 1, 1, 99, 0, 2, 0, 0]).unwrap()
    else {
        panic!("call");
    };
    assert_eq!(call.context.caller, reference(1));
    assert_eq!(call.context.callee, reference(1));
    assert_eq!(call.context.stack_object_ref, Some(reference(2)));
    assert_eq!(call.context.code_owner, 0x5678);
    assert_eq!(call.args, vec![-1, 2, 3, 4, -1, -1]);
    let lookup = &host.name_lookups.borrow()[0];
    assert_eq!(
        (
            lookup.string_table,
            lookup.string_scope,
            lookup.string_index
        ),
        (0x123, 1, -1)
    );
}

#[test]
fn named_synchronous_call_selects_execution_owner_and_copies_provider_registers() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    host.named_result = Some(bind(4096, 0));
    let mut shared = result(PrimitiveExit::ReturnTrue, 17, true);
    shared.thread_control = Some((true, 81));
    host.check_results.push_back(shared);
    assert_eq!(
        run_named(&mut thread, &mut host, [1, 0, 0, 0, 1, 0, 0, 0]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue)
    );
    assert_eq!(host.checks[0].run_in_owner, Some(reference(1)));
    assert!(host.checks[0].use_current_thread);
    assert_eq!(host.checks[0].context.callee, reference(2));
    assert_eq!(thread.temps[0], 17);
    assert!(thread.interrupt);
    assert_eq!(thread.schedule_idle_start, 81);
    assert_eq!(thread.last_exit, PrimitiveExit::ReturnTrue);
    let mut independent = result(PrimitiveExit::Error, 18, false);
    independent.thread_control = Some((false, 9));
    host.check_results.push_back(independent);
    assert_eq!(
        run_named(&mut thread, &mut host, [1, 0, 0, 0, 1, 99, 0, 0]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoFalse)
    );
    assert_eq!(host.checks[1].run_in_owner, Some(reference(2)));
    assert!(!host.checks[1].use_current_thread);
    assert_eq!(thread.temps[0], 17);
    assert!(thread.interrupt);
    assert_eq!(thread.schedule_idle_start, 81);
    assert_eq!(thread.last_exit, PrimitiveExit::ReturnTrue);
    set_stack(&mut thread, &mut host, 1);
    run_named(&mut thread, &mut host, [1, 0, 0, 0, 1, 1, 0, 0]).unwrap();
    assert_eq!(host.checks[2].run_in_owner, Some(reference(1)));
    assert!(!host.checks[2].use_current_thread);
    let mut aborted = result(PrimitiveExit::Error, 0, false);
    aborted.aborting = true;
    host.check_results.push_back(aborted);
    assert_eq!(
        run_named(&mut thread, &mut host, [1, 0, 0, 0, 1, 0, 0, 0]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::Error)
    );
}

#[test]
fn find_best_uses_source_thresholds_distance_and_stable_first_entity_ties() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    for id in 2..=6 {
        host.add(id, id as u32, 0, 0);
        host.behavior_entries
            .insert((ObjectId(id), 28), entry(None, None));
        host.memory
            .insert((ObjectId(id), EntityField::ObjectData, 39), 800);
    }
    host.entities.get_mut(&ObjectId(2)).unwrap().position.x = 48;
    host.memory
        .insert((ObjectId(2), EntityField::ObjectData, 39), 810);
    host.memory
        .insert((ObjectId(4), EntityField::ObjectData, 39), 999);
    host.memory
        .insert((ObjectId(4), EntityField::ObjectData, 25), 1);
    host.memory
        .insert((ObjectId(5), EntityField::ObjectData, 39), 1000);
    host.function_states.insert(
        reference(5),
        FunctionEntityState {
            broken: true,
            ..FunctionEntityState::default()
        },
    );
    assert_eq!(
        find_best(&mut thread, &mut host, [11, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(3));
    thread.mode = VmMode::Ts1;
    find_best(&mut thread, &mut host, [11, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(5));
    for id in 2..=6 {
        host.memory
            .insert((ObjectId(id), EntityField::ObjectData, 39), 799);
    }
    assert_eq!(
        find_best(&mut thread, &mut host, [11, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(5));
}

#[test]
fn find_best_surface_fallback_depends_on_declared_condition_and_rereads_position_after_check() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.behavior_entries
        .insert((ObjectId(2), 18), entry(None, None));
    host.memory
        .insert((ObjectId(2), EntityField::ObjectData, 31), 100);
    host.memory.insert((ObjectId(2), EntityField::Slot, 0), 1);
    assert_eq!(
        find_best(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    host.behavior_entries
        .get_mut(&(ObjectId(2), 18))
        .unwrap()
        .condition_declared = true;
    assert_eq!(
        find_best(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveExit::GotoTrue
    ); // Declared but absent BHAV skips the surface-slot fallback.
    host.behavior_entries
        .get_mut(&(ObjectId(2), 18))
        .unwrap()
        .condition = Some(bind(4097, 4));
    host.entities.get_mut(&ObjectId(2)).unwrap().position.x = 30000;
    host.check_positions.push_back(Some((
        ObjectId(2),
        VmPosition {
            x: 0,
            y: 0,
            level: 1,
        },
    )));
    host.add(3, 3, 0, 0);
    host.behavior_entries
        .insert((ObjectId(3), 18), entry(None, None));
    host.memory
        .insert((ObjectId(3), EntityField::ObjectData, 31), 99);
    find_best(&mut thread, &mut host, [0; 8]).unwrap();
    assert_eq!(thread.top().unwrap().context.stack_object, ObjectId(2));
    assert_eq!(host.checks.len(), 1);
}

#[test]
fn tso_repair_requires_broken_game_object_without_ts1_score_threshold() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.behavior_entries
        .insert((ObjectId(2), 29), entry(None, None));
    host.memory
        .insert((ObjectId(2), EntityField::ObjectData, 15), -20);
    host.function_states.insert(
        reference(2),
        FunctionEntityState {
            broken: true,
            ..FunctionEntityState::default()
        },
    );
    assert_eq!(
        find_best(&mut thread, &mut host, [14, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoTrue
    );
    thread.mode = VmMode::Ts1;
    assert_eq!(
        find_best(&mut thread, &mut host, [14, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
}

#[test]
fn idle_decrements_before_push_notification_and_interrupt_without_random_cycles() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    thread.top_mut().unwrap().args[0] = 8;
    thread.schedule_idle_start = 5;
    host.tick = 10;
    thread.interrupt = true;
    let call = RoutineCall {
        routine: bind(4096, 0).routine,
        context: thread.top().unwrap().context.clone(),
        args: vec![],
        action_tree: true,
        special_result: SpecialResult::Interaction {
            run_immediately: false,
        },
    };
    host.idle_decisions
        .push_back(IdleDecision::Push(call.clone()));
    assert_eq!(
        idle_for_input(&mut thread, &mut host, [0, 0, 1, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveOutcome::Call(call)
    );
    assert_eq!(thread.top().unwrap().args[0], 3);
    assert!(thread.interrupt);
    assert_eq!(thread.schedule_idle_start, 0);
    host.idle_decisions.push_back(IdleDecision::Notified);
    assert_eq!(
        idle_for_input(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue)
    );
    assert!(thread.interrupt);
    assert_eq!(
        host.memory[&(ObjectId(1), EntityField::ObjectData, 8)] & 64,
        64
    );
    assert_eq!(
        idle_for_input(&mut thread, &mut host, [0; 8]).unwrap(),
        PrimitiveOutcome::Exit(PrimitiveExit::GotoTrue)
    );
    assert!(!thread.interrupt);
    assert!(host.random_bounds.is_empty());
}

#[test]
fn idle_allow_push_is_exact_one_and_action_tree_restriction_does_not_change_wake_cadence() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        17,
        254,
        255,
        [0, 0, 1, 0, 0, 0, 0, 0],
    )]);
    thread.top_mut().unwrap().args[0] = 10;
    thread.top_mut().unwrap().action_tree = true;
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Sleeping { until_tick: 2 }
    );
    assert!(!host.idle_calls[0].1);
    thread.mode = VmMode::Ts1;
    thread.wake();
    host.tick = 2;
    thread.run(&store, &mut host, 1);
    assert!(host.idle_calls[1].1);
    thread.wake();
    thread.schedule_idle_start = 0;
    thread.top_mut().unwrap().args[0] = 5;
    assert_eq!(
        idle_for_input(&mut thread, &mut host, [0, 0, 2, 0, 0, 0, 0, 0]).unwrap(),
        PrimitiveOutcome::SleepUntil(7)
    );
    assert!(!host.idle_calls[2].1);
    assert!(host.random_bounds.is_empty());
}

#[test]
fn push_interaction_honors_priority_flags_and_checks_missing_action_before_icon_index() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    host.add(3, 3, 0, 0);
    host.entities.get_mut(&ObjectId(3)).unwrap().is_avatar = true;
    set_stack(&mut thread, &mut host, 3);
    thread.top_mut().unwrap().locals[2] = 2;
    thread.top_mut().unwrap().locals[1] = 1;
    thread.top_mut().unwrap().action_tree = true;
    host.memory
        .insert((ObjectId(1), EntityField::PersonData, 33), -5);
    push_interaction(&thread, &mut host, [7, 2, 0, 1 | 2 | 4 | 128, 1, 0, 0, 0]).unwrap();
    let request = &host.queued[0];
    assert_eq!(
        (
            request.source,
            request.target,
            request.priority,
            request.mode
        ),
        (reference(2), reference(3), 1, QueueMode::Normal)
    );
    assert_eq!(request.icon, Some(reference(1)));
    assert!(
        request.push_head
            && request.push_tail
            && request.skip_permissions
            && request.immediate_result_chooser
    );
    for (encoded, priority, mode) in [
        (1, 100, QueueMode::Normal),
        (2, 2, QueueMode::Normal),
        (3, 50, QueueMode::Normal),
        (4, 40, QueueMode::ParentIdle),
        (5, 30, QueueMode::ParentExit),
        (6, 0, QueueMode::Idle),
    ] {
        push_interaction(&thread, &mut host, [7, 2, encoded, 2, 0, 0, 0, 0]).unwrap();
        let request = host.queued.last().unwrap();
        assert_eq!((request.priority, request.mode), (priority, mode));
    }
    host.available_interactions = false;
    assert_eq!(
        push_interaction(&thread, &mut host, [7, 2, 0, 1 | 2, 255, 0, 0, 0]).unwrap(),
        PrimitiveExit::GotoFalse
    );
    host.available_interactions = true;
    assert!(matches!(
        push_interaction(&thread, &mut host, [7, 2, 0, 1 | 2, 255, 0, 0, 0]),
        Err(VmFault::Bounds { .. })
    ));
}

#[test]
fn test_interacting_uses_active_action_target_and_not_the_current_callee() {
    let (_, mut thread, mut host) = setup(vec![instruction(255)]);
    set_stack(&mut thread, &mut host, 2);
    host.interaction_states.insert(
        reference(1),
        InteractionState {
            action_tree: true,
            callee: Some(reference(2)),
        },
    );
    assert_eq!(
        test_interacting(&thread, &host).unwrap(),
        PrimitiveExit::GotoTrue
    );
    host.interaction_states
        .get_mut(&reference(1))
        .unwrap()
        .action_tree = false;
    assert_eq!(
        test_interacting(&thread, &host).unwrap(),
        PrimitiveExit::GotoFalse
    );
}
