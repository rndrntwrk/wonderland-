#[path = "vm_host.rs"]
mod support;
use sim_core::vm::*;
use support::*;

#[test]
fn animation_operand_retargets_parameter_ids_and_writes_event_to_selected_local() {
    let operand = [1, 0, 3, 0, 0, 4 | 32 | 64 | 2, 5, 0];
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(44, 254, 255, operand)]);
    thread.top_mut().unwrap().args[1] = 7;
    host.responses.push_back(HostResponse::AnimationEvent(19));
    thread.run(&store, &mut host, 1);
    assert_eq!(thread.stop, VmStop::Completed(PrimitiveExit::ReturnFalse));
    let HostRequest::Animation(request) = &host.requests[0] else {
        panic!("animation request");
    };
    assert_eq!(request.animation_id, 7);
    assert_eq!(request.scope, 65536);
    assert_eq!(request.mode, 0);
    assert!(request.backwards && request.hurryable);
    assert_eq!(request.expected_events, 5);
    assert_eq!(request.event_local, 3);
    assert!(!request.store_event_in_parameter);
}

#[test]
fn animation_event_branch_preserves_frame_for_resume_and_wait_preserves_ip() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        44,
        254,
        0,
        [1, 0, 0, 0, 1, 0, 1, 0],
    )]);
    host.responses.extend([
        HostResponse::NextTick,
        HostResponse::AnimationEvent(8),
        HostResponse::Complete(PrimitiveExit::GotoTrue),
    ]);
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Sleeping { until_tick: 2 }
    );
    assert_eq!(thread.top().unwrap().instruction_pointer, 0);
    let mut restored: VmThread =
        bincode::deserialize(&bincode::serialize(&thread).unwrap()).unwrap();
    restored.validate(&store).unwrap();
    host.tick = 2;
    restored.wake();
    assert_eq!(
        restored.run(&store, &mut host, 1).stop,
        VmStop::BudgetExhausted
    );
    assert_eq!(restored.top().unwrap().args[0], 8);
    assert_eq!(
        restored.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
}

#[test]
fn transfer_uses_resolved_big_amount_and_checkpoint_completion_writes_xl() {
    let operand = [3, 42, 1, 0, 0, 0, 5, 9];
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(25, 254, 255, operand)]);
    thread.temp_xl[1] = 120000;
    host.responses
        .push_back(HostResponse::Pending { request_id: 77 });
    assert_eq!(
        thread.run(&store, &mut host, 3).stop,
        VmStop::Waiting { request_id: 77 }
    );
    let HostRequest::External(request) = &host.requests[0] else {
        panic!("external request");
    };
    assert_eq!(request.amount, Some(120000));
    assert!(!request.is_check);
    let mut restored: VmThread =
        bincode::deserialize(&bincode::serialize(&thread).unwrap()).unwrap();
    restored
        .resume(
            77,
            VmResolution {
                response: HostResponse::Complete(PrimitiveExit::GotoFalse),
                writes: vec![RegisterWrite {
                    target: RegisterTarget::TempXl,
                    index: 0,
                    value: 0,
                }],
            },
        )
        .unwrap();
    assert_eq!(
        restored.run(&store, &mut host, 3).stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert_eq!(restored.temp_xl[0], 0);
    assert_eq!(host.requests.len(), 1);
}

#[test]
fn check_tree_cannot_request_non_test_transfer() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        25,
        254,
        255,
        [0, 0, 10, 0, 0, 0, 0, 9],
    )]);
    thread.is_check = true;
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert!(host.requests.is_empty());
}

#[test]
fn motive_change_decodes_scopes_and_clear_all_skips_invalid_operands() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        29,
        254,
        255,
        [8, 7, 3, 2, 1, 0, 75, 0],
    )]);
    thread.temps[1] = -12;
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    let HostRequest::MotiveChange(request) = &host.requests[0] else {
        panic!("motive request");
    };
    assert_eq!(request.raw_rate, -12);
    assert_eq!(request.raw_max, 75);
    assert_eq!(request.motive, 3);
    assert!(request.once);
    assert!(!request.clear_all);
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        29,
        254,
        255,
        [255, 255, 255, 1, 255, 255, 255, 255],
    )]);
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    let HostRequest::MotiveChange(request) = &host.requests[0] else {
        panic!("motive request");
    };
    assert!(request.clear_all);
    assert_eq!(request.raw_rate, 0);
}

