use wonderland_player_authoring::*;
fn place() -> Placement {
    Placement {
        x: 160,
        y: 320,
        level: 2,
        direction: 0x40,
    }
}
#[test]
fn buy_command_matches_original_type_actor_and_binarywriter_layout() {
    let actual = encode_client_command(
        0x01020304,
        &SourceCommand::Buy {
            guid: 0x11223344,
            placement: place(),
            value: -1,
            mode: 1,
            upgrade: 0,
        },
    )
    .unwrap();
    assert_eq!(
        actual,
        vec![
            3, 4, 3, 2, 1, 0x44, 0x33, 0x22, 0x11, 0xa0, 0, 0x40, 1, 2, 0x40, 255, 255, 255, 255,
            1, 0
        ]
    );
    assert!(validate_client_command(&actual, 0x01020304).is_ok());
    assert_eq!(
        validate_client_command(&actual, 99),
        Err(AuthoringError::WrongActor)
    );
}
#[test]
fn architecture_preserves_each_original_operation_and_exact_patterns() {
    for kind in 0..10 {
        let command = SourceCommand::Architecture(vec![ArchitectureCommand {
            kind,
            x: -2,
            y: 257,
            level: 3,
            x2: 70,
            y2: 6,
            pattern: 65535,
            style: 4096,
        }]);
        let bytes = encode_client_command(7, &command).unwrap();
        assert_eq!(bytes.len(), 31);
        assert_eq!(&bytes[5..10], &[1, 0, 0, 0, kind]);
        assert_eq!(decode_command_prefix(&bytes).unwrap(), (7, command, 31));
    }
}
#[test]
fn supported_player_commands_round_trip_without_server_outcomes() {
    let commands = [
        SourceCommand::Move {
            object_id: 123,
            placement: place(),
        },
        SourceCommand::Delete {
            object_id: 123,
            persist_id: 0,
            cleanup_all: true,
            success: false,
            mode: 2,
        },
        SourceCommand::SendToInventory {
            persist_id: 0xfedcba98,
            success: false,
        },
        SourceCommand::PlaceInventory {
            persist_id: 0xfedcba98,
            placement: place(),
            restore: InventoryRestore::default(),
            mode: 1,
        },
        SourceCommand::SetRoof {
            pitch: 0.75,
            style: 400,
        },
    ];
    for command in commands {
        let bytes = encode_client_command(12, &command).unwrap();
        let (actor, decoded, n) = decode_command_prefix(&bytes).unwrap();
        assert_eq!(actor, 12);
        assert_eq!(decoded, command);
        assert_eq!(n, bytes.len());
        assert!(validate_client_command(&bytes, 12).is_ok());
    }
}
#[test]
fn rejects_server_commands_truncation_trailing_bytes_and_forged_restore() {
    for tag in [0, 5, 12, 15, 19, 23, 26, 29, 33, 37] {
        let mut bytes = vec![tag];
        bytes.extend(1u32.to_le_bytes());
        assert!(validate_client_command(&bytes, 1).is_err());
    }
    let good = encode_client_command(
        1,
        &SourceCommand::Move {
            object_id: 2,
            placement: place(),
        },
    )
    .unwrap();
    for len in 0..good.len() {
        assert!(validate_client_command(&good[..len], 1).is_err());
    }
    let mut extra = good;
    extra.push(0);
    assert!(validate_client_command(&extra, 1).is_err());
    let forged = SourceCommand::PlaceInventory {
        persist_id: 2,
        placement: place(),
        restore: InventoryRestore {
            guid: 99,
            ..Default::default()
        },
        mode: 1,
    };
    assert!(encode_client_command(1, &forged).is_err());
}
