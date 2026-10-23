//! Authored source-contract harnesses, not original-content qualification.
use bincode::Options;
use sim_core::{
    avatars::{
        AvatarPlatform, AvatarState,
        motives::TsoMotiveTuning,
        state::{AvatarPermissions, TEMPLATE_PERSON},
    },
    ids::*,
    interactions::*,
    lot_permissions::*,
    owned_admission::*,
    runtime::{leave_adapter::*, *},
    state::*,
    vm::*,
    world::*,
};
use std::collections::BTreeMap;
fn content(action: Option<Vec<VmInstruction>>, reject_check: bool) -> ContentSet {
    content_with_check(
        action,
        reject_check.then_some(vec![VmInstruction::new(
            2,
            254,
            255,
            [0, 0, 1, 0, 0, 2, 7, 7],
        )]),
    )
}
fn content_with_check(
    action: Option<Vec<VmInstruction>>,
    check_program: Option<Vec<VmInstruction>>,
) -> ContentSet {
    let reject_check = check_program.is_some();
    let mut routines = RoutineStore::new();
    routines.bind_semiglobal(TEMPLATE_PERSON, 900).unwrap();
    let has_action = action.is_some();
    if let Some(program) = action {
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::SemiGlobal(900),
                    id: 8373,
                },
                VmRoutine::new(8373, 0, 4, program).unwrap(),
            )
            .unwrap();
    }
    if let Some(check_program) = check_program {
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::Private(TEMPLATE_PERSON),
                    id: 4096,
                },
                VmRoutine::new(4096, 0, 4, check_program).unwrap(),
            )
            .unwrap();
    }
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(TEMPLATE_PERSON, 4)],
        vec![],
        TuningSet {
            tso_motives: Some(TsoMotiveTuning {
                flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                category_weights: [[1000; 7]; 11],
            }),
            ..Default::default()
        },
    )
    .unwrap();
    if !has_action {
        return content;
    }
    content
        .with_interaction_tables(vec![(
            TEMPLATE_PERSON,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: InteractionKey {
                        tta_index: 173,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: 8373,
                        code_owner_guid: TEMPLATE_PERSON,
                    },
                    check: reject_check.then_some(RoutineBinding {
                        routine_id: 4096,
                        code_owner_guid: TEMPLATE_PERSON,
                    }),
                    flags: ActionFlags(
                        ActionFlags::ALLOW_VISITORS
                            | ActionFlags::ALLOW_CONSECUTIVE
                            | if reject_check {
                                ActionFlags::RUN_CHECK_ALWAYS
                            } else {
                                0
                            },
                    ),
                    permissions: PermissionFlags::default(),
                    label: Some("Authored leave".into()),
                }],
            },
        )])
        .unwrap()
}
fn leave_action() -> Vec<VmInstruction> {
    vec![VmInstruction::new(1, 254, 255, [25, 0, 0, 0, 0, 0, 0, 0])]
}
fn expr(lhs: Scope, index: i16, rhs: Scope, value: i16, next: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        sim_core::primitives::arithmetic::ExpressionOperand {
            lhs: Variable::new(lhs, index),
            rhs: Variable::new(rhs, value),
            is_signed: 0,
            operator: 5,
        }
        .encode(),
    )
}
fn idle_action() -> Vec<VmInstruction> {
    vec![
        expr(Scope::Parameters, 0, Scope::Literal, 1000, 1),
        VmInstruction::new(0, 0, 255, [0; 8]),
    ]
}
fn step(runtime: &mut SimRuntime, commands: Vec<AcceptedCommand>) -> TickOutcome {
    let tick = runtime.next_tick(commands).unwrap();
    runtime.step(&tick).unwrap()
}
fn fixture(content: ContentSet) -> SimRuntime {
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 41, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let source = SourceJoinMetadata {
        source_import_sha256: [1; 32],
        content: runtime.state().content,
        neighborhood_id: 8,
        lot_location: 16777472,
        avatar_guids: [TEMPLATE_PERSON].into(),
        april_fools_2019: false,
        object_flags: BTreeMap::new(),
        registered_groups: BTreeMap::new(),
    };
    step(
        &mut runtime,
        vec![
            AcceptedCommand::InitializeLotPermissions {
                lot: LotPermissionState::default(),
            },
            AcceptedCommand::InitializeOwnedAdmission { source },
        ],
    );
    for (id, principal) in [(17, 1), (18, 2)] {
        let mut portable = AvatarState::new(
            EntityRef {
                object_id: ObjectId(1),
                generation: 1,
            },
            PersistentId(id),
            AvatarPlatform::Tso,
        )
        .persistent_state()
        .unwrap();
        portable.name = format!("owned {id}");
        portable.permissions = AvatarPermissions::Visitor;
        let command = AcceptedCommand::AdmitOwnedAvatar(OwnedAvatarAdmission {
            control: OwnedControl {
                principal,
                account_id: 1000 + u64::from(id),
                avatar_id: PersistentId(id),
                actor: EntityRef {
                    object_id: ObjectId(0),
                    generation: 0,
                },
                control_generation: 1,
                session_ref: [id as u8; 32],
                admission_operation: format!("admission-{id:016}"),
                reservation_digest: [2; 32],
                export_digest: [3; 32],
            },
            portable,
            permission_revision: 1,
            permission_sha256: permission_digest(runtime.state()).unwrap(),
        });
        step(&mut runtime, vec![command]);
    }
    runtime
}
fn request(runtime: &SimRuntime, principal: u64) -> OwnedLeaveRequest {
    let control = runtime.state().owned_admission.actors().unwrap()[&principal].clone();
    OwnedLeaveRequest {
        authority_id: [4; 32],
        departure_operation: format!(
            "{}_{}_{}",
            "04".repeat(32),
            control.avatar_id.0,
            control.control_generation
        ),
        control,
    }
}
#[test]
fn same_tick_source_leave_captures_before_detach_and_replays() {
    let mut authority = fixture(content(Some(leave_action()), false));
    let request = request(&authority, 1);
    let actor = request.control.actor;
    let other = authority.state().owned_admission.actors().unwrap()[&2].actor;
    let mut replica = SimRuntime::from_state(
        authority.state().clone(),
        authority.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    let tick = authority
        .next_tick(vec![AcceptedCommand::RequestOwnedLeave(Box::new(
            request.clone(),
        ))])
        .unwrap();
    let outcome = authority.step(&tick).unwrap();
    let replay = replica.step(&tick).unwrap();
    assert_eq!(outcome.events, replay.events);
    assert_eq!(outcome.state_hash, replay.state_hash);
    assert!(outcome.effects.is_empty());
    let save = outcome
        .events
        .iter()
        .position(
            |event| matches!(event,RuntimeEvent::OwnedDepartureCaptured{actor:a,..} if *a==actor),
        )
        .expect("first source tick capture");
    let detach=outcome.events.iter().position(|event|matches!(event,RuntimeEvent::OwnedDepartureDetached{actor:a,reason:DepartureRemovalReason::SourceLeave,..} if *a==actor)).expect("actual mode25 deletion");
    assert!(save < detach);
    assert!(!authority.state().ids.is_live(actor));
    assert!(authority.state().ids.is_live(other));
    let retained = authority.state().owned_departures.records[&actor].clone();
    assert_eq!(retained.capture.as_ref().unwrap().captured_tick, tick.tick);
    assert_eq!(retained.detached_tick, Some(tick.tick));
    let snapshot = authority.snapshot().unwrap();
    replica.restore(&snapshot).unwrap();
    assert!(replica.step(&tick).unwrap().duplicate);
    let retry = step(
        &mut authority,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    assert!(!retry.events.iter().any(|event| matches!(
        event,
        RuntimeEvent::OwnedDepartureCaptured { .. } | RuntimeEvent::OwnedDepartureDetached { .. }
    )));
    assert_eq!(authority.state().owned_departures.records[&actor], retained);
}
#[test]
fn source_leave_queue_uses_priority50_skip_permissions_and_no_duplicate() {
    let mut runtime = fixture(content(Some(idle_action()), false));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(
            request.clone(),
        ))],
    );
    let entry = runtime.state().interaction_queues[&actor.object_id]
        .entries()
        .iter()
        .find(|entry| entry.invocation.definition.action.routine_id == 8373)
        .unwrap();
    assert_eq!(entry.invocation.priority, 50);
    assert_ne!(
        entry.invocation.definition.flags.0 & ActionFlags::SKIP_PERMISSIONS,
        0
    );
    let id = entry.id;
    assert!(!runtime.state().interaction_access.contains_key(&actor));
    assert!(
        runtime.state().owned_departures.records[&actor]
            .capture
            .is_some()
    );
    for _ in 0..3 {
        step(
            &mut runtime,
            vec![AcceptedCommand::RequestOwnedLeave(Box::new(
                request.clone(),
            ))],
        );
    }
    let entries: Vec<_> = runtime.state().interaction_queues[&actor.object_id]
        .entries()
        .iter()
        .filter(|entry| entry.invocation.definition.action.routine_id == 8373)
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, id);
}
#[test]
fn missing_source_action_fences_without_inventing_capture_or_cleanup() {
    let mut runtime = fixture(content(None, false));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    let other = runtime.state().owned_admission.actors().unwrap()[&2].actor;
    let before = runtime.state().entities[&other.object_id]
        .avatar
        .as_ref()
        .unwrap()
        .person_data
        .values[27];
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    for _ in 0..3 {
        step(&mut runtime, vec![]);
    }
    let record = &runtime.state().owned_departures.records[&actor];
    assert_eq!(record.blocker, Some(DepartureBlocker::MissingSourceAction));
    assert!(record.capture.is_none());
    assert!(record.detached_tick.is_none());
    assert!(runtime.state().ids.is_live(actor));
    assert!(!runtime.state().interaction_access.contains_key(&actor));
    assert_ne!(
        runtime.state().entities[&other.object_id]
            .avatar
            .as_ref()
            .unwrap()
            .person_data
            .values[27],
        before
    );
}
#[test]
fn stale_generation_is_failure_atomic_and_command_tag_is_append_only() {
    let mut runtime = fixture(content(Some(leave_action()), false));
    let mut request = request(&runtime, 1);
    request.control.control_generation += 1;
    request.departure_operation = format!(
        "{}_{}_{}",
        "04".repeat(32),
        request.control.avatar_id.0,
        request.control.control_generation
    );
    let before = runtime.snapshot().unwrap();
    let command = AcceptedCommand::RequestOwnedLeave(Box::new(request));
    let bytes = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize(&command)
        .unwrap();
    assert_eq!(&bytes[..4], &31_u32.to_le_bytes());
    let tick = runtime.next_tick(vec![command]).unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
}
#[test]
fn unknown_or_plugin_session_history_blocks_owned_detach() {
    let runtime = fixture(content(Some(leave_action()), false));
    let mut state = runtime.state().clone();
    state.owned_departures.eod_known_clear = false;
    let mut runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    let outcome = step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    assert_eq!(
        runtime.state().owned_departures.records[&actor].blocker,
        Some(DepartureBlocker::UnsupportedEodSession)
    );
    assert!(runtime.state().ids.is_live(actor));
    assert!(outcome.effects.is_empty());
    assert!(
        runtime.state().owned_departures.records[&actor]
            .capture
            .is_none()
    );
}
#[test]
fn generic_delete_cannot_impersonate_source_departure() {
    let mut runtime = fixture(content(Some(idle_action()), false));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Delete { entity: actor }])
        .unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn deferred_check_rejection_is_not_detach_and_forced_cleanup_is_distinct() {
    let mut runtime = fixture(content(Some(leave_action()), true));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    let capture = runtime.state().owned_departures.records[&actor]
        .capture
        .clone()
        .expect("first leave tick");
    assert!(runtime.state().ids.is_live(actor));
    assert!(
        runtime.state().owned_departures.records[&actor]
            .detached_tick
            .is_none()
    );
    let mut state = runtime.state().clone();
    state
        .entities
        .get_mut(&actor.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .lifecycle
        .kill_timeout = 1800;
    let mut runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let outcome = step(&mut runtime, vec![]);
    assert!(!runtime.state().ids.is_live(actor));
    assert_eq!(
        runtime.state().owned_departures.records[&actor]
            .capture
            .as_ref(),
        Some(&capture)
    );
    assert!(outcome.events.iter().any(|event| matches!(
        event,
        RuntimeEvent::OwnedDepartureDetached {
            reason: DepartureRemovalReason::ForcedCleanup,
            ..
        }
    )));
}
#[test]
fn typed_unsupported_dispatch_rolls_back_peer_rng_events_and_capture() {
    let program = vec![
        expr(Scope::StackObjectId, 0, Scope::Literal, 2, 1),
        expr(Scope::StackObjectAttributes, 0, Scope::Literal, 1234, 2),
        VmInstruction::new(8, 3, 255, [0, 0, 8, 0, 100, 0, 7, 0]),
        VmInstruction::new(23, 4, 255, [0; 8]),
        VmInstruction::new(42, 254, 255, [0, 0, 0, 0, 0, 4, 0, 0]),
    ];
    let mut runtime = fixture(content(Some(program), false));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    let other = runtime.state().owned_admission.actors().unwrap()[&2].actor;
    let prior_rng = runtime.state().rng.state();
    let other_timer = runtime.state().entities[&other.object_id]
        .avatar
        .as_ref()
        .unwrap()
        .person_data
        .values[27];
    let outcome = step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    let record = &runtime.state().owned_departures.records[&actor];
    assert_eq!(record.blocker, Some(DepartureBlocker::SourceExecutionFault));
    assert_eq!(record.blocked_primitive, Some(42));
    assert!(record.capture.is_none());
    assert!(record.detached_tick.is_none());
    assert_eq!(runtime.state().entities[&other.object_id].attributes[0], 0);
    assert_eq!(runtime.state().rng.state(), prior_rng.wrapping_add(2));
    assert_eq!(
        runtime.state().entities[&other.object_id]
            .avatar
            .as_ref()
            .unwrap()
            .person_data
            .values[27],
        other_timer.wrapping_add(1)
    );
    assert!(!outcome.events.iter().any(|event| matches!(
        event,
        RuntimeEvent::Presentation(_)
            | RuntimeEvent::OwnedDepartureCaptured { .. }
            | RuntimeEvent::OwnedDepartureDetached { .. }
    )));
    assert!(runtime.state().effects.pending_requests().next().is_none());
    assert!(outcome.effects.is_empty());
    let checkpoint = runtime.snapshot().unwrap();
    runtime.restore(&checkpoint).unwrap();
    assert_eq!(
        runtime.state().owned_departures.records[&actor].blocked_primitive,
        Some(42)
    );
}
#[test]
fn ordinary_unsupported_execution_remains_whole_tick_fatal() {
    let mut runtime = fixture(content(
        Some(vec![VmInstruction::new(
            42,
            254,
            255,
            [0, 0, 0, 0, 0, 4, 0, 0],
        )]),
        false,
    ));
    let actor = runtime.state().owned_admission.actors().unwrap()[&1].actor;
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: RoutineKey {
                scope: RoutineScope::SemiGlobal(900),
                id: 8373,
            },
            context: FrameContext {
                caller: actor,
                callee: actor,
                stack_object: actor.object_id,
                stack_object_ref: Some(actor),
                code_owner: TEMPLATE_PERSON,
            },
            args: vec![0; 4],
            replace: true,
        }])
        .unwrap();
    assert!(matches!(
        runtime.step(&tick),
        Err(RuntimeError::Vm(VmFault::UnsupportedPrimitive {
            opcode: 42,
            ..
        }))
    ));
    assert_eq!(runtime.snapshot().unwrap(), before);
}

