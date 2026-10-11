use serde_json::json;
use wonderland_game_services::{DirectoryQuery, DirectoryResult, ErrorCode, RosterEntry};
use wonderland_web_shell::connected_adapter::*;

fn connected() -> BrowserLedger {
    let mut state = BrowserLedger::default();
    let epoch = state.begin_login();
    assert!(state.accept_login(epoch));
    assert!(state.open_transport(epoch));
    state
}

#[test]
fn a_login_finishing_after_logout_cannot_restore_the_account() {
    let mut state = BrowserLedger::default();
    let first = state.begin_login();
    state.logout();
    assert!(!state.accept_login(first));
    assert!(!state.authenticated);
    assert_ne!(state.epoch, first);
}

#[test]
fn a_new_login_invalidates_an_earlier_login() {
    let mut state = BrowserLedger::default();
    let old = state.begin_login();
    let current = state.begin_login();
    assert!(!state.accept_login(old));
    assert!(state.accept_login(current));
}

#[test]
fn losing_the_socket_marks_unconfirmed_writes_unknown_without_replaying_them() {
    let mut state = connected();
    state
        .drafts
        .insert("mail".into(), "Meet at the park".into());
    state.selected_person = Some(42);
    let request = state.begin_operation("Send mail", Some("mail")).unwrap();
    assert!(state.close_transport(state.epoch, "Connection lost"));
    assert_eq!(
        state.operations[&request.operation_id].status,
        OperationStatus::Unknown("Connection lost".into())
    );
    assert_eq!(state.drafts["mail"], "Meet at the park");
    assert_eq!(state.selected_person, Some(42));
    assert!(state.begin_operation("Send mail", Some("mail")).is_err());
    assert!(state.open_transport(state.epoch));
    assert_eq!(state.operations.len(), 1);
    assert!(matches!(
        state.operations[&request.operation_id].status,
        OperationStatus::Unknown(_)
    ));
}

#[test]
fn a_rejected_send_retains_the_draft_and_unread_state() {
    let mut state = connected();
    state.drafts.insert("pm:42".into(), "Hello again".into());
    assert!(state.receive_message(state.epoch, "incoming-1", 42, false));
    let request = state.begin_operation("Message", Some("pm:42")).unwrap();
    assert!(state.receive_operation(
        &request,
        OperationStatus::Rejected("Player is offline".into())
    ));
    assert_eq!(state.drafts["pm:42"], "Hello again");
    assert_eq!(state.unread.get(&42), Some(&1));
}

#[test]
fn an_acknowledged_send_clears_only_the_exact_submitted_draft() {
    let mut state = connected();
    state.drafts.insert("chat".into(), "First message".into());
    let request = state.begin_operation("Chat", Some("chat")).unwrap();
    state.drafts.insert("chat".into(), "Second message".into());
    assert!(state.receive_operation(&request, OperationStatus::Accepted("Delivered".into())));
    assert_eq!(state.drafts["chat"], "Second message");
    let next = state.begin_operation("Chat", Some("chat")).unwrap();
    assert!(state.receive_operation(&next, OperationStatus::Accepted("Delivered".into())));
    assert_eq!(state.drafts["chat"], "");
}

#[test]
fn selecting_another_profile_discards_the_previous_profile_reply() {
    let mut state = connected();
    let old = state.begin_read("profile");
    let current = state.begin_read("profile");
    assert!(!state.finish_read("profile", &old));
    assert!(state.finish_read("profile", &current));
    assert!(!state.finish_read("profile", &current));
}

#[test]
fn logout_clears_private_state_and_ignores_prior_events() {
    let mut state = connected();
    state.drafts.insert("mail".into(), "Private draft".into());
    let request = state.begin_operation("Send", Some("mail")).unwrap();
    state.receive_message(state.epoch, "message-1", 9, false);
    state.selected_lot = Some(72);
    state.logout();
    assert!(state.drafts.is_empty());
    assert!(state.unread.is_empty());
    assert!(state.operations.is_empty());
    assert_eq!(state.selected_lot, None);
    assert!(!state.receive_operation(&request, OperationStatus::Accepted("Delivered".into())));
    assert!(!state.receive_message(request.epoch, "message-2", 9, false));
}

