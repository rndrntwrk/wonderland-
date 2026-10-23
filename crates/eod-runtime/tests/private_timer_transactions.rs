// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use wonderland_eod_runtime::{protocol, registry::TIMER_PLUGIN, *};

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        match connection.0 {
            42 | 43 => Some(ActorId(77)),
            44 => Some(ActorId(88)),
            _ => None,
        }
    }
}
struct Revoked;
impl ConnectionAuthority for Revoked {
    fn authenticated_actor(&self, _: ConnectionId) -> Option<ActorId> {
        None
    }
}
struct Registers([i16; 4]);
impl RegisterSource for Registers {
    fn timer_registers(&self, invoker: InvokerId) -> Option<TimerRegisters> {
        (invoker == InvokerId(1001)).then_some(TimerRegisters(self.0))
    }
}
#[derive(Default, Clone)]
struct Store {
    bytes: Vec<u8>,
    stamp: Option<CheckpointStamp>,
    deliveries: Vec<PrivateTimerDelivery>,
    fail: bool,
}
impl PrivateTimerTransactionSink for Store {
    fn stage_private_transaction(
        &mut self,
        stamp: CheckpointStamp,
        bytes: &[u8],
        deliveries: &[PrivateTimerDelivery],
    ) -> Result<(), StoreError> {
        if self.fail {
            return Err(StoreError::Unavailable);
        }
        self.bytes = bytes.to_vec();
        self.stamp = Some(stamp);
        self.deliveries = deliveries.to_vec();
        Ok(())
    }
}
impl PrivateTimerTransactionStore for Store {
    fn read_private_transaction(
        &mut self,
        _: HostScopeId,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        if self.bytes.len() > destination.len() {
            return Ok(PrivateRead {
                bytes_written: 0,
                complete: false,
            });
        }
        destination[..self.bytes.len()].copy_from_slice(&self.bytes);
        Ok(PrivateRead {
            bytes_written: self.bytes.len(),
            complete: true,
        })
    }
}
fn identity(epoch: u64) -> HostIdentity {
    HostIdentity {
        scope: HostScopeId(101),
        epoch,
    }
}
fn original() -> NativeHost {
    NativeHost::new(identity(1), HostLimits::default()).unwrap()
}
fn transaction() -> PrivateTimerTransaction {
    original().fork_private_timer_transaction().unwrap()
}
fn request() -> ConnectRequest {
    ConnectRequest {
        connection: ConnectionId(42),
        plugin: TIMER_PLUGIN,
        object: 2002,
        invoker: InvokerId(1001),
        registers: TimerRegisters([0, 1, 1, 2]),
    }
}
fn message(
    ticket: SessionTicket,
    sequence: u64,
    event: &'static str,
    payload: &'static [u8],
) -> ClientMessage<'static> {
    ClientMessage {
        version: protocol::PROTOCOL_VERSION,
        ticket,
        plugin: TIMER_PLUGIN,
        sequence,
        event,
        payload: WirePayload::Binary(payload),
    }
}
fn checkpoint(value: &mut PrivateTimerTransaction) -> Store {
    let mut store = Store::default();
    value.stage_private_transaction(&mut store).unwrap();
    store
}
fn restore(store: &mut Store, epoch: u64) -> Result<PrivateTimerTransaction, Error> {
    let expected = store.stamp.unwrap();
    PrivateTimerTransaction::restore_private_timer_transaction(
        store,
        identity(epoch),
        expected,
        HostLimits::default(),
    )
}
fn private_id(value: &PrivateTimerTransaction, event: &str) -> PrivateTimerOutputId {
    value
        .private_deliveries()
        .iter()
        .find(|d| {
            d.private_message()
                .is_some_and(|message| message.event() == event)
        })
        .unwrap()
        .id()
}

