//! Reproduces VMNetChatCmd and its related BinaryWriter layouts at 4c6b3e8.
use wonderland_vm_protocol::{
    decode_direct_command, decode_tick_list, CommandBody, DecodeLimits, ErrorKind,
};

fn text(value: &str) -> Vec<u8> {
    let mut size = value.len();
    let mut bytes = Vec::new();
    while size >= 128 {
        bytes.push((size as u8 & 127) | 128);
        size >>= 7;
    }
    bytes.push(size as u8);
    bytes.extend(value.as_bytes());
    bytes
}

fn chat(actor: u32, message: &str, channel: u8) -> Vec<u8> {
    let mut bytes = vec![4];
    bytes.extend(actor.to_le_bytes());
    bytes.extend(text(message));
    bytes.push(channel);
    bytes
}

#[test]
fn chat_retains_source_actor_utf8_and_the_private_delivery_bit() {
    let bytes = chat(0xf1234567, "Olá 🐇 [color=red]literal[/color]", 0x83);
    let command = decode_direct_command(&bytes, &DecodeLimits::default()).unwrap();
    assert_eq!(command.actor_uid, Some(0xf1234567));
    assert_eq!(command.consumed, bytes.len());
    let CommandBody::Chat {
        message,
        channel_id,
    } = command.body
    else {
        panic!("chat must be a typed source command");
    };
    assert_eq!(message, "Olá 🐇 [color=red]literal[/color]");
    assert_eq!(channel_id, 0x83);
}

#[test]
fn a_later_malformed_command_cannot_release_an_earlier_chat_message() {
    let mut bytes = vec![0];
    bytes.extend(1i32.to_le_bytes());
    bytes.extend(44u32.to_le_bytes());
    bytes.extend(0u64.to_le_bytes());
    bytes.extend(2i32.to_le_bytes());
    bytes.extend(chat(42, "Must not escape", 0));
    bytes.push(255);
    assert_eq!(
        decode_tick_list(&bytes, &DecodeLimits::default())
            .unwrap_err()
            .kind,
        ErrorKind::UnsupportedCommand(255)
    );
}

#[test]
fn chat_prefixes_and_noncanonical_strings_are_rejected() {
    let bytes = chat(42, "Message", 0);
    for end in 0..bytes.len() {
        assert!(decode_direct_command(&bytes[..end], &DecodeLimits::default()).is_err());
    }
    let mut invalid = bytes.clone();
    invalid.splice(5..6, [0x87, 0]);
    assert_eq!(
        decode_direct_command(&invalid, &DecodeLimits::default())
            .unwrap_err()
            .kind,
        ErrorKind::Invalid
    );
}

#[test]
fn permission_ignore_color_and_channel_edits_retain_the_source_fields() {
    let mut permission = vec![16];
    permission.extend(42u32.to_le_bytes());
    permission.extend(99u32.to_le_bytes());
    permission.extend(18u32.to_le_bytes());
    permission.extend([3, 2]);
    assert!(matches!(
        decode_direct_command(&permission, &DecodeLimits::default())
            .unwrap()
            .body,
        CommandBody::ChangePermissions {
            target_uid: 99,
            replace_uid: 18,
            level: 3,
            mode: 2
        }
    ));
    let mut ignored = vec![31];
    ignored.extend(42u32.to_le_bytes());
    ignored.extend(99u32.to_le_bytes());
    ignored.push(1);
    assert!(matches!(
        decode_direct_command(&ignored, &DecodeLimits::default())
            .unwrap()
            .body,
        CommandBody::SetIgnore {
            target_uid: 99,
            ignore: true
        }
    ));
    let color = [
        vec![39],
        42u32.to_le_bytes().to_vec(),
        vec![0x80, 1, 2, 3, 4],
    ]
    .concat();
    assert!(matches!(
        decode_direct_command(&color, &DecodeLimits::default())
            .unwrap()
            .body,
        CommandBody::ChatParameters {
            pitch: -128,
            color: 0x04030201
        }
    ));
    let mut channel = [
        vec![40],
        42u32.to_le_bytes().to_vec(),
        vec![3],
        text("Roomies"),
        text("Private source channel"),
    ]
    .concat();
    channel.extend([1, 2, 3, 12, 34, 56, 78]);
    let CommandBody::ChatEditChannel(channel) =
        decode_direct_command(&channel, &DecodeLimits::default())
            .unwrap()
            .body
    else {
        panic!("channel must retain its source policy");
    };
    assert_eq!(
        (
            channel.id,
            channel.minimum_view_permission,
            channel.minimum_send_permission,
            channel.flags
        ),
        (3, 1, 2, 3)
    );
    assert_eq!(channel.name, "Roomies");
    assert_eq!(channel.text_color, 0x4e38220c);
}
