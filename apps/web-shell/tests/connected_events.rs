use wonderland_game_services::*;
use wonderland_web_shell::connected_adapter::{OperationStatus, state::ConnectedState};

fn session(epoch: u64, state: SessionState) -> SessionProjection {
    SessionProjection {
        epoch,
        state,
        avatar_id: Some(42),
        shard_name: Some("Test city".into()),
        lot_location: None,
        lot_incarnation: None,
        capabilities: vec![],
    }
}

fn account() -> ConnectedState {
    let mut state = ConnectedState::default();
    let epoch = state.ledger.begin_login();
    assert!(state.accept_account(
        epoch,
        session(10, SessionState::Authenticated),
        vec![],
        vec![]
    ));
    state
}

#[test]
fn authentication_and_city_admission_are_distinct_from_transport_and_live_hud() {
    let mut state = account();
    assert!(!state.ledger.transport_ready);
    assert_eq!(
        state.session.as_ref().unwrap().state,
        SessionState::Authenticated
    );
    let epoch = state.ledger.epoch;
    assert!(state.receive(
        epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: None,
            event: GatewayEvent::Session {
                session: session(10, SessionState::CityConnecting)
            }
        }
    ));
    assert_eq!(
        state.session.as_ref().unwrap().state,
        SessionState::CityConnecting
    );
    assert!(state.roster.is_empty());
}

#[test]
fn an_old_upstream_session_cannot_replace_new_city_admission() {
    let mut state = account();
    let epoch = state.ledger.epoch;
    assert!(state.receive(
        epoch,
        GatewayEnvelope {
            epoch: 11,
            operation_id: None,
            event: GatewayEvent::Session {
                session: session(11, SessionState::CityReady)
            }
        }
    ));
    assert!(!state.receive(
        epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: None,
            event: GatewayEvent::Session {
                session: session(10, SessionState::Authenticated)
            }
        }
    ));
    assert_eq!(
        state.session.as_ref().unwrap().state,
        SessionState::CityReady
    );
}

#[test]
fn an_unknown_source_result_retains_the_draft_and_never_claims_delivery() {
    let mut state = account();
    state.ledger.open_transport(state.ledger.epoch);
    state.ledger.drafts.insert("mail".into(), "A draft".into());
    let request = state
        .ledger
        .begin_operation("Send mail", Some("mail"))
        .unwrap();
    let envelope = GatewayEnvelope {
        epoch: 10,
        operation_id: Some(request.operation_id.clone()),
        event: GatewayEvent::Outcome {
            status: OutcomeStatus::Unknown,
            source_code: None,
            data: serde_json::Value::Null,
            error: Some(ServiceError::new(
                ErrorCode::Timeout,
                "The server has not confirmed delivery.",
            )),
        },
    };
    assert!(state.receive(state.ledger.epoch, envelope));
    assert!(matches!(
        state.ledger.operations[&request.operation_id].status,
        OperationStatus::Unknown(_)
    ));
    assert_eq!(state.ledger.drafts["mail"], "A draft");
}

#[test]
fn an_authoritative_full_mail_poll_removes_deleted_mail_and_keeps_local_read_selection() {
    let mut state = account();
    state.ledger.open_transport(state.ledger.epoch);
    state.inbox.insert(7, serde_json::json!({"id":7}));
    state.read_mail.insert(8);
    state.selected_mail = Some(8);
    let request = state.ledger.begin_operation("Refresh inbox", None).unwrap();
    state.sent.insert(
        request.operation_id.clone(),
        GatewayOperation::MailPoll {
            since_ticks: DecimalU64(0),
        },
    );
    let data = serde_json::json!({"type":0,"messages":[{"id":8,"sender_id":55,"target_id":42,"subject":"Hello","body":"Meet me","sender_name":"Pat","time_ticks":"638950464000000000","type":0,"subtype":0,"read_state":0,"reply_id":null}]});
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: Some(request.operation_id),
            event: GatewayEvent::Outcome {
                status: OutcomeStatus::Accepted,
                source_code: Some(0),
                data,
                error: None
            }
        }
    ));
    assert_eq!(state.inbox.len(), 1);
    assert!(!state.inbox.contains_key(&7));
    assert_eq!(state.inbox[&8]["sender_name"], "Pat");
    assert!(state.read_mail.contains(&8));
    assert_eq!(state.selected_mail, Some(8));
}

#[test]
fn received_private_message_is_deduplicated_by_source_ack_identity() {
    let mut state = account();
    let data = serde_json::json!({"from_type":5,"from":55,"to":42,"type":0,"message":"Hello","ack_id":"source-42","reason":0,"color":0});
    let envelope = GatewayEnvelope {
        epoch: 10,
        operation_id: None,
        event: GatewayEvent::SourceEvent {
            family: "instant_message".into(),
            source_code: None,
            data,
        },
    };
    assert!(state.receive(state.ledger.epoch, envelope.clone()));
    assert!(!state.receive(state.ledger.epoch, envelope));
    assert_eq!(state.ledger.unread.get(&55), Some(&1));
    assert_eq!(state.events.len(), 1);
}