#[test]
fn candidate_mutation_and_staging_never_publish_into_original() {
    let mut host = original();
    let mut candidate = host.fork_private_timer_transaction().unwrap();
    let ticket = candidate.connect(&Auth, request()).unwrap();
    candidate.tick(&Auth, &Registers([0, 1, 1, 2])).unwrap();
    let mut store = checkpoint(&mut candidate);
    assert_eq!(host.public_projection().tick, 0);
    assert!(host.take_public_events().is_empty());
    assert_eq!(store.deliveries.len(), 3);
    assert!(store.deliveries.iter().all(|d| d.origin_ticket() == ticket));
    // The old committed predecessor remains independently usable on abort.
    assert_eq!(host.connect(&Auth, request()).unwrap(), ticket);
    assert_eq!(
        restore(&mut store, 2).unwrap().private_deliveries(),
        candidate.private_deliveries()
    );
}

#[test]
fn immediate_close_reopen_preserves_every_original_ticket_and_private_output() {
    let mut value = transaction();
    let old = value.connect(&Auth, request()).unwrap();
    value.disconnect_invoker(InvokerId(1001), old).unwrap();
    let new = value.connect(&Auth, request()).unwrap();
    assert_ne!(old, new);
    assert_eq!(
        value.disconnect_invoker(InvokerId(1001), old),
        Err(Error::StaleSession)
    );
    let public: Vec<_> = value
        .private_deliveries()
        .iter()
        .filter_map(|d| d.public_event().map(|e| (d.origin_ticket(), e.clone())))
        .collect();
    assert_eq!(
        public,
        vec![
            (
                old,
                PublicVmEvent::Connected {
                    invoker: InvokerId(1001)
                }
            ),
            (
                old,
                PublicVmEvent::Disconnected {
                    invoker: InvokerId(1001)
                }
            ),
            (
                new,
                PublicVmEvent::Connected {
                    invoker: InvokerId(1001)
                }
            ),
        ]
    );
    let private: Vec<_> = value
        .private_deliveries()
        .iter()
        .filter_map(|d| d.private_message().map(|m| (m.ticket(), m.event())))
        .collect();
    assert_eq!(
        private,
        vec![
            (old, "eod_enter"),
            (old, "Timer_Show"),
            (old, "eod_leave"),
            (new, "eod_enter"),
            (new, "Timer_Show")
        ]
    );
    let mut store = checkpoint(&mut value);
    let restored = restore(&mut store, 2).unwrap();
    assert_eq!(restored.private_deliveries(), value.private_deliveries());
}

#[test]
fn successor_requires_current_actor_rebind_and_never_readdresses_old_output() {
    let mut value = transaction();
    let old = value.connect(&Auth, request()).unwrap();
    let original_outputs = value.private_deliveries().to_vec();
    let mut store = checkpoint(&mut value);
    let mut restored = restore(&mut store, 2).unwrap();
    assert_eq!(
        restored.receive(
            &Auth,
            ConnectionId(42),
            message(old, 1, "Timer_Set", &[2, 3])
        ),
        Err(Error::WrongEpoch)
    );
    assert_eq!(
        restored.rebind(
            &Auth,
            ConnectionId(44),
            old.instance_address(),
            TimerRegisters([0, 1, 1, 2])
        ),
        Err(Error::RecipientMismatch)
    );
    let new = restored
        .rebind(
            &Auth,
            ConnectionId(43),
            old.instance_address(),
            TimerRegisters([0, 1, 1, 2]),
        )
        .unwrap();
    assert_eq!(new.host_epoch, 2);
    assert_ne!(new.generation, old.generation);
    assert_eq!(
        &restored.private_deliveries()[..original_outputs.len()],
        original_outputs
    );
    let old_id = private_id(&restored, "Timer_Show");
    assert_eq!(
        restored.acknowledge_private(&Auth, ConnectionId(42), old, old_id, 1, b"ack"),
        Err(Error::WrongEpoch)
    );
    assert_eq!(
        restored.acknowledge_private(&Auth, ConnectionId(43), new, old_id, 1, b"ack"),
        Err(Error::RecipientMismatch)
    );
    let target = restored
        .private_deliveries()
        .iter()
        .find(|d| d.id() == old_id)
        .unwrap()
        .private_recipient()
        .unwrap();
    restored
        .supersede_private(old_id, target, 1, b"durable same-association rebind")
        .unwrap();
    assert_eq!(
        restored
            .private_deliveries()
            .iter()
            .find(|d| d.id() == old_id)
            .unwrap()
            .disposition()
            .unwrap()
            .kind(),
        PrivateTimerDispositionKind::Superseded
    );
}

