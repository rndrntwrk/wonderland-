use wonderland_client_app::ShellState;
use wonderland_contracts::*;
use wonderland_web_shell::fixture::{Scenario, projection, reply, should_reject};

#[test]
fn cancellation_echoes_its_own_identity_with_an_explicit_acknowledgment() {
    let request = UiRequest {
        operation_id: "ui-9".into(),
        projection_revision: 4,
        kind: RequestKind::Cancellation {
            queue_operation_id: "ui-2".into(),
        },
    };
    assert_eq!(
        reply(&request, false),
        UiEvent::CancellationAcknowledged {
            operation_id: "ui-9".into(),
            projection_revision: 4
        }
    );
}

#[test]
fn rejection_preserves_the_request_identity_and_cannot_enter_a_lot() {
    let mut state = ShellState::new(projection(Scenario::Accept));
    state
        .dispatch(UiIntent::SelectCharacter("maya".into()))
        .unwrap();
    state.dispatch(UiIntent::Play).unwrap();
    state
        .dispatch(UiIntent::SelectPlace("harbor-cafe".into()))
        .unwrap();
    let request = state.dispatch(UiIntent::Visit).unwrap().remove(0);
    assert!(should_reject(Scenario::RejectTravel, &request.kind));
    state.receive(reply(&request, true)).unwrap();
    assert_eq!(state.screen, Screen::City);
    assert!(state.pending_requests.is_empty());
    assert!(matches!(state.last_error, Some(UiError::Rejected(_))));
}

#[test]
fn empty_and_unavailable_preview_characters_keep_play_gated() {
    let empty = projection(Scenario::EmptyCharacters);
    assert!(empty.characters.is_empty());
    empty.validate().unwrap();
    let mut unavailable = ShellState::new(projection(Scenario::UnavailableCharacters));
    unavailable
        .dispatch(UiIntent::SelectCharacter("maya".into()))
        .unwrap();
    assert!(matches!(
        unavailable.dispatch(UiIntent::Play),
        Err(UiError::Unavailable(_))
    ));
}
