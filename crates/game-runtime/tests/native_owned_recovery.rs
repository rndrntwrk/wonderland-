//! Authored protocol harness only; no original-content gameplay qualification.
use std::collections::BTreeMap;
use wonderland_game_runtime::{
    AcceptedCommand, GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    live_session::Checkpoint,
    live_wire::{
        WireLimits, encode_checkpoint,
        player::{
            ActionReceipt, ActionStatus, Bootstrap, NativePlayer, PlayerAction, PlayerBinding,
            decode_action, encode_bootstrap, encode_receipt,
        },
    },
    sim_core::{
        avatars::{AvatarPlatform, AvatarState, motives::TsoMotiveTuning, state::TEMPLATE_PERSON},
        ids::{EntityRef, ObjectId, PersistentId},
        interactions::{
            ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
            PrincipalKey, RoutineBinding,
        },
        lot_permissions::LotPermissionState,
        owned_admission::{
            OwnedAvatarAdmission, OwnedControl, SourceJoinMetadata, permission_digest,
        },
        state::{ContentSet, InteractionTable, ObjectDefinition, TuningSet},
        vm::{RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmRoutine},
    },
    world_view::WorldDocument,
};
#[path = "support/source_terrain.rs"]
mod source_terrain;
fn setup() -> (GameRuntime, Bootstrap) {
    let mut routines = RoutineStore::new();
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(TEMPLATE_PERSON),
                id: 4096,
            },
            VmRoutine::new(4096, 0, 4, vec![VmInstruction::new(15, 254, 255, [0; 8])]).unwrap(),
        )
        .unwrap();
    let mut objects = vec![ObjectDefinition::new(TEMPLATE_PERSON, 0)];
    let terrain = source_terrain::extend(&mut routines, &mut objects);
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
    .unwrap()
    .with_interaction_tables(vec![
        terrain,
        (
            TEMPLATE_PERSON,
            InteractionTable {
                local_table_present: true,
                definitions: vec![InteractionDefinition {
                    key: InteractionKey {
                        tta_index: 7,
                        scope: InteractionScope::Local,
                    },
                    action: RoutineBinding {
                        routine_id: 4096,
                        code_owner_guid: TEMPLATE_PERSON,
                    },
                    check: None,
                    flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
                    permissions: PermissionFlags::default(),
                    label: Some("Authored protocol action".into()),
                }],
            },
        ),
    ])
    .unwrap();
    let mut game = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Tso, 41, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let source = SourceJoinMetadata {
        source_import_sha256: [1; 32],
        content: game.sim().state().content,
        neighborhood_id: 8,
        lot_location: 16777472,
        avatar_guids: [TEMPLATE_PERSON].into(),
        april_fools_2019: false,
        object_flags: BTreeMap::new(),
        registered_groups: BTreeMap::new(),
    };
    game.advance(vec![
        AcceptedCommand::InitializeLotPermissions {
            lot: LotPermissionState::default(),
        },
        AcceptedCommand::InitializeOwnedAdmission { source },
    ])
    .unwrap();
    let portable = AvatarState::new(
        EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        },
        PersistentId(7),
        AvatarPlatform::Tso,
    )
    .persistent_state()
    .unwrap();
    game.advance(vec![AcceptedCommand::AdmitOwnedAvatar(
        OwnedAvatarAdmission {
            control: OwnedControl {
                principal: 9,
                account_id: 10,
                avatar_id: PersistentId(7),
                actor: EntityRef {
                    object_id: ObjectId(0),
                    generation: 0,
                },
                control_generation: 3,
                session_ref: [7; 32],
                admission_operation: "owned-recovery-0001".into(),
                reservation_digest: [2; 32],
                export_digest: [3; 32],
            },
            portable,
            permission_revision: 1,
            permission_sha256: permission_digest(game.sim().state()).unwrap(),
        },
    )])
    .unwrap();
    // Authored checkpoint geometry: this protocol harness has no source mailbox
    // or join/routing assets, and makes no movement/admission-content claim.
    let mut state = game.sim().state().clone();
    let actor = state.owned_admission.actors().unwrap()[&9].actor;
    state
        .entities
        .get_mut(&actor.object_id)
        .unwrap()
        .info
        .position = wonderland_game_runtime::sim_core::vm::VmPosition {
        x: 56,
        y: 56,
        level: 1,
    };
    wonderland_game_runtime::sim_core::runtime_memory::sync_projection(&mut state, actor).unwrap();
    let bytes =
        wonderland_game_runtime::sim_core::snapshot::encode(&state, game.sim().content()).unwrap();
    game.restore(&bytes).unwrap();
    let state = game.sim().state();
    let bootstrap = Bootstrap {
        binding: PlayerBinding {
            source_epoch: 3,
            lot_incarnation: 4,
            lot_location: 16777472,
            avatar_id: 7,
        },
        principal: PrincipalKey(9),
        actor: state.owned_admission.actors().unwrap()[&9].actor,
        content: game.sim().content().clone(),
        appearance: WorldDocument::from_blueprint_xml(
            "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
            "test:owned-recovery",
            "authored-fixture",
        )
        .unwrap(),
        lot: state.world.lot.clone(),
        mode: state.mode,
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        effect_namespace: state.effects.namespace(),
        limits: state.limits.clone(),
        effect_limits: state.effects.limits(),
    };
    (game, bootstrap)
}
fn install(p: &mut NativePlayer, game: &GameRuntime) {
    let bytes = game.snapshot().unwrap();
    p.receive(
        &encode_checkpoint(
            p.checkpoint_request().unwrap().id,
            Checkpoint {
                completed_tick: game.sim().state().completed_tick,
                state_hash: game.sim().state_hash().unwrap(),
                bytes: &bytes,
            },
            &[],
            WireLimits::default(),
        )
        .unwrap(),
    )
    .unwrap();
}
fn player(game: &GameRuntime, b: &Bootstrap) -> NativePlayer {
    let mut p = NativePlayer::open(&encode_bootstrap(b).unwrap(), b.binding, 2).unwrap();
    install(&mut p, game);
    p
}
fn prepare(p: &mut NativePlayer) -> Vec<u8> {
    let offers = p.offers(p.actor()).unwrap();
    assert!(!offers.offers.is_empty());
    p.prepare(
        p.actor(),
        offers.offers[0].interaction,
        offers.offers[0].param0,
    )
    .unwrap()
}
fn newer(game: &mut GameRuntime, b: &Bootstrap) -> Bootstrap {
    let mut next = b.clone();
    next.binding.lot_incarnation += 1;
    game.adopt_authority_epoch(game.sim().state().authority_epoch + 1)
        .unwrap();
    next.authority_epoch = game.sim().state().authority_epoch;
    next
}
#[test]
fn accepted_reply_loss_survives_new_incarnation_without_importing_or_resending_old_state() {
    let (mut game, b) = setup();
    let mut old = player(&game, &b);
    let request = prepare(&mut old);
    let PlayerAction::Invoke(intent) = decode_action(&request).unwrap() else {
        panic!()
    };
    let tick = game
        .advance(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap()
        .tick;
    old.disconnect();
    let capsule = old.recovery_capsule().unwrap();
    let next = newer(&mut game, &b);
    let mut fresh = player(&game, &next);
    let hash = fresh.state_hash().unwrap();
    fresh.restore_recovery(capsule).unwrap();
    assert_eq!(fresh.state_hash().unwrap(), hash);
    assert_eq!(fresh.status(), ActionStatus::Unknown);
    let query = fresh.pending_action_result().unwrap().unwrap();
    assert_eq!(query.lot_incarnation, 4);
    assert_eq!(query.request, request);
    assert!(
        fresh
            .prepare(
                fresh.actor(),
                InteractionKey {
                    tta_index: 7,
                    scope: InteractionScope::Local
                },
                0
            )
            .is_err()
    );
    fresh
        .receive(
            &encode_receipt(&ActionReceipt {
                request,
                accepted_tick: Some(tick),
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(fresh.status(), ActionStatus::Accepted);
    assert!(fresh.pending_action_result().unwrap().is_none());
    assert_eq!(decode_action(&prepare(&mut fresh)).unwrap().sequence(), 2);
}
#[test]
fn lookup_without_evidence_stays_unknown_and_repeated_takeovers_keep_original_incarnation() {
    let (mut game, b) = setup();
    let mut old = player(&game, &b);
    let request = prepare(&mut old);
    old.disconnect();
    let capsule = old.recovery_capsule().unwrap();
    let next = newer(&mut game, &b);
    let mut fresh = player(&game, &next);
    fresh.restore_recovery(capsule).unwrap();
    fresh.disconnect();
    let capsule = fresh.recovery_capsule().unwrap();
    let next = newer(&mut game, &next);
    let mut fresh = player(&game, &next);
    fresh.restore_recovery(capsule).unwrap();
    assert_eq!(fresh.status(), ActionStatus::Unknown);
    let query = fresh.pending_action_result().unwrap().unwrap();
    assert_eq!(query.lot_incarnation, 4);
    assert_eq!(query.request, request);
    fresh.dismiss_unknown().unwrap();
    assert!(fresh.pending_action_result().unwrap().is_none());
    assert_eq!(decode_action(&prepare(&mut fresh)).unwrap().sequence(), 2);
}
#[test]
fn rejected_receipt_and_dismissal_preserve_sequence_highwater_across_replacement() {
    for dismiss in [false, true] {
        let (mut game, b) = setup();
        let mut old = player(&game, &b);
        let request = prepare(&mut old);
        old.disconnect();
        old.reconnect().unwrap();
        install(&mut old, &game);
        if dismiss {
            old.dismiss_unknown().unwrap();
        } else {
            old.receive(
                &encode_receipt(&ActionReceipt {
                    request,
                    accepted_tick: None,
                })
                .unwrap(),
            )
            .unwrap();
        }
        let capsule = old.recovery_capsule().unwrap();
        let next = newer(&mut game, &b);
        let mut fresh = player(&game, &next);
        fresh.restore_recovery(capsule).unwrap();
        assert_eq!(decode_action(&prepare(&mut fresh)).unwrap().sequence(), 2);
    }
}
#[test]
fn capsule_requires_new_checkpoint_and_same_full_committed_control() {
    for mutation in 0..8 {
        let (mut game, b) = setup();
        let mut old = player(&game, &b);
        prepare(&mut old);
        old.disconnect();
        let capsule = old.recovery_capsule().unwrap();
        let mut next = newer(&mut game, &b);
        if mutation == 0 {
            let mut fresh =
                NativePlayer::open(&encode_bootstrap(&next).unwrap(), next.binding, 3).unwrap();
            assert!(fresh.restore_recovery(capsule).is_err());
            continue;
        }
        if mutation == 1 {
            next.binding.lot_incarnation = b.binding.lot_incarnation;
        } else {
            let mut state = game.sim().state().clone();
            let wonderland_game_runtime::sim_core::owned_admission::OwnedAdmissionState::Initialized {actors,..}=&mut state.owned_admission else {panic!()};
            let control = actors.get_mut(&9).unwrap();
            match mutation {
                2 => control.account_id += 1,
                3 => control.session_ref = [8; 32],
                4 => control.admission_operation = "different-entry-0001".into(),
                5 => control.export_digest = [9; 32],
                6 => control.reservation_digest = [10; 32],
                7 => {
                    control.control_generation += 1;
                    next.binding.source_epoch += 1;
                }
                _ => unreachable!(),
            }
            let bytes =
                wonderland_game_runtime::sim_core::snapshot::encode(&state, game.sim().content())
                    .unwrap();
            game.restore(&bytes).unwrap();
        }
        let mut fresh = player(&game, &next);
        assert!(
            fresh.restore_recovery(capsule).is_err(),
            "mutation {mutation}"
        );
        assert!(fresh.pending_action_result().unwrap().is_none());
    }
}
#[test]
fn mismatched_or_future_receipt_cannot_settle_recovered_unknown() {
    let (mut game, b) = setup();
    let mut old = player(&game, &b);
    let request = prepare(&mut old);
    old.disconnect();
    let capsule = old.recovery_capsule().unwrap();
    let next = newer(&mut game, &b);
    let mut fresh = player(&game, &next);
    fresh.restore_recovery(capsule).unwrap();
    for tick in [
        game.sim().state().completed_tick,
        game.sim().state().completed_tick + 1,
    ] {
        assert!(
            fresh
                .receive(
                    &encode_receipt(&ActionReceipt {
                        request: request.clone(),
                        accepted_tick: Some(tick)
                    })
                    .unwrap()
                )
                .is_err()
        );
        assert_eq!(fresh.status(), ActionStatus::Unknown);
    }
    let mut different = decode_action(&request).unwrap();
    let PlayerAction::Invoke(ref mut intent) = different else {
        panic!()
    };
    intent.command_sequence += 1;
    assert!(
        fresh
            .receive(
                &encode_receipt(&ActionReceipt {
                    request: wonderland_game_runtime::live_wire::player::encode_action(&different)
                        .unwrap(),
                    accepted_tick: None
                })
                .unwrap()
            )
            .is_err()
    );
    assert_eq!(fresh.status(), ActionStatus::Unknown);
    fresh
        .receive(
            &encode_receipt(&ActionReceipt {
                request,
                accepted_tick: None,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(fresh.status(), ActionStatus::Rejected);
}

#[test]
fn interrupted_bootstrap_keeps_original_capsule_until_a_later_checkpoint() {
    let (mut game, b) = setup();
    let mut old = player(&game, &b);
    let request = prepare(&mut old);
    old.disconnect();
    let capsule = old.recovery_capsule().unwrap();
    let next = newer(&mut game, &b);
    let mut intermediate =
        NativePlayer::open(&encode_bootstrap(&next).unwrap(), next.binding, 3).unwrap();
    assert!(intermediate.recovery_capsule().is_err());
    intermediate.close();
    let next = newer(&mut game, &next);
    let mut fresh = player(&game, &next);
    fresh.restore_recovery(capsule).unwrap();
    let query = fresh.pending_action_result().unwrap().unwrap();
    assert_eq!(query.lot_incarnation, 4);
    assert_eq!(query.request, request);
}
#[test]
fn capsule_rejects_different_source_location_and_principal_even_with_a_live_actor() {
    for principal_change in [false, true] {
        let (mut game, b) = setup();
        let mut old = player(&game, &b);
        prepare(&mut old);
        old.disconnect();
        let capsule = old.recovery_capsule().unwrap();
        let mut next = newer(&mut game, &b);
        if principal_change {
            let mut state = game.sim().state().clone();
            let wonderland_game_runtime::sim_core::owned_admission::OwnedAdmissionState::Initialized {actors,last_principal,..}=&mut state.owned_admission else {panic!()};
            let mut control = actors.remove(&9).unwrap();
            control.principal = 10;
            actors.insert(10, control);
            *last_principal = 10;
            state
                .interaction_access
                .get_mut(&next.actor)
                .unwrap()
                .principal = PrincipalKey(10);
            next.principal = PrincipalKey(10);
            let bytes =
                wonderland_game_runtime::sim_core::snapshot::encode(&state, game.sim().content())
                    .unwrap();
            game.restore(&bytes).unwrap();
        } else {
            next.binding.lot_location += 1;
        }
        let mut fresh = player(&game, &next);
        assert!(fresh.restore_recovery(capsule).is_err());
    }
}

#[test]
fn terrain_reply_loss_keeps_exact_bytes_and_sequence_across_replacement_authority() {
    for accepted in [false, true] {
        let (mut game, b) = setup();
        let mut old = player(&game, &b);
        let position = wonderland_game_runtime::TilePos::new(4, 4, 1).center();
        let request = old.prepare_goto(position, 7, 0).unwrap();
        let accepted_tick = if accepted {
            let PlayerAction::Goto(intent) = decode_action(&request).unwrap() else {
                panic!()
            };
            Some(
                game.advance(vec![AcceptedCommand::SourceGoto(intent)])
                    .unwrap()
                    .tick,
            )
        } else {
            None
        };
        old.disconnect();
        let capsule = old.recovery_capsule().unwrap();
        let next = newer(&mut game, &b);
        let mut fresh = player(&game, &next);
        let hash = fresh.state_hash().unwrap();
        fresh.restore_recovery(capsule).unwrap();
        assert_eq!(fresh.state_hash().unwrap(), hash);
        assert_eq!(fresh.status(), ActionStatus::Unknown);
        let query = fresh.pending_action_result().unwrap().unwrap();
        assert_eq!(query.lot_incarnation, b.binding.lot_incarnation);
        assert_eq!(query.request, request);
        assert!(fresh.prepare_goto(position, 7, 0).is_err());
        fresh
            .receive(
                &encode_receipt(&ActionReceipt {
                    request,
                    accepted_tick,
                })
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            fresh.status(),
            if accepted {
                ActionStatus::Accepted
            } else {
                ActionStatus::Rejected
            }
        );
        assert!(fresh.pending_action_result().unwrap().is_none());
        assert_eq!(
            decode_action(&fresh.prepare_goto(position, 7, 0).unwrap())
                .unwrap()
                .sequence(),
            2
        );
    }
}

#[test]
fn placement_suspension_retains_unknown_action_for_same_and_new_host_incarnation() {
    for change_incarnation in [false, true] {
        let (mut game, bootstrap) = setup();
        let mut suspended = player(&game, &bootstrap);
        let request = prepare(&mut suspended);
        let PlayerAction::Invoke(intent) = decode_action(&request).unwrap() else {
            panic!()
        };
        let tick = game
            .advance(vec![AcceptedCommand::QueueInteraction(intent)])
            .unwrap()
            .tick;
        // A placement-only UI pause disconnects transport but must retain this
        // exact owned NativePlayer. It is neither an outcome nor identity loss.
        suspended.disconnect();
        suspended.disconnect();
        assert_eq!(suspended.status(), ActionStatus::Unknown);
        let mut recovered = if change_incarnation {
            let capsule = suspended.recovery_capsule().unwrap();
            let next = newer(&mut game, &bootstrap);
            let mut fresh = player(&game, &next);
            let before = fresh.state_hash().unwrap();
            fresh.restore_recovery(capsule).unwrap();
            assert_eq!(fresh.state_hash().unwrap(), before);
            fresh
        } else {
            // The mounted controller keeps the same player for an unchanged
            // binding. Capsules intentionally require a newer incarnation.
            suspended.reconnect().unwrap();
            install(&mut suspended, &game);
            suspended
        };
        assert_eq!(
            recovered.state_hash().unwrap(),
            game.sim().state_hash().unwrap()
        );
        assert_eq!(recovered.status(), ActionStatus::Unknown);
        let query = recovered.pending_action_result().unwrap().unwrap();
        assert_eq!(query.request, request);
        assert_eq!(query.lot_incarnation, bootstrap.binding.lot_incarnation);
        let offers = recovered.offers(recovered.actor()).unwrap();
        assert!(
            recovered
                .prepare(
                    recovered.actor(),
                    offers.offers[0].interaction,
                    offers.offers[0].param0
                )
                .is_err()
        );
        recovered
            .receive(
                &encode_receipt(&ActionReceipt {
                    request,
                    accepted_tick: Some(tick),
                })
                .unwrap(),
            )
            .unwrap();
        assert_eq!(recovered.status(), ActionStatus::Accepted);
        assert_eq!(
            decode_action(&prepare(&mut recovered)).unwrap().sequence(),
            2
        );
    }
}