#[test]
fn send_is_not_ack_and_exact_ack_duplicates_survive_checkpoint() {
    let mut value = transaction();
    let ticket = value.connect(&Auth, request()).unwrap();
    let id = private_id(&value, "Timer_Show");
    // Reading or serializing an outbound frame leaves it pending.
    assert!(
        value
            .private_deliveries()
            .iter()
            .all(|d| d.disposition().is_none())
    );
    let before = checkpoint(&mut value);
    assert_eq!(
        value.acknowledge_private(&Auth, ConnectionId(44), ticket, id, 19, b"ack"),
        Err(Error::RecipientMismatch)
    );
    assert_eq!(
        value.acknowledge_private(&Auth, ConnectionId(43), ticket, id, 19, b"ack"),
        Err(Error::RecipientMismatch)
    );
    assert_eq!(
        value.acknowledge_private(&Revoked, ConnectionId(42), ticket, id, 19, b"ack"),
        Err(Error::Unauthenticated)
    );
    assert_eq!(checkpoint(&mut value).bytes, before.bytes);
    value
        .acknowledge_private(&Auth, ConnectionId(42), ticket, id, 19, b"ack")
        .unwrap();
    let accepted = checkpoint(&mut value);
    value
        .acknowledge_private(&Auth, ConnectionId(42), ticket, id, 19, b"ack")
        .unwrap();
    assert_eq!(checkpoint(&mut value).bytes, accepted.bytes);
    assert_eq!(
        value.acknowledge_private(&Auth, ConnectionId(42), ticket, id, 19, b"other"),
        Err(Error::EffectConflict)
    );
    let enter = private_id(&value, "eod_enter");
    assert_eq!(
        value.acknowledge_private(&Auth, ConnectionId(42), ticket, enter, 19, b"ack"),
        Err(Error::EffectConflict)
    );
    let mut accepted = accepted;
    let restored = restore(&mut accepted, 2).unwrap();
    let receipt = restored
        .private_deliveries()
        .iter()
        .find(|d| d.id() == id)
        .unwrap()
        .disposition()
        .unwrap();
    assert_eq!(receipt.kind(), PrivateTimerDispositionKind::Delivered);
    assert_eq!(receipt.receipt_key(), 19);
    assert_eq!(receipt.private_receipt(), b"ack");
}

#[test]
fn private_dispositions_are_explicit_distinct_and_conflict_fenced() {
    let mut value = transaction();
    value.connect(&Auth, request()).unwrap();
    let id = private_id(&value, "eod_enter");
    let target = value
        .private_deliveries()
        .iter()
        .find(|d| d.id() == id)
        .unwrap()
        .private_recipient()
        .unwrap();
    value
        .suppress_private(id, target, 1, b"authenticated owner revocation")
        .unwrap();
    value
        .suppress_private(id, target, 1, b"authenticated owner revocation")
        .unwrap();
    assert_eq!(
        value.supersede_private(id, target, 1, b"authenticated owner revocation"),
        Err(Error::EffectConflict)
    );
    assert_eq!(
        value.acknowledge_private(
            &Auth,
            target.connection,
            target.ticket,
            id,
            1,
            b"authenticated owner revocation"
        ),
        Err(Error::EffectConflict)
    );
    assert_eq!(
        value
            .private_deliveries()
            .iter()
            .find(|d| d.id() == id)
            .unwrap()
            .disposition()
            .unwrap()
            .kind(),
        PrivateTimerDispositionKind::Suppressed
    );
}

