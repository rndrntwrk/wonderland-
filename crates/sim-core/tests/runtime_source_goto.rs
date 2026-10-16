//! Authored source-contract harness, not original-content gameplay qualification.
use sim_core::{
    avatars::motives::TsoMotiveTuning,
    ids::{EntityRef, ObjectId, PersistentId},
    interactions::*,
    primitives::arithmetic::ExpressionOperand,
    runtime::{
        goto_adapter::{SourceGotoIntent, SOURCE_GOTO_GUID},
        *,
    },
    state::*,
    vm::*,
    world::*,
};
fn key(id: u16) -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(SOURCE_GOTO_GUID),
        id,
    }
}
fn expr(lhs: Scope, index: i16, rhs: Scope, value: i16, operator: u8, next: u8) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        ExpressionOperand {
            lhs: Variable::new(lhs, index),
            rhs: Variable::new(rhs, value),
            is_signed: 0,
            operator,
        }
        .encode(),
    )
}
fn content(include_goto: bool, invalid_init: bool, checked: bool) -> ContentSet {
    let mut routines = RoutineStore::new();
    let programs = [
        // Init observes OOW, initializes real placement flags and records order.
        (
            4096,
            vec![
                expr(Scope::MyObject, 29, Scope::Literal, -5, 2, 1),
                expr(Scope::MyObject, 42, Scope::Literal, 3, 5, 2),
                expr(Scope::MyObject, 4, Scope::Literal, 1, 5, 3),
                expr(Scope::MyObjectAttributes, 0, Scope::Literal, 1, 5, 4),
                expr(
                    Scope::Global,
                    0,
                    Scope::Literal,
                    1,
                    3,
                    if invalid_init { 5 } else { 254 },
                ),
                VmInstruction::new(655, 254, 255, [0; 8]),
            ],
        ),
        (
            4097,
            vec![expr(
                Scope::MyObjectAttributes,
                0,
                Scope::Literal,
                10,
                3,
                254,
            )],
        ),
        (
            4098,
            vec![expr(
                Scope::MyObjectAttributes,
                0,
                Scope::Literal,
                100,
                3,
                254,
            )],
        ),
        (
            4099,
            vec![expr(
                Scope::MyObjectAttributes,
                0,
                Scope::Literal,
                1000,
                3,
                254,
            )],
        ),
        (
            4100,
            vec![expr(
                Scope::MyObjectAttributes,
                0,
                Scope::Parameters,
                0,
                5,
                254,
            )],
        ),
        // A source check sees initialized ghost data, never Main or placement.
        (
            4101,
            vec![
                expr(Scope::StackObjectAttributes, 0, Scope::Literal, 11, 2, 1),
                expr(Scope::StackObject, 29, Scope::Literal, -1, 2, 254),
            ],
        ),
        (
            4102,
            vec![
                expr(Scope::Parameters, 0, Scope::Literal, 1000, 5, 1),
                VmInstruction::new(0, 0, 0, [0; 8]),
            ],
        ),
    ];
    for (id, program) in programs {
        routines
            .insert(key(id), VmRoutine::new(id, 0, 4, program).unwrap())
            .unwrap();
    }
    let avatar = ObjectDefinition::new(100, 4);
    let mut object = ObjectDefinition::new(SOURCE_GOTO_GUID, 4);
    object.entry_points.extend([
        (0, key(4096)),
        (8, key(4097)),
        (1, key(4098)),
        (9, key(4099)),
    ]);
    // Zero extent is source init data; placement still validates floor rules.
    object.object_data[8] = 4;
    let objects = if include_goto {
        vec![avatar, object]
    } else {
        vec![avatar]
    };
    let content = ContentSet::new(
        routines,
        objects,
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
    if !include_goto {
        return content;
    }
    content
        .with_interaction_tables(vec![(
            SOURCE_GOTO_GUID,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: InteractionKey {
                        tta_index: 7,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: 4100,
                        code_owner_guid: SOURCE_GOTO_GUID,
                    },
                    check: checked.then_some(RoutineBinding {
                        routine_id: 4101,
                        code_owner_guid: SOURCE_GOTO_GUID,
                    }),
                    flags: ActionFlags(
                        ActionFlags::ALLOW_VISITORS | ActionFlags::ALLOW_CONSECUTIVE,
                    ),
                    permissions: PermissionFlags::default(),
                    label: Some("Authored source GoTo".into()),
                }],
            },
        )])
        .unwrap()
}
fn fixture(include_goto: bool, invalid_init: bool) -> (SimRuntime, EntityRef) {
    fixture_content(content(include_goto, invalid_init, false))
}
fn fixture_content(content: ContentSet) -> (SimRuntime, EntityRef) {
    let mut r = SimRuntime::new(
        content,
        LotModel::new(8, 8, 2).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 7, 1, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    step(
        &mut r,
        vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 100,
            position: TilePos::new(1, 1, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(1),
            avatar: true,
        })],
    );
    let actor = r.state().entities[&ObjectId(1)].info.reference;
    step(
        &mut r,
        vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: Some(interaction_adapter::InteractionAccess {
                principal: PrincipalKey(9),
                allow_hidden: false,
            }),
        }],
    );
    (r, actor)
}
fn step(r: &mut SimRuntime, commands: Vec<AcceptedCommand>) -> TickOutcome {
    let tick = r.next_tick(commands).unwrap();
    r.step(&tick).unwrap()
}
fn intent(r: &SimRuntime, actor: EntityRef, sequence: u64) -> SourceGotoIntent {
    let offers = r
        .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
        .unwrap();
    SourceGotoIntent {
        principal: PrincipalKey(9),
        actor: offers.actor,
        world_revision: offers.world_revision,
        queue_revision: offers.queue_revision,
        command_sequence: sequence,
        x: 56,
        y: 56,
        level: 1,
        interaction: 7,
        param0: 0,
    }
}

