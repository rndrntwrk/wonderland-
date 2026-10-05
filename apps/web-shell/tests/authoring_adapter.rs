use wonderland_client_app::authoring::{
    AuthoringState, PreviewAuthoringProvider, preview_authoring_projection,
};
use wonderland_contracts::authoring::*;
use wonderland_web_shell::authoring_adapter::*;
#[test]
fn matching_commit_only_and_retained_commit_not_success() {
    let p = preview_authoring_projection();
    let mut state = AuthoringState::new(p.clone());
    let mut provider = PreviewAuthoringProvider::new(p).unwrap();
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName("New Sim".into()))
        .unwrap();
    let request = state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0);
    let event = provider.handle(&request);
    assert!(matches!(
        deliver(&mut state, &request, event.clone()).unwrap(),
        Delivery::Committed(_)
    ));
    assert_eq!(
        deliver(&mut state, &request, event).unwrap(),
        Delivery::Ignored
    );
}
#[test]
fn rejection_does_not_commit_provider_and_retry_can_commit() {
    let p = preview_authoring_projection();
    let mut state = AuthoringState::new(p.clone());
    let mut provider = PreviewAuthoringProvider::new(p.clone()).unwrap();
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName("Retry".into()))
        .unwrap();
    let req = state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0);
    assert!(matches!(
        deliver(&mut state, &req, preview_reply(&mut provider, &req, true)).unwrap(),
        Delivery::Rejected(_)
    ));
    assert_eq!(provider.snapshot(), &p);
    let req = state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0);
    assert!(matches!(
        deliver(&mut state, &req, preview_reply(&mut provider, &req, false)).unwrap(),
        Delivery::Committed(_)
    ));
}

#[test]
fn wrong_base_reply_and_old_error_do_not_classify_current_reply() {
    let p = preview_authoring_projection();
    let mut state = AuthoringState::new(p.clone());
    let mut provider = PreviewAuthoringProvider::new(p).unwrap();
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName("Fresh".into()))
        .unwrap();
    let req = state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0);
    let unrelated = AuthoringEvent::Rejected {
        operation_id: req.operation_id.clone(),
        base_revision: req.base_revision + 1,
        error: AuthoringError::PermissionDenied,
    };
    assert_eq!(
        deliver(&mut state, &req, unrelated).unwrap(),
        Delivery::Ignored
    );
    assert_eq!(state.pending(), Some(&req));
    assert!(matches!(
        deliver(&mut state, &req, preview_reply(&mut provider, &req, true)).unwrap(),
        Delivery::Rejected(_)
    ));
    let next = state
        .dispatch(AuthoringIntent::SubmitCreate)
        .unwrap()
        .remove(0);
    let event = provider.handle(&next);
    assert!(matches!(
        deliver(&mut state, &next, event).unwrap(),
        Delivery::Committed(_)
    ));
}

#[test]
fn travel_copy_follows_requested_destination() {
    use wonderland_contracts::{RequestKind, UiRequest};
    let shell = wonderland_client_app::ShellState::new(wonderland_client_app::preview_projection());
    for id in ["home", "harbor-cafe"] {
        let request = UiRequest {
            operation_id: "travel-test".into(),
            projection_revision: shell.projection.revision,
            kind: RequestKind::Travel {
                character_id: "maya".into(),
                place_id: id.into(),
            },
        };
        let name = &shell
            .projection
            .places
            .iter()
            .find(|p| p.id.as_ref() == id)
            .unwrap()
            .name;
        assert_eq!(
            travel_message(&shell, &request, true),
            format!("Visiting {name}…")
        );
        assert_eq!(
            travel_message(&shell, &request, false),
            format!("Welcome to {name}.")
        );
    }
}