#[test]
fn public_application_receipt_is_exact_and_does_not_ack_private_outputs() {
    let mut value = transaction();
    value.connect(&Auth, request()).unwrap();
    let id = value
        .private_deliveries()
        .iter()
        .find(|d| d.public_event().is_some())
        .unwrap()
        .id();
    value
        .record_public_delivery(id, 4, b"committed public command")
        .unwrap();
    let mut store = checkpoint(&mut value);
    let mut recovered = restore(&mut store, 2).unwrap();
    recovered
        .record_public_delivery(id, 4, b"committed public command")
        .unwrap();
    assert_eq!(
        recovered.record_public_delivery(id, 4, b"different command"),
        Err(Error::EffectConflict)
    );
    assert!(
        recovered
            .private_deliveries()
            .iter()
            .filter(|d| d.private_message().is_some())
            .all(|d| d.disposition().is_none())
    );
}

#[test]
fn actual_timer_handler_preserves_five_tick_refresh_across_candidate_forks() {
    let mut value = transaction();
    let ticket = value.connect(&Auth, request()).unwrap();
    value
        .receive(
            &Auth,
            ConnectionId(42),
            message(ticket, 1, "Timer_IsRunning_Change", &[1]),
        )
        .unwrap();
    value.tick(&Auth, &Registers([1, 1, 1, 2])).unwrap();
    value
        .receive(
            &Auth,
            ConnectionId(42),
            message(ticket, 2, "Timer_IsRunning_Change", &[0]),
        )
        .unwrap();
    value.tick(&Auth, &Registers([0, 1, 1, 3])).unwrap();
    let before = checkpoint(&mut value);
    let mut next = value.fork();
    for seconds in 4..=7 {
        next.tick(&Auth, &Registers([0, 1, 9, seconds])).unwrap();
    }
    let update = next
        .private_deliveries()
        .iter()
        .filter_map(|d| d.private_message())
        .find(|message| message.event() == "Timer_Update")
        .unwrap();
    assert!(matches!(update.body(), UiBody::Text("9:7")));
    assert_eq!(checkpoint(&mut value).bytes, before.bytes);
    assert_eq!(
        next.private_deliveries()
            .iter()
            .filter(|d| matches!(
                d.public_event(),
                Some(PublicVmEvent::Timer {
                    event: TimerVmEvent::Update,
                    ..
                })
            ))
            .count(),
        1
    );
}

#[test]
fn restored_handler_retains_tock_and_waits_for_authenticated_rebind() {
    let mut value = transaction();
    let ticket = value.connect(&Auth, request()).unwrap();
    value
        .receive(
            &Auth,
            ConnectionId(42),
            message(ticket, 1, "Timer_IsRunning_Change", &[1]),
        )
        .unwrap();
    value.tick(&Auth, &Registers([1, 1, 1, 2])).unwrap();
    value
        .receive(
            &Auth,
            ConnectionId(42),
            message(ticket, 2, "Timer_IsRunning_Change", &[0]),
        )
        .unwrap();
    for _ in 0..3 {
        value.tick(&Auth, &Registers([0, 1, 1, 2])).unwrap();
    }
    let mut store = checkpoint(&mut value);
    let mut recovered = restore(&mut store, 2).unwrap();
    // Detached time does not run the Timer handler.
    recovered.tick(&Auth, &Registers([0, 1, 2, 3])).unwrap();
    recovered
        .rebind(
            &Auth,
            ConnectionId(43),
            ticket.instance_address(),
            TimerRegisters([0, 1, 2, 3]),
        )
        .unwrap();
    recovered.tick(&Auth, &Registers([0, 1, 2, 4])).unwrap();
    assert!(
        !recovered
            .private_deliveries()
            .iter()
            .filter_map(|d| d.private_message())
            .any(|m| m.event() == "Timer_Update")
    );
    recovered.tick(&Auth, &Registers([0, 1, 2, 5])).unwrap();
    assert!(
        recovered
            .private_deliveries()
            .iter()
            .filter_map(|d| d.private_message())
            .any(|m| m.event() == "Timer_Update" && matches!(m.body(), UiBody::Text("2:5")))
    );
}

