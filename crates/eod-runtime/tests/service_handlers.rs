use std::collections::VecDeque;
use wonderland_eod_runtime::{
    native_provider::*,
    plugins::{
        ProviderOperation, ProviderReply,
        service::{
            CatalogOutfit, Config, Operation, Outfit, OutfitOwner, Reply, TradeObject, VmInput,
            WardrobeKind,
        },
    },
    protocol::PROTOCOL_VERSION,
    *,
};

#[derive(Default, Clone)]
struct Store(Vec<u8>);
impl PrivateCheckpointStore for Store {
    fn write_private(&mut self, _: PrivateCheckpointKey, bytes: &[u8]) -> Result<(), StoreError> {
        self.0 = bytes.to_vec();
        Ok(())
    }
    fn read_private(
        &mut self,
        _: PrivateCheckpointKey,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError> {
        let n = destination.len().min(self.0.len());
        destination[..n].copy_from_slice(&self.0[..n]);
        Ok(PrivateRead {
            bytes_written: n,
            complete: n == self.0.len(),
        })
    }
}
#[derive(Default)]
struct Provider {
    steps: VecDeque<(Operation, Result<Reply, NativeProviderFailure>)>,
    calls: Vec<NativeProviderRequest>,
    bad_epoch: bool,
}
impl Provider {
    fn reply(&mut self, operation: Operation, reply: Reply) {
        self.steps.push_back((operation, Ok(reply)))
    }
    fn retry(&mut self, operation: Operation) {
        self.steps
            .push_back((operation, Err(NativeProviderFailure::Retryable)))
    }
    fn exhausted(&self) {
        assert!(
            self.steps.is_empty(),
            "all expected private operations were consumed"
        )
    }
}
impl NativeProvider for Provider {
    fn execute(
        &mut self,
        host: HostIdentity,
        request: &NativeProviderRequest,
        destination: &mut [u8],
    ) -> Result<NativeProviderReceipt, NativeProviderFailure> {
        self.calls.push(request.clone());
        let (operation, reply) = self
            .steps
            .pop_front()
            .expect("unexpected native provider operation");
        assert_eq!(request.operation(), &ProviderOperation::Service(operation));
        let bytes_written = ProviderReply::Service(reply?)
            .write_private(destination)
            .unwrap();
        Ok(NativeProviderReceipt {
            host: if self.bad_epoch {
                HostIdentity {
                    epoch: host.epoch + 1,
                    ..host
                }
            } else {
                host
            },
            id: request.id(),
            bytes_written,
            complete: true,
        })
    }
}
struct Client {
    ticket: SessionTicket,
    connection: ConnectionId,
    plugin: PluginId,
    sequence: u64,
}
impl Client {
    fn send(
        &mut self,
        h: &mut NativeHost,
        event: &str,
        payload: WirePayload<'_>,
    ) -> Result<DispatchOutcome, Error> {
        let result = h.receive(
            &Auth,
            self.connection,
            ClientMessage {
                version: PROTOCOL_VERSION,
                ticket: self.ticket,
                plugin: self.plugin,
                sequence: self.sequence,
                event,
                payload,
            },
        );
        if result.is_ok() {
            self.sequence += 1
        }
        result
    }
    fn text(
        &mut self,
        h: &mut NativeHost,
        event: &str,
        body: &str,
    ) -> Result<DispatchOutcome, Error> {
        self.send(h, event, WirePayload::Text(body))
    }
    fn binary(
        &mut self,
        h: &mut NativeHost,
        event: &str,
        body: &[u8],
    ) -> Result<DispatchOutcome, Error> {
        self.send(h, event, WirePayload::Binary(body))
    }
    fn drain(&self, h: &mut NativeHost) -> Vec<PrivateUiMessage> {
        match h.take_private(&Auth, self.connection, self.ticket) {
            Ok(v) => v,
            Err(Error::StaleSession) => vec![],
            Err(e) => panic!("unexpected drain error {e:?}"),
        }
    }
}
fn create(h: &mut NativeHost, config: Config, object: u32) -> NativeControllerTicket {
    h.connect_native(NativeCreateRequest {
        object,
        cluster: u64::from(object),
        invoker: InvokerId(object),
        input: NativePluginInput::Service(config),
    })
    .unwrap()
}
fn join(
    h: &mut NativeHost,
    c: NativeControllerTicket,
    plugin: u32,
    avatar: u32,
    input: MemberInput,
) -> Client {
    let connection = ConnectionId(u64::from(avatar));
    let ticket = h
        .join_native(
            &Auth,
            NativeJoinRequest {
                connection,
                group: c.instance_address(),
                invoker: InvokerId(avatar + 1000),
                avatar_object: avatar as i16,
                avatar_id: avatar,
                input,
            },
        )
        .unwrap();
    Client {
        ticket,
        connection,
        plugin: PluginId(plugin),
        sequence: 1,
    }
}
fn barrier(h: &mut NativeHost, s: &mut Store, clients: &[&Client]) -> CheckpointStamp {
    for c in clients {
        c.drain(h);
    }
    h.take_public_events();
    h.take_native_commands();
    h.checkpoint_to(s).unwrap()
}
fn source_events(h: &mut NativeHost) -> Vec<(InvokerId, i16, Vec<i16>)> {
    h.take_public_events()
        .iter()
        .map(PublicVmEvent::source_event)
        .collect()
}
fn text_body(messages: &[PrivateUiMessage], event: &str) -> Option<String> {
    messages.iter().find_map(|m| {
        if m.event() == event {
            match m.body() {
                UiBody::Text(s) => Some(s.to_owned()),
                _ => None,
            }
        } else {
            None
        }
    })
}
fn binary_body(messages: &[PrivateUiMessage], event: &str) -> Option<Vec<u8>> {
    messages.iter().find_map(|m| {
        if m.event() == event {
            match m.body() {
                UiBody::Binary(b) => Some(b.to_vec()),
                _ => None,
            }
        } else {
            None
        }
    })
}
fn absent() -> Reply {
    Reply::PluginData {
        exists: false,
        revision: 0,
        bytes: vec![],
    }
}
fn strings(values: &[&str]) -> Vec<u8> {
    let mut out = vec![];
    for s in values {
        let mut n = s.len();
        while n >= 128 {
            out.push((n as u8) | 128);
            n >>= 7;
        }
        out.push(n as u8);
        out.extend(s.as_bytes());
    }
    out
}
struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, c: ConnectionId) -> Option<ActorId> {
        Some(ActorId(c.0))
    }
}
struct Regs;
impl RegisterSource for Regs {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        Some(TimerRegisters([0; 4]))
    }
}
fn host() -> NativeHost {
    NativeHost::new(
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 1,
        },
        HostLimits::default(),
    )
    .unwrap()
}
#[test]
fn property_selection_keeps_source_sign_extension_and_emits_every_name_byte() {
    let mut h = host();
    let c = h
        .connect_native(NativeCreateRequest {
            object: 10,
            cluster: 10,
            invoker: InvokerId(10),
            input: NativePluginInput::Service(Config::PropertySelect),
        })
        .unwrap();
    let mut input = MemberInput::default();
    input.registers[0] = -32768;
    input.registers[1] = 1;
    let t = h
        .join_native(
            &Auth,
            NativeJoinRequest {
                connection: ConnectionId(1),
                group: c.instance_address(),
                invoker: InvokerId(11),
                avatar_object: 1,
                avatar_id: 1,
                input,
            },
        )
        .unwrap();
    h.take_private(&Auth, ConnectionId(1), t).unwrap();
    h.take_public_events();
    h.tick(&Auth, &Regs).unwrap();
    let ui = h.take_private(&Auth, ConnectionId(1), t).unwrap();
    assert_eq!(ui.len(), 1);
    assert_eq!(ui[0].event(), "property_show");
    assert_eq!(ui[0].body(), UiBody::Binary(&[0, 128, 255, 255]));
    assert_eq!(original_hex("property.show"), vec![0, 128, 255, 255]);
    h.receive(
        &Auth,
        ConnectionId(1),
        ClientMessage {
            version: PROTOCOL_VERSION,
            ticket: t,
            plugin: PluginId(0x2000),
            sequence: 1,
            event: "property_select",
            payload: WirePayload::Binary(&[0x78, 0x56, 0x34, 0x12, b'A', 0xC3, 0xA9]),
        },
    )
    .unwrap();
    let events: Vec<_> = h
        .take_public_events()
        .iter()
        .map(PublicVmEvent::source_event)
        .collect();
    assert_eq!(
        events,
        vec![
            (InvokerId(11), 1, vec![0x5678, 0x1234]),
            (InvokerId(11), 2, vec![65]),
            (InvokerId(11), 2, vec![195]),
            (InvokerId(11), 2, vec![169])
        ]
    );
}

