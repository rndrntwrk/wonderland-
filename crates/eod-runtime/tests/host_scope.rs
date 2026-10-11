// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Independent-review regression and explicit scoped wire vectors.

use wonderland_eod_runtime::{protocol, registry::TIMER_PLUGIN, *};

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        (connection == ConnectionId(42)).then_some(ActorId(77))
    }
}

fn host(scope: u64, epoch: u64) -> NativeHost {
    NativeHost::new(
        HostIdentity {
            scope: HostScopeId(scope),
            epoch,
        },
        HostLimits::default(),
    )
    .unwrap()
}

fn timer(host: &mut NativeHost, invoker: u32) -> SessionTicket {
    let ticket = host
        .connect(
            &Auth,
            ConnectRequest {
                connection: ConnectionId(42),
                plugin: TIMER_PLUGIN,
                object: invoker,
                invoker: InvokerId(invoker),
                registers: TimerRegisters([0, 0, 1, 0]),
            },
        )
        .unwrap();
    host.take_public_events();
    ticket
}

fn message(ticket: SessionTicket) -> ClientMessage<'static> {
    ClientMessage {
        version: protocol::PROTOCOL_VERSION,
        ticket,
        plugin: TIMER_PLUGIN,
        sequence: 1,
        event: "Timer_Set",
        payload: WirePayload::Binary(&[99, 59]),
    }
}

#[test]
fn a_stale_ticket_from_another_host_scope_must_not_control_the_new_timer() {
    // Exact independent review scenario: one still-authenticated transport
    // moves scope 101 -> 202, with epoch/instance/generation all locally one.
    let mut first = host(101, 1);
    let first_ticket = timer(&mut first, 1001);
    first
        .take_private(&Auth, ConnectionId(42), first_ticket)
        .unwrap();
    let frame = protocol::encode(&message(first_ticket), &HostLimits::default()).unwrap();
    first
        .disconnect(&Auth, ConnectionId(42), first_ticket)
        .unwrap();

    let mut second = host(202, 1);
    let second_ticket = timer(&mut second, 2002);
    second
        .take_private(&Auth, ConnectionId(42), second_ticket)
        .unwrap();
    let result = second.receive_bytes(&Auth, ConnectionId(42), &frame);
    let events = second.take_public_events();
    assert!(
        result.is_err(),
        "stale first-scope frame returned {result:?} and emitted {events:?}"
    );
    assert_eq!(result, Err(Error::WrongScope));
    assert_ne!(first_ticket, second_ticket);
    assert_eq!(first_ticket.host_scope, HostScopeId(101));
    assert_eq!(second_ticket.host_scope, HostScopeId(202));
    assert!(events.is_empty());
    // The stale message must not consume the actual session's sequence one.
    second
        .receive(&Auth, ConnectionId(42), message(second_ticket))
        .unwrap();
    assert_eq!(
        second.take_public_events(),
        [PublicVmEvent::Timer {
            invoker: InvokerId(2002),
            event: TimerVmEvent::SetTime { packed_time: 25403 },
        }]
    );
}

#[test]
fn another_scopes_ticket_cannot_extract_private_outputs_or_disconnect() {
    let mut first = host(101, 1);
    let first_ticket = timer(&mut first, 1001);
    first
        .disconnect(&Auth, ConnectionId(42), first_ticket)
        .unwrap();
    let mut second = host(202, 1);
    let second_ticket = timer(&mut second, 2002);

    assert_eq!(
        second.take_private(&Auth, ConnectionId(42), first_ticket),
        Err(Error::WrongScope)
    );
    assert_eq!(
        second.disconnect(&Auth, ConnectionId(42), first_ticket),
        Err(Error::WrongScope)
    );
    assert_eq!(
        second.receive(&Auth, ConnectionId(42), message(first_ticket)),
        Err(Error::WrongScope)
    );
    let outputs = second
        .take_private(&Auth, ConnectionId(42), second_ticket)
        .unwrap();
    assert_eq!(outputs.len(), 2);
    assert!(
        outputs
            .iter()
            .all(|output| output.ticket() == second_ticket)
    );
    assert!(second.take_public_events().is_empty());
    second
        .disconnect(&Auth, ConnectionId(42), second_ticket)
        .unwrap();
    // The scope guard is also required for terminal outputs with no live session.
    assert_eq!(
        second.take_private(&Auth, ConnectionId(42), first_ticket),
        Err(Error::WrongScope)
    );
    let terminal = second
        .take_private(&Auth, ConnectionId(42), second_ticket)
        .unwrap();
    assert_eq!(terminal[0].ticket(), second_ticket);
    assert_eq!(terminal[0].event(), "eod_leave");
}

