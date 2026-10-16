use wonderland_legacy_formats::{tuning_dat::decode, Limits};
fn text(out: &mut Vec<u8>, text: &str) {
    assert!(text.len() < 128);
    out.push(text.len() as u8);
    out.extend(text.bytes().map(|b| b.wrapping_add(13)));
}
fn envelope(payload: &[u8]) -> Vec<u8> {
    let mut commands = Vec::new();
    let mut position = 0;
    while payload.len() - position >= 4 {
        let length = ((payload.len() - position).min(112) / 4) * 4;
        commands.push(0xe0 + (length / 4 - 1) as u8);
        commands.extend_from_slice(&payload[position..position + length]);
        position += length;
    }
    commands.push(0xfc + (payload.len() - position) as u8);
    commands.extend_from_slice(&payload[position..]);
    let size = (commands.len() + 9) as u32;
    let mut out = vec![1];
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0xfb10_u16.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes()[1..]);
    out.extend(commands);
    out
}
fn payload() -> Vec<u8> {
    let mut bytes = 1_u32.to_le_bytes().to_vec();
    text(&mut bytes, "SimMotives");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    text(&mut bytes, "WakeHours");
    text(&mut bytes, "16");
    bytes
}
#[test]
fn source_persist_qfs_and_offset_ascii_strings() {
    let bytes = envelope(&payload());
    assert_eq!(
        decode(&bytes, &Limits::default()).unwrap()["simmotives"]["WakeHours"],
        "16"
    );
    let mut alternate = bytes.clone();
    alternate[0] = 3;
    assert_eq!(
        decode(&bytes, &Limits::default()),
        decode(&alternate, &Limits::default())
    );
}
#[test]
fn rejects_truncated_headers_sizes_counts_duplicate_keys_and_budgets() {
    let bytes = envelope(&payload());
    for len in 0..bytes.len() {
        assert!(decode(&bytes[..len], &Limits::default()).is_err());
    }
    let mut bad = bytes.clone();
    bad[1] ^= 1;
    assert!(decode(&bad, &Limits::default()).is_err());
    assert!(decode(
        &bytes,
        &Limits {
            max_entries: 1,
            ..Limits::default()
        }
    )
    .is_err());
    assert!(decode(
        &bytes,
        &Limits {
            max_string_bytes: 3,
            ..Limits::default()
        }
    )
    .is_err());
    assert!(decode(
        &bytes,
        &Limits {
            max_total_decoded_bytes: 100,
            ..Limits::default()
        }
    )
    .is_err());
    let mut p = payload();
    let count = 4 + 1 + "SimMotives".len();
    p[count..count + 4].copy_from_slice(&2u32.to_le_bytes());
    text(&mut p, "WakeHours");
    text(&mut p, "17");
    assert!(decode(&envelope(&p), &Limits::default()).is_err());
}
