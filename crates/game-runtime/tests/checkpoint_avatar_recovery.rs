//! Recovery must rebuild supplied visual history without publishing old outcomes.
#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;

use wonderland_game_runtime::live_session::{Checkpoint, SessionStatus, TickFrame};
use wonderland_game_runtime::live_wire::player::{
    ActionStatus, Bootstrap, NativePlayer, PlayerAction, PlayerBinding, PlayerUpdate,
    decode_action, encode_bootstrap,
};
use wonderland_game_runtime::live_wire::{
    NativeWire, Received, WireError, WireLimits, encode_checkpoint, encode_ticks,
};
use wonderland_game_runtime::sim_core::vm::{EntityField, MemoryAddress};
use wonderland_game_runtime::world_view::WorldDocument;
use wonderland_game_runtime::{AcceptedCommand, EntityRef, GameRuntime};

struct Saved {
    bytes: Vec<u8>,
    tick: u64,
    hash: [u8; 32],
}
impl Saved {
    fn new(server: &GameRuntime) -> Self {
        Self {
            bytes: server.snapshot().unwrap(),
            tick: server.sim().state().completed_tick,
            hash: server.sim().state_hash().unwrap(),
        }
    }
    fn packet(&self, request: u64, tail: &[TickFrame]) -> Vec<u8> {
        encode_checkpoint(
            request,
            Checkpoint {
                bytes: &self.bytes,
                completed_tick: self.tick,
                state_hash: self.hash,
            },
            tail,
            WireLimits::default(),
        )
        .unwrap()
    }
}
fn setup() -> (GameRuntime, NativeWire, EntityRef) {
    let (server, replica, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let wire = NativeWire::new(replica, WireLimits::default()).unwrap();
    (server, wire, actor)
}
fn step(server: &mut GameRuntime, commands: Vec<AcceptedCommand>) -> TickFrame {
    let accepted = server.sim().next_tick(commands).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    }
}
fn scale(server: &mut GameRuntime, actor: EntityRef, value: i16) -> TickFrame {
    step(
        server,
        vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor,
                field: EntityField::PersonData,
                index: 63,
            },
            value,
        }],
    )
}
fn deliver(wire: &mut NativeWire, saved: &Saved, tail: &[TickFrame]) -> Vec<wonderland_game_runtime::AvatarVisualFrame> {
    let packet = saved.packet(wire.checkpoint_request().unwrap().id, tail);
    let (update, frames) = wire
        .receive_with_avatar_frames(wire.connection(), &packet)
        .unwrap();
    assert!(matches!(update, Received::Checkpoint(_)));
    frames.expect("small recovery trace must fit its presentation budget")
}

#[test]
fn checkpoint_tail_captures_seed_and_every_validated_intermediate_frame() {
    let (mut server, mut wire, actor) = setup();
    let saved = Saved::new(&server);
    let mut expected = vec![server.avatar_visual_frame()];
    let mut tail = Vec::new();
    for value in [25, 50, 75] {
        tail.push(scale(&mut server, actor, value));
        expected.push(server.avatar_visual_frame());
    }
    assert_eq!(deliver(&mut wire, &saved, &tail), expected);
    assert_eq!(wire.replica().runtime().unwrap().snapshot().unwrap(), server.snapshot().unwrap());
}

#[test]
fn empty_checkpoint_tail_supplies_only_its_actual_seed() {
    let (server, mut wire, _) = setup();
    assert_eq!(deliver(&mut wire, &Saved::new(&server), &[]), vec![server.avatar_visual_frame()]);
}

