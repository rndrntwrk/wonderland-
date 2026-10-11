// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Literal source-derived native-host cases. No original runtime was executed.

use std::collections::BTreeMap;
use wonderland_eod_runtime::{persistence::*, registry::*, *};

const A: ConnectionId = ConnectionId(11);
const B: ConnectionId = ConnectionId(22);
const A_NEW: ConnectionId = ConnectionId(33);

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        match connection {
            A | A_NEW => Some(ActorId(101)),
            B => Some(ActorId(202)),
            _ => None,
        }
    }
}
struct NoRegisters;
impl RegisterSource for NoRegisters {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        None
    }
}
fn host() -> NativeHost {
    NativeHost::new(identity(1), HostLimits::default()).unwrap()
}
fn identity(epoch: u64) -> HostIdentity {
    HostIdentity {
        scope: HostScopeId(987),
        epoch,
    }
}
fn req(connection: ConnectionId, invoker: u32, input: PluginInput) -> PluginConnectRequest {
    PluginConnectRequest {
        connection,
        object: 700,
        invoker: InvokerId(invoker),
        input,
    }
}
fn msg<'a>(
    ticket: SessionTicket,
    plugin: PluginId,
    sequence: u64,
    event: &'a str,
    payload: WirePayload<'a>,
) -> ClientMessage<'a> {
    ClientMessage {
        version: protocol::PROTOCOL_VERSION,
        ticket,
        plugin,
        sequence,
        event,
        payload,
    }
}
fn text_message<'a>(
    ticket: SessionTicket,
    plugin: PluginId,
    sequence: u64,
    event: &'a str,
    payload: &'a str,
) -> ClientMessage<'a> {
    msg(ticket, plugin, sequence, event, WirePayload::Text(payload))
}
fn drain(
    host: &mut NativeHost,
    ticket: SessionTicket,
    connection: ConnectionId,
) -> Vec<(String, Vec<u8>)> {
    host.take_private(&Auth, connection, ticket)
        .unwrap()
        .into_iter()
        .map(|m| {
            (
                m.event().into(),
                match m.body() {
                    UiBody::Text(t) => t.as_bytes().to_vec(),
                    UiBody::Binary(b) => b.to_vec(),
                },
            )
        })
        .collect()
}

