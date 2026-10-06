#[path = "live_session/support.rs"]
mod support;
use wonderland_game_runtime::live_wire::{encode_checkpoint, WireLimits};
use wonderland_game_runtime::live_wire::player::{Bootstrap, NativePlayer, PlayerBinding, encode_bootstrap};
use wonderland_game_runtime::live_session::Checkpoint;
use wonderland_game_runtime::world_view::WorldDocument;

fn setup() -> (wonderland_game_runtime::GameRuntime, Bootstrap) {
    let (server, _, actor) = support::pair_with_content(support::content("cursebook_set_permission.iff", 4107, false), true);
    let state = server.sim().state();
    let value = Bootstrap {
        binding: PlayerBinding { source_epoch: 3, lot_incarnation: 4, lot_location: 55, avatar_id: 7 },
        principal: support::PRINCIPAL, actor,
        content: server.sim().content().clone(),
        appearance: WorldDocument::from_blueprint_xml("<house><size>8</size><world><floors/><walls/></world><objects/></house>", "test:empty-world", "authored-fixture").unwrap(),
        lot: state.world.lot.clone(), mode: state.mode,
        lot_id: state.lot_id, authority_epoch: state.authority_epoch,
        effect_namespace: state.effects.namespace(), limits: state.limits.clone(), effect_limits: state.effects.limits(),
    };
    (server, value)
}
fn install(player: &mut NativePlayer, server: &wonderland_game_runtime::GameRuntime) {
    let bytes = server.snapshot().unwrap();
    let request = player.checkpoint_request().unwrap();
    let packet = encode_checkpoint(request.id, Checkpoint { completed_tick: server.sim().state().completed_tick,
        state_hash: server.sim().state_hash().unwrap(), bytes: &bytes }, &[], WireLimits::default()).unwrap();
    player.receive(&packet).unwrap();
}
#[test]
fn bootstrap_never_makes_the_scene_live_until_accepted_checkpoint() {
    let (server, value) = setup();
    let bytes = encode_bootstrap(&value).unwrap();
    let mut player = NativePlayer::open(&bytes, value.binding, 2).unwrap();
    assert!(player.projection().is_err());
    install(&mut player, &server);
    assert_eq!(player.projection().unwrap(), server.projection());
    assert_eq!(player.world().unwrap().revision.tick, server.sim().state().completed_tick);
}
#[test]
fn mismatched_avatar_lot_and_source_incarnations_are_rejected() {
    let (_, value) = setup();
    let bytes = encode_bootstrap(&value).unwrap();
    for binding in [PlayerBinding { avatar_id: 8, ..value.binding }, PlayerBinding { lot_location: 56, ..value.binding },
        PlayerBinding { source_epoch: 99, ..value.binding }, PlayerBinding { lot_incarnation: 99, ..value.binding }] {
        assert!(NativePlayer::open(&bytes, binding, 2).is_err());
    }
}
#[test]
fn corrupted_and_trailing_bootstrap_bytes_cannot_open_a_player() {
    let (_, value) = setup(); let bytes = encode_bootstrap(&value).unwrap();
    for mut bad in [bytes[..bytes.len()-1].to_vec(), [bytes.as_slice(), &[0]].concat(), bytes.clone()] {
        if bad.len() == bytes.len() { bad[0] ^= 1; }
        assert!(NativePlayer::open(&bad, value.binding, 2).is_err());
    }
}
#[test]
fn a_checkpoint_without_the_admitted_actor_cannot_enable_actions() {
    let (server, mut value) = setup(); value.actor.generation += 1;
    let mut player = NativePlayer::open(&encode_bootstrap(&value).unwrap(), value.binding, 2).unwrap();
    let bytes = server.snapshot().unwrap();
    let packet = encode_checkpoint(1, Checkpoint {completed_tick: server.sim().state().completed_tick,
        state_hash:server.sim().state_hash().unwrap(), bytes:&bytes}, &[], WireLimits::default()).unwrap();
    assert!(player.receive(&packet).is_err()); assert!(player.projection().is_err());
}