#[test]
fn checkpoint_tail_and_live_packet_groups_publish_identical_visual_inputs() {
    let (mut server, mut checkpoint_wire, actor) = setup();
    let (_, mut live_wire, _) = setup();
    let saved = Saved::new(&server);
    let mut live_frames = deliver(&mut live_wire, &saved, &[]);
    let mut tail = Vec::new();
    for value in 1..=18 {
        tail.push(scale(&mut server, actor, value));
    }
    for chunk in tail.chunks(3) {
        let packet = encode_ticks(chunk, WireLimits::default()).unwrap();
        let (update, frames) = live_wire.receive_with_avatar_frames(live_wire.connection(), &packet).unwrap();
        assert!(matches!(update, Received::Ticks(_)));
        live_frames.extend(frames.unwrap());
    }
    assert_eq!(deliver(&mut checkpoint_wire, &saved, &tail), live_frames);
    assert_eq!(checkpoint_wire.replica().cursor(), live_wire.replica().cursor());
}

#[test]
fn hidden_avatars_remain_in_the_recovery_trace() {
    let (mut server, mut wire, actor) = setup();
    let saved = Saved::new(&server);
    let mut expected = vec![server.avatar_visual_frame()];
    let mut tail = Vec::new();
    for value in [2, 0] {
        tail.push(step(&mut server, vec![AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity { entity: actor, field: EntityField::ObjectData, index: 34 },
            value,
        }]));
        expected.push(server.avatar_visual_frame());
    }
    let frames = deliver(&mut wire, &saved, &tail);
    assert_eq!(frames, expected);
    assert!(frames.iter().all(|frame| frame.avatars.len() == 1));
}