#[test]
fn routing_check_trees_return_false_and_source_reach_mouth_is_explicit_fault() {
    for opcode in [27, 45] {
        let (store, mut thread, mut host) = setup(vec![instruction(opcode)]);
        thread.is_check = true;
        assert_eq!(
            thread.run(&store, &mut host, 1).stop,
            VmStop::Completed(PrimitiveExit::ReturnFalse)
        );
        assert!(host.requests.is_empty());
    }
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
    assert!(host.requests.is_empty());
}

#[test]
fn response_register_writes_are_atomic_when_a_later_index_is_invalid() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 8 });
    thread.run(&store, &mut host, 1);
    thread.temps[0] = 2;
    let before = thread.clone();
    let result = thread.resume(
        8,
        VmResolution {
            response: HostResponse::Complete(PrimitiveExit::GotoTrue),
            writes: vec![
                RegisterWrite {
                    target: RegisterTarget::Temp,
                    index: 0,
                    value: 99,
                },
                RegisterWrite {
                    target: RegisterTarget::Local,
                    index: 600,
                    value: 1,
                },
            ],
        },
    );
    assert!(matches!(result, Err(VmFault::Bounds { .. })));
    assert_eq!(thread, before);
    assert_eq!(thread.temps[0], 2);
}

#[test]
fn restored_request_context_and_frame_reference_validation_rejects_forgery() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 8 });
    thread.run(&store, &mut host, 1);
    thread.validate(&store).unwrap();
    let HostRequest::Route(request) = &mut thread.continuation.as_mut().unwrap().request else {
        panic!("route");
    };
    request.context.callee = reference(2);
    assert!(thread.validate(&store).is_err());
    thread.continuation = None;
    thread.stop = VmStop::Ready;
    thread.top_mut().unwrap().context.callee.generation = 0;
    assert!(thread.validate(&store).is_err());
}

#[test]
fn synchronous_provider_writes_update_registers_and_stack_cache_before_the_next_instruction() {
    let (store, mut thread, mut host) =
        setup(vec![VmInstruction::new(45, 1, 1, [0; 8]), instruction(255)]);
    host.entities
        .get_mut(&sim_core::ids::ObjectId(2))
        .unwrap()
        .reference
        .generation = 7;
    host.responses.push_back(HostResponse::CompleteWithWrites {
        exit: PrimitiveExit::GotoTrue,
        writes: vec![
            RegisterWrite {
                target: RegisterTarget::Temp,
                index: 0,
                value: 65535,
            },
            RegisterWrite {
                target: RegisterTarget::TempXl,
                index: 1,
                value: 90000,
            },
            RegisterWrite {
                target: RegisterTarget::Local,
                index: 2,
                value: 11,
            },
            RegisterWrite {
                target: RegisterTarget::StackObjectId,
                index: 0,
                value: 2,
            },
        ],
    });
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::BudgetExhausted
    );
    assert_eq!(thread.temps[0], -1);
    assert_eq!(thread.temp_xl[1], 90000);
    assert_eq!(thread.top().unwrap().locals[2], 11);
    assert_eq!(
        thread.top().unwrap().context.stack_object_ref,
        Some(sim_core::ids::EntityRef {
            object_id: sim_core::ids::ObjectId(2),
            generation: 7
        })
    );
    let HostRequest::Route(request) = &host.requests[0] else {
        panic!("route");
    };
    assert_eq!(request.locals.len(), 8);
    assert_eq!(request.locals[2], 0);
    thread.run(&store, &mut host, 1);
    assert_eq!(host.observations[0].temps[0], 0);
    assert_eq!(host.observations[1].temps[0], -1);
}

#[test]
fn synchronous_write_batch_rejects_invalid_later_index_without_partial_register_mutation() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    thread.temps[0] = 2;
    host.responses.push_back(HostResponse::CompleteWithWrites {
        exit: PrimitiveExit::GotoTrue,
        writes: vec![
            RegisterWrite {
                target: RegisterTarget::Temp,
                index: 0,
                value: 99,
            },
            RegisterWrite {
                target: RegisterTarget::Parameter,
                index: 4,
                value: 1,
            },
        ],
    });
    assert!(matches!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Faulted(VmFault::Bounds { .. })
    ));
    assert_eq!(thread.temps[0], 2);
    assert_eq!(thread.top().unwrap().instruction_pointer, 0);
}