#[derive(Default)]
struct Store(Vec<u8>);
impl PrivateCheckpointStore for Store {
    fn write_private(&mut self, _: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError> {
        self.0 = bytes.to_vec();
        Ok(())
    }
    fn read_private(
        &mut self,
        _: PrivateCheckpointKey,
        dst: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        let n = dst.len().min(self.0.len());
        dst[..n].copy_from_slice(&self.0[..n]);
        Ok(PrivateRead {
            bytes_written: n,
            complete: n == self.0.len(),
        })
    }
}

#[derive(Default)]
struct Provider {
    data: BTreeMap<PluginDataKey, (u64, Vec<u8>)>,
    receipts: BTreeMap<PluginWriteId, (PluginDataWrite, PluginWriteReceipt)>,
    calls: usize,
    applied: usize,
    lose_response: bool,
}
impl PluginDataProvider for Provider {
    fn load(
        &mut self,
        _: HostIdentity,
        key: PluginDataKey,
        dst: &mut [u8],
    ) -> Result<PluginDataRead, PersistenceFailure> {
        match self.data.get(&key) {
            None => Ok(PluginDataRead {
                revision: 0,
                exists: false,
                bytes_written: 0,
                complete: true,
            }),
            Some((revision, bytes)) => {
                let n = dst.len().min(bytes.len());
                dst[..n].copy_from_slice(&bytes[..n]);
                Ok(PluginDataRead {
                    revision: *revision,
                    exists: true,
                    bytes_written: n,
                    complete: n == bytes.len(),
                })
            }
        }
    }
    fn write(
        &mut self,
        _: HostIdentity,
        request: &PluginDataWrite,
    ) -> Result<PluginWriteReceipt, PersistenceFailure> {
        self.calls += 1;
        if let Some((saved, receipt)) = self.receipts.get(&request.id()) {
            assert_eq!(saved, request);
            return Ok(*receipt);
        }
        let revision = self.data.get(&request.key()).map_or(0, |(rev, _)| *rev);
        let decision = if revision != request.expected_revision() {
            PluginWriteDecision::Conflict
        } else {
            self.applied += 1;
            self.data
                .insert(request.key(), (revision + 1, request.bytes().to_vec()));
            PluginWriteDecision::Applied {
                revision: revision + 1,
            }
        };
        let receipt = PluginWriteReceipt {
            id: request.id(),
            decision,
        };
        self.receipts
            .insert(request.id(), (request.clone(), receipt));
        if self.lose_response {
            self.lose_response = false;
            return Err(PersistenceFailure::Retryable);
        }
        Ok(receipt)
    }
}
fn scoreboard() -> PluginInput {
    PluginInput::Scoreboard {
        persistent_object: 77,
    }
}
fn sign(mode: SignsMode, owner: bool, roomie: bool, max_length: u16) -> PluginInput {
    PluginInput::Signs {
        persistent_object: 78,
        input: SignsInput {
            mode,
            max_length,
            is_roommate: roomie,
            owner_authorized: owner,
        },
    }
}
fn door(mode: DoorMode, editor: bool) -> PluginInput {
    PluginInput::PermissionDoor {
        persistent_object: 79,
        input: DoorInput {
            mode,
            max_fee: 200,
            permission_state: 1,
            door_fee: 20,
            flags: 252,
            edit_authorized: editor,
        },
    }
}

#[test]
fn dance_routes_buttons_to_only_the_bound_controller_with_native_avatar_identity() {
    let mut host = host();
    let controller = host.connect_dance_controller(700, InvokerId(700)).unwrap();
    let alice = host
        .connect_plugin(
            &Auth,
            req(
                A,
                101,
                PluginInput::DanceFloor {
                    avatar_object: 1234,
                },
            ),
        )
        .unwrap();
    let bob = host
        .connect_plugin(
            &Auth,
            req(
                B,
                202,
                PluginInput::DanceFloor {
                    avatar_object: 2345,
                },
            ),
        )
        .unwrap();
    assert_eq!(
        drain(&mut host, alice, A),
        [("eod_enter".into(), vec![]), ("dance_show".into(), vec![])]
    );
    drain(&mut host, bob, B);
    host.take_public_events();
    for (sequence, body, code) in [(1, "1", 1), (2, "255", 255), (3, " +25 ", 25)] {
        host.receive(
            &Auth,
            A,
            text_message(alice, DANCE_FLOOR_PLUGIN, sequence, "press_button", body),
        )
        .unwrap();
        assert_eq!(
            host.take_public_events()
                .iter()
                .map(PublicVmEvent::source_event)
                .collect::<Vec<_>>(),
            [(InvokerId(700), code, vec![1234])]
        );
    }
    for (sequence, body) in [(4, "256"), (5, "-1"), (6, "1,2345")] {
        host.receive(
            &Auth,
            A,
            text_message(alice, DANCE_FLOOR_PLUGIN, sequence, "press_button", body),
        )
        .unwrap();
        assert!(host.take_public_events().is_empty());
    }
    assert_eq!(
        host.receive(
            &Auth,
            A,
            text_message(bob, DANCE_FLOOR_PLUGIN, 1, "press_button", "1")
        ),
        Err(Error::RecipientMismatch)
    );
    host.disconnect_invoker(InvokerId(700)).unwrap();
    host.take_public_events();
    host.receive(
        &Auth,
        B,
        text_message(bob, DANCE_FLOOR_PLUGIN, 1, "press_button", "2"),
    )
    .unwrap();
    assert!(host.take_public_events().is_empty());
    assert_eq!(controller.host_scope, HostScopeId(987));
}

#[test]
fn scoreboard_load_edit_and_write_ahead_checkpoint_use_real_host_dispatch() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    assert_eq!(
        drain(&mut host, ticket, A),
        [
            ("eod_enter".into(), vec![]),
            ("scoreboard_show".into(), vec![])
        ]
    );
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    assert_eq!(
        drain(&mut host, ticket, A),
        [("scoreboard_state".into(), vec![0, 1, 0, 0, 0, 0])]
    );
    host.receive(
        &Auth,
        A,
        text_message(
            ticket,
            SCOREBOARD_PLUGIN,
            1,
            "scoreboard_setscore",
            "LHS,999",
        ),
    )
    .unwrap();
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(101), 1, vec![999])
    );
    assert_eq!(
        drain(&mut host, ticket, A),
        [("scoreboard_state".into(), vec![0, 1, 231, 3, 0, 0])]
    );
    assert_eq!(
        host.drive_persistence(&mut provider)
            .unwrap()
            .waiting_for_checkpoint,
        1
    );
    assert_eq!(provider.calls, 0);
    host.checkpoint_to(&mut store).unwrap();
    host.drive_persistence(&mut provider).unwrap();
    assert_eq!(provider.applied, 1);
    host.receive(
        &Auth,
        A,
        text_message(
            ticket,
            SCOREBOARD_PLUGIN,
            2,
            "scoreboard_updatescore",
            "LHS,32767",
        ),
    )
    .unwrap();
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(101), 1, vec![0])
    );
    assert_eq!(drain(&mut host, ticket, A)[0].1, vec![0, 1, 0, 0, 0, 0]);
}

