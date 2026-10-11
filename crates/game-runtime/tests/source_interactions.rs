//! Original BHAV bytes executed by sim-core. Object/lot setup is a declared
//! harness, not a claim that these partial resources are complete game objects.
use sim_core::{
    ids::{EntityRef, PersistentId},
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{FrameContext, PrimitiveExit, RoutineKey, RoutineScope, RoutineStore, VmMode, VmStop},
    world::{Facing, LotModel, TilePos},
};
use wonderland_content_runtime_bridge::import_bhav;
use wonderland_legacy_formats::{Limits, iff, semantic::decode_strings};

const OWNER: u32 = 0x0478_6aed;
fn source(name: &str) -> iff::IffFile {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    iff::decode(
        &std::fs::read(
            root.join("TSOClient/FSO.Content.TSO/Content/Objects")
                .join(name),
        )
        .unwrap(),
        &Limits::default(),
    )
    .unwrap()
}
fn original_content(name: &str, id: u16) -> ContentSet {
    let file = source(name);
    let chunk = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BHAV" && c.key.id == id)
        .unwrap();
    let mut store = RoutineStore::new();
    store
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id,
            },
            import_bhav(chunk, &Limits::default()).unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    let mut content = ContentSet::new(store, vec![object], vec![], TuningSet::default()).unwrap();
    if let Some(chunk) = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"STR#" && c.key.id == 302)
    {
        let strings = decode_strings(&chunk.data, &Limits::default()).unwrap();
        let values = strings.sets[0].iter().map(|s| s.value.text()).collect();
        content = content.with_strings(vec![((OWNER, 302), values)]).unwrap();
    }
    content
}
fn runtime(content: ContentSet) -> (SimRuntime, EntityRef) {
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let accepted = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    let outcome = runtime.step(&accepted).unwrap();
    let actor = outcome
        .events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::Spawned(r) = e {
                Some(*r)
            } else {
                None
            }
        })
        .unwrap();
    (runtime, actor)
}

#[test]
fn original_check_returns_detached_mutations_without_changing_live_state() {
    let (runtime, actor) = runtime(original_content("Casino_2-Tile_Bar_CC.iff", 4110));
    let before = runtime.snapshot().unwrap();
    let result = runtime
        .query_behavior_state(
            actor,
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            FrameContext::for_entity(actor, OWNER),
            vec![0; 4],
            100,
        )
        .unwrap();
    assert_eq!(
        result.thread.stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    assert_eq!(result.instructions, 3);
    assert_eq!(
        result.state.entities[&actor.object_id].attributes,
        [0, 0, 30, 0]
    );
    assert_eq!(
        runtime.state().entities[&actor.object_id].attributes,
        [10, 20, 30, 40]
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn original_check_preserves_every_dynamic_variant_and_parameter() {
    let (runtime, actor) = runtime(original_content("fso_christmas_flag.iff", 4108));
    let before = runtime.snapshot().unwrap();
    let result = runtime
        .query_behavior_state(
            actor,
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4108,
            },
            FrameContext::for_entity(actor, OWNER),
            vec![0; 4],
            100,
        )
        .unwrap();
    assert_eq!(
        result.thread.stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    let variants: Vec<_> = result
        .thread
        .action_strings
        .unwrap()
        .into_iter()
        .map(|s| (s.name, s.parameter0))
        .collect();
    assert_eq!(
        variants,
        [
            ("Debug/Set Team/None".into(), 0),
            ("Debug/Set Team/Elves".into(), 1),
            ("Debug/Set Team/Reindeer".into(), 2)
        ]
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[test]
fn detached_original_check_rejects_stale_callee_generation() {
    let (runtime, actor) = runtime(original_content("Casino_2-Tile_Bar_CC.iff", 4110));
    let before = runtime.snapshot().unwrap();
    let mut context = FrameContext::for_entity(actor, OWNER);
    context.callee.generation += 1;
    assert!(
        runtime
            .query_behavior_state(
                actor,
                RoutineKey {
                    scope: RoutineScope::Private(OWNER),
                    id: 4110
                },
                context,
                vec![0; 4],
                100
            )
            .is_err()
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
}

use sim_core::interactions::{
    ActionFlags, CancelIntent, EntityKey, InteractionDefinition, InteractionIntent, InteractionKey,
    InteractionScope, PermissionFlags, PrincipalKey, QueryOptions, QueueEventKind, RoutineBinding,
};
use sim_core::runtime::interaction_adapter::InteractionAccess;
use sim_core::state::InteractionTable;
fn with_table(content: ContentSet, id: u16, check: Option<u16>) -> ContentSet {
    content
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
                        routine_id: id,
                        code_owner_guid: OWNER,
                    },
                    check: check.map(|routine_id| RoutineBinding {
                        routine_id,
                        code_owner_guid: OWNER,
                    }),
                    flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                    permissions: PermissionFlags::default(),
                    label: Some("Original routine".into()),
                }],
            },
        )])
        .unwrap()
}
fn grant(runtime: &mut SimRuntime, actor: EntityRef) {
    let tick = runtime
        .next_tick(vec![AcceptedCommand::SetInteractionAuthority {
            actor,
            access: Some(InteractionAccess {
                principal: PrincipalKey(42),
                allow_hidden: false,
            }),
        }])
        .unwrap();
    runtime.step(&tick).unwrap();
}
fn offers(runtime: &SimRuntime, actor: EntityRef) -> sim_core::interactions::OfferBatch {
    runtime
        .interaction_offers(PrincipalKey(42), actor, actor, QueryOptions::default())
        .unwrap()
}
fn intent(runtime: &SimRuntime, actor: EntityRef, sequence: u64) -> InteractionIntent {
    let batch = offers(runtime, actor);
    InteractionIntent {
        principal: PrincipalKey(42),
        seen: batch.seen,
        queue_revision: runtime.state().interaction_queues[&actor.object_id].revision(),
        command_sequence: sequence,
        interaction: batch.offers[0].interaction,
        param0: batch.offers[0].param0,
    }
}

