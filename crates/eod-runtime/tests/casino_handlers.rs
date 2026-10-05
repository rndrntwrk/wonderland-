// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Actual NativeHost acceptance. The in-memory ledger is an explicit test
//! provider, not an implementation of the production account authority.
use std::collections::BTreeMap;
use wonderland_eod_runtime::{
    native_provider::*,
    plugins::{
        ProviderOperation, ProviderReply,
        casino::{self, Account, Config, Operation, Reply, VmInput},
    },
    *,
};

struct Auth;
impl ConnectionAuthority for Auth {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId> {
        Some(ActorId(connection.0))
    }
}
struct Registers;
impl RegisterSource for Registers {
    fn timer_registers(&self, _: InvokerId) -> Option<TimerRegisters> {
        None
    }
}
#[derive(Clone, Default)]
struct Store {
    key: Option<PrivateCheckpointKey>,
    bytes: Vec<u8>,
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
        if self.key != Some(key) {
            return Err(StoreError::Corrupt);
        }
        let n = destination.len().min(self.bytes.len());
        destination[..n].copy_from_slice(&self.bytes[..n]);
        Ok(PrivateRead {
            bytes_written: n,
            complete: n == self.bytes.len(),
        })
    }
}
struct Ledger {
    balances: BTreeMap<Account, u32>,
    committed: BTreeMap<NativeOperationId, (NativeProviderRequest, Vec<u8>)>,
    attempts: Vec<(HostIdentity, NativeOperationId)>,
    deny_next: bool,
    lose_next_reply: bool,
    wrong_amount: bool,
    stale_epoch: bool,
}
impl Ledger {
    fn new(bank: u32) -> Self {
        let mut balances =
            BTreeMap::from([(Account::System, u32::MAX), (Account::Object(99), bank)]);
        for player in 1..=16 {
            balances.insert(Account::Avatar(1000 + player), 10000);
        }
        Self {
            balances,
            committed: BTreeMap::new(),
            attempts: vec![],
            deny_next: false,
            lose_next_reply: false,
            wrong_amount: false,
            stale_epoch: false,
        }
    }
    fn wallet(&self, player: u32) -> u32 {
        self.balances[&Account::Avatar(1000 + player)]
    }
    fn transfers(&self) -> usize {
        self.committed
            .values()
            .filter(|(r, _)| {
                matches!(
                    r.operation(),
                    ProviderOperation::Casino(Operation::Transfer { .. })
                )
            })
            .count()
    }
}
impl NativeProvider for Ledger {
    fn execute(
        &mut self,
        host: HostIdentity,
        request: &NativeProviderRequest,
        destination: &mut [u8],
    ) -> Result<NativeProviderReceipt, NativeProviderFailure> {
        assert_eq!(request.object(), 99);
        self.attempts.push((host, request.id()));
        if !self.committed.contains_key(&request.id()) {
            let ProviderOperation::Casino(operation) = request.operation() else {
                panic!("foreign request");
            };
            let (source, target, amount, transfer) = match *operation {
                Operation::QueryBalances { source, target } => (source, target, 1, false),
                Operation::Transfer {
                    source,
                    target,
                    amount,
                } => (source, target, amount, true),
            };
            let source_balance = self.balances[&source];
            let target_balance = self.balances[&target];
            let denied = std::mem::take(&mut self.deny_next);
            let success = !denied
                && (!transfer
                    || source_balance >= amount
                        && target_balance
                            .checked_add(amount)
                            .is_some_and(|n| n <= i32::MAX as u32));
            if success && transfer {
                self.balances.insert(source, source_balance - amount);
                self.balances.insert(target, target_balance + amount);
            }
            let reply = if denied {
                Reply::ProviderDenied
            } else {
                Reply::Transaction {
                    success,
                    source,
                    target,
                    amount,
                    source_balance: self.balances[&source],
                    target_balance: self.balances[&target],
                }
            };
            let mut bytes = vec![0; 128];
            let n = ProviderReply::Casino(reply)
                .write_private(&mut bytes)
                .unwrap();
            bytes.truncate(n);
            self.committed
                .insert(request.id(), (request.clone(), bytes));
        }
        let (saved, bytes) = &self.committed[&request.id()];
        assert_eq!(
            saved, request,
            "one immutable operation ID cannot name two effects"
        );
        if std::mem::take(&mut self.lose_next_reply) {
            return Err(NativeProviderFailure::Retryable);
        }
        if self.wrong_amount {
            let ProviderOperation::Casino(operation) = request.operation() else {
                unreachable!()
            };
            let (source, target, amount) = match *operation {
                Operation::QueryBalances { source, target } => (source, target, 1),
                Operation::Transfer {
                    source,
                    target,
                    amount,
                } => (source, target, amount),
            };
            let bytes_written = ProviderReply::Casino(Reply::Transaction {
                success: true,
                source,
                target,
                amount: amount + 1,
                source_balance: self.balances[&source],
                target_balance: self.balances[&target],
            })
            .write_private(destination)
            .unwrap();
            return Ok(NativeProviderReceipt {
                host,
                id: request.id(),
                bytes_written,
                complete: true,
            });
        }
        destination[..bytes.len()].copy_from_slice(bytes);
        Ok(NativeProviderReceipt {
            host: if self.stale_epoch {
                HostIdentity {
                    epoch: host.epoch - 1,
                    ..host
                }
            } else {
                host
            },
            id: request.id(),
            bytes_written: bytes.len(),
            complete: true,
        })
    }
}
type Message = (u64, String, Vec<u8>);
struct Fixture {
    host: NativeHost,
    controller: NativeControllerTicket,
    plugin: PluginId,
    clients: BTreeMap<u64, SessionTicket>,
    sequences: BTreeMap<u64, u64>,
    messages: Vec<Message>,
    events: Vec<(InvokerId, i16, Vec<i16>)>,
    store: Store,
    ledger: Ledger,
}
fn slots(seed: u64) -> Config {
    Config::Slots {
        object: 99,
        object_guid: 2448255364,
        seed,
        machine_type: 0,
        payback_percent: 100,
        enabled: true,
    }
}
fn table(kind: u8) -> Config {
    match kind {
        1 => Config::Roulette {
            object: 99,
            min_bet: 1,
            max_bet: 100,
            seed: 9,
        },
        2 => Config::Blackjack {
            object: 99,
            min_bet: 1,
            max_bet: 100,
            seed: 9,
            dealer_name: "Momi".into(),
        },
        _ => Config::HoldEm {
            object: 99,
            min_ante: 1,
            max_ante: 100,
            max_side: 100,
            seed: 9,
            dealer_name: "Momi".into(),
        },
    }
}
impl Fixture {
    fn new(config: Config) -> Self {
        let plugin = match &config {
            Config::Slots { .. } => casino::SLOTS,
            Config::Roulette { .. } => casino::ROULETTE,
            Config::Blackjack { .. } => casino::BLACKJACK,
            Config::HoldEm { .. } => casino::HOLDEM,
        };
        let mut host = NativeHost::new(
            HostIdentity {
                scope: HostScopeId(77),
                epoch: 1,
            },
            HostLimits::default(),
        )
        .unwrap();
        let controller = host
            .connect_native(NativeCreateRequest {
                object: 99,
                cluster: 99,
                invoker: InvokerId(900),
                input: NativePluginInput::Casino(config),
            })
            .unwrap();
        Self {
            host,
            controller,
            plugin,
            clients: BTreeMap::new(),
            sequences: BTreeMap::new(),
            messages: vec![],
            events: vec![],
            store: Store::default(),
            ledger: Ledger::new(if plugin == casino::SLOTS {
                10000
            } else {
                200000
            }),
        }
    }
    fn join(&mut self, player: u64, role: u8, chair: i16) {
        let mut input = MemberInput {
            role,
            owner_authorized: role == 2,
            ..MemberInput::default()
        };
        input.registers[3] = chair;
        let ticket = self
            .host
            .join_native(
                &Auth,
                NativeJoinRequest {
                    connection: ConnectionId(player),
                    group: self.controller.instance_address(),
                    invoker: InvokerId(200 + player as u32),
                    avatar_object: 100 + player as i16,
                    avatar_id: 1000 + player as u32,
                    input,
                },
            )
            .unwrap();
        self.clients.insert(player, ticket);
        self.sequences.insert(player, 1);
    }
    fn drain(&mut self) {
        self.events.extend(
            self.host
                .take_public_events()
                .into_iter()
                .map(|e| e.source_event()),
        );
        assert!(self.host.take_native_commands().is_empty());
        for (&player, &ticket) in &self.clients {
            for message in self
                .host
                .take_private(&Auth, ConnectionId(player), ticket)
                .unwrap()
            {
                self.messages.push((
                    player,
                    message.event().into(),
                    match message.body() {
                        UiBody::Text(s) => s.as_bytes().to_vec(),
                        UiBody::Binary(b) => b.to_vec(),
                    },
                ));
            }
        }
    }
    fn clear(&mut self) {
        self.drain();
        self.messages.clear();
        self.events.clear();
    }
    fn prepare(&mut self) -> CheckpointStamp {
        self.drain();
        self.host.checkpoint_to(&mut self.store).unwrap()
    }
    fn pay(&mut self) -> NativeProviderProgress {
        self.prepare();
        let p = self.host.drive_native_provider(&mut self.ledger).unwrap();
        self.drain();
        p
    }
    fn pay_all(&mut self) {
        for _ in 0..32 {
            if self.pay().completed == 0 {
                return;
            }
        }
        panic!("provider did not quiesce");
    }
    fn send(
        &mut self,
        player: u64,
        event: &str,
        payload: WirePayload<'_>,
    ) -> Result<DispatchOutcome, Error> {
        let sequence = self.sequences[&player];
        let result = self.host.receive(
            &Auth,
            ConnectionId(player),
            ClientMessage {
                version: protocol::PROTOCOL_VERSION,
                ticket: self.clients[&player],
                plugin: self.plugin,
                sequence,
                event,
                payload,
            },
        );
        if result.is_ok() {
            *self.sequences.get_mut(&player).unwrap() += 1;
        }
        result
    }
    fn vm(&mut self, input: VmInput) -> Result<(), Error> {
        self.host.deliver_native_event(
            self.controller,
            InvokerId(900),
            NativeVmInput::Casino(input),
        )
    }
    fn ticks(&mut self, count: usize) {
        for _ in 0..count {
            self.host.tick(&Auth, &Registers).unwrap();
        }
        self.drain();
    }
    fn has(&self, event: &str) -> bool {
        self.messages.iter().any(|(_, e, _)| e == event)
    }
    fn restore(&mut self, stamp: CheckpointStamp) {
        self.host = NativeHost::restore_from(
            &mut self.store,
            HostIdentity {
                scope: HostScopeId(77),
                epoch: stamp.epoch + 1,
            },
            stamp,
            HostLimits::default(),
        )
        .unwrap();
    }
    fn rebind(&mut self) {
        self.controller = self
            .host
            .rebind_native_controller(self.controller.instance_address(), InvokerId(900))
            .unwrap();
        for (&player, ticket) in &mut self.clients {
            *ticket = self
                .host
                .rebind(
                    &Auth,
                    ConnectionId(player),
                    ticket.instance_address(),
                    TimerRegisters([0; 4]),
                )
                .unwrap();
            self.sequences.insert(player, 1);
        }
        self.drain();
    }
}
fn strings(values: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for s in values {
        let mut n = s.len();
        while n >= 128 {
            bytes.push((n as u8 & 127) | 128);
            n >>= 7;
        }
        bytes.push(n as u8);
        bytes.extend_from_slice(s.as_bytes());
    }
    bytes
}
fn parse_strings(bytes: &[u8]) -> Vec<String> {
    let mut at = 0;
    let mut result = vec![];
    while at < bytes.len() {
        let mut n = 0;
        let mut shift = 0;
        loop {
            let b = bytes[at];
            at += 1;
            n |= ((b & 127) as usize) << shift;
            if b < 128 {
                break;
            }
            shift += 7;
            assert!(shift < 35);
        }
        result.push(std::str::from_utf8(&bytes[at..at + n]).unwrap().to_owned());
        at += n;
    }
    result
}

