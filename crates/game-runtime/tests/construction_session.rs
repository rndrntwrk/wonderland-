//! The grant and completion fixture are not a production account or journal.
#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_game_runtime::live_wire::construction::{session::*, *};
use wonderland_game_runtime::live_wire::player::PlayerBinding;
use wonderland_game_runtime::sim_core::world::build::*;
use wonderland_game_runtime::*;

fn setup() -> (
    GameRuntime,
    ConstructionGrant,
    ConstructionRequest,
    ConstructionSession,
) {
    let (game, _, actor) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let binding = PlayerBinding {
        source_epoch: 3,
        lot_incarnation: 4,
        lot_location: 55,
        avatar_id: 7,
    };
    let mut authority = BuildAuthority::new(actor, PersistentId(7), 1000);
    authority.can_build = true;
    authority.permissions_revision = 11;
    authority.account_revision = 12;
    authority.catalog_revision = 13;
    authority.prices.floor = 7;
    let grant = ConstructionGrant {
        binding,
        principal: support::PRINCIPAL,
        authority,
        floor_patterns: BTreeSet::from([1, 2]),
    };
    let state = game.sim().state();
    let request = ConstructionRequest {
        binding,
        request_id: 42,
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        architecture_revision: state.world.lot.revision().architecture,
        permissions_revision: 11,
        account_revision: 12,
        catalog_revision: 13,
        selections: vec![Selection::Floor {
            tile: TilePos::new(2, 2, 1),
            pattern: 1,
        }],
    };
    let session = ConstructionSession::new(&game, &grant, 1000).unwrap();
    (game, grant, request, session)
}
fn offer(
    game: &GameRuntime,
    grant: &ConstructionGrant,
    request: &ConstructionRequest,
    session: &mut ConstructionSession,
) -> QuoteView {
    session.offer(game, grant, 900, request, 100).unwrap()
}
fn complete(game: &mut GameRuntime, status: DurableBuildStatus) {
    let request = game
        .sim()
        .state()
        .world
        .builds
        .pending_effect()
        .unwrap()
        .clone();
    let committed = matches!(status, DurableBuildStatus::Committed);
    game.advance(vec![AcceptedCommand::CompleteBuild(
        ServerBuildConfirmation {
            operation: request.operation,
            actor: request.actor,
            owner: request.owner,
            preview_hash: request.preview_hash,
            charged_cost: if committed { request.cost } else { 0 },
            durable_receipt: if committed { 99 } else { 0 },
            status,
            created_objects: BTreeMap::new(),
        },
    )])
    .unwrap();
}
#[test]
fn stores_one_pure_quote_with_server_deadline_and_exact_price() {
    let (game, g, r, mut s) = setup();
    let before = game.snapshot().unwrap();
    let q = offer(&game, &g, &r, &mut s);
    assert_eq!(q.consent.cost, 7);
    assert_eq!(q.operation, 900);
    assert_eq!(q.expires_at_ms, 1100);
    assert_eq!(q.binding, r.binding);
    assert_eq!(game.snapshot().unwrap(), before);
}
#[test]
fn exact_duplicate_quote_does_not_extend_its_expiry() {
    let (game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    assert_eq!(s.offer(&game, &g, 900, &r, 1000).unwrap(), q);
}
#[test]
fn duplicate_id_cannot_change_selections_or_operation() {
    let (game, g, r, mut s) = setup();
    offer(&game, &g, &r, &mut s);
    let mut changed = r.clone();
    changed.selections = vec![Selection::Floor {
        tile: TilePos::new(1, 1, 1),
        pattern: 2,
    }];
    assert!(s.offer(&game, &g, 900, &changed, 101).is_err());
    assert!(s.offer(&game, &g, 901, &r, 102).is_err());
}
#[test]
fn another_quote_requires_expiry_or_explicit_cancellation() {
    let (game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    let mut next = r.clone();
    next.request_id += 1;
    assert!(s.offer(&game, &g, 901, &next, 101).is_err());
    s.cancel(&game, &g, q.consent.request_id, 900, 102).unwrap();
    assert!(s.offer(&game, &g, 901, &next, 103).is_ok());
    assert!(s.offer(&game, &g, 900, &r, 104).is_err());
}
#[test]
fn cancelled_quote_cannot_be_submitted() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.cancel(&game, &g, r.request_id, 900, 101).unwrap();
    assert!(s.submit(&mut game, &g, 900, q.consent, 102).is_err());
    assert_eq!(
        s.refresh(&game, &g, 103).unwrap().unwrap().phase,
        Phase::Cancelled
    );
}
#[test]
fn expiry_boundary_is_exclusive_and_does_not_reissue_old_id() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    assert!(s.submit(&mut game, &g, 900, q.consent, 1100).is_err());
    assert_eq!(
        s.refresh(&game, &g, 1100).unwrap().unwrap().phase,
        Phase::Expired
    );
    assert!(s.offer(&game, &g, 900, &r, 1101).is_err());
    let mut next = r.clone();
    next.request_id += 1;
    assert!(s.offer(&game, &g, 901, &next, 1102).is_ok());
}
#[test]
fn backwards_clock_and_deadline_overflow_do_not_publish_quotes() {
    let (game, g, r, mut s) = setup();
    offer(&game, &g, &r, &mut s);
    assert!(s.refresh(&game, &g, 99).is_err());
    let mut other = ConstructionSession::new(&game, &g, 1000).unwrap();
    assert!(other.offer(&game, &g, 900, &r, u64::MAX).is_err());
    assert!(other.view().is_none());
}
#[test]
fn zero_or_excessive_lease_is_rejected() {
    let (game, g, _, _) = setup();
    for ttl in [0, MAX_QUOTE_LIFETIME_MS + 1] {
        assert!(ConstructionSession::new(&game, &g, ttl).is_err());
    }
}
#[test]
fn wrong_consent_and_operation_never_advance_the_authority() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    let before = game.snapshot().unwrap();
    let mut c = q.consent;
    c.cost += 1;
    assert!(s.submit(&mut game, &g, 900, c, 101).is_err());
    assert!(s.submit(&mut game, &g, 901, q.consent, 102).is_err());
    assert_eq!(game.snapshot().unwrap(), before);
}
#[test]
fn submission_advances_once_and_returns_the_actual_frame_and_effect() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    let before = game.sim().state().completed_tick;
    let admitted = s
        .submit(&mut game, &g, 900, q.consent, 101)
        .unwrap()
        .unwrap();
    assert_eq!(admitted.frame.accepted.tick, before + 1);
    assert_eq!(admitted.frame.state_hash, admitted.outcome.state_hash);
    assert!(
        admitted
            .outcome
            .events
            .iter()
            .any(|e| matches!(e, RuntimeEvent::BuildRequested(_)))
    );
    assert_eq!(
        game.sim()
            .state()
            .world
            .lot
            .tile(TilePos::new(2, 2, 1))
            .unwrap()
            .floor,
        0
    );
    let snapshot = game.snapshot().unwrap();
    assert!(
        s.submit(&mut game, &g, 900, q.consent, 102)
            .unwrap()
            .is_none()
    );
    assert_eq!(snapshot, game.snapshot().unwrap());
}
#[test]
fn pending_is_not_success_and_only_native_completion_can_finish_it() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    assert_eq!(
        s.refresh(&game, &g, 102).unwrap().unwrap().phase,
        Phase::Pending
    );
    complete(&mut game, DurableBuildStatus::Committed);
    let v = s.refresh(&game, &g, 103).unwrap().unwrap();
    assert!(matches!(
        v.phase,
        Phase::Outcome(BuildCommitStatus::Committed { .. })
    ));
    let before = game.snapshot().unwrap();
    assert!(
        s.submit(&mut game, &g, 900, q.consent, 9000)
            .unwrap()
            .is_none()
    );
    assert_eq!(before, game.snapshot().unwrap());
}
#[test]
fn actual_rejection_is_reported_without_creating_geometry() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    complete(
        &mut game,
        DurableBuildStatus::Rejected {
            reason: BuildError::InsufficientFunds,
        },
    );
    assert_eq!(
        s.refresh(&game, &g, 102).unwrap().unwrap().phase,
        Phase::Outcome(BuildCommitStatus::Rejected {
            reason: BuildError::InsufficientFunds
        })
    );
    assert_eq!(
        game.sim()
            .state()
            .world
            .lot
            .tile(TilePos::new(2, 2, 1))
            .unwrap()
            .floor,
        0
    );
}
#[test]
fn disconnect_keeps_the_operation_and_recovery_does_not_resubmit() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    s.disconnect();
    assert_eq!(s.view().unwrap().phase, Phase::Unknown);
    let before = game.snapshot().unwrap();
    assert_eq!(
        s.refresh(&game, &g, 102).unwrap().unwrap().phase,
        Phase::Pending
    );
    assert!(
        s.submit(&mut game, &g, 900, q.consent, 103)
            .unwrap()
            .is_none()
    );
    assert_eq!(before, game.snapshot().unwrap());
    complete(&mut game, DurableBuildStatus::Committed);
    assert!(matches!(
        s.refresh(&game, &g, 104).unwrap().unwrap().phase,
        Phase::Outcome(_)
    ));
}
#[test]
fn disconnect_invalidates_unconfirmed_quote_instead_of_confirming_it() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.disconnect();
    assert_eq!(s.view().unwrap().phase, Phase::Cancelled);
    assert!(s.submit(&mut game, &g, 900, q.consent, 101).is_err());
}
#[test]
fn principal_transfer_does_not_transfer_the_quote_session() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    let mut other = g.clone();
    other.principal = PrincipalKey(900);
    game.advance(vec![AcceptedCommand::SetInteractionAuthority {
        actor: g.authority.actor,
        access: Some(InteractionAccess {
            principal: other.principal,
            allow_hidden: false,
        }),
    }])
    .unwrap();
    assert!(s.submit(&mut game, &other, 900, q.consent, 101).is_err());
    assert!(s.refresh(&game, &other, 102).is_err());
}
#[test]
fn revocation_during_quote_rejects_before_any_build_tick() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    let mut revoked = g.clone();
    revoked.authority.can_build = false;
    let before = game.snapshot().unwrap();
    assert!(s.submit(&mut game, &revoked, 900, q.consent, 101).is_err());
    assert_eq!(before, game.snapshot().unwrap());
}
#[test]
fn close_prevents_reuse_even_with_the_same_grant() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.close();
    assert!(s.view().is_none());
    assert!(s.offer(&game, &g, 900, &r, 101).is_err());
    assert!(s.submit(&mut game, &g, 900, q.consent, 102).is_err());
}

