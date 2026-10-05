// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use std::collections::BTreeMap;
use wonderland_eod_runtime::{
    effects::*,
    protocol::{self, PROTOCOL_VERSION},
    registry::{self, TIMER_PLUGIN},
    *,
};

const ALICE: ActorId = ActorId(0xA11CE);
const BOB: ActorId = ActorId(0xB0B);
const A: ConnectionId = ConnectionId(11);
const B: ConnectionId = ConnectionId(22);
const RECONNECTED_A: ConnectionId = ConnectionId(33);

struct Authority(BTreeMap<ConnectionId, ActorId>);
impl ConnectionAuthority for Authority {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        self.0.get(&connection).copied()
    }
}
fn authority() -> Authority {
    Authority([(A, ALICE), (B, BOB), (RECONNECTED_A, ALICE)].into())
}

struct Registers(TimerRegisters);
impl RegisterSource for Registers {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        Some(self.0)
    }
}
struct MissingRegisters;
impl RegisterSource for MissingRegisters {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        None
    }
}
fn values() -> Registers {
    Registers(TimerRegisters([0, 1, 1, 2]))
}
fn identity(epoch: u64) -> HostIdentity {
    HostIdentity {
        scope: HostScopeId(712),
        epoch,
    }
}
fn limits() -> HostLimits {
    HostLimits {
        max_checkpoint_bytes: 4096,
        ..HostLimits::default()
    }
}
fn host() -> NativeHost {
    NativeHost::new(identity(7), limits()).unwrap()
}
fn request(connection: ConnectionId, invoker: u32, registers: [i16; 4]) -> ConnectRequest {
    ConnectRequest {
        connection,
        plugin: TIMER_PLUGIN,
        object: 900,
        invoker: InvokerId(invoker),
        registers: TimerRegisters(registers),
    }
}
fn connect(
    host: &mut NativeHost,
    auth: &Authority,
    connection: ConnectionId,
    invoker: u32,
) -> SessionTicket {
    host.connect(auth, request(connection, invoker, [0, 1, 1, 2]))
        .unwrap()
}
fn message<'a>(
    ticket: SessionTicket,
    sequence: u64,
    event: &'a str,
    payload: WirePayload<'a>,
) -> ClientMessage<'a> {
    ClientMessage {
        version: PROTOCOL_VERSION,
        ticket,
        plugin: TIMER_PLUGIN,
        sequence,
        event,
        payload,
    }
}
fn binary<'a>(
    ticket: SessionTicket,
    sequence: u64,
    event: &'a str,
    payload: &'a [u8],
) -> ClientMessage<'a> {
    message(ticket, sequence, event, WirePayload::Binary(payload))
}
fn drain(host: &mut NativeHost, auth: &Authority, connection: ConnectionId, ticket: SessionTicket) {
    host.take_public_events();
    host.take_private(auth, connection, ticket).unwrap();
}

#[test]
fn registry_is_exact_and_never_claims_original_runtime_verification() {
    let entries = registry::REGISTRATIONS;
    assert_eq!(entries.len(), 30);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.ui_type.is_some())
            .count(),
        28
    );
    assert!(entries.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(entries.iter().all(|entry| !entry.original_runtime_verified));
    assert_eq!(
        registry::lookup(PluginId(0x6D113845)).unwrap().ui_type,
        None
    );
    assert_eq!(
        registry::lookup(PluginId(0xCCC5BC43)).unwrap().ui_type,
        None
    );
    let unusual = registry::lookup(PluginId(0xEC55D705)).unwrap();
    assert_eq!(unusual.server_type, "VMEODDancePlatformPlugin");
    assert_eq!(unusual.ui_type, Some("UINCDanceFloorEOD"));
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.runtime == registry::RuntimeStatus::SourceTranslatedTimer)
            .count(),
        1
    );
    assert!(registry::lookup(PluginId(0)).is_none());
}

#[test]
fn all_unverified_registrations_and_unknown_ids_fail_closed() {
    let mut host = host();
    let auth = authority();
    for registration in registry::REGISTRATIONS
        .iter()
        .filter(|entry| entry.id != TIMER_PLUGIN)
    {
        let mut req = request(A, 100, [0, 0, 0, 0]);
        req.plugin = registration.id;
        assert_eq!(host.connect(&auth, req), Err(Error::UnverifiedPlugin));
        assert_eq!(
            registration.recovery,
            registry::RecoveryPolicy::AbortAndReconcileThroughProvider
        );
    }
    let mut unknown = request(A, 100, [0, 0, 0, 0]);
    unknown.plugin = PluginId(0xDEADBEEF);
    assert_eq!(host.connect(&auth, unknown), Err(Error::UnregisteredPlugin));
    assert!(host.take_public_events().is_empty());
}