#[test]
fn slots_prepared_debit_crash_replay_requires_rebound_native_authority() {
    let mut f = Fixture::new(slots(7));
    f.join(1, 1, 0);
    let p = f.host.drive_native_provider(&mut f.ledger).unwrap();
    assert_eq!(p.waiting_for_checkpoint, 1);
    assert!(f.ledger.attempts.is_empty());
    assert!(matches!(
        f.host.checkpoint_to(&mut f.store),
        Err(Error::CheckpointBusy)
    ));
    f.pay_all();
    assert!(f.has("slots_new_game"));
    f.clear();
    f.send(1, "slots_execute_bet", WirePayload::Text("5"))
        .unwrap();
    f.drain();
    assert!(!f.has("slots_spin"));
    assert_eq!(f.ledger.wallet(1), 10000);
    assert_eq!(
        f.send(1, "slots_execute_bet", WirePayload::Text("5")),
        Err(Error::PluginNotReady)
    );
    let stamp = f.prepare();
    let old_ticket = f.clients[&1];
    let old_controller = f.controller;
    f.ledger.lose_next_reply = true;
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .retry_pending,
        1
    );
    assert_eq!(f.ledger.wallet(1), 9995);
    assert_eq!(f.ledger.transfers(), 1);
    f.restore(stamp);
    let attempts = f.ledger.attempts.len();
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .retry_pending,
        1
    );
    assert_eq!(f.ledger.attempts.len(), attempts);
    assert_eq!(
        f.host.deliver_native_event(
            old_controller,
            InvokerId(900),
            NativeVmInput::Casino(VmInput::RetrySettlements)
        ),
        Err(Error::WrongEpoch)
    );
    assert_eq!(
        f.host.rebind(
            &Auth,
            ConnectionId(2),
            old_ticket.instance_address(),
            TimerRegisters([0; 4])
        ),
        Err(Error::RecipientMismatch)
    );
    f.rebind();
    f.clear();
    f.pay_all();
    assert!(f.has("slots_spin"));
    assert_eq!(f.ledger.wallet(1), 9995);
    assert_eq!(f.ledger.transfers(), 1);
    let last = f.ledger.attempts.last().unwrap();
    assert_eq!(last.0.epoch, 2);
    assert_eq!(last.1.origin_epoch, 1);
}