#[test]
fn signs_permission_calculation_and_utf16_text_limit_are_private_and_host_bound() {
    let mut host = host();
    let mut provider = Provider::default();
    assert_eq!(
        host.connect_plugin(
            &Auth,
            req(A, 101, sign(SignsMode::OwnerWrite, false, true, 2))
        ),
        Err(Error::NotAuthorized)
    );
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, sign(SignsMode::Write, false, true, 3)))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    let original = [63, 0, 6, b'A', 0xf0, 0x9f, 0x98, 0x80, b'z']; // A + U+1F600 + z is four UTF-16 units.
    assert_eq!(
        host.receive(
            &Auth,
            A,
            msg(
                ticket,
                SIGNS_PLUGIN,
                1,
                "set_message",
                WirePayload::Binary(&original)
            )
        ),
        Err(Error::PluginNotReady)
    );
    host.drive_persistence(&mut provider).unwrap();
    host.tick(&Auth, &NoRegisters).unwrap();
    assert_eq!(
        drain(&mut host, ticket, A),
        [
            ("signs_init".into(), b"1\n3".to_vec()),
            ("signs_show".into(), vec![15, 0, 0])
        ]
    );
    host.receive(
        &Auth,
        A,
        msg(
            ticket,
            SIGNS_PLUGIN,
            1,
            "set_message",
            WirePayload::Binary(&original),
        ),
    )
    .unwrap();
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(101), 1, vec![1])
    );
    let mut store = Store::default();
    host.checkpoint_to(&mut store).unwrap();
    host.drive_persistence(&mut provider).unwrap();
    assert_eq!(
        provider.data.values().next().unwrap().1,
        vec![15, 0, 5, b'A', 0xf0, 0x9f, 0x98, 0x80]
    );
    assert!(!format!("{host:?}").contains('😀'));
}

#[test]
fn door_editor_and_code_input_follow_source_event_and_secret_rules() {
    let mut host = host();
    let mut provider = Provider::default();
    assert_eq!(
        host.connect_plugin(&Auth, req(A, 101, door(DoorMode::Edit, false))),
        Err(Error::NotAuthorized)
    );
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, door(DoorMode::Edit, true)))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    host.tick(&Auth, &NoRegisters).unwrap();
    assert_eq!(
        drain(&mut host, ticket, A),
        [
            ("door_init".into(), b"0\n200\n1\n20\n252".to_vec()),
            ("door_code".into(), b"0".to_vec())
        ]
    );
    for (seq, event, body, code, args) in [
        (1, "set_state", "2", 2, vec![2]),
        (2, "set_fee", "200", 3, vec![200]),
        (3, "set_flags", "65535", 4, vec![252]),
    ] {
        host.receive(
            &Auth,
            A,
            text_message(ticket, PERMISSION_DOOR_PLUGIN, seq, event, body),
        )
        .unwrap();
        assert_eq!(
            host.take_public_events()[0].source_event(),
            (InvokerId(101), code, args)
        );
    }
    host.receive(
        &Auth,
        A,
        text_message(ticket, PERMISSION_DOOR_PLUGIN, 4, "set_code", "001234567"),
    )
    .unwrap();
    assert!(host.take_public_events().is_empty());
    assert!(drain(&mut host, ticket, A).is_empty());
    host.receive(
        &Auth,
        A,
        text_message(ticket, PERMISSION_DOOR_PLUGIN, 5, "close", ""),
    )
    .unwrap();
    assert_eq!(
        host.take_public_events()
            .iter()
            .map(PublicVmEvent::source_event)
            .collect::<Vec<_>>(),
        [(InvokerId(101), 1, vec![]), (InvokerId(101), -1, vec![])]
    );
    drain(&mut host, ticket, A);
    let mut store = Store::default();
    host.checkpoint_to(&mut store).unwrap();
    host.drive_persistence(&mut provider).unwrap();
    let input = host
        .connect_plugin(&Auth, req(B, 202, door(DoorMode::CodeInput, false)))
        .unwrap();
    drain(&mut host, input, B);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    host.tick(&Auth, &NoRegisters).unwrap();
    assert_eq!(
        drain(&mut host, input, B),
        [("door_init".into(), b"2\n200\n1\n20\n252".to_vec())]
    );
    host.receive(
        &Auth,
        B,
        text_message(input, PERMISSION_DOOR_PLUGIN, 1, "try_code", "1234567"),
    )
    .unwrap();
    assert_eq!(
        host.take_public_events()
            .iter()
            .map(PublicVmEvent::source_event)
            .collect::<Vec<_>>(),
        [(InvokerId(202), 7, vec![1]), (InvokerId(202), -1, vec![])]
    );
}

