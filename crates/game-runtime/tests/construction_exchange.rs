//! Actual runtime/session fixtures, not production money, network authentication or a journal.
#[allow(dead_code)]
#[path = "live_session/support.rs"]
mod support;
use std::collections::{BTreeMap, BTreeSet};
use wonderland_game_runtime::live_wire::construction::{
    exchange::*, session::ConstructionSession, *,
};
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
    let request = ConstructionRequest {
        binding,
        request_id: 42,
        lot_id: 11,
        authority_epoch: 7,
        architecture_revision: game.sim().state().world.lot.revision().architecture,
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
fn call(request: &ConstructionRequest, call_id: u64, command: Command) -> Vec<u8> {
    encode_call(&Call {
        binding: request.binding,
        call_id,
        command,
    })
    .unwrap()
}
fn quote_call(request: &ConstructionRequest) -> Vec<u8> {
    call(request, 1, Command::Quote(request.clone()))
}
fn quote_at(
    game: &mut GameRuntime,
    grant: &ConstructionGrant,
    request: &ConstructionRequest,
    session: &mut ConstructionSession,
) -> Snapshot {
    let before = game.snapshot().unwrap();
    let handled = dispatch(session, game, grant, &quote_call(request), Some(900), 100).unwrap();
    assert!(handled.admitted.is_none());
    assert_eq!(before, game.snapshot().unwrap());
    decode_reply(handled.reply.as_ref().unwrap())
        .unwrap()
        .snapshot
        .unwrap()
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
fn quote_dispatch_is_pure_and_preserves_price_operation_and_expiry() {
    let (mut game, grant, request, mut session) = setup();
    let snap = quote_at(&mut game, &grant, &request, &mut session);
    assert_eq!(snap.consent.cost, 7);
    assert_eq!(snap.operation, 900);
    assert_eq!(snap.expires_at_ms, 1100);
    assert_eq!(snap.phase, WirePhase::Quoted);
}
#[test]
fn construction_codec_preserves_full_width_ids_and_rejects_other_direction() {
    let (_, _, mut request, _) = setup();
    request.request_id = u64::MAX;
    request.binding.source_epoch = u64::MAX - 1;
    request.lot_id = 9_007_199_254_740_993;
    let value = Call {
        binding: request.binding,
        call_id: u64::MAX,
        command: Command::Quote(request),
    };
    let bytes = encode_call(&value).unwrap();
    assert_eq!(decode_call(&bytes).unwrap(), value);
    assert!(decode_reply(&bytes).is_err());
    let status = Call {
        binding: value.binding,
        call_id: u64::MAX,
        command: Command::Status {
            request_id: u64::MAX,
            operation: None,
        },
    };
    let bytes = encode_call(&status).unwrap();
    let reply = encode_reply(&Reply {
        request: bytes.clone(),
        snapshot: None,
    })
    .unwrap();
    assert!(decode_call(&reply).is_err());
    assert_eq!(decode_reply(&reply).unwrap().request, bytes);
}
#[test]
fn malformed_envelopes_do_not_enter_the_authority() {
    let (mut game, grant, request, mut session) = setup();
    let bytes = quote_call(&request);
    let before = game.snapshot().unwrap();
    for n in 0..bytes.len() {
        assert!(decode_call(&bytes[..n]).is_err());
    }
    for offset in [0, 4, 8, 15, 16] {
        let mut wrong = bytes.clone();
        wrong[offset] ^= 255;
        assert!(dispatch(&mut session, &mut game, &grant, &wrong, Some(900), 100).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_call(&trailing).is_err());
    assert_eq!(game.snapshot().unwrap(), before);
    assert!(session.view().is_none());
}
#[test]
fn authenticated_binding_and_principal_are_checked_before_returning_status() {
    let (mut game, grant, request, mut session) = setup();
    let snap = quote_at(&mut game, &grant, &request, &mut session);
    let status = call(
        &request,
        2,
        Command::Status {
            request_id: request.request_id,
            operation: Some(snap.operation),
        },
    );
    let mut other = grant.clone();
    other.principal = PrincipalKey(1234);
    assert!(dispatch(&mut session, &mut game, &other, &status, None, 101).is_err());
    let mut invalid = decode_call(&status).unwrap();
    invalid.binding.avatar_id += 1;
    assert!(
        dispatch(
            &mut session,
            &mut game,
            &grant,
            &encode_call(&invalid).unwrap(),
            None,
            101
        )
        .is_err()
    );
}
#[test]
fn confirm_dispatch_admits_once_but_never_fabricates_a_commit() {
    let (mut game, grant, request, mut session) = setup();
    let snap = quote_at(&mut game, &grant, &request, &mut session);
    let bytes = call(
        &request,
        2,
        Command::Confirm {
            operation: 900,
            consent: snap.consent,
        },
    );
    let first = dispatch(&mut session, &mut game, &grant, &bytes, None, 101).unwrap();
    assert!(first.admitted.is_some());
    assert_eq!(
        decode_reply(first.reply.as_ref().unwrap())
            .unwrap()
            .snapshot
            .unwrap()
            .phase,
        WirePhase::Pending
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
    let before = game.snapshot().unwrap();
    let again = dispatch(&mut session, &mut game, &grant, &bytes, None, 102).unwrap();
    assert!(again.admitted.is_none());
    assert_eq!(game.snapshot().unwrap(), before);
}
#[test]
fn status_returns_only_the_exact_requested_operation() {
    let (mut game, grant, request, mut session) = setup();
    quote_at(&mut game, &grant, &request, &mut session);
    let bytes = call(
        &request,
        2,
        Command::Status {
            request_id: request.request_id,
            operation: Some(901),
        },
    );
    assert!(dispatch(&mut session, &mut game, &grant, &bytes, None, 101).is_err());
}
#[test]
fn real_completion_is_reported_without_reexecuting_or_redispatching_the_build() {
    let (mut game, grant, request, mut session) = setup();
    let snap = quote_at(&mut game, &grant, &request, &mut session);
    let confirm = call(
        &request,
        2,
        Command::Confirm {
            operation: 900,
            consent: snap.consent,
        },
    );
    let _admitted = dispatch(&mut session, &mut game, &grant, &confirm, None, 101).unwrap();
    complete(&mut game, DurableBuildStatus::Committed);
    let before = game.snapshot().unwrap();
    let status = call(
        &request,
        3,
        Command::Status {
            request_id: request.request_id,
            operation: Some(900),
        },
    );
    let reply = dispatch(&mut session, &mut game, &grant, &status, None, 102).unwrap();
    assert!(reply.admitted.is_none());
    assert!(matches!(
        decode_reply(reply.reply.as_ref().unwrap())
            .unwrap()
            .snapshot
            .unwrap()
            .phase,
        WirePhase::Outcome(BuildCommitStatus::Committed { .. })
    ));
    assert_eq!(game.snapshot().unwrap(), before);
    assert_eq!(
        game.sim()
            .state()
            .world
            .lot
            .tile(TilePos::new(2, 2, 1))
            .unwrap()
            .floor,
        1
    );
}
#[test]
fn explicit_cancel_closes_only_an_unsubmitted_quote() {
    let (mut game, grant, request, mut session) = setup();
    quote_at(&mut game, &grant, &request, &mut session);
    let cancel = call(
        &request,
        2,
        Command::Cancel {
            request_id: request.request_id,
            operation: 900,
        },
    );
    let reply = dispatch(&mut session, &mut game, &grant, &cancel, None, 101).unwrap();
    assert_eq!(
        decode_reply(reply.reply.as_ref().unwrap())
            .unwrap()
            .snapshot
            .unwrap()
            .phase,
        WirePhase::Cancelled
    );
}

use wonderland_game_runtime::live_wire::construction::exchange::client::{Client, ClientStage};
fn client_for(request: &ConstructionRequest) -> Client {
    Client::new(request.binding, request.lot_id, request.authority_epoch).unwrap()
}
fn client_quote(
    client: &mut Client,
    game: &mut GameRuntime,
    grant: &ConstructionGrant,
    request: &ConstructionRequest,
    session: &mut ConstructionSession,
) {
    let pending = client.begin(request.clone()).unwrap();
    let result = dispatch(session, game, grant, &pending.bytes, Some(900), 100).unwrap();
    client
        .accept(pending.connection, result.reply.as_ref().unwrap())
        .unwrap();
    assert_eq!(client.stage(), ClientStage::Reviewing);
}
#[test]
fn client_requires_exact_reviewed_consent_and_waits_for_replica_commit() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let consent = client.quote().unwrap().consent;
    let mut wrong = consent;
    wrong.cost += 1;
    assert!(client.confirm(wrong).is_err());
    let submitted = client.confirm(consent).unwrap();
    let accepted = dispatch(&mut session, &mut game, &grant, &submitted.bytes, None, 101).unwrap();
    client
        .accept(submitted.connection, accepted.reply.as_ref().unwrap())
        .unwrap();
    assert_eq!(client.stage(), ClientStage::Pending);
    let stale_projection = game.projection();
    complete(&mut game, DurableBuildStatus::Committed);
    let status = client.poll().unwrap();
    let reply = dispatch(&mut session, &mut game, &grant, &status.bytes, None, 102).unwrap();
    client
        .accept(status.connection, reply.reply.as_ref().unwrap())
        .unwrap();
    assert_eq!(client.stage(), ClientStage::AwaitingReplica);
    assert!(
        !client
            .observe(status.connection, &stale_projection)
            .unwrap()
    );
    assert_eq!(client.stage(), ClientStage::AwaitingReplica);
    assert!(
        client
            .observe(status.connection, &game.projection())
            .unwrap()
    );
    assert_eq!(client.stage(), ClientStage::Committed);
}
#[test]
fn lost_confirmation_uses_read_only_status_and_never_prepares_an_automatic_retry() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let sent = client.confirm(client.quote().unwrap().consent).unwrap();
    let _admitted = dispatch(&mut session, &mut game, &grant, &sent.bytes, None, 101).unwrap();
    client.disconnect().unwrap();
    session.disconnect();
    client.reconnect().unwrap();
    assert_eq!(client.stage(), ClientStage::Unknown);
    assert!(client.confirm(client.quote().unwrap().consent).is_err());
    let mut newer = request.clone();
    newer.request_id += 1;
    assert!(client.begin(newer).is_err());
    let status = client.poll().unwrap();
    assert!(matches!(
        decode_call(&status.bytes).unwrap().command,
        Command::Status {
            request_id: 42,
            operation: Some(900)
        }
    ));
    let before = game.snapshot().unwrap();
    let reply = dispatch(&mut session, &mut game, &grant, &status.bytes, None, 102).unwrap();
    assert!(reply.admitted.is_none());
    assert_eq!(game.snapshot().unwrap(), before);
    client
        .accept(status.connection, reply.reply.as_ref().unwrap())
        .unwrap();
    assert_eq!(client.stage(), ClientStage::Pending);
}
#[test]
fn obsolete_callback_is_rejected_before_decoding_and_cannot_replace_new_state() {
    let (_, _, request, _) = setup();
    let mut client = client_for(&request);
    let first = client.begin(request).unwrap();
    client.disconnect().unwrap();
    client.reconnect().unwrap();
    assert!(
        client
            .accept(first.connection, b"bad bytes from old connection")
            .is_err()
    );
    assert_eq!(client.stage(), ClientStage::Unknown);
    assert!(client.poll().is_ok());
}
#[test]
fn client_matches_complete_request_bytes_not_only_call_number() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    let prepared = client.begin(request.clone()).unwrap();
    let handled = dispatch(
        &mut session,
        &mut game,
        &grant,
        &prepared.bytes,
        Some(900),
        100,
    )
    .unwrap();
    let valid = handled.reply.unwrap();
    let mut bad = decode_reply(&valid).unwrap();
    let mut echo = decode_call(&bad.request).unwrap();
    if let Command::Quote(request) = &mut echo.command {
        request.account_revision += 1;
    }
    bad.request = encode_call(&echo).unwrap();
    assert!(
        client
            .accept(prepared.connection, &encode_reply(&bad).unwrap())
            .is_err()
    );
    assert_eq!(client.stage(), ClientStage::Quoting);
    client.accept(prepared.connection, &valid).unwrap();
    assert_eq!(client.stage(), ClientStage::Reviewing);
}
#[test]
fn no_server_record_after_a_lost_submit_remains_unknown_not_rejected_or_safe_to_repeat() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let _sent = client.confirm(client.quote().unwrap().consent).unwrap();
    client.disconnect().unwrap();
    client.reconnect().unwrap();
    let mut empty = ConstructionSession::new(&game, &grant, 1000).unwrap();
    let status = client.poll().unwrap();
    let reply = dispatch(&mut empty, &mut game, &grant, &status.bytes, None, 101).unwrap();
    client
        .accept(status.connection, reply.reply.as_ref().unwrap())
        .unwrap();
    assert_eq!(client.stage(), ClientStage::Unknown);
    let mut next = request;
    next.request_id += 1;
    assert!(client.begin(next).is_err());
}
#[test]
fn old_quotes_cannot_change_price_or_deadline_during_status_recovery() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let query = client.poll().unwrap();
    let handled = dispatch(&mut session, &mut game, &grant, &query.bytes, None, 101).unwrap();
    let reply = handled.reply.unwrap();
    let mut corrupt = decode_reply(&reply).unwrap();
    corrupt.snapshot.as_mut().unwrap().consent.cost += 10;
    assert!(
        client
            .accept(query.connection, &encode_reply(&corrupt).unwrap())
            .is_err()
    );
    corrupt = decode_reply(&reply).unwrap();
    corrupt.snapshot.as_mut().unwrap().expires_at_ms += 1000;
    assert!(
        client
            .accept(query.connection, &encode_reply(&corrupt).unwrap())
            .is_err()
    );
    client.accept(query.connection, &reply).unwrap();
    assert_eq!(client.quote().unwrap().consent.cost, 7);
}
#[test]
fn replica_with_different_world_or_conflicting_cost_cannot_mark_a_purchase_complete() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let sent = client.confirm(client.quote().unwrap().consent).unwrap();
    let _admitted = dispatch(&mut session, &mut game, &grant, &sent.bytes, None, 101).unwrap();
    complete(&mut game, DurableBuildStatus::Committed);
    let mut wrong = game.projection();
    wrong.lot_id += 1;
    assert!(client.observe(sent.connection, &wrong).is_err());
    assert_ne!(client.stage(), ClientStage::Committed);
    assert!(client.observe(sent.connection, &game.projection()).unwrap());
    assert_eq!(client.stage(), ClientStage::Committed);
}
#[test]
fn cancelled_quote_allows_a_new_explicit_request_but_rejects_its_old_callback() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let cancel = client.cancel().unwrap();
    let result = dispatch(&mut session, &mut game, &grant, &cancel.bytes, None, 101).unwrap();
    let reply = result.reply.unwrap();
    client.accept(cancel.connection, &reply).unwrap();
    assert_eq!(client.stage(), ClientStage::Cancelled);
    let mut next = request;
    next.request_id += 1;
    let next_call = client.begin(next).unwrap();
    assert!(client.accept(cancel.connection, &reply).is_err());
    assert_eq!(client.stage(), ClientStage::Quoting);
    assert!(
        decode_call(&next_call.bytes).unwrap().call_id
            > decode_call(&cancel.bytes).unwrap().call_id
    );
}
#[test]
fn a_real_durable_rejection_is_not_presented_as_a_successful_build() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let sent = client.confirm(client.quote().unwrap().consent).unwrap();
    let _admitted = dispatch(&mut session, &mut game, &grant, &sent.bytes, None, 101).unwrap();
    complete(
        &mut game,
        DurableBuildStatus::Rejected {
            reason: BuildError::InsufficientFunds,
        },
    );
    assert!(client.observe(sent.connection, &game.projection()).unwrap());
    assert_eq!(client.stage(), ClientStage::Rejected);
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
fn missing_later_status_cannot_erase_a_committed_replica_outcome() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let sent = client.confirm(client.quote().unwrap().consent).unwrap();
    let _admitted = dispatch(&mut session, &mut game, &grant, &sent.bytes, None, 101).unwrap();
    complete(&mut game, DurableBuildStatus::Committed);
    client.observe(sent.connection, &game.projection()).unwrap();
    let query = client.poll().unwrap();
    let absent = encode_reply(&Reply {
        request: query.bytes,
        snapshot: None,
    })
    .unwrap();
    client.accept(query.connection, &absent).unwrap();
    assert_eq!(client.stage(), ClientStage::Committed);
}
#[test]
fn retired_quote_cannot_be_silently_reopened_by_a_later_reply() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let old = client.quote().unwrap().clone();
    let cancel = client.cancel().unwrap();
    let result = dispatch(&mut session, &mut game, &grant, &cancel.bytes, None, 101).unwrap();
    client
        .accept(cancel.connection, result.reply.as_ref().unwrap())
        .unwrap();
    let query = client.poll().unwrap();
    let reopened = encode_reply(&Reply {
        request: query.bytes,
        snapshot: Some(old),
    })
    .unwrap();
    assert!(client.accept(query.connection, &reopened).is_err());
    assert_eq!(client.stage(), ClientStage::Cancelled);
}
#[test]
fn oversized_or_fabricated_nested_lengths_reject_before_server_work() {
    use bincode::Options;
    let (_, _, request, _) = setup();
    let mut bytes = quote_call(&request);
    let vector_size = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialized_size(&request.selections)
        .unwrap() as usize;
    let count_offset = bytes.len() - vector_size;
    bytes[count_offset..count_offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(decode_call(&bytes).is_err());
    assert!(decode_call(&vec![0; MAX_CALL_BYTES + 1]).is_err());
    assert!(decode_reply(&vec![0; MAX_REPLY_BYTES + 1]).is_err());
}
#[test]
fn zero_calls_duplicate_scope_and_excessive_selections_cannot_be_encoded() {
    let (_, _, request, _) = setup();
    let mut value = Call {
        binding: request.binding,
        call_id: 0,
        command: Command::Quote(request.clone()),
    };
    assert!(encode_call(&value).is_err());
    value.call_id = 1;
    value.binding.source_epoch += 1;
    assert!(encode_call(&value).is_err());
    let mut too_many = request;
    too_many.selections = vec![too_many.selections[0].clone(); MAX_BUILD_EDITS + 1];
    assert!(
        encode_call(&Call {
            binding: too_many.binding,
            call_id: 1,
            command: Command::Quote(too_many)
        })
        .is_err()
    );
}
#[test]
fn expiry_is_server_owned_and_delayed_consent_cannot_start_a_build() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let before = game.snapshot().unwrap();
    let confirm = client.confirm(client.quote().unwrap().consent).unwrap();
    assert!(dispatch(&mut session, &mut game, &grant, &confirm.bytes, None, 1100).is_err());
    assert_eq!(game.snapshot().unwrap(), before);
    client.disconnect().unwrap();
    client.reconnect().unwrap();
    let query = client.poll().unwrap();
    let reply = dispatch(&mut session, &mut game, &grant, &query.bytes, None, 1101).unwrap();
    client
        .accept(query.connection, reply.reply.as_ref().unwrap())
        .unwrap();
    assert_eq!(client.stage(), ClientStage::Expired);
}
#[test]
fn cancellation_after_admission_cannot_undo_the_durable_request() {
    let (mut game, grant, request, mut session) = setup();
    let snap = quote_at(&mut game, &grant, &request, &mut session);
    let confirm = call(
        &request,
        2,
        Command::Confirm {
            operation: 900,
            consent: snap.consent,
        },
    );
    let _admitted = dispatch(&mut session, &mut game, &grant, &confirm, None, 101).unwrap();
    let before = game.snapshot().unwrap();
    let cancel = call(
        &request,
        3,
        Command::Cancel {
            request_id: request.request_id,
            operation: 900,
        },
    );
    assert!(dispatch(&mut session, &mut game, &grant, &cancel, None, 102).is_err());
    assert_eq!(game.snapshot().unwrap(), before);
    assert!(game.sim().state().world.builds.pending_effect().is_some());
}
#[test]
fn another_actual_operation_with_the_same_number_but_different_quote_does_not_match() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    client_quote(&mut client, &mut game, &grant, &request, &mut session);
    let sent = client.confirm(client.quote().unwrap().consent).unwrap();
    let (mut other, mut other_grant, other_request, mut other_session) = setup();
    other_grant.authority.prices.floor = 11;
    let quote = quote_at(&mut other, &other_grant, &other_request, &mut other_session);
    let confirm = call(
        &other_request,
        2,
        Command::Confirm {
            operation: 900,
            consent: quote.consent,
        },
    );
    let _admitted = dispatch(
        &mut other_session,
        &mut other,
        &other_grant,
        &confirm,
        None,
        101,
    )
    .unwrap();
    complete(&mut other, DurableBuildStatus::Committed);
    assert!(
        client
            .observe(sent.connection, &other.projection())
            .is_err()
    );
    assert_eq!(client.stage(), ClientStage::Sending);
}
#[test]
fn invalid_reply_leaves_a_pending_client_unchanged_and_can_be_replaced_only_by_its_exact_reply() {
    let (mut game, grant, request, mut session) = setup();
    let mut client = client_for(&request);
    let prepared = client.begin(request).unwrap();
    let before = game.snapshot().unwrap();
    let handled = dispatch(
        &mut session,
        &mut game,
        &grant,
        &prepared.bytes,
        Some(900),
        100,
    )
    .unwrap();
    let reply = handled.reply.unwrap();
    for end in [0, 8, 15, 16, reply.len() - 1] {
        assert!(client.accept(prepared.connection, &reply[..end]).is_err());
    }
    assert_eq!(client.stage(), ClientStage::Quoting);
    assert!(client.waiting_for_reply());
    assert!(client.quote().is_none());
    assert_eq!(game.snapshot().unwrap(), before);
    client.accept(prepared.connection, &reply).unwrap();
    assert_eq!(client.stage(), ClientStage::Reviewing);
}