#[test]
fn new_vm_uses_source_global_defaults_but_import_and_restore_keep_saved_values() {
    for mode in [VmMode::Tso, VmMode::Ts1] {
        let mut runtime = SimRuntime::new(
            content(false, false, false),
            LotModel::new(8, 8, 2).unwrap(),
            RuntimeConfig::new(mode, 7, 1, 123),
            RuntimeRole::Authority,
        )
        .unwrap();
        let mut expected = vec![0; 38];
        expected[20] = 255;
        expected[25] = 4;
        expected[17] = 4;
        assert_eq!(runtime.state().globals, expected);
        let mut imported_state = runtime.state().clone();
        imported_state.globals[20] = 0;
        imported_state.globals[25] = 0;
        imported_state.globals[17] = 9;
        let imported = SimRuntime::from_state(
            imported_state.clone(),
            runtime.content().clone(),
            RuntimeRole::Replica,
        )
        .unwrap();
        assert_eq!(imported.state(), &imported_state);
        runtime.restore(&imported.snapshot().unwrap()).unwrap();
        assert_eq!(runtime.state(), &imported_state);
    }
}

#[test]
fn detached_ghost_query_is_repeatable_without_allocating_or_running_main_or_placement() {
    let (r, actor) = fixture_content(content(true, false, true));
    let before = r.state().clone();
    let a = r
        .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
        .unwrap();
    let b = r
        .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.offers.len(), 1);
    assert_eq!(r.state(), &before);
}