#[test]
fn checkpointed_lost_response_replays_original_write_and_gates_restored_ui() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let old = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, old, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, old, A);
    host.receive(
        &Auth,
        A,
        text_message(old, SCOREBOARD_PLUGIN, 1, "scoreboard_setscore", "RHS,17"),
    )
    .unwrap();
    host.take_public_events();
    drain(&mut host, old, A);
    let stamp = host.checkpoint_to(&mut store).unwrap();
    provider.lose_response = true;
    assert_eq!(
        host.drive_persistence(&mut provider).unwrap().retry_pending,
        1
    );
    assert_eq!(provider.applied, 1);
    let original_id = *provider.receipts.keys().next().unwrap();
    let mut restored =
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default()).unwrap();
    assert_eq!(
        restored.rebind(&Auth, A_NEW, old.instance_address(), TimerRegisters([0; 4])),
        Err(Error::ReconciliationRequired)
    );
    restored.drive_persistence(&mut provider).unwrap();
    assert_eq!((provider.calls, provider.applied), (2, 1));
    assert_eq!(*provider.receipts.keys().next().unwrap(), original_id);
    let new = restored
        .rebind(&Auth, A_NEW, old.instance_address(), TimerRegisters([0; 4]))
        .unwrap();
    assert_eq!(new.host_epoch, 2);
    assert!(new.generation > old.generation);
    assert_eq!(
        drain(&mut restored, new, A_NEW),
        [
            ("eod_enter".into(), vec![]),
            ("scoreboard_show".into(), vec![]),
            ("scoreboard_state".into(), vec![0, 1, 0, 0, 17, 0])
        ]
    );
    assert!(restored.take_public_events().is_empty());
}

#[test]
fn provider_divergence_and_conflicts_are_explicit_and_do_not_overwrite_data() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    let stamp = host.checkpoint_to(&mut store).unwrap();
    let key = PluginDataKey {
        scope: HostScopeId(987),
        plugin: SCOREBOARD_PLUGIN,
        persistent_object: 77,
    };
    provider.data.insert(key, (9, vec![1, 2, 3, 0, 4, 0]));
    let mut restored =
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default()).unwrap();
    assert_eq!(
        restored.drive_persistence(&mut provider),
        Err(Error::PersistenceDiverged)
    );
    assert_eq!(
        restored.rebind(
            &Auth,
            A_NEW,
            ticket.instance_address(),
            TimerRegisters([0; 4])
        ),
        Err(Error::ReconciliationRequired)
    );
    host.receive(
        &Auth,
        A,
        text_message(
            ticket,
            SCOREBOARD_PLUGIN,
            1,
            "scoreboard_setscore",
            "LHS,12",
        ),
    )
    .unwrap();
    host.take_public_events();
    drain(&mut host, ticket, A);
    host.checkpoint_to(&mut store).unwrap();
    assert_eq!(host.drive_persistence(&mut provider).unwrap().conflicted, 1);
    assert_eq!(provider.data[&key], (9, vec![1, 2, 3, 0, 4, 0]));
    assert_eq!(host.abort_conflicted_writes().unwrap(), 1);
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(101), -1, vec![])
    );
    assert_eq!(
        host.receive(
            &Auth,
            A,
            text_message(
                ticket,
                SCOREBOARD_PLUGIN,
                2,
                "scoreboard_setscore",
                "LHS,13"
            )
        ),
        Err(Error::StaleSession)
    );
}

#[test]
fn inconsistent_scoreboard_handler_and_intent_bytes_fail_before_provider_dispatch() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    host.receive(
        &Auth,
        A,
        text_message(
            ticket,
            SCOREBOARD_PLUGIN,
            1,
            "scoreboard_setscore",
            "RHS,17",
        ),
    )
    .unwrap();
    host.take_public_events();
    drain(&mut host, ticket, A);
    let stamp = host.checkpoint_to(&mut store).unwrap();
    let original = store.0.clone();
    // Format2 header68 + participant64 + schema2 + length4 + SCB1/loaded5
    // + source field offset4 = RHS low byte147. The final blob is the intent.
    for offset in [147, original.len() - 2] {
        store.0 = original.clone();
        assert_eq!(store.0[offset], 17);
        store.0[offset] = 18;
        assert_eq!(
            NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default())
                .unwrap_err(),
            Error::InvalidCheckpoint
        );
    }
    assert_eq!(provider.calls, 0);
}

#[test]
fn ready_scoreboard_snapshot_must_match_its_reconciled_provider_binding() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    let stamp = host.checkpoint_to(&mut store).unwrap();
    store.0[145] = 8;
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default())
            .unwrap_err(),
        Error::InvalidCheckpoint
    );
}

#[test]
fn orphaned_intent_requires_canonical_plugin_bytes_even_when_source_loads_allow_tails() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    host.receive(
        &Auth,
        A,
        text_message(ticket, SCOREBOARD_PLUGIN, 1, "scoreboard_setscore", "LHS,5"),
    )
    .unwrap();
    host.take_public_events();
    drain(&mut host, ticket, A);
    host.disconnect(&Auth, A, ticket).unwrap();
    host.take_public_events();
    drain(&mut host, ticket, A);
    let stamp = host.checkpoint_to(&mut store).unwrap();
    let length_offset = store.0.len() - 10;
    store.0[length_offset..length_offset + 4].copy_from_slice(&7u32.to_le_bytes());
    store.0.push(0x99);
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default())
            .unwrap_err(),
        Error::InvalidCheckpoint
    );
}

