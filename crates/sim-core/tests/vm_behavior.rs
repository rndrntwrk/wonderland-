#[path = "vm_host.rs"]
mod support;
use sim_core::ids::ObjectId;
use sim_core::vm::*;
use support::*;

#[test]
fn bounded_bhav_decoder_versions_and_branch_limits() {
    for version in [0x8000u16, 0x8001, 0x8002, 0x8003] {
        let mut bytes = version.to_le_bytes().to_vec();
        if version == 0x8003 {
            bytes.extend_from_slice(&[1, 4, 3, 0, 0, 7, 0, 1, 0, 0, 0]);
        } else {
            bytes.extend_from_slice(&[1, 0, 1, 4, 3, 0, 7, 0, 0, 0]);
        }
        bytes.extend_from_slice(&[255, 0, 254, 255, 0, 0, 0, 0, 0, 0, 0, 0]);
        let routine = VmRoutine::decode(256, &bytes, BhavLimits::default()).unwrap();
        assert_eq!(routine.instructions()[0].opcode, 255);
        assert_eq!(routine.arguments(), if version >= 0x8002 { 4 } else { 0 });
        assert!(VmRoutine::decode(256, &bytes[..bytes.len() - 1], BhavLimits::default()).is_err());
    }
    assert!(VmInstruction::decode(&[0; 11]).is_err());
    assert!(VmRoutine::new(256, 0, 0, vec![VmInstruction::new(0, 1, 255, [0; 8])]).is_err());
    assert!(VmRoutine::new(256, 0, 0, vec![instruction(255); 254]).is_err());
    assert!(VmRoutine::decode(256, &[0; 12], BhavLimits::default()).is_err());
}

#[test]
fn branch_253_falls_back_but_255_opcode_is_missing_success() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(255, 253, 255, [0; 8])]);
    assert_eq!(
        thread.run(&store, &mut host, 8).stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert_eq!(
        thread.diagnostics,
        vec![VmDiagnostic::MissingPrimitive { opcode: 255 }]
    );
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(255, 253, 253, [0; 8])]);
    assert_eq!(
        thread.run(&store, &mut host, 8).stop,
        VmStop::Completed(PrimitiveExit::Error)
    );
}

#[test]
fn calls_keep_code_owner_separate_and_resolve_all_three_resource_spaces() {
    let mut store = RoutineStore::new();
    store.bind_semiglobal(100, 900).unwrap();
    let root = RoutineKey {
        scope: RoutineScope::Global,
        id: 256,
    };
    for (key, opcode, args) in [
        (root, 4096, 4),
        (
            RoutineKey {
                scope: RoutineScope::Private(100),
                id: 4096,
            },
            8192,
            6,
        ),
        (
            RoutineKey {
                scope: RoutineScope::SemiGlobal(900),
                id: 8192,
            },
            300,
            4,
        ),
        (
            RoutineKey {
                scope: RoutineScope::Global,
                id: 300,
            },
            255,
            4,
        ),
    ] {
        store
            .insert(
                key,
                VmRoutine::new(
                    key.id,
                    1,
                    args,
                    vec![VmInstruction::new(
                        opcode,
                        254,
                        255,
                        [-1i16, 7, -1, -1]
                            .into_iter()
                            .flat_map(i16::to_le_bytes)
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap(),
                    )],
                )
                .unwrap(),
            )
            .unwrap();
    }
    let mut host = Host::new();
    let mut thread = VmThread::new(reference(1), VmMode::Tso);
    let context = FrameContext {
        caller: reference(1),
        callee: reference(2),
        stack_object: ObjectId(-7),
        stack_object_ref: None,
        code_owner: 100,
    };
    thread.temps = [8; 20];
    thread
        .push_entry(&store, root, context.clone(), vec![0; 4])
        .unwrap();
    thread.run(&store, &mut host, 1);
    assert_eq!(thread.frames.len(), 2);
    assert_eq!(thread.top().unwrap().args, vec![8, 7, 8, 8, 8, 8]);
    assert_eq!(thread.top().unwrap().locals.len(), 6);
    assert_eq!(thread.top().unwrap().context, context);
    thread.run(&store, &mut host, 1);
    assert_eq!(
        thread.top().unwrap().routine.scope,
        RoutineScope::SemiGlobal(900)
    );
    assert_eq!(thread.top().unwrap().context, context);
    assert_eq!(
        thread.run(&store, &mut host, 20).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
}

#[test]
fn all_zero_trailing_call_arguments_disable_temp_substitution() {
    let (mut store, mut thread, mut host) = setup(vec![VmInstruction::new(
        4096,
        254,
        255,
        [255, 255, 0, 0, 0, 0, 0, 0],
    )]);
    let key = RoutineKey {
        scope: RoutineScope::Private(0x1234),
        id: 4096,
    };
    store
        .insert(
            key,
            VmRoutine::new(4096, 0, 5, vec![instruction(255)]).unwrap(),
        )
        .unwrap();
    thread.temps = [90; 20];
    thread.run(&store, &mut host, 1);
    assert_eq!(thread.top().unwrap().args, vec![-1, 0, 0, 0, -1]);
}

#[test]
fn missing_routine_unwinds_error_while_empty_routine_hits_budget_without_advancing() {
    let (store, mut thread, mut host) = setup(vec![instruction(4096)]);
    assert_eq!(
        thread.run(&store, &mut host, 8).stop,
        VmStop::Completed(PrimitiveExit::Error)
    );
    assert!(matches!(
        thread.diagnostics[0],
        VmDiagnostic::MissingRoutine { .. }
    ));
    let (mut store, mut thread, mut host) = setup(vec![instruction(4096)]);
    store
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(0x1234),
                id: 4096,
            },
            VmRoutine::new(4096, 0, 0, vec![]).unwrap(),
        )
        .unwrap();
    assert_eq!(
        thread.run(&store, &mut host, 5).stop,
        VmStop::BudgetExhausted
    );
    assert_eq!(thread.top().unwrap().instruction_pointer, 0);
    assert_eq!(thread.frames.len(), 1);
}