#[test]
fn slots_receipt_amount_and_current_host_epoch_are_checked_before_progression() {
    let mut f = Fixture::new(slots(1));
    f.join(1, 1, 0);
    f.prepare();
    f.ledger.wrong_amount = true;
    assert_eq!(
        f.host.drive_native_provider(&mut f.ledger),
        Err(Error::ProviderReceiptMismatch)
    );
    f.drain();
    assert!(!f.has("slots_new_game"));
    f.ledger.wrong_amount = false;
    f.ledger.stale_epoch = true;
    assert_eq!(
        f.host.drive_native_provider(&mut f.ledger),
        Err(Error::ProviderReceiptMismatch)
    );
    f.ledger.stale_epoch = false;
    f.pay_all();
    assert!(f.has("slots_new_game"));
    assert_eq!(f.ledger.committed.len(), 1);
}

#[test]
fn slots_failed_payout_survives_teardown_checkpoint_and_native_retry() {
    // Stable private native seed fixture; it is never accepted through a UI event.
    let mut f = Fixture::new(slots(2));
    f.join(1, 1, 0);
    f.pay_all();
    f.clear();
    f.send(1, "slots_execute_bet", WirePayload::Text("1"))
        .unwrap();
    f.pay_all();
    let spin = &f
        .messages
        .iter()
        .find(|(_, e, _)| e == "slots_spin")
        .unwrap()
        .2;
    assert_eq!(spin, &[1, 5, 3]);
    f.send(1, "slots_wheels_stopped", WirePayload::Text(""))
        .unwrap();
    f.ledger.deny_next = true;
    f.pay_all();
    assert!(!f.has("slots_display_win"));
    assert_eq!(f.ledger.wallet(1), 9999);
    f.host.disconnect_invoker(InvokerId(900)).unwrap();
    f.drain();
    f.clients.clear();
    let stamp = f.prepare();
    f.restore(stamp);
    f.controller = f
        .host
        .rebind_native_controller(f.controller.instance_address(), InvokerId(900))
        .unwrap();
    f.vm(VmInput::RetrySettlements).unwrap();
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .waiting_for_checkpoint,
        1
    );
    f.pay_all();
    assert_eq!(f.ledger.wallet(1), 10001);
    assert_eq!(f.vm(VmInput::RetrySettlements), Err(Error::StaleSession));
}