#[test]
fn available_original_variants_survive_authorized_offer_query() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("fso_christmas_flag.iff", 4108),
        4108,
        Some(4108),
    ));
    grant(&mut runtime, actor);
    let before = runtime.snapshot().unwrap();
    let batch = offers(&runtime, actor);
    let rows: Vec<_> = batch
        .offers
        .iter()
        .map(|o| (o.label.as_str(), o.param0))
        .collect();
    assert_eq!(
        rows,
        [
            ("Debug/Set Team/None", 0),
            ("Debug/Set Team/Elves", 1),
            ("Debug/Set Team/Reindeer", 2)
        ]
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
    assert!(
        runtime
            .interaction_offers(PrincipalKey(43), actor, actor, QueryOptions::default())
            .is_err()
    );
}

#[test]
fn accepted_queue_executes_original_routine_and_reports_actual_completion() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("Casino_2-Tile_Bar_CC.iff", 4110),
        4110,
        None,
    ));
    grant(&mut runtime, actor);
    let invocation = intent(&runtime, actor, 1);
    let input = runtime
        .next_tick(vec![AcceptedCommand::QueueInteraction(invocation)])
        .unwrap();
    let outcome = runtime.step(&input).unwrap();
    assert_eq!(
        runtime.state().entities[&actor.object_id].attributes,
        [0, 0, 30, 0]
    );
    let transitions: Vec<_> = outcome
        .events
        .iter()
        .filter_map(|e| {
            if let RuntimeEvent::Interaction(e) = e {
                Some(&e.kind)
            } else {
                None
            }
        })
        .collect();
    assert!(
        transitions
            .iter()
            .any(|e| matches!(e, QueueEventKind::Enqueued { .. }))
    );
    assert!(
        transitions
            .iter()
            .any(|e| matches!(e, QueueEventKind::Started { .. }))
    );
    assert!(transitions.iter().any(|e| matches!(
        e,
        QueueEventKind::Finished {
            result: sim_core::interactions::FinishResult::Succeeded
        }
    )));
    assert!(
        runtime.state().interaction_queues[&actor.object_id]
            .entries()
            .is_empty()
    );
}

#[test]
fn original_actions_keep_completing_after_diagnostic_history_is_full() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("Casino_2-Tile_Bar_CC.iff", 4110),
        4110,
        None,
    ));
    grant(&mut runtime, actor);
    for sequence in 1..=130 {
        let invocation = intent(&runtime, actor, sequence);
        let accepted = runtime
            .next_tick(vec![AcceptedCommand::QueueInteraction(invocation)])
            .unwrap();
        let outcome = runtime.step(&accepted).unwrap();
        assert!(outcome.events.iter().any(|e| matches!(e, RuntimeEvent::Interaction(e) if matches!(e.kind, QueueEventKind::Finished { result: sim_core::interactions::FinishResult::Succeeded }))), "source action {sequence} lost its frame return");
        assert!(
            runtime.state().interaction_queues[&actor.object_id]
                .entries()
                .is_empty()
        );
    }
    assert_eq!(
        runtime.state().threads[&actor.object_id].diagnostics.len(),
        128
    );
}