#[test]
fn nonzero_current_object_refuses_only_detached_terrain_queries_without_mutation() {
    let (r, actor) = fixture(true, false);
    let command = intent(&r, actor, 1);
    let mut state = r.state().clone();
    let avatar = state
        .entities
        .get_mut(&actor.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap();
    avatar.person_data.values[76] = 99;
    avatar.person_data.values[75] = 17;
    let mut r = SimRuntime::from_state(state, r.content().clone(), RuntimeRole::Authority).unwrap();
    let before = r.state().clone();
    let error = r
        .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
        .unwrap_err();
    assert!(error.to_string().contains("selection mode"));
    assert_eq!(r.state(), &before);
    // VMNetGotoCmd has no CurrentObject gate; preserve its authority timing.
    step(&mut r, vec![AcceptedCommand::SourceGoto(command)]);
    assert_eq!(
        r.state().interaction_queues[&actor.object_id].last_command_sequence(),
        Some(1)
    );
}
#[test]
fn goto_creates_source_object_checked_and_queues_without_teleporting_actor() {
    let (mut r, actor) = fixture(true, false);
    let before = r.state().entities[&actor.object_id].info.position;
    let command = intent(&r, actor, 1);
    let tick = r
        .next_tick(vec![AcceptedCommand::SourceGoto(command)])
        .unwrap();
    let mut replica =
        SimRuntime::from_state(r.state().clone(), r.content().clone(), RuntimeRole::Replica)
            .unwrap();
    let a = r.step(&tick).unwrap();
    let b = replica.step(&tick).unwrap();
    assert_eq!(a.state_hash, b.state_hash);
    assert_eq!(a.events, b.events);
    assert_eq!(r.state().entities[&actor.object_id].info.position, before);
    let object = r
        .state()
        .entities
        .values()
        .find(|e| e.info.guid == SOURCE_GOTO_GUID)
        .unwrap();
    assert_eq!(
        object.info.position,
        VmPosition {
            x: 56,
            y: 56,
            level: 1
        }
    );
    assert_eq!(object.info.direction, 0);
    assert_eq!(object.attributes[0], 1111); // Init, dynamic, placement, then scheduled Main.
    assert_eq!(
        (
            object.object_data[45],
            object.object_data[46],
            object.object_data[47]
        ),
        (1997, 6, 1)
    );
    assert_eq!(
        r.state().world.object(object.info.reference).unwrap().owner,
        Some(PersistentId(0))
    );
    assert_eq!(
        r.state().interaction_queues[&actor.object_id].last_command_sequence(),
        Some(1)
    );
    assert!(r.step(&tick).unwrap().duplicate);
    assert_eq!(r.state_hash().unwrap(), a.state_hash);
}
#[test]
fn rejected_identity_stamps_variants_and_placement_are_failure_atomic() {
    let (mut r, actor) = fixture(true, false);
    let good = intent(&r, actor, 1);
    for variant in 0..8 {
        let mut bad = good;
        match variant {
            0 => bad.principal = PrincipalKey(99),
            1 => bad.actor.key.generation += 1,
            2 => bad.actor.revision += 1,
            3 => bad.world_revision += 1,
            4 => bad.queue_revision += 1,
            5 => bad.interaction = 99,
            6 => bad.level = 2,
            _ => bad.x = -1,
        };
        let before = r.state().clone();
        let tick = r.next_tick(vec![AcceptedCommand::SourceGoto(bad)]).unwrap();
        assert!(r.step(&tick).is_err(), "variant {variant}");
        assert_eq!(r.state(), &before);
    }
}
#[test]
fn missing_content_and_faulting_source_init_fail_closed_without_any_allocation() {
    for (available, fault) in [(false, false), (true, true)] {
        let (mut r, actor) = fixture(available, fault);
        let before = r.state().clone();
        assert!(r
            .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
            .is_err());
        assert_eq!(r.state(), &before);
        let actor_stamp = EntityVersion {
            key: interaction_adapter::entity_key(actor),
            revision: r.state().entities[&actor.object_id].revision,
        };
        let tick = r
            .next_tick(vec![AcceptedCommand::SourceGoto(SourceGotoIntent {
                principal: PrincipalKey(9),
                actor: actor_stamp,
                world_revision: r.state().completed_tick,
                queue_revision: r.state().interaction_queues[&actor.object_id].revision(),
                command_sequence: 1,
                x: 56,
                y: 56,
                level: 1,
                interaction: 7,
                param0: 0,
            })])
            .unwrap();
        assert!(r.step(&tick).is_err());
        assert_eq!(r.state(), &before);
    }
}

#[test]
fn queue_twenty_limit_cancel_and_replay_keep_source_sequence_contract() {
    let (mut r, actor) = fixture(true, false);
    step(
        &mut r,
        vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: key(4102),
            context: FrameContext::for_entity(actor, SOURCE_GOTO_GUID),
            args: vec![0; 4],
            replace: true,
        }],
    );
    for sequence in 1..=20 {
        let selected = intent(&r, actor, sequence);
        step(&mut r, vec![AcceptedCommand::SourceGoto(selected)]);
        assert_eq!(
            r.state().interaction_queues[&actor.object_id]
                .entries()
                .len(),
            sequence as usize
        );
    }
    let before = r.state().clone();
    assert!(r
        .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
        .is_err());
    let actor_stamp = EntityVersion {
        key: interaction_adapter::entity_key(actor),
        revision: r.state().entities[&actor.object_id].revision,
    };
    let rejected = SourceGotoIntent {
        principal: PrincipalKey(9),
        actor: actor_stamp,
        world_revision: r.state().completed_tick,
        queue_revision: r.state().interaction_queues[&actor.object_id].revision(),
        command_sequence: 21,
        x: 56,
        y: 56,
        level: 1,
        interaction: 7,
        param0: 0,
    };
    let tick = r
        .next_tick(vec![AcceptedCommand::SourceGoto(rejected)])
        .unwrap();
    assert!(r.step(&tick).is_err());
    assert_eq!(r.state(), &before);
    let queue = &r.state().interaction_queues[&actor.object_id];
    let cancel = CancelIntent {
        principal: PrincipalKey(9),
        world_revision: r.state().completed_tick,
        actor: actor_stamp,
        queue_revision: queue.revision(),
        command_sequence: 21,
        action: queue.entries()[19].id,
    };
    step(&mut r, vec![AcceptedCommand::CancelInteraction(cancel)]);
    assert_eq!(
        r.state().interaction_queues[&actor.object_id]
            .entries()
            .len(),
        19
    );
    let replay = intent(&r, actor, 20);
    let before = r.state().clone();
    let tick = r
        .next_tick(vec![AcceptedCommand::SourceGoto(replay)])
        .unwrap();
    assert!(r.step(&tick).is_err());
    assert_eq!(r.state(), &before);
}
#[test]
fn source_metadata_initializes_only_new_zero_pid_object_and_roundtrips_snapshot() {
    use sim_core::{
        lot_permissions::LotPermissionState,
        owned_admission::{OwnedAdmissionState, SourceJoinMetadata},
    };
    let (mut r, actor) = fixture(true, false);
    step(
        &mut r,
        vec![AcceptedCommand::InitializeLotPermissions {
            lot: LotPermissionState::default(),
        }],
    );
    let source = SourceJoinMetadata {
        source_import_sha256: [1; 32],
        content: r.state().content,
        neighborhood_id: 1,
        lot_location: 123,
        avatar_guids: [100].into(),
        april_fools_2019: false,
        object_flags: Default::default(),
        registered_groups: Default::default(),
    };
    step(
        &mut r,
        vec![AcceptedCommand::InitializeOwnedAdmission { source }],
    );
    let before = r.state().clone();
    let chosen = intent(&r, actor, 1);
    assert_eq!(r.state(), &before);
    step(&mut r, vec![AcceptedCommand::SourceGoto(chosen)]);
    let object = r
        .state()
        .entities
        .values()
        .find(|e| e.info.guid == SOURCE_GOTO_GUID)
        .unwrap();
    let OwnedAdmissionState::Initialized { source, .. } = &r.state().owned_admission else {
        panic!("admission missing")
    };
    assert_eq!(source.object_flags.len(), 1);
    assert_eq!(source.object_flags.get(&object.info.reference), Some(&0));
    assert!(source.registered_groups.is_empty());
    let bytes = r.snapshot().unwrap();
    let mut recovered = SimRuntime::new(
        r.content().clone(),
        LotModel::new(8, 8, 2).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 7, 1, 123),
        RuntimeRole::Replica,
    )
    .unwrap();
    recovered.restore(&bytes).unwrap();
    assert_eq!(r.state_hash().unwrap(), recovered.state_hash().unwrap());
    let deleted = object.info.reference;
    step(&mut r, vec![AcceptedCommand::Delete { entity: deleted }]);
    let OwnedAdmissionState::Initialized { source, .. } = &r.state().owned_admission else {
        panic!("admission missing")
    };
    assert!(source.object_flags.is_empty());
}
#[test]
fn occupied_and_unsupported_floor_tiles_do_not_offer_actions() {
    let (r, actor) = fixture(true, false);
    let before = r.state().clone();
    for at in [
        TilePos::new(1, 1, 1).center(),
        TilePos::new(3, 3, 2).center(),
        TilePos::new(-1, 3, 1).center(),
        LotPosition::new(55, 56, 1),
    ] {
        assert!(r.terrain_offers(PrincipalKey(9), actor, at).is_err());
        assert_eq!(r.state(), &before);
    }
}