#[test]
fn blackjack_committed_debit_after_controller_teardown_creates_one_durable_refund() {
    let mut f = Fixture::new(table(2));
    f.join(8, 1, 0);
    f.join(1, 0, 1);
    f.pay_all();
    f.ticks(1);
    f.clear();
    f.send(
        1,
        "blackjack_bet_request",
        WirePayload::Binary(&10i32.to_le_bytes()),
    )
    .unwrap();
    f.prepare();
    f.host.disconnect_invoker(InvokerId(900)).unwrap();
    f.drain();
    f.clients.clear();
    let stamp = f.prepare();
    f.restore(stamp);
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .completed,
        1
    );
    assert_eq!(f.ledger.wallet(1), 9990);
    let p = f.host.drive_native_provider(&mut f.ledger).unwrap();
    assert_eq!(p.waiting_for_checkpoint, 1);
    f.pay_all();
    assert_eq!(f.ledger.wallet(1), 10000);
    assert_eq!(f.ledger.transfers(), 2);
    assert_eq!(
        f.host
            .rebind_native_controller(f.controller.instance_address(), InvokerId(900)),
        Err(Error::StaleSession)
    );
}

#[test]
fn roulette_retries_preserve_ordered_debits_and_source_spin_duration() {
    let mut f = Fixture::new(table(1));
    f.join(8, 1, 0);
    f.join(1, 0, 1);
    f.pay_all();
    f.join(2, 0, 2);
    f.pay_all();
    f.ticks(1);
    f.clear();
    f.send(1, "roulette_new_bet", WirePayload::Text("10%ST8%7"))
        .unwrap();
    f.send(2, "roulette_new_bet", WirePayload::Text("10%ST8%8"))
        .unwrap();
    f.ticks(902);
    assert!(!f.has("roulette_spin"));
    f.ledger.lose_next_reply = true;
    f.prepare();
    let p = f.host.drive_native_provider(&mut f.ledger).unwrap();
    assert_eq!(p.completed, 0);
    assert!(p.retry_pending >= 1);
    assert_eq!(f.ledger.wallet(1), 9990);
    assert_eq!(f.ledger.wallet(2), 10000);
    f.pay_all();
    f.ticks(1);
    assert!(f.has("roulette_spin"));
    assert_eq!(f.ledger.wallet(2), 9990);
    let stamp = f.prepare();
    f.restore(stamp);
    f.rebind();
    assert!(
        f.has("roulette_spin"),
        "the retained winning outcome is replayed without another debit"
    );
    let transfers = f.ledger.transfers();
    f.clear();
    f.ticks(359);
    assert_eq!(f.ledger.transfers(), transfers);
    assert!(!f.has("roulette_new_game"));
    f.ticks(2);
    f.pay_all();
    f.ticks(182);
    assert!(f.has("roulette_new_game"));
    assert_eq!(
        f.ledger
            .balances
            .values()
            .filter(|b| **b != u32::MAX)
            .map(|n| u64::from(*n))
            .sum::<u64>(),
        360000
    );
}