#[test]
fn dresser_cleanup_event_precedes_disconnect_on_ui_close_and_transport_close() {
    for external in [false, true] {
        let mut h = host();
        let mut s = Store::default();
        let mut p = Provider::default();
        let c = create(
            &mut h,
            Config::Wardrobe {
                kind: WardrobeKind::Dresser,
                object: 10,
                rack_type: 0,
                defaults: [0; 3],
            },
            10,
        );
        let mut a = join(&mut h, c, 0x8B300068, 1, MemberInput::default());
        barrier(&mut h, &mut s, &[&a]);
        p.reply(
            Operation::LoadPluginData {
                object: 10,
                plugin: a.plugin,
            },
            absent(),
        );
        p.reply(
            Operation::ListOutfits {
                owner: OutfitOwner::Avatar(1),
            },
            Reply::Outfits(vec![]),
        );
        assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 2);
        a.drain(&mut h);
        h.take_public_events();
        if external {
            h.disconnect(&Auth, a.connection, a.ticket).unwrap()
        } else {
            assert_eq!(
                a.text(&mut h, "close", "").unwrap(),
                DispatchOutcome::Closed
            )
        }
        let events = source_events(&mut h);
        let cleanup = events
            .iter()
            .position(|e| *e == (InvokerId(1001), 2, vec![]))
            .unwrap();
        let disconnected = events
            .iter()
            .position(|e| *e == (InvokerId(1001), -1, vec![]))
            .unwrap();
        assert!(
            cleanup < disconnected,
            "source dresser cleanup must be accepted before its invoker disconnects: {events:?}"
        );
        assert_eq!(a.drain(&mut h).last().unwrap().event(), "eod_leave");
    }
}

#[test]
fn rack_name_denial_retains_authorized_writer_for_checkpointed_retry_after_disconnect() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::Wardrobe {
            kind: WardrobeKind::Owner,
            object: 10,
            rack_type: 0,
            defaults: [0; 3],
        },
        10,
    );
    let mut a = join(
        &mut h,
        c,
        0x2B58020B,
        1,
        MemberInput {
            owner_authorized: true,
            ..MemberInput::default()
        },
    );
    barrier(&mut h, &mut s, &[&a]);
    p.reply(
        Operation::LoadPluginData {
            object: 10,
            plugin: a.plugin,
        },
        absent(),
    );
    p.reply(
        Operation::ListOutfits {
            owner: OutfitOwner::Object(10),
        },
        Reply::Outfits(vec![]),
    );
    h.drive_native_provider(&mut p).unwrap();
    a.drain(&mut h);
    h.take_public_events();
    a.text(&mut h, "rackowner_update_name", "Autumn Wardrobe")
        .unwrap();
    a.text(&mut h, "close", "").unwrap();
    let save = Operation::SaveRackName {
        object: 10,
        avatar: 1,
        expected_revision: 0,
        name: "Autumn Wardrobe".into(),
    };
    barrier(&mut h, &mut s, &[&a]);
    p.reply(save.clone(), Reply::ProviderDenied);
    h.drive_native_provider(&mut p).unwrap();
    let stamp = barrier(&mut h, &mut s, &[&a]);
    let mut restored = NativeHost::restore_from(
        &mut s,
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 2,
        },
        stamp,
        HostLimits::default(),
    )
    .unwrap();
    let controller = restored
        .rebind_native_controller(c.instance_address(), InvokerId(10))
        .unwrap();
    restored
        .deliver_native_event(
            controller,
            InvokerId(10),
            NativeVmInput::Service(VmInput::RetryPersistence),
        )
        .unwrap();
    assert_eq!(
        restored
            .drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        1
    );
    barrier(&mut restored, &mut s, &[]);
    p.reply(save, Reply::Saved { revision: 1 });
    assert_eq!(restored.drive_native_provider(&mut p).unwrap().completed, 1);
    p.exhausted();
    assert_eq!(
        restored.rebind_native_controller(c.instance_address(), InvokerId(10)),
        Err(Error::StaleSession)
    );
}

#[test]
fn provider_predecessor_retry_blocks_later_operations_in_the_same_group() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::Wardrobe {
            kind: WardrobeKind::Customer,
            object: 10,
            rack_type: 0,
            defaults: [0; 3],
        },
        10,
    );
    let a = join(&mut h, c, 0xCB492685, 1, MemberInput::default());
    let load = Operation::LoadPluginData {
        object: 10,
        plugin: a.plugin,
    };
    let list = Operation::ListOutfits {
        owner: OutfitOwner::Object(10),
    };
    barrier(&mut h, &mut s, &[&a]);
    p.retry(load.clone());
    p.reply(list.clone(), Reply::Outfits(vec![]));
    let result = h.drive_native_provider(&mut p).unwrap();
    assert_eq!(
        p.calls.len(),
        1,
        "unversioned provider snapshots stay ordered across transport retry"
    );
    assert_eq!(result.retry_pending, 2);
    p.steps.clear();
    p.reply(load, absent());
    p.reply(list, Reply::Outfits(vec![]));
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 2);
    p.exhausted();
}

#[test]
fn newspaper_private_payload_is_checkpoint_prepared_and_current_epoch_fenced() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(&mut h, Config::Newspaper, 10);
    let mut a = join(&mut h, c, 0x1000, 1, MemberInput::default());
    assert_eq!(
        text_body(&a.drain(&mut h), "newspaper_show"),
        Some(String::new())
    );
    h.take_public_events();
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        1
    );
    assert!(p.calls.is_empty());
    let mut news = 1u32.to_le_bytes().to_vec();
    news.extend(9u32.to_le_bytes());
    news.extend(strings(&[
        "A headline",
        "A private local newspaper description",
    ]));
    news.extend(100i64.to_le_bytes());
    news.extend(200i64.to_le_bytes());
    news.extend(1u32.to_le_bytes());
    for n in [123u32, 4, 1.25f32.to_bits(), 2] {
        news.extend(n.to_le_bytes());
    }
    assert_eq!(news, original_hex("newspaper.bytes"));
    let stamp = barrier(&mut h, &mut s, &[&a]);
    assert_eq!(&s.0[..6], b"EODP\x04\0");
    let mut h = NativeHost::restore_from(
        &mut s,
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 2,
        },
        stamp,
        HostLimits::default(),
    )
    .unwrap();
    assert_eq!(h.drive_native_provider(&mut p).unwrap().retry_pending, 1);
    assert_eq!(a.text(&mut h, "close", ""), Err(Error::WrongEpoch));
    h.rebind_native_controller(c.instance_address(), InvokerId(10))
        .unwrap();
    a.ticket = h
        .rebind(
            &Auth,
            a.connection,
            a.ticket.instance_address(),
            TimerRegisters([0; 4]),
        )
        .unwrap();
    a.sequence = 1;
    a.drain(&mut h);
    p.bad_epoch = true;
    p.reply(Operation::DynamicPayouts, Reply::Newspaper(news.clone()));
    assert_eq!(
        h.drive_native_provider(&mut p),
        Err(Error::ProviderReceiptMismatch)
    );
    assert!(a.drain(&mut h).is_empty());
    p.bad_epoch = false;
    p.reply(Operation::DynamicPayouts, Reply::Newspaper(news.clone()));
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 1);
    assert_eq!(p.calls[0], p.calls[1]);
    assert_eq!(binary_body(&a.drain(&mut h), "newspaper_state"), Some(news));
    assert!(h.take_public_events().is_empty());
    p.exhausted();
}

#[test]
fn newspaper_malformed_legacy_lengths_leave_the_operation_pending() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(&mut h, Config::Newspaper, 10);
    let a = join(&mut h, c, 0x1000, 1, MemberInput::default());
    barrier(&mut h, &mut s, &[&a]);
    for bytes in [vec![], vec![255; 8], vec![0, 0, 0, 0, 0, 0, 0, 0, 1]] {
        p.reply(Operation::DynamicPayouts, Reply::Newspaper(bytes));
        assert!(matches!(
            h.drive_native_provider(&mut p),
            Err(Error::InvalidPluginData)
        ));
        assert!(a.drain(&mut h).is_empty());
    }
    p.reply(Operation::DynamicPayouts, Reply::Newspaper(vec![0; 8]));
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 1);
    assert_eq!(
        binary_body(&a.drain(&mut h), "newspaper_state"),
        Some(vec![0; 8])
    );
    assert!(p.calls.windows(2).all(|x| x[0] == x[1]));
}

