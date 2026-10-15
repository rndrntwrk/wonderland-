//! Authored waiting-action harness shared by host unit/process tests. This is
//! never source-content qualification and is not a production loader fallback.
use wonderland_game_runtime::sim_core::{
    avatars::{motives::TsoMotiveTuning, state::TEMPLATE_PERSON},
    interactions::{
        ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
        RoutineBinding,
    },
    state::{ContentSet, InteractionTable, ObjectDefinition, TuningSet},
    vm::{RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmRoutine},
};

pub fn content() -> ContentSet {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(TEMPLATE_PERSON),
                id: 4998,
            },
            VmRoutine::new(
                4998,
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
                        true_pointer: 254,
                        false_pointer: 254,
                        operand: [0; 8],
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();
    // Authored placement flags permit the unchanged join fallback to place the
    // actor on this empty lot. No checkpoint or accepted state is patched later.
    let mut person = ObjectDefinition::new(TEMPLATE_PERSON, 0);
    person.object_data[4] = 1;
    person.object_data[42] = person.placement_rules.flags as i16;
    ContentSet::new(
        routines,
        vec![person],
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
        TEMPLATE_PERSON,
        InteractionTable {
            local_table_present: true,
            definitions: vec![InteractionDefinition {
                key: InteractionKey {
                    tta_index: 7,
                    scope: InteractionScope::Local,
                },
                action: RoutineBinding {
                    routine_id: 4998,
                    code_owner_guid: TEMPLATE_PERSON,
                },
                check: None,
                flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                permissions: PermissionFlags::default(),
                label: Some("Authored waiting-action receipt harness".into()),
            }],
        },
    )])
    .unwrap()
}

/// Read-only test aid: derive bytes from an actual admitted actor's current
/// offers/visible queue. The production host still revalidates and executes.
pub fn player_action(
    host: &crate::runtime::Host,
    principal: u64,
    sequence: u64,
    cancel: bool,
) -> crate::Result<serde_json::Value> {
    use wonderland_game_runtime::{
        CancelIntent, InteractionIntent, PrincipalKey, QueryOptions, entity_key,
        live_wire::player::{PlayerAction, encode_action},
        sim_core::interactions::EntityVersion,
    };
    use wonderland_native_host_contracts::DecimalId;
    let state = host.committed.sim().state();
    let owned = state
        .owned_admission
        .actors()
        .and_then(|actors| actors.get(&principal))
        .ok_or("test actor not admitted")?;
    let queue = state
        .interaction_queues
        .get(&owned.actor.object_id)
        .ok_or("test actor queue unavailable")?;
    let action = if cancel {
        let entry = queue
            .entries()
            .iter()
            .enumerate()
            .find(|(index, _)| queue.entry_visible(*index) == Some(true))
            .map(|(_, entry)| entry)
            .ok_or("test visible action unavailable")?;
        PlayerAction::Cancel(CancelIntent {
            principal: PrincipalKey(principal),
            world_revision: state.completed_tick,
            actor: EntityVersion {
                key: entity_key(owned.actor),
                revision: state.entities[&owned.actor.object_id].revision,
            },
            queue_revision: queue.revision(),
            command_sequence: sequence,
            action: entry.id,
        })
    } else {
        let offers = host.committed.offers(
            PrincipalKey(principal),
            owned.actor,
            owned.actor,
            QueryOptions::default(),
        )?;
        let offer = offers
            .offers
            .first()
            .ok_or("test source offer unavailable")?;
        PlayerAction::Invoke(InteractionIntent {
            principal: PrincipalKey(principal),
            seen: offers.seen,
            queue_revision: queue.revision(),
            command_sequence: sequence,
            interaction: offer.interaction,
            param0: offer.param0,
        })
    };
    Ok(serde_json::json!({
        "kind":"fixture_player_action",
        "control":{
            "principal":DecimalId(principal),"avatar_id":DecimalId(u64::from(owned.avatar_id.0)),
            "control_generation":DecimalId(owned.control_generation),"session_ref":crate::runtime::hex(&owned.session_ref)
        },
        "action_hex":crate::runtime::hex(&encode_action(&action)?)
    }))
}