#[test]
fn repeated_messages_do_not_increment_unread_and_opening_a_conversation_is_local() {
    let mut state = connected();
    assert!(state.receive_message(state.epoch, "m-1", 9, false));
    assert!(!state.receive_message(state.epoch, "m-1", 9, false));
    assert!(state.receive_message(state.epoch, "m-2", 10, false));
    state.select_conversation(9);
    assert_eq!(state.unread.get(&9), Some(&0));
    assert_eq!(state.unread.get(&10), Some(&1));
}

#[test]
fn directory_pages_preserve_every_source_row_and_the_next_page_parameters() {
    let query = DirectoryQuery::AvatarPage {
        shard_id: 17,
        page: 2,
        per_page: 100,
    };
    let data = json!({"avatars": (100..200).map(|id|json!({"avatar_id":id})).collect::<Vec<_>>(), "page":2,"total_pages":9,"total_avatars":842});
    let page = DirectoryPage::from_result(&DirectoryResult {
        query: query.clone(),
        data,
    });
    assert_eq!(page.rows.len(), 100);
    assert_eq!(page.rows[99]["avatar_id"], 199);
    assert_eq!(page.total_items, Some(842));
    assert_eq!(page.total_pages, Some(9));
    assert_eq!(
        DirectoryPage::next_query(&query, 3),
        Some(DirectoryQuery::AvatarPage {
            shard_id: 17,
            page: 3,
            per_page: 100
        })
    );
}

#[test]
fn a_read_does_not_truncate_or_parse_a_response_over_the_transport_budget() {
    let bytes = vec![b' '; MAX_RESPONSE_BYTES + 1];
    assert_eq!(
        decode_bounded::<serde_json::Value>(&bytes)
            .unwrap_err()
            .code,
        ErrorCode::ResponseTooLarge
    );
}

#[test]
fn source_roster_ids_cross_the_browser_boundary_without_float_conversion_or_invented_hud() {
    let bytes = br#"{"avatar_id":42,"shard_name":"Source City","name":"Ada","description":"","head_key":"18446744073709551615","body_key":"9007199254740993","appearance":null,"home":null,"money":null,"motives":null}"#;
    let entry: RosterEntry = decode_bounded(bytes).unwrap();
    assert_eq!(entry.head_key.unwrap().0, u64::MAX);
    assert_eq!(entry.body_key.unwrap().0, 9_007_199_254_740_993);
    assert_eq!(entry.money, None);
    assert_eq!(entry.motives, None);
}

#[test]
fn a_maximum_raw_vm_frame_survives_its_real_json_integer_array_expansion() {
    use wonderland_game_services::{GatewayEnvelope, GatewayEvent};
    let original = GatewayEnvelope {
        epoch: u64::MAX,
        operation_id: None,
        event: GatewayEvent::VmFrame {
            lot_incarnation: u64::MAX,
            direct: true,
            data: vec![255; MAX_VM_FRAME_BYTES],
        },
    };
    let bytes = serde_json::to_vec(&original).unwrap();
    assert!(bytes.len() > 4 * 1024 * 1024);
    assert!(bytes.len() <= MAX_GATEWAY_ENVELOPE_BYTES);
    let decoded = decode_gateway_envelope(&bytes).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn raw_vm_frame_overflow_is_rejected_even_when_its_json_fits_the_envelope_budget() {
    use wonderland_game_services::{ErrorCode, GatewayEnvelope, GatewayEvent};
    let original = GatewayEnvelope {
        epoch: 1,
        operation_id: None,
        event: GatewayEvent::VmFrame {
            lot_incarnation: 1,
            direct: false,
            data: vec![0; MAX_VM_FRAME_BYTES + 1],
        },
    };
    let bytes = serde_json::to_vec(&original).unwrap();
    assert!(bytes.len() < MAX_GATEWAY_ENVELOPE_BYTES);
    assert_eq!(
        decode_gateway_envelope(&bytes).unwrap_err().code,
        ErrorCode::ResponseTooLarge
    );
}

#[test]
fn oversized_gateway_json_is_rejected_before_deserialization() {
    let bytes = vec![b' '; MAX_GATEWAY_ENVELOPE_BYTES + 1];
    assert_eq!(
        decode_gateway_envelope(&bytes).unwrap_err().code,
        wonderland_game_services::ErrorCode::ResponseTooLarge
    );
}

#[test]
fn an_original_http_response_between_four_and_eight_mib_is_not_artificially_rejected() {
    let text = "a".repeat(5 * 1024 * 1024);
    let bytes = serde_json::to_vec(&text).unwrap();
    let decoded: String = decode_bounded(&bytes).unwrap();
    assert_eq!(decoded.len(), text.len());
}
