use wonderland_game_services::{protocol::*, *};

#[test]
fn find_lot_request_uses_location_not_database_identity_and_bool_byte() {
    let op = encode_operation(
        &GatewayOperation::JoinLot {
            lot_location: 0x000a0014,
            open_if_closed: false,
        },
        42,
        "Alice",
        "join-1",
    )
    .unwrap();
    assert_eq!(&op.bytes[12..], &[0, 5, 0, 0, 0, 11, 0, 10, 0, 20, 0]);
    assert_eq!(op.family, "find_lot");
}

#[test]
fn purchase_rejection_decodes_original_u16_enums_and_does_not_claim_acceptance() {
    let packet = Packet {
        channel: 1000,
        packet_type: 3,
        body: vec![0, 2, 0, 3, 0, 0, 0, 0, 0, 0, 0, 99],
    };
    match parse_packet(&packet).unwrap() {
        SourcePacket::Response {
            family,
            source_code,
            accepted,
            data,
        } => {
            assert_eq!(family, "purchase_lot");
            assert_eq!(source_code, 3);
            assert!(!accepted);
            assert_eq!(data["new_funds"], 99);
        }
        _ => panic!("expected purchase outcome"),
    }
}

#[test]
fn private_message_sender_is_session_actor_and_utf8_ack_is_correlated() {
    let op = encode_operation(
        &GatewayOperation::PrivateMessage {
            target_avatar_id: 70,
            message: "héllo".into(),
            color: 0x112233,
        },
        42,
        "Alice",
        "msg-9",
    )
    .unwrap();
    let (_, packets) = decode_frame(&op.bytes, 4096).unwrap();
    match parse_packet(&packets[0]).unwrap() {
        SourcePacket::Event { family, data, .. } => {
            assert_eq!(family, "instant_message");
            assert_eq!(data["from"], 42);
            assert_eq!(data["to"], 70);
            assert_eq!(data["message"], "héllo");
            assert_eq!(data["ack_id"], "msg-9");
        }
        _ => panic!("expected incoming message projection"),
    }
}

#[test]
fn character_creation_retains_independent_high_halves_of_source_keys() {
    let op = encode_operation(
        &GatewayOperation::CreateAvatar {
            name: "A".into(),
            description: "B".into(),
            gender: Gender::Female,
            skin: SkinTone::Dark,
            head_key: DecimalU64(0x123456780000000d),
            body_key: DecimalU64(0x89abcdef0000000d),
        },
        0,
        "",
        "cas-1",
    )
    .unwrap();
    assert_eq!(&op.bytes[12..18], &[0x27, 0x30, 0, 0, 0, 61]);
    assert_eq!(
        &op.bytes[18 + 37..],
        &[
            1, b'A', 1, b'B', 1, 2, 0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0, 0x89, 0xab, 0xcd, 0xef
        ]
    );
}

#[test]
fn server_issued_lot_ticket_is_private_and_packet_requires_all_fields() {
    let mut body = vec![0, 0, 0, 10, 0, 20, 32];
    body.extend(b"12345678901234567890123456789012");
    body.extend([10]);
    body.extend(b"localhost:");
    body.extend([2, b'4', b'2']);
    let packet = Packet {
        channel: 1000,
        packet_type: 6,
        body,
    };
    let value = parse_packet(&packet).unwrap();
    assert!(!format!("{value:?}").contains("12345678901234567890123456789012"));
    match value {
        SourcePacket::FindLot {
            status,
            lot_location,
            user,
            ..
        } => {
            assert_eq!(status, 0);
            assert_eq!(lot_location, 0x000a0014);
            assert_eq!(user, "42");
        }
        _ => panic!(),
    }
    let mut incomplete = packet;
    incomplete.body.pop();
    assert_eq!(
        parse_packet(&incomplete).unwrap_err().code,
        ErrorCode::InvalidResponse
    );
}

#[test]
fn oversized_vm_length_and_invalid_vlc_terminate_without_allocating() {
    let packet = Packet {
        channel: 1000,
        packet_type: 7,
        body: vec![127, 255, 255, 255],
    };
    assert_eq!(
        parse_packet(&packet).unwrap_err().code,
        ErrorCode::InvalidResponse
    );
    let packet = Packet {
        channel: 1000,
        packet_type: 6,
        body: vec![0, 0, 0, 0, 0, 1, 255, 255, 255, 255, 255, 255],
    };
    assert_eq!(
        parse_packet(&packet).unwrap_err().code,
        ErrorCode::InvalidResponse
    );
}

#[test]
fn delete_mail_and_retirement_have_no_fabricated_receipt() {
    for operation in [
        GatewayOperation::MailDelete { message_id: 42 },
        GatewayOperation::RetireAvatar,
    ] {
        assert!(
            !encode_operation(&operation, 42, "Alice", "delete-1")
                .unwrap()
                .has_response
        );
    }
}

#[test]
fn mailbox_and_bulletin_counts_are_checked_before_allocation() {
    for packet_type in [19, 24] {
        assert_eq!(
            parse_packet(&Packet {
                channel: 1000,
                packet_type,
                body: vec![0, 0, 127, 255, 255, 255]
            })
            .unwrap_err()
            .code,
            ErrorCode::InvalidResponse
        );
    }
}

#[test]
fn eod_submission_preserves_actor_plugin_and_original_binary_u16_length() {
    let command = GatewayOperation::Eod {
        incarnation: 1,
        plugin_id: 0x8b300068,
        event_name: "e".into(),
        text: None,
        binary: Some(vec![1, 2]),
    };
    let encoded = encode_operation(&command, 42, "Alice", "eod-1").unwrap();
    assert!(encoded.lot);
    assert!(!encoded.has_response);
    assert_eq!(
        &encoded.bytes[18..],
        &[
            0, 0, 0, 16, 18, 42, 0, 0, 0, 0x68, 0, 0x30, 0x8b, 1, b'e', 1, 2, 0, 1, 2
        ]
    );
    let malformed = GatewayOperation::Eod {
        incarnation: 1,
        plugin_id: 1,
        event_name: "e".into(),
        text: Some("text".into()),
        binary: Some(vec![1]),
    };
    assert!(encode_operation(&malformed, 42, "Alice", "eod-2").is_err());
}

#[test]
fn snapshot_refresh_uses_original_actorless_resync_command() {
    let bytes = request_world_snapshot(0x01020304).unwrap();
    assert_eq!(&bytes[18..], &[0, 0, 0, 9, 13, 0, 0, 0, 0, 4, 3, 2, 1]);
}

#[test]
fn original_roommate_poll_delivers_invitation_request_packets() {
    match parse_packet(&Packet {
        channel: 1000,
        packet_type: 12,
        body: vec![0, 0, 0, 0, 0, 42, 0, 10, 0, 20],
    })
    .unwrap()
    {
        SourcePacket::Event { family, data, .. } => {
            assert_eq!(family, "roommate_invitation");
            assert_eq!(data["avatar_id"], 42);
            assert_eq!(data["lot_location"], 0x000a0014);
            assert_eq!(data["action"], "invite");
        }
        _ => panic!("original POLL response must expose the actual incoming invitation"),
    }
}