fn finish_card_round(f: &mut Fixture) {
    // Callers clear before betting; include the existing initial animation.
    let mut ui_cursor = 0;
    let mut event_cursor = 0;
    for _ in 0..1000 {
        f.pay_all();
        f.drain();
        let messages = f.messages[ui_cursor..].to_vec();
        ui_cursor = f.messages.len();
        for (player, event, _) in messages {
            if event == "blackjack_insurance_prompt" {
                f.send(
                    player,
                    "blackjack_insurance_request",
                    WirePayload::Binary(&[0]),
                )
                .unwrap();
            }
            if matches!(
                event.as_str(),
                "blackjack_resume_hand" | "blackjack_resume_double" | "blackjack_resume_split"
            ) {
                f.send(player, "blackjack_stand_request", WirePayload::Binary(&[]))
                    .unwrap();
            }
            if event == "holdemcasino_allow_input" {
                f.send(player, "holdemcasino_decision", WirePayload::Binary(&[1]))
                    .unwrap();
            }
        }
        f.drain();
        let events = f.events[event_cursor..].to_vec();
        event_cursor = f.events.len();
        for (invoker, code, _) in events {
            if invoker != InvokerId(900) {
                continue;
            }
            let callback = match code {
                1 | 8 | 9 | 10 | 12 | 17 | 21 => Some(100),
                5 => Some(101),
                11 => Some(102),
                2 | 3 | 13 => Some(103),
                _ => None,
            };
            if let Some(code) = callback {
                f.vm(VmInput::SourceEvent { code }).unwrap();
            }
        }
        f.ticks(1);
        if f.has(if f.plugin == casino::BLACKJACK {
            "blackjack_new_game"
        } else {
            "holdemcasino_new_game"
        }) {
            return;
        }
    }
    panic!("card round did not finish");
}

