#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;
use wonderland_game_runtime::live_wire::player::admission::revalidate_action;
use wonderland_game_runtime::live_wire::player::{PlayerAction, encode_action};
use wonderland_game_runtime::{AcceptedCommand, GameRuntime, PrincipalKey, RuntimeRole};

fn prepared() -> (GameRuntime, wonderland_game_runtime::EntityRef, Vec<u8>) {
    let (server, replica, actor) =
        support::server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let selection = support::source_selection(&replica, actor, 0);
    let intent = replica
        .prepare_interaction(replica.connection(), selection)
        .unwrap();
    (
        server,
        actor,
        encode_action(&PlayerAction::Invoke(intent)).unwrap(),
    )
}

#[test]
fn server_rechecks_the_source_action_after_ordinary_tick_latency_without_mutating_during_query() {
    let (mut server, actor, bytes) = prepared();
    for _ in 0..3 {
        server.advance(vec![]).unwrap();
    }
    let before = server.snapshot().unwrap();
    let action = revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap();
    assert_eq!(server.snapshot().unwrap(), before);
    let PlayerAction::Invoke(intent) = action else {
        panic!("Expected the exact invocation")
    };
    assert_eq!(
        intent.seen.world_revision,
        server.sim().state().completed_tick
    );
    assert_eq!(intent.command_sequence, 1);
    assert_eq!(intent.interaction.tta_index, 7);
    server
        .advance(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    assert_eq!(
        server.sim().state().entities[&actor.object_id].attributes,
        [0, 0, 30, 0]
    );
}

#[test]
fn server_does_not_accept_a_client_asserted_principal_or_actor() {
    let (server, actor, bytes) = prepared();
    assert!(revalidate_action(&server, PrincipalKey(999), actor, &bytes).is_err());
    let mut other = actor;
    other.generation += 1;
    assert!(revalidate_action(&server, support::PRINCIPAL, other, &bytes).is_err());
}

#[test]
fn revoked_source_authority_and_changed_queue_are_not_refreshed_away() {
    let (mut server, actor, bytes) = prepared();
    server
        .advance(vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: None,
        }])
        .unwrap();
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
    let (mut server, actor, bytes) = prepared();
    let PlayerAction::Invoke(intent) =
        revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap()
    else {
        unreachable!()
    };
    server
        .advance(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}

#[test]
fn future_and_expired_selections_are_rejected_not_retimed() {
    let (mut server, actor, bytes) = prepared();
    let mut action = wonderland_game_runtime::live_wire::player::decode_action(&bytes).unwrap();
    let PlayerAction::Invoke(ref mut intent) = action else {
        unreachable!()
    };
    intent.seen.world_revision += 100;
    assert!(
        revalidate_action(
            &server,
            support::PRINCIPAL,
            actor,
            &encode_action(&action).unwrap()
        )
        .is_err()
    );
    for _ in 0..129 {
        server.advance(vec![]).unwrap();
    }
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}

#[test]
fn server_revalidation_is_not_available_on_a_replica() {
    let (server, actor, bytes) = prepared();
    let mut config = wonderland_game_runtime::RuntimeConfig::new(
        wonderland_game_runtime::VmMode::Ts1,
        11,
        7,
        123,
    );
    config.limits = server.sim().state().limits.clone();
    let mut replica = GameRuntime::new(
        server.sim().content().clone(),
        server.sim().state().world.lot.clone(),
        config,
        RuntimeRole::Replica,
    )
    .unwrap();
    replica.restore(&server.snapshot().unwrap()).unwrap();
    assert!(revalidate_action(&replica, support::PRINCIPAL, actor, &bytes).is_err());
}

#[test]
fn stale_target_and_invented_source_variants_are_rejected_without_writes() {
    use wonderland_game_runtime::live_wire::player::decode_action;
    let (server, actor, bytes) = prepared();
    let before = server.snapshot().unwrap();
    for mutation in 0..3 {
        let mut action = decode_action(&bytes).unwrap();
        let PlayerAction::Invoke(ref mut intent) = action else {
            unreachable!()
        };
        match mutation {
            0 => intent.seen.target.key.generation += 1,
            1 => intent.interaction.tta_index = u32::MAX,
            _ => intent.param0 = 123,
        }
        assert!(
            revalidate_action(
                &server,
                support::PRINCIPAL,
                actor,
                &encode_action(&action).unwrap()
            )
            .is_err()
        );
        assert_eq!(server.snapshot().unwrap(), before);
    }
}

#[test]
fn original_dynamic_menu_parameters_are_rechecked_exactly_after_latency() {
    let (mut server, replica, actor) =
        support::server_and_client("fso_christmas_flag.iff", 4108, true);
    let requests: Vec<_> = (0..=2)
        .map(|param0| {
            let selection = support::source_selection(&replica, actor, param0);
            let intent = replica
                .prepare_interaction(replica.connection(), selection)
                .unwrap();
            encode_action(&PlayerAction::Invoke(intent)).unwrap()
        })
        .collect();
    for _ in 0..3 {
        server.advance(vec![]).unwrap();
    }
    let before = server.snapshot().unwrap();
    for (param0, bytes) in requests.iter().enumerate() {
        let PlayerAction::Invoke(intent) =
            revalidate_action(&server, support::PRINCIPAL, actor, bytes).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(intent.param0, param0 as i16);
        assert_eq!(intent.interaction.tta_index, 7);
        assert_eq!(
            intent.seen.world_revision,
            server.sim().state().completed_tick
        );
        assert_eq!(server.snapshot().unwrap(), before);
    }
}