#[test]
fn binary_wire_vector_preserves_scope_epoch_and_all_payload_bytes() {
    let mut host = host(0x0102030405060708, 0x1112131415161718);
    let ticket = timer(&mut host, 1001);
    let input = ClientMessage {
        sequence: 0x2122232425262728,
        ..message(ticket)
    };
    let encoded = protocol::encode(&input, &HostLimits::default()).unwrap();
    // Explicit unreleased-v1 wire vector: EODI, version, scope, epoch,
    // instance, generation, plugin, sequence, kind, lengths, event, payload.
    let expected = [
        0x45, 0x4f, 0x44, 0x49, 0x01, 0x00, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x18,
        0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x9e, 0xfe, 0x65, 0xaa, 0x28, 0x27, 0x26,
        0x25, 0x24, 0x23, 0x22, 0x21, 0x01, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x54, 0x69, 0x6d,
        0x65, 0x72, 0x5f, 0x53, 0x65, 0x74, 0x63, 0x3b,
    ];
    assert_eq!(encoded, expected);
    let decoded = protocol::decode(&expected, &HostLimits::default()).unwrap();
    assert_eq!(decoded.ticket, ticket);
    assert_eq!(decoded.plugin, TIMER_PLUGIN);
    assert_eq!(decoded.sequence, input.sequence);
    assert_eq!(decoded.event, "Timer_Set");
    assert!(matches!(decoded.payload, WirePayload::Binary(&[99, 59])));
    assert_eq!(
        protocol::encode(&decoded, &HostLimits::default()).unwrap(),
        expected
    );
}

#[test]
fn text_wire_roundtrip_preserves_distinct_identity_fields_and_utf8() {
    let ticket = SessionTicket {
        host_scope: HostScopeId(0x8182838485868788),
        host_epoch: 0x9192939495969798,
        instance: InstanceId(0xA1A2A3A4A5A6A7A8),
        generation: 0xB1B2B3B4B5B6B7B8,
    };
    let input = ClientMessage {
        ticket,
        sequence: 0xC1C2C3C4C5C6C7C8,
        plugin: PluginId(0xD1D2D3D4),
        event: "Timer_Close",
        payload: WirePayload::Text("Olá, 雪\0"),
        version: protocol::PROTOCOL_VERSION,
    };
    let frame = protocol::encode(&input, &HostLimits::default()).unwrap();
    assert_eq!(
        &frame[6..14],
        &[0x88, 0x87, 0x86, 0x85, 0x84, 0x83, 0x82, 0x81]
    );
    assert_eq!(
        &frame[14..22],
        &[0x98, 0x97, 0x96, 0x95, 0x94, 0x93, 0x92, 0x91]
    );
    assert_eq!(
        &frame[22..30],
        &[0xA8, 0xA7, 0xA6, 0xA5, 0xA4, 0xA3, 0xA2, 0xA1]
    );
    assert_eq!(
        &frame[30..38],
        &[0xB8, 0xB7, 0xB6, 0xB5, 0xB4, 0xB3, 0xB2, 0xB1]
    );
    assert_eq!(&frame[38..42], &[0xD4, 0xD3, 0xD2, 0xD1]);
    assert_eq!(
        &frame[42..50],
        &[0xC8, 0xC7, 0xC6, 0xC5, 0xC4, 0xC3, 0xC2, 0xC1]
    );
    assert_eq!(frame[50], 0);
    let decoded = protocol::decode(&frame, &HostLimits::default()).unwrap();
    assert_eq!(decoded.ticket, ticket);
    assert_eq!(decoded.plugin, input.plugin);
    assert_eq!(decoded.sequence, input.sequence);
    assert_eq!(decoded.event, "Timer_Close");
    assert!(matches!(decoded.payload, WirePayload::Text("Olá, 雪\0")));
    assert_eq!(
        protocol::encode(&decoded, &HostLimits::default()).unwrap(),
        frame
    );
}