#[test]
fn blackjack_real_host_masks_dealer_card_and_settles_a_complete_round() {
    let mut f = Fixture::new(table(2));
    f.join(8, 1, 0);
    f.join(1, 0, 1);
    f.pay_all();
    f.ticks(1);
    f.clear();
    assert_eq!(
        f.send(
            1,
            "blackjack_callback",
            WirePayload::Binary(&100i16.to_le_bytes())
        ),
        Err(Error::EventNotAllowed)
    );
    f.send(
        1,
        "blackjack_bet_request",
        WirePayload::Binary(&10i32.to_le_bytes()),
    )
    .unwrap();
    f.ticks(10);
    assert!(!f.has("blackjack_deal_sequence"));
    f.pay_all();
    f.ticks(62);
    let deal = parse_strings(
        &f.messages
            .iter()
            .find(|(_, e, _)| e == "blackjack_deal_sequence")
            .unwrap()
            .2,
    );
    assert_eq!(deal.last().unwrap(), "Back");
    assert_eq!(deal.len(), 9);
    let stamp = f.prepare();
    f.restore(stamp);
    f.rebind();
    let hand = f
        .messages
        .iter()
        .rev()
        .find(|(_, e, _)| e == "blackjack_sync_all_hands")
        .unwrap();
    assert_eq!(parse_strings(&hand.2).last().unwrap(), "Back");
    // Keep only the original round's initial animation; rebind emits no completion.
    finish_card_round(&mut f);
    assert!(f.has("blackjack_win_loss_message"));
    let payout = i32::from_le_bytes(
        f.messages
            .iter()
            .find(|(_, e, _)| e == "blackjack_win_loss_message")
            .unwrap()
            .2
            .as_slice()
            .try_into()
            .unwrap(),
    ) as u32;
    assert_eq!(f.ledger.wallet(1), 9990 + payout);
    assert_eq!(f.ledger.transfers(), 1 + usize::from(payout > 0));
}

#[test]
fn holdem_real_host_keeps_each_hole_private_then_settles_side_and_called_game() {
    let mut f = Fixture::new(table(3));
    f.join(8, 1, 0);
    f.join(1, 0, 1);
    f.pay_all();
    f.join(2, 0, 2);
    f.pay_all();
    f.ticks(1);
    f.clear();
    f.send(
        1,
        "holdemcasino_submit_bets",
        WirePayload::Binary(&strings(&["10", "10"])),
    )
    .unwrap();
    f.pay_all();
    f.send(
        2,
        "holdemcasino_submit_bets",
        WirePayload::Binary(&strings(&["10", "0"])),
    )
    .unwrap();
    f.pay_all();
    f.ticks(62);
    for player in 1..=2 {
        let deal = parse_strings(
            &f.messages
                .iter()
                .find(|(p, e, _)| *p == player && e == "holdemcasino_deal_sequence")
                .unwrap()
                .2,
        );
        assert_eq!(deal.len(), 13);
        assert_eq!(&deal[8..10], &["Back", "Back"]);
        let other = if player == 1 { 2 } else { 0 };
        assert_eq!(&deal[other..other + 2], &["Back", "Back"]);
        let own = if player == 1 { 0 } else { 2 };
        assert_ne!(deal[own], "Back");
    }
    finish_card_round(&mut f);
    assert!(f.has("holdemcasino_final_deal_sequence"));
    for player in 1..=2 {
        let final_message = f
            .messages
            .iter()
            .find(|(p, e, _)| *p == player && e == "holdemcasino_win_loss_message")
            .unwrap();
        let fields = parse_strings(&final_message.2);
        assert_eq!(
            fields.len(),
            if matches!(fields[1].as_str(), "TwoPair" | "FullHouse") {
                4
            } else {
                3
            }
        );
        assert!(f.ledger.committed.values().any(|(r, _)| matches!(r.operation(), ProviderOperation::Casino(Operation::Transfer { source: Account::Avatar(id), target: Account::Object(99), amount: 20 }) if *id == 1000 + player as u32)));
    }
    assert_eq!(
        f.ledger
            .balances
            .values()
            .filter(|b| **b != u32::MAX)
            .map(|n| u64::from(*n))
            .sum::<u64>(),
        360000
    );
}