#[test]
fn forged_session_cannot_send_or_extract_another_recipients_private_outputs() {
    let mut host = host();
    let auth = authority();
    let alice = connect(&mut host, &auth, A, 100);
    let bob = connect(&mut host, &auth, B, 200);
    assert_eq!(
        host.receive(&auth, A, binary(bob, 1, "Timer_Set", &[9, 1])),
        Err(Error::RecipientMismatch)
    );
    assert_eq!(
        host.take_private(&auth, A, bob),
        Err(Error::RecipientMismatch)
    );
    assert_eq!(
        host.take_private(&auth, RECONNECTED_A, alice),
        Err(Error::RecipientMismatch)
    );
    let bob_outputs = host.take_private(&auth, B, bob).unwrap();
    assert_eq!(
        bob_outputs
            .iter()
            .map(PrivateUiMessage::event)
            .collect::<Vec<_>>(),
        ["eod_enter", "Timer_Show"]
    );
    assert!(
        bob_outputs
            .iter()
            .all(|output| output.ticket() == bob && output.plugin() == TIMER_PLUGIN)
    );
    assert_eq!(host.take_public_events().len(), 2); // Only the original connect events.
    host.receive(&auth, A, binary(alice, 1, "Timer_Set", &[9, 1]))
        .unwrap();
    assert_eq!(
        host.take_public_events(),
        [PublicVmEvent::Timer {
            invoker: InvokerId(100),
            event: TimerVmEvent::SetTime { packed_time: 2305 }
        }]
    );
}

#[test]
fn authentication_is_required_and_revocation_invalidates_even_known_tickets() {
    let mut host = host();
    let mut auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    auth.0.remove(&A);
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 1, "Timer_Set", &[1, 2])),
        Err(Error::Unauthenticated)
    );
    assert_eq!(
        host.take_private(&auth, A, ticket),
        Err(Error::Unauthenticated)
    );
    host.tick(&auth, &MissingRegisters).unwrap();
    assert_eq!(
        host.take_public_events(),
        [PublicVmEvent::Disconnected {
            invoker: InvokerId(100)
        }]
    );
    auth.0.insert(A, ALICE);
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 1, "Timer_Set", &[1, 2])),
        Err(Error::StaleSession)
    );
    assert_eq!(
        host.take_private(&auth, A, ticket),
        Err(Error::StaleSession)
    );
}

#[test]
fn reassigned_transport_cannot_inherit_private_queue() {
    let mut host = host();
    let mut auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    auth.0.insert(A, BOB); // Deliberately adversarial provider change.
    assert_eq!(
        host.take_private(&auth, A, ticket),
        Err(Error::RecipientMismatch)
    );
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 1, "Timer_Set", &[1, 2])),
        Err(Error::RecipientMismatch)
    );
    host.tick(&auth, &MissingRegisters).unwrap();
    assert_eq!(
        host.take_private(&auth, A, ticket),
        Err(Error::StaleSession)
    );
}

#[test]
fn stale_generations_wrong_epoch_wrong_plugin_and_sequence_replay_are_rejected() {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    let valid = binary(ticket, 1, "Timer_Set", &[2, 3]);
    let mut bad = valid;
    bad.ticket.host_epoch += 1;
    assert_eq!(host.receive(&auth, A, bad), Err(Error::WrongEpoch));
    bad = valid;
    bad.ticket.generation += 1;
    assert_eq!(host.receive(&auth, A, bad), Err(Error::StaleSession));
    bad = valid;
    bad.plugin = PluginId(0xCB2819CB);
    assert_eq!(host.receive(&auth, A, bad), Err(Error::WrongPlugin));
    bad = valid;
    bad.sequence = 2;
    assert_eq!(host.receive(&auth, A, bad), Err(Error::UnexpectedSequence));
    host.receive(&auth, A, valid).unwrap();
    assert_eq!(
        host.receive(&auth, A, valid),
        Err(Error::UnexpectedSequence)
    );
    assert_eq!(host.take_public_events().len(), 1);
    host.disconnect(&auth, A, ticket).unwrap();
    let next = connect(&mut host, &auth, A, 100);
    assert_ne!(next, ticket);
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 2, "Timer_Set", &[1, 2])),
        Err(Error::StaleSession)
    );
}

