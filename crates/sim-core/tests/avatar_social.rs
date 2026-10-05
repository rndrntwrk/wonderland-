use sim_core::avatars::lifecycle::*;
use sim_core::avatars::social::*;
use sim_core::ids::{EntityRef, ObjectId, PersistentId};

fn entity(id: i16) -> EntityRef {
    EntityRef {
        object_id: ObjectId(id),
        generation: 1,
    }
}
fn participant(id: i16, role: u16) -> Participant {
    Participant {
        entity: entity(id),
        action_id: id as u64,
        role,
        session_epoch: 1,
        connected: true,
        ready_barrier: None,
        finished: false,
    }
}
fn reservation() -> SocialReservation {
    SocialReservation {
        resource: entity(10),
        slot: 0,
        token: 99,
    }
}

#[test]
fn avatar_social_barrier_snapshot_reconnect_and_contended_participant() {
    let mut social = SocialCoordinator::default();
    social
        .begin(
            SocialId(1),
            vec![participant(1, 0), participant(2, 1)],
            vec![reservation()],
        )
        .unwrap();
    assert!(matches!(
        social.begin(
            SocialId(2),
            vec![participant(2, 0), participant(3, 1)],
            Vec::new()
        ),
        Err(SocialError::ParticipantBusy(_))
    ));
    assert_eq!(social.interactions.len(), 1);
    assert!(social
        .ready(SocialId(1), entity(1), 1, 0, 12)
        .unwrap()
        .is_empty());
    social.disconnect(entity(1), 1).unwrap();
    let mut restored: SocialCoordinator =
        bincode::deserialize(&bincode::serialize(&social).unwrap()).unwrap();
    restored.validate().unwrap();
    for coordinator in [&mut social, &mut restored] {
        coordinator.reconnect(entity(1), 2).unwrap();
        assert_eq!(
            coordinator.ready(SocialId(1), entity(1), 1, 0, 12),
            Err(SocialError::StaleSession)
        );
        assert_eq!(
            coordinator.ready(SocialId(1), entity(2), 1, 0, 13).unwrap(),
            vec![SocialEvent::Released {
                social: SocialId(1),
                barrier: 0,
                tick: 13,
                participants: vec![entity(1), entity(2)]
            }]
        );
    }
    assert_eq!(social, restored);
}

#[test]
fn avatar_social_departure_releases_reservations_and_cancels_each_action_once() {
    let mut social = SocialCoordinator::default();
    social
        .begin(
            SocialId(1),
            vec![participant(1, 0), participant(2, 1)],
            vec![reservation()],
        )
        .unwrap();
    let stale = EntityRef {
        generation: 2,
        ..entity(1)
    };
    assert!(social
        .depart(stale, CancellationReason::Departure)
        .unwrap()
        .is_empty());
    let events = social
        .depart(entity(1), CancellationReason::Departure)
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SocialEvent::CancelAction { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SocialEvent::ReleaseReservation(_)))
            .count(),
        1
    );
    assert!(social
        .depart(entity(1), CancellationReason::Departure)
        .unwrap()
        .is_empty());
    assert!(social.active_participants.is_empty());
    assert!(social.active_reservations.is_empty());
    social.validate().unwrap();
}

#[test]
fn avatar_social_rejects_two_grants_for_same_physical_slot_atomically() {
    let mut social = SocialCoordinator::default();
    social
        .begin(
            SocialId(1),
            vec![participant(1, 0), participant(2, 1)],
            vec![reservation()],
        )
        .unwrap();
    let mut other = reservation();
    other.token = 100;
    let old = social.clone();
    assert_eq!(
        social.begin(
            SocialId(2),
            vec![participant(3, 0), participant(4, 1)],
            vec![other]
        ),
        Err(SocialError::ReservationBusy)
    );
    assert_eq!(social, old);
}

#[test]
fn avatar_social_barrier_counter_exhaustion_is_atomic() {
    let mut social = SocialCoordinator::default();
    social
        .begin(
            SocialId(1),
            vec![participant(1, 0), participant(2, 1)],
            Vec::new(),
        )
        .unwrap();
    let interaction = social.interactions.get_mut(&SocialId(1)).unwrap();
    interaction.next_barrier = u32::MAX;
    interaction
        .participants
        .get_mut(&entity(1))
        .unwrap()
        .ready_barrier = Some(u32::MAX);
    social.validate().unwrap();
    let before = social.clone();
    assert_eq!(
        social.ready(SocialId(1), entity(2), 1, u32::MAX, 12),
        Err(SocialError::CounterExhausted)
    );
    assert_eq!(social, before);
}

#[test]
fn avatar_relationships_wrap_before_clamp_and_count_only_avatar_long_term_friends() {
    let mut relationships = RelationshipState::default();
    let mut request = RelationshipRequest {
        target: RelationshipTarget::Persistent(PersistentId(5)),
        variable: 1,
        operation: RelationshipOperation::Read,
        fail_if_too_small: true,
        never_clamp: false,
        category_multiplier: 1.0,
    };
    assert_eq!(
        relationships.apply(request).unwrap(),
        RelationshipResult::TooSmall
    );
    assert!(relationships.values.is_empty());
    request.fail_if_too_small = false;
    request.operation = RelationshipOperation::Set(60);
    relationships.apply(request).unwrap();
    request.target = RelationshipTarget::Persistent(PersistentId(1 << 24));
    relationships.apply(request).unwrap();
    assert_eq!(relationships.outgoing_friend_count(), 1);
    request.target = RelationshipTarget::Local(entity(2));
    request.never_clamp = true;
    request.operation = RelationshipOperation::Set(32760);
    relationships.apply(request).unwrap();
    request.never_clamp = false;
    request.operation = RelationshipOperation::Add(100);
    request.category_multiplier = 1.5;
    assert_eq!(
        relationships.apply(request).unwrap(),
        RelationshipResult::Value(-100)
    );
    assert_eq!(
        relationship_target(true, false, false, 1, 2, entity(2), PersistentId(99)),
        RelationshipTarget::Neighbor(2)
    );
    relationships.validate().unwrap();
}