#[test]
fn reconciliation_is_not_completion_and_blocks_another_operation() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    // Trusted out-of-band architecture change, as in the original native engine's test.
    let mut state = game.sim().state().clone();
    state
        .world
        .lot
        .set_floor(TilePos::new(7, 7, 1), 99)
        .unwrap();
    let bytes = sim_core::snapshot::encode(&state, game.sim().content()).unwrap();
    game.restore(&bytes).unwrap();
    complete(&mut game, DurableBuildStatus::Committed);
    let v = s.refresh(&game, &g, 102).unwrap().unwrap();
    assert!(matches!(
        v.phase,
        Phase::Outcome(BuildCommitStatus::NeedsReconciliation { .. })
    ));
    let mut next = r;
    next.request_id += 1;
    next.architecture_revision = game.sim().state().world.lot.revision().architecture;
    assert!(
        s.offer(&game, &g, 901, &next, 103).is_err(),
        "unreconciled committed cost must not unlock another operation"
    );
}

#[test]
fn missing_native_operation_after_restore_stays_unknown_not_retryable() {
    let (mut game, g, r, mut s) = setup();
    let before = game.snapshot().unwrap();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    s.disconnect();
    game.restore(&before).unwrap();
    assert!(
        s.refresh(&game, &g, 102).is_err(),
        "regressed runtime cannot reconcile"
    );
    game.advance(vec![]).unwrap();
    assert_eq!(
        s.refresh(&game, &g, 103).unwrap().unwrap().phase,
        Phase::Unknown
    );
    let snapshot = game.snapshot().unwrap();
    assert!(
        s.submit(&mut game, &g, 900, q.consent, 104)
            .unwrap()
            .is_none()
    );
    let mut next = r;
    next.request_id += 1;
    assert!(s.offer(&game, &g, 901, &next, 105).is_err());
    assert_eq!(snapshot, game.snapshot().unwrap());
}