#[test]
fn event_allowlist_includes_payload_kind_and_excludes_host_control_events() {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    for event in [
        "eod_enter",
        "eod_leave",
        "Timer_Show",
        "Timer_Off",
        "Timer_Update",
        "SetTime",
        "unknown",
    ] {
        assert_eq!(
            host.receive(&auth, A, binary(ticket, 1, event, &[1])),
            Err(Error::EventNotAllowed)
        );
    }
    assert_eq!(
        host.receive(
            &auth,
            A,
            message(ticket, 1, "Timer_Set", WirePayload::Text("1,2"))
        ),
        Err(Error::EventNotAllowed)
    );
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 1, "Timer_Close", &[])),
        Err(Error::EventNotAllowed)
    );
    assert!(host.take_public_events().is_empty());
    assert_eq!(
        host.receive(
            &auth,
            A,
            message(
                ticket,
                1,
                "Timer_Close",
                WirePayload::Text("ignored source body")
            )
        ),
        Ok(DispatchOutcome::Closed)
    );
    assert_eq!(
        host.take_private(&auth, A, ticket).unwrap()[0].event(),
        "eod_leave"
    );
    assert_eq!(host.disconnect(&auth, A, ticket), Err(Error::StaleSession));
}

#[test]
fn frame_version_size_lengths_utf8_and_forged_verified_or_recipient_fields_are_rejected() {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    let valid = protocol::encode(&binary(ticket, 1, "Timer_Set", &[1, 2]), &limits()).unwrap();
    let mut version = valid.clone();
    version[4..6].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        host.receive_bytes(&auth, A, &version),
        Err(Error::UnsupportedProtocolVersion)
    );
    let mut verified = valid.clone();
    verified[50] = 0x80;
    assert_eq!(
        host.receive_bytes(&auth, A, &verified),
        Err(Error::InvalidMessage)
    );
    let mut recipient = valid.clone();
    recipient.extend_from_slice(&BOB.0.to_le_bytes());
    assert_eq!(
        host.receive_bytes(&auth, A, &recipient),
        Err(Error::InvalidMessage)
    );
    let mut length_bomb = valid.clone();
    length_bomb[53..57].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        host.receive_bytes(&auth, A, &length_bomb),
        Err(Error::InvalidMessage)
    );
    let mut event_bomb = valid.clone();
    event_bomb[51..53].copy_from_slice(&u16::MAX.to_le_bytes());
    assert_eq!(
        host.receive_bytes(&auth, A, &event_bomb),
        Err(Error::MessageTooLarge)
    );
    let mut invalid_utf8 = valid.clone();
    invalid_utf8[50] = 0;
    *invalid_utf8.last_mut().unwrap() = 255;
    assert_eq!(
        host.receive_bytes(&auth, A, &invalid_utf8),
        Err(Error::InvalidMessage)
    );
    assert_eq!(
        host.receive_bytes(&auth, A, &vec![0; limits().max_message_bytes + 1]),
        Err(Error::MessageTooLarge)
    );
    for end in 0..valid.len() {
        assert!(
            protocol::decode(&valid[..end], &limits()).is_err(),
            "accepted truncated frame ending at {end}"
        );
    }
    let payload = vec![0; limits().max_message_bytes];
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 1, "Timer_Set", &payload)),
        Err(Error::MessageTooLarge)
    );
    assert_eq!(
        host.receive_bytes(&auth, A, &valid),
        Ok(DispatchOutcome::Accepted)
    );
    assert_eq!(host.take_public_events().len(), 1);
}

#[test]
fn source_invalid_timer_payloads_are_noops_and_do_not_poison_the_session() {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    for (index, (event, payload)) in [
        ("Timer_Set", vec![]),
        ("Timer_Set", vec![1]),
        ("Timer_Set", vec![100, 59]),
        ("Timer_Set", vec![99, 60]),
        ("Timer_State_Change", vec![2]),
        ("Timer_IsRunning_Change", vec![]),
        ("Timer_IsRunning_Change", vec![255]),
    ]
    .iter()
    .enumerate()
    {
        host.receive(&auth, A, binary(ticket, index as u64 + 1, event, payload))
            .unwrap();
    }
    assert!(host.take_public_events().is_empty());
    host.receive(&auth, A, binary(ticket, 8, "Timer_Set", &[99, 59, 255]))
        .unwrap();
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(100), 2, vec![25403])
    );
}

