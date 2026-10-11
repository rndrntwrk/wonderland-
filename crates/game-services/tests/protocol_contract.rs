use wonderland_game_services::{ErrorCode, protocol::*};

#[test]
fn original_mixed_endian_frame_decodes_without_guessing_lengths() {
    let bytes = [
        0xe8, 3, 0, 0, 0, 0, 0, 0, 11, 0, 0, 0, 0, 5, 0, 0, 0, 11, 1, 2, 3, 4, 0,
    ];
    let (used, packets) = decode_frame(&bytes, 1024).unwrap();
    assert_eq!(used, 23);
    assert_eq!(
        packets,
        vec![Packet {
            channel: 1000,
            packet_type: 5,
            body: vec![1, 2, 3, 4, 0]
        }]
    );
    assert_eq!(encode_packet(1000, 5, &[1, 2, 3, 4, 0]).unwrap(), bytes);
}

#[test]
fn malformed_inner_lengths_and_huge_outer_length_reject_before_allocation() {
    for bytes in [
        vec![0xe8, 3, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 5, 0, 0, 0, 5],
        vec![0xe8, 3, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 5, 0, 0, 0, 7],
    ] {
        assert_eq!(
            decode_frame(&bytes, 1024).unwrap_err().code,
            ErrorCode::InvalidResponse
        );
    }
    assert_eq!(decode_frame(&[0; 11], 1024).unwrap().0, 0);
    let huge = [0xe8, 3, 0, 0, 0, 0, 0, 0, 255, 255, 255, 127];
    assert_eq!(
        decode_frame(&huge, 1024).unwrap_err().code,
        ErrorCode::ResponseTooLarge
    );
}

#[test]
fn challenge_reply_has_original_fixed_fields_and_ticket_only_in_password() {
    let bytes = session_response("42", "12345678901234567890123456789012").unwrap();
    assert_eq!(&bytes[..12], &[21, 0, 0, 0, 0, 0, 0, 0, 100, 1, 0, 0]);
    assert_eq!(&bytes[12..15], b"42\0");
    assert_eq!(bytes[12 + 318], 39);
    assert_eq!(&bytes[12 + 322..12 + 324], &[4, 0]);
    assert_eq!(&bytes[12 + 324..], b"12345678901234567890123456789012");
    assert!(session_response("42", &"x".repeat(33)).is_err());
}
