//! Opt-in source callback semantics, not avatar admission.
use bincode::Options;
use sim_core::{
    ids::{EntityRef, ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{
        FrameContext, RoutineKey, RoutineScope, RoutineStore, Scope, Variable, VmFault,
        VmInstruction, VmMode, VmRoutine,
    },
    world::{Facing, LotModel, LotPosition},
};
fn assign(lhs: Variable, rhs: Variable, next: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        ExpressionOperand {
            lhs,
            rhs,
            is_signed: 0,
            operator: 5,
        }
        .encode(),
    )
}
fn key(id: u16) -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(100),
        id,
    }
}
fn make(role: RuntimeRole, mode: VmMode) -> SimRuntime {
    let mut routines = RoutineStore::new();
    for (id, instructions) in [
        (
            4096,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 888),
                    1,
                ),
                assign(
                    Variable::new(Scope::Temps, 0),
                    Variable::new(Scope::Literal, 99),
                    2,
                ),
                assign(
                    Variable::new(Scope::Global, 21),
                    Variable::new(Scope::Global, 32767),
                    254,
                ),
            ],
        ),
        (
            4097,
            vec![assign(
                Variable::new(Scope::Global, 21),
                Variable::new(Scope::Global, 20),
                254,
            )],
        ),
        (
            4098,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 999),
                    1,
                ),
                VmInstruction::new(65535, 254, 255, [0; 8]),
            ],
        ),
        (
            4099,
            vec![VmInstruction::new(
                2,
                0,
                0,
                ExpressionOperand {
                    lhs: Variable::new(Scope::Global, 20),
                    rhs: Variable::new(Scope::Literal, 777),
                    is_signed: 0,
                    operator: 5,
                }
                .encode(),
            )],
        ),
        (
            4101,
            vec![assign(
                Variable::new(Scope::Global, 22),
                Variable::new(Scope::Literal, 42),
                255,
            )],
        ),
        (4102, vec![]),
        (
            4110,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 555),
                    1,
                ),
                assign(
                    Variable::new(Scope::Global, 32767),
                    Variable::new(Scope::Literal, 42),
                    254,
                ),
            ],
        ),
        (
            4111,
            vec![VmInstruction::new(
                2,
                254,
                255,
                ExpressionOperand {
                    lhs: Variable::new(Scope::Global, 20),
                    rhs: Variable::new(Scope::Global, 32767),
                    is_signed: 0,
                    operator: 11,
                }
                .encode(),
            )],
        ),
        (
            4112,
            vec![
                assign(
                    Variable::new(Scope::StackObjectTemp, 0),
                    Variable::new(Scope::Literal, 555),
                    1,
                ),
                assign(
                    Variable::new(Scope::Global, 21),
                    Variable::new(Scope::Global, 32767),
                    254,
                ),
            ],
        ),
        (
            4103,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 999),
                    1,
                ),
                assign(
                    Variable::new(Scope::Temps, 0),
                    Variable::new(Scope::Temps, 32767),
                    254,
                ),
            ],
        ),
        (
            4104,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 999),
                    1,
                ),
                assign(
                    Variable::new(Scope::MyObjectAttributes, 0),
                    Variable::new(Scope::Literal, 1),
                    254,
                ),
            ],
        ),
        (
            4105,
            vec![
                assign(
                    Variable::new(Scope::Global, 22),
                    Variable::new(Scope::Literal, 333),
                    1,
                ),
                // If run, the child writes global 20 before its bounds exception.
                // The bounded profile must reject this entire routine up front.
                VmInstruction::new(4096, 254, 255, [0; 8]),
            ],
        ),
        (
            4100,
            vec![
                assign(
                    Variable::new(Scope::Temps, 0),
                    Variable::new(Scope::Literal, 73),
                    1,
                ),
                VmInstruction::new(0, 254, 255, [0; 8]),
            ],
        ),
    ] {
        routines
            .insert(key(id), VmRoutine::new(id, 0, 4, instructions).unwrap())
            .unwrap();
    }
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(100, 1)],
        vec![],
        TuningSet::default(),
    )
    .unwrap()
    .with_named_trees(vec![
        ((100, "fault".into()), key(4096)),
        ((100, "after".into()), key(4097)),
        ((100, "unsupported".into()), key(4098)),
        ((100, "budget".into()), key(4099)),
        ((100, "false".into()), key(4101)),
        ((100, "empty".into()), key(4102)),
        ((100, "destination fault".into()), key(4110)),
        ((100, "increment rhs fault".into()), key(4111)),
        ((100, "entity bank fault".into()), key(4112)),
        ((100, "unqualified bounds".into()), key(4103)),
        ((100, "revision".into()), key(4104)),
        ((100, "nested".into()), key(4105)),
    ])
    .unwrap();
    SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).unwrap(),
        RuntimeConfig::new(mode, 7, 1, 123),
        role,
    )
    .unwrap()
}
fn actor(n: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(n),
        generation: 1,
    }
}
fn callback(entity: EntityRef, name: &str) -> AcceptedCommand {
    AcceptedCommand::ExecuteSourceExpressionCallback {
        entity,
        name: name.into(),
        args: vec![],
    }
}
fn initialized(mode: VmMode) -> SimRuntime {
    let mut runtime = make(RuntimeRole::Authority, mode);
    let tick = runtime
        .next_tick(
            (0..2)
                .map(|_| {
                    AcceptedCommand::Spawn(SpawnSpec {
                        guid: 100,
                        position: LotPosition::OUT_OF_WORLD,
                        facing: Facing::NORTH,
                        persistent_id: PersistentId(0),
                        avatar: false,
                    })
                })
                .collect(),
        )
        .unwrap();
    runtime.step(&tick).unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor(1),
            routine: key(4100),
            context: FrameContext::for_entity(actor(1), 100),
            args: vec![100, 0, 0, 0],
            replace: true,
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
    runtime
}
#[test]
fn source_exception_retains_mutations_and_continues_with_replay_and_restore() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut authority = initialized(mode);
        let mut replica = make(RuntimeRole::Replica, mode);
        replica.restore(&authority.snapshot().unwrap()).unwrap();
        let owned = authority.state().threads[&ObjectId(1)].clone();
        assert!(!owned.frames.is_empty());
        let tick = authority
            .next_tick(vec![
                callback(actor(1), "fault"),
                callback(actor(2), "after"),
            ])
            .unwrap();
        let outcome = authority.step(&tick).unwrap();
        let replay = replica.step(&tick).unwrap();
        assert_eq!(outcome.events, replay.events);
        assert_eq!(authority.state().globals[20], 888);
        assert_eq!(authority.state().globals[21], 888);
        let thread = &authority.state().threads[&ObjectId(1)];
        assert_eq!(thread.frames, owned.frames);
        assert_eq!(thread.temps, owned.temps);
        assert_eq!(thread.temp_xl, owned.temp_xl);
        let results: Vec<_> = outcome
            .events
            .iter()
            .filter_map(|event| match event {
                RuntimeEvent::SourceNamedEntryPoint {
                    entity,
                    accepted,
                    suppressed_fault,
                    ..
                } => Some((*entity, *accepted, suppressed_fault.clone())),
                _ => None,
            })
            .collect();
        assert!(
            matches!(&results[..], [(first, false, Some(VmFault::Bounds { area, .. })), (second, true, None)] if *first == actor(1) && *second == actor(2) && area == "global")
        );
        assert_eq!(
            authority.state_hash().unwrap(),
            replica.state_hash().unwrap()
        );
        let mut restored = make(RuntimeRole::Replica, mode);
        restored.restore(&authority.snapshot().unwrap()).unwrap();
        assert_eq!(
            authority.state_hash().unwrap(),
            restored.state_hash().unwrap()
        );
        let duplicate = restored.step(&tick).unwrap();
        assert!(duplicate.duplicate);
        assert!(duplicate.events.is_empty());
        assert!(duplicate.effects.is_empty());
    }
}