#[test]
fn bulletin_source_latest_animation_and_once_only_posted_refresh_are_fenced() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(&mut h, Config::Bulletin { lot: 55 }, 10);
    let mut input = MemberInput::default();
    input.registers[0] = 2;
    let mut a = join(&mut h, c, 0x1003, 1, input);
    assert_eq!(
        text_body(&a.drain(&mut h), "bulletin_show"),
        Some("2".into())
    );
    h.take_public_events();
    for body in ["0", "1", "2"] {
        a.text(&mut h, "bulletin_mode", body).unwrap();
    }
    assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 2, vec![])]);
    assert_eq!(
        a.text(&mut h, "S_AnimChanged", ""),
        Err(Error::EventNotAllowed)
    );
    h.deliver_native_event(
        c,
        InvokerId(10),
        NativeVmInput::Service(VmInput::BulletinAnimationFinished { seat: 0 }),
    )
    .unwrap();
    assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 4, vec![])]);
    h.deliver_native_event(
        c,
        InvokerId(10),
        NativeVmInput::Service(VmInput::BulletinAnimationFinished { seat: 0 }),
    )
    .unwrap();
    a.text(&mut h, "bulletin_mode", "1").unwrap();
    assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 3, vec![])]);
    a.text(&mut h, "bulletin_posted", "").unwrap();
    a.text(&mut h, "bulletin_posted", "").unwrap();
    barrier(&mut h, &mut s, &[&a]);
    for _ in 0..2 {
        p.reply(
            Operation::BulletinState { lot: 55 },
            Reply::Bulletin {
                last_id: 0x87654321,
                activity: 65537,
            },
        );
    }
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 2);
    assert_eq!(
        source_events(&mut h),
        vec![
            (InvokerId(10), 1, vec![0x4321, 0x8765u16 as i16, 1]),
            (InvokerId(1001), 1, vec![0x4321, 0x8765u16 as i16, 1])
        ]
    );
    p.exhausted();
}

#[test]
fn cooldown_all_eight_source_scopes_and_community_exclusions_use_atomic_native_queries() {
    for community in [false, true] {
        for mode in 0..8 {
            let mut h = host();
            let mut s = Store::default();
            let mut p = Provider::default();
            let now = 100_000_000_000;
            let duration = 37_230_000_000;
            let c = create(
                &mut h,
                Config::Cooldown {
                    persistent_object: 10,
                    object_guid: 0x1234,
                    lot: 99,
                    category: 7,
                    community,
                    utc_ticks: now,
                },
                10,
            );
            let mut input = MemberInput::default();
            input.registers[..4].copy_from_slice(&[mode, 1, 2, 3]);
            let a = join(&mut h, c, 0x1004, 1, input);
            barrier(&mut h, &mut s, &[&a]);
            let local = mode <= 1 || community && matches!(mode, 5 | 7);
            let by_account = matches!(mode, 1 | 3 | 6 | 7);
            let category = if !local && mode >= 4 { Some(7) } else { None };
            let op = Operation::Cooldown {
                object: 10,
                object_guid: 0x1234,
                lot: 99,
                category,
                local,
                by_account,
                avatar: 1,
                now_ticks: now,
                duration_ticks: duration,
            };
            p.reply(
                op,
                Reply::Cooldown {
                    allowed: mode % 2 == 0,
                    expires_ticks: now + 65 * 10_000_000,
                },
            );
            h.drive_native_provider(&mut p).unwrap();
            let base = if local {
                1
            } else if category.is_some() {
                3
            } else {
                5
            };
            let code = base + i16::from(by_account) + if mode % 2 == 0 { 100 } else { 0 };
            assert_eq!(
                source_events(&mut h),
                vec![(
                    InvokerId(1001),
                    code,
                    if mode % 2 == 0 {
                        vec![1, 2, 3]
                    } else {
                        vec![0, 1, 5]
                    }
                )]
            );
            assert!(a.drain(&mut h).is_empty());
            assert_eq!(
                h.deliver_native_event(
                    c,
                    InvokerId(10),
                    NativeVmInput::Service(VmInput::SetUtcTicks(now - 1))
                ),
                Err(Error::InvalidPluginInput)
            );
            h.deliver_native_event(
                c,
                InvokerId(10),
                NativeVmInput::Service(VmInput::SetUtcTicks(now + 1)),
            )
            .unwrap();
            p.exhausted();
        }
    }
}

#[test]
fn cooldown_legacy_local_account_sharing_expiry_and_bounds_are_explicit() {
    use wonderland_eod_runtime::plugins::service::{
        CooldownRecord, encode_local_cooldowns, evaluate_local_cooldown,
    };
    let rows = [
        CooldownRecord {
            avatar: 1,
            account: 10,
            expiry_ticks: 500,
        },
        CooldownRecord {
            avatar: 2,
            account: 20,
            expiry_ticks: 300,
        },
    ];
    let bytes = encode_local_cooldowns(&rows).unwrap();
    assert_eq!(bytes, original_hex("cooldown.tuples"));
    assert_eq!(bytes.len(), 32);
    let denied = evaluate_local_cooldown(&bytes, 3, 10, true, 100, 50).unwrap();
    assert!(!denied.allowed);
    assert_eq!(denied.expiry_ticks, 500);
    assert_eq!(denied.replacement, rows);
    let allowed = evaluate_local_cooldown(&bytes, 3, 10, true, 500, 50).unwrap();
    assert!(allowed.allowed);
    assert_eq!(
        allowed.replacement,
        vec![
            rows[1],
            CooldownRecord {
                avatar: 3,
                account: 10,
                expiry_ticks: 550
            }
        ]
    );
    assert!(evaluate_local_cooldown(&bytes[..31], 3, 10, true, 100, 50).is_err());
    assert!(evaluate_local_cooldown(&[], 1, 1, false, 0, -1).is_err());
    assert!(evaluate_local_cooldown(&[], 1, 0, true, 0, 1).is_err());
}

#[test]
fn trunk_requires_trusted_collection_then_closes_after_native_equip_and_costume_fallback() {
    for (kind, gender, requested, selected, path) in [
        (0, 0, "123", 123, "wedding_male.col"),
        (
            5,
            1,
            "not an asset",
            6_000_069_312_525,
            "costumes_female.col",
        ),
    ] {
        let mut h = host();
        let mut s = Store::default();
        let mut p = Provider::default();
        let c = create(&mut h, Config::Trunk { kind }, 10);
        let mut a = join(
            &mut h,
            c,
            0xAA5E36DC,
            1,
            MemberInput {
                gender,
                ..MemberInput::default()
            },
        );
        assert_eq!(
            text_body(&a.drain(&mut h), "trunk_fill_UI"),
            Some(path.into())
        );
        h.take_public_events();
        assert_eq!(
            a.text(&mut h, "trunk_wear_costume", requested),
            Err(Error::PluginNotReady)
        );
        barrier(&mut h, &mut s, &[&a]);
        p.reply(
            Operation::ReadTrunkCollection { kind, gender },
            Reply::Collection(vec![123]),
        );
        h.drive_native_provider(&mut p).unwrap();
        assert_eq!(
            a.text(&mut h, "trunk_wear_costume", requested).unwrap(),
            DispatchOutcome::Closed
        );
        let commands = h.take_native_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command,
            NativeCommand::SetOutfit {
                avatar_id: 1,
                scope: OutfitScope::DynamicCostume,
                outfit: selected
            }
        );
        assert_eq!(source_events(&mut h)[0], (InvokerId(1001), 1, vec![]));
        assert_eq!(a.drain(&mut h).last().unwrap().event(), "eod_leave");
    }
}

#[test]
fn property_output_overflow_rolls_back_sequence_and_selected_value() {
    let limits = HostLimits {
        max_public_events: 3,
        ..HostLimits::default()
    };
    let mut h = NativeHost::new(
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 1,
        },
        limits,
    )
    .unwrap();
    let c = create(&mut h, Config::PropertySelect, 10);
    let mut a = join(&mut h, c, 0x2000, 1, MemberInput::default());
    a.drain(&mut h);
    h.take_public_events();
    h.tick(&Auth, &Regs).unwrap();
    a.drain(&mut h);
    assert_eq!(
        a.binary(&mut h, "property_select", &[1, 0, 0, 0, b'A', b'B', b'C']),
        Err(Error::QueueFull)
    );
    assert!(h.take_public_events().is_empty());
    assert_eq!(a.sequence, 1);
    a.binary(&mut h, "property_select", &[2, 0, 0, 0, b'Z'])
        .unwrap();
    assert_eq!(
        source_events(&mut h),
        vec![
            (InvokerId(1001), 1, vec![2, 0]),
            (InvokerId(1001), 2, vec![90])
        ]
    );
}