#[test]
fn timeout_and_revocation_keep_old_custody_with_original_ticket() {
    for revoked in [false, true] {
        let limits = HostLimits {
            idle_timeout_ticks: 1,
            ..HostLimits::default()
        };
        let mut value = NativeHost::new(identity(1), limits)
            .unwrap()
            .fork_private_timer_transaction()
            .unwrap();
        let ticket = value.connect(&Auth, request()).unwrap();
        if revoked {
            value.tick(&Revoked, &Registers([0, 1, 1, 2])).unwrap();
        } else {
            value.tick(&Auth, &Registers([0, 1, 1, 2])).unwrap();
        }
        assert!(value.current_ticket(InvokerId(1001)).is_none());
        assert!(
            value
                .private_deliveries()
                .iter()
                .all(|d| d.origin_ticket() == ticket && d.disposition().is_none())
        );
        assert!(
            value
                .private_deliveries()
                .iter()
                .any(|d| matches!(d.public_event(), Some(PublicVmEvent::Disconnected { .. })))
        );
        assert!(
            value
                .private_deliveries()
                .iter()
                .filter_map(|d| d.private_message())
                .any(|m| m.event() == "Timer_Show")
        );
    }
}

#[test]
fn malformed_restore_is_bounded_and_cannot_bypass_epoch_or_stamp() {
    let mut value = transaction();
    value.connect(&Auth, request()).unwrap();
    let saved = checkpoint(&mut value);
    let mut store = saved.clone();
    assert!(matches!(restore(&mut store, 1), Err(Error::WrongEpoch)));
    let wrong = CheckpointStamp {
        revision: saved.stamp.unwrap().revision + 1,
        ..saved.stamp.unwrap()
    };
    assert!(matches!(
        PrivateTimerTransaction::restore_private_timer_transaction(
            &mut store,
            identity(2),
            wrong,
            HostLimits::default()
        ),
        Err(Error::CheckpointStampMismatch)
    ));
    for length in 0..saved.bytes.len() {
        let mut truncated = Store {
            bytes: saved.bytes[..length].to_vec(),
            ..saved.clone()
        };
        assert!(
            restore(&mut truncated, 2).is_err(),
            "accepted truncated length {length}"
        );
    }
    let nested = u32::from_le_bytes(saved.bytes[6..10].try_into().unwrap()) as usize;
    let count_offset = 10 + nested + 8;
    let mut bad = saved.clone();
    bad.bytes[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
    let mut bad = saved.clone();
    bad.bytes[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
    let mut bad = saved.clone();
    bad.bytes.push(0);
    assert!(restore(&mut bad, 2).is_err());
    // An output cannot claim it was produced in the successor epoch.
    let first_output = count_offset + 4;
    let mut bad = saved.clone();
    bad.bytes[first_output + 16..first_output + 24].copy_from_slice(&2u64.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
}

#[test]
fn sink_failure_and_unstageable_mutation_preserve_original_candidate() {
    let mut value = transaction();
    value.connect(&Auth, request()).unwrap();
    let mut failed = Store {
        fail: true,
        ..Store::default()
    };
    assert_eq!(
        value.stage_private_transaction(&mut failed),
        Err(Error::StoreFailure)
    );
    let first = checkpoint(&mut value);
    assert_eq!(first.stamp.unwrap().revision, 1);
    assert_eq!(checkpoint(&mut value).bytes, first.bytes);
    let limits = HostLimits {
        max_checkpoint_bytes: 256,
        ..HostLimits::default()
    };
    let mut small = NativeHost::new(identity(1), limits)
        .unwrap()
        .fork_private_timer_transaction()
        .unwrap();
    let empty = checkpoint(&mut small);
    assert_eq!(
        small.connect(&Auth, request()),
        Err(Error::CheckpointTooLarge)
    );
    assert_eq!(checkpoint(&mut small).bytes, empty.bytes);
    assert!(small.current_ticket(InvokerId(1001)).is_none());
}

#[test]
fn queue_limits_count_retained_custody_and_fail_without_partial_mutation() {
    let limits = HostLimits {
        max_private_messages: 2,
        ..HostLimits::default()
    };
    let mut value = NativeHost::new(identity(1), limits)
        .unwrap()
        .fork_private_timer_transaction()
        .unwrap();
    let ticket = value.connect(&Auth, request()).unwrap();
    let before = checkpoint(&mut value);
    assert_eq!(
        value.disconnect_invoker(InvokerId(1001), ticket),
        Err(Error::QueueFull)
    );
    assert_eq!(value.current_ticket(InvokerId(1001)), Some(ticket));
    assert_eq!(checkpoint(&mut value).bytes, before.bytes);
}

#[test]
fn legacy_barrier_contract_and_non_timer_rejection_are_unchanged() {
    struct LegacyStore;
    impl PrivateCheckpointStore for LegacyStore {
        fn write_private(&mut self, _: PrivateCheckpointKey, _: &[u8]) -> Result<(), StoreError> {
            Ok(())
        }
        fn read_private(
            &mut self,
            _: PrivateCheckpointKey,
            _: &mut [u8],
        ) -> Result<PrivateRead, StoreError> {
            Err(StoreError::Unavailable)
        }
    }
    let mut host = original();
    let ticket = host.connect(&Auth, request()).unwrap();
    assert!(matches!(
        host.fork_private_timer_transaction(),
        Err(Error::CheckpointBusy)
    ));
    assert_eq!(
        host.checkpoint_to(&mut LegacyStore),
        Err(Error::CheckpointBusy)
    );
    assert_eq!(host.take_public_events().len(), 1);
    assert_eq!(
        host.take_private(&Auth, ConnectionId(42), ticket)
            .unwrap()
            .len(),
        2
    );
    assert!(host.fork_private_timer_transaction().is_ok());
    let mut other = original();
    other
        .connect_dance_controller(2002, InvokerId(1001))
        .unwrap();
    assert!(matches!(
        other.fork_private_timer_transaction(),
        Err(Error::WrongPlugin)
    ));
    assert_eq!(other.take_public_events().len(), 1);
}

#[test]
fn private_debug_is_redacted_and_receipts_are_bounded() {
    let mut value = transaction();
    let ticket = value.connect(&Auth, request()).unwrap();
    let id = private_id(&value, "Timer_Show");
    let before = checkpoint(&mut value);
    assert_eq!(
        value.acknowledge_private(&Auth, ConnectionId(42), ticket, id, 1, &vec![0; 4097]),
        Err(Error::MessageTooLarge)
    );
    assert_eq!(checkpoint(&mut value).bytes, before.bytes);
    assert_eq!(format!("{value:?}"), "PrivateTimerTransaction([REDACTED])");
    for delivery in value.private_deliveries() {
        assert_eq!(format!("{delivery:?}"), "PrivateTimerDelivery([REDACTED])");
    }
}

#[test]
fn public_supersession_requires_exact_original_ticket_and_remains_distinct() {
    let mut value = transaction();
    let old = value.connect(&Auth, request()).unwrap();
    value.disconnect_invoker(InvokerId(1001), old).unwrap();
    let new = value.connect(&Auth, request()).unwrap();
    let id = value
        .private_deliveries()
        .iter()
        .find(|d| matches!(d.public_event(), Some(PublicVmEvent::Disconnected { .. })))
        .unwrap()
        .id();
    assert_eq!(
        value.supersede_public(id, new, 31, b"cleared source association"),
        Err(Error::StaleSession)
    );
    value
        .supersede_public(id, old, 31, b"cleared source association")
        .unwrap();
    value
        .supersede_public(id, old, 31, b"cleared source association")
        .unwrap();
    assert_eq!(
        value.record_public_delivery(id, 31, b"cleared source association"),
        Err(Error::EffectConflict)
    );
    let mut store = checkpoint(&mut value);
    let restored = restore(&mut store, 2).unwrap();
    assert_eq!(
        restored
            .private_deliveries()
            .iter()
            .find(|d| d.id() == id)
            .unwrap()
            .disposition()
            .unwrap()
            .kind(),
        PrivateTimerDispositionKind::Superseded
    );
}

#[test]
fn malformed_delivery_identity_and_oversized_receipts_are_rejected() {
    let mut value = transaction();
    value.connect(&Auth, request()).unwrap();
    let saved = checkpoint(&mut value);
    let nested = u32::from_le_bytes(saved.bytes[6..10].try_into().unwrap()) as usize;
    let first = 10 + nested + 12;
    let second = first + 48;
    let third = second + 63;
    let mut bad = saved.clone();
    // Duplicate monotonic output ID.
    bad.bytes[second..second + 8].copy_from_slice(&1u64.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
    let mut bad = saved.clone();
    // Conflicting actor for the exact active instance.
    bad.bytes[second + 41..second + 49].copy_from_slice(&88u64.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
    let mut bad = saved.clone();
    // Conflicting original connection for the same immutable ticket.
    bad.bytes[third + 49..third + 57].copy_from_slice(&43u64.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
    let mut bad = saved.clone();
    // A forged private payload length never allocates an input-sized buffer.
    bad.bytes[second + 58..second + 62].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
    let id = private_id(&value, "Timer_Show");
    let target = value
        .private_deliveries()
        .iter()
        .find(|d| d.id() == id)
        .unwrap()
        .private_recipient()
        .unwrap();
    value.suppress_private(id, target, 9, b"receipt").unwrap();
    let mut bad = checkpoint(&mut value);
    let length_offset = bad.bytes.len() - 7 - 4;
    bad.bytes[length_offset..length_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(restore(&mut bad, 2).is_err());
}

#[test]
fn recovery_show_uses_retained_flags_and_source_packed_minutes_not_scratch_temps() {
    let mut value = transaction();
    let ticket = value.connect(&Auth, request()).unwrap();
    value
        .receive(
            &Auth,
            ConnectionId(42),
            message(ticket, 1, "Timer_Set", &[1, 7]),
        )
        .unwrap();
    value
        .receive(
            &Auth,
            ConnectionId(42),
            message(ticket, 2, "Timer_IsRunning_Change", &[1]),
        )
        .unwrap();
    let mut store = checkpoint(&mut value);
    let mut recovered = restore(&mut store, 2).unwrap();
    let fresh = recovered
        .rebind(
            &Auth,
            ConnectionId(43),
            ticket.instance_address(),
            TimerRegisters([30, 2, 99, 59]),
        )
        .unwrap();
    let shown = recovered
        .private_deliveries()
        .iter()
        .find(|d| {
            d.origin_ticket() == fresh
                && d.private_message()
                    .is_some_and(|m| m.event() == "Timer_Show")
        })
        .unwrap();
    assert!(matches!(
        shown.private_message().unwrap().body(),
        UiBody::Binary([1, 1, 1, 7])
    ));
    // Recovery itself is checkpointed, even without a source tick. Restoring
    // that exact successor stamp and reconnecting again must remain possible.
    let mut store = checkpoint(&mut recovered);
    let mut again = restore(&mut store, 3).unwrap();
    let next = again
        .rebind(
            &Auth,
            ConnectionId(42),
            fresh.instance_address(),
            TimerRegisters([30, 2, 0, 0]),
        )
        .unwrap();
    let shown = again
        .private_deliveries()
        .iter()
        .find(|d| {
            d.origin_ticket() == next
                && d.private_message()
                    .is_some_and(|m| m.event() == "Timer_Show")
        })
        .unwrap();
    assert!(matches!(
        shown.private_message().unwrap().body(),
        UiBody::Binary([1, 1, 1, 7])
    ));
}