#[test]
fn future_entity_observations_are_not_laundered_into_current_state() {
    use wonderland_game_runtime::live_wire::player::decode_action;
    let (server, actor, bytes) = prepared();
    for target in [false, true] {
        let mut action = decode_action(&bytes).unwrap();
        let PlayerAction::Invoke(ref mut intent) = action else {
            unreachable!()
        };
        if target {
            intent.seen.target.revision = u64::MAX;
        } else {
            intent.seen.actor.revision = u64::MAX;
        }
        assert!(
            revalidate_action(
                &server,
                support::PRINCIPAL,
                actor,
                &encode_action(&action).unwrap()
            )
            .is_err()
        );
    }
}

#[test]
fn observation_window_includes_its_boundary_and_rejects_malformed_packets() {
    use wonderland_game_runtime::live_wire::player::admission::MAX_SELECTION_AGE_TICKS;
    let (mut server, actor, bytes) = prepared();
    for _ in 0..MAX_SELECTION_AGE_TICKS {
        server.advance(vec![]).unwrap();
    }
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_ok());
    for truncated in [&bytes[..0], &bytes[..8], &bytes[..bytes.len() - 1]] {
        assert!(revalidate_action(&server, support::PRINCIPAL, actor, truncated).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &trailing).is_err());
    server.advance(vec![]).unwrap();
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}

// Declared test-only waiting action. Source primitive semantics are real, but
// this assembled two-instruction routine is not labelled an original resource.
fn waiting_content() -> wonderland_game_runtime::sim_core::state::ContentSet {
    use wonderland_game_runtime::sim_core::{
        interactions::{
            ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
            RoutineBinding,
        },
        state::{ContentSet, InteractionTable, ObjectDefinition, TuningSet},
        vm::{RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmRoutine},
    };
    let mut routines = RoutineStore::new();
    for id in [4998, 4999] {
        let finish = if id == 4998 { 254 } else { 0 };
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::Private(support::OWNER),
                    id,
                },
                VmRoutine::new(
                    id,
                    0,
                    4,
                    vec![
                        VmInstruction {
                            opcode: 2,
                            true_pointer: 1,
                            false_pointer: 1,
                            operand: [0, 0, 100, 0, 0, 5, 9, 7],
                        },
                        VmInstruction {
                            opcode: 17,
                            true_pointer: finish,
                            false_pointer: finish,
                            operand: [0, 0, u8::from(id == 4999), 0, 0, 0, 0, 0],
                        },
                    ],
                )
                .unwrap(),
            )
            .unwrap();
    }
    let mut object = ObjectDefinition::new(support::OWNER, 4);
    object.entry_points.insert(
        1,
        RoutineKey {
            scope: RoutineScope::Private(support::OWNER),
            id: 4999,
        },
    );
    ContentSet::new(routines, vec![object], vec![], TuningSet::default())
        .unwrap()
        .with_interaction_tables(vec![(
            support::OWNER,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: InteractionKey {
                        tta_index: 7,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: 4998,
                        code_owner_guid: support::OWNER,
                    },
                    check: None,
                    flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                    permissions: PermissionFlags::default(),
                    label: Some("Waiting admission harness".into()),
                }],
            },
        )])
        .unwrap()
}

