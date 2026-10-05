use wonderland_game_services::*;
use wonderland_web_shell::connected_adapter::{RequestStamp, state::ConnectedState};

const HOME_LOCATION: u32 = 0x005c0068;
fn projection(
    epoch: u64,
    state: SessionState,
    avatar_id: Option<u32>,
    shard: Option<&str>,
) -> SessionProjection {
    SessionProjection {
        epoch,
        state,
        avatar_id,
        shard_name: shard.map(str::to_owned),
        lot_location: None,
        lot_incarnation: None,
        capabilities: vec![],
    }
}
fn account() -> ConnectedState {
    let mut state = ConnectedState::default();
    let epoch = state.ledger.begin_login();
    let entry = RosterEntry {
        avatar_id: 42,
        shard_name: "Source city".into(),
        name: "Home Sim".into(),
        description: String::new(),
        head_key: None,
        body_key: None,
        appearance: None,
        home: Some(RosterHome {
            lot_id: 717,
            location: HOME_LOCATION,
            name: "Actual home".into(),
        }),
        money: None,
        motives: None,
    };
    state.accept_account(
        epoch,
        projection(10, SessionState::Authenticated, None, None),
        vec![entry],
        vec![],
    );
    state.ledger.open_transport(epoch);
    state
}
fn source_session(
    state: &mut ConnectedState,
    epoch: u64,
    phase: SessionState,
    avatar: Option<u32>,
    shard: Option<&str>,
) {
    state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch,
            operation_id: None,
            event: GatewayEvent::Session {
                session: projection(epoch, phase, avatar, shard),
            },
        },
    );
}
fn arm(state: &mut ConnectedState) -> RequestStamp {
    let request = state.home_request().unwrap();
    assert_eq!(
        request,
        GatewayOperation::ConnectCity {
            avatar_id: 42,
            shard_name: "Source city".into()
        }
    );
    let stamp = state.ledger.begin_operation("Go home city", None).unwrap();
    state.sent.insert(stamp.operation_id.clone(), request);
    state.wire_epochs.insert(stamp.operation_id.clone(), 10);
    assert!(state.arm_home_city(&stamp));
    stamp
}
fn outcome(state: &mut ConnectedState, operation_id: &str, status: OutcomeStatus) {
    state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: Some(operation_id.into()),
            event: GatewayEvent::Outcome {
                status,
                source_code: None,
                data: serde_json::Value::Null,
                error: None,
            },
        },
    );
}
fn home_join() -> GatewayOperation {
    GatewayOperation::JoinLot {
        lot_location: HOME_LOCATION,
        open_if_closed: false,
    }
}

