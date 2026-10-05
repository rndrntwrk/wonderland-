use wonderland_game_services::{GatewayOperation, protocol::encode_operation};
use wonderland_vm_protocol::{DecodeLimits, decode_direct_command};

#[test]
fn cancellation_is_original_source_seven_with_authenticated_actor_and_action_uid() {
    let operation: GatewayOperation = serde_json::from_value(serde_json::json!({
        "type":"cancel_interaction", "lot_incarnation":8, "action_uid":0x1234
    }))
    .unwrap();
    let encoded = encode_operation(&operation, 42, "Alice", "cancel-1").unwrap();
    assert!(encoded.lot);
    assert!(!encoded.has_response);
    assert_eq!(
        &encoded.bytes[18..],
        &[0, 0, 0, 7, 7, 42, 0, 0, 0, 0x34, 0x12]
    );
    let command = decode_direct_command(&encoded.bytes[22..], &DecodeLimits::default()).unwrap();
    assert_eq!(command.kind, 7);
    assert_eq!(command.actor_uid, Some(42));
}

#[test]
fn goto_preserves_explicit_source_menu_action_and_parameter_without_guessing_defaults() {
    let operation: GatewayOperation = serde_json::from_value(serde_json::json!({
        "type":"walk_to", "lot_incarnation":8, "interaction":25, "param0":-7,
        "x":160,"y":320,"level":2
    }))
    .unwrap();
    let encoded = encode_operation(&operation, 42, "Alice", "walk-1").unwrap();
    assert!(encoded.lot);
    assert!(!encoded.has_response);
    assert_eq!(
        &encoded.bytes[18..],
        &[
            0, 0, 0, 14, 10, 42, 0, 0, 0, 25, 0, 249, 255, 160, 0, 64, 1, 2
        ]
    );
    let command = decode_direct_command(&encoded.bytes[22..], &DecodeLimits::default()).unwrap();
    assert_eq!(command.kind, 10);
    assert_eq!(command.actor_uid, Some(42));
    assert!(
        serde_json::from_value::<GatewayOperation>(serde_json::json!({
            "type":"walk_to","lot_incarnation":8,"x":160,"y":320,"level":2
        }))
        .is_err()
    );
}