#[test]
fn per_tick_rate_limit_is_atomic_and_resets_only_on_authoritative_tick() {
    let auth = authority();
    let mut host = NativeHost::new(
        identity(7),
        HostLimits {
            max_messages_per_tick: 1,
            ..limits()
        },
    )
    .unwrap();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    host.receive(&auth, A, binary(ticket, 1, "Timer_Set", &[1, 2]))
        .unwrap();
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 2, "Timer_Set", &[3, 4])),
        Err(Error::RateLimited)
    );
    host.take_public_events();
    host.tick(&auth, &values()).unwrap();
    host.receive(&auth, A, binary(ticket, 2, "Timer_Set", &[3, 4]))
        .unwrap();
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(100), 2, vec![772])
    );
}

#[test]
fn participants_instances_and_timers_are_bounded_without_counter_consumption() {
    let auth = authority();
    for (configured, expected) in [
        (
            HostLimits {
                max_participants: 1,
                ..limits()
            },
            Error::ParticipantLimit,
        ),
        (
            HostLimits {
                max_instances: 1,
                ..limits()
            },
            Error::InstanceLimit,
        ),
        (
            HostLimits {
                max_timers: 1,
                ..limits()
            },
            Error::TimerLimit,
        ),
    ] {
        let mut host = NativeHost::new(identity(7), configured).unwrap();
        let first = connect(&mut host, &auth, A, 100);
        assert_eq!(
            host.connect(&auth, request(B, 200, [0, 0, 0, 0])),
            Err(expected)
        );
        assert_eq!(
            host.connect(&auth, request(RECONNECTED_A, 201, [0, 0, 0, 0])),
            Err(Error::ParticipantAlreadyConnected)
        );
        assert_eq!(
            host.connect(&auth, request(B, 100, [0, 0, 0, 0])),
            Err(Error::ParticipantAlreadyConnected)
        );
        host.disconnect(&auth, A, first).unwrap();
        let second = connect(&mut host, &auth, B, 200);
        assert_eq!(second.instance, InstanceId(2));
        assert_eq!(second.generation, 2);
    }
}

#[test]
fn queue_backpressure_does_not_apply_a_message_or_consume_its_sequence() {
    let auth = authority();
    let mut host = NativeHost::new(
        identity(7),
        HostLimits {
            max_public_events: 1,
            max_private_messages: 2,
            ..limits()
        },
    )
    .unwrap();
    let ticket = connect(&mut host, &auth, A, 100);
    assert_eq!(
        host.receive(&auth, A, binary(ticket, 1, "Timer_IsRunning_Change", &[1])),
        Err(Error::QueueFull)
    );
    assert_eq!(
        host.take_public_events(),
        [PublicVmEvent::Connected {
            invoker: InvokerId(100)
        }]
    );
    host.receive(&auth, A, binary(ticket, 1, "Timer_IsRunning_Change", &[1]))
        .unwrap();
    assert_eq!(host.take_public_events()[0].source_event().1, 3);
    assert_eq!(
        host.connect(&auth, request(B, 200, [0, 0, 0, 0])),
        Err(Error::QueueFull)
    );
    host.take_private(&auth, A, ticket).unwrap();
    let next = connect(&mut host, &auth, B, 200);
    assert_eq!(next.instance, InstanceId(2));
}

#[test]
fn private_byte_cap_and_failed_tick_are_atomic() {
    let auth = authority();
    let mut host = NativeHost::new(
        identity(7),
        HostLimits {
            max_private_output_bytes: 32,
            ..limits()
        },
    )
    .unwrap();
    let ticket = host.connect(&auth, request(A, 100, [1, 0, 0, 1])).unwrap();
    host.take_public_events();
    assert_eq!(
        host.tick(&auth, &Registers(TimerRegisters([1, 0, 0, 0]))),
        Err(Error::QueueFull)
    );
    assert_eq!(host.public_projection().tick, 0);
    host.take_private(&auth, A, ticket).unwrap();
    host.tick(&auth, &Registers(TimerRegisters([1, 0, 0, 0])))
        .unwrap();
    let output = host.take_private(&auth, A, ticket).unwrap();
    assert_eq!(
        output
            .iter()
            .map(PrivateUiMessage::event)
            .collect::<Vec<_>>(),
        ["Timer_Off", "Timer_Update"]
    );
    assert_eq!(output[1].body(), UiBody::Text("0:0"));
    assert_eq!(host.public_projection().tick, 1);
}

#[test]
fn missing_vm_input_rolls_back_all_timer_and_tick_changes() {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    assert_eq!(
        host.tick(&auth, &MissingRegisters),
        Err(Error::MissingRegisters)
    );
    assert_eq!(host.public_projection().tick, 0);
    host.tick(&auth, &values()).unwrap();
    assert_eq!(host.public_projection().tick, 1);
}