#[test]
fn native_casino_configuration_cannot_name_another_objects_bank_account() {
    let mut host = NativeHost::new(
        HostIdentity {
            scope: HostScopeId(77),
            epoch: 1,
        },
        HostLimits::default(),
    )
    .unwrap();
    assert!(
        host.connect_native(NativeCreateRequest {
            object: 98,
            cluster: 99,
            invoker: InvokerId(900),
            input: NativePluginInput::Casino(slots(0))
        })
        .is_err()
    );
}

#[test]
fn late_table_balance_query_after_teardown_does_not_require_a_new_kick_animation() {
    let mut f = Fixture::new(table(2));
    f.join(1, 0, 1);
    f.prepare();
    f.host.disconnect_invoker(InvokerId(900)).unwrap();
    f.drain();
    f.clients.clear();
    let stamp = f.prepare();
    f.restore(stamp);
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .completed,
        1
    );
    assert!(f.host.take_public_events().is_empty());
    assert_eq!(f.ledger.wallet(1), 10000);
    assert_eq!(
        f.host
            .rebind_native_controller(f.controller.instance_address(), InvokerId(900)),
        Err(Error::StaleSession)
    );
}

#[test]
fn slots_late_debit_after_teardown_refunds_instead_of_settling_the_selected_spin() {
    // Seed 0 selects [3, 1, 1] (a loss); seed 2 selects [1, 5, 3] (a win).
    // Teardown before the debit receipt must return the same five-unit stake.
    for seed in [0, 2] {
        let mut f = Fixture::new(slots(seed));
        f.join(1, 1, 0);
        f.pay_all();
        f.clear();
        f.send(1, "slots_execute_bet", WirePayload::Text("5"))
            .unwrap();
        f.prepare();
        f.ledger.lose_next_reply = true;
        assert_eq!(
            f.host
                .drive_native_provider(&mut f.ledger)
                .unwrap()
                .retry_pending,
            1
        );
        assert_eq!(f.ledger.wallet(1), 9995);
        f.host.disconnect_invoker(InvokerId(900)).unwrap();
        f.drain();
        f.clients.clear();
        let stamp = f.prepare();
        f.restore(stamp);
        assert_eq!(
            f.host
                .drive_native_provider(&mut f.ledger)
                .unwrap()
                .completed,
            1
        );
        assert_eq!(
            f.host
                .drive_native_provider(&mut f.ledger)
                .unwrap()
                .waiting_for_checkpoint,
            1,
            "the late debit must create a durable refund obligation"
        );
        if seed == 2 {
            f.prepare();
            f.ledger.deny_next = true;
            assert_eq!(
                f.host
                    .drive_native_provider(&mut f.ledger)
                    .unwrap()
                    .completed,
                1
            );
            assert_eq!(f.ledger.wallet(1), 9995);
            let stamp = f.prepare();
            f.restore(stamp);
            f.controller = f
                .host
                .rebind_native_controller(f.controller.instance_address(), InvokerId(900))
                .unwrap();
            f.vm(VmInput::RetrySettlements).unwrap();
        }
        f.pay_all();
        assert_eq!(f.ledger.wallet(1), 10000, "seed {seed}");
        assert_eq!(f.ledger.transfers(), if seed == 2 { 3 } else { 2 });
        assert!(!f.has("slots_spin"));
        assert!(!f.has("slots_display_loss"));
        assert!(!f.has("slots_display_win"));
        assert_eq!(
            f.host
                .rebind_native_controller(f.controller.instance_address(), InvokerId(900)),
            Err(Error::StaleSession)
        );
    }
}