#[test]
fn an_admitted_matching_city_uses_the_actual_home_location_directly() {
    let mut state = account();
    source_session(
        &mut state,
        10,
        SessionState::CityReady,
        Some(42),
        Some("Source city"),
    );
    assert_eq!(state.home_request().unwrap(), home_join());
    assert!(!state.has_home_intent());
}
#[test]
fn home_waits_for_the_exact_accepted_city_request_and_consumes_the_intent_once() {
    let mut state = account();
    let stamp = arm(&mut state);
    source_session(
        &mut state,
        10,
        SessionState::CityConnecting,
        Some(42),
        Some("Source city"),
    );
    assert_eq!(state.take_home_follow_up(), None);
    source_session(
        &mut state,
        10,
        SessionState::CityReady,
        Some(42),
        Some("Source city"),
    );
    assert_eq!(state.take_home_follow_up(), None);
    let other = state
        .ledger
        .begin_operation("Refresh roster", None)
        .unwrap();
    outcome(&mut state, &other.operation_id, OutcomeStatus::Accepted);
    assert_eq!(state.take_home_follow_up(), None);
    outcome(&mut state, &stamp.operation_id, OutcomeStatus::Accepted);
    assert_eq!(state.take_home_follow_up(), Some(home_join()));
    assert_eq!(state.take_home_follow_up(), None);
    assert!(!state.has_home_intent());
}
#[test]
fn an_early_acceptance_still_waits_for_matching_source_city_readiness() {
    let mut state = account();
    let stamp = arm(&mut state);
    outcome(&mut state, &stamp.operation_id, OutcomeStatus::Accepted);
    assert_eq!(state.take_home_follow_up(), None);
    source_session(
        &mut state,
        10,
        SessionState::CityReady,
        Some(42),
        Some("Source city"),
    );
    assert_eq!(state.take_home_follow_up(), Some(home_join()));
}
#[test]
fn rejected_or_unknown_admission_never_becomes_an_automatic_home_retry() {
    for status in [OutcomeStatus::Rejected, OutcomeStatus::Unknown] {
        let mut state = account();
        let stamp = arm(&mut state);
        outcome(&mut state, &stamp.operation_id, status);
        assert_eq!(state.take_home_follow_up(), None);
        assert!(!state.has_home_intent());
        source_session(
            &mut state,
            10,
            SessionState::CityReady,
            Some(42),
            Some("Source city"),
        );
        outcome(&mut state, &stamp.operation_id, OutcomeStatus::Accepted);
        assert_eq!(state.take_home_follow_up(), None);
    }
}
#[test]
fn switching_selection_or_source_home_cancels_the_captured_destination() {
    for change in 0..4 {
        let mut state = account();
        let stamp = arm(&mut state);
        match change {
            0 => state.selected_avatar = Some(99),
            1 => state.selected_shard = Some("Other city".into()),
            2 => state.roster[0].home.as_mut().unwrap().location += 1,
            _ => state.roster[0].home = None,
        }
        assert_eq!(state.take_home_follow_up(), None);
        assert!(!state.has_home_intent());
        source_session(
            &mut state,
            10,
            SessionState::CityReady,
            Some(42),
            Some("Source city"),
        );
        outcome(&mut state, &stamp.operation_id, OutcomeStatus::Accepted);
        assert_eq!(state.take_home_follow_up(), None);
    }
}
#[test]
fn a_mismatched_city_or_a_new_source_epoch_cannot_complete_the_old_intent() {
    for change in 0..3 {
        let mut state = account();
        let stamp = arm(&mut state);
        match change {
            0 => source_session(
                &mut state,
                10,
                SessionState::CityReady,
                Some(99),
                Some("Source city"),
            ),
            1 => source_session(
                &mut state,
                10,
                SessionState::CityReady,
                Some(42),
                Some("Other city"),
            ),
            _ => source_session(&mut state, 11, SessionState::Authenticated, None, None),
        }
        assert_eq!(state.take_home_follow_up(), None);
        assert!(!state.has_home_intent());
        source_session(
            &mut state,
            10,
            SessionState::CityReady,
            Some(42),
            Some("Source city"),
        );
        outcome(&mut state, &stamp.operation_id, OutcomeStatus::Accepted);
        assert_eq!(state.take_home_follow_up(), None);
    }
}
#[test]
fn disconnect_logout_or_an_explicit_other_action_cancels_home_intent() {
    for change in 0..4 {
        let mut state = account();
        let stamp = arm(&mut state);
        match change {
            0 => {
                state.ledger.close_transport(state.ledger.epoch, "Lost");
            }
            1 => {
                state.ledger.logout();
            }
            2 => state.cancel_home_intent(),
            _ => source_session(&mut state, 10, SessionState::Disconnected, None, None),
        }
        assert_eq!(state.take_home_follow_up(), None);
        assert!(!state.has_home_intent());
        source_session(
            &mut state,
            10,
            SessionState::CityReady,
            Some(42),
            Some("Source city"),
        );
        outcome(&mut state, &stamp.operation_id, OutcomeStatus::Accepted);
        assert_eq!(state.take_home_follow_up(), None);
    }
}
#[test]
fn no_supplied_home_or_no_connection_never_invents_a_lot_destination() {
    let mut state = account();
    state.roster[0].home = None;
    assert!(state.home_request().is_err());
    let mut state = account();
    state.roster[0].home.as_mut().unwrap().location = 0;
    assert!(state.home_request().is_err());
    let mut state = account();
    state.ledger.close_transport(state.ledger.epoch, "Lost");
    assert!(state.home_request().is_err());
    let mut state = account();
    source_session(
        &mut state,
        10,
        SessionState::CityReady,
        Some(99),
        Some("Other city"),
    );
    assert!(state.home_request().is_err());
}