#[test]
fn new_handler_allowlists_reject_wrong_kinds_scope_and_recipient_without_consuming_sequence() {
    for (input, plugin, event) in [
        (
            PluginInput::DanceFloor {
                avatar_object: 1234,
            },
            DANCE_FLOOR_PLUGIN,
            "press_button",
        ),
        (
            sign(SignsMode::OwnerWrite, true, true, 20),
            SIGNS_PLUGIN,
            "set_message",
        ),
        (scoreboard(), SCOREBOARD_PLUGIN, "scoreboard_setscore"),
        (
            door(DoorMode::View, false),
            PERMISSION_DOOR_PLUGIN,
            "set_state",
        ),
    ] {
        let mut host = host();
        let ticket = host.connect_plugin(&Auth, req(A, 101, input)).unwrap();
        let wrong_kind = if plugin == SIGNS_PLUGIN {
            WirePayload::Text("text")
        } else {
            WirePayload::Binary(&[1])
        };
        assert_eq!(
            host.receive(&Auth, A, msg(ticket, plugin, 1, event, wrong_kind)),
            Err(Error::EventNotAllowed)
        );
        assert_eq!(
            host.receive(&Auth, B, text_message(ticket, plugin, 1, "close", "")),
            Err(Error::RecipientMismatch)
        );
        assert_eq!(
            host.take_private(&Auth, B, ticket),
            Err(Error::RecipientMismatch)
        );
        let foreign = SessionTicket {
            host_scope: HostScopeId(988),
            ..ticket
        };
        assert_eq!(
            host.receive(&Auth, A, text_message(foreign, plugin, 1, "close", "")),
            Err(Error::WrongScope)
        );
        assert_eq!(host.take_private(&Auth, A, foreign), Err(Error::WrongScope));
        assert_eq!(
            host.receive(
                &Auth,
                A,
                text_message(ticket, TIMER_PLUGIN, 1, "Timer_Close", "")
            ),
            Err(Error::WrongPlugin)
        );
        assert_eq!(
            host.receive(&Auth, A, text_message(ticket, plugin, 1, "close", "")),
            Ok(DispatchOutcome::Closed)
        );
    }
}

#[test]
fn malformed_handler_commands_are_noops_and_do_not_queue_provider_writes() {
    let cases = [
        (
            sign(SignsMode::OwnerWrite, true, true, 2),
            SIGNS_PLUGIN,
            vec![
                ("set_message", WirePayload::Binary(&[])),
                ("set_message", WirePayload::Binary(&[15, 0, 2, b'x'])),
                (
                    "set_message",
                    WirePayload::Binary(&[15, 0, 0xff, 0xff, 0xff, 0xff, 0x7f]),
                ),
                // Source strict writer would throw after cutting the surrogate pair.
                (
                    "set_message",
                    WirePayload::Binary(&[15, 0, 5, b'A', 0xf0, 0x9f, 0x98, 0x80]),
                ),
            ],
        ),
        (
            scoreboard(),
            SCOREBOARD_PLUGIN,
            vec![
                ("scoreboard_setscore", WirePayload::Text("lhs,2")),
                ("scoreboard_setscore", WirePayload::Text("LHS,32768")),
                ("scoreboard_updatescore", WirePayload::Text("LHS,1,2")),
                ("scoreboard_updatecolor", WirePayload::Text("LHS,256")),
            ],
        ),
        (
            door(DoorMode::Edit, true),
            PERMISSION_DOOR_PLUGIN,
            vec![
                ("set_code", WirePayload::Text("1000000000")),
                ("set_code", WirePayload::Text("-1")),
                ("set_fee", WirePayload::Text("201")),
                ("set_state", WirePayload::Text("3")),
                ("set_flags", WirePayload::Text("65536")),
            ],
        ),
    ];
    for (input, plugin, commands) in cases {
        let mut host = host();
        let mut provider = Provider::default();
        let ticket = host.connect_plugin(&Auth, req(A, 101, input)).unwrap();
        drain(&mut host, ticket, A);
        host.take_public_events();
        host.drive_persistence(&mut provider).unwrap();
        host.tick(&Auth, &NoRegisters).unwrap();
        drain(&mut host, ticket, A);
        for (index, (event, payload)) in commands.iter().enumerate() {
            assert_eq!(
                host.receive(
                    &Auth,
                    A,
                    msg(ticket, plugin, index as u64 + 1, event, *payload)
                ),
                Ok(DispatchOutcome::Accepted)
            );
        }
        assert!(host.take_public_events().is_empty());
        assert!(drain(&mut host, ticket, A).is_empty());
        assert_eq!(
            host.drive_persistence(&mut provider).unwrap(),
            PersistenceProgress::default()
        );
        assert_eq!(provider.calls, 0);
        assert_eq!(
            host.receive(
                &Auth,
                A,
                text_message(ticket, plugin, commands.len() as u64 + 1, "close", "")
            ),
            Ok(DispatchOutcome::Closed)
        );
    }
}