#[test]
fn accepted_cancellation_removes_pending_original_action_before_execution() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("Casino_2-Tile_Bar_CC.iff", 4110),
        4110,
        None,
    ));
    grant(&mut runtime, actor);
    let invocation = intent(&runtime, actor, 1);
    let cancel = CancelIntent {
        principal: PrincipalKey(42),
        world_revision: invocation.seen.world_revision,
        actor: invocation.seen.actor,
        queue_revision: 1,
        command_sequence: 2,
        action: sim_core::interactions::ActionId(1),
    };
    let input = runtime
        .next_tick(vec![
            AcceptedCommand::QueueInteraction(invocation),
            AcceptedCommand::CancelInteraction(cancel),
        ])
        .unwrap();
    let outcome = runtime.step(&input).unwrap();
    assert_eq!(
        runtime.state().entities[&actor.object_id].attributes,
        [10, 20, 30, 40]
    );
    assert!(
        runtime.state().interaction_queues[&actor.object_id]
            .entries()
            .is_empty()
    );
    assert!(outcome.events.iter().any(|e| matches!(e, RuntimeEvent::Interaction(e) if matches!(e.kind, QueueEventKind::Removed { reason:sim_core::interactions::RemovalReason::Cancelled }))));
    assert!(!outcome.events.iter().any(|e| matches!(e, RuntimeEvent::Interaction(e) if matches!(e.kind,QueueEventKind::Started{..}))));
}

#[test]
fn queue_revalidation_rejects_stale_generation_without_replacing_live_slot() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("Casino_2-Tile_Bar_CC.iff", 4110),
        4110,
        None,
    ));
    grant(&mut runtime, actor);
    let mut invocation = intent(&runtime, actor, 1);
    invocation.seen.target.key = EntityKey {
        slot: actor.object_id.0 as u32,
        generation: actor.generation + 1,
    };
    let before = runtime.snapshot().unwrap();
    let input = runtime
        .next_tick(vec![AcceptedCommand::QueueInteraction(invocation)])
        .unwrap();
    assert!(runtime.step(&input).is_err());
    assert_eq!(runtime.snapshot().unwrap(), before);
}

fn content_with_idle_main(action: Option<Vec<sim_core::vm::VmInstruction>>) -> ContentSet {
    use sim_core::vm::{VmInstruction, VmRoutine};
    let file = source("Casino_2-Tile_Bar_CC.iff");
    let mut store = RoutineStore::new();
    let action = action
        .map(|body| VmRoutine::new(4110, 0, 4, body).unwrap())
        .unwrap_or_else(|| {
            import_bhav(
                file.chunks
                    .iter()
                    .find(|c| c.key.kind == *b"BHAV" && c.key.id == 4110)
                    .unwrap(),
                &Limits::default(),
            )
            .unwrap()
        });
    store
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            action,
        )
        .unwrap();
    // Authored scheduler harness uses the actual source IdleForInput primitive.
    store
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4999,
            },
            VmRoutine::new(
                4999,
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
                        true_pointer: 0,
                        false_pointer: 0,
                        operand: [0, 0, 1, 0, 0, 0, 0, 0],
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    object.entry_points.insert(
        1,
        RoutineKey {
            scope: RoutineScope::Private(OWNER),
            id: 4999,
        },
    );
    with_table(
        ContentSet::new(store, vec![object], vec![], TuningSet::default()).unwrap(),
        4110,
        None,
    )
}