#[test]
fn resumed_embedded_writes_have_a_combined_limit_and_restore_rejects_forged_register_indices() {
    let (store, mut thread, mut host) = setup(vec![instruction(45)]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 5 });
    thread.run(&store, &mut host, 1);
    let write = RegisterWrite {
        target: RegisterTarget::Temp,
        index: 0,
        value: 1,
    };
    assert!(thread
        .resume(
            5,
            VmResolution {
                response: HostResponse::CompleteWithWrites {
                    exit: PrimitiveExit::GotoTrue,
                    writes: vec![write.clone(); 513]
                },
                writes: vec![write; 512]
            }
        )
        .is_err());
    let mut forged = thread.clone();
    forged.stop = VmStop::Ready;
    forged.continuation.as_mut().unwrap().resolution = Some(VmResolution {
        response: HostResponse::CompleteWithWrites {
            exit: PrimitiveExit::GotoTrue,
            writes: vec![RegisterWrite {
                target: RegisterTarget::StackObjectId,
                index: 1,
                value: 2,
            }],
        },
        writes: vec![],
    });
    assert!(forged.validate(&store).is_err());
    thread
        .resume(
            5,
            VmResolution {
                response: HostResponse::CompleteWithWrites {
                    exit: PrimitiveExit::GotoTrue,
                    writes: vec![RegisterWrite {
                        target: RegisterTarget::TempXl,
                        index: 1,
                        value: 90000,
                    }],
                },
                writes: vec![RegisterWrite {
                    target: RegisterTarget::Temp,
                    index: 0,
                    value: 77,
                }],
            },
        )
        .unwrap();
    thread.validate(&store).unwrap();
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(thread.temps[0], 77);
    assert_eq!(thread.temp_xl[1], 90000);
}

#[test]
fn look_towards_check_guards_body_modes_after_required_entity_dereferences() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        22,
        254,
        255,
        [2, 0, 0, 0, 0, 0, 0, 0],
    )]);
    thread.is_check = true;
    assert_eq!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Completed(PrimitiveExit::ReturnFalse)
    );
    assert!(host.requests.is_empty());
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        22,
        254,
        255,
        [4, 0, 0, 0, 0, 0, 0, 0],
    )]);
    thread.is_check = true;
    write_variable(
        &mut thread,
        &mut host,
        Variable::new(Scope::StackObjectId, 0),
        0,
    )
    .unwrap();
    assert!(matches!(
        thread.run(&store, &mut host, 1).stop,
        VmStop::Faulted(VmFault::MissingEntity(_))
    ));
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        22,
        254,
        255,
        [1, 0, 0, 0, 0, 0, 0, 0],
    )]);
    thread.is_check = true;
    thread.run(&store, &mut host, 1);
    assert_eq!(host.requests.len(), 1);
}

#[test]
fn continuation_restore_binds_request_family_operand_and_literal_amount_to_source_instruction() {
    let (store, mut thread, mut host) = setup(vec![VmInstruction::new(
        25,
        254,
        255,
        [0, 0, 5, 0, 0, 0, 0, 0],
    )]);
    host.responses
        .push_back(HostResponse::Pending { request_id: 77 });
    thread.run(&store, &mut host, 1);
    thread.validate(&store).unwrap();
    let mut forged = thread.clone();
    let HostRequest::External(request) = &mut forged.continuation.as_mut().unwrap().request else {
        panic!("external");
    };
    request.amount = Some(500);
    assert!(forged.validate(&store).is_err());
    let mut forged = thread.clone();
    let HostRequest::External(request) = &mut forged.continuation.as_mut().unwrap().request else {
        panic!("external");
    };
    request.operand[2] = 6;
    assert!(forged.validate(&store).is_err());
    let mut forged = thread.clone();
    let HostRequest::External(request) = &mut forged.continuation.as_mut().unwrap().request else {
        panic!("external");
    };
    request.parameters[0] = 9;
    assert!(forged.validate(&store).is_err());
    // Other entity scripts may mutate shared registers while the request stays latched.
    thread.temps[0] = 45;
    thread.temp_xl[0] = 123456;
    thread.validate(&store).unwrap();
    let (route_store, mut route, mut route_host) = setup(vec![instruction(45)]);
    route_host
        .responses
        .push_back(HostResponse::Pending { request_id: 4 });
    route.run(&route_store, &mut route_host, 1);
    for local in [false, true] {
        let mut forged = route.clone();
        let HostRequest::Route(request) = &mut forged.continuation.as_mut().unwrap().request else {
            panic!("route");
        };
        if local {
            request.locals[0] = 8;
        } else {
            request.parameters[0] = 8;
        }
        assert!(forged.validate(&route_store).is_err());
    }
    route.temps[0] = -89;
    route.validate(&route_store).unwrap();
    let HostRequest::Route(request) = &mut route.continuation.as_mut().unwrap().request else {
        panic!("route");
    };
    request.kind = RouteKind::Snap;
    assert!(route.validate(&route_store).is_err());
    let (sleep_store, _, _) = setup(vec![instruction(0)]);
    assert!(thread.validate(&sleep_store).is_err());
}