#[test]
fn sleep_keeps_instruction_and_serializes_elapsed_countdown() {
    let (store, mut thread, mut host) = setup(vec![instruction(0)]);
    thread.top_mut().unwrap().args[0] = 3;
    host.tick = 10;
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Sleeping { until_tick: 13 }
    );
    assert_eq!(thread.top().unwrap().args[0], 2);
    assert_eq!(thread.top().unwrap().instruction_pointer, 0);
    let encoded = bincode::serialize(&thread).unwrap();
    let mut restored: VmThread = bincode::deserialize(&encoded).unwrap();
    restored.validate(&store).unwrap();
    host.tick = 13;
    thread.wake();
    restored.wake();
    let mut restored_host = host.clone();
    assert_eq!(
        thread.run(&store, &mut host, 10),
        restored.run(&store, &mut restored_host, 10)
    );
    assert_eq!(thread, restored);
    assert_eq!(host.rng, restored_host.rng);
    assert_eq!(host.random_bounds, vec![1]);
}

#[test]
fn interrupted_sleep_decrements_then_resumes_without_rng_cycle() {
    let (store, mut thread, mut host) = setup(vec![instruction(0)]);
    thread.top_mut().unwrap().args[0] = 10;
    thread.interrupt = true;
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert!(host.random_bounds.is_empty());
    assert!(!thread.interrupt);
    assert_eq!(thread.schedule_idle_start, 0);
}

#[test]
fn pending_host_request_roundtrip_rejects_duplicate_and_stale_responses() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 99 });
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Waiting { request_id: 99 }
    );
    let mut restored: VmThread =
        bincode::deserialize(&bincode::serialize(&thread).unwrap()).unwrap();
    restored.validate(&store).unwrap();
    assert!(restored
        .resume(98, VmResolution::complete(PrimitiveExit::GotoTrue))
        .is_err());
    restored
        .resume(99, VmResolution::complete(PrimitiveExit::GotoFalse))
        .unwrap();
    assert!(restored
        .resume(99, VmResolution::complete(PrimitiveExit::GotoTrue))
        .is_err());
    assert_eq!(
        restored.run(&store, &mut host, 10).stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert_eq!(host.requests.len(), 1);
}

#[test]
fn registered_unsupported_primitive_is_distinct_from_unregistered_legacy_success() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        47,
        254,
        255,
        [2, 0, 0, 0, 0, 0, 0, 0],
    )]);
    assert!(matches!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Faulted(VmFault::UnsupportedPrimitive { opcode: 47, .. })
    ));
    assert!(thread.diagnostics.is_empty());
    let (store, mut thread, mut host) = setup(vec![instruction(10)]);
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(
        thread.diagnostics,
        vec![VmDiagnostic::MissingPrimitive { opcode: 10 }]
    );
}

#[test]
fn reset_hook_interrupts_after_one_instruction_and_clears_pending_continuation() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 9 });
    host.reset_requested = true;
    let report = thread.run(&store, &mut host, 5);
    assert_eq!(report.instructions, 1);
    assert_eq!(report.stop, VmStop::Completed(PrimitiveExit::Interrupt));
    assert!(thread.frames.is_empty());
    assert!(thread.continuation.is_none());
    assert_eq!(thread.last_exit, PrimitiveExit::Interrupt);
    thread.validate(&store).unwrap();
}