#[test]
fn failed_checkpoint_does_not_make_a_write_dispatchable_and_ambiguous_work_cannot_be_aborted() {
    struct FailingStore;
    impl PrivateCheckpointStore for FailingStore {
        fn write_private(&mut self, _: PrivateCheckpointKey, _: &[u8]) -> Result<(), StoreError> {
            Err(StoreError::Unavailable)
        }
        fn read_private(
            &mut self,
            _: PrivateCheckpointKey,
            _: &mut [u8],
        ) -> Result<PrivateRead, StoreError> {
            Err(StoreError::Unavailable)
        }
    }
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    host.receive(
        &Auth,
        A,
        text_message(ticket, SCOREBOARD_PLUGIN, 1, "scoreboard_setscore", "LHS,9"),
    )
    .unwrap();
    assert_eq!(host.checkpoint_to(&mut store), Err(Error::CheckpointBusy));
    host.take_public_events();
    drain(&mut host, ticket, A);
    assert_eq!(
        host.checkpoint_to(&mut FailingStore),
        Err(Error::StoreFailure)
    );
    assert_eq!(
        host.drive_persistence(&mut provider)
            .unwrap()
            .waiting_for_checkpoint,
        1
    );
    assert_eq!(provider.calls, 0);
    assert_eq!(
        host.receive(
            &Auth,
            A,
            text_message(ticket, SCOREBOARD_PLUGIN, 2, "scoreboard_setscore", "RHS,8")
        ),
        Err(Error::PersistencePending)
    );
    assert_eq!(host.checkpoint_to(&mut store).unwrap().revision, 1);
    provider.lose_response = true;
    assert_eq!(
        host.drive_persistence(&mut provider).unwrap().retry_pending,
        1
    );
    assert_eq!(host.abort_conflicted_writes().unwrap(), 0);
    assert_eq!(host.drive_persistence(&mut provider).unwrap().applied, 1);
    host.receive(
        &Auth,
        A,
        text_message(ticket, SCOREBOARD_PLUGIN, 2, "scoreboard_setscore", "RHS,8"),
    )
    .unwrap();
    assert_eq!(
        host.take_public_events()[0].source_event(),
        (InvokerId(101), 2, vec![8])
    );
}

#[test]
fn pending_record_byte_and_output_limits_roll_back_source_state_and_sequence() {
    for (limits, expected) in [
        (
            HostLimits {
                max_persistence_records: 1,
                ..HostLimits::default()
            },
            Error::PersistenceLimit,
        ),
        (
            HostLimits {
                max_total_persistence_bytes: 6,
                ..HostLimits::default()
            },
            Error::PersistenceLimit,
        ),
        (
            HostLimits {
                max_public_events: 1,
                ..HostLimits::default()
            },
            Error::QueueFull,
        ),
    ] {
        let mut host = NativeHost::new(identity(1), limits).unwrap();
        let mut provider = Provider::default();
        let key = PluginDataKey {
            scope: HostScopeId(987),
            plugin: SCOREBOARD_PLUGIN,
            persistent_object: 77,
        };
        provider.data.insert(key, (1, vec![0, 1, 0, 0, 0, 0]));
        let ticket = host
            .connect_plugin(&Auth, req(A, 101, scoreboard()))
            .unwrap();
        drain(&mut host, ticket, A);
        // Keep the single public connect event so the public-queue case rejects.
        host.drive_persistence(&mut provider).unwrap();
        drain(&mut host, ticket, A);
        assert_eq!(
            host.receive(
                &Auth,
                A,
                text_message(
                    ticket,
                    SCOREBOARD_PLUGIN,
                    1,
                    "scoreboard_setscore",
                    "LHS,999"
                )
            ),
            Err(expected)
        );
        assert_eq!(
            host.take_public_events(),
            [PublicVmEvent::Connected {
                invoker: InvokerId(101)
            }]
        );
        assert!(drain(&mut host, ticket, A).is_empty());
        let mut store = Store::default();
        let stamp = host.checkpoint_to(&mut store).unwrap();
        let mut restored =
            NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default())
                .unwrap();
        restored.drive_persistence(&mut provider).unwrap();
        let rebound = restored
            .rebind(
                &Auth,
                A_NEW,
                ticket.instance_address(),
                TimerRegisters([0; 4]),
            )
            .unwrap();
        assert_eq!(
            drain(&mut restored, rebound, A_NEW).last().unwrap().1,
            vec![0, 1, 0, 0, 0, 0]
        );
        host.receive(
            &Auth,
            A,
            text_message(ticket, SCOREBOARD_PLUGIN, 1, "close", ""),
        )
        .unwrap();
        assert_eq!(provider.calls, 0);
    }
}

