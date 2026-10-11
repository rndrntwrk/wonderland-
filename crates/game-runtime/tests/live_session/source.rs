//! Real original routines exercised through the live session; declared fixtures
//! do not establish complete source-object or deployed-provider qualification.
use super::support::{
    OWNER, PRINCIPAL, content, pair_with_content, server_and_client, source_selection,
};
use super::*;
use sim_core::interactions::{PrincipalKey, QueryOptions, QueueEventKind};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_game_runtime::live_session::InteractionSelection;
use wonderland_game_runtime::{AcceptedCommand, RuntimeEvent};

#[test]
fn source_action_is_prepared_without_mutation_then_executes_on_both_replicas() {
    let (mut server, mut a, actor) = server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let mut b = LiveReplica::new(
        GameRuntime::new(
            server.sim().content().clone(),
            LotModel::new(8, 8, 1).unwrap(),
            RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
            RuntimeRole::Replica,
        )
        .unwrap(),
        StreamIdentity {
            browser_epoch: 12,
            ..identity()
        },
        ReplayLimits::default(),
    )
    .unwrap();
    install(&mut b, &server, &[]);
    let selection = source_selection(&a, actor, 0);
    // A harmless ordinary tick does not permanently disable the rendered menu.
    let idle = advance(&mut server);
    for replica in [&mut a, &mut b] {
        replica
            .apply_batch(replica.connection(), std::slice::from_ref(&idle))
            .unwrap();
    }
    let before = a.runtime().unwrap().snapshot().unwrap();
    let intent = a.prepare_interaction(a.connection(), selection).unwrap();
    assert_eq!(a.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(intent.param0, selection.param0);
    assert_eq!(intent.interaction, selection.interaction);
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    let expected = server.apply_accepted(&accepted).unwrap();
    let frame = TickFrame {
        accepted,
        state_hash: expected.state_hash,
    };
    for replica in [&mut a, &mut b] {
        let result = replica
            .apply_batch(replica.connection(), std::slice::from_ref(&frame))
            .unwrap();
        assert_eq!(result[0].events, expected.events);
        assert!(result[0].effects.is_empty());
        assert!(result[0].events.iter().any(|event| matches!(event,
            RuntimeEvent::Interaction(event) if matches!(event.kind,
                QueueEventKind::Finished { result: sim_core::interactions::FinishResult::Succeeded }))));
        assert_eq!(
            replica.runtime().unwrap().sim().state().entities[&actor.object_id].attributes,
            [0, 0, 30, 0]
        );
        assert_eq!(
            replica.runtime().unwrap().snapshot().unwrap(),
            server.snapshot().unwrap()
        );
        assert!(
            replica
                .apply_batch(replica.connection(), std::slice::from_ref(&frame))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn original_dynamic_menu_variants_keep_source_parameters_and_reject_invented_ones() {
    let (_, client, actor) = server_and_client("fso_christmas_flag.iff", 4108, true);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let batch = client
        .offers(
            client.connection(),
            PRINCIPAL,
            actor,
            actor,
            QueryOptions::default(),
        )
        .unwrap();
    assert_eq!(
        batch
            .offers
            .iter()
            .map(|o| (o.label.as_str(), o.param0))
            .collect::<Vec<_>>(),
        [
            ("Debug/Set Team/None", 0),
            ("Debug/Set Team/Elves", 1),
            ("Debug/Set Team/Reindeer", 2),
        ]
    );
    for param0 in 0..=2 {
        let selection = source_selection(&client, actor, param0);
        let intent = client
            .prepare_interaction(client.connection(), selection)
            .unwrap();
        assert_eq!(intent.param0, param0);
    }
    let mut selection = source_selection(&client, actor, 0);
    selection.param0 = 37;
    assert!(
        client
            .prepare_interaction(client.connection(), selection)
            .is_err()
    );
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
}

#[test]
fn selection_is_fenced_by_principal_entity_generation_session_and_readiness() {
    let (mut server, mut client, actor) =
        server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let selection = source_selection(&client, actor, 0);
    let token = client.connection();
    let before = client.runtime().unwrap().snapshot().unwrap();
    let mut wrong = selection;
    wrong.principal = PrincipalKey(43);
    assert!(client.prepare_interaction(token, wrong).is_err());
    wrong = selection;
    wrong.target.generation += 1;
    assert!(client.prepare_interaction(token, wrong).is_err());
    wrong = selection;
    wrong.actor.generation += 1;
    assert!(client.prepare_interaction(token, wrong).is_err());
    wrong = selection;
    wrong.command_sequence = 0;
    assert!(client.prepare_interaction(token, wrong).is_err());
    client.suspend(token).unwrap();
    assert!(client.prepare_interaction(token, selection).is_err());
    client.reconnect().unwrap();
    assert!(
        client
            .prepare_interaction(client.connection(), selection)
            .is_err()
    );
    install(&mut client, &server, &[]);
    assert!(client.prepare_interaction(token, selection).is_err());
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    // A newly accepted revocation removes the menu without mutating its source.
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: None,
        }])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    client
        .apply_batch(
            client.connection(),
            &[TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            }],
        )
        .unwrap();
    assert!(
        client
            .prepare_interaction(client.connection(), selection)
            .is_err()
    );
}

#[test]
fn typed_advertisement_map_is_an_admissible_native_command() {
    let (mut server, mut client, actor) =
        server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    // These tuple map keys are native simulation data, not JSON object keys.
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::SetInteractionProjection {
            entity: actor,
            queued_users: BTreeSet::new(),
            active_advertisements: Some(BTreeMap::from([((0, 7), 12)])),
        }])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    let frame = TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    };
    client.apply_batch(client.connection(), &[frame]).unwrap();
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}

