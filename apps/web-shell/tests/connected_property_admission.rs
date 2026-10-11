use serde_json::json;
use wonderland_game_services::*;
use wonderland_web_shell::connected_adapter::{
    OperationStatus,
    state::{ConnectedState, LoadState, Panel, ReadSlot},
};

fn session(state: SessionState) -> SessionProjection {
    let ready = state == SessionState::LotReady;
    SessionProjection {
        epoch: 10,
        state,
        avatar_id: Some(42),
        shard_name: Some("Source city".into()),
        lot_location: ready.then_some(55),
        lot_incarnation: ready.then_some(9),
        capabilities: vec![],
    }
}
fn receive_session(state: &mut ConnectedState, session: SessionProjection) -> bool {
    state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: session.epoch,
            operation_id: None,
            event: GatewayEvent::Session { session },
        },
    )
}
fn visiting() -> (ConnectedState, String) {
    let mut state = ConnectedState::default();
    let epoch = state.ledger.begin_login();
    state.accept_account(epoch, session(SessionState::CityReady), vec![], vec![]);
    state.ledger.open_transport(epoch);
    state.panel = Some(Panel::Property);
    state.ledger.selected_lot = Some(1);
    state.reads.insert(
        "property".into(),
        ReadSlot {
            status: LoadState::Ready,
            result: Some(DirectoryResult {
                query: DirectoryQuery::LotByLocation {
                    shard_id: 7,
                    location: 55,
                },
                data: json!({"lot_id":1,"location":55}),
            }),
        },
    );
    let request = state
        .ledger
        .begin_operation("Enter property", None)
        .unwrap();
    state.sent.insert(
        request.operation_id.clone(),
        GatewayOperation::JoinLot {
            lot_location: 55,
            open_if_closed: false,
        },
    );
    state.wire_epochs.insert(request.operation_id.clone(), 10);
    receive_session(&mut state, session(SessionState::LotConnecting));
    (state, request.operation_id)
}

#[test]
fn only_matching_source_lot_readiness_reveals_the_visited_world() {
    let (mut state, id) = visiting();
    assert_eq!(state.panel, Some(Panel::Property));
    state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: Some(id.clone()),
            event: GatewayEvent::Pending {
                family: "find_lot".into(),
            },
        },
    );
    assert_eq!(state.panel, Some(Panel::Property));
    state.receive(
        state.ledger.epoch,
        GatewayEnvelope {
            epoch: 10,
            operation_id: Some(id),
            event: GatewayEvent::Outcome {
                status: OutcomeStatus::Accepted,
                source_code: Some(0),
                data: json!({}),
                error: None,
            },
        },
    );
    assert_eq!(state.panel, Some(Panel::Property));
    assert!(receive_session(&mut state, session(SessionState::LotReady)));
    assert_eq!(state.panel, None);
}

#[test]
fn rejected_and_invalid_or_unrelated_admissions_keep_the_property_visible() {
    let (mut state, id) = visiting();
    state.ledger.operations.get_mut(&id).unwrap().status =
        OperationStatus::Rejected("Lot is closed".into());
    receive_session(&mut state, session(SessionState::CityReady));
    assert_eq!(state.panel, Some(Panel::Property));
    for kind in 0..6 {
        let (mut state, id) = visiting();
        let mut ready = session(SessionState::LotReady);
        match kind {
            0 => ready.lot_location = None,
            1 => ready.lot_incarnation = Some(0),
            2 => ready.avatar_id = Some(43),
            3 => state.ledger.selected_lot = Some(2),
            4 => state.wire_epochs.insert(id, 9).map(|_| ()).unwrap_or(()),
            _ => ready.lot_location = Some(56),
        }
        receive_session(&mut state, ready);
        assert_eq!(
            state.panel,
            Some(Panel::Property),
            "unrelated admission case {kind}"
        );
    }
}

#[test]
fn duplicate_ready_updates_and_other_panels_are_not_closed() {
    let (mut state, _) = visiting();
    receive_session(&mut state, session(SessionState::LotReady));
    state.panel = Some(Panel::Property);
    receive_session(&mut state, session(SessionState::LotReady));
    assert_eq!(state.panel, Some(Panel::Property));
    let (mut state, _) = visiting();
    state.panel = Some(Panel::Chat);
    receive_session(&mut state, session(SessionState::LotReady));
    assert_eq!(state.panel, Some(Panel::Chat));
}

#[test]
fn a_new_source_epoch_or_previously_completed_admission_cannot_close_a_reopened_property() {
    let (mut state, _) = visiting();
    let mut changed = session(SessionState::LotReady);
    changed.epoch = 11;
    receive_session(&mut state, changed);
    assert_eq!(state.panel, Some(Panel::Property));
    let (mut state, id) = visiting();
    state.ledger.operations.get_mut(&id).unwrap().status =
        OperationStatus::Unknown("Interrupted".into());
    receive_session(&mut state, session(SessionState::LotReady));
    assert_eq!(state.panel, Some(Panel::Property));
}