#[test]
fn source_disconnect_preserves_private_drafts_and_marks_pending_action_unknown() {
    let mut state = account();
    state.ledger.open_transport(state.ledger.epoch);
    state
        .ledger
        .drafts
        .insert("private:55".into(), "Keep this draft".into());
    state.ledger.selected_person = Some(55);
    state.ledger.unread.insert(55, 2);
    let pending = state
        .ledger
        .begin_operation("Private message", Some("private:55"))
        .unwrap();
    let mut disconnected = session(10, SessionState::Disconnected);
    disconnected.avatar_id = None;
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: None,
            event: GatewayEvent::Session {
                session: disconnected
            }
        }
    ));
    assert!(!state.ledger.transport_ready);
    assert_eq!(state.ledger.drafts["private:55"], "Keep this draft");
    assert_eq!(state.ledger.selected_person, Some(55));
    assert_eq!(state.ledger.unread.get(&55), Some(&2));
    assert!(matches!(
        state.ledger.operations[&pending.operation_id].status,
        OperationStatus::Unknown(_)
    ));
}

#[test]
fn another_avatar_cannot_inherit_private_mail_messages_or_drafts_after_reconnect() {
    let mut state = account();
    state
        .ledger
        .drafts
        .insert("private:55".into(), "For my first Sim".into());
    state.inbox.insert(7, serde_json::json!({"id":7}));
    state.ledger.selected_person = Some(55);
    let mut disconnected = session(10, SessionState::Disconnected);
    disconnected.avatar_id = None;
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: None,
            event: GatewayEvent::Session {
                session: disconnected
            }
        }
    ));
    let mut other = session(11, SessionState::CityReady);
    other.avatar_id = Some(99);
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 11,
            operation_id: None,
            event: GatewayEvent::Session { session: other }
        }
    ));
    assert!(state.ledger.drafts.is_empty());
    assert!(state.inbox.is_empty());
    assert_eq!(state.ledger.selected_person, None);
}

#[test]
fn only_source_invites_are_shown_and_a_rejected_reply_keeps_them_available() {
    let mut state = account();
    state.ledger.open_transport(state.ledger.epoch);
    let invitation = GatewayEnvelope {
        epoch: 10,
        operation_id: None,
        event: GatewayEvent::SourceEvent {
            family: "roommate_invitation".into(),
            source_code: Some(0),
            data: serde_json::json!({"action":"invite","avatar_id":55,"lot_location":0x003a0045u32}),
        },
    };
    assert!(state.receive(state.ledger.epoch, invitation.clone()));
    assert!(state.receive(state.ledger.epoch, invitation));
    assert_eq!(state.roommate_invitations.len(), 1);
    let rejected = state
        .ledger
        .begin_operation("Accept invitation", None)
        .unwrap();
    state.sent.insert(
        rejected.operation_id.clone(),
        GatewayOperation::Roommate {
            action: RoommateAction::Accept,
            avatar_id: 55,
            lot_location: 0x003a0045,
        },
    );
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: Some(rejected.operation_id),
            event: GatewayEvent::Outcome {
                status: OutcomeStatus::Rejected,
                source_code: Some(8),
                data: serde_json::Value::Null,
                error: None
            }
        }
    ));
    assert_eq!(state.roommate_invitations.len(), 1);
    let accepted = state
        .ledger
        .begin_operation("Accept invitation", None)
        .unwrap();
    state.sent.insert(
        accepted.operation_id.clone(),
        GatewayOperation::Roommate {
            action: RoommateAction::Accept,
            avatar_id: 55,
            lot_location: 0x003a0045,
        },
    );
    assert!(state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: Some(accepted.operation_id),
            event: GatewayEvent::Outcome {
                status: OutcomeStatus::Accepted,
                source_code: Some(9),
                data: serde_json::Value::Null,
                error: None
            }
        }
    ));
    assert!(state.roommate_invitations.is_empty());
}

#[test]
fn live_balances_and_needs_are_invalidated_when_the_native_lot_scope_changes() {
    for (next_state, next_incarnation, next_actor) in [
        (SessionState::CityReady, None, Some(42)),
        (SessionState::LotReady, Some(8), Some(42)),
        (SessionState::Disconnected, None, None),
        (SessionState::LotReady, Some(7), Some(99)),
    ] {
        let mut state = account();
        let mut current = session(10, SessionState::LotReady);
        current.lot_incarnation = Some(7);
        current.lot_location = Some(0x003a0045);
        state.receive(
            state.ledger.epoch,
            GatewayEnvelope {
                epoch: 10,
                operation_id: None,
                event: GatewayEvent::Session { session: current },
            },
        );
        state.roster.push(RosterEntry {
            avatar_id: 42,
            shard_name: "Test city".into(),
            name: "My Sim".into(),
            description: String::new(),
            head_key: None,
            body_key: None,
            appearance: None,
            home: None,
            money: Some(500),
            motives: Some([75; 8]),
        });
        let mut next = session(10, next_state);
        next.lot_incarnation = next_incarnation;
        next.avatar_id = next_actor;
        state.receive(
            state.ledger.epoch,
            GatewayEnvelope {
                epoch: 10,
                operation_id: None,
                event: GatewayEvent::Session { session: next },
            },
        );
        assert_eq!(state.roster[0].money, None);
        assert_eq!(state.roster[0].motives, None);
    }
}
