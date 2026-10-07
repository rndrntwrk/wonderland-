#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;
use wonderland_game_runtime::{AcceptedCommand, GameRuntime, PrincipalKey, RuntimeRole};
use wonderland_game_runtime::live_wire::player::{PlayerAction, encode_action};
use wonderland_game_runtime::live_wire::player::admission::revalidate_action;

fn prepared() -> (GameRuntime, wonderland_game_runtime::EntityRef, Vec<u8>) {
    let (server, replica, actor) = support::server_and_client("Casino_2-Tile_Bar_CC.iff", 4110, false);
    let selection = support::source_selection(&replica, actor, 0);
    let intent = replica.prepare_interaction(replica.connection(), selection).unwrap();
    (server, actor, encode_action(&PlayerAction::Invoke(intent)).unwrap())
}

#[test]
fn server_rechecks_the_source_action_after_ordinary_tick_latency_without_mutating_during_query() {
    let (mut server, actor, bytes) = prepared();
    for _ in 0..3 { server.advance(vec![]).unwrap(); }
    let before = server.snapshot().unwrap();
    let action = revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap();
    assert_eq!(server.snapshot().unwrap(), before);
    let PlayerAction::Invoke(intent) = action else { panic!("Expected the exact invocation") };
    assert_eq!(intent.seen.world_revision, server.sim().state().completed_tick);
    assert_eq!(intent.command_sequence, 1);
    assert_eq!(intent.interaction.tta_index, 7);
    server.advance(vec![AcceptedCommand::QueueInteraction(intent)]).unwrap();
    assert_eq!(server.sim().state().entities[&actor.object_id].attributes, [0, 0, 30, 0]);
}

#[test]
fn server_does_not_accept_a_client_asserted_principal_or_actor() {
    let (server, actor, bytes) = prepared();
    assert!(revalidate_action(&server, PrincipalKey(999), actor, &bytes).is_err());
    let mut other = actor; other.generation += 1;
    assert!(revalidate_action(&server, support::PRINCIPAL, other, &bytes).is_err());
}

#[test]
fn revoked_source_authority_and_changed_queue_are_not_refreshed_away() {
    let (mut server, actor, bytes) = prepared();
    server.advance(vec![AcceptedCommand::SetInteractionAuthority {actor, access: None}]).unwrap();
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
    let (mut server, actor, bytes) = prepared();
    let PlayerAction::Invoke(intent) = revalidate_action(&server, support::PRINCIPAL, actor, &bytes).unwrap() else {unreachable!()};
    server.advance(vec![AcceptedCommand::QueueInteraction(intent)]).unwrap();
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}

#[test]
fn future_and_expired_selections_are_rejected_not_retimed() {
    let (mut server, actor, bytes) = prepared();
    let mut action = wonderland_game_runtime::live_wire::player::decode_action(&bytes).unwrap();
    let PlayerAction::Invoke(ref mut intent) = action else {unreachable!()};
    intent.seen.world_revision += 100;
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &encode_action(&action).unwrap()).is_err());
    for _ in 0..129 {server.advance(vec![]).unwrap();}
    assert!(revalidate_action(&server, support::PRINCIPAL, actor, &bytes).is_err());
}

#[test]
fn server_revalidation_is_not_available_on_a_replica() {
    let (server, actor, bytes) = prepared();
    let mut config = wonderland_game_runtime::RuntimeConfig::new(wonderland_game_runtime::VmMode::Ts1, 11, 7, 123);
    config.limits = server.sim().state().limits.clone();
    let mut replica = GameRuntime::new(server.sim().content().clone(), server.sim().state().world.lot.clone(), config, RuntimeRole::Replica).unwrap();
    replica.restore(&server.snapshot().unwrap()).unwrap();
    assert!(revalidate_action(&replica, support::PRINCIPAL, actor, &bytes).is_err());
}