#[test]
fn slots_late_debit_after_player_departure_refunds_before_reusing_the_machine() {
    let mut f = Fixture::new(slots(0));
    f.join(1, 1, 0);
    f.pay_all();
    f.clear();
    f.send(1, "slots_execute_bet", WirePayload::Text("5"))
        .unwrap();
    f.prepare();
    f.ledger.lose_next_reply = true;
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .retry_pending,
        1
    );
    f.host.disconnect_invoker(InvokerId(201)).unwrap();
    f.drain();
    f.clients.clear();
    f.pay_all();
    assert_eq!(f.ledger.wallet(1), 10000);
    assert_eq!(f.ledger.transfers(), 2);
    assert!(!f.has("slots_spin"));
    f.join(2, 1, 0);
    f.pay_all();
    assert!(
        f.messages
            .iter()
            .any(|(player, event, _)| *player == 2 && event == "slots_new_game")
    );
}

fn assert_extra_debit_teardown_refunds(kind: u8, event: &str) {
    let mut config = table(kind);
    if let Config::Blackjack { seed, .. } = &mut config {
        // The first two player cards are ten-value cards, allowing both moves.
        *seed = 5;
    }
    let mut f = Fixture::new(config);
    f.join(8, 1, 0);
    f.join(1, 0, 1);
    f.pay_all();
    f.ticks(1);
    f.clear();
    if kind == 2 {
        f.send(
            1,
            "blackjack_bet_request",
            WirePayload::Binary(&10i32.to_le_bytes()),
        )
        .unwrap();
    } else {
        f.send(
            1,
            "holdemcasino_submit_bets",
            WirePayload::Binary(&strings(&["10", "0"])),
        )
        .unwrap();
    }
    f.pay_all();
    f.ticks(62);
    f.vm(VmInput::SourceEvent { code: 100 }).unwrap();
    f.ticks(if kind == 2 { 1 } else { 95 });
    assert!(f.has(if kind == 2 {
        "blackjack_resume_split"
    } else {
        "holdemcasino_allow_input"
    }));
    f.clear();
    f.send(
        1,
        event,
        WirePayload::Binary(if kind == 2 { &[] } else { &[1] }),
    )
    .unwrap();
    f.prepare();
    f.ledger.lose_next_reply = true;
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .retry_pending,
        1
    );
    assert_eq!(f.ledger.wallet(1), if kind == 2 { 9980 } else { 9970 });
    f.host.disconnect_invoker(InvokerId(900)).unwrap();
    f.drain();
    f.clients.clear();
    let stamp = f.prepare();
    f.restore(stamp);
    assert_eq!(
        f.host
            .drive_native_provider(&mut f.ledger)
            .unwrap()
            .completed,
        1
    );
    assert!(f.host.take_public_events().is_empty());
    let stamp = f.prepare();
    f.restore(stamp);
    f.pay_all();
    assert_eq!(f.ledger.wallet(1), 10000);
    assert_eq!(f.ledger.transfers(), 3);
    assert_eq!(
        f.host
            .rebind_native_controller(f.controller.instance_address(), InvokerId(900)),
        Err(Error::StaleSession)
    );
}

#[test]
fn blackjack_late_split_debit_after_teardown_keeps_the_refund_checkpoint_valid() {
    assert_extra_debit_teardown_refunds(2, "blackjack_split_request");
}

#[test]
fn blackjack_late_double_debit_after_teardown_keeps_the_refund_checkpoint_valid() {
    assert_extra_debit_teardown_refunds(2, "blackjack_double_request");
}

#[test]
fn holdem_late_call_debit_after_teardown_keeps_the_refund_checkpoint_valid() {
    assert_extra_debit_teardown_refunds(3, "holdemcasino_decision");
}