fn draw_deck(title: &str, description: &str, current: u8, cards: &[(&str, u8)]) -> Vec<u8> {
    let mut b = strings(&[title, description]);
    b.push(current);
    for (text, count) in cards {
        b.extend(strings(&[text]));
        b.push(*count);
    }
    b
}
fn outfit(id: u32, asset: u64, owner: OutfitOwner, category: u8) -> Outfit {
    Outfit {
        outfit_id: id,
        asset_id: asset,
        sale_price: 250,
        purchase_price: 200,
        owner,
        outfit_type: category,
        source: 0,
    }
}
fn start_wardrobe(
    kind: WardrobeKind,
    rack_type: u8,
    defaults: [u64; 3],
    outfits: Vec<Outfit>,
) -> (NativeHost, NativeControllerTicket, Client, Store, Provider) {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::Wardrobe {
            kind,
            object: 10,
            rack_type,
            defaults,
        },
        10,
    );
    let plugin = match kind {
        WardrobeKind::Customer => 0xCB492685,
        WardrobeKind::Owner => 0x2B58020B,
        WardrobeKind::Dresser => 0x8B300068,
    };
    let a = join(
        &mut h,
        c,
        plugin,
        1,
        MemberInput {
            owner_authorized: kind == WardrobeKind::Owner,
            ..MemberInput::default()
        },
    );
    barrier(&mut h, &mut s, &[&a]);
    p.reply(
        Operation::LoadPluginData {
            object: 10,
            plugin: a.plugin,
        },
        absent(),
    );
    p.reply(
        Operation::ListOutfits {
            owner: if kind == WardrobeKind::Dresser {
                OutfitOwner::Avatar(1)
            } else {
                OutfitOwner::Object(10)
            },
        },
        Reply::Outfits(outfits),
    );
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 2);
    (h, c, a, s, p)
}
fn complete(
    h: &mut NativeHost,
    s: &mut Store,
    p: &mut Provider,
    clients: &[&Client],
    op: Operation,
    reply: Reply,
) {
    barrier(h, s, clients);
    p.reply(op, reply);
    assert_eq!(h.drive_native_provider(p).unwrap().completed, 1);
    p.exhausted();
}

#[test]
fn draw_card_owner_editor_preserves_source_counts_strings_and_compare_and_swap_bytes() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::DrawCard {
            object: 10,
            seed: 123,
        },
        10,
    );
    let mut a = join(
        &mut h,
        c,
        0x895C1CEB,
        1,
        MemberInput {
            owner_authorized: true,
            ..MemberInput::default()
        },
    );
    assert_eq!(
        binary_body(&a.drain(&mut h), "DrawCard_Please_Wait"),
        Some(vec![0])
    );
    assert_eq!(
        a.binary(&mut h, "DrawCard_Add_Card", &strings(&["Third", "255"])),
        Err(Error::PluginNotReady)
    );
    let initial = draw_deck("Lucky", "Two cards", 0, &[("First", 1), ("Second", 99)]);
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::LoadPluginData {
            object: 10,
            plugin: a.plugin,
        },
        Reply::PluginData {
            exists: true,
            revision: 7,
            bytes: initial,
        },
    );
    let ui = a.drain(&mut h);
    assert_eq!(
        binary_body(&ui, "DrawCard_Update_Deck"),
        Some(strings(&["First", "Second"]))
    );
    assert_eq!(
        binary_body(&ui, "DrawCard_Update_Deck_Numbers"),
        Some(strings(&["2", "100"]))
    );
    a.binary(&mut h, "DrawCard_Add_Card", &strings(&["Third", "255"]))
        .unwrap();
    assert_eq!(
        binary_body(&a.drain(&mut h), "DrawCard_Update_Deck_Numbers"),
        Some(strings(&["3", "355"]))
    );
    a.binary(&mut h, "DrawCard_Goto_Card", &[1]).unwrap();
    a.drain(&mut h);
    a.binary(&mut h, "DrawCard_Edit_Frequency", &[0]).unwrap();
    assert_eq!(
        binary_body(&a.drain(&mut h), "DrawCard_Update_Deck_Numbers"),
        Some(strings(&["3", "355"]))
    );
    a.binary(&mut h, "DrawCard_Edit_Frequency", &[2]).unwrap();
    a.binary(&mut h, "DrawCard_Edit_Card", &strings(&["Updated"]))
        .unwrap();
    a.binary(
        &mut h,
        "DrawCard_Edit_Game",
        &strings(&["New title", "New rules"]),
    )
    .unwrap();
    a.drain(&mut h);
    a.text(&mut h, "DrawCard_Close", "").unwrap();
    let expected = draw_deck(
        "New title",
        "New rules",
        1,
        &[("First", 1), ("Updated", 2), ("Third", 255)],
    );
    barrier(&mut h, &mut s, &[&a]);
    assert_eq!(expected, original_hex("draw.save"));
    let save = Operation::SavePluginData {
        object: 10,
        plugin: a.plugin,
        expected_revision: 7,
        bytes: expected,
    };
    // The successful save must not silently drop the VM flag-reset event merely
    // because the source dialog already disconnected its native controller.
    p.reply(save.clone(), Reply::Saved { revision: 8 });
    assert_eq!(h.drive_native_provider(&mut p), Err(Error::PluginNotReady));
    h.rebind_native_controller(c.instance_address(), InvokerId(10))
        .unwrap();
    p.reply(save, Reply::Saved { revision: 8 });
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 1);
    assert_eq!(p.calls[p.calls.len() - 1], p.calls[p.calls.len() - 2]);
    assert_eq!(source_events(&mut h), vec![(InvokerId(10), 1, vec![])]);
    p.exhausted();
}

#[test]
fn draw_card_uniform_unique_entries_ignore_frequency_and_restore_does_not_redraw() {
    for seed in 1..=12 {
        let mut chosen = vec![];
        for frequency in [1, 255] {
            let mut h = host();
            let mut s = Store::default();
            let mut p = Provider::default();
            let c = create(&mut h, Config::DrawCard { object: 10, seed }, 10);
            let mut input = MemberInput::default();
            input.registers[0] = 3;
            let mut a = join(&mut h, c, 0x895C1CEB, 1, input);
            complete(
                &mut h,
                &mut s,
                &mut p,
                &[&a],
                Operation::LoadPluginData {
                    object: 10,
                    plugin: a.plugin,
                },
                Reply::PluginData {
                    exists: true,
                    revision: 1,
                    bytes: draw_deck("T", "D", 0, &[("A", 1), ("B", frequency)]),
                },
            );
            let b = binary_body(&a.drain(&mut h), "DrawCard_Drawn").unwrap();
            chosen.push(b[1]);
            let stamp = barrier(&mut h, &mut s, &[&a]);
            let mut restored = NativeHost::restore_from(
                &mut s,
                HostIdentity {
                    scope: HostScopeId(77),
                    epoch: 2,
                },
                stamp,
                HostLimits::default(),
            )
            .unwrap();
            restored
                .rebind_native_controller(c.instance_address(), InvokerId(10))
                .unwrap();
            a.ticket = restored
                .rebind(
                    &Auth,
                    a.connection,
                    a.ticket.instance_address(),
                    TimerRegisters([0; 4]),
                )
                .unwrap();
            assert_eq!(
                binary_body(&a.drain(&mut restored), "DrawCard_Drawn"),
                Some(b)
            );
        }
        assert_eq!(
            chosen[0], chosen[1],
            "source unique-entry draw semantics for seed {seed}"
        );
    }
}

