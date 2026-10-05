//! Golden wire bytes reproduce original BinaryWriter serializers; no server or
//! source fixture is provided by production code.
use wonderland_vm_protocol::*;
fn eod() -> Vec<u8> {
    vec![
        18, 7, 0, 0, 0, 1, 2, 3, 4, 9, b'e', b'o', b'd', b'_', b'e', b'n', b't', b'e', b'r', 0, 2,
        b'4', b'2',
    ]
}
fn list(commands: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0, 1, 0, 0, 0, 42, 0, 0, 0];
    b.extend(u64::MAX.to_le_bytes());
    b.extend((commands.len() as i32).to_le_bytes());
    for c in commands {
        b.extend(c);
    }
    b
}
#[test]
fn golden_eod_tick_preserves_actor_plugin_event_and_exact_u64_seed() {
    let b = list(&[eod()]);
    let decoded = decode_tick_list(&b, &DecodeLimits::default()).unwrap();
    assert_eq!(decoded.consumed, b.len());
    let tick = &decoded.ticks[0];
    assert_eq!(tick.tick_id, 42);
    assert_eq!(tick.random_seed, u64::MAX);
    let command = &tick.commands[0];
    assert_eq!(command.offset, 21);
    assert_eq!(command.consumed, eod().len());
    match &command.body {
        CommandBody::EodMessage(e) => {
            assert_eq!(e.actor_uid, 7);
            assert_eq!(e.plugin_id, 0x04030201);
            assert_eq!(e.event_name, "eod_enter");
            assert_eq!(e.payload, EodPayload::Text("42".into()));
        }
        _ => panic!("wrong body"),
    };
    assert!(serde_json::to_string(&decoded)
        .unwrap()
        .contains("18446744073709551615"));
}
#[test]
fn unknown_command_rejects_whole_tick_without_returning_prior_eod() {
    let b = list(&[eod(), vec![255], eod()]);
    assert_eq!(
        decode_tick_list(&b, &DecodeLimits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedCommand(255)
    );
}
#[test]
fn eod_direct_has_no_tick_header_and_requires_complete_input() {
    assert!(matches!(
        decode_direct_command(&eod(), &Default::default())
            .unwrap()
            .body,
        CommandBody::EodMessage(_)
    ));
    let mut b = eod();
    b.push(0);
    assert_eq!(
        decode_direct_command(&b, &Default::default())
            .unwrap_err()
            .kind,
        ErrorKind::Trailing
    );
}
#[test]
fn malformed_count_string_and_binary_payload_are_bounded() {
    let mut b = list(&[]);
    b[1..5].copy_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(
        decode_tick_list(&b, &Default::default()).unwrap_err().kind,
        ErrorKind::Invalid
    );
    let limits = DecodeLimits {
        max_string_bytes: 4,
        ..Default::default()
    };
    assert_eq!(
        decode_direct_command(&eod(), &limits).unwrap_err().kind,
        ErrorKind::Limit
    );
    let b = vec![18, 7, 0, 0, 0, 1, 0, 0, 0, 1, b'x', 1, 4, 0, 1];
    assert_eq!(
        decode_direct_command(&b, &Default::default())
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
}
fn text(s: &str) -> Vec<u8> {
    assert!(s.len() < 128);
    let mut b = vec![s.len() as u8];
    b.extend(s.as_bytes());
    b
}
#[test]
fn every_supported_non_snapshot_command_matches_original_serializer_extent() {
    let mut join = vec![0xee, 0xff];
    join.extend(5i32.to_le_bytes());
    join.extend(text("Sim"));
    join.extend(7u32.to_le_bytes());
    join.extend([0; 24]);
    join.push(3);
    join.extend([0; 8]);
    join.extend([0; 16]);
    join.extend([0; 6]);
    join.extend([0; 86]);
    join.extend([0; 12]);
    let mut arch = 1i32.to_le_bytes().to_vec();
    arch.push(5);
    arch.extend([0; 8]);
    arch.push(1);
    arch.extend([0; 12]);
    let mut blueprint = 8i32.to_le_bytes().to_vec();
    blueprint.extend(b"<house/>");
    blueprint.extend([0; 30]);
    let mut env = 1i32.to_le_bytes().to_vec();
    env.extend(123u32.to_le_bytes());
    env.extend(1i32.to_le_bytes());
    env.extend(456u32.to_le_bytes());
    let mut tune = 0i32.to_le_bytes().to_vec();
    tune.extend(1i32.to_le_bytes());
    tune.extend(text("city"));
    tune.extend(1i32.to_le_bytes());
    tune.extend(2i32.to_le_bytes());
    tune.extend(1i32.to_le_bytes());
    tune.extend(3i32.to_le_bytes());
    tune.extend(0.5f32.to_le_bytes());
    let mut inventory = 1i32.to_le_bytes().to_vec();
    inventory.extend(123u32.to_le_bytes());
    inventory.extend(text("source item"));
    inventory.extend([0; 26]);
    inventory.push(0);
    inventory.extend(1i32.to_le_bytes());
    inventory.extend(7i32.to_le_bytes());
    let mut place = vec![0; 4 + 2 + 2 + 1 + 1 + 4 + 4 + 1 + 1 + 4];
    place.extend(2i32.to_le_bytes());
    place.extend([1, 2]);
    place.push(1);
    place.extend(7i16.to_le_bytes());
    place.push(0);
    let mut batch = 2i32.to_le_bytes().to_vec();
    batch.extend([7, 0, 8, 0, 1, 2]);
    let mut chat_channel = vec![1];
    chat_channel.extend(text("General"));
    chat_channel.extend(text("Source"));
    chat_channel.extend([0, 0, 3, 255, 255, 255, 255]);
    let mut adjacent = 1i32.to_le_bytes().to_vec();
    adjacent.push(1);
    adjacent.extend(3i32.to_le_bytes());
    adjacent.extend([12, 18, 255]);
    let cases: Vec<(u8, Vec<u8>)> = vec![
        (0, join),
        (1, vec![0; 9]),
        (2, arch),
        (3, vec![0; 16]),
        (4, [text("Olá"), vec![0]].concat()),
        (5, blueprint),
        (6, vec![]),
        (7, vec![0; 2]),
        (8, vec![0; 8]),
        (9, vec![0; 9]),
        (10, vec![0; 9]),
        (11, [vec![0], text("Answer")].concat()),
        (13, vec![0; 8]),
        (14, [text("Title"), text("Message")].concat()),
        (
            15,
            [vec![7, 0, 0, 1], vec![0; 4], vec![1], vec![0; 20]].concat(),
        ),
        (16, vec![0; 10]),
        (17, vec![7, 0, 5, 0, 2, 1, 0, 2, 0]),
        (19, vec![0; 6]),
        (20, adjacent),
        (21, vec![0; 5]),
        (22, place),
        (23, inventory),
        (24, env),
        (25, vec![0; 2]),
        (26, vec![0; 3]),
        (27, vec![0; 8]),
        (28, vec![0; 5]),
        (29, vec![0; 2]),
        (30, vec![0; 3]),
        (31, vec![0; 5]),
        (
            32,
            [0.5f32.to_le_bytes().to_vec(), 16u32.to_le_bytes().to_vec()].concat(),
        ),
        (33, vec![0; 14]),
        (34, vec![0; 4]),
        (35, vec![0; 2]),
        (36, vec![0; 20]),
        (37, tune),
        (38, batch),
        (39, vec![0; 5]),
        (40, chat_channel),
        (41, vec![]),
        (42, vec![0; 9]),
        (43, vec![0; 7]),
        (44, vec![0; 26]),
        (45, vec![0; 1]),
        (46, vec![0; 51]),
        (47, vec![0; 8]),
        (
            48,
            [3i32.to_le_bytes().to_vec(), vec![12, 18, 255]].concat(),
        ),
    ];
    for (kind, payload) in cases {
        let mut c = vec![kind];
        if ![5, 13].contains(&kind) {
            c.extend(7u32.to_le_bytes());
        }
        c.extend(payload);
        let d = decode_direct_command(&c, &Default::default())
            .unwrap_or_else(|e| panic!("command {kind}: {e}"));
        assert_eq!(d.consumed, c.len(), "source extent of {kind}");
        assert_eq!(d.kind, kind);
        let mixed = list(&[c, eod()]);
        let decoded = decode_tick_list(&mixed, &Default::default())
            .unwrap_or_else(|e| panic!("mixed command {kind}: {e}"));
        assert!(
            matches!(
                decoded.ticks[0].commands[1].body,
                CommandBody::EodMessage(_)
            ),
            "next event after {kind}"
        );
    }
}
#[test]
fn all_truncated_command_prefixes_and_noncanonical_strings_reject() {
    let original = eod();
    for n in 0..original.len() {
        assert!(decode_direct_command(&original[..n], &Default::default()).is_err());
    }
    let mut b = original.clone();
    b.splice(9..10, [0x89, 0x00]);
    assert_eq!(
        decode_direct_command(&b, &Default::default())
            .unwrap_err()
            .kind,
        ErrorKind::Invalid
    );
    let mut b = original.clone();
    b.splice(9..10, [0xff, 0xff, 0xff, 0xff, 0xff]);
    assert_eq!(
        decode_direct_command(&b, &Default::default())
            .unwrap_err()
            .kind,
        ErrorKind::Invalid
    );
}
#[test]
fn decoded_binary_eod_bytes_are_payload_and_never_scanned_as_commands() {
    let mut b = vec![18];
    b.extend(7u32.to_le_bytes());
    b.extend(123u32.to_le_bytes());
    b.extend(text("eod_leave"));
    b.push(1);
    b.extend(4u16.to_le_bytes());
    b.extend([12, 18, 255, 0]);
    let command = decode_direct_command(&b, &Default::default()).unwrap();
    let CommandBody::EodMessage(message) = command.body else {
        panic!("wrong body")
    };
    assert_eq!(message.payload, EodPayload::Binary(vec![12, 18, 255, 0]));
}
