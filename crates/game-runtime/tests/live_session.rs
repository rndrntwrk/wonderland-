//! These tests exercise the real A/B runtime, not a second simulation model.
use sim_core::state::{ContentSet, TuningSet};
use sim_core::vm::RoutineStore;
use wonderland_game_runtime::live_session::{
    Checkpoint, LiveReplica, ReplayLimits, SessionStatus, StreamIdentity, TickFrame,
};
use wonderland_game_runtime::{GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode};

fn runtime(role: RuntimeRole) -> GameRuntime {
    GameRuntime::new(
        ContentSet::new(RoutineStore::new(), vec![], vec![], TuningSet::default()).unwrap(),
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        role,
    )
    .unwrap()
}

fn identity() -> StreamIdentity {
    StreamIdentity {
        browser_epoch: 2,
        source_epoch: 3,
        lot_incarnation: 4,
    }
}

fn client() -> LiveReplica {
    LiveReplica::new(
        runtime(RuntimeRole::Replica),
        identity(),
        ReplayLimits::default(),
    )
    .unwrap()
}

fn advance(server: &mut GameRuntime) -> TickFrame {
    let accepted = server.sim().next_tick(vec![]).unwrap();
    let outcome = server.apply_accepted(&accepted).unwrap();
    TickFrame {
        accepted,
        state_hash: outcome.state_hash,
    }
}

fn install(client: &mut LiveReplica, server: &GameRuntime, tail: &[TickFrame]) {
    let bytes = server.snapshot().unwrap();
    let checkpoint = Checkpoint {
        completed_tick: server.sim().state().completed_tick,
        state_hash: server.sim().state_hash().unwrap(),
        bytes: &bytes,
    };
    client
        .install_checkpoint(client.checkpoint_ticket().unwrap(), checkpoint, tail)
        .unwrap();
}

#[test]
fn checkpoint_is_required_before_live_projection_or_ticks() {
    let mut client = client();
    let mut server = runtime(RuntimeRole::Authority);
    assert_eq!(client.status(), SessionStatus::AwaitingCheckpoint);
    assert!(client.projection().is_err());
    let token = client.connection();
    assert!(client.apply_batch(token, &[advance(&mut server)]).is_err());
    assert_eq!(client.runtime().unwrap().sim().state().completed_tick, 0);
    install(&mut client, &server, &[]);
    assert_eq!(client.status(), SessionStatus::Live);
    assert_eq!(client.projection().unwrap(), server.projection());
}

#[test]
fn two_replicas_follow_the_same_real_runtime_ticks() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut a = client();
    let mut b = client();
    install(&mut a, &server, &[]);
    install(&mut b, &server, &[]);
    for _ in 0..60 {
        let frame = advance(&mut server);
        for replica in [&mut a, &mut b] {
            let token = replica.connection();
            let outcomes = replica
                .apply_batch(token, std::slice::from_ref(&frame))
                .unwrap();
            assert_eq!(outcomes.len(), 1);
            assert!(outcomes[0].effects.is_empty());
            assert_eq!(outcomes[0].state_hash, frame.state_hash);
            assert_eq!(replica.projection().unwrap(), server.projection());
        }
    }
    assert_eq!(
        a.runtime().unwrap().snapshot().unwrap(),
        b.runtime().unwrap().snapshot().unwrap()
    );
}

#[test]
fn invalid_later_tick_cannot_publish_an_earlier_prefix() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    install(&mut client, &server, &[]);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let first = advance(&mut server);
    let mut second = advance(&mut server);
    second.accepted.rng_before ^= 1;
    assert!(
        client
            .apply_batch(client.connection(), &[first, second])
            .is_err()
    );
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(client.status(), SessionStatus::AwaitingCheckpoint);
    assert!(client.projection().is_err());
}

#[test]
fn a_server_hash_mismatch_requires_recovery_without_publishing_state() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    install(&mut client, &server, &[]);
    let before = client.runtime().unwrap().snapshot().unwrap();
    let mut frame = advance(&mut server);
    frame.state_hash[0] ^= 1;
    assert!(client.apply_batch(client.connection(), &[frame]).is_err());
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(client.status(), SessionStatus::AwaitingCheckpoint);
}

#[test]
fn exact_duplicate_does_not_emit_another_outcome() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    install(&mut client, &server, &[]);
    let frame = advance(&mut server);
    let token = client.connection();
    client
        .apply_batch(token, std::slice::from_ref(&frame))
        .unwrap();
    let before = client.runtime().unwrap().snapshot().unwrap();
    assert!(client.apply_batch(token, &[frame]).unwrap().is_empty());
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
    assert_eq!(client.status(), SessionStatus::Live);
}

#[test]
fn old_socket_callbacks_and_checkpoint_tickets_cannot_restore_after_reconnect() {
    let mut server = runtime(RuntimeRole::Authority);
    let mut client = client();
    let old_token = client.connection();
    let old_ticket = client.checkpoint_ticket().unwrap();
    client.suspend(old_token).unwrap();
    client.reconnect().unwrap();
    let current_ticket = client.checkpoint_ticket().unwrap();
    let bytes = server.snapshot().unwrap();
    assert!(
        client
            .install_checkpoint(
                old_ticket,
                Checkpoint {
                    completed_tick: 0,
                    state_hash: server.sim().state_hash().unwrap(),
                    bytes: &bytes,
                },
                &[]
            )
            .is_err()
    );
    assert!(
        client
            .apply_batch(old_token, &[advance(&mut server)])
            .is_err()
    );
    assert!(client.suspend(old_token).is_err());
    assert_eq!(client.checkpoint_ticket(), Some(current_ticket));
    install(&mut client, &server, &[]);
    assert_eq!(client.status(), SessionStatus::Live);
}

#[test]
fn recovery_cannot_move_the_committed_tick_backwards() {
    let mut server = runtime(RuntimeRole::Authority);
    let old_bytes = server.snapshot().unwrap();
    let old_hash = server.sim().state_hash().unwrap();
    let mut client = client();
    let _ = advance(&mut server);
    install(&mut client, &server, &[]);
    let before = client.runtime().unwrap().snapshot().unwrap();
    client.request_checkpoint(client.connection()).unwrap();
    let ticket = client.checkpoint_ticket().unwrap();
    assert!(
        client
            .install_checkpoint(
                ticket,
                Checkpoint {
                    completed_tick: 0,
                    state_hash: old_hash,
                    bytes: &old_bytes,
                },
                &[]
            )
            .is_err()
    );
    assert_eq!(client.runtime().unwrap().snapshot().unwrap(), before);
}

#[test]
fn closed_session_drops_runtime_and_cannot_be_reopened() {
    let mut client = client();
    let token = client.connection();
    client.close();
    assert!(client.runtime().is_none());
    assert_eq!(client.status(), SessionStatus::Closed);
    assert!(client.projection().is_err());
    assert!(client.reconnect().is_err());
    assert!(client.request_checkpoint(token).is_err());
}

#[path = "live_session/recovery.rs"]
mod recovery;
#[path = "live_session/source.rs"]
mod source;

#[path = "live_session/support.rs"]
mod support;