fn start_source(actor: EntityRef) -> AcceptedCommand {
    AcceptedCommand::StartBehavior {
        entity: actor,
        routine: RoutineKey {
            scope: RoutineScope::SemiGlobal(900),
            id: 8373,
        },
        context: FrameContext::for_entity(actor, TEMPLATE_PERSON),
        args: vec![0; 4],
        replace: true,
    }
}
#[test]
fn existing_external_wait_is_retained_and_blocks_source_timeout_without_fake_reply() {
    let mut runtime = fixture(content(
        Some(vec![VmInstruction::new(1, 254, 255, [0; 8])]),
        false,
    ));
    let actor = runtime.state().owned_admission.actors().unwrap()[&1].actor;
    let issued = step(&mut runtime, vec![start_source(actor)]);
    assert_eq!(issued.effects.len(), 1);
    let pending = runtime.state().effects.clone();
    let continuations = runtime.state().continuations.clone();
    let request = request(&runtime, 1);
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    assert_eq!(
        runtime.state().owned_departures.records[&actor].blocker,
        Some(DepartureBlocker::UnreconciledExternalEffect)
    );
    assert_eq!(runtime.state().effects, pending);
    assert_eq!(runtime.state().continuations, continuations);
    assert!(
        runtime.state().owned_departures.records[&actor]
            .capture
            .is_none()
    );
}
#[test]
fn another_invoker_plugin_invalidates_fresh_lot_no_session_proof() {
    let mut runtime = fixture(content(
        Some(vec![VmInstruction::new(62, 254, 255, [0; 8])]),
        false,
    ));
    let other = runtime.state().owned_admission.actors().unwrap()[&2].actor;
    let issued = step(&mut runtime, vec![start_source(other)]);
    assert_eq!(issued.effects.len(), 1);
    assert!(!runtime.state().owned_departures.eod_known_clear);
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    assert_eq!(
        runtime.state().owned_departures.records[&actor].blocker,
        Some(DepartureBlocker::UnsupportedEodSession)
    );
    assert!(runtime.state().ids.is_live(actor));
    assert!(
        runtime.state().owned_departures.records[&actor]
            .capture
            .is_none()
    );
}