#[test]
fn dresser_source_big_endian_outfits_scope_mapping_and_delete_default_acknowledgment() {
    let first = outfit(11, 101, OutfitOwner::Avatar(1), 0);
    let second = outfit(12, 102, OutfitOwner::Avatar(1), 0);
    let head = outfit(13, 103, OutfitOwner::Avatar(1), 8);
    let (mut h, _c, mut a, mut s, mut p) = start_wardrobe(
        WardrobeKind::Dresser,
        0,
        [101, 0, 0],
        vec![first.clone(), second.clone(), head.clone()],
    );
    let wire = binary_body(&a.drain(&mut h), "set_outfits").unwrap();
    assert_eq!(&wire[..8], &[0, 0, 0, 3, 0, 0, 0, 11]);
    assert_eq!(&wire[24..26], &[0, 1]);
    assert_eq!(&wire[31..33], &[0, 0]);
    h.take_public_events();
    for (item, scope, arg) in [
        (first.clone(), OutfitScope::DynamicDaywear, 100),
        (head.clone(), OutfitScope::DecorationHead, 3),
    ] {
        a.text(&mut h, "dresser_change_outfit", &item.outfit_id.to_string())
            .unwrap();
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::ListOutfits {
                owner: OutfitOwner::Avatar(1),
            },
            Reply::Outfits(vec![item.clone()]),
        );
        assert_eq!(
            h.take_native_commands()[0].command,
            NativeCommand::SetOutfit {
                avatar_id: 1,
                scope,
                outfit: item.asset_id
            }
        );
        assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 1, vec![arg])]);
    }
    a.text(&mut h, "dresser_delete_outfit", "11").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![first.clone()]),
    );
    assert!(h.take_native_commands().is_empty());
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        0
    );
    a.text(&mut h, "dresser_delete_outfit", "11").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![first.clone(), second.clone()]),
    );
    assert!(h.take_native_commands().is_empty());
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::DeleteOutfit {
            owner: OutfitOwner::Avatar(1),
            actor: 1,
            outfit: 11,
            keep_one_in_category: true,
        },
        Reply::Mutation { success: false },
    );
    assert!(h.take_native_commands().is_empty());
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![first.clone(), second.clone()]),
    );
    a.drain(&mut h);
    a.text(&mut h, "dresser_delete_outfit", "11").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![first, second.clone()]),
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::DeleteOutfit {
            owner: OutfitOwner::Avatar(1),
            actor: 1,
            outfit: 11,
            keep_one_in_category: true,
        },
        Reply::Mutation { success: true },
    );
    assert_eq!(
        h.take_native_commands()[0].command,
        NativeCommand::SetOutfitIfCurrent {
            avatar_id: 1,
            scope: OutfitScope::DefaultDaywear,
            expected: 101,
            outfit: 102
        }
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![second]),
    );
    a.drain(&mut h);
}

#[test]
fn rack_purchase_distinguishes_duplicate_category_limit_and_atomic_success() {
    for error in [Some(0), Some(1), Some(2), None] {
        let rack = outfit(11, 101, OutfitOwner::Object(10), 0);
        let (mut h, _c, mut a, mut s, mut p) =
            start_wardrobe(WardrobeKind::Customer, 0, [0; 3], vec![rack.clone()]);
        a.drain(&mut h);
        h.take_public_events();
        a.text(&mut h, "rack_try_outfit_on", "11").unwrap();
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::ListOutfits {
                owner: OutfitOwner::Object(10),
            },
            Reply::Outfits(vec![rack.clone()]),
        );
        assert_eq!(
            h.take_native_commands()[0].command,
            NativeCommand::SetOutfit {
                avatar_id: 1,
                scope: OutfitScope::DynamicCostume,
                outfit: 101
            }
        );
        assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 1, vec![0])]);
        a.text(&mut h, "rack_purchase", "11,true").unwrap();
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::ListOutfits {
                owner: OutfitOwner::Object(10),
            },
            Reply::Outfits(vec![rack.clone()]),
        );
        let owned = match error {
            Some(0) => vec![outfit(21, 101, OutfitOwner::Avatar(1), 0)],
            Some(1) => (0..5)
                .map(|i| outfit(21 + i, 200 + u64::from(i), OutfitOwner::Avatar(1), 0))
                .collect(),
            _ => vec![],
        };
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::ListOutfits {
                owner: OutfitOwner::Avatar(1),
            },
            Reply::Outfits(owned),
        );
        assert!(h.take_native_commands().is_empty());
        if matches!(error, Some(0 | 1)) {
            assert_eq!(
                binary_body(&a.drain(&mut h), "rack_buy_error"),
                Some(vec![error.unwrap()])
            );
            assert_eq!(
                h.drive_native_provider(&mut p)
                    .unwrap()
                    .waiting_for_checkpoint,
                0
            );
            continue;
        }
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::PurchaseOutfit {
                object: 10,
                avatar: 1,
                outfit: 11,
                asset: 101,
                price: 250,
            },
            Reply::Mutation {
                success: error.is_none(),
            },
        );
        if error == Some(2) {
            assert_eq!(
                binary_body(&a.drain(&mut h), "rack_buy_error"),
                Some(vec![2])
            );
            assert!(h.take_native_commands().is_empty());
        } else {
            assert_eq!(
                h.take_native_commands()[0].command,
                NativeCommand::SetOutfit {
                    avatar_id: 1,
                    scope: OutfitScope::DefaultDaywear,
                    outfit: 101
                }
            );
            assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 3, vec![0])]);
            complete(
                &mut h,
                &mut s,
                &mut p,
                &[&a],
                Operation::ListOutfits {
                    owner: OutfitOwner::Object(10),
                },
                Reply::Outfits(vec![]),
            );
            assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 8, vec![0])]);
        }
    }
}

#[test]
fn rack_owner_rejects_unauthorized_join_and_stocks_only_canonical_catalog_price() {
    let mut h = host();
    let c = create(
        &mut h,
        Config::Wardrobe {
            kind: WardrobeKind::Owner,
            object: 10,
            rack_type: 3,
            defaults: [0; 3],
        },
        10,
    );
    assert_eq!(
        h.join_native(
            &Auth,
            NativeJoinRequest {
                connection: ConnectionId(1),
                group: c.instance_address(),
                invoker: InvokerId(1001),
                avatar_object: 1,
                avatar_id: 1,
                input: MemberInput::default()
            }
        ),
        Err(Error::NotAuthorized)
    );
    let (mut h, _c, mut a, mut s, mut p) = start_wardrobe(WardrobeKind::Owner, 3, [0; 3], vec![]);
    a.drain(&mut h);
    h.take_public_events();
    for value in ["11,0", "11,-1", "11,1000000", "bad"] {
        a.text(&mut h, "rackowner_update_price", value).unwrap();
    }
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        0
    );
    a.text(&mut h, "rackowner_stock", "991").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ReadCatalog { rack_type: 3 },
        Reply::Catalog(vec![CatalogOutfit {
            asset_id: 991,
            price: 1234,
            outfit_type: 5,
        }]),
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::StockOutfit {
            object: 10,
            avatar: 1,
            asset: 991,
            price: 1234,
            outfit_type: 5,
        },
        Reply::Mutation { success: true },
    );
    assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 4, vec![0])]);
    let stock = outfit(11, 991, OutfitOwner::Object(10), 5);
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Object(10),
        },
        Reply::Outfits(vec![stock.clone()]),
    );
    assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 8, vec![1])]);
    a.drain(&mut h);
    a.text(&mut h, "rackowner_update_price", "11,999999")
        .unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::UpdateOutfitPrice {
            object: 10,
            avatar: 1,
            outfit: 11,
            price: 999999,
        },
        Reply::Mutation { success: true },
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Object(10),
        },
        Reply::Outfits(vec![stock]),
    );
    a.drain(&mut h);
    a.text(&mut h, "rackowner_delete", "11").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::DeleteOutfit {
            owner: OutfitOwner::Object(10),
            actor: 1,
            outfit: 11,
            keep_one_in_category: false,
        },
        Reply::Mutation { success: true },
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Object(10),
        },
        Reply::Outfits(vec![]),
    );
    assert_eq!(source_events(&mut h), vec![(InvokerId(1001), 8, vec![0])]);
}

fn inventory(id: u32, guid: u32) -> TradeObject {
    TradeObject {
        guid,
        persistent_id: id,
        data: vec![1, 2, 3],
        lot_id: 0,
        object_count: 0,
        object_value: 0,
        lot_name: String::new(),
    }
}
fn trade_setup() -> (
    NativeHost,
    NativeControllerTicket,
    Client,
    Client,
    Store,
    Provider,
) {
    let mut h = host();
    let c = create(
        &mut h,
        Config::SecureTrade {
            untradable_guids: vec![0xBAD],
        },
        10,
    );
    let a = join(&mut h, c, 0x897F82F5, 1, MemberInput::default());
    let b = join(&mut h, c, 0x897F82F5, 2, MemberInput::default());
    a.drain(&mut h);
    b.drain(&mut h);
    h.take_public_events();
    (h, c, a, b, Store::default(), Provider::default())
}
fn trade_ticks(h: &mut NativeHost, a: &Client, b: &Client, count: u32) {
    for _ in 0..count {
        h.tick(&Auth, &Regs).unwrap();
        a.drain(h);
        b.drain(h);
        h.take_public_events();
    }
}