#[test]
fn malformed_or_oversized_provider_reads_and_wrong_receipts_remain_recoverable() {
    struct WrongProvider {
        inner: Provider,
        bad_read: bool,
        bad_receipt: bool,
    }
    impl PluginDataProvider for WrongProvider {
        fn load(
            &mut self,
            host: HostIdentity,
            key: PluginDataKey,
            dst: &mut [u8],
        ) -> Result<PluginDataRead, PersistenceFailure> {
            if self.bad_read {
                return Ok(PluginDataRead {
                    revision: 1,
                    exists: true,
                    bytes_written: usize::MAX,
                    complete: true,
                });
            }
            self.inner.load(host, key, dst)
        }
        fn write(
            &mut self,
            host: HostIdentity,
            request: &PluginDataWrite,
        ) -> Result<PluginWriteReceipt, PersistenceFailure> {
            let mut receipt = self.inner.write(host, request)?;
            if self.bad_receipt {
                receipt.id.key.persistent_object += 1;
            }
            Ok(receipt)
        }
    }
    let mut host = host();
    let mut provider = WrongProvider {
        inner: Provider::default(),
        bad_read: true,
        bad_receipt: true,
    };
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    assert_eq!(
        host.drive_persistence(&mut provider),
        Err(Error::PersistenceTooLarge)
    );
    assert_eq!(
        host.receive(
            &Auth,
            A,
            text_message(ticket, SCOREBOARD_PLUGIN, 1, "scoreboard_setscore", "LHS,1")
        ),
        Err(Error::PluginNotReady)
    );
    provider.bad_read = false;
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    host.receive(
        &Auth,
        A,
        text_message(ticket, SCOREBOARD_PLUGIN, 1, "scoreboard_setscore", "LHS,1"),
    )
    .unwrap();
    host.take_public_events();
    drain(&mut host, ticket, A);
    host.checkpoint_to(&mut store).unwrap();
    assert_eq!(
        host.drive_persistence(&mut provider),
        Err(Error::ProviderReceiptMismatch)
    );
    assert_eq!(provider.inner.applied, 1);
    provider.bad_receipt = false;
    assert_eq!(host.drive_persistence(&mut provider).unwrap().applied, 1);
    assert_eq!((provider.inner.calls, provider.inner.applied), (2, 1));
}

#[test]
fn restored_dance_controller_is_detached_scoped_and_not_charged_as_a_timer() {
    let mut host = NativeHost::new(
        identity(1),
        HostLimits {
            max_timers: 1,
            ..HostLimits::default()
        },
    )
    .unwrap();
    let controller = host.connect_dance_controller(700, InvokerId(700)).unwrap();
    let ticket = host
        .connect_plugin(
            &Auth,
            req(
                A,
                101,
                PluginInput::DanceFloor {
                    avatar_object: 1234,
                },
            ),
        )
        .unwrap();
    drain(&mut host, ticket, A);
    let timer = host
        .connect(
            &Auth,
            ConnectRequest {
                connection: B,
                plugin: TIMER_PLUGIN,
                object: 800,
                invoker: InvokerId(202),
                registers: TimerRegisters([0, 1, 1, 2]),
            },
        )
        .unwrap();
    drain(&mut host, timer, B);
    host.take_public_events();
    let mut store = Store::default();
    let stamp = host.checkpoint_to(&mut store).unwrap();
    assert_eq!(&store.0[4..6], &[2, 0]);
    let mut restored = NativeHost::restore_from(
        &mut store,
        identity(2),
        stamp,
        HostLimits {
            max_timers: 1,
            ..HostLimits::default()
        },
    )
    .unwrap();
    let current = restored
        .rebind(
            &Auth,
            A_NEW,
            ticket.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    drain(&mut restored, current, A_NEW);
    restored
        .receive(
            &Auth,
            A_NEW,
            text_message(current, DANCE_FLOOR_PLUGIN, 1, "press_button", "25"),
        )
        .unwrap();
    assert!(restored.take_public_events().is_empty());
    assert_eq!(
        restored.rebind_dance_controller(
            InstanceAddress {
                host_scope: HostScopeId(988),
                ..controller
            },
            InvokerId(700)
        ),
        Err(Error::WrongScope)
    );
    assert_eq!(
        restored.rebind_dance_controller(controller, InvokerId(701)),
        Err(Error::RecipientMismatch)
    );
    restored
        .rebind_dance_controller(controller, InvokerId(700))
        .unwrap();
    restored
        .receive(
            &Auth,
            A_NEW,
            text_message(current, DANCE_FLOOR_PLUGIN, 2, "press_button", "-0\0"),
        )
        .unwrap();
    assert_eq!(
        restored.take_public_events()[0].source_event(),
        (InvokerId(700), 0, vec![1234])
    );
    let timer = restored
        .rebind(
            &Auth,
            B,
            timer.instance_address(),
            TimerRegisters([0, 1, 1, 9]),
        )
        .unwrap();
    assert_eq!(drain(&mut restored, timer, B)[1].1, vec![0, 1, 1, 9]);
}

#[test]
fn format_two_rejects_every_truncation_schema_length_and_record_count_bomb() {
    let mut host = host();
    let mut provider = Provider::default();
    let mut store = Store::default();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, scoreboard()))
        .unwrap();
    drain(&mut host, ticket, A);
    host.take_public_events();
    host.drive_persistence(&mut provider).unwrap();
    drain(&mut host, ticket, A);
    host.receive(
        &Auth,
        A,
        text_message(
            ticket,
            SCOREBOARD_PLUGIN,
            1,
            "scoreboard_setscore",
            "RHS,17",
        ),
    )
    .unwrap();
    host.take_public_events();
    drain(&mut host, ticket, A);
    let stamp = host.checkpoint_to(&mut store).unwrap();
    let original = store.0.clone();
    let limits = HostLimits {
        max_checkpoint_bytes: 4096,
        ..HostLimits::default()
    };
    for end in 0..original.len() {
        store.0 = original[..end].to_vec();
        assert!(
            NativeHost::restore_from(&mut store, identity(2), stamp, limits.clone()).is_err(),
            "accepted truncated checkpoint at {end}"
        );
    }
    for (offset, bytes, expected) in [
        (
            132,
            2u16.to_le_bytes().to_vec(),
            Error::UnsupportedPluginSchema,
        ),
        (
            134,
            u32::MAX.to_le_bytes().to_vec(),
            Error::InvalidCheckpoint,
        ),
        (
            64,
            u32::MAX.to_le_bytes().to_vec(),
            Error::InvalidCheckpoint,
        ),
    ] {
        store.0 = original.clone();
        store.0[offset..offset + bytes.len()].copy_from_slice(&bytes);
        assert_eq!(
            NativeHost::restore_from(&mut store, identity(2), stamp, limits.clone()).unwrap_err(),
            expected
        );
    }
    store.0 = original;
    store.0.push(0);
    assert_eq!(
        NativeHost::restore_from(&mut store, identity(2), stamp, limits).unwrap_err(),
        Error::InvalidCheckpoint
    );
}

