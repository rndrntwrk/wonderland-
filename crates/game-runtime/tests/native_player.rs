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
        PlayerAction::Cancel(_) => unreachable!(),
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