#[test]
fn secure_trade_source_offers_are_private_delay_acceptance_and_commit_once_after_checkpoint() {
    let (mut h, _c, mut a, mut b, mut s, mut p) = trade_setup();
    let item = inventory(101, 0xACE);
    a.text(&mut h, "trade_offer", "i101|0").unwrap();
    assert_eq!(
        b.text(&mut h, "trade_offer", "a"),
        Err(Error::PersistencePending)
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a, &b],
        Operation::ReadInventory {
            avatar: 1,
            item: 101,
        },
        Reply::Inventory(Some(item.clone())),
    );
    let private_a = a.drain(&mut h);
    let private_b = b.drain(&mut h);
    assert_eq!(text_body(&private_a, "trade_time"), Some("5".into()));
    let own = binary_body(&private_a, "trade_me").unwrap();
    let other = binary_body(&private_b, "trade_other").unwrap();
    assert_eq!(own, other);
    assert_eq!(own, original_hex("trade.offer"));
    assert_eq!(&own[..9], &[1, 0, 0, 0, 1, 0xCE, 0x0A, 0, 0]);
    assert!(h.take_public_events().is_empty());
    a.text(&mut h, "trade_offer", "a").unwrap();
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        0
    );
    trade_ticks(&mut h, &a, &b, 149);
    a.text(&mut h, "trade_offer", "a").unwrap();
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        0
    );
    trade_ticks(&mut h, &a, &b, 1);
    a.text(&mut h, "trade_offer", "a").unwrap();
    a.drain(&mut h);
    b.drain(&mut h);
    b.text(&mut h, "trade_offer", "a").unwrap();
    assert_eq!(
        text_body(&a.drain(&mut h), "trade_inprogress"),
        Some(String::new())
    );
    assert_eq!(
        text_body(&b.drain(&mut h), "trade_inprogress"),
        Some(String::new())
    );
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        1
    );
    let make_offer = |avatar, items| wonderland_eod_runtime::plugins::service::TradeOffer {
        avatar,
        items,
        money: 0,
        accepted: true,
    };
    let offers = [
        make_offer(1, [Some(item), None, None, None, None]),
        make_offer(2, [None, None, None, None, None]),
    ];
    let op = Operation::SecureTrade {
        offers,
        untradable: vec![0xBAD],
    };
    barrier(&mut h, &mut s, &[&a, &b]);
    p.retry(op.clone());
    assert_eq!(h.drive_native_provider(&mut p).unwrap().retry_pending, 1);
    a.text(&mut h, "trade_offer", "m999999").unwrap();
    assert!(a.drain(&mut h).is_empty());
    p.reply(op, Reply::Trade { error: 0 });
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 1);
    assert_eq!(p.calls[p.calls.len() - 1], p.calls[p.calls.len() - 2]);
    assert_eq!(
        text_body(&a.drain(&mut h), "trade_message"),
        Some("0|14".into())
    );
    assert_eq!(
        text_body(&b.drain(&mut h), "trade_message"),
        Some("0|14".into())
    );
    h.tick(&Auth, &Regs).unwrap();
    assert_eq!(a.drain(&mut h).last().unwrap().event(), "eod_leave");
    assert_eq!(b.drain(&mut h).last().unwrap().event(), "eod_leave");
}

#[test]
fn secure_trade_zero_money_reset_untradable_and_property_slot_zero_are_regressed() {
    let (mut h, _c, mut a, b, mut s, mut p) = trade_setup();
    a.text(&mut h, "trade_offer", "m50").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a, &b],
        Operation::CheckFunds {
            avatar: 1,
            other: 2,
            amount: 50,
        },
        Reply::Funds {
            avatar: 1,
            other: 2,
            amount: 50,
            success: true,
        },
    );
    a.drain(&mut h);
    b.drain(&mut h);
    a.text(&mut h, "trade_offer", "m0").unwrap();
    let own = binary_body(&a.drain(&mut h), "trade_me").unwrap();
    assert_eq!(&own[own.len() - 5..], &[0; 5]);
    b.drain(&mut h);
    a.text(&mut h, "trade_offer", "i99|0").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a, &b],
        Operation::ReadInventory {
            avatar: 1,
            item: 99,
        },
        Reply::Inventory(Some(inventory(99, 0xBAD))),
    );
    assert_eq!(text_body(&a.drain(&mut h), "trade_error"), Some("7".into()));
    b.drain(&mut h);
    a.text(&mut h, "trade_offer", "po0").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a, &b],
        Operation::ReadProperty {
            avatar: 1,
            with_objects: true,
            untradable: vec![0xBAD],
        },
        Reply::Property {
            lot_id: 55,
            object_count: 2,
            object_value: 123,
            lot_name: "Home".into(),
        },
    );
    a.drain(&mut h);
    b.drain(&mut h);
    a.text(&mut h, "trade_offer", "po1").unwrap();
    assert_eq!(text_body(&a.drain(&mut h), "trade_error"), Some("4".into()));
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        0
    );
    a.text(&mut h, "trade_offer", "po5").unwrap();
    assert_eq!(
        h.drive_native_provider(&mut p)
            .unwrap()
            .waiting_for_checkpoint,
        0
    );
}

#[test]
fn secure_trade_prepared_transaction_survives_both_departures_and_private_restore() {
    let (mut h, c, mut a, mut b, mut s, mut p) = trade_setup();
    a.text(&mut h, "trade_offer", "a").unwrap();
    b.text(&mut h, "trade_offer", "a").unwrap();
    let empty = |avatar| wonderland_eod_runtime::plugins::service::TradeOffer {
        avatar,
        items: [None, None, None, None, None],
        money: 0,
        accepted: true,
    };
    let op = Operation::SecureTrade {
        offers: [empty(1), empty(2)],
        untradable: vec![0xBAD],
    };
    barrier(&mut h, &mut s, &[&a, &b]);
    h.disconnect(&Auth, a.connection, a.ticket).unwrap();
    let stamp = barrier(&mut h, &mut s, &[&a, &b]);
    let mut restored = NativeHost::restore_from(
        &mut s,
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 2,
        },
        stamp,
        HostLimits::default(),
    )
    .unwrap();
    p.reply(op, Reply::Trade { error: 0 });
    assert_eq!(restored.drive_native_provider(&mut p).unwrap().completed, 1);
    assert!(restored.take_public_events().is_empty());
    assert!(restored.take_native_commands().is_empty());
    assert_eq!(
        restored.rebind_native_controller(c.instance_address(), InvokerId(10)),
        Err(Error::StaleSession)
    );
    assert_eq!(
        restored.take_private(&Auth, b.connection, b.ticket),
        Err(Error::WrongEpoch)
    );
}

#[test]
fn provider_private_output_overflow_retains_exact_prepared_request_for_replay() {
    let mut h = NativeHost::new(
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 1,
        },
        HostLimits {
            max_private_messages: 2,
            ..HostLimits::default()
        },
    )
    .unwrap();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(&mut h, Config::Newspaper, 10);
    let a = join(&mut h, c, 0x1000, 1, MemberInput::default());
    barrier(&mut h, &mut s, &[&a]);
    let d = create(&mut h, Config::PropertySelect, 20);
    let b = join(&mut h, d, 0x2000, 2, MemberInput::default());
    h.tick(&Auth, &Regs).unwrap();
    p.reply(Operation::DynamicPayouts, Reply::Newspaper(vec![0; 8]));
    assert_eq!(h.drive_native_provider(&mut p), Err(Error::QueueFull));
    assert!(a.drain(&mut h).is_empty());
    assert_eq!(b.drain(&mut h).len(), 2);
    p.reply(Operation::DynamicPayouts, Reply::Newspaper(vec![0; 8]));
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 1);
    assert_eq!(p.calls[0], p.calls[1]);
    assert_eq!(
        binary_body(&a.drain(&mut h), "newspaper_state"),
        Some(vec![0; 8])
    );
}