#[test]
fn unsupported_deferred_check_is_failure_not_false_or_forced_success() {
    let mut runtime = fixture(content_with_check(
        Some(leave_action()),
        Some(vec![VmInstruction::new(
            42,
            254,
            255,
            [0, 0, 0, 0, 0, 4, 0, 0],
        )]),
    ));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))])
        .unwrap();
    assert!(runtime.step(&tick).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
    assert!(runtime.state().ids.is_live(actor));
    assert!(
        !runtime
            .state()
            .owned_departures
            .records
            .contains_key(&actor)
    );
}

#[test]
fn schema_five_migration_keeps_source_state_but_has_no_eod_proof() {
    use sha2::{Digest, Sha256};
    let runtime = fixture(content(Some(idle_action()), false));
    assert!(runtime.state().owned_departures.eod_known_clear);
    let current = runtime.snapshot().unwrap();
    let end = current.len() - 32 - 17 - 40;
    let mut legacy = current[..end].to_vec();
    legacy[10..12].copy_from_slice(&5u16.to_le_bytes());
    legacy[108..110].copy_from_slice(&5u16.to_le_bytes());
    legacy[100..108].copy_from_slice(&((end - 108) as u64).to_le_bytes());
    let checksum = Sha256::digest(&legacy);
    legacy.extend_from_slice(&checksum);
    let mut restored = SimRuntime::from_state(
        runtime.state().clone(),
        runtime.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    restored.restore(&legacy).unwrap();
    let mut expected = runtime.state().clone();
    expected.owned_departures = OwnedDepartureState::default();
    assert_eq!(restored.state(), &expected);
    let saved = restored.snapshot().unwrap();
    restored.restore(&saved).unwrap();
    assert!(!restored.state().owned_departures.eod_known_clear);
    assert_eq!(restored.snapshot().unwrap(), saved);
}

#[test]
fn retained_departure_rejects_forged_fence_capture_and_fault_classification() {
    let mut runtime = fixture(content(Some(idle_action()), false));
    let request = request(&runtime, 1);
    let actor = request.control.actor;
    step(
        &mut runtime,
        vec![AcceptedCommand::RequestOwnedLeave(Box::new(request))],
    );
    let valid = runtime.state().clone();
    for case in 0..5 {
        let mut state = valid.clone();
        match case {
            0 => {
                state.interaction_access.insert(
                    actor,
                    sim_core::runtime::interaction_adapter::InteractionAccess {
                        principal: PrincipalKey(1),
                        allow_hidden: false,
                    },
                );
            }
            1 => {
                let record = state.owned_departures.records.get_mut(&actor).unwrap();
                record.blocker = Some(DepartureBlocker::SourceExecutionFault);
                record.blocked_primitive = Some(47);
            }
            2 => {
                state
                    .owned_departures
                    .records
                    .get_mut(&actor)
                    .unwrap()
                    .eod_disconnect_requested =
                    !state.owned_departures.records[&actor].eod_disconnect_requested;
            }
            3 => {
                state
                    .owned_departures
                    .records
                    .get_mut(&actor)
                    .unwrap()
                    .capture
                    .as_mut()
                    .unwrap()
                    .portable
                    .motive_data[11] = 1;
            }
            _ => {
                state
                    .owned_departures
                    .records
                    .get_mut(&actor)
                    .unwrap()
                    .capture
                    .as_mut()
                    .unwrap()
                    .changed_relationships
                    .insert(PersistentId(19), vec![999]);
            }
        }
        assert!(
            SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Replica).is_err(),
            "malformed departure case {case}"
        );
    }
}