#[test]
fn deterministic_idle_timeout_has_exact_tick_and_instance_order() {
    let auth = authority();
    let configured = HostLimits {
        idle_timeout_ticks: 3,
        ..limits()
    };
    let mut host = NativeHost::new(identity(7), configured).unwrap();
    let first = connect(&mut host, &auth, A, 200);
    let second = connect(&mut host, &auth, B, 100);
    drain(&mut host, &auth, A, first);
    host.take_private(&auth, B, second).unwrap();
    for tick in 1..3 {
        host.tick(&auth, &values()).unwrap();
        assert_eq!(host.public_projection().tick, tick);
        assert!(host.take_public_events().is_empty());
    }
    host.tick(&auth, &MissingRegisters).unwrap();
    assert_eq!(
        host.take_public_events(),
        [
            PublicVmEvent::Disconnected {
                invoker: InvokerId(200)
            },
            PublicVmEvent::Disconnected {
                invoker: InvokerId(100)
            }
        ]
    );
    assert_eq!(
        host.take_private(&auth, A, first).unwrap()[0].event(),
        "eod_leave"
    );
    assert_eq!(
        host.take_private(&auth, B, second).unwrap()[0].event(),
        "eod_leave"
    );
    assert_eq!(
        host.receive(&auth, A, binary(first, 1, "Timer_Set", &[1, 2])),
        Err(Error::StaleSession)
    );
}

struct MemoryPrivateStore {
    bytes: Vec<u8>,
    fail_write: bool,
    impossible_read_count: bool,
}
impl MemoryPrivateStore {
    fn empty() -> Self {
        Self {
            bytes: vec![],
            fail_write: false,
            impossible_read_count: false,
        }
    }
}
impl PrivateCheckpointStore for MemoryPrivateStore {
    fn write_private(&mut self, _: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError> {
        if self.fail_write {
            return Err(StoreError::Unavailable);
        }
        self.bytes = bytes.to_vec();
        Ok(())
    }
    fn read_private(
        &mut self,
        _: PrivateCheckpointKey,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        let count = destination.len().min(self.bytes.len());
        destination[..count].copy_from_slice(&self.bytes[..count]);
        Ok(PrivateRead {
            bytes_written: if self.impossible_read_count {
                usize::MAX
            } else {
                count
            },
            complete: self.bytes.len() <= destination.len(),
        })
    }
}
fn saved() -> (MemoryPrivateStore, CheckpointStamp, SessionTicket) {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, ticket);
    let mut store = MemoryPrivateStore::empty();
    let stamp = host.checkpoint_to(&mut store).unwrap();
    (store, stamp, ticket)
}

#[test]
fn checkpoint_requires_quiescence_and_failed_store_does_not_advance_stamp() {
    let mut host = host();
    let auth = authority();
    let ticket = connect(&mut host, &auth, A, 100);
    let mut store = MemoryPrivateStore::empty();
    assert_eq!(host.checkpoint_to(&mut store), Err(Error::CheckpointBusy));
    host.take_public_events();
    assert_eq!(host.checkpoint_to(&mut store), Err(Error::CheckpointBusy));
    host.take_private(&auth, A, ticket).unwrap();
    store.fail_write = true;
    assert_eq!(host.checkpoint_to(&mut store), Err(Error::StoreFailure));
    store.fail_write = false;
    let stamp = host.checkpoint_to(&mut store).unwrap();
    assert_eq!(stamp.revision, 1);
    let newer = host.checkpoint_to(&mut store).unwrap();
    assert_eq!(newer.revision, 2);
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::CheckpointStampMismatch
    );
}