#[test]
fn legacy_fault_unsupported_budget_and_stale_identity_still_reject_atomically() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut runtime = initialized(mode);
        for command in [
            AcceptedCommand::ExecuteNamedEntryPoint {
                entity: actor(1),
                name: "fault".into(),
                args: vec![],
            },
            callback(actor(1), "unsupported"),
            callback(actor(1), "budget"),
            callback(actor(1), "unqualified bounds"),
            callback(
                EntityRef {
                    generation: 2,
                    ..actor(1)
                },
                "fault",
            ),
        ] {
            let before = runtime.snapshot().unwrap();
            let tick = runtime.next_tick(vec![command]).unwrap();
            assert!(runtime.step(&tick).is_err());
            assert_eq!(before, runtime.snapshot().unwrap());
        }
    }
}

#[test]
fn source_false_and_inert_callbacks_have_explicit_results_and_stable_tags() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut runtime = initialized(mode);
        let old = AcceptedCommand::ExecuteNamedEntryPoint {
            entity: actor(1),
            name: "false".into(),
            args: vec![],
        };
        let new = callback(actor(1), "false");
        for (command, tag) in [(old, 25_u32), (new, 26_u32)] {
            let bytes = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .with_little_endian()
                .serialize(&command)
                .unwrap();
            assert_eq!(&bytes[..4], &tag.to_le_bytes());
        }
        let tick = runtime
            .next_tick(vec![
                callback(actor(1), "false"),
                callback(actor(1), "empty"),
                callback(actor(1), "missing"),
            ])
            .unwrap();
        let result = runtime.step(&tick).unwrap();
        assert_eq!(runtime.state().globals[22], 42);
        let callbacks: Vec<_> = result
            .events
            .iter()
            .filter_map(|event| match event {
                RuntimeEvent::SourceNamedEntryPoint {
                    name,
                    accepted,
                    suppressed_fault,
                    ..
                } => Some((name.as_str(), *accepted, suppressed_fault.is_none())),
                _ => None,
            })
            .collect();
        assert_eq!(
            callbacks,
            vec![
                ("false", false, true),
                ("empty", false, true),
                ("missing", false, true)
            ]
        );
    }
}
#[test]
fn source_callback_never_suppresses_host_revision_exhaustion() {
    let runtime = initialized(VmMode::Tso);
    let mut state = runtime.state().clone();
    state.entities.get_mut(&ObjectId(1)).unwrap().revision = u64::MAX;
    let mut runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![callback(actor(1), "revision")])
        .unwrap();
    let error = runtime.step(&tick).unwrap_err();
    assert!(format!("{error:?}").contains("entity revision exhausted"));
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn nested_callback_is_refused_before_outer_or_inner_writes() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut runtime = initialized(mode);
        let before = runtime.snapshot().unwrap();
        let tick = runtime
            .next_tick(vec![callback(actor(1), "nested")])
            .unwrap();
        let error = runtime.step(&tick).unwrap_err();
        assert!(format!("{error:?}").contains("expression-only routine"));
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
}
#[test]
fn source_callback_enforces_command_bounds_before_work() {
    let mut runtime = initialized(VmMode::Tso);
    for (name, args) in [
        (String::new(), vec![]),
        ("x".repeat(257), vec![]),
        ("fault".into(), vec![0; 17]),
    ] {
        let before = runtime.snapshot().unwrap();
        let tick = runtime
            .next_tick(vec![AcceptedCommand::ExecuteSourceExpressionCallback {
                entity: actor(1),
                name,
                args,
            }])
            .unwrap();
        assert!(runtime.step(&tick).is_err());
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
}

#[test]
fn destination_bounds_and_rhs_fault_keep_prior_writes() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut runtime = initialized(mode);
        for (name, value) in [("destination fault", 555), ("increment rhs fault", 556)] {
            let tick = runtime.next_tick(vec![callback(actor(1), name)]).unwrap();
            let outcome = runtime.step(&tick).unwrap();
            assert_eq!(runtime.state().globals[20], value);
            assert!(outcome.events.iter().any(|event| matches!(event,
                RuntimeEvent::SourceNamedEntryPoint { accepted: false, suppressed_fault: Some(VmFault::Bounds { area, .. }), .. } if area == "global"
            )));
        }
    }
}
#[test]
fn explicit_entity_temp_write_survives_callback_local_fault() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut runtime = initialized(mode);
        let tick = runtime
            .next_tick(vec![callback(actor(1), "entity bank fault")])
            .unwrap();
        runtime.step(&tick).unwrap();
        assert_eq!(runtime.state().threads[&ObjectId(1)].temps[0], 555);
    }
}
#[test]
fn later_fatal_command_rolls_back_suppressed_callback_and_entire_tick() {
    for mode in [VmMode::Ts1, VmMode::Tso] {
        let mut runtime = initialized(mode);
        let before = runtime.snapshot().unwrap();
        let tick = runtime
            .next_tick(vec![
                callback(actor(1), "fault"),
                callback(
                    EntityRef {
                        generation: 2,
                        ..actor(2)
                    },
                    "after",
                ),
            ])
            .unwrap();
        assert!(runtime.step(&tick).is_err());
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
}
#[test]
fn indirect_pet_reset_fault_rejects_without_leaking_outer_or_inner_writes() {
    // Expression-only is a local opcode condition, not a transitive guarantee.
    // A Burning flag write on a pet invokes reset entrypoint 3 synchronously.
    let mut routines = RoutineStore::new();
    for (id, instructions) in [
        (
            4096,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 333),
                    1,
                ),
                assign(
                    Variable::new(Scope::MyObject, 8),
                    Variable::new(Scope::Literal, 1 << 9),
                    254,
                ),
            ],
        ),
        (
            4097,
            vec![
                assign(
                    Variable::new(Scope::Global, 21),
                    Variable::new(Scope::Literal, 444),
                    1,
                ),
                assign(
                    Variable::new(Scope::Global, 22),
                    Variable::new(Scope::Global, 32767),
                    254,
                ),
            ],
        ),
    ] {
        routines
            .insert(key(id), VmRoutine::new(id, 0, 4, instructions).unwrap())
            .unwrap();
    }
    let mut object = ObjectDefinition::new(100, 1);
    object.entry_points.insert(3, key(4097));
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default())
        .unwrap()
        .with_named_trees(vec![((100, "indirect reset".into()), key(4096))])
        .unwrap();
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 100,
            position: LotPosition::OUT_OF_WORLD,
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: true,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let mut state = runtime.state().clone();
    state
        .entities
        .get_mut(&ObjectId(1))
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .person_data
        .values[65] = 8;
    runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    assert!(runtime.state().entities[&ObjectId(1)]
        .avatar
        .as_ref()
        .unwrap()
        .is_pet());
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![callback(actor(1), "indirect reset")])
        .unwrap();
    let error = runtime.step(&tick).unwrap_err();
    assert!(
        matches!(error, sim_core::runtime::RuntimeError::Vm(VmFault::HostUnsupported(ref detail))
        if detail.contains("Bounds") && detail.contains("global")),
        "{error:?}"
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
}
