//! Controlled native source bytes, not injected browser events.
use wonderland_browser_gateway::lot_chat::LotChatGate;

fn string(value: &str) -> Vec<u8> {
    assert!(value.len() < 128);
    [vec![value.len() as u8], value.as_bytes().to_vec()].concat()
}
fn join(uid: u32, name: &str) -> Vec<u8> {
    [
        vec![0],
        uid.to_le_bytes().to_vec(),
        vec![0xee, 0xff],
        5i32.to_le_bytes().to_vec(),
        string(name),
        uid.to_le_bytes().to_vec(),
        vec![0; 24 + 1 + 8 + 16 + 6 + 86 + 12],
    ]
    .concat()
}
fn chat(uid: u32, message: &str) -> Vec<u8> {
    [
        vec![4],
        uid.to_le_bytes().to_vec(),
        string(message),
        vec![0],
    ]
    .concat()
}
fn tick(id: u32, commands: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend(1i32.to_le_bytes());
    bytes.extend(id.to_le_bytes());
    bytes.extend(0u64.to_le_bytes());
    bytes.extend((commands.len() as i32).to_le_bytes());
    for command in commands {
        bytes.extend(command);
    }
    bytes
}
fn seeded() -> LotChatGate {
    let mut gate = LotChatGate::new(42, 55, 9);
    gate.observe(
        false,
        include_bytes!("../../../crates/vm-protocol/tests/fixtures/source-v38-state-sync-tick.bin"),
    )
    .unwrap();
    gate.observe(false, &tick(43, &[join(42, "Alice"), join(99, "Bob")]))
        .unwrap();
    gate
}

#[test]
fn original_wire_chat_becomes_a_lot_and_actor_bound_sequenced_delivery() {
    let mut gate = seeded();
    let packet = tick(44, &[chat(99, "Hello from the controlled lot.")]);
    let first = gate.observe(false, &packet).unwrap().unwrap();
    assert_eq!(first.lot_incarnation, 9);
    assert_eq!(first.projection.viewer_id, 42);
    assert_eq!(first.projection.lot_location, 55);
    assert_eq!(first.projection.messages[0].sender_name, "Bob");
    assert_eq!(
        first.projection.messages[0].text,
        "Hello from the controlled lot."
    );
    assert!(
        gate.observe(false, &packet).unwrap().is_none(),
        "cached tick replay has no second chat delivery"
    );
    let next = gate
        .observe(false, &tick(45, &[chat(99, "Another message")]))
        .unwrap()
        .unwrap();
    assert!(next.sequence > first.sequence);
}

#[test]
fn equal_native_direct_messages_get_different_delivery_ids_and_reset_clears_identity() {
    let mut gate = seeded();
    let first = gate.observe(true, &chat(99, "Same")).unwrap().unwrap();
    let second = gate.observe(true, &chat(99, "Same")).unwrap().unwrap();
    assert!(second.sequence > first.sequence);
    gate.reset(42, 55, 10);
    let reset = gate
        .observe(true, &chat(99, "Old identity"))
        .unwrap()
        .unwrap();
    assert_eq!(reset.lot_incarnation, 10);
    assert!(!reset.projection.ready);
    assert!(reset.projection.messages.is_empty());
}

#[test]
fn invalid_source_input_publishes_lost_chat_authority_without_retaining_private_metadata() {
    let mut gate = seeded();
    let update = gate.observe(true, &[255]).unwrap().unwrap();
    assert!(!update.projection.ready);
    assert!(update.projection.channels.is_empty());
    assert!(update.projection.messages.is_empty());
    assert!(
        gate.observe(true, &chat(99, "Must remain hidden"))
            .unwrap()
            .is_none()
    );
}