#[test]
fn restored_timer_preserves_pending_five_tick_refresh_and_rebinds_only_recorded_actor() {
    let mut host = host();
    let auth = authority();
    let old = connect(&mut host, &auth, A, 100);
    drain(&mut host, &auth, A, old);
    host.receive(&auth, A, binary(old, 1, "Timer_IsRunning_Change", &[1]))
        .unwrap();
    host.take_public_events();
    host.tick(&auth, &values()).unwrap();
    host.receive(&auth, A, binary(old, 2, "Timer_IsRunning_Change", &[0]))
        .unwrap();
    host.take_public_events();
    host.tick(&auth, &values()).unwrap(); // tock 1, emits Update.
    host.take_public_events();
    host.tick(&auth, &values()).unwrap(); // tock 2.
    let mut store = MemoryPrivateStore::empty();
    let stamp = host.checkpoint_to(&mut store).unwrap();
    let mut restored = NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap();
    assert_eq!(restored.public_projection(), host.public_projection());
    assert_eq!(
        restored.receive(&auth, A, binary(old, 3, "Timer_Set", &[1, 2])),
        Err(Error::WrongEpoch)
    );
    assert_eq!(restored.take_private(&auth, A, old), Err(Error::WrongEpoch));
    assert_eq!(
        restored.rebind(&auth, B, old.instance_address(), values().0),
        Err(Error::RecipientMismatch)
    );
    let new = restored
        .rebind(
            &auth,
            RECONNECTED_A,
            old.instance_address(),
            TimerRegisters([0, 1, 1, 4]),
        )
        .unwrap();
    assert!(new.generation > old.generation);
    assert_eq!(new.host_epoch, 8);
    assert_eq!(
        restored.rebind(&auth, RECONNECTED_A, old.instance_address(), values().0),
        Err(Error::AlreadyBound)
    );
    let output = restored.take_private(&auth, RECONNECTED_A, new).unwrap();
    assert_eq!(output[1].body(), UiBody::Binary(&[0, 1, 1, 4]));
    assert!(restored.take_public_events().is_empty()); // Does not re-emit VM connect.
    for seconds in 5..7 {
        restored
            .tick(&auth, &Registers(TimerRegisters([0, 1, 1, seconds])))
            .unwrap();
        assert!(
            restored
                .take_private(&auth, RECONNECTED_A, new)
                .unwrap()
                .is_empty()
        );
    }
    restored
        .tick(&auth, &Registers(TimerRegisters([0, 1, 1, 7])))
        .unwrap();
    let output = restored.take_private(&auth, RECONNECTED_A, new).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].event(), "Timer_Update");
    assert_eq!(output[0].body(), UiBody::Text("1:7"));
    restored
        .receive(&auth, RECONNECTED_A, binary(new, 1, "Timer_Set", &[1, 7]))
        .unwrap();
}

#[test]
fn detached_restoration_times_out_without_a_vm_or_transport_binding() {
    let auth = authority();
    let configured = HostLimits {
        idle_timeout_ticks: 2,
        ..limits()
    };
    let mut original = NativeHost::new(identity(7), configured.clone()).unwrap();
    let ticket = connect(&mut original, &auth, A, 100);
    drain(&mut original, &auth, A, ticket);
    let mut store = MemoryPrivateStore::empty();
    let stamp = original.checkpoint_to(&mut store).unwrap();
    let mut restored =
        NativeHost::restore_from(&mut store, identity(8), stamp, configured).unwrap();
    restored.tick(&auth, &MissingRegisters).unwrap();
    assert!(restored.take_public_events().is_empty());
    restored.tick(&auth, &MissingRegisters).unwrap();
    assert_eq!(
        restored.take_public_events(),
        [PublicVmEvent::Disconnected {
            invoker: InvokerId(100)
        }]
    );
    assert_eq!(
        restored.rebind(&auth, RECONNECTED_A, ticket.instance_address(), values().0),
        Err(Error::StaleSession)
    );
}

#[test]
fn checkpoints_reject_schema_version_epochs_stamps_and_timeout_policy_drift() {
    let (mut store, stamp, _) = saved();
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(7), stamp, limits()).unwrap_err(),
        Error::WrongEpoch
    );
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(6), stamp, limits()).unwrap_err(),
        Error::WrongEpoch
    );
    let original = store.bytes.clone();
    for (offset, expected) in [
        (4, Error::UnsupportedCheckpointVersion),
        (6, Error::UnsupportedPluginSchema),
    ] {
        store.bytes = original.clone();
        store.bytes[offset..offset + 2].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(
            NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
            expected
        );
    }
    store.bytes = original;
    let wrong_stamp = CheckpointStamp {
        revision: stamp.revision + 1,
        ..stamp
    };
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), wrong_stamp, limits()).unwrap_err(),
        Error::CheckpointStampMismatch
    );
    assert_eq!(
        NativeHost::restore_from(
            &mut store,
            identity(8),
            stamp,
            HostLimits {
                idle_timeout_ticks: 3,
                ..limits()
            }
        )
        .unwrap_err(),
        Error::InvalidCheckpoint
    );
}