#[test]
fn premature_door_attempt_closes_without_loading_or_revealing_a_code() {
    let mut host = host();
    let ticket = host
        .connect_plugin(&Auth, req(A, 101, door(DoorMode::CodeInput, false)))
        .unwrap();
    host.take_public_events();
    assert_eq!(
        host.receive(
            &Auth,
            A,
            text_message(ticket, PERMISSION_DOOR_PLUGIN, 1, "try_code", "123")
        ),
        Ok(DispatchOutcome::Closed)
    );
    assert_eq!(
        host.take_public_events(),
        [PublicVmEvent::Disconnected {
            invoker: InvokerId(101)
        }]
    );
    assert_eq!(drain(&mut host, ticket, A), [("eod_leave".into(), vec![])]);
    assert_eq!(
        host.drive_persistence(&mut Provider::default()).unwrap(),
        PersistenceProgress::default()
    );
}

#[test]
fn restored_normal_sign_writer_cannot_change_baseline_permissions_even_if_actor_is_owner() {
    for owner_authorized in [false, true] {
        let mut host = host();
        let mut provider = Provider::default();
        let mut store = Store::default();
        let key = PluginDataKey {
            scope: HostScopeId(987),
            plugin: SIGNS_PLUGIN,
            persistent_object: 78,
        };
        provider.data.insert(key, (1, vec![15, 0, 0]));
        let ticket = host
            .connect_plugin(
                &Auth,
                req(A, 101, sign(SignsMode::Write, owner_authorized, true, 20)),
            )
            .unwrap();
        drain(&mut host, ticket, A);
        host.take_public_events();
        host.drive_persistence(&mut provider).unwrap();
        host.tick(&Auth, &NoRegisters).unwrap();
        drain(&mut host, ticket, A);
        host.receive(
            &Auth,
            A,
            msg(
                ticket,
                SIGNS_PLUGIN,
                1,
                "set_message",
                WirePayload::Binary(&[63, 0, 1, b'x']),
            ),
        )
        .unwrap();
        host.take_public_events();
        let stamp = host.checkpoint_to(&mut store).unwrap();
        // The SGN1 current data begins 13 bytes after its payload at138.
        assert_eq!(store.0[151], 15);
        store.0[151] = 63;
        let pending_flags = store.0.len() - 4;
        assert_eq!(store.0[pending_flags], 15);
        store.0[pending_flags] = 63;
        assert_eq!(
            NativeHost::restore_from(&mut store, identity(2), stamp, HostLimits::default())
                .unwrap_err(),
            Error::InvalidCheckpoint
        );
        assert_eq!(provider.data[&key], (1, vec![15, 0, 0]));
        assert_eq!(provider.calls, 0);
    }
}