#[test]
fn scoped_header_counts_toward_frame_limit_and_unscoped_draft_is_rejected() {
    let mut host = host(101, 1);
    let ticket = timer(&mut host, 1001);
    let limits = HostLimits {
        max_message_bytes: 80,
        ..HostLimits::default()
    };
    let payload = [0, 255, 128, 1, 254, 127, 0, 255, 128, 1, 254, 127, 0, 255];
    let input = ClientMessage {
        payload: WirePayload::Binary(&payload),
        ..message(ticket)
    };
    let frame = protocol::encode(&input, &limits).unwrap();
    assert_eq!(frame.len(), 80); // 57-byte scoped header + 9-byte event + 14-byte body.
    let decoded = protocol::decode(&frame, &limits).unwrap();
    assert_eq!(decoded.ticket, ticket);
    assert!(matches!(decoded.payload, WirePayload::Binary(bytes) if bytes == payload));
    let longer = [0; 15];
    assert_eq!(
        protocol::encode(
            &ClientMessage {
                payload: WirePayload::Binary(&longer),
                ..input
            },
            &limits
        ),
        Err(Error::MessageTooLarge)
    );
    let mut too_long = frame.clone();
    too_long.push(0);
    assert!(matches!(
        protocol::decode(&too_long, &limits),
        Err(Error::MessageTooLarge)
    ));

    let valid = protocol::encode(&message(ticket), &HostLimits::default()).unwrap();
    let mut unscoped_draft = valid[..6].to_vec();
    unscoped_draft.extend_from_slice(&valid[14..]);
    assert!(protocol::decode(&unscoped_draft, &HostLimits::default()).is_err());
}

struct Store {
    bytes: Vec<u8>,
    key: Option<PrivateCheckpointKey>,
    reads: usize,
}
impl PrivateCheckpointStore for Store {
    fn write_private(&mut self, key: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError> {
        self.key = Some(key);
        self.bytes = bytes.to_vec();
        Ok(())
    }
    fn read_private(
        &mut self,
        key: PrivateCheckpointKey,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        self.reads += 1;
        if self.key != Some(key) {
            return Err(StoreError::Corrupt);
        }
        let size = self.bytes.len().min(destination.len());
        destination[..size].copy_from_slice(&self.bytes[..size]);
        Ok(PrivateRead {
            bytes_written: size,
            complete: size == self.bytes.len(),
        })
    }
}

#[test]
fn checkpoint_restore_and_rebind_require_the_same_scoped_instance_address() {
    let mut first = host(101, 1);
    let old_scope_ticket = timer(&mut first, 1001);
    let mut second = host(202, 1);
    let prior = timer(&mut second, 2002);
    second.take_private(&Auth, ConnectionId(42), prior).unwrap();
    let mut store = Store {
        bytes: vec![],
        key: None,
        reads: 0,
    };
    let stamp = second.checkpoint_to(&mut store).unwrap();

    assert!(matches!(
        NativeHost::restore_from(
            &mut store,
            HostIdentity {
                scope: HostScopeId(101),
                epoch: 2
            },
            stamp,
            HostLimits::default()
        ),
        Err(Error::CheckpointStampMismatch)
    ));
    assert_eq!(store.reads, 0); // A wrong destination scope is rejected before storage access.
    let mut restored = NativeHost::restore_from(
        &mut store,
        HostIdentity {
            scope: HostScopeId(202),
            epoch: 2,
        },
        stamp,
        HostLimits::default(),
    )
    .unwrap();
    assert_eq!(
        restored.rebind(
            &Auth,
            ConnectionId(42),
            old_scope_ticket.instance_address(),
            TimerRegisters([0, 0, 1, 0])
        ),
        Err(Error::WrongScope)
    );
    let current = restored
        .rebind(
            &Auth,
            ConnectionId(42),
            prior.instance_address(),
            TimerRegisters([0, 0, 1, 0]),
        )
        .unwrap();
    assert_eq!(current.host_scope, HostScopeId(202));
    assert_eq!(current.host_epoch, 2);
    assert_eq!(current.generation, prior.generation + 1); // Rejected rebind consumed no counter.
    let outputs = restored
        .take_private(&Auth, ConnectionId(42), current)
        .unwrap();
    assert!(outputs.iter().all(|output| output.ticket() == current));
    restored.disconnect_invoker(InvokerId(2002)).unwrap();
    let terminal = restored
        .take_private(&Auth, ConnectionId(42), current)
        .unwrap();
    assert_eq!(terminal[0].ticket(), current);
    assert_eq!(terminal[0].event(), "eod_leave");
}