#[test]
fn host_interrupt_hook_notifies_sleep_and_preserves_notification_after_terminal_instruction() {
    let (store, mut thread, mut host) = setup(vec![instruction(0)]);
    thread.top_mut().unwrap().args[0] = 10;
    host.interrupt_requested = true;
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Sleeping { until_tick: 11 }
    );
    assert!(thread.interrupt);
    host.tick = 2;
    thread.wake();
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert!(!thread.interrupt);
    assert!(host.random_bounds.is_empty());
    let (store, mut thread, mut host) = setup(vec![instruction(255)]);
    host.interrupt_requested = true;
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert!(thread.interrupt);
}

#[test]
fn host_temp_write_hook_updates_live_self_registers_before_next_instruction_and_after_return() {
    use sim_core::primitives::arithmetic::ExpressionOperand;
    let assign = |lhs, rhs| {
        ExpressionOperand {
            lhs,
            rhs,
            operator: 5,
            is_signed: 0,
        }
        .encode()
    };
    let (store, mut thread, mut host) = setup(vec![
        VmInstruction::new(28, 1, 255, [1, 0, 0, 0, 1, 0, 0, 0]),
        VmInstruction::new(
            2,
            2,
            255,
            assign(
                Variable::new(Scope::Temps, 4),
                Variable::new(Scope::Temps, 3),
            ),
        ),
        VmInstruction::new(
            2,
            3,
            255,
            assign(
                Variable::new(Scope::TempXl, 0),
                Variable::new(Scope::TempXl, 1),
            ),
        ),
        VmInstruction::new(28, 254, 255, [1, 0, 0, 0, 1, 0, 0, 0]),
    ]);
    host.temp_xl_writes = vec![(1, 123456)];
    host.named_result = Some(BoundRoutine {
        routine: thread.top().unwrap().routine,
        code_owner: 0x1234,
        arguments: 4,
    });
    // Simulate a nested check writing its removed ancestor through StackObject.Thread.TempRegisters.
    // This deliberately takes the host memory path, not the VM's same-thread scope13 shortcut.
    for value in [321, -7] {
        host.check_memory.push_back(vec![(
            MemoryAddress::Entity {
                entity: thread.owner,
                field: EntityField::Temp,
                index: 3,
            },
            value,
        )]);
    }
    assert_eq!(
        thread.run(&store, &mut host, 4).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(thread.temps[4], 321);
    assert_eq!(thread.temps[3], -7);
    assert_eq!(thread.temp_xl, [123456, 123456]);
    assert_eq!(host.checks.len(), 2);
    assert!(host.temp_writes.is_empty());
    thread.validate(&store).unwrap();
}

#[test]
fn invalid_host_temp_write_batches_are_bounded_and_atomic_across_short_and_xl_banks() {
    for (writes, xl_writes) in [
        (vec![(0, 99), (20, 7)], vec![(0, 99999)]),
        (vec![(0, 99); 21], vec![(0, 99999)]),
        (vec![(0, 99)], vec![(0, 99999), (2, 6)]),
        (vec![(0, 99)], vec![(0, 99999); 3]),
    ] {
        let (store, mut thread, mut host) = setup(vec![instruction(255)]);
        thread.temps[0] = 5;
        thread.temp_xl[0] = 1234;
        host.temp_writes = writes;
        host.temp_xl_writes = xl_writes;
        assert!(matches!(
            thread.run(&store, &mut host, 1).stop,
            VmStop::Faulted(VmFault::Bounds { .. } | VmFault::InvalidContent(_))
        ));
        assert_eq!(thread.temps[0], 5);
        assert_eq!(thread.temp_xl[0], 1234);
        assert!(host.temp_writes.is_empty());
        assert!(host.temp_xl_writes.is_empty());
        thread.validate(&store).unwrap();
    }
}

#[test]
fn nested_thread_control_is_applied_before_parent_returns_or_schedules_next_tick() {
    for terminal in [false, true] {
        let call = VmInstruction::new(
            28,
            if terminal { 254 } else { 1 },
            255,
            [1, 0, 0, 0, 1, 0, 0, 0],
        );
        let (store, mut thread, mut host) = setup(if terminal {
            vec![call]
        } else {
            vec![call, instruction(255)]
        });
        thread.interrupt = true;
        host.named_result = Some(BoundRoutine {
            routine: thread.top().unwrap().routine,
            code_owner: 0x1234,
            arguments: 4,
        });
        host.check_thread_controls
            .push_back(Some((false, 77, PrimitiveExit::ReturnFalse)));
        let report = thread.run(&store, &mut host, 1);
        assert!(!thread.interrupt);
        assert_eq!(thread.schedule_idle_start, 77);
        assert_eq!(
            thread.last_exit,
            if terminal {
                PrimitiveExit::ReturnTrue
            } else {
                PrimitiveExit::ReturnFalse
            }
        );
        assert_eq!(
            report.stop,
            if terminal {
                VmStop::Completed(PrimitiveExit::ReturnTrue)
            } else {
                VmStop::BudgetExhausted
            }
        );
        thread.validate(&store).unwrap();
    }
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.control_on_request = Some((true, 99, PrimitiveExit::ReturnFalse));
    host.responses.push_back(HostResponse::NextTick);
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Sleeping { until_tick: 2 }
    );
    assert!(thread.interrupt);
    assert_eq!(thread.last_exit, PrimitiveExit::ReturnFalse);
    assert_eq!(thread.schedule_idle_start, 1);
    thread.validate(&store).unwrap();
}