#[test]
fn cancellation_revalidates_the_exact_visible_queue_item_after_latency() {
    use wonderland_game_runtime::sim_core::interactions::EntityVersion;
    use wonderland_game_runtime::{CancelIntent, entity_key};
    let (mut server, replica, actor) = support::pair_with_content(waiting_content(), false);
    let selection = support::source_selection(&replica, actor, 0);
    let intent = replica
        .prepare_interaction(replica.connection(), selection)
        .unwrap();
    server
        .advance(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    let state = server.sim().state();
    let queue = &state.interaction_queues[&actor.object_id];
    let action = queue.entries()[0].id;
    let cancel = CancelIntent {
        principal: support::PRINCIPAL,
        world_revision: state.completed_tick,
        actor: EntityVersion {
            key: entity_key(actor),
            revision: state.entities[&actor.object_id].revision,
        },
        queue_revision: queue.revision(),
        command_sequence: 2,
        action,
    };
    let bytes = encode_action(&PlayerAction::Cancel(cancel)).unwrap();
    for _ in 0..3 {
        server.advance(vec![]).unwrap();
    }
    let before = server.snapshot().unwrap();
    let PlayerAction::Cancel(updated) =
        revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(server.snapshot().unwrap(), before);
    assert_eq!(updated.action, action);
    assert_eq!(updated.queue_revision, cancel.queue_revision);
    assert_eq!(updated.command_sequence, 2);
    assert_eq!(updated.world_revision, server.sim().state().completed_tick);
    let mut unknown = cancel;
    unknown.action.0 = u64::MAX;
    assert!(
        revalidate_action(
            &server,
            support::PRINCIPAL,
            actor,
            &encode_action(&PlayerAction::Cancel(unknown)).unwrap()
        )
        .is_err()
    );
    server
        .advance(vec![AcceptedCommand::CancelInteraction(updated)])
        .unwrap();
    assert!(server.projection().queues[0].entries.is_empty());
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}

#[path = "support/source_terrain.rs"]
mod source_terrain;
fn prepared_terrain() -> (
    GameRuntime,
    wonderland_game_runtime::EntityRef,
    wonderland_game_runtime::sim_core::runtime::goto_adapter::SourceGotoIntent,
) {
    use wonderland_game_runtime::{TilePos, sim_core::runtime::goto_adapter::SourceGotoIntent};
    let (server, actor) = source_terrain::runtime();
    let batch = server
        .sim()
        .terrain_offers(support::PRINCIPAL, actor, TilePos::new(4, 4, 1).center())
        .unwrap();
    let intent = SourceGotoIntent {
        principal: support::PRINCIPAL,
        actor: batch.actor,
        world_revision: batch.world_revision,
        queue_revision: batch.queue_revision,
        command_sequence: 1,
        x: 72,
        y: 72,
        level: 1,
        interaction: 7,
        param0: 0,
    };
    (server, actor, intent)
}
#[test]
fn terrain_admission_preserves_source_selection_after_latency_and_refreshes_only_stamps() {
    let (mut server, actor, intent) = prepared_terrain();
    let bytes = encode_action(&PlayerAction::Goto(intent)).unwrap();
    for _ in 0..3 {
        server.advance(vec![]).unwrap();
    }
    let before = server.snapshot().unwrap();
    let PlayerAction::Goto(updated) =
        revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap()
    else {
        panic!()
    };
    assert_eq!(server.snapshot().unwrap(), before);
    assert_eq!(
        updated,
        wonderland_game_runtime::sim_core::runtime::goto_adapter::SourceGotoIntent {
            world_revision: server.sim().state().completed_tick,
            actor: wonderland_game_runtime::interactions::EntityVersion {
                key: intent.actor.key,
                revision: server.sim().state().entities[&actor.object_id].revision,
            },
            ..intent
        }
    );
    server
        .advance(vec![AcceptedCommand::SourceGoto(updated)])
        .unwrap();
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}
#[test]
fn terrain_admission_rejects_forged_identity_future_stamps_queue_and_missing_content_action() {
    let (server, actor, good) = prepared_terrain();
    let before = server.snapshot().unwrap();
    for variant in 0..8 {
        let mut bad = good;
        match variant {
            0 => bad.principal = PrincipalKey(99),
            1 => bad.actor.key.generation += 1,
            2 => bad.actor.revision = u64::MAX,
            3 => bad.world_revision = u64::MAX,
            4 => bad.queue_revision += 1,
            5 => bad.interaction += 1,
            6 => bad.level = -1,
            _ => bad.command_sequence = 0,
        }
        assert!(
            revalidate_action(
                &server,
                support::PRINCIPAL,
                actor,
                &encode_action(&PlayerAction::Goto(bad)).unwrap()
            )
            .is_err(),
            "variant {variant}"
        );
        assert_eq!(server.snapshot().unwrap(), before);
    }
    let bytes = encode_action(&PlayerAction::Goto(good)).unwrap();
    assert!(revalidate_action(&server, PrincipalKey(99), actor, &bytes).is_err());
}
#[test]
fn terrain_admission_does_not_refresh_expired_or_revoked_authority() {
    let (mut server, actor, intent) = prepared_terrain();
    let bytes = encode_action(&PlayerAction::Goto(intent)).unwrap();
    for _ in 0..=wonderland_game_runtime::live_wire::player::admission::MAX_SELECTION_AGE_TICKS {
        server.advance(vec![]).unwrap();
    }
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
    let (mut server, actor, intent) = prepared_terrain();
    server
        .advance(vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: None,
        }])
        .unwrap();
    assert!(
        revalidate_action(
            &server,
            support::PRINCIPAL,
            actor,
            &encode_action(&PlayerAction::Goto(intent)).unwrap()
        )
        .is_err()
    );
}

#[test]
fn terrain_admission_preserves_raw_param0_and_off_center_signed_coordinates_until_execution() {
    let (server, actor, good) = prepared_terrain();
    let before = server.snapshot().unwrap();
    for (x, y, param0) in [(25, 41, 99), (-1, 56, -123), (i16::MAX, i16::MIN, i16::MAX)] {
        let intent = wonderland_game_runtime::sim_core::runtime::goto_adapter::SourceGotoIntent {
            x,
            y,
            param0,
            ..good
        };
        let bytes = encode_action(&PlayerAction::Goto(intent)).unwrap();
        assert_eq!(
            revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap(),
            PlayerAction::Goto(intent)
        );
        assert_eq!(server.snapshot().unwrap(), before);
    }
}