#[test]
fn real_idle_host_pushes_original_action_without_replacing_main_frame() {
    let (mut runtime, actor) = runtime(content_with_idle_main(None));
    assert!(
        !matches!(
            runtime.state().threads[&actor.object_id].stop,
            VmStop::Faulted(_)
        ),
        "idle main needs the real queue host"
    );
    grant(&mut runtime, actor);
    let request = intent(&runtime, actor, 1);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::QueueInteraction(request)])
        .unwrap();
    let outcome = runtime.step(&tick).unwrap();
    assert_eq!(
        runtime.state().entities[&actor.object_id].attributes,
        [0, 0, 30, 0]
    );
    assert_eq!(runtime.state().threads[&actor.object_id].frames.len(), 1);
    assert_eq!(
        runtime.state().threads[&actor.object_id].frames[0]
            .routine
            .id,
        4999
    );
    assert!(outcome.events.iter().any(|e|matches!(e,RuntimeEvent::Interaction(e) if matches!(e.kind,QueueEventKind::Finished{..}))));
}

#[test]
fn active_cancel_notifies_real_idle_primitive_and_waits_for_frame_return() {
    use sim_core::vm::VmInstruction;
    let action = vec![
        VmInstruction {
            opcode: 2,
            true_pointer: 1,
            false_pointer: 1,
            operand: [0, 0, 100, 0, 0, 5, 9, 7],
        },
        VmInstruction {
            opcode: 17,
            true_pointer: 254,
            false_pointer: 255,
            operand: [0, 0, 0, 0, 0, 0, 0, 0],
        },
    ];
    let (mut runtime, actor) = runtime(content_with_idle_main(Some(action)));
    grant(&mut runtime, actor);
    let request = intent(&runtime, actor, 1);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::QueueInteraction(request)])
        .unwrap();
    let outcome = runtime.step(&tick).unwrap();
    let queue = &runtime.state().interaction_queues[&actor.object_id];
    assert_eq!(
        queue.active_len(),
        1,
        "source action must actually be suspended"
    );
    assert!(!outcome.events.iter().any(|e|matches!(e,RuntimeEvent::Interaction(e) if matches!(e.kind,QueueEventKind::Finished{..}))));
    let actor_version = offers(&runtime, actor).seen.actor;
    let request = CancelIntent {
        principal: PrincipalKey(42),
        world_revision: runtime.state().completed_tick,
        actor: actor_version,
        queue_revision: queue.revision(),
        command_sequence: 2,
        action: queue.active().unwrap().id,
    };
    let tick = runtime
        .next_tick(vec![AcceptedCommand::CancelInteraction(request)])
        .unwrap();
    let outcome = runtime.step(&tick).unwrap();
    assert!(outcome.events.iter().any(|e|matches!(e,RuntimeEvent::Interaction(e) if matches!(e.kind,QueueEventKind::CancelRequested))));
    assert!(outcome.events.iter().any(|e|matches!(e,RuntimeEvent::Interaction(e) if matches!(e.kind,QueueEventKind::Finished{..}))));
    assert!(
        runtime.state().interaction_queues[&actor.object_id]
            .entries()
            .is_empty()
    );
    assert_eq!(runtime.state().threads[&actor.object_id].frames.len(), 1);
}

#[test]
fn missing_opcode_cannot_finish_an_accepted_action_successfully() {
    use sim_core::vm::{VmInstruction, VmRoutine};
    let mut store = RoutineStore::new();
    store
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            VmRoutine::new(
                4110,
                0,
                4,
                vec![VmInstruction {
                    opcode: 254,
                    true_pointer: 254,
                    false_pointer: 255,
                    operand: [0; 8],
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let content = with_table(
        ContentSet::new(
            store,
            vec![ObjectDefinition::new(OWNER, 0)],
            vec![],
            TuningSet::default(),
        )
        .unwrap(),
        4110,
        None,
    );
    let (mut runtime, actor) = runtime(content);
    grant(&mut runtime, actor);
    let request = intent(&runtime, actor, 1);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::QueueInteraction(request)])
        .unwrap();
    let outcome = runtime.step(&tick).unwrap();
    assert!(!outcome.events.iter().any(|e|matches!(e,RuntimeEvent::Interaction(e) if matches!(e.kind,QueueEventKind::Finished{result:sim_core::interactions::FinishResult::Succeeded}))));
    assert!(
        outcome
            .events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::ThreadFault { .. }))
    );
}

#[test]
fn restored_queue_rejects_impossible_active_prefix() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("Casino_2-Tile_Bar_CC.iff", 4110),
        4110,
        None,
    ));
    grant(&mut runtime, actor);
    let mut state = runtime.state().clone();
    let mut queue = serde_json::to_value(&state.interaction_queues[&actor.object_id]).unwrap();
    queue["active_len"] = serde_json::json!(1);
    state
        .interaction_queues
        .insert(actor.object_id, serde_json::from_value(queue).unwrap());
    assert!(
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Replica).is_err()
    );
}