#[test]
fn checkpoint_parser_rejects_all_truncations_counts_duplicates_and_unverified_plugin_state() {
    let (mut store, stamp, _) = saved();
    let original = store.bytes.clone();
    for end in 0..original.len() {
        store.bytes = original[..end].to_vec();
        assert!(
            NativeHost::restore_from(&mut store, identity(8), stamp, limits()).is_err(),
            "accepted truncated checkpoint ending at {end}"
        );
    }
    store.bytes = original.clone();
    store.bytes.extend_from_slice(&[0]);
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::InvalidCheckpoint
    );
    store.bytes = original.clone();
    store.bytes[64..68].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::InvalidCheckpoint
    );
    store.bytes = original.clone();
    store.bytes[64..68].copy_from_slice(&2u32.to_le_bytes());
    store.bytes.extend_from_slice(&original[68..]); // Duplicate ID/actor/invoker/generation.
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::InvalidCheckpoint
    );
    store.bytes = original.clone();
    store.bytes[76..80].copy_from_slice(&0xCB2819CBu32.to_le_bytes());
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::UnverifiedPlugin
    );
    store.bytes[76..80].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::UnregisteredPlugin
    );
    for offset in [136, 137, 138] {
        // running, mode, updated_after_stop.
        store.bytes = original.clone();
        store.bytes[offset] = 255;
        assert_eq!(
            NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
            Error::InvalidCheckpoint
        );
    }
    store.bytes = vec![0; limits().max_checkpoint_bytes + 1];
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::CheckpointTooLarge
    );
    store.bytes = original;
    store.impossible_read_count = true;
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(8), stamp, limits()).unwrap_err(),
        Error::CheckpointTooLarge
    );
}

#[test]
fn private_debug_and_public_projection_never_include_private_state_or_payloads() {
    let auth = authority();
    let mut host = host();
    let ticket = host
        .connect(&auth, request(A, 100, [0, 1, 12345, 23456]))
        .unwrap();
    let outputs = host.take_private(&auth, A, ticket).unwrap();
    assert_eq!(format!("{:?}", outputs[1]), "PrivateUiMessage([REDACTED])");
    assert_eq!(format!("{:?}", outputs[1].body()), "UiBody([REDACTED])");
    assert_eq!(
        format!("{:?}", host.public_projection()),
        "VmProjection { tick: 0 }"
    );
    let debug = format!("{host:?}");
    for secret in [
        "12345",
        "23456",
        "Timer_Show",
        "actor",
        "A11CE",
        "generation",
        "minutes",
        "tock",
    ] {
        assert!(!debug.contains(secret), "private Debug leaked {secret}");
    }
    assert!(debug.contains("[REDACTED]"));
    let private_body = b"PRIVATE-CARD-SEED-AND-HIDDEN-HAND";
    let msg = binary(ticket, 1, "Timer_Set", private_body);
    assert!(!format!("{msg:?}").contains("PRIVATE-CARD"));
    assert!(!format!("{:?}", msg.payload).contains("PRIVATE-CARD"));
    let mut outbox = EffectOutbox::new(EffectLimits::default()).unwrap();
    outbox
        .queue(
            effect_key(1),
            EffectOperation::WritePluginData(private_body),
        )
        .unwrap();
    assert_eq!(format!("{outbox:?}"), "EffectOutbox([REDACTED])");
    assert_eq!(
        format!("{:?}", EffectOperation::WritePluginData(private_body)),
        "EffectOperation([REDACTED])"
    );
}

fn effect_key(operation: u64) -> EffectKey {
    // Synthetic provider-boundary test only; does not instantiate this plugin.
    EffectKey {
        scope: HostScopeId(712),
        plugin: PluginId(0xCB2819CB),
        instance: InstanceId(1),
        operation,
    }
}
struct LostResponseProvider {
    requests: BTreeMap<EffectKey, DurableEffectRequest>,
    attempts: usize,
    applied: usize,
}
impl DurableEffectProvider for LostResponseProvider {
    fn request(
        &mut self,
        request: &DurableEffectRequest,
    ) -> Result<ProviderReceipt, ProviderFailure> {
        self.attempts += 1;
        if let Some(prior) = self.requests.get(&request.key()) {
            assert_eq!(prior, request);
        } else {
            assert_eq!(format!("{request:?}"), "DurableEffectRequest([REDACTED])");
            self.requests.insert(request.key(), request.clone());
            self.applied += 1;
        }
        if self.attempts == 1 {
            return Err(ProviderFailure::Retryable);
        }
        Ok(ProviderReceipt {
            key: request.key(),
            decision: ProviderDecision::Applied,
        })
    }
}