#[test]
fn avatar_leave_source_deadline_no_leave_action_and_session_fence() {
    let mut life = AvatarLifecycle::default();
    life.reconnect(1).unwrap();
    life.disconnect(1, 5).unwrap();
    life.reconnect(2).unwrap();
    assert_eq!(life.disconnect(1, 6), Err(LifecycleError::StaleSession));
    let context = LeaveContext {
        action_available: true,
        action_already_queued: false,
    };
    assert_eq!(life.request_leave(context).len(), 2);
    assert_eq!(life.kill_timeout, 0);
    let queued = LeaveContext {
        action_already_queued: true,
        ..context
    };
    for _ in 0..1800 {
        assert!(!life
            .tick(queued)
            .iter()
            .any(|r| matches!(r, LifecycleRequest::Delete { .. })));
    }
    assert_eq!(
        life.tick(queued),
        vec![
            LifecycleRequest::ForceEodDisconnect,
            LifecycleRequest::Delete { cleanup: true }
        ]
    );
    assert!(life.tick(queued).is_empty());
    let mut missing = AvatarLifecycle::default();
    missing.request_leave(LeaveContext {
        action_available: false,
        action_already_queued: false,
    });
    assert_eq!(missing.kill_timeout, 1800);
    assert!(missing
        .tick(queued)
        .iter()
        .any(|r| matches!(r, LifecycleRequest::Delete { .. })));
}

#[test]
fn avatar_relationship_capacity_and_dirty_capacity_fail_atomically() {
    let mut relationships = RelationshipState::default();
    for id in 1..=65_536 {
        relationships
            .values
            .insert(RelationshipTarget::Persistent(PersistentId(id)), vec![0]);
    }
    relationships.validate().unwrap();
    let before = relationships.clone();
    let mut request = RelationshipRequest {
        target: RelationshipTarget::Persistent(PersistentId(65_537)),
        variable: 0,
        operation: RelationshipOperation::Set(1),
        fail_if_too_small: false,
        never_clamp: false,
        category_multiplier: 1.0,
    };
    assert_eq!(
        relationships.apply(request),
        Err(SocialError::CapacityExceeded)
    );
    assert_eq!(relationships, before);
    relationships.changed_persistent = (2..=65_537).map(PersistentId).collect();
    relationships.validate().unwrap();
    let before = relationships.clone();
    request.target = RelationshipTarget::Persistent(PersistentId(1));
    assert_eq!(
        relationships.apply(request),
        Err(SocialError::CapacityExceeded)
    );
    assert_eq!(relationships, before);
}

#[test]
fn avatar_terminal_social_capacity_does_not_create_partial_claims() {
    let mut social = SocialCoordinator::default();
    social
        .begin(
            SocialId(1),
            vec![participant(1, 0), participant(2, 1)],
            Vec::new(),
        )
        .unwrap();
    social
        .cancel(SocialId(1), CancellationReason::User)
        .unwrap();
    let terminal = social.interactions[&SocialId(1)].clone();
    for id in 2..=65_536 {
        let mut interaction = terminal.clone();
        interaction.id = SocialId(id);
        social.interactions.insert(SocialId(id), interaction);
    }
    social.validate().unwrap();
    let before = social.clone();
    assert_eq!(
        social.begin(
            SocialId(65_537),
            vec![participant(1, 0), participant(2, 1)],
            Vec::new()
        ),
        Err(SocialError::CapacityExceeded)
    );
    assert_eq!(social, before);
    assert!(social.active_participants.is_empty());
}

#[test]
fn avatar_lifecycle_snapshot_rejects_inconsistent_phase_and_resumes_deadline() {
    let mut life = AvatarLifecycle {
        phase: LifecyclePhase::Leaving,
        ..AvatarLifecycle::default()
    };
    assert!(life.validate().is_err());
    life.phase = LifecyclePhase::Active;
    life.kill_timeout = 0;
    assert!(life.validate().is_err());
    life.phase = LifecyclePhase::DeleteRequested;
    life.kill_timeout = -1;
    assert!(life.validate().is_err());
    life.phase = LifecyclePhase::Removed;
    assert!(life.validate().is_err());
    life.phase = LifecyclePhase::Leaving;
    life.kill_timeout = 1800;
    life.validate().unwrap();
    let mut restored: AvatarLifecycle =
        bincode::deserialize(&bincode::serialize(&life).unwrap()).unwrap();
    let ctx = LeaveContext {
        action_available: true,
        action_already_queued: true,
    };
    assert_eq!(life.tick(ctx), restored.tick(ctx));
    assert_eq!(life, restored);
    restored.validate().unwrap();
    assert_eq!(restored.phase, LifecyclePhase::DeleteRequested);
}
