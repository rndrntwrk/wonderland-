use wonderland_client_app::authoring::{AuthoringState, preview_authoring_projection};
use wonderland_contracts::authoring::*;
use wonderland_web_shell::persistence::*;
#[test]
fn bounded_acknowledged_roundtrip_excludes_draft_pending() {
    let snapshot = preview_authoring_projection();
    let mut state = AuthoringState::new(snapshot.clone());
    state.dispatch(AuthoringIntent::OpenCreate).unwrap();
    state
        .dispatch(AuthoringIntent::UpdateName("draft secret".into()))
        .unwrap();
    state.dispatch(AuthoringIntent::SubmitCreate).unwrap();
    let json = encode_snapshot(state.projection()).unwrap();
    assert!(!json.contains("draft secret"));
    assert!(!json.contains("operation_id"));
    assert_eq!(decode_snapshot(&json).unwrap(), snapshot);
}
#[test]
fn rejects_corrupt_version_invalid_and_oversize() {
    assert!(decode_snapshot("broken").is_err());
    let json = encode_snapshot(&preview_authoring_projection()).unwrap();
    assert!(decode_snapshot(&json.replacen("\"version\":1", "\"version\":9", 1)).is_err());
    assert!(decode_snapshot(&" ".repeat(MAX_AUTHORING_JSON_BYTES + 1)).is_err());
    let mut p = preview_authoring_projection();
    p.profiles[0].character.money = -1;
    assert!(encode_snapshot(&p).is_err());
}
