// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::*;
use crate::plugins::ProviderOperation;
use crate::protocol::PrivateBody;
use std::collections::{BTreeMap, VecDeque};

pub(super) struct Harness {
    pub state: State,
    pub roster: Roster,
    pub tick: u64,
    pub pending: VecDeque<(u64, Operation)>,
    pub messages: Vec<(Target, &'static str, Vec<u8>, bool)>,
    pub events: Vec<(Target, i16, Vec<i16>)>,
    pub balances: BTreeMap<Account, u32>,
    pub closed: bool,
}
pub(super) fn member(seat: u8, role: u8, chair: i16) -> Member {
    let mut input = MemberInput {
        role,
        owner_authorized: role == 2,
        ..MemberInput::default()
    };
    input.registers[3] = chair;
    Member {
        seat,
        actor: crate::ActorId(100 + seat as u64),
        avatar_object: 10 + seat as i16,
        avatar_id: 1000 + seat as u32,
        input,
    }
}
impl Harness {
    pub fn new(config: Config, balance: u32) -> Self {
        let state = State::new(config).unwrap();
        let mut balances = BTreeMap::new();
        balances.insert(Account::System, u32::MAX);
        balances.insert(Account::Object(99), balance);
        for n in 0..16 {
            balances.insert(Account::Avatar(1000 + n), 10000);
        }
        Self {
            state,
            roster: [None; 16],
            tick: 0,
            pending: VecDeque::new(),
            messages: Vec::new(),
            events: Vec::new(),
            balances,
            closed: false,
        }
    }
    pub fn accept(&mut self, actions: Actions) {
        let mut queue: VecDeque<_> = actions.items.into();
        let mut count = 0;
        while let Some(action) = queue.pop_front() {
            count += 1;
            assert!(count < 1024);
            match action {
                Action::Ui {
                    target,
                    event,
                    body,
                } => {
                    let (data, binary) = match body {
                        PrivateBody::Text(s) => (s.into_bytes(), false),
                        PrivateBody::Binary(b) => (b, true),
                    };
                    self.messages.push((target, event, data, binary));
                }
                Action::Object { target, code, args } => self.events.push((target, code, args)),
                Action::Provider {
                    callback,
                    operation,
                } => match *operation {
                    ProviderOperation::Casino(op) => self.pending.push_back((callback, op)),
                    #[allow(unreachable_patterns)]
                    _ => panic!("foreign provider"),
                },
                Action::Close { target } => match target {
                    Target::Member(seat) => {
                        if let Some(m) = self.roster[seat as usize].take() {
                            let a = self.state.leave(m, &self.roster, self.tick).unwrap();
                            queue.extend(a.items);
                        }
                    }
                    _ => {
                        if !self.closed {
                            self.closed = true;
                            for seat in 0..16 {
                                if let Some(m) = self.roster[seat].take() {
                                    let a = self.state.leave(m, &self.roster, self.tick).unwrap();
                                    queue.extend(a.items);
                                }
                            }
                            queue.extend(
                                self.state.shutdown(&self.roster, self.tick).unwrap().items,
                            );
                        }
                    }
                },
                _ => panic!("unexpected casino action"),
            }
        }
        self.check();
    }
    pub fn check(&self) {
        assert!(
            self.state.validate(&self.roster),
            "invalid live kernel at tick {}",
            self.tick
        );
        let mut w = Writer::default();
        self.state.save(&mut w);
        assert!(w.0.len() < 256 * 1024);
        let mut r = Reader::new(&w.0);
        let restored = State::restore(self.state.plugin(), &mut r).unwrap();
        r.finish().unwrap();
        assert!(
            restored.validate(&self.roster),
            "invalid restored kernel at tick {}",
            self.tick
        );
        let mut w2 = Writer::default();
        restored.save(&mut w2);
        assert_eq!(w.0, w2.0);
    }
    pub fn join(&mut self, m: Member) {
        let mut candidate = self.state.clone();
        self.roster[m.seat as usize] = Some(m);
        let actions = candidate.join(m, &self.roster, self.tick).unwrap();
        self.state = candidate;
        self.accept(actions);
    }
    pub fn msg(&mut self, m: Member, event: &str, body: &[u8]) -> Result<(), Error> {
        let mut candidate = self.state.clone();
        let a = candidate.message(m, event, body, &self.roster, self.tick)?;
        self.state = candidate;
        self.accept(a);
        Ok(())
    }
    pub fn step(&mut self, n: usize) {
        for _ in 0..n {
            self.tick += 1;
            let mut candidate = self.state.clone();
            let a = candidate.tick(&self.roster, self.tick).unwrap();
            self.state = candidate;
            self.accept(a);
        }
    }
    pub fn vm(&mut self, input: VmInput) -> Result<(), Error> {
        let mut candidate = self.state.clone();
        let a = candidate.vm_event(&input, &self.roster, self.tick)?;
        self.state = candidate;
        self.accept(a);
        Ok(())
    }
    pub fn leave(&mut self, m: Member) {
        self.roster[m.seat as usize] = None;
        let a = self.state.leave(m, &self.roster, self.tick).unwrap();
        self.accept(a);
    }
    pub fn pay(&mut self, success: bool) {
        let (id, op) = self.pending.pop_front().expect("provider request");
        let (s, t, a) = op.details();
        let mut sb = *self.balances.get(&s).unwrap();
        let mut tb = *self.balances.get(&t).unwrap();
        if success && matches!(op, Operation::Transfer { .. }) {
            assert!(sb >= a);
            sb -= a;
            tb = tb.checked_add(a).unwrap();
            self.balances.insert(s, sb);
            self.balances.insert(t, tb);
        }
        let reply = Reply::Transaction {
            success,
            source: s,
            target: t,
            amount: a,
            source_balance: sb,
            target_balance: tb,
        };
        let mut candidate = self.state.clone();
        let actions = candidate
            .reply(id, &reply, &self.roster, self.tick)
            .unwrap();
        self.state = candidate;
        self.accept(actions);
    }
    pub fn pay_all(&mut self) {
        let mut n = 0;
        while !self.pending.is_empty() {
            n += 1;
            assert!(n < 100);
            self.pay(true)
        }
    }
    pub fn has(&self, event: &str) -> bool {
        self.messages.iter().any(|(_, e, _, _)| *e == event)
    }
    pub fn clear(&mut self) {
        self.messages.clear();
        self.events.clear();
    }
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
#[test]
fn slots_rejects_duplicate_wagers_and_never_reveals_before_debit() {
    let mut h = Harness::new(slots(7), 10000);
    let m = member(0, 1, 0);
    h.join(m);
    assert!(!h.has("slots_new_game"));
    h.pay_all();
    h.clear();
    h.msg(m, "slots_execute_bet", b"5").unwrap();
    assert!(!h.has("slots_spin"));
    assert_eq!(
        h.msg(m, "slots_execute_bet", b"5"),
        Err(Error::PluginNotReady)
    );
    assert_eq!(h.pending.len(), 1);
    h.pay(true);
    assert!(h.has("slots_spin"));
    h.msg(m, "slots_wheels_stopped", b"").unwrap();
    let pending = h.pending.len();
    assert_eq!(
        h.msg(m, "slots_wheels_stopped", b""),
        Err(Error::PluginNotReady)
    );
    assert_eq!(h.pending.len(), pending);
    h.pay_all();
    h.vm(VmInput::SourceEvent { code: 6 }).unwrap();
    assert!(h.has("slots_new_game"));
}
#[test]
fn slots_denied_payout_survives_disconnect_and_requires_native_retry() {
    // Find a deterministic native fixture seed with a winning first result.
    let seed = (0..1000)
        .find(|seed| {
            let mut h = Harness::new(slots(*seed), 10000);
            let m = member(0, 1, 0);
            h.join(m);
            h.pay_all();
            h.msg(m, "slots_execute_bet", b"1").unwrap();
            h.pay(true);
            h.msg(m, "slots_wheels_stopped", b"").unwrap();
            !h.pending.is_empty()
        })
        .unwrap();
    let mut h = Harness::new(slots(seed), 10000);
    let m = member(0, 1, 0);
    h.join(m);
    h.pay_all();
    h.msg(m, "slots_execute_bet", b"1").unwrap();
    h.pay(true);
    h.msg(m, "slots_wheels_stopped", b"").unwrap();
    h.pay(false);
    assert!(!h.state.can_close());
    assert!(!h.has("slots_display_win"));
    h.leave(m);
    h.step(100);
    assert!(h.pending.is_empty());
    assert!(!h.state.can_close());
    h.vm(VmInput::RetrySettlements).unwrap();
    h.pay_all();
    assert!(h.state.can_close());
}
#[test]
fn source_slot_wheel_counts_and_payout_patterns_are_literal() {
    assert_eq!(slots::wheel(100).unwrap().len(), 244);
    let wheel = slots::wheel(100).unwrap();
    let counts = (0..12)
        .map(|v| wheel.iter().filter(|n| **n == v).count())
        .collect::<Vec<_>>();
    assert_eq!(counts, [12, 42, 12, 49, 12, 36, 12, 24, 12, 12, 15, 6]);
    for rate in 80..=110 {
        let w = slots::wheel(rate).unwrap();
        assert!((200..=300).contains(&w.len()));
        assert!(w.iter().all(|n| *n < 12));
    }
    for (stops, multiplier) in [
        ([11, 11, 11], 500),
        ([9, 9, 9], 150),
        ([7, 7, 7], 75),
        ([5, 5, 5], 50),
        ([3, 3, 3], 25),
        ([5, 3, 1], 10),
        ([1, 1, 0], 5),
        ([1, 3, 11], 2),
        ([3, 1, 5], 0),
    ] {
        assert_eq!(slots::payout(5, stops), 5 * multiplier);
    }
}
#[test]
fn provider_receipt_account_and_amount_mismatch_cannot_change_state() {
    let mut h = Harness::new(slots(1), 10000);
    let m = member(0, 1, 0);
    h.join(m);
    let (id, op) = h.pending.front().unwrap();
    let (s, t, a) = op.details();
    let mut before = Writer::default();
    h.state.save(&mut before);
    let wrong = Reply::Transaction {
        success: true,
        source: s,
        target: Account::Avatar(555),
        amount: a,
        source_balance: 1,
        target_balance: 10000,
    };
    assert_eq!(
        h.state.reply(*id, &wrong, &h.roster, 0).err(),
        Some(Error::ProviderReceiptMismatch)
    );
    let wrong = Reply::Transaction {
        success: true,
        source: s,
        target: t,
        amount: a + 1,
        source_balance: 1,
        target_balance: 10000,
    };
    assert_eq!(
        h.state.reply(*id, &wrong, &h.roster, 0).err(),
        Some(Error::ProviderReceiptMismatch)
    );
    let mut after = Writer::default();
    h.state.save(&mut after);
    assert_eq!(before.0, after.0);
}
#[test]
fn owner_money_commands_use_trusted_role_and_checked_bounds() {
    let mut h = Harness::new(slots(1), 10000);
    let owner = member(0, 2, 0);
    h.join(owner);
    h.pay_all();
    h.msg(owner, "slots_deposit", b"2147483647").unwrap();
    assert!(h.has("slots_deposit_fail"));
    assert!(h.pending.is_empty());
    h.msg(owner, "slots_deposit", b"500").unwrap();
    assert_eq!(h.pending.len(), 1);
    h.pay(false);
    assert!(h.has("slots_deposit_NSF"));
    assert_eq!(*h.balances.get(&Account::Object(99)).unwrap(), 10000);
    h.msg(owner, "slots_new_odds", &[255]).unwrap();
    assert!(h.events.iter().any(|(_, c, a)| *c == 11 && *a == vec![110]));
    assert_eq!(
        h.msg(owner, "slots_execute_bet", b"1"),
        Err(Error::NotAuthorized)
    );
}
#[test]
fn owner_payload_cannot_supply_balance_or_transaction_receipt() {
    let state = State::new(slots(2)).unwrap();
    for e in [
        "slots_balance",
        "slots_transaction_complete",
        "slots_provider_reply",
        "slots_set_seed",
    ] {
        assert!(!state.allows(e, false));
        assert!(!state.allows(e, true));
    }
    for cfg in [
        Config::Roulette {
            object: 99,
            min_bet: 1,
            max_bet: 100,
            seed: 1,
        },
        Config::Blackjack {
            object: 99,
            min_bet: 1,
            max_bet: 100,
            seed: 1,
            dealer_name: "Momi".to_owned(),
        },
        Config::HoldEm {
            object: 99,
            min_ante: 1,
            max_ante: 100,
            max_side: 100,
            seed: 1,
            dealer_name: "Momi".to_owned(),
        },
    ] {
        let state = State::new(cfg).unwrap();
        assert!(!state.allows("provider_reply", true));
        assert!(!state.allows("callback", true));
        assert!(!state.allows("set_deck", true));
    }
}
