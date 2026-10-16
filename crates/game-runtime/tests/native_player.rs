// Share the browser fixture's source loader rather than compiling it twice.
use browser_peer::support;
use wonderland_game_runtime::live_session::{Checkpoint, TickFrame};
use wonderland_game_runtime::live_wire::player::{
    ActionReceipt, ActionStatus, Bootstrap, NativePlayer, PlayerAction, PlayerBinding,
    decode_action, encode_bootstrap, encode_receipt,
};
use wonderland_game_runtime::live_wire::{WireLimits, encode_checkpoint, encode_ticks};
use wonderland_game_runtime::world_view::WorldDocument;
use wonderland_game_runtime::{AcceptedCommand, GameRuntime};

fn setup() -> (GameRuntime, Bootstrap) {
    let (server, _, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let state = server.sim().state();
    let value = Bootstrap {
        binding: PlayerBinding {
            source_epoch: 3,
            lot_incarnation: 4,
            lot_location: 55,
            avatar_id: 7,
        },
        principal: support::PRINCIPAL,
        actor,
        content: server.sim().content().clone(),
        appearance: WorldDocument::from_blueprint_xml(
            "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
            "test:empty-world",
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
    (server, value)
}
fn install(player: &mut NativePlayer, server: &GameRuntime) {
    let bytes = server.snapshot().unwrap();
    let request = player.checkpoint_request().unwrap();
    let packet = encode_checkpoint(
        request.id,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &bytes,
        },
        &[],
        WireLimits::default(),
    )
    .unwrap();
    player.receive(&packet).unwrap();
}
fn player(server: &GameRuntime, value: &Bootstrap) -> NativePlayer {
    let mut player =
        NativePlayer::open(&encode_bootstrap(value).unwrap(), value.binding, 2).unwrap();
    install(&mut player, server);
    player
}
fn prepare(player: &mut NativePlayer) -> Vec<u8> {
    let actor = player.actor();
    let offers = player.offers(actor).unwrap();
    player
        .prepare(actor, offers.offers[0].interaction, offers.offers[0].param0)
        .unwrap()
}
fn accept(server: &mut GameRuntime, player: &mut NativePlayer, request: &[u8]) -> u64 {
    let command = match decode_action(request).unwrap() {
        PlayerAction::Invoke(intent) => AcceptedCommand::QueueInteraction(intent),
        PlayerAction::Cancel(intent) => AcceptedCommand::CancelInteraction(intent),
        PlayerAction::Goto(intent) => AcceptedCommand::SourceGoto(intent),
    };
    let accepted = server.sim().next_tick(vec![command]).unwrap();
    let result = server.apply_accepted(&accepted).unwrap();
    player
        .receive(
            &encode_ticks(
                &[TickFrame {
                    accepted,
                    state_hash: result.state_hash,
                }],
                WireLimits::default(),
            )
            .unwrap(),
        )
        .unwrap();
    result.tick
}
#[test]
fn bootstrap_never_makes_the_scene_live_until_accepted_checkpoint() {
    let (server, value) = setup();
    let bytes = encode_bootstrap(&value).unwrap();
    let mut player = NativePlayer::open(&bytes, value.binding, 2).unwrap();
    assert!(player.projection().is_err());
    install(&mut player, &server);
    assert_eq!(player.projection().unwrap(), server.projection());
    assert_eq!(
        player.world().unwrap().revision.tick,
        server.sim().state().completed_tick
    );
}
#[test]
fn mismatched_avatar_lot_and_source_incarnations_are_rejected() {
    let (_, value) = setup();
    let bytes = encode_bootstrap(&value).unwrap();
    for binding in [
        PlayerBinding {
            avatar_id: 8,
            ..value.binding
        },
        PlayerBinding {
            lot_location: 56,
            ..value.binding
        },
        PlayerBinding {
            source_epoch: 99,
            ..value.binding
        },
        PlayerBinding {
            lot_incarnation: 99,
            ..value.binding
        },
    ] {
        assert!(NativePlayer::open(&bytes, binding, 2).is_err());
    }
}
#[test]
fn corrupted_and_trailing_bootstrap_bytes_cannot_open_a_player() {
    let (_, value) = setup();
    let bytes = encode_bootstrap(&value).unwrap();
    for mut bad in [
        bytes[..bytes.len() - 1].to_vec(),
        [bytes.as_slice(), &[0]].concat(),
        bytes.clone(),
    ] {
        if bad.len() == bytes.len() {
            bad[0] ^= 1;
        }
        assert!(NativePlayer::open(&bad, value.binding, 2).is_err());
    }
}
#[test]
fn a_checkpoint_without_the_admitted_actor_cannot_enable_actions() {
    let (server, mut value) = setup();
    value.actor.generation += 1;
    let mut player =
        NativePlayer::open(&encode_bootstrap(&value).unwrap(), value.binding, 2).unwrap();
    let bytes = server.snapshot().unwrap();
    let packet = encode_checkpoint(
        1,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &bytes,
        },
        &[],
        WireLimits::default(),
    )
    .unwrap();
    assert!(player.receive(&packet).is_err());
    assert!(player.projection().is_err());
}
#[test]
fn prepared_action_preserves_live_state_and_blocks_a_second_pending_command() {
    let (server, value) = setup();
    let mut player = player(&server, &value);
    let before = player.projection().unwrap();
    let request = prepare(&mut player);
    assert_eq!(decode_action(&request).unwrap().sequence(), 1);
    assert_eq!(player.projection().unwrap(), before);
    assert_eq!(player.status(), ActionStatus::Pending);
    let offers = player.offers(value.actor).unwrap();
    assert!(
        player
            .prepare(
                value.actor,
                offers.offers[0].interaction,
                offers.offers[0].param0
            )
            .is_err()
    );
}
#[test]
fn source_action_is_only_accepted_after_matching_state_and_receipt() {
    let (mut server, value) = setup();
    let mut player = player(&server, &value);
    let request = prepare(&mut player);
    let future = server.sim().state().completed_tick + 1;
    let receipt = encode_receipt(&ActionReceipt {
        request: request.clone(),
        accepted_tick: Some(future),
    })
    .unwrap();
    assert!(player.receive(&receipt).is_err());
    assert_eq!(player.status(), ActionStatus::Pending);
    assert_eq!(accept(&mut server, &mut player, &request), future);
    assert_eq!(player.status(), ActionStatus::Pending);
    player.receive(&receipt).unwrap();
    assert_eq!(player.status(), ActionStatus::Accepted);
    assert_eq!(player.projection().unwrap(), server.projection());
    assert_eq!(decode_action(&prepare(&mut player)).unwrap().sequence(), 2);
}
#[test]
fn an_old_tick_cannot_falsely_acknowledge_a_new_pending_action() {
    let (server, value) = setup();
    let mut player = player(&server, &value);
    let request = prepare(&mut player);
    let receipt = encode_receipt(&ActionReceipt {
        request,
        accepted_tick: Some(server.sim().state().completed_tick),
    })
    .unwrap();
    assert!(player.receive(&receipt).is_err());
    assert_eq!(player.status(), ActionStatus::Pending);
}
#[test]
fn uncorrelated_receipt_cannot_clear_pending_or_advance_its_sequence() {
    let (server, value) = setup();
    let mut a = player(&server, &value);
    let mut b = player(&server, &value);
    let request = prepare(&mut a);
    let mut other = decode_action(&prepare(&mut b)).unwrap();
    match &mut other {
        PlayerAction::Invoke(intent) => intent.command_sequence = 9,
        PlayerAction::Cancel(_) | PlayerAction::Goto(_) => unreachable!(),
    };
    let bad = encode_receipt(&ActionReceipt {
        request: wonderland_game_runtime::live_wire::player::encode_action(&other).unwrap(),
        accepted_tick: None,
    })
    .unwrap();
    assert!(a.receive(&bad).is_err());
    assert_eq!(a.status(), ActionStatus::Pending);
    a.receive(
        &encode_receipt(&ActionReceipt {
            request,
            accepted_tick: None,
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(a.status(), ActionStatus::Rejected);
    assert_eq!(decode_action(&prepare(&mut a)).unwrap().sequence(), 2);
}
#[test]
fn reconnect_retains_unknown_operation_and_never_replays_it() {
    let (server, value) = setup();
    let mut player = player(&server, &value);
    let request = prepare(&mut player);
    player.disconnect();
    assert_eq!(player.status(), ActionStatus::Unknown);
    assert!(player.projection().is_err());
    player.reconnect().unwrap();
    assert!(player.projection().is_err());
    install(&mut player, &server);
    assert_eq!(player.status(), ActionStatus::Unknown);
    let offers = player.offers(value.actor).unwrap();
    assert!(
        player
            .prepare(
                value.actor,
                offers.offers[0].interaction,
                offers.offers[0].param0
            )
            .is_err()
    );
    // Explicit abandonment permits a NEW sequence, never transmission of request.
    player.dismiss_unknown().unwrap();
    let next = prepare(&mut player);
    assert_ne!(next, request);
    assert_eq!(decode_action(&next).unwrap().sequence(), 2);
}
#[test]
fn pending_result_can_be_reconciled_after_a_fresh_checkpoint() {
    let (mut server, value) = setup();
    let mut player = player(&server, &value);
    let request = prepare(&mut player);
    let tick = accept(&mut server, &mut player, &request);
    player.disconnect();
    player.reconnect().unwrap();
    install(&mut player, &server);
    player
        .receive(
            &encode_receipt(&ActionReceipt {
                request,
                accepted_tick: Some(tick),
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(player.status(), ActionStatus::Accepted);
}
#[test]
fn closed_native_player_cannot_expose_state_or_reconnect() {
    let (server, value) = setup();
    let mut player = player(&server, &value);
    player.close();
    assert!(player.projection().is_err());
    assert!(player.world().is_err());
    assert!(player.offers(value.actor).is_err());
    assert!(player.reconnect().is_err());
}

#[test]
fn accepted_updates_preserve_the_entire_ordered_runtime_event_batch() {
    use wonderland_game_runtime::live_wire::player::PlayerUpdate;
    let (mut server, value) = setup();
    let mut player = player(&server, &value);
    let request = prepare(&mut player);
    let PlayerAction::Invoke(intent) = decode_action(&request).unwrap() else {
        unreachable!()
    };
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    assert!(
        !outcome.events.is_empty(),
        "The source behavior must actually emit events"
    );
    let bytes = encode_ticks(
        &[TickFrame {
            accepted,
            state_hash: outcome.state_hash,
        }],
        WireLimits::default(),
    )
    .unwrap();
    let PlayerUpdate::Ticks(outcomes) = player.receive_update(&bytes).unwrap() else {
        panic!("Expected live outcomes")
    };
    assert_eq!(outcomes, vec![outcome]);
    assert_eq!(player.activity().count(), 1);
    assert_eq!(
        player.activity().next().unwrap().result,
        wonderland_game_runtime::live_wire::player::activity::ActionResult::Completed
    );
    assert_eq!(
        player.status(),
        ActionStatus::Pending,
        "Completion and transport receipt are distinct facts"
    );
    let PlayerUpdate::Ticks(duplicate) = player.receive_update(&bytes).unwrap() else {
        panic!("Expected duplicate suppression")
    };
    assert!(
        duplicate.is_empty(),
        "Repeated latest ticks cannot replay presentation"
    );
    assert_eq!(player.activity().count(), 1);
    let history = player.activity().cloned().collect::<Vec<_>>();
    player.disconnect();
    player.reconnect().unwrap();
    install(&mut player, &server);
    assert_eq!(player.activity().cloned().collect::<Vec<_>>(), history);
    player.close();
    assert_eq!(player.activity().count(), 0);
}

#[test]
fn checkpoint_recovery_is_distinct_from_live_events_or_action_acceptance() {
    use wonderland_game_runtime::live_wire::player::PlayerUpdate;
    let (server, value) = setup();
    let mut player =
        NativePlayer::open(&encode_bootstrap(&value).unwrap(), value.binding, 2).unwrap();
    let bytes = server.snapshot().unwrap();
    let packet = encode_checkpoint(
        player.checkpoint_request().unwrap().id,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &bytes,
        },
        &[],
        WireLimits::default(),
    )
    .unwrap();
    let PlayerUpdate::Checkpoint(cursor) = player.receive_update(&packet).unwrap() else {
        panic!("Recovery must not be relabelled as live events")
    };
    assert_eq!(cursor.completed_tick, server.sim().state().completed_tick);
    assert_eq!(player.status(), ActionStatus::Idle);
}

#[test]
fn a_bad_later_tick_exposes_no_partial_events_or_activity() {
    let (mut server, value) = setup();
    let mut player = player(&server, &value);
    let before = player.projection().unwrap();
    let request = prepare(&mut player);
    let PlayerAction::Invoke(intent) = decode_action(&request).unwrap() else {
        unreachable!()
    };
    let accepted = server
        .sim()
        .next_tick(vec![AcceptedCommand::QueueInteraction(intent)])
        .unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    let next = server.sim().next_tick(vec![]).unwrap();
    let mut wrong_hash = server.apply_accepted(&next).unwrap().state_hash;
    wrong_hash[0] ^= 1;
    let packet = encode_ticks(
        &[
            TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            },
            TickFrame {
                accepted: next,
                state_hash: wrong_hash,
            },
        ],
        WireLimits::default(),
    )
    .unwrap();
    assert!(player.receive_update(&packet).is_err());
    assert!(player.projection().is_err());
    assert_eq!(
        player
            .checkpoint_request()
            .unwrap()
            .cursor
            .unwrap()
            .completed_tick,
        before.tick
    );
    assert_eq!(player.activity().count(), 0);
}

// Reuse the actual opted-in browser authority fixture, not a second reset model.
#[allow(dead_code)]
#[path = "../examples/native_browser_peer.rs"]
mod browser_peer;

#[test]
fn browser_fixture_observes_both_unmodified_source_motive_branches() {
    use std::collections::BTreeSet;
    let (mut server, actor) = browser_peer::source().unwrap();
    let mut value = setup().1;
    value.actor = actor;
    value.binding.avatar_id = 42;
    value.content = server.sim().content().clone();
    let values = |p: &NativePlayer| {
        let n = p.projection().unwrap().entities[0].needs.clone().unwrap();
        [n.energy, n.hunger, n.hygiene, n.bladder, n.social, n.fun]
    };
    let expected = BTreeSet::from([[100, 100, 100, 50, 50, 50], [50, 50, 50, 100, 100, 100]]);
    let mut observed = BTreeSet::new();
    for _ in 0..32 {
        browser_peer::reset_needs(&mut server, actor).unwrap();
        let mut p = player(&server, &value);
        assert_eq!(
            values(&p),
            [50; 6],
            "Both original source paths need an observable baseline"
        );
        let request = prepare(&mut p);
        accept(&mut server, &mut p, &request);
        let result = values(&p);
        assert!(
            expected.contains(&result),
            "Original motive branch result: {result:?}"
        );
        assert_eq!(
            p.activity().last().unwrap().result,
            wonderland_game_runtime::live_wire::player::activity::ActionResult::Completed
        );
        observed.insert(result);
    }
    assert_eq!(
        observed, expected,
        "Exercise both branches, without replacing the original random instruction"
    );
}

#[test]
fn native_avatar_visual_frame_is_read_only_and_bound_to_live_admission() {
    let (server, value) = setup();
    let mut client =
        NativePlayer::open(&encode_bootstrap(&value).unwrap(), value.binding, 2).unwrap();
    assert!(client.avatar_visual_frame().is_err());
    install(&mut client, &server);
    let before = client.world().unwrap();
    let visual = client.avatar_visual_frame().unwrap();
    assert_eq!(visual.revision, before.revision);
    assert_eq!(visual.avatars.len(), 1);
    assert_eq!(visual.avatars[0].entity, client.actor());
    assert_eq!(client.world().unwrap(), before);
    client.disconnect();
    assert!(client.avatar_visual_frame().is_err());
}

#[test]
fn presented_wire_batch_contains_every_step_but_checkpoint_contains_only_its_final_state() {
    use wonderland_game_runtime::live_wire::player::PlayerUpdate;
    use wonderland_game_runtime::sim_core::vm::{EntityField, MemoryAddress};
    let (mut server, value) = setup();
    let mut p = player(&server, &value);
    let mut frames = Vec::new();
    let mut expected = Vec::new();
    for scale in [25, 50, 75] {
        let accepted = server
            .sim()
            .next_tick(vec![AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: value.actor,
                    field: EntityField::PersonData,
                    index: 63,
                },
                value: scale,
            }])
            .unwrap();
        let outcome = server.apply_accepted(&accepted).unwrap();
        frames.push(TickFrame {
            accepted,
            state_hash: outcome.state_hash,
        });
        expected.push(server.avatar_visual_frame());
    }
    let (update, actual) = p
        .receive_presented(&encode_ticks(&frames, WireLimits::default()).unwrap())
        .unwrap();
    assert!(matches!(update,PlayerUpdate::Ticks(ref outcomes) if outcomes.len()==3));
    assert_eq!(actual.unwrap(), expected);
    let (_, duplicate) = p
        .receive_presented(&encode_ticks(&frames[2..], WireLimits::default()).unwrap())
        .unwrap();
    assert!(duplicate.unwrap().is_empty());
    p.disconnect();
    p.reconnect().unwrap();
    let bytes = server.snapshot().unwrap();
    let checkpoint = encode_checkpoint(
        p.checkpoint_request().unwrap().id,
        Checkpoint {
            completed_tick: server.sim().state().completed_tick,
            state_hash: server.sim().state_hash().unwrap(),
            bytes: &bytes,
        },
        &[],
        WireLimits::default(),
    )
    .unwrap();
    let (update, recovered) = p.receive_presented(&checkpoint).unwrap();
    assert!(matches!(update, PlayerUpdate::Checkpoint(_)));
    assert_eq!(recovered.unwrap(), vec![server.avatar_visual_frame()]);
}
#[test]
fn a_receipt_cannot_advance_or_reseed_avatar_pose_history() {
    use wonderland_game_runtime::live_wire::player::PlayerUpdate;
    let (mut server, value) = setup();
    let mut p = player(&server, &value);
    let request = prepare(&mut p);
    let tick = accept(&mut server, &mut p, &request);
    let before = p.avatar_visual_frame().unwrap();
    let (update, frames) = p
        .receive_presented(
            &encode_receipt(&ActionReceipt {
                request,
                accepted_tick: Some(tick),
            })
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        update,
        PlayerUpdate::Receipt(ActionStatus::Accepted)
    ));
    assert!(frames.is_none());
    assert_eq!(p.avatar_visual_frame().unwrap(), before);
}

// Keep WLB1 frozen when a presentation-only JSON field is added to WorldDocument.
#[test]
fn native_bootstrap_without_lighting_keeps_published_v1_bytes_and_decoding() {
    use sha2::{Digest, Sha256};
    let (_, value) = setup();
    let bytes = encode_bootstrap(&value).unwrap();
    assert_eq!(bytes.len(), 12_796);
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "f72071935f91247083da73dde5a8ea724ca016f2c6a01b96d99ba4c38f957d9c"
    );
    NativePlayer::open(&bytes, value.binding, 2)
        .expect("published WLB1 appearance must remain decodable");
}
#[test]
fn native_bootstrap_cannot_silently_serialize_unversioned_lighting() {
    #[path = "../../world-view/examples/support/lighting.rs"]
    mod fixture;
    let (_, mut value) = setup();
    value.appearance = fixture::lit_world();
    assert!(
        encode_bootstrap(&value).is_err(),
        "new lighting needs a separately versioned transport contract, not shifted WLB1 fields"
    );
}

#[test]
fn terrain_goto_wire_layout_is_append_only_and_matches_edge_offsets() {
    use wonderland_game_runtime::live_wire::player::encode_action;
    // Independent edge-compatible vector. WLC1 payload tag 2 is appended after
    // the original invocation/cancellation ABI; all values use fixed little endian.
    let mut vector = b"WLC1\r\n\x1a\n".to_vec();
    vector.extend_from_slice(&61u64.to_le_bytes());
    vector.extend_from_slice(&2u32.to_le_bytes());
    vector.extend_from_slice(&1u64.to_le_bytes()); // principal: 20
    vector.extend_from_slice(&2u32.to_le_bytes()); // actor slot: 28
    vector.extend_from_slice(&3u32.to_le_bytes()); // generation: 32
    vector.extend_from_slice(&4u64.to_le_bytes()); // revision: 36
    vector.extend_from_slice(&5u64.to_le_bytes()); // world: 44
    vector.extend_from_slice(&6u64.to_le_bytes()); // queue: 52
    vector.extend_from_slice(&7u64.to_le_bytes()); // sequence: 60
    vector.extend_from_slice(&24i16.to_le_bytes()); // x: 68
    vector.extend_from_slice(&40i16.to_le_bytes()); // y: 70
    vector.push(1); // level: 72
    vector.extend_from_slice(&7u16.to_le_bytes()); // interaction: 73
    vector.extend_from_slice(&(-2i16).to_le_bytes()); // Param0: 75
    assert_eq!(vector.len(), 77);
    let action = decode_action(&vector).expect("appended terrain source action tag");
    assert_eq!(action.sequence(), 7);
    let PlayerAction::Goto(intent) = &action else {
        panic!("Expected terrain tag 2")
    };
    assert_eq!(intent.principal.0, 1);
    assert_eq!(
        (
            intent.actor.key.slot,
            intent.actor.key.generation,
            intent.actor.revision
        ),
        (2, 3, 4)
    );
    assert_eq!((intent.world_revision, intent.queue_revision), (5, 6));
    assert_eq!(
        (
            intent.x,
            intent.y,
            intent.level,
            intent.interaction,
            intent.param0
        ),
        (24, 40, 1, 7, -2)
    );
    assert_eq!(encode_action(&action).unwrap(), vector);
    for length in 0..vector.len() {
        assert!(decode_action(&vector[..length]).is_err());
    }
    let mut zero_sequence = vector.clone();
    zero_sequence[60..68].fill(0);
    assert!(decode_action(&zero_sequence).is_err());
    let mut trailing = vector.clone();
    trailing.push(0);
    assert!(decode_action(&trailing).is_err());
    let mut unknown_tag = vector;
    unknown_tag[16..20].copy_from_slice(&3u32.to_le_bytes());
    assert!(decode_action(&unknown_tag).is_err());
}

#[path = "support/source_terrain.rs"]
mod source_terrain;
fn terrain_setup() -> (GameRuntime, Bootstrap) {
    let (server, actor) = source_terrain::runtime();
    let (_, mut bootstrap) = setup();
    bootstrap.content = server.sim().content().clone();
    bootstrap.actor = actor;
    bootstrap.mode = wonderland_game_runtime::VmMode::Tso;
    (server, bootstrap)
}
fn prepare_terrain(player: &mut NativePlayer) -> Vec<u8> {
    player
        .prepare_goto(
            wonderland_game_runtime::TilePos::new(4, 4, 1).center(),
            7,
            0,
        )
        .unwrap()
}
#[test]
fn terrain_menu_and_prepare_are_read_only_and_preserve_exact_source_selection() {
    let (server, value) = terrain_setup();
    let mut player = player(&server, &value);
    let position = wonderland_game_runtime::TilePos::new(4, 4, 1).center();
    let before = player.state_hash().unwrap();
    let batch = player.terrain_offers(position).unwrap();
    assert_eq!(batch.offers[0].label, "Authored GoTo boundary action");
    assert_eq!(batch.position, position);
    assert!(player.prepare_goto(position, 8, 0).is_err());
    assert!(player.prepare_goto(position, 7, 99).is_err());
    assert_eq!(player.status(), ActionStatus::Idle);
    let request = prepare_terrain(&mut player);
    let PlayerAction::Goto(intent) = decode_action(&request).unwrap() else {
        panic!()
    };
    assert_eq!(
        (
            intent.x,
            intent.y,
            intent.level,
            intent.interaction,
            intent.param0
        ),
        (72, 72, 1, 7, 0)
    );
    assert_eq!(intent.actor, batch.actor);
    assert_eq!(intent.world_revision, batch.world_revision);
    assert_eq!(intent.queue_revision, batch.queue_revision);
    assert_eq!(intent.command_sequence, 1);
    assert_eq!(player.state_hash().unwrap(), before);
    assert!(player.prepare_goto(position, 7, 0).is_err());
}
#[test]
fn terrain_action_requires_accepted_state_and_exact_receipt_without_client_motion() {
    let (mut server, value) = terrain_setup();
    let mut player = player(&server, &value);
    let position = player.projection().unwrap().entities[0].position;
    let request = prepare_terrain(&mut player);
    let future = server.sim().state().completed_tick + 1;
    let receipt = encode_receipt(&ActionReceipt {
        request: request.clone(),
        accepted_tick: Some(future),
    })
    .unwrap();
    assert!(player.receive(&receipt).is_err());
    assert_eq!(accept(&mut server, &mut player, &request), future);
    assert_eq!(player.status(), ActionStatus::Pending);
    player.receive(&receipt).unwrap();
    assert_eq!(player.status(), ActionStatus::Accepted);
    assert_eq!(player.projection().unwrap(), server.projection());
    assert_eq!(player.projection().unwrap().entities[0].position, position);
}
#[test]
fn disconnected_terrain_action_is_unknown_never_replayed_and_can_only_be_rechosen() {
    let (server, value) = terrain_setup();
    let mut player = player(&server, &value);
    let request = prepare_terrain(&mut player);
    player.disconnect();
    assert!(
        player
            .terrain_offers(wonderland_game_runtime::TilePos::new(4, 4, 1).center())
            .is_err()
    );
    player.reconnect().unwrap();
    install(&mut player, &server);
    assert_eq!(player.status(), ActionStatus::Unknown);
    assert!(
        player
            .prepare_goto(
                wonderland_game_runtime::TilePos::new(4, 4, 1).center(),
                7,
                0
            )
            .is_err()
    );
    player.dismiss_unknown().unwrap();
    let next = prepare_terrain(&mut player);
    assert_ne!(next, request);
    assert_eq!(decode_action(&next).unwrap().sequence(), 2);
}
