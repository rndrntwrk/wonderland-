//! Wire fixture is a test-only original serializer fixture, not player content.
use wonderland_player_authoring::*;
use wonderland_vm_protocol::snapshot::*;
fn source() -> Snapshot {
    let mut snapshot = decode_snapshot(
        include_bytes!("../../vm-protocol/tests/fixtures/source-v38-object.fsov"),
        &Default::default(),
    )
    .unwrap();
    let mut actor = snapshot.entities[0].clone();
    actor.object_id = 99;
    actor.persist_id = 42;
    actor.platform = EntityPlatform::Avatar {
        budget: 123456789,
        permissions: 3,
        ignored: vec![],
        jobs: vec![],
        flags: 0,
        chat_rgb: [0; 3],
        chat_pitch: 0,
        chat_channel: 0,
    };
    snapshot.entities.push(actor);
    snapshot
}
fn actor() -> SourceActorLot {
    SourceActorLot {
        avatar_id: 42,
        lot_id: Some(987),
        location: 55,
        epoch: 2,
        incarnation: 7,
    }
}
#[test]
fn actual_state_sync_preserves_packed_location_and_exact_actor_values() {
    let mut state = AuthoringState::default();
    state.observe_snapshot(actor(), &source(), 10).unwrap();
    let snapshot = state.snapshot.as_ref().expect("source snapshot installed");
    assert_eq!(snapshot.actor.lot_id, Some(987));
    assert_eq!(snapshot.actor.location, 55);
    assert_eq!(snapshot.permission, Some(3));
    assert_eq!(snapshot.budget, Some(123456789));
    assert_eq!(
        snapshot.bounds,
        Some(LotBounds {
            width: 2,
            height: 2,
            levels: 1
        })
    );
    assert_eq!(snapshot.objects[0].guid, 0x12345678);
}
#[test]
fn mismatched_lot_or_missing_actor_never_installs_a_snapshot() {
    let mut state = AuthoringState::default();
    let mut s = source();
    s.platform.lot_id = 987;
    assert_eq!(
        state.observe_snapshot(actor(), &s, 1),
        Err(AuthoringError::Stale)
    );
    assert!(state.snapshot.is_none());
    let mut s = source();
    s.entities.pop();
    assert_eq!(
        state.observe_snapshot(actor(), &s, 1),
        Err(AuthoringError::Missing("source actor"))
    );
    assert!(state.snapshot.is_none());
}
#[test]
fn refresh_expires_picks_but_preserves_eod_and_unknown_transaction() {
    let mut state = AuthoringState::default();
    let source = source();
    state.observe_snapshot(actor(), &source, 10).unwrap();
    let old = state.snapshot.as_ref().unwrap().objects[0].entity.clone();
    assert_eq!(
        state
            .snapshot
            .as_ref()
            .unwrap()
            .prepare(&AuthoringIntent::Move {
                entity: old.clone(),
                placement: Placement {
                    x: 16,
                    y: 16,
                    level: 1,
                    direction: 1
                }
            }),
        Err(AuthoringError::Missing("source object movability"))
    );
    let enter = SourceEodMessage {
        actor_uid: 42,
        plugin_id: DRESSER_PLUGIN,
        event_name: "eod_enter".into(),
        text: Some(String::new()),
        binary: None,
        incarnation: 19,
    };
    state.observe_eod(actor(), &enter).unwrap();
    state
        .submit(AuthoringIntent::SetRoof {
            pitch: 0.5,
            style: 9,
        })
        .unwrap();
    state.observe_snapshot(actor(), &source, 11).unwrap();
    assert!(state.snapshot.as_ref().unwrap().eod.is_some());
    assert!(matches!(state.status, OperationState::Unknown(_)));
    assert!(
        state.pending.is_some(),
        "refresh cannot permit duplicate unconfirmed costs"
    );
    assert!(state.draft.is_none());
    assert_eq!(
        state
            .snapshot
            .as_ref()
            .unwrap()
            .prepare(&AuthoringIntent::Delete {
                entity: old,
                desired_mode: 2
            }),
        Err(AuthoringError::Stale)
    );
    assert_eq!(
        state.observe_snapshot(actor(), &source, 10),
        Err(AuthoringError::Stale)
    );
}
#[test]
fn unreplayed_tick_invalidates_world_authority_but_keeps_eod_and_unknown_write() {
    let mut state = AuthoringState::default();
    state.observe_snapshot(actor(), &source(), 10).unwrap();
    let enter = SourceEodMessage {
        actor_uid: 42,
        plugin_id: DRESSER_PLUGIN,
        event_name: "eod_enter".into(),
        text: Some(String::new()),
        binary: None,
        incarnation: 19,
    };
    state.observe_eod(actor(), &enter).unwrap();
    state
        .submit(AuthoringIntent::SetRoof {
            pitch: 0.5,
            style: 9,
        })
        .unwrap();
    state.invalidate_world_projection();
    assert_eq!(
        state.prepare(&AuthoringIntent::SetRoof {
            pitch: 0.5,
            style: 9
        }),
        Err(AuthoringError::Missing("current world projection"))
    );
    assert!(state.snapshot.as_ref().unwrap().eod.is_some());
    assert!(state.pending.is_some());
    assert!(matches!(state.status, OperationState::Unknown(_)));
    state
        .observe_eod(
            actor(),
            &SourceEodMessage {
                event_name: "set_outfits".into(),
                binary: Some(0i32.to_be_bytes().to_vec()),
                ..enter
            },
        )
        .unwrap();
    assert!(
        !state.world_projection_valid,
        "EOD observations cannot revive stale world authority"
    );
    state.abandon_pending();
    assert!(state.pending.is_none());
    assert_eq!(state.unknown_operations.len(), 1);
    assert!(matches!(state.status, OperationState::Unknown(_)));
    state.observe_snapshot(actor(), &source(), 11).unwrap();
    assert!(state.world_projection_valid);
}
#[test]
fn new_lot_eod_before_state_sync_does_not_inherit_old_presentation_fence() {
    for clear_snapshot in [false, true] {
        let mut state = AuthoringState::default();
        state.observe_snapshot(actor(), &source(), 100).unwrap();
        state
            .observe_eod(
                actor(),
                &SourceEodMessage {
                    actor_uid: 42,
                    plugin_id: DRESSER_PLUGIN,
                    event_name: "eod_enter".into(),
                    text: Some(String::new()),
                    binary: None,
                    incarnation: 200,
                },
            )
            .unwrap();
        if clear_snapshot {
            state.snapshot = None;
        }
        let mut next = actor();
        next.incarnation += 1;
        let enter = SourceEodMessage {
            actor_uid: 42,
            plugin_id: DRESSER_PLUGIN,
            event_name: "eod_enter".into(),
            text: Some(String::new()),
            binary: None,
            incarnation: 20,
        };
        state.observe_eod(next.clone(), &enter).unwrap();
        assert_eq!(state.presentation_generation, None);
        assert!(state.diagonal_floor_tiles.is_none());
        assert!(
            state.observe_snapshot(next.clone(), &source(), 1).is_ok(),
            "the new native session starts a new presentation fence"
        );
        assert_eq!(
            state.observe_snapshot(next, &source(), 1),
            Err(AuthoringError::Stale)
        );
    }
}
