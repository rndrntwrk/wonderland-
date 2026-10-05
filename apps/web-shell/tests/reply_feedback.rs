use wonderland_client_app::ShellState;
use wonderland_contracts::*;
use wonderland_web_shell::{
    feedback::{ReplyFeedback, receive_reply},
    fixture::{Scenario, projection, reply},
};

fn cafe_state() -> ShellState {
    let mut state = ShellState::new(projection(Scenario::Accept));
    state
        .dispatch(UiIntent::SelectCharacter("maya".into()))
        .unwrap();
    state.dispatch(UiIntent::Play).unwrap();
    state
        .dispatch(UiIntent::SelectPlace("harbor-cafe".into()))
        .unwrap();
    let request = state.dispatch(UiIntent::Visit).unwrap().remove(0);
    state.receive(reply(&request, false)).unwrap();
    state
}

#[test]
fn accepted_outstanding_action_announces_its_reply_after_another_action_rejects() {
    let mut state = cafe_state();
    let object = state.projection.objects[0].clone();
    state
        .dispatch(UiIntent::SelectObject(object.target.clone()))
        .unwrap();
    let mut requests = Vec::new();
    for action in ["make-coffee", "inspect"] {
        requests.push(
            state
                .dispatch(UiIntent::TakeOffer {
                    target: object.target.clone(),
                    action_id: action.into(),
                    expected_revision: object.revision,
                })
                .unwrap()
                .remove(0),
        );
    }
    assert_eq!(state.pending_requests.len(), 2);

    let rejection = receive_reply(&mut state, &requests[0], reply(&requests[0], true));
    assert_eq!(
        rejection,
        ReplyFeedback::Error(UiError::Rejected(
            "The coffee machine is busy. Try again.".into()
        ))
    );
    let retained_error = state.last_error.clone();
    assert!(state.queue.is_empty());

    let acceptance = receive_reply(&mut state, &requests[1], reply(&requests[1], false));

    assert_eq!(
        state.last_error, retained_error,
        "Earlier reducer error must remain intact"
    );
    assert!(state.pending_requests.is_empty());
    assert_eq!(state.queue.len(), 1);
    assert_eq!(state.queue[0].action_id, ActionId::from("inspect"));
    assert_eq!(
        acceptance,
        ReplyFeedback::Announcement("Action accepted and added to the queue.")
    );
}

#[test]
fn feedback_reports_receive_errors_instead_of_announcing_success() {
    let mut state = cafe_state();
    let request = UiRequest {
        operation_id: "invalid-projection".into(),
        projection_revision: state.projection.revision,
        kind: RequestKind::Travel {
            character_id: "maya".into(),
            place_id: "harbor-cafe".into(),
        },
    };
    let mut invalid = state.projection.clone();
    invalid.version = 999;
    let feedback = receive_reply(
        &mut state,
        &request,
        UiEvent::ProjectionUpdated {
            projection: invalid,
        },
    );
    assert!(matches!(
        feedback,
        ReplyFeedback::Error(UiError::InvalidProjection(_))
    ));
}