#[test]
fn native_outcome_with_reused_operation_but_other_quote_is_rejected() {
    let (mut game, g, r, mut s) = setup();
    let before = game.snapshot().unwrap();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    game.restore(&before).unwrap();
    let mut other = r;
    other.selections = vec![Selection::Floor {
        tile: TilePos::new(5, 5, 1),
        pattern: 1,
    }];
    let conflict = quote(&game, &g, 900, &other).unwrap();
    let command = confirm(&game, &g, &conflict, conflict.consent()).unwrap();
    game.advance(vec![command]).unwrap();
    complete(&mut game, DurableBuildStatus::Committed);
    assert!(s.refresh(&game, &g, 102).is_err());
    assert!(!matches!(s.view().unwrap().phase, Phase::Outcome(_)));
}

#[test]
fn loss_of_new_build_permission_does_not_erase_existing_operation_result() {
    let (mut game, mut g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 101).unwrap();
    g.authority.can_build = false;
    complete(&mut game, DurableBuildStatus::Committed);
    assert!(matches!(
        s.refresh(&game, &g, 102).unwrap().unwrap().phase,
        Phase::Outcome(BuildCommitStatus::Committed { .. })
    ));
}

#[test]
fn maximum_request_id_never_wraps_or_reuses_a_completed_correlation() {
    let (game, g, mut r, mut s) = setup();
    r.request_id = u64::MAX;
    let q = offer(&game, &g, &r, &mut s);
    s.cancel(&game, &g, q.consent.request_id, q.operation, 101)
        .unwrap();
    r.request_id = 1;
    assert!(s.offer(&game, &g, 901, &r, 102).is_err());
}