#[test]
fn effect_request_retry_reuses_identity_after_provider_applies_and_loses_response() {
    let mut outbox = EffectOutbox::new(EffectLimits::default()).unwrap();
    let key = effect_key(1);
    let operation = EffectOperation::WritePluginData(b"private persisted state");
    assert_eq!(outbox.queue(key, operation), Ok(QueueResult::Queued));
    assert_eq!(outbox.queue(key, operation), Ok(QueueResult::AlreadyQueued));
    assert_eq!(
        outbox.queue(key, EffectOperation::WritePluginData(b"different state")),
        Err(Error::EffectConflict)
    );
    let mut provider = LostResponseProvider {
        requests: BTreeMap::new(),
        attempts: 0,
        applied: 0,
    };
    assert_eq!(
        outbox.dispatch(key, &mut provider),
        Ok(EffectDispatch::RetryPending)
    );
    assert_eq!(provider.applied, 1);
    let receipt = ProviderReceipt {
        key,
        decision: ProviderDecision::Applied,
    };
    assert_eq!(
        outbox.dispatch(key, &mut provider),
        Ok(EffectDispatch::Complete(receipt))
    );
    assert_eq!(
        outbox.dispatch(key, &mut provider),
        Ok(EffectDispatch::Complete(receipt))
    );
    assert_eq!(outbox.queue(key, operation), Ok(QueueResult::AlreadyFinal));
    assert_eq!((provider.attempts, provider.applied), (2, 1));
}

#[test]
fn wrong_provider_receipt_is_not_recorded_and_declined_receipts_are_terminal() {
    struct Provider {
        wrong: bool,
        calls: usize,
    }
    impl DurableEffectProvider for Provider {
        fn request(
            &mut self,
            request: &DurableEffectRequest,
        ) -> Result<ProviderReceipt, ProviderFailure> {
            self.calls += 1;
            Ok(ProviderReceipt {
                key: if self.wrong {
                    effect_key(2)
                } else {
                    request.key()
                },
                decision: ProviderDecision::Declined,
            })
        }
    }
    let mut outbox = EffectOutbox::new(EffectLimits::default()).unwrap();
    outbox
        .queue(
            effect_key(1),
            EffectOperation::ReserveFunds {
                account: ALICE,
                amount: 7,
            },
        )
        .unwrap();
    let mut provider = Provider {
        wrong: true,
        calls: 0,
    };
    assert_eq!(
        outbox.dispatch(effect_key(1), &mut provider),
        Err(Error::ProviderReceiptMismatch)
    );
    provider.wrong = false;
    let receipt = ProviderReceipt {
        key: effect_key(1),
        decision: ProviderDecision::Declined,
    };
    assert_eq!(
        outbox.dispatch(effect_key(1), &mut provider),
        Ok(EffectDispatch::Complete(receipt))
    );
    assert_eq!(
        outbox.dispatch(effect_key(1), &mut provider),
        Ok(EffectDispatch::Complete(receipt))
    );
    assert_eq!(provider.calls, 2);
}

#[test]
fn effect_records_payloads_and_reservation_scope_are_bounded() {
    let mut outbox = EffectOutbox::new(EffectLimits {
        max_records: 2,
        max_payload_bytes: 4,
        max_total_payload_bytes: 5,
    })
    .unwrap();
    assert_eq!(
        outbox.queue(effect_key(1), EffectOperation::WritePluginData(b"12345")),
        Err(Error::MessageTooLarge)
    );
    outbox
        .queue(effect_key(1), EffectOperation::WritePluginData(b"1234"))
        .unwrap();
    assert_eq!(
        outbox.queue(effect_key(2), EffectOperation::WritePluginData(b"12")),
        Err(Error::EffectLimit)
    );
    let foreign = EffectKey {
        scope: HostScopeId(999),
        ..effect_key(1)
    };
    assert_eq!(
        outbox.queue(
            effect_key(2),
            EffectOperation::RefundReservation {
                reservation: foreign
            }
        ),
        Err(Error::InvalidMessage)
    );
    assert_eq!(
        outbox.queue(
            effect_key(2),
            EffectOperation::RefundReservation {
                reservation: effect_key(2)
            }
        ),
        Err(Error::InvalidMessage)
    );
    outbox
        .queue(
            effect_key(2),
            EffectOperation::RefundReservation {
                reservation: effect_key(1),
            },
        )
        .unwrap();
    assert_eq!(
        outbox.queue(effect_key(3), EffectOperation::WritePluginData(b"")),
        Err(Error::EffectLimit)
    );
    assert_eq!(
        outbox.queue(effect_key(1), EffectOperation::WritePluginData(b"1234")),
        Ok(QueueResult::AlreadyQueued)
    );
}