#[test]
fn ghost_callbacks_use_empty_registries_id_one_and_reset_rng_before_menu_alias_checks() {
    let mut routines = RoutineStore::new();
    let random = |dest_scope: Scope, dest: u8, next| {
        VmInstruction::new(
            8,
            next,
            255,
            [
                dest,
                0,
                dest_scope as u8,
                0,
                232,
                3,
                Scope::Literal as u8,
                0,
            ],
        )
    };
    let next_avatar = VmInstruction::new(31, 255, 1, [0, 0, 0, 0, 129, Scope::Temps as u8, 0, 0]);
    let programs = [
        (
            4096,
            vec![
                next_avatar,
                expr(Scope::MyObject, 11, Scope::Literal, 1, 2, 2),
                random(Scope::MyObjectAttributes, 1, 3),
                expr(Scope::MyObjectAttributes, 0, Scope::Literal, 7, 5, 254),
            ],
        ),
        (
            4097,
            vec![
                random(Scope::MyObjectAttributes, 2, 1),
                expr(
                    Scope::MyObjectAttributes,
                    1,
                    Scope::MyObjectAttributes,
                    2,
                    2,
                    254,
                ),
            ],
        ),
        (4098, vec![VmInstruction::new(15, 254, 255, [0; 8])]),
        (4100, vec![VmInstruction::new(15, 254, 255, [0; 8])]),
        (
            4101,
            vec![
                expr(Scope::StackObjectAttributes, 0, Scope::Literal, 7, 2, 1),
                expr(Scope::StackObject, 11, Scope::Literal, 1, 2, 2),
                expr(Scope::StackObjectId, 0, Scope::Literal, 1, 2, 3),
                expr(
                    Scope::StackObjectAttributes,
                    1,
                    Scope::StackObjectAttributes,
                    2,
                    2,
                    4,
                ),
                random(Scope::Temps, 0, 5),
                expr(Scope::Temps, 0, Scope::StackObjectAttributes, 1, 2, 6),
                // Direct ghost StackObject initially has ID1. Assigning numeric1
                // resolves the real avatar through the restored source registry.
                expr(Scope::StackObjectId, 0, Scope::Literal, 1, 5, 7),
                expr(Scope::StackObjectAttributes, 0, Scope::Literal, 1234, 2, 8),
                expr(Scope::Temps, 1, Scope::Literal, 0, 5, 9),
                // The mirror is not a registered object in the live menu world.
                VmInstruction::new(31, 255, 254, [0xc4, 7, 0, 0, 132, Scope::Temps as u8, 0, 1]),
            ],
        ),
    ];
    for (id, program) in programs {
        routines
            .insert(key(id), VmRoutine::new(id, 0, 4, program).unwrap())
            .unwrap();
    }
    let mut avatar = ObjectDefinition::new(100, 4);
    avatar.attributes[0] = 1234;
    let mut ghost = ObjectDefinition::new(SOURCE_GOTO_GUID, 4);
    ghost.object_data[8] = 4;
    ghost.object_data[42] = 3;
    ghost.object_data[4] = 1;
    ghost
        .entry_points
        .extend([(0, key(4096)), (8, key(4097)), (1, key(4098))]);
    let content = ContentSet::new(
        routines,
        vec![avatar, ghost],
        vec![],
        TuningSet {
            tso_motives: Some(TsoMotiveTuning {
                flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                category_weights: [[1000; 7]; 11],
            }),
            ..Default::default()
        },
    )
    .unwrap()
    .with_interaction_tables(vec![(
        SOURCE_GOTO_GUID,
        InteractionTable {
            local_table_present: true,
            definitions: vec![InteractionDefinition {
                key: InteractionKey {
                    tta_index: 7,
                    scope: InteractionScope::Local,
                },
                action: RoutineBinding {
                    routine_id: 4100,
                    code_owner_guid: SOURCE_GOTO_GUID,
                },
                check: Some(RoutineBinding {
                    routine_id: 4101,
                    code_owner_guid: SOURCE_GOTO_GUID,
                }),
                flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                permissions: PermissionFlags::default(),
                label: Some("Source sandbox checks passed".into()),
            }],
        },
    )])
    .unwrap();
    let (r, actor) = fixture_content(content);
    let before = r.state().clone();
    let offers = r
        .terrain_offers(PrincipalKey(9), actor, TilePos::new(3, 3, 1).center())
        .unwrap();
    assert_eq!(offers.offers.len(), 1);
    assert_eq!(offers.offers[0].label, "Source sandbox checks passed");
    assert_eq!(r.state(), &before);
}