// Authored source Timer boundary tests. These do not qualify original assets.
fn timer_content() -> ContentSet {
    let mut operand = [0, 1, 2, 0, 0, 0, 0, 0];
    operand[4..8].copy_from_slice(&sim_core::eod::TIMER_PLUGIN_ID.to_le_bytes());
    content(
        Some(vec![
            expr(Scope::Local, 0, Scope::Parameters, 0, 1),
            expr(Scope::Local, 1, Scope::Parameters, 1, 2),
            expr(Scope::Local, 2, Scope::Parameters, 2, 3),
            VmInstruction::new(62, 254, 4, operand),
            expr(Scope::Global, 0, Scope::Local, 2, 255),
        ]),
        false,
    )
}
fn timer_start(
    invoker: EntityRef,
    actor: EntityRef,
    object: EntityRef,
    event: i16,
) -> AcceptedCommand {
    let mut context = FrameContext::for_entity(invoker, TEMPLATE_PERSON);
    // Distinct stack object must never become the invoker or substitute locals.
    context.stack_object = actor.object_id;
    context.stack_object_ref = Some(actor);
    AcceptedCommand::StartBehavior {
        entity: invoker,
        routine: RoutineKey {
            scope: RoutineScope::SemiGlobal(900),
            id: 8373,
        },
        context,
        args: vec![actor.object_id.0, object.object_id.0, event, 0],
        replace: true,
    }
}
fn timer_event(
    session: sim_core::eod::EodSessionIdentity,
    event_id: u64,
    code: i16,
    data: Vec<i16>,
) -> AcceptedCommand {
    AcceptedCommand::EodEventDelivered(sim_core::eod::EodEventDelivery {
        session,
        event_id,
        code,
        data,
    })
}
fn timer_connected() -> (SimRuntime, sim_core::eod::EodSessionIdentity) {
    let mut runtime = fixture(timer_content());
    let actor = runtime.state().owned_admission.actors().unwrap()[&1].actor;
    let invoker = runtime.state().owned_admission.actors().unwrap()[&2].actor;
    let outcome = step(&mut runtime, vec![timer_start(invoker, actor, invoker, -5)]);
    let session = outcome
        .events
        .iter()
        .find_map(|e| match e {
            RuntimeEvent::EodConnectRequested {
                session,
                request_id,
                tick,
                ..
            } => {
                assert_eq!(*tick, outcome.tick);
                assert_ne!(*request_id, 0);
                Some(*session)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(session.actor, actor);
    assert_eq!(session.object, invoker);
    assert_eq!(session.invoker, invoker);
    assert_eq!(session.thread, invoker);
    assert_eq!(session.control_generation, 1);
    assert_eq!(runtime.state().threads[&invoker.object_id].temps[0], 0);
    assert!(matches!(
        runtime.state().threads[&invoker.object_id].stop,
        VmStop::Sleeping { .. }
    ));
    assert!(runtime.state().effects.pending_requests().next().is_none());
    assert!(runtime.state().owned_departures.eod_known_clear);
    (runtime, session)
}
#[test]
fn timer_source_locals_fifo_zero_event_and_events_before_ended() {
    let (mut runtime, session) = timer_connected();
    step(
        &mut runtime,
        vec![
            timer_event(session, 1, -2, vec![]),
            timer_event(session, 2, 0, vec![37, 91]),
            timer_event(session, 3, 4, vec![12]),
            timer_event(session, 4, -1, vec![]),
            timer_start(session.invoker, session.actor, session.object, 0),
        ],
    );
    assert_eq!(runtime.state().globals[0], 0);
    assert_eq!(
        &runtime.state().threads[&session.thread.object_id].temps[..2],
        &[37, 91]
    );
    assert_eq!(
        runtime.state().eod.sessions[&session.session_id]
            .events
            .len(),
        2
    );
    assert!(runtime.state().eod.actor_teardown_pending(session.actor));
    // Preserve the real source register bank across this poll. A replacement
    // StartBehavior intentionally creates a fresh thread and resets its temps.
    let mut poll = timer_start(session.invoker, session.actor, session.object, 0);
    if let AcceptedCommand::StartBehavior { replace, .. } = &mut poll {
        *replace = false;
    }
    step(&mut runtime, vec![poll]);
    assert_eq!(runtime.state().globals[0], 4);
    assert_eq!(
        &runtime.state().threads[&session.thread.object_id].temps[..2],
        &[12, 91]
    );
    assert!(
        runtime
            .state()
            .eod
            .connections
            .contains_key(&session.thread)
    );
    step(
        &mut runtime,
        vec![timer_start(
            session.invoker,
            session.actor,
            session.object,
            0,
        )],
    );
    assert!(
        !runtime
            .state()
            .eod
            .connections
            .contains_key(&session.thread)
    );
    assert!(!runtime.state().eod.actor_teardown_pending(session.actor));
    assert!(matches!(
        runtime.state().threads[&session.thread.object_id].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    ));
}
#[test]
fn timer_repeated_connections_and_old_close_are_exact_session_bound() {
    let (mut runtime, first) = timer_connected();
    let outcome = step(
        &mut runtime,
        vec![timer_start(first.invoker, first.actor, first.object, -2)],
    );
    let disconnect = outcome.events.iter().position(|e| matches!(e, RuntimeEvent::EodDisconnectRequested { session, .. } if *session == first)).unwrap();
    let connect = outcome
        .events
        .iter()
        .position(|e| matches!(e, RuntimeEvent::EodConnectRequested { .. }))
        .unwrap();
    assert!(disconnect < connect);
    let second = runtime.state().eod.sessions[&2].identity;
    assert_eq!(second.control_generation, first.control_generation);
    assert_eq!(
        runtime.state().eod.connections[&first.thread],
        second.session_id
    );
    step(&mut runtime, vec![timer_event(first, 1, -1, vec![])]);
    assert_eq!(
        runtime.state().eod.connections[&first.thread],
        second.session_id
    );
    assert!(runtime.state().eod.actor_teardown_pending(first.actor));
    let outcome = step(
        &mut runtime,
        vec![timer_start(second.invoker, second.actor, second.object, -1)],
    );
    assert!(outcome.events.iter().any(|e| matches!(e,
        RuntimeEvent::EodDisconnectRequested { session, .. } if *session == second)));
    assert!(runtime.state().eod.actor_teardown_pending(first.actor));
    step(&mut runtime, vec![timer_event(second, 1, -1, vec![])]);
    assert!(!runtime.state().eod.actor_teardown_pending(first.actor));
    assert!(runtime.state().owned_departures.eod_known_clear);
}
#[test]
fn timer_authoritative_events_reject_stale_conflicting_and_out_of_order_atomically() {
    let (mut runtime, session) = timer_connected();
    let event = timer_event(session, 1, 0, vec![7]);
    step(&mut runtime, vec![event.clone()]);
    step(&mut runtime, vec![event.clone()]);
    assert_eq!(runtime.state().eod.sessions[&1].events.len(), 1);
    let mut stale = session;
    stale.actor.generation += 1;
    for bad in [
        timer_event(session, 1, 0, vec![8]),
        timer_event(session, 3, 0, vec![]),
        timer_event(stale, 2, 0, vec![]),
        timer_event(session, 2, 0, vec![1; 21]),
    ] {
        let before = runtime.snapshot().unwrap();
        let tick = runtime.next_tick(vec![bad]).unwrap();
        assert!(runtime.step(&tick).is_err());
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Delete {
            entity: session.actor,
        }])
        .unwrap();
    assert!(runtime.step(&tick).is_err());
}
#[test]
fn timer_snapshot_replay_keeps_fifo_receipts_ids_and_legacy_unknown() {
    let (mut authority, session) = timer_connected();
    step(
        &mut authority,
        vec![
            timer_event(session, 1, -2, vec![]),
            timer_event(session, 2, 0, vec![123]),
        ],
    );
    let mut replica = SimRuntime::from_state(
        authority.state().clone(),
        authority.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    replica.restore(&authority.snapshot().unwrap()).unwrap();
    for commands in [
        vec![timer_event(session, 2, 0, vec![123])],
        vec![
            timer_event(session, 3, -1, vec![]),
            timer_start(session.invoker, session.actor, session.object, 0),
        ],
        vec![timer_start(
            session.invoker,
            session.actor,
            session.object,
            0,
        )],
    ] {
        let tick = authority.next_tick(commands).unwrap();
        assert_eq!(
            authority.step(&tick).unwrap().state_hash,
            replica.step(&tick).unwrap().state_hash
        );
    }
    let mut unknown = authority.state().clone();
    unknown.owned_departures.eod_known_clear = false;
    let mut unknown =
        SimRuntime::from_state(unknown, authority.content().clone(), RuntimeRole::Authority)
            .unwrap();
    step(
        &mut unknown,
        vec![timer_start(
            session.invoker,
            session.actor,
            session.object,
            -5,
        )],
    );
    let current = unknown.state().eod.sessions[&2].identity;
    step(
        &mut unknown,
        vec![
            timer_event(current, 1, -1, vec![]),
            timer_start(current.invoker, current.actor, current.object, 0),
        ],
    );
    assert!(!unknown.state().owned_departures.eod_known_clear);
}
#[test]
fn schema_six_eod_migration_keeps_legacy_cleanup_unknown() {
    use sha2::{Digest, Sha256};
    let runtime = fixture(content(Some(idle_action()), false));
    let current = runtime.snapshot().unwrap();
    let end = current.len() - 32 - 40;
    let mut legacy = current[..end].to_vec();
    legacy[10..12].copy_from_slice(&6u16.to_le_bytes());
    legacy[108..110].copy_from_slice(&6u16.to_le_bytes());
    legacy[100..108].copy_from_slice(&((end - 108) as u64).to_le_bytes());
    legacy.extend_from_slice(&Sha256::digest(&legacy));
    let restored = sim_core::snapshot::decode(
        &legacy,
        runtime.content(),
        sim_core::snapshot::SnapshotExpectation::new(41, 7),
    )
    .unwrap();
    assert!(!restored.owned_departures.eod_known_clear);
    assert_eq!(restored.eod, sim_core::eod::EodState::default());
}

#[test]
fn timer_peer_cancellation_and_reset_are_association_bound() {
    struct QueueHarness;
    impl sim_core::interactions::adapters::QueueRuntime for QueueHarness {
        fn target_is_alive(&self, _: EntityKey) -> bool {
            true
        }
        fn check_action(&mut self, _: &ActionInvocation) -> sim_core::interactions::Result<bool> {
            Ok(true)
        }
        fn start_action(&mut self, _: &ActionInvocation) -> sim_core::interactions::Result<bool> {
            Ok(true)
        }
    }
    let (runtime, first) = timer_connected();
    // A queued source action is enough for source CancelAction(index0) to
    // force-disconnect the avatar even when an object/peer is its invoker.
    let mut state = runtime.state().clone();
    let mut definition = runtime
        .content()
        .interaction_table(TEMPLATE_PERSON)
        .unwrap()
        .definitions[0]
        .clone();
    definition.action.routine_id = 8373;
    let actor_key = interaction_adapter::entity_key(first.actor);
    let action = state
        .interaction_queues
        .get_mut(&first.actor.object_id)
        .unwrap()
        .enqueue_internal(
            ActionInvocation::pushed(
                actor_key,
                actor_key,
                definition,
                PushPriority::UserDriven,
                None,
                false,
                false,
            ),
            &QueueHarness,
        )
        .unwrap()
        .result;
    let mut runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    let queue = &runtime.state().interaction_queues[&first.actor.object_id];
    let cancel = CancelIntent {
        principal: PrincipalKey(1),
        world_revision: runtime.state().completed_tick,
        actor: EntityVersion {
            key: actor_key,
            revision: runtime.state().entities[&first.actor.object_id].revision,
        },
        queue_revision: queue.revision(),
        command_sequence: 1,
        action,
    };
    let outcome = step(
        &mut runtime,
        vec![AcceptedCommand::CancelInteraction(cancel)],
    );
    assert!(outcome.events.iter().any(|event| matches!(event,
        RuntimeEvent::EodDisconnectRequested { session, .. } if *session == first)));
    let outcome = step(
        &mut runtime,
        vec![AcceptedCommand::Reset {
            entity: first.actor,
        }],
    );
    assert!(
        !outcome
            .events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::EodDisconnectRequested { .. }))
    );
    step(
        &mut runtime,
        vec![
            timer_event(first, 1, -1, vec![]),
            timer_start(first.invoker, first.actor, first.object, 0),
        ],
    );
    step(
        &mut runtime,
        vec![timer_start(first.invoker, first.actor, first.object, -5)],
    );
    let second = runtime.state().eod.sessions[&2].identity;
    let outcome = step(
        &mut runtime,
        vec![AcceptedCommand::Reset {
            entity: first.actor,
        }],
    );
    assert!(outcome.events.iter().any(|event| matches!(event,
        RuntimeEvent::EodDisconnectRequested { session, .. } if *session == second)));
}
#[test]
fn timer_mismatched_caller_thread_is_explicitly_unsupported_without_side_effects() {
    let mut runtime = fixture(timer_content());
    let actor = runtime.state().owned_admission.actors().unwrap()[&1].actor;
    let invoker = runtime.state().owned_admission.actors().unwrap()[&2].actor;
    let mut command = timer_start(invoker, actor, invoker, -5);
    if let AcceptedCommand::StartBehavior { context, .. } = &mut command {
        context.caller = actor;
    }
    let before = runtime.snapshot().unwrap();
    let tick = runtime.next_tick(vec![command]).unwrap();
    assert!(matches!(
        runtime.step(&tick),
        Err(RuntimeError::Vm(VmFault::UnsupportedPrimitive {
            opcode: 62,
            ..
        }))
    ));
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn timer_overlapping_actor_association_is_failure_atomic() {
    let (mut runtime, first) = timer_connected();
    let before = runtime.snapshot().unwrap();
    let tick = runtime
        .next_tick(vec![timer_start(
            first.actor,
            first.actor,
            first.object,
            -5,
        )])
        .unwrap();
    assert!(matches!(
        runtime.step(&tick),
        Err(RuntimeError::Vm(VmFault::UnsupportedPrimitive {
            opcode: 62,
            ..
        }))
    ));
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn timer_invoker_reset_disposes_source_fifo_but_cannot_prove_host_teardown() {
    let (mut runtime, session) = timer_connected();
    step(&mut runtime, vec![timer_event(session, 1, 0, vec![44])]);
    let outcome = step(
        &mut runtime,
        vec![AcceptedCommand::Reset {
            entity: session.invoker,
        }],
    );
    assert!(
        !runtime
            .state()
            .eod
            .connections
            .contains_key(&session.thread)
    );
    let retained = &runtime.state().eod.sessions[&session.session_id];
    assert_eq!(retained.source_detached_tick, Some(outcome.tick));
    assert!(retained.events.is_empty());
    assert!(!retained.ended);
    assert!(runtime.state().eod.actor_teardown_pending(session.actor));
    // Resetting an invoker that is a different avatar does not fake
    // ForceDisconnectObj; source ForceDisconnect(reset avatar) finds no match.
    assert!(
        !outcome
            .events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::EodDisconnectRequested { .. }))
    );
    step(&mut runtime, vec![timer_event(session, 2, -1, vec![])]);
    assert!(!runtime.state().eod.actor_teardown_pending(session.actor));
}

#[test]
fn timer_detached_check_cannot_inherit_live_connection() {
    let (mut runtime, session) = timer_connected();
    step(&mut runtime, vec![timer_event(session, 1, 0, vec![55])]);
    let before = runtime.snapshot().unwrap();
    for event in [0, -1, -2] {
        let result = runtime
            .query_behavior(
                session.invoker,
                RoutineKey {
                    scope: RoutineScope::SemiGlobal(900),
                    id: 8373,
                },
                FrameContext::for_entity(session.invoker, TEMPLATE_PERSON),
                vec![
                    session.actor.object_id.0,
                    session.object.object_id.0,
                    event,
                    0,
                ],
                100,
            )
            .unwrap();
        assert!(matches!(
            result.stop,
            VmStop::Faulted(VmFault::UnsupportedPrimitive { opcode: 62, .. })
        ));
        assert_eq!(runtime.snapshot().unwrap(), before);
    }
}

#[test]
fn timer_public_disposition_is_captured_before_same_tick_reset() {
    let (mut runtime, session) = timer_connected();
    let outcome = step(
        &mut runtime,
        vec![
            timer_event(session, 1, 0, vec![44]),
            AcceptedCommand::Reset {
                entity: session.invoker,
            },
        ],
    );
    assert!(
        runtime.state().eod.sessions[&session.session_id]
            .source_detached_tick
            .is_some()
    );
    assert!(outcome.events.iter().any(|e| matches!(e, RuntimeEvent::EodEventApplied { delivery, attached: true, .. } if delivery.event_id == 1)));
    let outcome = step(&mut runtime, vec![timer_event(session, 2, -1, vec![])]);
    assert!(outcome.events.iter().any(|e| matches!(e, RuntimeEvent::EodEventApplied { delivery, attached: false, .. } if delivery.event_id == 2)));
}

#[test]
fn timer_handlerless_polls_do_not_allocate_receipts_across_restore_and_replay() {
    let mut operand = [0, 1, 2, 0, 0, 0, 0, 0];
    operand[4..8].copy_from_slice(&sim_core::eod::TIMER_PLUGIN_ID.to_le_bytes());
    // An authored continuous InvokePlugin loop. Each live false-next-tick poll
    // returns through instruction 4 and reaches InvokePlugin again next tick.
    let mut authority = fixture(content(
        Some(vec![
            expr(Scope::Local, 0, Scope::Parameters, 0, 1),
            expr(Scope::Local, 1, Scope::Parameters, 1, 2),
            expr(Scope::Local, 2, Scope::Parameters, 2, 3),
            VmInstruction::new(62, 254, 4, operand),
            expr(Scope::Global, 0, Scope::Local, 2, 3),
        ]),
        false,
    ));
    let actor = authority.state().owned_admission.actors().unwrap()[&1].actor;
    let invoker = authority.state().owned_admission.actors().unwrap()[&2].actor;
    step(
        &mut authority,
        vec![timer_start(invoker, actor, invoker, -5)],
    );
    let session = authority.state().eod.sessions[&1].identity;
    let next_request = authority.state().eod.next_request_id;
    assert_eq!(next_request, 2);
    let mut replica = SimRuntime::from_state(
        authority.state().clone(),
        authority.content().clone(),
        RuntimeRole::Replica,
    )
    .unwrap();
    for index in 0..8192 {
        if index == 4096 {
            // Exact snapshot restart keeps the source association, not a new one.
            let saved = authority.snapshot().unwrap();
            authority.restore(&saved).unwrap();
            replica.restore(&saved).unwrap();
        }
        let tick = authority.next_tick(vec![]).unwrap();
        let actual = authority.step(&tick).unwrap();
        let replay = replica.step(&tick).unwrap();
        assert_eq!(actual.state_hash, replay.state_hash);
        assert_eq!(actual.events, replay.events);
        assert!(!actual.events.iter().any(|event| matches!(
            event,
            RuntimeEvent::EodSimanticsRequested { .. }
                | RuntimeEvent::EodConnectRequested { .. }
                | RuntimeEvent::EodDisconnectRequested { .. }
        )));
        assert!(actual.effects.is_empty());
        assert_eq!(authority.state().eod.next_request_id, next_request);
        assert_eq!(authority.state().eod.connections.get(&invoker), Some(&1));
        assert!(matches!(
            authority.state().threads[&invoker.object_id].stop,
            VmStop::Sleeping { .. }
        ));
    }
    // A real public code zero still applies once and reaches the source FIFO.
    let update = timer_event(session, 1, 0, vec![37]);
    step(&mut authority, vec![update.clone()]);
    step(&mut authority, vec![update]);
    assert_eq!(authority.state().eod.delivered.len(), 1);
    assert!(authority.state().eod.sessions[&1].events.is_empty());
    assert_eq!(authority.state().eod.next_request_id, next_request);
    // Handlerless optimization must not suppress the real source close effect.
    let closed = step(
        &mut authority,
        vec![timer_start(invoker, actor, invoker, -1)],
    );
    assert!(closed.events.iter().any(|event| matches!(
        event,
        RuntimeEvent::EodDisconnectRequested { request_id: 2, .. }
    )));
    assert_eq!(authority.state().eod.next_request_id, 3);
}