#[test]
fn accepted_sequence_cannot_be_prepared_again_after_completion() {
    let (mut server, mut client, actor) =
        server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let selection = source_selection(&client, actor, 0);
    let intent = client
        .prepare_interaction(client.connection(), selection)
        .unwrap();
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    client
        .apply_batch(
            client.connection(),
            &[TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            }],
        )
        .unwrap();
    assert!(
        client
            .prepare_interaction(client.connection(), selection)
            .is_err()
    );
    let next = InteractionSelection {
        command_sequence: 2,
        ..selection
    };
    assert!(
        client
            .prepare_interaction(client.connection(), next)
            .is_ok()
    );
}

fn idle_content() -> ContentSet {
    use sim_core::interactions::{
        ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
        RoutineBinding,
    };
    use sim_core::state::{InteractionTable, ObjectDefinition};
    use sim_core::vm::{RoutineKey, RoutineScope, VmInstruction, VmRoutine};
    // Authored harness uses source Expression + IdleForInput primitives. It is
    // deliberately not described as an original BHAV or as a complete object.
    let mut routines = RoutineStore::new();
    for id in [4998, 4999] {
        let finish = if id == 4998 { 254 } else { 0 };
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::Private(OWNER),
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
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.entry_points.insert(
        1,
        RoutineKey {
            scope: RoutineScope::Private(OWNER),
            id: 4999,
        },
    );
    ContentSet::new(routines, vec![object], vec![], TuningSet::default())
        .unwrap()
        .with_interaction_tables(vec![(
            OWNER,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: InteractionKey {
                        tta_index: 7,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: 4998,
                        code_owner_guid: OWNER,
                    },
                    check: None,
                    flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                    permissions: PermissionFlags::default(),
                    label: Some("Idle harness".into()),
                }],
            },
        )])
        .unwrap()
}

#[test]
fn cancellation_preparation_is_read_only_and_accepted_idle_completion_clears_the_queue() {
    use wonderland_game_runtime::live_session::CancelSelection;
    let (mut server, mut client, actor) = pair_with_content(idle_content(), false);
    let invoke = client
        .prepare_interaction(client.connection(), source_selection(&client, actor, 0))
        .unwrap();
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::QueueInteraction(invoke)])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    client
        .apply_batch(
            client.connection(),
            &[TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            }],
        )
        .unwrap();
    let projection = client.projection().unwrap();
    let queue = &projection.queues[0];
    assert_eq!(queue.entries.len(), 1);
    assert!(queue.entries[0].active);
    let selection = CancelSelection {
        principal: PRINCIPAL,
        actor,
        action: sim_core::interactions::ActionId(queue.entries[0].id),
        command_sequence: 2,
    };
    let before = client.runtime().unwrap().snapshot().unwrap();
    for bad in [
        CancelSelection {
            command_sequence: 1,
            ..selection
        },
        CancelSelection {
            command_sequence: 0,
            ..selection
        },
        CancelSelection {
            principal: PrincipalKey(999),
            ..selection
        },
        CancelSelection {
            action: sim_core::interactions::ActionId(999),
            ..selection
        },
        CancelSelection {
            actor: wonderland_game_runtime::EntityRef {
                generation: actor.generation + 1,
                ..actor
            },
            ..selection
        },
    ] {
        assert!(client.prepare_cancel(client.connection(), bad).is_err());
    }
    let cancel = client
        .prepare_cancel(client.connection(), selection)
        .unwrap();
    assert_eq!(cancel.action, selection.action);
    assert_eq!(cancel.queue_revision, queue.revision);
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::CancelInteraction(cancel)])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    let result = client
        .apply_batch(
            client.connection(),
            &[TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            }],
        )
        .unwrap();
    assert_eq!(result[0].events, outcome.events);
    assert!(
        result[0].events.iter().any(|e| matches!(e,
        RuntimeEvent::Interaction(event) if matches!(event.kind, QueueEventKind::CancelRequested)))
    );
    assert!(client.projection().unwrap().queues[0].entries.is_empty());
    assert!(
        client
            .prepare_cancel(client.connection(), selection)
            .is_err()
    );
    assert_eq!(
        client.runtime().unwrap().sim().state().threads[&actor.object_id]
            .frames
            .len(),
        1
    );
}

#[test]
fn an_original_motive_routine_changes_live_needs_only_after_accepted_execution() {
    use sim_core::vm::{EntityField, MemoryAddress};
    let (mut server, mut client, actor) =
        pair_with_content(content("cursebook_set_permission.iff", 4107, false), true);
    let commands = [5, 7, 8]
        .into_iter()
        .map(|index| AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor,
                field: EntityField::Motive,
                index,
            },
            value: 0,
        })
        .collect();
    let accepted = server.sim().next_tick(commands).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    client
        .apply_batch(
            client.connection(),
            &[TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            }],
        )
        .unwrap();
    assert_eq!(
        client.projection().unwrap().entities[0]
            .needs
            .as_ref()
            .unwrap()
            .hunger,
        50
    );
    let before = client.runtime().unwrap().snapshot().unwrap();
    let intent = client
        .prepare_interaction(client.connection(), source_selection(&client, actor, 0))
        .unwrap();
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    client
        .apply_batch(
            client.connection(),
            &[TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            }],
        )
        .unwrap();
    let projection = client.projection().unwrap();
    let needs = projection.entities[0].needs.as_ref().unwrap();
    assert_eq!((needs.energy, needs.hunger, needs.hygiene), (100, 100, 100));
    assert_eq!(needs.room, None);
    assert_eq!(projection.entities[0].raw_motives.unwrap()[7], 100);
    assert!(projection.queues[0].entries.is_empty());
    assert_eq!(
        client.runtime().unwrap().snapshot().unwrap(),
        server.snapshot().unwrap()
    );
}