#[test]
fn authority_preserves_raw_param0_and_uses_real_placement_then_deferred_check() {
    let (mut r, actor) = fixture_content(content(true, false, true));
    let mut selected = intent(&r, actor, 1);
    selected.x = 25;
    selected.y = 24;
    selected.param0 = -123;
    // UI would refuse this occupied, off-center tile. The original network
    // command instead creates the zero-extent helper and checks its placement.
    assert!(r
        .terrain_offers(PrincipalKey(9), actor, LotPosition::new(25, 24, 1))
        .is_err());
    let tick = r
        .next_tick(vec![AcceptedCommand::SourceGoto(selected)])
        .unwrap();
    r.step(&tick).unwrap();
    let helper = r
        .state()
        .entities
        .values()
        .find(|e| e.info.guid == SOURCE_GOTO_GUID)
        .unwrap();
    assert_eq!(
        helper.info.position,
        VmPosition {
            x: 25,
            y: 24,
            level: 1
        }
    );
    assert_eq!(
        r.state().interaction_queues[&actor.object_id].last_command_sequence(),
        Some(1)
    );
    // The real object's post-placement attributes fail this authored check;
    // admission remains accepted and the queue declines the action naturally.
    assert_eq!(r.state().entities[&actor.object_id].attributes[0], 0);
}

#[test]
fn authoritative_source_action_receives_exact_signed_param0_and_zero_tail() {
    let (mut r, actor) = fixture(true, false);
    // Park the real caller so the newly accepted invocation remains inspectable.
    step(
        &mut r,
        vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: key(4102),
            context: FrameContext::for_entity(actor, SOURCE_GOTO_GUID),
            args: vec![0; 4],
            replace: true,
        }],
    );
    let mut selected = intent(&r, actor, 1);
    selected.param0 = -123;
    step(&mut r, vec![AcceptedCommand::SourceGoto(selected)]);
    let queued = &r.state().interaction_queues[&actor.object_id].entries()[0].invocation;
    assert_eq!(queued.args, [-123, 0, 0, 0]);
    assert_eq!(queued.definition.key.scope, InteractionScope::Local);
}