type CheckpointSections = (Vec<(usize, usize)>, Vec<(usize, usize)>);
fn checkpoint_sections(bytes: &[u8]) -> CheckpointSections {
    fn u32_at(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
    }
    assert_eq!(&bytes[..8], b"EODP\x04\0\x01\0");
    let mut at = 12 + u32_at(bytes, 8);
    let members = u32_at(bytes, at);
    at += 4 + members * 127;
    let groups = u32_at(bytes, at);
    at += 4;
    let mut kernels = vec![];
    for _ in 0..groups {
        at += 157;
        let size = u32_at(bytes, at);
        at += 4;
        kernels.push((at, size));
        at += size;
    }
    at += 8;
    let ops = u32_at(bytes, at);
    at += 4;
    let mut operations = vec![];
    for _ in 0..ops {
        at += 48;
        let size = u32_at(bytes, at);
        at += 4;
        operations.push((at, size));
        at += size;
    }
    assert_eq!(at, bytes.len());
    (kernels, operations)
}
#[test]
fn native_checkpoint_rejects_mismatched_kernel_object_callback_and_journal_before_dispatch() {
    let (mut h, _c, a, mut s, _p) = start_wardrobe(WardrobeKind::Customer, 0, [0; 3], vec![]);
    a.drain(&mut h);
    h.take_public_events();
    let mut a = a;
    a.text(&mut h, "rack_try_outfit_on", "11").unwrap();
    let stamp = barrier(&mut h, &mut s, &[&a]);
    let (kernels, operations) = checkpoint_sections(&s.0);
    assert_eq!(kernels.len(), 1);
    assert_eq!(operations.len(), 1);
    let mut corrupt = s.clone();
    let pos = operations[0].0;
    assert_eq!(&corrupt.0[pos..pos + 3], &[3, 6, 2]);
    corrupt.0[pos + 3..pos + 7].copy_from_slice(&999u32.to_le_bytes());
    assert!(matches!(
        NativeHost::restore_from(
            &mut corrupt,
            HostIdentity {
                scope: HostScopeId(77),
                epoch: 2
            },
            stamp,
            HostLimits::default()
        ),
        Err(Error::InvalidCheckpoint)
    ));
    let mut corrupt = s.clone();
    let pos = kernels[0].0;
    assert_eq!(&corrupt.0[pos..pos + 2], &[2, 0]);
    corrupt.0[pos + 2..pos + 6].copy_from_slice(&999u32.to_le_bytes());
    assert!(matches!(
        NativeHost::restore_from(
            &mut corrupt,
            HostIdentity {
                scope: HostScopeId(77),
                epoch: 2
            },
            stamp,
            HostLimits::default()
        ),
        Err(Error::InvalidCheckpoint)
    ));
    let mut corrupt = s.clone();
    let callback = operations[0].0 - 4 - 8;
    corrupt.0[callback..callback + 8].copy_from_slice(&0u64.to_le_bytes());
    assert!(matches!(
        NativeHost::restore_from(
            &mut corrupt,
            HostIdentity {
                scope: HostScopeId(77),
                epoch: 2
            },
            stamp,
            HostLimits::default()
        ),
        Err(Error::InvalidCheckpoint)
    ));
    let mut valid = NativeHost::restore_from(
        &mut s,
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 2,
        },
        stamp,
        HostLimits::default(),
    )
    .unwrap();
    assert_eq!(
        valid
            .drive_native_provider(&mut Provider::default())
            .unwrap()
            .retry_pending,
        1
    );
}

fn original_value(key: &str) -> &'static str {
    include_str!("../../../fixtures/eod/service/expected.txt")
        .lines()
        .find_map(|line| {
            line.split_once('=')
                .filter(|(k, _)| *k == key)
                .map(|(_, value)| value)
        })
        .expect("pinned original source literal")
}
fn original_hex(key: &str) -> Vec<u8> {
    let text = original_value(key);
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn draw_utf16_display_boundary_keeps_valid_unicode_where_original_writer_throws() {
    assert_eq!(
        original_value("draw.source_utf16"),
        "EncoderFallbackException"
    );
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::DrawCard {
            object: 10,
            seed: 1,
        },
        10,
    );
    let mut a = join(
        &mut h,
        c,
        0x895C1CEB,
        1,
        MemberInput {
            owner_authorized: true,
            ..MemberInput::default()
        },
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::LoadPluginData {
            object: 10,
            plugin: a.plugin,
        },
        Reply::PluginData {
            exists: true,
            revision: 1,
            bytes: draw_deck("T", "D", 0, &[]),
        },
    );
    a.drain(&mut h);
    let prefix = "a".repeat(39);
    let full = format!("{prefix}😀Z");
    a.binary(&mut h, "DrawCard_Add_Card", &strings(&[&full, "1"]))
        .unwrap();
    assert_eq!(
        binary_body(&a.drain(&mut h), "DrawCard_Update_Deck"),
        Some(strings(&[&prefix]))
    );
}

#[test]
fn dresser_name_load_denial_terminates_instead_of_leaving_an_unrecoverable_dialog() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::Wardrobe {
            kind: WardrobeKind::Owner,
            object: 10,
            rack_type: 0,
            defaults: [0; 3],
        },
        10,
    );
    let a = join(
        &mut h,
        c,
        0x2B58020B,
        1,
        MemberInput {
            owner_authorized: true,
            ..MemberInput::default()
        },
    );
    barrier(&mut h, &mut s, &[&a]);
    p.reply(
        Operation::LoadPluginData {
            object: 10,
            plugin: a.plugin,
        },
        Reply::ProviderDenied,
    );
    p.reply(
        Operation::ListOutfits {
            owner: OutfitOwner::Object(10),
        },
        Reply::Outfits(vec![]),
    );
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 2);
    assert_eq!(a.drain(&mut h).last().unwrap().event(), "eod_leave");
    assert_eq!(
        h.rebind_native_controller(c.instance_address(), InvokerId(10)),
        Err(Error::StaleSession)
    );
}

#[test]
fn pending_dresser_default_delete_reserves_avatar_and_fences_detached_vm_commands() {
    let first = outfit(11, 101, OutfitOwner::Avatar(1), 0);
    let second = outfit(12, 102, OutfitOwner::Avatar(1), 0);
    let (mut h, c, mut a, mut s, mut p) = start_wardrobe(
        WardrobeKind::Dresser,
        0,
        [101, 0, 0],
        vec![first.clone(), second.clone()],
    );
    a.drain(&mut h);
    h.take_public_events();
    a.text(&mut h, "dresser_delete_outfit", "11").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![first, second]),
    );
    barrier(&mut h, &mut s, &[&a]);
    h.disconnect(&Auth, a.connection, a.ticket).unwrap();
    a.drain(&mut h);
    h.take_public_events();
    let other = create(&mut h, Config::PropertySelect, 20);
    assert_eq!(
        h.join_native(
            &Auth,
            NativeJoinRequest {
                connection: ConnectionId(1),
                group: other.instance_address(),
                invoker: InvokerId(1001),
                avatar_object: 1,
                avatar_id: 1,
                input: MemberInput::default()
            }
        ),
        Err(Error::PersistencePending)
    );
    let stamp = barrier(&mut h, &mut s, &[&a]);
    let mut h = NativeHost::restore_from(
        &mut s,
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 2,
        },
        stamp,
        HostLimits::default(),
    )
    .unwrap();
    let deletion = Operation::DeleteOutfit {
        owner: OutfitOwner::Avatar(1),
        actor: 1,
        outfit: 11,
        keep_one_in_category: true,
    };
    p.reply(deletion.clone(), Reply::Mutation { success: true });
    assert_eq!(h.drive_native_provider(&mut p), Err(Error::PluginNotReady));
    assert!(h.take_native_commands().is_empty());
    h.rebind_native_controller(c.instance_address(), InvokerId(10))
        .unwrap();
    p.reply(deletion, Reply::Mutation { success: true });
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 1);
    let cmds = h.take_native_commands();
    assert_eq!(cmds.len(), 1);
    assert_eq!(
        cmds[0].command,
        NativeCommand::SetOutfitIfCurrent {
            avatar_id: 1,
            scope: OutfitScope::DefaultDaywear,
            expected: 101,
            outfit: 102
        }
    );
    assert_eq!(p.calls[p.calls.len() - 1], p.calls[p.calls.len() - 2]);
}

#[test]
fn late_dresser_delete_compares_original_asset_and_preserves_newer_vm_default() {
    for initial in [101, 103] {
        let outfits = vec![
            outfit(11, 101, OutfitOwner::Avatar(1), 0),
            outfit(12, 102, OutfitOwner::Avatar(1), 0),
            outfit(13, 103, OutfitOwner::Avatar(1), 0),
        ];
        let (mut h, c, mut a, mut s, mut p) =
            start_wardrobe(WardrobeKind::Dresser, 0, [initial, 0, 0], outfits.clone());
        a.drain(&mut h);
        h.take_public_events();
        a.text(&mut h, "dresser_delete_outfit", "11").unwrap();
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::ListOutfits {
                owner: OutfitOwner::Avatar(1),
            },
            Reply::Outfits(outfits),
        );
        // A trusted later VM choice changes C while the durable deletion of A is pending.
        h.deliver_native_event(
            c,
            InvokerId(10),
            NativeVmInput::Service(VmInput::ObserveDefaultOutfits {
                avatar_id: 1,
                defaults: [103, 0, 0],
            }),
        )
        .unwrap();
        complete(
            &mut h,
            &mut s,
            &mut p,
            &[&a],
            Operation::DeleteOutfit {
                owner: OutfitOwner::Avatar(1),
                actor: 1,
                outfit: 11,
                keep_one_in_category: true,
            },
            Reply::Mutation { success: true },
        );
        let commands = h.take_native_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command,
            NativeCommand::SetOutfitIfCurrent {
                avatar_id: 1,
                scope: OutfitScope::DefaultDaywear,
                expected: 101,
                outfit: 102
            }
        );
        // Apply the specified adapter predicate: newer C wins, while a current A
        // (including A selected after the list response) gets the retained B.
        for (mut current, wanted) in [(103, 103), (101, 102)] {
            if let NativeCommand::SetOutfitIfCurrent {
                expected, outfit, ..
            } = commands[0].command
                && current == expected
            {
                current = outfit;
            }
            assert_eq!(current, wanted);
        }
    }
}