#[test]
fn old_cancel_cannot_cancel_a_newer_quote() {
    let (game, g, mut r, mut s) = setup();
    offer(&game, &g, &r, &mut s);
    s.cancel(&game, &g, 42, 900, 101).unwrap();
    r.request_id += 1;
    let newer = s.offer(&game, &g, 901, &r, 102).unwrap();
    assert!(s.cancel(&game, &g, 42, 900, 103).is_err());
    assert_eq!(s.view().unwrap().quote, newer);
    assert_eq!(s.view().unwrap().phase, Phase::Quoted);
}

#[test]
fn submitted_operation_does_not_expire_or_accept_quote_cancellation() {
    let (mut game, g, r, mut s) = setup();
    let q = offer(&game, &g, &r, &mut s);
    s.submit(&mut game, &g, 900, q.consent, 1099).unwrap();
    assert!(s.cancel(&game, &g, 42, 900, 5000).is_err());
    assert_eq!(
        s.refresh(&game, &g, 5001).unwrap().unwrap().phase,
        Phase::Pending
    );
}

#[test]
fn admitted_frame_replays_in_the_real_replica_without_a_second_authority_command() {
    let (mut game, g, r, mut s) = setup();
    let (_, mut replica, _) = support::pair_with_content(
        support::content("cursebook_set_permission.iff", 4107, false),
        true,
    );
    let q = offer(&game, &g, &r, &mut s);
    let admitted = s
        .submit(&mut game, &g, 900, q.consent, 101)
        .unwrap()
        .unwrap();
    let outcomes = replica
        .apply_batch(replica.connection(), &[admitted.frame])
        .unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].state_hash, admitted.outcome.state_hash);
    assert!(outcomes[0].effects.is_empty());
    assert_eq!(
        replica.runtime().unwrap().snapshot().unwrap(),
        game.snapshot().unwrap()
    );
}