#[test]
fn next_tick_branch_advances_first_but_terminal_return_does_not_add_a_yield() {
    let (store, mut thread, mut host) =
        setup(vec![VmInstruction::new(45, 1, 1, [0; 8]), instruction(255)]);
    host.responses
        .push_back(HostResponse::Complete(PrimitiveExit::GotoFalseNextTick));
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Sleeping { until_tick: 2 }
    );
    assert_eq!(thread.top().unwrap().instruction_pointer, 1);
    host.tick = 2;
    thread.wake();
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Complete(PrimitiveExit::GotoFalseNextTick));
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
}

#[test]
fn error_return_unwinds_all_normal_callers_and_retry_discards_boolean() {
    let (mut store, mut thread, mut host) = setup(vec![instruction(4096)]);
    let key = RoutineKey {
        scope: RoutineScope::Private(0x1234),
        id: 4096,
    };
    store
        .insert(
            key,
            VmRoutine::new(4096, 0, 4, vec![VmInstruction::new(255, 253, 253, [0; 8])]).unwrap(),
        )
        .unwrap();
    assert_eq!(
        thread.run(&store, &mut host, 10).stop,
        VmStop::Completed(PrimitiveExit::Error)
    );
    assert!(thread.frames.is_empty());
    let (mut store, mut thread, mut host) = setup(vec![instruction(4096)]);
    store
        .insert(
            key,
            VmRoutine::new(4096, 0, 4, vec![instruction(255)]).unwrap(),
        )
        .unwrap();
    thread.run(&store, &mut host, 1);
    thread.top_mut().unwrap().special_result = SpecialResult::Retry;
    thread.run(&store, &mut host, 1);
    assert_eq!(thread.frames.len(), 1);
    assert_eq!(thread.top().unwrap().instruction_pointer, 0);
    assert_eq!(thread.stop, VmStop::BudgetExhausted);
}

#[test]
fn recursive_calls_and_temp_argument_substitution_have_explicit_bounds() {
    let (store, mut thread, mut host) = setup(vec![instruction(256)]);
    assert_eq!(
        thread.run(&store, &mut host, 300).stop,
        VmStop::Faulted(VmFault::StackLimit)
    );
    assert_eq!(thread.frames.len(), MAX_STACK_DEPTH);
    let (mut store, mut thread, mut host) =
        setup(vec![VmInstruction::new(4096, 254, 255, [255; 8])]);
    store
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(0x1234),
                id: 4096,
            },
            VmRoutine::new(4096, 0, 21, vec![instruction(255)]).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        thread.run(&store, &mut host, 2).stop,
        VmStop::Faulted(VmFault::Bounds { index: 20, .. })
    ));
    assert_eq!(thread.frames.len(), 1);
}

#[test]
fn checkpoint_of_resolved_but_unconsumed_continuation_is_valid_after_zero_budget() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 3 });
    thread.run(&store, &mut host, 1);
    thread
        .resume(3, VmResolution::complete(PrimitiveExit::GotoTrue))
        .unwrap();
    thread.run(&store, &mut host, 0);
    assert_eq!(thread.stop, VmStop::BudgetExhausted);
    thread.validate(&store).unwrap();
    let mut restored: VmThread =
        bincode::deserialize(&bincode::serialize(&thread).unwrap()).unwrap();
    assert_eq!(
        restored.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(host.requests.len(), 1);
}