#[test]
fn terminal_revision_is_rejected_before_a_durable_write_is_dispatched() {
    let mut h = host();
    let mut s = Store::default();
    let mut p = Provider::default();
    let c = create(
        &mut h,
        Config::DrawCard {
            object: 10,
            seed: 1,
        },
        10,
    );
    let mut a = join(
        &mut h,
        c,
        0x895C1CEB,
        1,
        MemberInput {
            owner_authorized: true,
            ..MemberInput::default()
        },
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::LoadPluginData {
            object: 10,
            plugin: a.plugin,
        },
        Reply::PluginData {
            exists: true,
            revision: u64::MAX - 1,
            bytes: draw_deck("T", "D", 0, &[]),
        },
    );
    a.binary(&mut h, "DrawCard_Add_Card", &strings(&["card", "1"]))
        .unwrap();
    assert_eq!(
        a.text(&mut h, "DrawCard_Close", ""),
        Err(Error::CounterExhausted)
    );
    barrier(&mut h, &mut s, &[&a]);
    assert_eq!(h.drive_native_provider(&mut p).unwrap().completed, 0);
    p.exhausted();
}

#[test]
fn dresser_distinct_owned_records_with_the_same_asset_keep_a_valid_default() {
    let first = outfit(11, 101, OutfitOwner::Avatar(1), 0);
    let second = outfit(12, 101, OutfitOwner::Avatar(1), 0);
    let (mut h, _, mut a, mut s, mut p) = start_wardrobe(
        WardrobeKind::Dresser,
        0,
        [101, 0, 0],
        vec![first.clone(), second.clone()],
    );
    a.drain(&mut h);
    h.take_public_events();
    a.text(&mut h, "dresser_delete_outfit", "11").unwrap();
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::ListOutfits {
            owner: OutfitOwner::Avatar(1),
        },
        Reply::Outfits(vec![first, second]),
    );
    complete(
        &mut h,
        &mut s,
        &mut p,
        &[&a],
        Operation::DeleteOutfit {
            owner: OutfitOwner::Avatar(1),
            actor: 1,
            outfit: 11,
            keep_one_in_category: true,
        },
        Reply::Mutation { success: true },
    );
    assert_eq!(
        h.take_native_commands()[0].command,
        NativeCommand::SetOutfitIfCurrent {
            avatar_id: 1,
            scope: OutfitScope::DefaultDaywear,
            expected: 101,
            outfit: 101
        }
    );
}

fn ui_signature(messages: Vec<PrivateUiMessage>) -> Vec<(String, u8, Vec<u8>)> {
    messages
        .into_iter()
        .map(|m| {
            let (kind, body) = match m.body() {
                UiBody::Text(s) => (0, s.as_bytes().to_vec()),
                UiBody::Binary(b) => (1, b.to_vec()),
            };
            (m.event().to_owned(), kind, body)
        })
        .collect()
}

#[test]
fn all_service_initial_reads_and_property_state_resume_identically_after_private_restore() {
    for plugin in [
        0x1000, 0x1003, 0x1004, 0x2000, 0xAA5E36DC, 0x895C1CEB, 0xCB492685, 0x2B58020B, 0x8B300068,
    ] {
        let mut input = MemberInput::default();
        let (config, replies) = match plugin {
            0x1000 => (
                Config::Newspaper,
                vec![(Operation::DynamicPayouts, Reply::Newspaper(vec![0; 8]))],
            ),
            0x1003 => {
                input.registers[0] = 2;
                (
                    Config::Bulletin { lot: 55 },
                    vec![(
                        Operation::BulletinState { lot: 55 },
                        Reply::Bulletin {
                            last_id: 123,
                            activity: 17,
                        },
                    )],
                )
            }
            0x1004 => {
                input.registers[1] = 1;
                (
                    Config::Cooldown {
                        persistent_object: 10,
                        object_guid: 123,
                        lot: 55,
                        category: 2,
                        community: false,
                        utc_ticks: 100_000_000_000,
                    },
                    vec![(
                        Operation::Cooldown {
                            object: 10,
                            object_guid: 123,
                            lot: 55,
                            category: None,
                            local: true,
                            by_account: false,
                            avatar: 1,
                            now_ticks: 100_000_000_000,
                            duration_ticks: 36_000_000_000,
                        },
                        Reply::Cooldown {
                            allowed: true,
                            expires_ticks: 136_000_000_000,
                        },
                    )],
                )
            }
            0x2000 => (Config::PropertySelect, vec![]),
            0xAA5E36DC => (
                Config::Trunk { kind: 0 },
                vec![(
                    Operation::ReadTrunkCollection { kind: 0, gender: 0 },
                    Reply::Collection(vec![101, 303]),
                )],
            ),
            0x895C1CEB => {
                input.owner_authorized = true;
                (
                    Config::DrawCard {
                        object: 10,
                        seed: 123,
                    },
                    vec![(
                        Operation::LoadPluginData {
                            object: 10,
                            plugin: PluginId(plugin),
                        },
                        absent(),
                    )],
                )
            }
            _ => {
                let kind = match plugin {
                    0xCB492685 => WardrobeKind::Customer,
                    0x2B58020B => WardrobeKind::Owner,
                    _ => WardrobeKind::Dresser,
                };
                input.owner_authorized = kind == WardrobeKind::Owner;
                let owner = if kind == WardrobeKind::Dresser {
                    OutfitOwner::Avatar(1)
                } else {
                    OutfitOwner::Object(10)
                };
                (
                    Config::Wardrobe {
                        kind,
                        object: 10,
                        rack_type: 0,
                        defaults: [0; 3],
                    },
                    vec![
                        (
                            Operation::LoadPluginData {
                                object: 10,
                                plugin: PluginId(plugin),
                            },
                            absent(),
                        ),
                        (
                            Operation::ListOutfits { owner },
                            Reply::Outfits(vec![outfit(11, 101, owner, 0)]),
                        ),
                    ],
                )
            }
        };
        let mut h = host();
        let mut store = Store::default();
        let mut provider = Provider::default();
        let c = create(&mut h, config, 10);
        let a = join(&mut h, c, plugin, 1, input);
        let stamp = barrier(&mut h, &mut store, &[&a]);
        for (op, reply) in &replies {
            provider.reply(op.clone(), reply.clone());
        }
        assert_eq!(
            h.drive_native_provider(&mut provider).unwrap().completed,
            replies.len()
        );
        h.tick(&Auth, &Regs).unwrap();
        let expected_ui = ui_signature(a.drain(&mut h));
        let expected_events = h.take_public_events();
        let expected_commands = h.take_native_commands();
        let mut recovered = NativeHost::restore_from(
            &mut store,
            HostIdentity {
                scope: HostScopeId(77),
                epoch: 2,
            },
            stamp,
            HostLimits::default(),
        )
        .unwrap();
        let mut replay = Provider::default();
        assert_eq!(
            recovered
                .drive_native_provider(&mut replay)
                .unwrap()
                .retry_pending,
            replies.len()
        );
        assert!(replay.calls.is_empty());
        recovered
            .rebind_native_controller(c.instance_address(), InvokerId(10))
            .unwrap();
        assert_eq!(
            recovered
                .drive_native_provider(&mut replay)
                .unwrap()
                .retry_pending,
            replies.len()
        );
        assert!(replay.calls.is_empty());
        let ticket = recovered
            .rebind(
                &Auth,
                a.connection,
                a.ticket.instance_address(),
                TimerRegisters([0; 4]),
            )
            .unwrap();
        let rebound = Client {
            ticket,
            connection: a.connection,
            plugin: a.plugin,
            sequence: 1,
        };
        rebound.drain(&mut recovered);
        recovered.take_public_events();
        recovered.take_native_commands();
        for (op, reply) in &replies {
            replay.reply(op.clone(), reply.clone());
        }
        assert_eq!(
            recovered
                .drive_native_provider(&mut replay)
                .unwrap()
                .completed,
            replies.len()
        );
        recovered.tick(&Auth, &Regs).unwrap();
        assert_eq!(
            ui_signature(rebound.drain(&mut recovered)),
            expected_ui,
            "private continuation {plugin:08X}"
        );
        assert_eq!(
            recovered.take_public_events(),
            expected_events,
            "VM continuation {plugin:08X}"
        );
        assert_eq!(
            recovered.take_native_commands(),
            expected_commands,
            "typed command continuation {plugin:08X}"
        );
        assert_eq!(
            replay.calls, provider.calls,
            "immutable operation identity {plugin:08X}"
        );
        replay.exhausted();
    }
}