#[test]
fn in_tick_original_checks_spend_real_instruction_budget() {
    let (mut runtime, actor) = runtime(with_table(
        original_content("Casino_2-Tile_Bar_CC.iff", 4110),
        4110,
        Some(4110),
    ));
    grant(&mut runtime, actor);
    let request = intent(&runtime, actor, 1);
    let tick = runtime
        .next_tick(vec![AcceptedCommand::QueueInteraction(request)])
        .unwrap();
    let outcome = runtime.step(&tick).unwrap();
    assert_eq!(
        outcome.instructions, 9,
        "three intent-validation checks, three in-tick checks, and three action assignments"
    );
}

#[test]
fn previous_schema_one_snapshot_migrates_without_changing_original_state() {
    use sha2::{Digest, Sha256};
    let (runtime, actor) = runtime(original_content("Casino_2-Tile_Bar_CC.iff", 4110));
    let mut state = runtime.state().clone();
    state.interaction_queues.clear();
    let runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Replica).unwrap();
    let current = runtime.snapshot().unwrap();
    // Schema 2 appended two empty maps to the unchanged schema-1 state tuple.
    // This is the actual original fixed-width bincode representation.
    let end = current.len() - 32 - 16;
    let mut legacy = current[..end].to_vec();
    legacy[10..12].copy_from_slice(&1u16.to_le_bytes());
    legacy[108..110].copy_from_slice(&1u16.to_le_bytes());
    legacy[100..108].copy_from_slice(&((end - 108) as u64).to_le_bytes());
    let hash = Sha256::digest(&legacy);
    legacy.extend_from_slice(&hash);
    let mut restored = runtime;
    restored.restore(&legacy).unwrap();
    assert_eq!(
        restored.state().entities[&actor.object_id].attributes,
        [10, 20, 30, 40]
    );
    assert_eq!(restored.state().completed_tick, 1);
    assert_eq!(
        restored.state().interaction_queues[&actor.object_id].owner(),
        entity_key_for(actor)
    );
}
fn entity_key_for(actor: EntityRef) -> EntityKey {
    EntityKey {
        slot: actor.object_id.0 as u32,
        generation: actor.generation,
    }
}

#[test]
fn original_motive_helper_changes_needs_through_accepted_game_runtime() {
    use sim_core::vm::{EntityField, MemoryAddress};
    use wonderland_game_runtime::GameRuntime;
    let content = with_table(
        original_content("cursebook_set_permission.iff", 4107),
        4107,
        None,
    );
    let mut game = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let outcome = game
        .advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(7),
            avatar: true,
        })])
        .unwrap();
    let actor = outcome
        .events
        .iter()
        .find_map(|e| {
            if let RuntimeEvent::Spawned(r) = e {
                Some(*r)
            } else {
                None
            }
        })
        .unwrap();
    let mut commands = vec![AcceptedCommand::SetInteractionAuthority {
        actor,
        access: Some(InteractionAccess {
            principal: PrincipalKey(42),
            allow_hidden: false,
        }),
    }];
    for index in [5, 7, 8] {
        commands.push(AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor,
                field: EntityField::Motive,
                index,
            },
            value: 0,
        });
    }
    game.advance(commands).unwrap();
    let before = game.projection();
    assert_eq!(before.entities[0].needs.as_ref().unwrap().hunger, 50);
    let request = intent(game.sim(), actor, 1);
    game.invoke(request).unwrap();
    let after = game.projection();
    let needs = after.entities[0].needs.as_ref().unwrap();
    assert_eq!((needs.energy, needs.hunger, needs.hygiene), (100, 100, 100));
    assert_eq!(after.entities[0].raw_motives.unwrap()[7], 100);
    assert!(after.queues[0].entries.is_empty());
}

#[test]
fn tso_avatar_without_original_motive_tuning_is_rejected_atomically() {
    use wonderland_game_runtime::GameRuntime;
    let content = original_content("cursebook_set_permission.iff", 4107);
    let mut game = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let before = game.snapshot().unwrap();
    assert!(
        game.advance(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(7),
            avatar: true
        })])
        .is_err()
    );
    assert_eq!(game.snapshot().unwrap(), before);
}