#[test]
fn a_bad_final_tail_hash_publishes_no_partial_state_or_visuals() {
    let (mut server, mut wire, actor) = setup();
    let saved = Saved::new(&server);
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    let cursor = wire.replica().cursor();
    let first = scale(&mut server, actor, 25);
    let mut second = scale(&mut server, actor, 50);
    second.state_hash[0] ^= 1;
    let packet = saved.packet(wire.checkpoint_request().unwrap().id, &[first, second]);
    assert!(wire.receive_with_avatar_frames(wire.connection(), &packet).is_err());
    assert_eq!(wire.replica().runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(wire.replica().cursor(), cursor);
    assert_eq!(wire.replica().status(), SessionStatus::Suspended);
}

#[test]
fn a_bad_checkpoint_hash_cannot_publish_its_seed() {
    let (server, mut wire, _) = setup();
    let mut saved = Saved::new(&server);
    saved.hash[0] ^= 1;
    let before = wire.replica().runtime().unwrap().snapshot().unwrap();
    let packet = saved.packet(wire.checkpoint_request().unwrap().id, &[]);
    assert!(wire.receive_with_avatar_frames(wire.connection(), &packet).is_err());
    assert_eq!(wire.replica().runtime().unwrap().snapshot().unwrap(), before);
}

#[test]
fn obsolete_checkpoint_callbacks_cannot_seed_the_replacement_connection() {
    let (mut server, mut wire, actor) = setup();
    let saved = Saved::new(&server);
    let first = scale(&mut server, actor, 25);
    let packet = saved.packet(wire.checkpoint_request().unwrap().id, &[first]);
    let old = wire.connection();
    wire.disconnect(old).unwrap();
    let current = wire.reconnect().unwrap();
    assert!(matches!(wire.receive_with_avatar_frames(old, &packet), Err(WireError::StaleConnection)));
    assert!(matches!(wire.receive_with_avatar_frames(current, &packet), Err(WireError::StaleResponse)));
    assert_eq!(wire.replica().status(), SessionStatus::AwaitingCheckpoint);
}

#[test]
fn recovery_rebuilds_supplied_frames_before_and_after_the_previous_anchor() {
    let (mut server, mut wire, actor) = setup();
    let saved = Saved::new(&server);
    let mut expected = vec![server.avatar_visual_frame()];
    deliver(&mut wire, &saved, &[]);
    let mut tail = Vec::new();
    for value in [25, 50, 75] {
        tail.push(scale(&mut server, actor, value));
        expected.push(server.avatar_visual_frame());
    }
    let packet = encode_ticks(&tail[..2], WireLimits::default()).unwrap();
    wire.receive(wire.connection(), &packet).unwrap();
    wire.disconnect(wire.connection()).unwrap();
    wire.reconnect().unwrap();
    assert_eq!(deliver(&mut wire, &saved, &tail), expected);
    assert_eq!(wire.replica().cursor().unwrap().completed_tick, saved.tick + 3);
}

#[test]
fn an_exact_duplicate_last_tail_tick_cannot_compound_retained_blends() {
    let (mut server, mut wire, actor) = setup();
    let saved = Saved::new(&server);
    let seed = server.avatar_visual_frame();
    let frame = scale(&mut server, actor, 25);
    let expected = vec![seed, server.avatar_visual_frame()];
    assert_eq!(deliver(&mut wire, &saved, &[frame.clone(), frame]), expected);
}

#[test]
fn state_only_checkpoint_delivery_keeps_the_same_authoritative_result() {
    let (mut server, mut visual_wire, actor) = setup();
    let (_, mut plain_wire, _) = setup();
    let saved = Saved::new(&server);
    let tail = [scale(&mut server, actor, 25), scale(&mut server, actor, 50)];
    deliver(&mut visual_wire, &saved, &tail);
    let packet = saved.packet(plain_wire.checkpoint_request().unwrap().id, &tail);
    assert!(matches!(plain_wire.receive(plain_wire.connection(), &packet).unwrap(), Received::Checkpoint(_)));
    assert_eq!(plain_wire.replica().cursor(), visual_wire.replica().cursor());
    assert_eq!(plain_wire.replica().runtime().unwrap().snapshot().unwrap(), server.snapshot().unwrap());
}

#[test]
fn player_recovery_returns_poses_without_replaying_activity_or_resolving_unknown_action() {
    let (mut server, _, actor) = setup();
    let state = server.sim().state();
    let value = Bootstrap {
        binding: PlayerBinding { source_epoch: 3, lot_incarnation: 4, lot_location: 55, avatar_id: 7 },
        principal: support::PRINCIPAL,
        actor,
        content: server.sim().content().clone(),
        appearance: WorldDocument::from_blueprint_xml(
            "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
            "test:checkpoint-visual-tail", "declared-harness",
        ).unwrap(),
        lot: state.world.lot.clone(),
        mode: state.mode,
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        effect_namespace: state.effects.namespace(),
        limits: state.limits.clone(),
        effect_limits: state.effects.limits(),
    };
    let mut player = NativePlayer::open(&encode_bootstrap(&value).unwrap(), value.binding, 2).unwrap();
    let saved = Saved::new(&server);
    player.receive(&saved.packet(player.checkpoint_request().unwrap().id, &[])).unwrap();
    let offer = player.offers(actor).unwrap().offers[0].clone();
    let request = player.prepare(actor, offer.interaction, offer.param0).unwrap();
    let PlayerAction::Invoke(intent) = decode_action(&request).unwrap() else { panic!("invoke expected") };
    let mut expected = vec![server.avatar_visual_frame()];
    let first = step(&mut server, vec![AcceptedCommand::QueueInteraction(intent)]);
    expected.push(server.avatar_visual_frame());
    let second = scale(&mut server, actor, 60);
    expected.push(server.avatar_visual_frame());
    player.disconnect();
    player.reconnect().unwrap();
    let (update, frames) = player.receive_presented(&saved.packet(player.checkpoint_request().unwrap().id, &[first, second])).unwrap();
    assert!(matches!(update, PlayerUpdate::Checkpoint(_)), "recovery must not return historical TickOutcomes");
    assert_eq!(frames.unwrap(), expected);
    assert_eq!(player.status(), ActionStatus::Unknown);
    assert_eq!(player.activity().count(), 0);
    assert_eq!(player.projection().unwrap(), server.projection());
    assert!(player.prepare(actor, offer.interaction, offer.param0).is_err());
    player.dismiss_unknown().unwrap();
    let next_offer = player.offers(actor).unwrap().offers[0].clone();
    let next = player.prepare(actor, next_offer.interaction, next_offer.param0).unwrap();
    assert!(decode_action(&next).unwrap().sequence() > decode_action(&request).unwrap().sequence());
}
