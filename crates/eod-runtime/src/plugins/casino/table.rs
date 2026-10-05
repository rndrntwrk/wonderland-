// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Shared dealer/croupier admission and durable money boundary. Gameplay and
//! wire payloads below follow each separate source handler's actual rules.
use super::{
    cards::*,
    finance::*,
    roulette::{self, Bet},
    *,
};

const CLOSED: u8 = 0;
const WAITING: u8 = 1;
const BETTING: u8 = 2;
const PENDING: u8 = 3;
const ANIMATION: u8 = 4;
const PLAYER: u8 = 5;
const DEALER: u8 = 6;
const SIDE: u8 = 7;
const FINALE: u8 = 8;
const HOLD: u8 = 9;
const INTERMISSION: u8 = 10;
const INSURANCE_ROUND: u8 = 11;
const SPINNING: u8 = 12;
const SETTLING: u8 = 13;
const MANAGING: u8 = 14;
const MOVE_PENDING: u8 = 15;
const INITIAL_CALLBACK: u8 = 1;
const PLAYER_CALLBACK: u8 = 2;
const DEALER_CALLBACK: u8 = 3;
const SPLIT_CALLBACK: u8 = 4;
const CHECK_CALLBACK: u8 = 5;
const SIDE_CALLBACK: u8 = 6;
const PAY_CALLBACK: u8 = 7;
const COLLECT_CALLBACK: u8 = 8;
const COMMUNITY_CALLBACK: u8 = 9;
const OWNER_SEAT: u8 = 15;

#[derive(Clone)]
struct Player {
    member: Member,
    connected: bool,
    ready: bool,
    balance: u32,
    bet: u32,
    side: u32,
    call: u32,
    contributed: u32,
    received: u32,
    submitted: bool,
    accepted: bool,
    observed: bool,
    insured: bool,
    prompted: bool,
    folded: bool,
    done: bool,
    refunding: bool,
    hands: Vec<BjHand>,
    active_hand: usize,
    hole: Vec<Card>,
    bets: Vec<Bet>,
    side_done: bool,
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Harness, member};
    use super::*;
    fn cfg(kind: u8) -> Config {
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
                dealer_name: "Momi".to_owned(),
            },
            _ => Config::HoldEm {
                object: 99,
                min_ante: 1,
                max_ante: 100,
                max_side: 100,
                seed: 9,
                dealer_name: "Momi".to_owned(),
            },
        }
    }
    fn c(rank: u8, suit: u8) -> Card {
        Card(suit * 13 + rank - 1)
    }
    fn table(h: &Harness) -> &Table {
        let Game::Table(t) = &h.state.game else {
            panic!()
        };
        t
    }
    fn table_mut(h: &mut Harness) -> &mut Table {
        let Game::Table(t) = &mut h.state.game else {
            panic!()
        };
        t
    }
    fn draw_order(h: &mut Harness, cards: &[Card]) {
        let t = table_mut(h);
        t.shoe.cards.rotate_left(t.shoe.at);
        t.shoe.at = 0;
        for (i, c) in cards.iter().enumerate() {
            let j = t.shoe.cards[i..].iter().position(|n| n == c).unwrap() + i;
            t.shoe.cards.swap(i, j);
        }
        h.check();
    }
    fn start(kind: u8) -> (Harness, Member) {
        let mut h = Harness::new(cfg(kind), 200000);
        h.join(member(7, 1, 0));
        let p = member(3, 0, 1);
        h.join(p);
        h.pay_all();
        h.step(1);
        assert_eq!(table(&h).phase, BETTING);
        h.clear();
        (h, p)
    }
    fn initial(h: &mut Harness, p: Member, ante: i32, side: i32) {
        if table(h).kind == 2 {
            h.msg(p, "blackjack_bet_request", &ante.to_le_bytes())
                .unwrap();
        } else {
            h.msg(
                p,
                "holdemcasino_submit_bets",
                &strings(&[ante.to_string(), side.to_string()]),
            )
            .unwrap();
        }
        assert!(!h.has(if table(h).kind == 2 {
            "blackjack_deal_sequence"
        } else {
            "holdemcasino_deal_sequence"
        }));
        h.pay_all();
        h.step(62);
        assert_eq!(table(h).expected, Some(INITIAL_CALLBACK));
    }
    fn callback(h: &mut Harness) {
        let expected = table(h).expected.unwrap();
        let code = match expected {
            SPLIT_CALLBACK => 102,
            CHECK_CALLBACK => 101,
            SIDE_CALLBACK | PAY_CALLBACK | COLLECT_CALLBACK => 103,
            _ => 100,
        };
        h.vm(VmInput::SourceEvent { code }).unwrap();
    }
    fn finish(h: &mut Harness) {
        for _ in 0..1000 {
            if !h.pending.is_empty() {
                h.pay_all();
            } else if table(h).expected.is_some() {
                callback(h);
            } else if table(h).phase == PLAYER && table(h).next.is_none() {
                let i = table(h).active as usize;
                let p = table(h).player(i).unwrap().member;
                if table(h).kind == 2 {
                    h.msg(p, "blackjack_stand_request", &[]).unwrap();
                } else {
                    h.msg(p, "holdemcasino_decision", &[1]).unwrap();
                }
            } else if table(h).phase == INSURANCE_ROUND && table(h).next.is_none() {
                let member = table(h)
                    .players
                    .iter()
                    .flatten()
                    .find(|p| p.prompted)
                    .map(|p| p.member);
                if let Some(p) = member {
                    h.msg(p, "blackjack_insurance_request", &[0]).unwrap();
                } else {
                    h.step(1);
                }
            } else {
                h.step(1);
            }
            if h.has(if table(h).kind == 2 {
                "blackjack_new_game"
            } else {
                "holdemcasino_new_game"
            }) {
                return;
            }
        }
        panic!("round did not finish; phase {}", table(h).phase)
    }

    #[test]
    fn blackjack_paid_split_and_double_complete_with_source_natural_payout() {
        let (mut h, p) = start(2);
        draw_order(
            &mut h,
            &[
                c(10, 0),
                c(11, 0),
                c(10, 2),
                c(8, 2),
                c(1, 0),
                c(6, 1),
                c(5, 3),
            ],
        );
        initial(&mut h, p, 10, 0);
        callback(&mut h);
        h.step(1);
        assert_eq!(table(&h).phase, PLAYER);
        h.msg(p, "blackjack_split_request", &[]).unwrap();
        assert_eq!(table(&h).players[0].as_ref().unwrap().hands.len(), 1);
        h.pay_all();
        assert_eq!(table(&h).expected, Some(SPLIT_CALLBACK));
        assert_eq!(
            h.vm(VmInput::SourceEvent { code: 100 }),
            Err(Error::EventNotAllowed)
        );
        callback(&mut h);
        h.step(1);
        assert_eq!(table(&h).players[0].as_ref().unwrap().active_hand, 1);
        h.msg(p, "blackjack_double_request", &[]).unwrap();
        h.pay_all();
        callback(&mut h);
        finish(&mut h);
        assert!(h.messages.iter().any(|(_,e,b,_)|*e=="blackjack_win_loss_message"&&*b==65i32.to_le_bytes()));
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10035);
    }
    #[test]
    fn blackjack_hole_card_hidden_until_dealer_turn_and_insurance_source_odd_rule() {
        let (mut h, p) = start(2);
        draw_order(&mut h, &[c(9, 0), c(8, 0), c(1, 2), c(13, 3)]);
        initial(&mut h, p, 5, 0);
        let data = &h
            .messages
            .iter()
            .find(|(_, e, _, _)| *e == "blackjack_deal_sequence")
            .unwrap()
            .2;
        let values = parse_strings(data, 9).unwrap();
        assert_eq!(values.last().unwrap(), "Back");
        assert!(!values.iter().any(|s| s == "King_Spades"));
        callback(&mut h);
        h.step(1);
        h.msg(p, "blackjack_insurance_request", &[1]).unwrap();
        assert!(!table(&h).players[0].as_ref().unwrap().insured);
        h.pay_all();
        callback(&mut h);
        finish(&mut h);
        assert!(
            h.messages
                .iter()
                .any(|(_, e, b, _)| *e == "blackjack_win_loss_message" && *b == 5i32.to_le_bytes())
        );
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 9998);
    }
    #[test]
    fn holdem_side_payment_and_final_push_use_flop_and_seven_cards() {
        let (mut h, p) = start(3);
        draw_order(
            &mut h,
            &[
                c(1, 0),
                c(1, 1),
                c(9, 0),
                c(10, 0),
                c(2, 3),
                c(3, 2),
                c(4, 0),
                c(5, 3),
                c(6, 1),
            ],
        );
        initial(&mut h, p, 10, 10);
        let data = &h
            .messages
            .iter()
            .find(|(_, e, _, _)| *e == "holdemcasino_deal_sequence")
            .unwrap()
            .2;
        let values = parse_strings(data, 13).unwrap();
        assert_eq!(&values[..2], ["Ace_Clubs", "Ace_Diamonds"]);
        assert_eq!(&values[8..10], ["Back", "Back"]);
        callback(&mut h);
        h.step(1);
        assert_eq!(h.pending.len(), 1);
        assert!(!h.has("holdemcasino_sidebet_win_message"));
        h.pay_all();
        callback(&mut h);
        h.step(1);
        assert_eq!(
            table(&h).phase,
            PLAYER,
            "a paid side winner resumes without the no-winners Hold delay"
        );
        h.msg(p, "holdemcasino_decision", &[1]).unwrap();
        h.pay_all();
        callback(&mut h);
        finish(&mut h);
        assert!(
            h.messages
                .iter()
                .any(|(_, e, b, _)| *e == "holdemcasino_win_loss_message"
                    && parse_strings(b, 3).is_ok_and(|v| v[0] == "p30"))
        );
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10070);
    }
    #[test]
    fn holdem_initial_and_late_views_do_not_disclose_opponents_hole_cards() {
        let (mut h, p) = start(3);
        let observer = member(4, 0, 2);
        h.join(observer);
        h.pay_all();
        draw_order(
            &mut h,
            &[
                c(1, 0),
                c(1, 1),
                c(9, 0),
                c(10, 0),
                c(2, 3),
                c(3, 2),
                c(4, 0),
            ],
        );
        h.msg(
            p,
            "holdemcasino_submit_bets",
            &strings(&["10".to_owned(), "0".to_owned()]),
        )
        .unwrap();
        h.pay_all();
        h.step(963);
        assert_eq!(table(&h).expected, Some(INITIAL_CALLBACK));
        let message = h
            .messages
            .iter()
            .find(|(t, e, _, _)| {
                *t == Target::Member(observer.seat) && *e == "holdemcasino_deal_sequence"
            })
            .unwrap();
        assert_eq!(
            &parse_strings(&message.2, 13).unwrap()[..2],
            ["Back", "Back"]
        );
        h.clear();
        let actions = h.state.rebind(observer, &h.roster, h.tick).unwrap();
        h.accept(actions);
        let message = h
            .messages
            .iter()
            .find(|(t, e, _, _)| {
                *t == Target::Member(observer.seat) && *e == "holdemcasino_sync_hands_up"
            })
            .unwrap();
        assert_eq!(
            &parse_strings(&message.2, 10).unwrap()[..2],
            ["Back", "Back"]
        );
    }
    #[test]
    fn roulette_bets_lock_and_failed_debit_never_creates_payout() {
        let (mut h, p) = start(1);
        h.msg(p, "roulette_new_bet", b"100%ST8%7").unwrap();
        h.step(902);
        assert_eq!(h.pending.len(), 1);
        assert!(!h.has("roulette_spin"));
        assert_eq!(
            h.msg(p, "roulette_new_bet", b"1%ST8%7"),
            Err(Error::PluginNotReady)
        );
        h.pay(false);
        h.step(1);
        assert!(
            h.messages.iter().any(|(_, e, b, _)| *e == "roulette_spin"
                && std::str::from_utf8(b).unwrap().ends_with("%0"))
        );
        h.step(542);
        assert!(h.pending.is_empty());
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10000);
    }
    #[test]
    fn holdem_rejects_negative_side_without_debit() {
        let (mut h, p) = start(3);
        assert_eq!(
            h.msg(
                p,
                "holdemcasino_submit_bets",
                &strings(&["10".to_owned(), "-10".to_owned()])
            ),
            Err(Error::InvalidPluginInput)
        );
        assert!(h.pending.is_empty());
    }
    #[test]
    fn debit_completing_after_shutdown_is_refunded_exactly_once() {
        let (mut h, p) = start(2);
        h.msg(p, "blackjack_bet_request", &10i32.to_le_bytes())
            .unwrap();
        let actions = h.state.shutdown(&h.roster, h.tick).unwrap();
        h.accept(actions);
        assert!(!h.state.can_close());
        h.pay(true);
        assert_eq!(h.pending.len(), 1);
        h.pay(true);
        assert!(h.state.can_close());
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10000);
        h.step(100);
        assert!(h.pending.is_empty());
    }
    #[test]
    fn caller_does_not_lose_accepted_ante_on_call_provider_decline() {
        let (mut h, p) = start(3);
        initial(&mut h, p, 10, 0);
        callback(&mut h);
        h.step(1);
        if !h.pending.is_empty() {
            h.pay_all();
        }
        while table(&h).phase != PLAYER {
            if table(&h).expected.is_some() {
                callback(&mut h);
            }
            h.step(1);
        }
        h.msg(p, "holdemcasino_decision", &[1]).unwrap();
        h.pay(false);
        assert!(table(&h).players[0].as_ref().unwrap().accepted);
        assert_eq!(table(&h).players[0].as_ref().unwrap().bet, 10);
        assert_eq!(table(&h).phase, PLAYER);
        h.msg(p, "holdemcasino_decision", &[0]).unwrap();
        finish(&mut h);
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 9990);
    }
    #[test]
    fn table_waits_for_join_query_without_entering_permanent_settlement() {
        let mut h = Harness::new(cfg(2), 200000);
        let p = member(0, 0, 1);
        h.join(p);
        h.pay_all();
        let q = member(1, 0, 2);
        h.join(q);
        h.join(member(7, 1, 0));
        h.step(10);
        h.pay_all();
        h.step(2);
        assert_eq!(table(&h).phase, BETTING);
        h.msg(p, "blackjack_bet_request", &10i32.to_le_bytes())
            .unwrap();
    }
    #[test]
    fn disconnect_during_paid_split_request_resolves_without_player_callback() {
        let (mut h, p) = start(2);
        draw_order(
            &mut h,
            &[c(10, 0), c(11, 0), c(10, 2), c(8, 2), c(1, 0), c(6, 1)],
        );
        initial(&mut h, p, 10, 0);
        callback(&mut h);
        h.step(1);
        h.msg(p, "blackjack_split_request", &[]).unwrap();
        h.leave(p);
        h.clear();
        h.pay_all();
        assert_ne!(table(&h).expected, Some(SPLIT_CALLBACK));
        for _ in 0..150 {
            if !h.pending.is_empty() {
                h.pay_all();
            } else if table(&h).expected.is_some() {
                callback(&mut h);
            } else {
                h.step(1);
            }
            if table(&h).phase == WAITING {
                break;
            }
        }
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10005);
        assert_eq!(table(&h).phase, WAITING);
        assert!(
            !h.events.iter().any(|(_, code, args)| {
                [2, 3, 6, 7].contains(code) && args.first() == Some(&p.avatar_object)
            }),
            "a departed avatar must not acquire a new settlement animation"
        );
    }
    #[test]
    fn checkpoint_rejects_pending_wager_above_configured_limit() {
        let (mut h, p) = start(2);
        h.msg(p, "blackjack_bet_request", &10i32.to_le_bytes())
            .unwrap();
        let pending = table_mut(&mut h)
            .finance
            .pending
            .values_mut()
            .next()
            .unwrap();
        pending.primary = 1001;
        let Operation::Transfer { amount, .. } = &mut pending.operation else {
            panic!()
        };
        *amount = 1001;
        assert!(!h.state.validate(&h.roster));
    }
    #[test]
    fn checkpoint_rejects_missing_animation_gate_and_illegal_next_phase() {
        let (h, _) = start(2);
        for next in [ANIMATION, SETTLING, MOVE_PENDING, MANAGING, SPINNING] {
            let mut damaged = h.state.clone();
            let Game::Table(t) = &mut damaged.game else {
                panic!()
            };
            t.next = Some(next);
            assert!(
                !damaged.validate(&h.roster),
                "unreachable next phase {next}"
            );
        }
        let mut damaged = h.state.clone();
        let Game::Table(t) = &mut damaged.game else {
            panic!()
        };
        t.phase = ANIMATION;
        t.next = None;
        t.expected = None;
        assert!(!damaged.validate(&h.roster));
    }
    #[test]
    fn checkpoint_rejects_payout_that_does_not_match_frozen_cards() {
        let (mut h, p) = start(2);
        draw_order(&mut h, &[c(1, 0), c(13, 0), c(10, 2), c(8, 2)]);
        initial(&mut h, p, 10, 0);
        for _ in 0..150 {
            if table(&h).finance.pending.values().any(|p| p.kind == PAYOUT) {
                break;
            }
            if table(&h).expected.is_some() {
                callback(&mut h);
            } else {
                h.step(1);
            }
        }
        let pending = table_mut(&mut h)
            .finance
            .pending
            .values_mut()
            .find(|p| p.kind == PAYOUT)
            .unwrap();
        assert_eq!(pending.primary, 25);
        pending.primary = 999;
        let Operation::Transfer { amount, .. } = &mut pending.operation else {
            panic!()
        };
        *amount = 999;
        assert!(!h.state.validate(&h.roster));
    }
    #[test]
    fn refused_predeal_refund_can_resume_the_remaining_players_after_native_retry() {
        let (mut h, p) = start(2);
        let q = member(4, 0, 2);
        h.join(q);
        h.pay_all();
        h.msg(p, "blackjack_bet_request", &10i32.to_le_bytes())
            .unwrap();
        h.pay_all();
        h.leave(p);
        assert_eq!(h.pending.len(), 1);
        h.pay(false);
        assert!(!h.state.can_close());
        h.step(963);
        h.vm(VmInput::RetrySettlements).unwrap();
        h.pay_all();
        h.step(3);
        assert_eq!(table(&h).phase, BETTING);
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10000);
        h.msg(q, "blackjack_bet_request", &10i32.to_le_bytes())
            .unwrap();
    }
    #[test]
    fn shutdown_while_insurance_payment_is_pending_refunds_all_committed_stakes() {
        let (mut h, p) = start(2);
        draw_order(&mut h, &[c(9, 0), c(8, 0), c(1, 2), c(13, 3)]);
        initial(&mut h, p, 5, 0);
        callback(&mut h);
        h.step(1);
        h.msg(p, "blackjack_insurance_request", &[1]).unwrap();
        let actions = h.state.shutdown(&h.roster, h.tick).unwrap();
        h.accept(actions);
        h.pay_all();
        assert_eq!(h.balances[&Account::Avatar(p.avatar_id)], 10000);
        assert!(h.state.can_close());
    }
}
impl Player {
    fn new(member: Member) -> Self {
        Self {
            member,
            connected: true,
            ready: false,
            balance: 0,
            bet: 0,
            side: 0,
            call: 0,
            contributed: 0,
            received: 0,
            submitted: false,
            accepted: false,
            observed: false,
            insured: false,
            prompted: false,
            folded: false,
            done: false,
            refunding: false,
            hands: Vec::new(),
            active_hand: 0,
            hole: Vec::new(),
            bets: Vec::new(),
            side_done: false,
        }
    }
    fn reset(&mut self) {
        let balance = self.balance;
        let observed = self.observed;
        let ready = self.ready;
        let connected = self.connected;
        *self = Self::new(self.member);
        self.balance = balance;
        self.observed = observed;
        self.ready = ready;
        self.connected = connected;
    }
    fn target(&self) -> Target {
        Target::Member(self.member.seat)
    }
    fn total_bets(&self) -> u32 {
        self.bets.iter().map(Bet::amount).sum()
    }
    fn save(&self, w: &mut Writer) {
        save_member(w, &self.member);
        for b in [
            self.connected,
            self.ready,
            self.submitted,
            self.accepted,
            self.observed,
            self.insured,
            self.prompted,
            self.folded,
            self.done,
            self.refunding,
            self.side_done,
        ] {
            w.bool(b)
        }
        for n in [
            self.balance,
            self.bet,
            self.side,
            self.call,
            self.contributed,
            self.received,
        ] {
            w.u32(n)
        }
        w.u32(self.active_hand as u32);
        w.u32(self.hands.len() as u32);
        for h in &self.hands {
            h.save(w)
        }
        w.bytes(&self.hole.iter().map(|c| c.0).collect::<Vec<_>>());
        w.u32(self.bets.len() as u32);
        for b in &self.bets {
            b.save(w)
        }
    }
    fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let member = restore_member(r)?;
        let connected = r.bool()?;
        let ready = r.bool()?;
        let submitted = r.bool()?;
        let accepted = r.bool()?;
        let observed = r.bool()?;
        let insured = r.bool()?;
        let prompted = r.bool()?;
        let folded = r.bool()?;
        let done = r.bool()?;
        let refunding = r.bool()?;
        let side_done = r.bool()?;
        let balance = r.u32()?;
        let bet = r.u32()?;
        let side = r.u32()?;
        let call = r.u32()?;
        let contributed = r.u32()?;
        let received = r.u32()?;
        let active_hand = r.u32()? as usize;
        let n = r.count(4)?;
        let mut hands = Vec::with_capacity(n);
        for _ in 0..n {
            hands.push(BjHand::restore(r)?)
        }
        let hole = r.bytes(2)?.into_iter().map(Card).collect();
        let n = r.count(roulette::MAX_BETS)?;
        let mut bets = Vec::with_capacity(n);
        for _ in 0..n {
            bets.push(Bet::restore(r)?)
        }
        Ok(Self {
            member,
            connected,
            ready,
            balance,
            bet,
            side,
            call,
            contributed,
            received,
            submitted,
            accepted,
            observed,
            insured,
            prompted,
            folded,
            done,
            refunding,
            hands,
            active_hand,
            hole,
            bets,
            side_done,
        })
    }
}

#[derive(Clone)]
pub(super) struct Table {
    kind: u8,
    object: u32,
    min: u32,
    max: u32,
    max_side: u32,
    name: String,
    balance: Option<u32>,
    dealer: Option<Member>,
    owner: Option<Member>,
    owner_connected: bool,
    players: [Option<Player>; 4],
    phase: u8,
    next: Option<u8>,
    frame: u32,
    seconds: u32,
    active: i8,
    expected: Option<u8>,
    events: Vec<(i16, Vec<i16>)>,
    settlement_stage: u8,
    broken: bool,
    enabled: bool,
    closing: bool,
    rng: Rng,
    shoe: Shoe,
    dealer_hand: Option<BjHand>,
    dealer_hole: Vec<Card>,
    board: Vec<Card>,
    winning: u8,
    finance: Finance,
}
impl Table {
    pub fn new(config: Config) -> Result<Self, Error> {
        let (kind, object, min, max, max_side, seed, name) = match config {
            Config::Roulette {
                object,
                min_bet,
                max_bet,
                seed,
            } => (1, object, min_bet, max_bet, 0, seed, String::new()),
            Config::Blackjack {
                object,
                min_bet,
                max_bet,
                seed,
                dealer_name,
            } => (2, object, min_bet, max_bet, 0, seed, dealer_name),
            Config::HoldEm {
                object,
                min_ante,
                max_ante,
                max_side,
                seed,
                dealer_name,
            } => (3, object, min_ante, max_ante, max_side, seed, dealer_name),
            _ => return Err(Error::InvalidPluginInput),
        };
        if !Account::Object(object).valid()
            || min < 1
            || max < min
            || max > 1000
            || !(0..=1000).contains(&max_side)
            || name.len() > 128
            || name.chars().any(char::is_control)
        {
            return Err(Error::InvalidPluginInput);
        }
        let mut rng = Rng::new(seed);
        let shoe = Shoe::new(if kind == 2 { 6 } else { 1 }, &mut rng)?;
        Ok(Self {
            kind,
            object,
            min: min as u32,
            max: max as u32,
            max_side: max_side as u32,
            name,
            balance: None,
            dealer: None,
            owner: None,
            owner_connected: false,
            players: std::array::from_fn(|_| None),
            phase: CLOSED,
            next: None,
            frame: 0,
            seconds: 0,
            active: -1,
            expected: None,
            events: Vec::new(),
            settlement_stage: 0,
            broken: false,
            enabled: true,
            closing: false,
            rng,
            shoe,
            dealer_hand: None,
            dealer_hole: Vec::new(),
            board: Vec::new(),
            winning: 255,
            finance: Finance::new(),
        })
    }
    pub fn plugin(&self) -> PluginId {
        match self.kind {
            1 => ROULETTE,
            2 => BLACKJACK,
            _ => HOLDEM,
        }
    }
    pub fn trusted_object(&self) -> u32 {
        self.object
    }
    fn limit(&self) -> u32 {
        if self.kind == 1 {
            i32::MAX as u32
        } else {
            999999
        }
    }
    fn reserve(&self) -> u32 {
        match self.kind {
            1 => self.max * 140,
            2 => self.max * 8,
            _ => self.max * 84 + self.max_side * 104,
        }
    }
    fn funded(&self) -> bool {
        self.balance
            .is_some_and(|b| b >= self.reserve() && b <= self.limit())
            && !self.broken
            && self.enabled
            && !self.closing
    }
    fn any_connected(&self) -> bool {
        self.players
            .iter()
            .flatten()
            .any(|p| p.connected && p.ready)
    }
    fn any_accepted(&self) -> bool {
        self.players.iter().flatten().any(|p| p.accepted && !p.done)
    }
    fn all_submitted(&self) -> bool {
        self.players
            .iter()
            .flatten()
            .filter(|p| p.connected && p.ready)
            .all(|p| p.submitted)
    }
    fn player_index(&self, m: Member) -> Result<usize, Error> {
        self.players
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.connected && p.member == m))
            .ok_or(Error::StaleSession)
    }
    fn player(&self, i: usize) -> Result<&Player, Error> {
        self.players
            .get(i)
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidIdentity)
    }
    fn player_mut(&mut self, i: usize) -> Result<&mut Player, Error> {
        self.players
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(Error::InvalidIdentity)
    }
    fn broadcast_text(&self, out: &mut Actions, event: &'static str, body: String) {
        for p in self.players.iter().flatten().filter(|p| p.connected) {
            out.text(p.target(), event, body.clone());
        }
    }
    fn broadcast_binary(&self, out: &mut Actions, event: &'static str, body: Vec<u8>) {
        for p in self.players.iter().flatten().filter(|p| p.connected) {
            out.binary(p.target(), event, body.clone());
        }
    }
    fn send_binary(&self, out: &mut Actions, i: usize, event: &'static str, body: Vec<u8>) {
        if let Some(p) = self
            .players
            .get(i)
            .and_then(Option::as_ref)
            .filter(|p| p.connected)
        {
            out.binary(p.target(), event, body)
        }
    }
    fn send_text(&self, out: &mut Actions, i: usize, event: &'static str, body: String) {
        if let Some(p) = self
            .players
            .get(i)
            .and_then(Option::as_ref)
            .filter(|p| p.connected)
        {
            out.text(p.target(), event, body)
        }
    }
    fn alert(&self, out: &mut Actions, i: usize, code: u8) {
        self.send_binary(
            out,
            i,
            match self.kind {
                1 => "roulette_alert",
                2 => "blackjack_alert",
                _ => "holdemcasino_alert",
            },
            vec![code],
        )
    }
    fn broadcast_players(&self, out: &mut Actions) {
        if self.kind != 1 {
            let list = self
                .players
                .iter()
                .map(|p| {
                    p.as_ref()
                        .filter(|p| p.connected)
                        .map_or(0, |p| p.member.avatar_object)
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            self.broadcast_text(
                out,
                if self.kind == 2 {
                    "blackjack_players_update"
                } else {
                    "holdemcasino_players_update"
                },
                list,
            );
        }
    }
    fn request(
        &mut self,
        kind: u8,
        index: usize,
        primary: u32,
        secondary: u32,
        out: &mut Actions,
    ) -> Result<(), Error> {
        let member = if index == OWNER_SEAT as usize {
            self.owner.ok_or(Error::InvalidIdentity)?
        } else {
            self.player(index)?.member
        };
        let avatar = Account::Avatar(member.avatar_id);
        let object = Account::Object(self.object);
        let operation = match kind {
            PROBE_PLAYER => Operation::QueryBalances {
                source: object,
                target: avatar,
            },
            PROBE_OWNER => Operation::QueryBalances {
                source: Account::System,
                target: object,
            },
            WITHDRAW | PAYOUT | SIDE_PAYOUT | REFUND => Operation::Transfer {
                source: object,
                target: avatar,
                amount: primary,
            },
            _ => Operation::Transfer {
                source: avatar,
                target: object,
                amount: primary
                    .checked_add(secondary)
                    .ok_or(Error::InvalidPluginInput)?,
            },
        };
        self.finance.request(
            Pending {
                kind,
                seat: index as u8,
                primary,
                secondary,
                operation,
            },
            out,
        )
    }
    pub fn join(&mut self, m: Member, _r: &Roster) -> Result<Actions, Error> {
        if !valid_member(m) || m.input.role > 2 {
            return Err(Error::InvalidPluginInput);
        }
        if self.closing {
            return Err(Error::PluginNotReady);
        }
        if self
            .players
            .iter()
            .flatten()
            .any(|p| p.member.avatar_id == m.avatar_id)
            || self.dealer.is_some_and(|d| d.avatar_id == m.avatar_id)
            || self.owner.is_some_and(|o| o.avatar_id == m.avatar_id)
        {
            return Err(Error::ParticipantAlreadyConnected);
        }
        let mut out = Actions::default();
        match m.input.role {
            0 => {
                if self.owner_connected {
                    return Err(Error::PluginNotReady);
                }
                if self.finance.busy() {
                    return Err(Error::PersistencePending);
                }
                let i = if self.kind == 1 {
                    self.players
                        .iter()
                        .position(Option::is_none)
                        .ok_or(Error::ParticipantLimit)?
                } else {
                    let n = m.input.registers[3];
                    if !(1..=4).contains(&n) {
                        return Err(Error::InvalidPluginInput);
                    }
                    (n - 1) as usize
                };
                if self.players[i].is_some() {
                    return Err(Error::ParticipantLimit);
                }
                self.players[i] = Some(Player::new(m));
                self.broadcast_players(&mut out);
                self.request(PROBE_PLAYER, i, 1, 0, &mut out)?;
            }
            1 => {
                if self.dealer.is_some() || self.owner_connected {
                    return Err(Error::ParticipantLimit);
                }
                self.dealer = Some(m);
                if matches!(self.phase, CLOSED | WAITING) {
                    self.next = Some(if self.any_connected() {
                        BETTING
                    } else {
                        WAITING
                    });
                }
            }
            2 => {
                if !m.input.owner_authorized {
                    return Err(Error::NotAuthorized);
                }
                if self.owner_connected
                    || self.dealer.is_some()
                    || self
                        .players
                        .iter()
                        .flatten()
                        .any(|p| p.connected || p.accepted && !p.done)
                    || self.finance.busy()
                {
                    return Err(Error::PluginNotReady);
                }
                self.owner = Some(m);
                self.owner_connected = true;
                // Completed departed players have no remaining account claim.
                // Retaining their old wager limits would constrain owner tuning.
                self.players = std::array::from_fn(|_| None);
                self.phase = MANAGING;
                self.next = None;
                self.request(PROBE_OWNER, OWNER_SEAT as usize, 1, 0, &mut out)?;
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(out)
    }
    pub fn allows(&self, event: &str, binary: bool) -> bool {
        match self.kind {
            1 => {
                if binary {
                    event == "roulette_UI_close"
                } else {
                    matches!(
                        event,
                        "roulette_new_bet"
                            | "roulette_remove_bet"
                            | "roulette_deposit"
                            | "roulette_withdraw"
                            | "roulette_new_minimum"
                            | "roulette_new_maximum"
                    )
                }
            }
            2 => {
                if binary {
                    matches!(
                        event,
                        "blackjack_hit_request"
                            | "blackjack_stand_request"
                            | "blackjack_double_request"
                            | "blackjack_split_request"
                            | "blackjack_bet_request"
                            | "blackjack_insurance_request"
                    )
                } else {
                    matches!(
                        event,
                        "blackjack_close"
                            | "blackjack_deposit"
                            | "blackjack_withdraw"
                            | "blackjack_new_minimum"
                            | "blackjack_new_maximum"
                    )
                }
            }
            _ => {
                if binary {
                    matches!(
                        event,
                        "holdemcasino_decision" | "holdemcasino_submit_bets" | "holdemcasino_close"
                    )
                } else {
                    matches!(
                        event,
                        "holdemcasino_deposit"
                            | "holdemcasino_withdraw"
                            | "holdemcasino_new_minimum"
                            | "holdemcasino_new_maximum"
                            | "holdemcasino_new_side"
                    )
                }
            }
        }
    }
    pub fn message(
        &mut self,
        m: Member,
        event: &str,
        bytes: &[u8],
        _r: &Roster,
    ) -> Result<Actions, Error> {
        if self.closing {
            return Err(Error::PluginNotReady);
        }
        let mut out = Actions::default();
        if matches!(
            event,
            "roulette_UI_close" | "blackjack_close" | "holdemcasino_close"
        ) {
            if self.owner == Some(m) && self.owner_connected {
                out.object(
                    Target::Controller,
                    if self.kind == 1 { 3 } else { 19 },
                    vec![m.avatar_object],
                );
            } else {
                self.player_index(m)?;
                out.object(
                    Target::Controller,
                    if self.kind == 1 { 11 } else { 20 },
                    vec![m.avatar_object],
                );
            }
            out.close(Target::Member(m.seat));
            return Ok(out);
        }
        if event.ends_with("_deposit")
            || event.ends_with("_withdraw")
            || event.contains("_new_minimum")
            || event.contains("_new_maximum")
            || event.ends_with("_new_side")
        {
            self.manage(m, event, bytes, &mut out)?;
            return Ok(out);
        }
        let i = self.player_index(m)?;
        if !self.player(i)?.ready {
            return Err(Error::PluginNotReady);
        }
        if self.kind == 1 {
            self.roulette_message(i, event, bytes, &mut out)?;
            return Ok(out);
        }
        match event {
            "blackjack_bet_request" | "holdemcasino_submit_bets" => {
                if self.phase != BETTING
                    || self.next.is_some()
                    || self.player(i)?.submitted
                    || self.finance.busy()
                {
                    return Err(Error::PluginNotReady);
                }
                let (ante, side) = if self.kind == 2 {
                    (binary_number(bytes)?, 0)
                } else {
                    let fields = parse_strings(bytes, 2)?;
                    (
                        fields[0]
                            .trim()
                            .parse::<i32>()
                            .map_err(|_| Error::InvalidPluginInput)?,
                        fields[1]
                            .trim()
                            .parse::<i32>()
                            .map_err(|_| Error::InvalidPluginInput)?,
                    )
                };
                let err = if ante < self.min as i32 {
                    Some(if self.kind == 2 { 6 } else { 9 })
                } else if ante > self.max as i32 {
                    Some(if self.kind == 2 { 7 } else { 10 })
                } else if side < 0 {
                    return Err(Error::InvalidPluginInput);
                } else if side > self.max_side as i32 {
                    Some(17)
                } else if ante as u32 + side as u32 > self.player(i)?.balance {
                    Some(if self.kind == 2 { 8 } else { 11 })
                } else {
                    None
                };
                if let Some(code) = err {
                    self.alert(&mut out, i, code);
                    self.send_binary(
                        &mut out,
                        i,
                        if self.kind == 2 {
                            "blackjack_toggle_betting"
                        } else {
                            "holdemcasino_toggle_betting"
                        },
                        vec![1],
                    );
                } else {
                    self.player_mut(i)?.submitted = true;
                    self.request(BET, i, ante as u32, side as u32, &mut out)?;
                }
            }
            "blackjack_insurance_request" => self.insurance(i, bytes, &mut out)?,
            "blackjack_hit_request"
            | "blackjack_stand_request"
            | "blackjack_double_request"
            | "blackjack_split_request" => {
                if !bytes.is_empty() {
                    return Err(Error::InvalidPluginInput);
                }
                self.blackjack_move(i, event, &mut out)?;
            }
            "holdemcasino_decision" => self.holdem_move(i, bytes, &mut out)?,
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(out)
    }

    fn manage(
        &mut self,
        m: Member,
        event: &str,
        bytes: &[u8],
        out: &mut Actions,
    ) -> Result<(), Error> {
        if self.owner != Some(m) || !self.owner_connected || !m.input.owner_authorized {
            return Err(Error::NotAuthorized);
        }
        if self.phase != MANAGING || self.finance.busy() {
            return Err(Error::PluginNotReady);
        }
        let balance = self.balance.ok_or(Error::PluginNotReady)?;
        let n = text_number(bytes);
        if event.ends_with("_deposit") || event.ends_with("_withdraw") {
            let deposit = event.ends_with("_deposit");
            let error = match n {
                Ok(0) => Some("Null"),
                Ok(v) if v < 0 => Some("Invalid"),
                Err(_) => Some("Invalid"),
                Ok(v)
                    if deposit
                        && balance
                            .checked_add(v as u32)
                            .is_none_or(|s| s > self.limit()) =>
                {
                    Some("Overflow")
                }
                Ok(v) if !deposit && v as u32 > balance => Some("Overflow"),
                _ => None,
            };
            if let Some(e) = error {
                out.text(
                    Target::Member(m.seat),
                    match (self.kind, deposit) {
                        (1, true) => "roulette_deposit_fail",
                        (1, false) => "roulette_withdraw_fail",
                        (2, true) => "blackjack_deposit_fail",
                        (2, false) => "blackjack_withdraw_fail",
                        (_, true) => "holdemcasino_deposit_fail",
                        _ => "holdemcasino_withdraw_fail",
                    },
                    e.to_owned(),
                );
            } else {
                self.request(
                    if deposit { DEPOSIT } else { WITHDRAW },
                    OWNER_SEAT as usize,
                    n? as u32,
                    0,
                    out,
                )?;
            }
            return Ok(());
        }
        let mode = if event.ends_with("_new_minimum") {
            0
        } else if event.ends_with("_new_maximum") {
            1
        } else {
            2
        };
        let failure = match n {
            Err(_) => Some(if bytes.is_empty() { "Null" } else { "Invalid" }),
            Ok(v)
                if v < if mode == 2 {
                    0
                } else if mode == 1 {
                    self.min as i32
                } else {
                    1
                } =>
            {
                Some("BetTooLow")
            }
            Ok(v) if v > if mode == 0 { self.max as i32 } else { 1000 } => Some("BetTooHigh"),
            Ok(v)
                if balance
                    < match self.kind {
                        1 => v as u32 * 140,
                        2 => v as u32 * 8,
                        _ => {
                            if mode == 2 {
                                self.max * 84 + v as u32 * 104
                            } else {
                                v as u32 * 84 + self.max_side * 104
                            }
                        }
                    } =>
            {
                Some("BetTooHighForBalance")
            }
            _ => None,
        };
        if let Some(e) = failure {
            out.text(
                Target::Member(m.seat),
                match (self.kind, mode) {
                    (1, 0) => "roulette_n_bet_fail",
                    (1, _) => "roulette_x_bet_fail",
                    (2, 0) => "blackjack_n_bet_fail",
                    (2, _) => "blackjack_x_bet_fail",
                    (_, 0) => "holdemcasino_n_bet_fail",
                    (_, 1) => "holdemcasino_x_bet_fail",
                    _ => "holdemcasino_s_bet_fail",
                },
                e.to_owned(),
            );
        } else {
            let value = n? as u32;
            match mode {
                0 => self.min = value,
                1 => self.max = value,
                _ => self.max_side = value,
            };
            out.object(
                Target::Controller,
                match (self.kind, mode) {
                    (1, 0) => 8,
                    (1, _) => 9,
                    (_, 0) => 14,
                    (_, 1) => 15,
                    _ => 22,
                },
                vec![value as i16],
            );
            out.text(
                Target::Member(m.seat),
                match (self.kind, mode) {
                    (1, 0) => "roulette_min_bet_success",
                    (1, _) => "roulette_max_bet_success",
                    (2, 0) => "blackjack_min_bet_success",
                    (2, _) => "blackjack_max_bet_success",
                    (_, 0) => "holdemcasino_min_bet_success",
                    (_, 1) => "holdemcasino_max_bet_success",
                    _ => "holdemcasino_side_bet_success",
                },
                value.to_string(),
            );
        }
        Ok(())
    }

    fn roulette_message(
        &mut self,
        i: usize,
        event: &str,
        bytes: &[u8],
        out: &mut Actions,
    ) -> Result<(), Error> {
        if self.phase != BETTING || self.next.is_some() {
            return Err(Error::PluginNotReady);
        }
        let bet = match Bet::parse(bytes) {
            Ok(b) => b,
            Err(_) => {
                self.send_text(out, i, "roulette_unknown_error", String::new());
                self.sync_my_bets(i, out)?;
                return Ok(());
            }
        };
        let place = event == "roulette_new_bet";
        if !place && event != "roulette_remove_bet" {
            return Err(Error::EventNotAllowed);
        }
        let p = self.player(i)?;
        if place && p.total_bets() + bet.amount() > self.max {
            self.send_binary(out, i, "roulette_over_max", vec![bet.amount() as u8]);
            self.sync_my_bets(i, out)?;
            return Ok(());
        }
        if place && p.total_bets() + bet.amount() > p.balance {
            self.send_text(out, i, "roulette_bet_failed", p.balance.to_string());
            self.sync_my_bets(i, out)?;
            return Ok(());
        }
        let position = p.bets.iter().position(|b| b.same(&bet));
        let mut changed = false;
        if let Some(j) = position {
            let p = self.player_mut(i)?;
            changed = if place {
                p.bets[j].add(&bet)
            } else {
                p.bets[j].remove(&bet)
            };
            if p.bets[j].amount() == 0 {
                p.bets.remove(j);
            }
        } else if place {
            let p = self.player_mut(i)?;
            if p.bets.len() >= roulette::MAX_BETS {
                return Err(Error::ParticipantLimit);
            }
            p.bets.push(bet.clone());
            changed = true;
        }
        if changed {
            let avatar = self.player(i)?.member.avatar_object;
            out.object(Target::Controller, if place { 4 } else { 5 }, vec![avatar]);
            self.sync_neighbors(Some(i), out)?;
        } else if place {
            self.send_binary(out, i, "roulette_stack_overflow", vec![bet.amount() as u8]);
            self.sync_my_bets(i, out)?;
        }
        Ok(())
    }
    fn sync_my_bets(&self, i: usize, out: &mut Actions) -> Result<(), Error> {
        self.send_text(
            out,
            i,
            "roulette_sync_mine",
            roulette::sync(self.player(i)?.bets.iter())?,
        );
        Ok(())
    }
    fn sync_neighbors(&self, except: Option<usize>, out: &mut Actions) -> Result<(), Error> {
        for i in 0..4 {
            if Some(i) == except || !self.players[i].as_ref().is_some_and(|p| p.connected) {
                continue;
            }
            let data = roulette::sync(
                self.players
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .filter_map(|(_, p)| p.as_ref())
                    .flat_map(|p| p.bets.iter()),
            )?;
            self.send_text(out, i, "roulette_sync_neighbor", data);
        }
        Ok(())
    }

    fn blackjack_move(&mut self, i: usize, event: &str, out: &mut Actions) -> Result<(), Error> {
        if self.phase != PLAYER || self.next.is_some() {
            return Err(Error::PluginNotReady);
        }
        if self.active != i as i8 {
            self.alert(out, i, 2);
            return Ok(());
        }
        if self.finance.busy() {
            return Err(Error::PersistencePending);
        }
        let p = self.player(i)?;
        let active_hand = p.active_hand;
        let hand = p.hands.get(active_hand).ok_or(Error::InvalidPluginInput)?;
        let avatar = p.member.avatar_object;
        match event {
            "blackjack_hit_request" => {
                if !hand.playable() {
                    self.alert(out, i, 3);
                    return Ok(());
                }
                let card = self.shoe.draw();
                let p = self.player_mut(i)?;
                p.hands[p.active_hand].hit(card, false)?;
                self.broadcast_binary(out, "blackjack_hit_broadcast", vec![i as u8]);
                self.wait_animation(PLAYER_CALLBACK, 9, vec![avatar], out);
            }
            "blackjack_stand_request" => {
                self.player_mut(i)?.hands[active_hand].stand();
                self.broadcast_binary(out, "blackjack_stand_broadcast", vec![i as u8]);
                self.wait_animation(PLAYER_CALLBACK, 8, vec![avatar], out);
            }
            "blackjack_double_request" | "blackjack_split_request" => {
                let split = event == "blackjack_split_request";
                let allowed = if split {
                    hand.kind == 20 && p.hands.len() < 4
                } else {
                    matches!(hand.kind, 2 | 20)
                };
                if !allowed {
                    self.alert(out, i, if split { 5 } else { 4 });
                    return Ok(());
                }
                if p.balance < p.bet {
                    self.alert(out, i, if split { 10 } else { 9 });
                    return Ok(());
                }
                let bet = p.bet;
                self.phase = MOVE_PENDING;
                self.request(if split { SPLIT } else { DOUBLE }, i, bet, 0, out)?;
            }
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(())
    }
    fn insurance(&mut self, i: usize, bytes: &[u8], out: &mut Actions) -> Result<(), Error> {
        if self.phase != INSURANCE_ROUND || self.next.is_some() || self.finance.busy() {
            return Err(Error::PluginNotReady);
        }
        if bytes.len() != 1 || bytes[0] > 1 {
            return Err(Error::InvalidPluginInput);
        }
        let p = self.player(i)?;
        if !p.prompted || !p.accepted {
            return Err(Error::PluginNotReady);
        }
        let half = p.bet / 2;
        if bytes[0] == 1 && half > 0 {
            self.request(INSURANCE, i, half, 0, out)?;
        } else {
            let p = self.player_mut(i)?;
            p.prompted = false;
            p.insured = bytes[0] == 1;
            self.send_binary(
                out,
                i,
                "blackjack_insurance_callback",
                0i32.to_le_bytes().to_vec(),
            );
            self.end_insurance(out)?;
        }
        Ok(())
    }
    fn end_insurance(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.players.iter().flatten().any(|p| p.prompted)
            || self.finance.pending.values().any(|p| p.kind == INSURANCE)
        {
            return Ok(());
        }
        let bj = i16::from(self.dealer_hand.as_ref().is_some_and(|h| h.kind == 21));
        self.wait_animation(CHECK_CALLBACK, 5, vec![bj], out);
        Ok(())
    }
    fn holdem_move(&mut self, i: usize, bytes: &[u8], out: &mut Actions) -> Result<(), Error> {
        if self.phase != PLAYER || self.next.is_some() {
            return Err(Error::PluginNotReady);
        }
        if bytes.len() != 1 || bytes[0] > 1 {
            return Err(Error::InvalidPluginInput);
        }
        if self.active != i as i8 {
            self.alert(out, i, 5);
            return Ok(());
        }
        if self.finance.busy() {
            return Err(Error::PersistencePending);
        }
        let p = self.player(i)?;
        if p.folded || p.call > 0 {
            return Err(Error::PluginNotReady);
        }
        let amount = p.bet * 2;
        let avatar = p.member.avatar_object;
        if bytes[0] == 1 {
            if p.balance < amount {
                self.alert(out, i, 16);
                self.send_binary(out, i, "holdemcasino_allow_input", Vec::new());
                return Ok(());
            }
            self.phase = MOVE_PENDING;
            self.request(CALL, i, amount, 0, out)?;
        } else {
            self.player_mut(i)?.folded = true;
            self.send_binary(out, i, "holdemcasino_decision_callback", vec![0]);
            self.broadcast_binary(out, "holdemcasino_fold_broadcast", vec![i as u8 + 1]);
            self.wait_animation(PLAYER_CALLBACK, 8, vec![avatar], out);
        }
        Ok(())
    }
    fn wait_animation(&mut self, expected: u8, code: i16, args: Vec<i16>, out: &mut Actions) {
        self.phase = ANIMATION;
        self.next = None;
        self.frame = 0;
        self.expected = Some(expected);
        out.object(Target::Controller, code, args);
    }

    pub fn tick(&mut self, _r: &Roster) -> Result<Actions, Error> {
        let mut out = Actions::default();
        if self.closing {
            return Ok(out);
        }
        if let Some(next) = self.next.take() {
            self.enter(next, &mut out)?;
        }
        match self.phase {
            WAITING => {
                if self.dealer.is_some() && self.any_connected() {
                    self.next = Some(BETTING);
                }
            }
            BETTING => {
                if self.kind != 1 && self.all_submitted() {
                    self.next = Some(PENDING);
                } else if self.seconds > 0 {
                    if self.second() {
                        self.broadcast_time(&mut out);
                    }
                } else if self.kind == 1 {
                    self.next = Some(PENDING);
                } else {
                    self.end_betting(&mut out)?;
                }
            }
            PENDING => {
                if self.seconds > 0 {
                    self.second();
                } else if !self.finance.busy() {
                    if self.kind == 1 {
                        self.finish_roulette_bets(&mut out)?;
                    } else if self.any_accepted() {
                        self.deal(&mut out)?;
                    } else {
                        self.next = Some(if self.any_connected() {
                            BETTING
                        } else {
                            WAITING
                        });
                    }
                }
            }
            PLAYER => {
                if self.seconds > 0 {
                    if self.second() {
                        self.broadcast_time(&mut out);
                    }
                } else {
                    self.timeout_move(&mut out)?;
                }
            }
            INSURANCE_ROUND => {
                if self.seconds > 0 {
                    if self.second() {
                        self.broadcast_time(&mut out);
                    }
                } else {
                    for i in 0..4 {
                        if self.players[i].as_ref().is_some_and(|p| p.prompted)
                            && !self.finance.player_busy(i as u8)
                        {
                            self.player_mut(i)?.prompted = false;
                            self.send_binary(
                                &mut out,
                                i,
                                "blackjack_insurance_callback",
                                0i32.to_le_bytes().to_vec(),
                            );
                        }
                    }
                    self.end_insurance(&mut out)?;
                }
            }
            FINALE => {
                if self.seconds > 0 {
                    self.second();
                } else {
                    self.next = Some(INTERMISSION);
                }
            }
            HOLD => {
                if self.seconds > 0 {
                    self.second();
                } else {
                    self.next = Some(PLAYER);
                }
            }
            SPINNING => {
                self.frame += 1;
                if self.frame >= 360 {
                    self.next = Some(INTERMISSION);
                }
            }
            INTERMISSION if self.kind == 1 => {
                if !self.finance.owes() {
                    self.frame += 1;
                    if self.frame >= 180 {
                        self.next = Some(BETTING);
                    }
                }
            }
            _ => {}
        }
        Ok(out)
    }
    fn second(&mut self) -> bool {
        self.frame += 1;
        if self.frame >= 30 {
            self.frame = 0;
            self.seconds -= 1;
            true
        } else {
            false
        }
    }
    fn broadcast_time(&self, out: &mut Actions) {
        if self.kind == 1 {
            self.broadcast_text(out, "roulette_round_time", self.seconds.to_string());
        } else {
            self.broadcast_binary(
                out,
                if self.kind == 2 {
                    "blackjack_timer"
                } else {
                    "holdemcasino_timer"
                },
                (self.seconds as i32).to_le_bytes().to_vec(),
            );
        }
    }
    fn enter(&mut self, next: u8, out: &mut Actions) -> Result<(), Error> {
        self.phase = next;
        self.frame = 0;
        self.seconds = 0;
        match next {
            CLOSED => {
                self.abort(out)?;
            }
            WAITING => {
                self.active = -1;
                if self.kind != 1 {
                    self.shoe.shuffle(&mut self.rng, 2)?;
                    out.object(Target::Controller, 13, vec![]);
                }
            }
            BETTING => self.new_game(out)?,
            PENDING => {
                if self.kind == 1 {
                    self.lock_roulette(out)?;
                } else {
                    self.seconds = 2;
                    self.broadcast_time(out);
                }
            }
            PLAYER => {
                self.seconds = 15;
                if self.kind == 2 {
                    self.advance_blackjack(out)?;
                } else {
                    self.advance_holdem(out)?;
                }
            }
            DEALER => {
                if self.kind == 2 {
                    self.dealer_blackjack(out)?;
                } else {
                    self.dealer_holdem(out)?;
                }
            }
            INSURANCE_ROUND => {
                self.seconds = 10;
                for i in 0..4 {
                    if self.players[i]
                        .as_ref()
                        .is_some_and(|p| p.accepted && !p.done && p.connected)
                    {
                        self.player_mut(i)?.prompted = true;
                        let amount = self.player(i)?.bet / 2;
                        self.send_binary(
                            out,
                            i,
                            "blackjack_insurance_prompt",
                            (amount as i32).to_le_bytes().to_vec(),
                        );
                    }
                }
                self.end_insurance(out)?;
            }
            FINALE => {
                if self.kind == 2 {
                    self.seconds = 2;
                    let d = self.dealer_hand.as_ref().ok_or(Error::InvalidPluginInput)?;
                    let total = if d.kind == 21 { 0 } else { d.total().0 as i32 };
                    self.broadcast_binary(out, "dealer_hand_total", total.to_le_bytes().to_vec());
                } else {
                    self.settle(false, out)?;
                }
            }
            SIDE => self.settle(true, out)?,
            HOLD => self.seconds = 3,
            INTERMISSION => {
                if self.kind == 1
                    && let Some(d) = self.dealer
                {
                    out.object(Target::Controller, 2, vec![d.avatar_object]);
                }
                self.settle(false, out)?;
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    fn new_game(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.finance.busy() {
            self.phase = WAITING;
            return Ok(());
        }
        if !self.funded() || self.dealer.is_none() {
            let code = if self.broken {
                if self.kind == 1 {
                    5
                } else if self.kind == 2 {
                    15
                } else {
                    18
                }
            } else if self.kind == 1 {
                4
            } else if self.kind == 2 {
                13
            } else {
                14
            };
            for i in 0..4 {
                self.alert(out, i, code);
            }
            self.next = Some(CLOSED);
            return Ok(());
        }
        for i in 0..4 {
            if self.players[i].as_ref().is_some_and(|p| !p.connected) {
                self.players[i] = None;
            } else if let Some(p) = &mut self.players[i] {
                p.reset();
                if p.ready && p.balance < self.min {
                    let id = p.member.avatar_object;
                    let target = p.target();
                    out.binary(
                        target,
                        match self.kind {
                            1 => "roulette_gameoverNSF",
                            2 => "blackjack_alert",
                            _ => "holdemcasino_alert",
                        },
                        if self.kind == 1 {
                            vec![(self.min / 255) as u8, (self.min % 255) as u8]
                        } else {
                            vec![if self.kind == 2 { 14 } else { 15 }]
                        },
                    );
                    out.object(
                        Target::Controller,
                        if self.kind == 1 { 11 } else { 20 },
                        vec![id],
                    );
                    out.close(target);
                }
            }
        }
        self.dealer_hand = None;
        self.dealer_hole.clear();
        self.board.clear();
        self.active = -1;
        self.winning = 255;
        self.expected = None;
        self.events.clear();
        self.settlement_stage = 0;
        self.seconds = 30;
        if !self.any_connected() {
            self.next = Some(WAITING);
            return Ok(());
        }
        match self.kind {
            1 => {
                for i in 0..4 {
                    if self.players[i].is_some() {
                        self.send_binary(out, i, "roulette_new_game", self.roulette_limits(i)?);
                    }
                }
                self.sync_neighbors(None, out)?;
            }
            2 => self.broadcast_binary(
                out,
                "blackjack_new_game",
                strings(std::slice::from_ref(&self.name)),
            ),
            _ => {
                self.shoe.shuffle(&mut self.rng, 2)?;
                self.broadcast_binary(out, "holdemcasino_new_game", vec![]);
            }
        }
        Ok(())
    }
    fn end_betting(&mut self, out: &mut Actions) -> Result<(), Error> {
        self.next = Some(PENDING);
        self.broadcast_binary(
            out,
            if self.kind == 2 {
                "blackjack_toggle_betting"
            } else {
                "holdemcasino_toggle_betting"
            },
            vec![0],
        );
        for i in 0..4 {
            if self.players[i]
                .as_ref()
                .is_some_and(|p| p.connected && !p.submitted)
            {
                let warned = self.player(i)?.observed;
                self.alert(
                    out,
                    i,
                    if self.kind == 2 {
                        if warned { 12 } else { 11 }
                    } else if warned {
                        13
                    } else {
                        12
                    },
                );
                if warned {
                    let p = self.player(i)?;
                    out.object(Target::Controller, 20, vec![p.member.avatar_object]);
                    out.close(p.target());
                } else {
                    self.player_mut(i)?.observed = true;
                }
            }
        }
        Ok(())
    }
    fn roulette_limits(&self, i: usize) -> Result<Vec<u8>, Error> {
        let balance = self.player(i)?.balance;
        Ok([self.min, self.max, balance]
            .into_iter()
            .flat_map(|n| [(n / 255) as u8, (n % 255) as u8])
            .collect())
    }
    fn lock_roulette(&mut self, out: &mut Actions) -> Result<(), Error> {
        self.winning = self.rng.below(38)? as u8;
        if self.winning == 37 {
            self.winning = 100;
        }
        for i in 0..4 {
            if self.players[i].is_none() {
                continue;
            }
            let amount = self.player(i)?.total_bets();
            if amount >= self.min && self.player(i)?.connected {
                self.player_mut(i)?.submitted = true;
                self.request(BET, i, amount, 0, out)?;
            } else if amount > 0 {
                self.send_text(out, i, "roulette_under_min", amount.to_string());
                self.player_mut(i)?.bets.clear();
                self.sync_my_bets(i, out)?;
            }
        }
        self.seconds = 0;
        if !self.finance.busy() {
            self.finish_roulette_bets(out)?;
        }
        Ok(())
    }
    fn finish_roulette_bets(&mut self, out: &mut Actions) -> Result<(), Error> {
        self.phase = SPINNING;
        self.frame = 0;
        if let Some(d) = self.dealer {
            out.object(Target::Controller, 1, vec![d.avatar_object]);
        }
        for i in 0..4 {
            if let Some(p) = self.players[i].as_ref() {
                let payout = if p.accepted {
                    p.bets.iter().map(|b| b.payout(self.winning)).sum()
                } else {
                    0
                };
                self.send_text(
                    out,
                    i,
                    "roulette_spin",
                    format!("{}%{payout}", self.winning),
                );
            }
        }
        Ok(())
    }

    fn deal(&mut self, out: &mut Actions) -> Result<(), Error> {
        let mut participants = vec![0; 4];
        for (i, entry) in self.players.iter_mut().enumerate() {
            if let Some(p) = entry
                && p.accepted
                && !p.done
            {
                participants[i] = p.member.avatar_object;
                if self.kind == 2 {
                    p.hands
                        .push(BjHand::new(self.shoe.draw(), self.shoe.draw()));
                } else {
                    p.hole = vec![self.shoe.draw(), self.shoe.draw()];
                }
            }
        }
        if self.kind == 2 {
            self.dealer_hand = Some(BjHand::new(self.shoe.draw(), self.shoe.draw()));
        } else {
            self.dealer_hole = vec![self.shoe.draw(), self.shoe.draw()];
            self.board = vec![self.shoe.draw(), self.shoe.draw(), self.shoe.draw()];
        }
        self.wait_animation(INITIAL_CALLBACK, 1, participants, out);
        for i in 0..4 {
            if self.players[i].as_ref().is_some_and(|p| p.connected) {
                let cards = self.cards_for(i, false, true)?;
                self.send_binary(
                    out,
                    i,
                    if self.kind == 2 {
                        "blackjack_deal_sequence"
                    } else {
                        "holdemcasino_deal_sequence"
                    },
                    strings(&cards),
                );
            }
        }
        self.sync_bets(out);
        Ok(())
    }
    fn cards_for(
        &self,
        viewer: usize,
        reveal: bool,
        include_board: bool,
    ) -> Result<Vec<String>, Error> {
        let mut out = Vec::new();
        if self.kind == 2 {
            let mut counts = Vec::new();
            for p in &self.players {
                if let Some(p) = p.as_ref().filter(|p| p.accepted && !p.hands.is_empty()) {
                    let h = p.hands.get(p.active_hand).unwrap_or(&p.hands[0]);
                    counts.push(h.cards.len().to_string());
                    out.extend(h.names());
                } else {
                    counts.push("0".to_owned());
                }
            }
            let mut dealer = self
                .dealer_hand
                .as_ref()
                .map_or_else(Vec::new, BjHand::names);
            counts.push(dealer.len().to_string());
            if !reveal && dealer.len() > 1 {
                for c in &mut dealer[1..] {
                    *c = "Back".to_owned();
                }
            }
            out.extend(dealer);
            counts.extend(out);
            Ok(counts)
        } else {
            for (i, p) in self.players.iter().enumerate() {
                if let Some(p) = p.as_ref().filter(|p| p.accepted && p.hole.len() == 2) {
                    for c in &p.hole {
                        out.push(if i == viewer || reveal && !p.folded {
                            c.name()
                        } else {
                            "Back".to_owned()
                        });
                    }
                } else {
                    out.extend([String::new(), String::new()]);
                }
            }
            if self.dealer_hole.len() == 2 {
                for c in &self.dealer_hole {
                    out.push(if reveal { c.name() } else { "Back".to_owned() });
                }
            } else {
                out.extend(["Back".to_owned(), "Back".to_owned()]);
            }
            if include_board {
                out.extend(self.board.iter().map(|c| c.name()));
            }
            Ok(out)
        }
    }
    fn sync_bets(&self, out: &mut Actions) {
        let values = self
            .players
            .iter()
            .flat_map(|p| {
                let amounts = p.as_ref().map_or((0, 0), |p| (p.bet, p.side));
                if self.kind == 2 {
                    vec![amounts.0.to_string()]
                } else {
                    vec![amounts.0.to_string(), amounts.1.to_string()]
                }
            })
            .collect::<Vec<_>>();
        self.broadcast_binary(
            out,
            if self.kind == 2 {
                "blackjack_sync_accepted_bets"
            } else {
                "holdemcasino_sync_accepted_bets"
            },
            strings(&values),
        );
    }
    fn sync_bet(&self, i: usize, out: &mut Actions) -> Result<(), Error> {
        let p = self.player(i)?;
        if self.kind == 2 {
            self.broadcast_text(
                out,
                [
                    "blackjack_bet_update_player0",
                    "blackjack_bet_update_player1",
                    "blackjack_bet_update_player2",
                    "blackjack_bet_update_player3",
                ][i],
                p.contributed.to_string(),
            );
        } else if self.kind == 3 {
            self.broadcast_binary(
                out,
                [
                    "holdemcasino_antebet_update_player1",
                    "holdemcasino_antebet_update_player2",
                    "holdemcasino_antebet_update_player3",
                    "holdemcasino_antebet_update_player4",
                ][i],
                strings(&[p.bet.to_string()]),
            );
            self.broadcast_binary(
                out,
                [
                    "holdemcasino_sidebet_update_player1",
                    "holdemcasino_sidebet_update_player2",
                    "holdemcasino_sidebet_update_player3",
                    "holdemcasino_sidebet_update_player4",
                ][i],
                strings(&[p.side.to_string()]),
            );
        }
        Ok(())
    }
    fn sync_blackjack(&self, i: usize, out: &mut Actions) -> Result<(), Error> {
        let p = self.player(i)?;
        let Some(h) = p.hands.get(p.active_hand) else {
            return Ok(());
        };
        let cards = h.names();
        let mut broadcast = vec![i.to_string()];
        broadcast.extend(cards.clone());
        self.broadcast_binary(out, "blackjack_sync_player", strings(&broadcast));
        let mut own = vec![p.active_hand.to_string()];
        own.extend(cards);
        self.send_binary(out, i, "blackjack_active_hand", strings(&own));
        Ok(())
    }
    fn enable_blackjack(&self, i: usize, out: &mut Actions) -> Result<(), Error> {
        let p = self.player(i)?;
        let h = p
            .hands
            .get(p.active_hand)
            .ok_or(Error::InvalidPluginInput)?;
        let name = if h.kind == 20 && p.hands.len() < 4 && p.balance >= p.bet {
            "blackjack_resume_split"
        } else if matches!(h.kind, 2 | 20) && p.balance >= p.bet {
            "blackjack_resume_double"
        } else {
            "blackjack_resume_hand"
        };
        let mut cards = vec![p.active_hand.to_string()];
        cards.extend(h.names());
        self.send_binary(out, i, name, strings(&cards));
        Ok(())
    }
    fn advance_blackjack(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.active < 0 {
            self.active = 0;
        }
        while self.active < 4 {
            let i = self.active as usize;
            if self.players[i]
                .as_ref()
                .is_none_or(|p| !p.accepted || p.done || p.hands.is_empty())
            {
                self.active += 1;
                continue;
            }
            loop {
                let p = self.player(i)?;
                let h = p
                    .hands
                    .get(p.active_hand)
                    .ok_or(Error::InvalidPluginInput)?;
                if h.playable() && p.connected {
                    self.sync_blackjack(i, out)?;
                    self.enable_blackjack(i, out)?;
                    return Ok(());
                }
                if h.playable() {
                    let n = p.active_hand;
                    self.player_mut(i)?.hands[n].stand();
                }
                self.sync_blackjack(i, out)?;
                let p = self.player(i)?;
                let h = &p.hands[p.active_hand];
                self.send_binary(
                    out,
                    i,
                    if h.kind == 21 {
                        "blackjack_blackjack"
                    } else if matches!(h.kind, 4 | 5) {
                        "blackjack_double"
                    } else {
                        "blackjack_stand"
                    },
                    strings(&h.names()),
                );
                if p.active_hand + 1 < p.hands.len() {
                    self.player_mut(i)?.active_hand += 1;
                } else {
                    break;
                }
            }
            self.active += 1;
        }
        self.next = Some(DEALER);
        Ok(())
    }
    fn advance_holdem(&mut self, out: &mut Actions) -> Result<(), Error> {
        self.active += 1;
        while self.active < 4 {
            let i = self.active as usize;
            if self.players[i]
                .as_ref()
                .is_some_and(|p| p.accepted && !p.done)
            {
                if self.player(i)?.connected {
                    self.send_binary(out, i, "holdemcasino_allow_input", vec![]);
                    self.broadcast_binary(
                        out,
                        "holdemcasino_set_active_player",
                        ((i + 1) as i32).to_le_bytes().to_vec(),
                    );
                    return Ok(());
                } else {
                    self.player_mut(i)?.folded = true;
                }
            }
            self.active += 1;
        }
        self.next = Some(DEALER);
        Ok(())
    }
    fn dealer_blackjack(&mut self, out: &mut Actions) -> Result<(), Error> {
        self.active = 4;
        let hand = self.dealer_hand.as_ref().ok_or(Error::InvalidPluginInput)?;
        self.broadcast_binary(out, "blackjack_sync_dealer", strings(&hand.names()));
        let (total, soft) = hand.total();
        if total < 17 || total == 17 && soft {
            let card = self.shoe.draw();
            self.dealer_hand
                .as_mut()
                .ok_or(Error::InvalidPluginInput)?
                .hit(card, false)?;
            self.wait_animation(
                DEALER_CALLBACK,
                12,
                vec![self.dealer.ok_or(Error::InvalidIdentity)?.avatar_object],
                out,
            );
        } else {
            self.dealer_hand
                .as_mut()
                .ok_or(Error::InvalidPluginInput)?
                .stand();
            self.next = Some(FINALE);
        }
        Ok(())
    }
    fn dealer_holdem(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.board.len() == 3
            && self
                .players
                .iter()
                .flatten()
                .any(|p| p.accepted && !p.folded && p.call > 0)
        {
            self.board.extend([self.shoe.draw(), self.shoe.draw()]);
            self.broadcast_binary(
                out,
                "holdemcasino_final_deal_sequence",
                strings(&self.board[3..].iter().map(|c| c.name()).collect::<Vec<_>>()),
            );
            let ids = self
                .players
                .iter()
                .map(|p| {
                    p.as_ref()
                        .filter(|p| p.accepted && !p.folded && p.call > 0)
                        .map_or(0, |p| p.member.avatar_object)
                })
                .collect();
            self.wait_animation(COMMUNITY_CALLBACK, 17, ids, out);
        } else {
            self.next = Some(FINALE);
        }
        Ok(())
    }
    fn timeout_move(&mut self, out: &mut Actions) -> Result<(), Error> {
        let i = usize::try_from(self.active).map_err(|_| Error::InvalidPluginInput)?;
        if self.kind == 2 {
            self.send_binary(out, i, "blackjack_disable_hand_buttons", vec![0]);
            self.blackjack_move(i, "blackjack_stand_request", out)
        } else {
            self.holdem_move(i, &[0], out)
        }
    }

    pub fn vm_event(&mut self, input: &VmInput, _r: &Roster) -> Result<Actions, Error> {
        let mut out = Actions::default();
        match *input {
            VmInput::RetrySettlements => {
                self.finance.retry(&mut out)?;
            }
            VmInput::SetBroken(b) => {
                self.broken = b;
                if b && matches!(self.phase, BETTING | WAITING) {
                    self.next = Some(CLOSED);
                }
            }
            VmInput::SetEnabled(b) => {
                self.enabled = b;
                if !b && matches!(self.phase, BETTING | WAITING) {
                    self.next = Some(CLOSED);
                }
            }
            VmInput::SourceEvent { code } => {
                if self.closing {
                    return Err(Error::PluginNotReady);
                }
                let expected = self.expected.ok_or(Error::PluginNotReady)?;
                let wanted = match expected {
                    SPLIT_CALLBACK => 102,
                    CHECK_CALLBACK => 101,
                    SIDE_CALLBACK | PAY_CALLBACK | COLLECT_CALLBACK => 103,
                    _ => 100,
                };
                if code != wanted {
                    return Err(Error::EventNotAllowed);
                }
                self.expected = None;
                match expected {
                    INITIAL_CALLBACK => {
                        self.next = Some(if self.kind == 3 {
                            SIDE
                        } else if self
                            .dealer_hand
                            .as_ref()
                            .is_some_and(|h| h.cards[0].rank() == 1)
                        {
                            INSURANCE_ROUND
                        } else {
                            PLAYER
                        });
                    }
                    PLAYER_CALLBACK => self.next = Some(PLAYER),
                    DEALER_CALLBACK => self.next = Some(DEALER),
                    SPLIT_CALLBACK => {
                        self.split_cards(&mut out)?;
                        self.next = Some(PLAYER);
                    }
                    CHECK_CALLBACK => {
                        let natural = self.dealer_hand.as_ref().is_some_and(|h| h.kind == 21);
                        self.broadcast_binary(
                            &mut out,
                            "dealer_blackjack_result",
                            vec![u8::from(natural)],
                        );
                        if natural {
                            self.active = 4;
                            self.next = Some(DEALER);
                        } else {
                            self.next = Some(PLAYER);
                        }
                    }
                    COMMUNITY_CALLBACK => self.next = Some(FINALE),
                    SIDE_CALLBACK | PAY_CALLBACK => self.dealer_queue(&mut out)?,
                    COLLECT_CALLBACK => {
                        self.next = Some(if self.any_connected() {
                            BETTING
                        } else {
                            WAITING
                        })
                    }
                    _ => return Err(Error::InvalidPluginInput),
                }
            }
        }
        Ok(out)
    }
    fn split_cards(&mut self, out: &mut Actions) -> Result<(), Error> {
        let i = usize::try_from(self.active).map_err(|_| Error::InvalidPluginInput)?;
        let p = self.player(i)?;
        let n = p.active_hand;
        if p.hands.len() >= 4 || p.hands.get(n).is_none_or(|h| h.kind != 20) {
            return Err(Error::InvalidPluginInput);
        }
        let old = p.hands[n].cards.clone();
        let a = self.shoe.draw();
        let b = self.shoe.draw();
        let p = self.player_mut(i)?;
        p.hands[n] = BjHand::new(old[1], a);
        p.hands.insert(n + 1, BjHand::new(old[0], b));
        let mut data = vec![n.to_string()];
        data.extend(p.hands[n].names());
        data.extend(p.hands[n + 1].names());
        self.send_binary(out, i, "blackjack_split", strings(&data));
        Ok(())
    }

    fn payout_amount(&self, i: usize, side: bool) -> Result<u32, Error> {
        let p = self.player(i)?;
        if !p.accepted {
            return Err(Error::InvalidPluginInput);
        }
        if side {
            if self.kind != 3 || self.board.len() < 3 || p.hole.len() != 2 {
                return Err(Error::InvalidPluginInput);
            }
            let mut cards = p.hole.clone();
            cards.extend_from_slice(&self.board[..3]);
            return Ok(poker(&cards)?.side_payout(p.side));
        }
        if self.kind == 1 {
            if self.winning == 255 {
                return Err(Error::InvalidPluginInput);
            }
            Ok(p.bets.iter().map(|b| b.payout(self.winning)).sum())
        } else if self.kind == 2 {
            let d = self.dealer_hand.as_ref().ok_or(Error::InvalidPluginInput)?;
            if p.hands.is_empty() {
                return Err(Error::InvalidPluginInput);
            }
            Ok(p.hands.iter().map(|h| h.payout(d, p.bet)).sum::<u32>()
                + if d.kind == 21 && p.insured { p.bet } else { 0 })
        } else if p.folded || p.call == 0 {
            Ok(0)
        } else {
            if self.board.len() != 5 || p.hole.len() != 2 || self.dealer_hole.len() != 2 {
                return Err(Error::InvalidPluginInput);
            }
            let mut cards = p.hole.clone();
            cards.extend_from_slice(&self.board);
            let mut dealer = self.dealer_hole.clone();
            dealer.extend_from_slice(&self.board);
            Ok(poker(&cards)?.payout(&poker(&dealer)?, p.bet, p.call).0)
        }
    }
    fn settle(&mut self, side: bool, out: &mut Actions) -> Result<(), Error> {
        self.settlement_stage = if side { 1 } else { 2 };
        self.events.clear();
        if self.kind == 3 && !side && self.board.len() == 5 {
            for i in 0..4 {
                if self.players[i].as_ref().is_some_and(|p| p.connected) {
                    self.send_binary(
                        out,
                        i,
                        "holdemcasino_sync_hands_up",
                        strings(&self.cards_for(i, true, false)?),
                    );
                }
            }
        }
        for i in 0..4 {
            let Some(p) = self.players[i].as_ref() else {
                continue;
            };
            if !p.accepted || if side { p.side_done } else { p.done } {
                continue;
            }
            let amount = self.payout_amount(i, side)?;
            if side {
                self.player_mut(i)?.side_done = true;
            } else {
                self.player_mut(i)?.done = true;
            }
            if amount > 0 {
                self.request(if side { SIDE_PAYOUT } else { PAYOUT }, i, amount, 0, out)?;
            } else {
                self.settlement_output(i, 0, side, out)?;
            }
        }
        if self.finance.owes() {
            self.phase = SETTLING;
        } else {
            self.finish_settlements(out)?;
        }
        Ok(())
    }
    fn settlement_output(
        &mut self,
        i: usize,
        amount: u32,
        side: bool,
        out: &mut Actions,
    ) -> Result<(), Error> {
        let p = self.player(i)?;
        if !p.connected {
            return Ok(());
        }
        let avatar = p.member.avatar_object;
        if self.kind == 1 {
            if amount > 0 {
                out.object(Target::Controller, 6, vec![avatar, amount as i16]);
                self.send_text(out, i, "roulette_sound_play", "ui_moneyback".to_owned());
            } else {
                out.object(Target::Controller, 7, vec![avatar]);
            }
            return Ok(());
        }
        if self.kind == 2 {
            self.send_binary(
                out,
                i,
                "blackjack_win_loss_message",
                (amount as i32).to_le_bytes().to_vec(),
            );
        } else {
            let event = if side {
                "holdemcasino_sidebet_win_message"
            } else {
                "holdemcasino_win_loss_message"
            };
            let mut data = Vec::new();
            if side {
                if amount > 0 {
                    let mut hand = p.hole.clone();
                    hand.extend_from_slice(&self.board[..3]);
                    data.push(amount.to_string());
                    data.extend(poker(&hand)?.description());
                }
            } else if !p.folded && p.call > 0 {
                let mut hand = p.hole.clone();
                hand.extend_from_slice(&self.board);
                let mut dealer = self.dealer_hole.clone();
                dealer.extend_from_slice(&self.board);
                let player = poker(&hand)?;
                let dealer = poker(&dealer)?;
                let (_, prefix) = player.payout(&dealer, p.bet, p.call);
                data.push(if amount > 0 && prefix != ' ' {
                    format!("{prefix}{amount}")
                } else {
                    amount.to_string()
                });
                data.extend(if amount > 0 {
                    player.description()
                } else {
                    dealer.description()
                });
            }
            self.send_binary(out, i, event, strings(&data));
            if side && amount == 0 {
                return Ok(());
            }
        }
        if amount > 0 {
            out.object(
                Target::Controller,
                if side { 23 } else { 6 },
                vec![avatar, amount as i16],
            );
            self.events.push((3, vec![avatar]));
        } else if !side {
            out.object(Target::Controller, 7, vec![avatar]);
            self.events.push((2, vec![avatar]));
        }
        Ok(())
    }
    fn finish_settlements(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.finance.owes() {
            return Ok(());
        }
        if self.closing {
            return Ok(());
        }
        if self.kind == 1 {
            self.phase = INTERMISSION;
            self.frame = 0;
        } else {
            self.phase = if self.settlement_stage == 1 {
                SIDE
            } else {
                INTERMISSION
            };
            self.dealer_queue(out)?;
        }
        Ok(())
    }
    fn dealer_queue(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.finance.owes() {
            return Err(Error::PersistencePending);
        }
        if !self.events.is_empty() {
            let (code, args) = self.events.remove(0);
            self.frame = 1;
            self.expected = Some(if self.settlement_stage == 1 {
                SIDE_CALLBACK
            } else {
                PAY_CALLBACK
            });
            out.object(Target::Controller, code, args);
        } else if self.settlement_stage == 1 {
            self.expected = None;
            self.next = Some(if self.phase == SIDE && self.frame == 0 {
                HOLD
            } else {
                PLAYER
            });
            self.frame = 1;
        } else {
            self.expected = Some(COLLECT_CALLBACK);
            out.object(Target::Controller, 13, vec![]);
        }
        Ok(())
    }

    pub fn reply(&mut self, id: u64, reply: &Reply, _r: &Roster) -> Result<Actions, Error> {
        // A provider balance snapshot has no version. Accept callbacks only in
        // the same immutable allocation order used by the native dispatcher.
        if self.finance.pending.first_key_value().map(|(key, _)| *key) != Some(id) {
            return Err(Error::ProviderReceiptMismatch);
        }
        let (p, result) = self.finance.complete(id, reply)?;
        let mut out = Actions::default();
        let i = p.seat as usize;
        let was_probe = p.kind == PROBE_PLAYER;
        if result.success {
            let (source, target, _) = p.operation.details();
            if source == Account::Object(self.object) {
                self.balance = result.source_balance;
            } else if target == Account::Object(self.object) {
                self.balance = result.target_balance;
            }
            if i != OWNER_SEAT as usize {
                let balance = if source == Account::Avatar(self.player(i)?.member.avatar_id) {
                    result.source_balance
                } else {
                    result.target_balance
                };
                if let Some(b) = balance {
                    self.player_mut(i)?.balance = b;
                }
            }
        }
        match p.kind {
            PROBE_PLAYER => {
                if result.success {
                    self.player_mut(i)?.ready = true;
                    if self.closing {
                        // Teardown retains only private financial bookkeeping;
                        // a late balance query must not start a kick animation.
                    } else if self.funded() {
                        self.show_player(i, &mut out)?;
                        if self.dealer.is_some() && matches!(self.phase, CLOSED | WAITING) {
                            self.next = Some(BETTING);
                        }
                    } else {
                        self.alert(
                            &mut out,
                            i,
                            if self.kind == 1 {
                                4
                            } else if self.kind == 2 {
                                13
                            } else {
                                14
                            },
                        );
                        let player = self.player(i)?;
                        out.object(
                            Target::Controller,
                            if self.kind == 1 { 11 } else { 20 },
                            vec![player.member.avatar_object],
                        );
                        if player.connected {
                            out.close(player.target());
                        }
                    }
                } else {
                    let player = self.player(i)?;
                    if player.connected {
                        out.close(player.target());
                    }
                }
            }
            PROBE_OWNER => {
                if result.success && self.owner_connected {
                    let owner = self.owner.ok_or(Error::InvalidIdentity)?;
                    out.text(
                        Target::Member(owner.seat),
                        match self.kind {
                            1 => "roulette_manage",
                            2 => "blackjack_owner_show",
                            _ => "holdemcasino_owner_show",
                        },
                        self.manage_text(),
                    );
                } else if self.owner_connected {
                    let m = self.owner.ok_or(Error::InvalidIdentity)?;
                    out.object(
                        Target::Controller,
                        if self.kind == 1 { 3 } else { 19 },
                        vec![m.avatar_object],
                    );
                    out.close(Target::Member(m.seat));
                }
            }
            DEPOSIT | WITHDRAW => {
                if self.owner_connected {
                    let target = Target::Member(self.owner.ok_or(Error::InvalidIdentity)?.seat);
                    if result.success {
                        out.text(
                            target,
                            match self.kind {
                                1 => "roulette_resume_manage",
                                2 => "blackjack_resume_manage",
                                _ => "holdemcasino_resume_manage",
                            },
                            self.balance.unwrap_or(0).to_string(),
                        );
                    } else if p.kind == DEPOSIT {
                        out.text(
                            target,
                            match self.kind {
                                1 => "roulette_deposit_NSF",
                                2 => "blackjack_deposit_NSF",
                                _ => "holdemcasino_deposit_NSF",
                            },
                            p.primary.to_string(),
                        );
                    } else {
                        out.text(
                            target,
                            match self.kind {
                                1 => "roulette_withdraw_fail",
                                2 => "blackjack_withdraw_fail",
                                _ => "holdemcasino_withdraw_fail",
                            },
                            "Unknown".to_owned(),
                        );
                    }
                }
            }
            BET => {
                if result.success {
                    let player = self.player_mut(i)?;
                    player.bet = p.primary;
                    player.side = p.secondary;
                    player.contributed = p.primary + p.secondary;
                    player.accepted = true;
                    player.submitted = true;
                    player.observed = false;
                    let connected = player.connected;
                    if self.closing || !connected {
                        self.refund(i, &mut out)?;
                    } else {
                        let avatar = self.player(i)?.member.avatar_object;
                        if self.kind == 1 {
                            out.object(Target::Controller, 10, vec![avatar, p.primary as i16]);
                            self.send_text(
                                &mut out,
                                i,
                                "roulette_sound_play",
                                "ui_object_place".to_owned(),
                            );
                        } else {
                            self.send_binary(
                                &mut out,
                                i,
                                if self.kind == 2 {
                                    "blackjack_toggle_betting"
                                } else {
                                    "holdemcasino_toggle_betting"
                                },
                                vec![0],
                            );
                            self.send_binary(
                                &mut out,
                                i,
                                if self.kind == 2 {
                                    "blackjack_change_bet"
                                } else {
                                    "holdemcasino_bet_callback"
                                },
                                if self.kind == 2 {
                                    (p.primary as i32).to_le_bytes().to_vec()
                                } else {
                                    strings(&[p.primary.to_string(), p.secondary.to_string()])
                                },
                            );
                            out.object(
                                Target::Controller,
                                4,
                                vec![avatar, (p.primary + p.secondary) as i16],
                            );
                            self.sync_bet(i, &mut out)?;
                            if self.all_submitted() {
                                self.next = Some(PENDING);
                            }
                        }
                    }
                } else {
                    let player = self.player_mut(i)?;
                    player.submitted = false;
                    player.accepted = false;
                    player.bet = 0;
                    player.side = 0;
                    if self.kind == 1 {
                        self.send_text(
                            &mut out,
                            i,
                            "roulette_bet_failed",
                            self.player(i)?.balance.to_string(),
                        );
                        self.player_mut(i)?.bets.clear();
                        self.sync_my_bets(i, &mut out)?;
                    } else {
                        self.alert(&mut out, i, if self.kind == 2 { 8 } else { 11 });
                        if self.phase == BETTING {
                            self.send_binary(
                                &mut out,
                                i,
                                if self.kind == 2 {
                                    "blackjack_toggle_betting"
                                } else {
                                    "holdemcasino_toggle_betting"
                                },
                                vec![1],
                            );
                        }
                    }
                }
            }
            DOUBLE | SPLIT | CALL => {
                if result.success {
                    self.player_mut(i)?.contributed += p.primary;
                    let avatar = self.player(i)?.member.avatar_object;
                    if self.closing {
                        self.refund(i, &mut out)?;
                    } else if !self.player(i)?.connected {
                        if p.kind == SPLIT {
                            self.split_cards(&mut out)?;
                        } else if p.kind == DOUBLE {
                            let card = self.shoe.draw();
                            let player = self.player_mut(i)?;
                            player.hands[player.active_hand].hit(card, true)?;
                        } else {
                            let player = self.player_mut(i)?;
                            player.call = p.primary;
                            player.folded = false;
                        }
                        if self.kind == 2 {
                            for h in &mut self.player_mut(i)?.hands {
                                h.stand();
                            }
                        }
                        self.phase = PLAYER;
                        self.expected = None;
                        self.next = Some(PLAYER);
                    } else if p.kind == SPLIT {
                        self.broadcast_binary(&mut out, "blackjack_split_broadcast", vec![i as u8]);
                        self.wait_animation(
                            SPLIT_CALLBACK,
                            11,
                            vec![avatar, p.primary as i16],
                            &mut out,
                        );
                    } else if p.kind == DOUBLE {
                        let card = self.shoe.draw();
                        let player = self.player_mut(i)?;
                        player.hands[player.active_hand].hit(card, true)?;
                        self.broadcast_binary(
                            &mut out,
                            "blackjack_double_broadcast",
                            vec![i as u8],
                        );
                        self.wait_animation(
                            PLAYER_CALLBACK,
                            10,
                            vec![avatar, p.primary as i16],
                            &mut out,
                        );
                    } else {
                        self.player_mut(i)?.call = p.primary;
                        self.send_binary(&mut out, i, "holdemcasino_decision_callback", vec![1]);
                        self.broadcast_binary(
                            &mut out,
                            "holdemcasino_call_broadcast",
                            vec![i as u8 + 1],
                        );
                        out.object(Target::Controller, 16, vec![avatar, p.primary as i16]);
                        self.wait_animation(
                            PLAYER_CALLBACK,
                            21,
                            vec![avatar, p.primary as i16],
                            &mut out,
                        );
                    }
                    if self.kind == 2 {
                        self.sync_bet(i, &mut out)?;
                    }
                } else if !self.closing {
                    self.phase = PLAYER;
                    self.seconds = 15;
                    self.frame = 0;
                    self.alert(
                        &mut out,
                        i,
                        if p.kind == CALL {
                            16
                        } else if p.kind == SPLIT {
                            10
                        } else {
                            9
                        },
                    );
                    if self.player(i)?.connected {
                        if self.kind == 2 {
                            self.enable_blackjack(i, &mut out)?;
                        } else {
                            self.send_binary(&mut out, i, "holdemcasino_allow_input", vec![]);
                        }
                    } else {
                        self.next = Some(PLAYER);
                    }
                }
            }
            INSURANCE => {
                self.player_mut(i)?.prompted = false;
                if result.success {
                    let player = self.player_mut(i)?;
                    player.insured = true;
                    player.contributed += p.primary;
                    let avatar = player.member.avatar_object;
                    if self.closing {
                        self.refund(i, &mut out)?;
                    } else {
                        self.send_binary(
                            &mut out,
                            i,
                            "blackjack_insurance_callback",
                            (p.primary as i32).to_le_bytes().to_vec(),
                        );
                        out.object(Target::Controller, 16, vec![avatar, p.primary as i16]);
                    }
                } else {
                    self.alert(&mut out, i, 8);
                    self.send_binary(
                        &mut out,
                        i,
                        "blackjack_insurance_callback",
                        0i32.to_le_bytes().to_vec(),
                    );
                }
                if !self.closing {
                    self.end_insurance(&mut out)?;
                }
            }
            PAYOUT | SIDE_PAYOUT | REFUND => {
                if result.success {
                    self.player_mut(i)?.received = self
                        .player(i)?
                        .received
                        .checked_add(p.primary)
                        .ok_or(Error::CounterExhausted)?;
                    if p.kind != REFUND && !self.closing {
                        self.settlement_output(i, p.primary, p.kind == SIDE_PAYOUT, &mut out)?;
                    }
                    if !self.finance.owes() && p.kind != REFUND {
                        self.finish_settlements(&mut out)?;
                    }
                } else {
                    let refund = p.kind == REFUND;
                    self.finance.retain_denied(p)?;
                    if !refund {
                        self.phase = SETTLING;
                    }
                }
            }
            _ => return Err(Error::ProviderReceiptMismatch),
        }
        if self.closing && !self.finance.busy() {
            for i in 0..4 {
                if self.players[i]
                    .as_ref()
                    .is_some_and(|p| p.accepted && !p.done)
                {
                    self.refund(i, &mut out)?;
                }
            }
        }
        if !self.owner_connected && !self.finance.player_busy(OWNER_SEAT) {
            self.owner = None;
        }
        if was_probe
            && self
                .players
                .get(i)
                .and_then(Option::as_ref)
                .is_some_and(|p| !p.connected && !p.accepted)
            && !self.finance.player_busy(i as u8)
        {
            self.players[i] = None;
        }
        Ok(out)
    }
    fn manage_text(&self) -> String {
        if self.kind == 3 {
            format!(
                "{}%{}%{}%{}",
                self.balance.unwrap_or(0),
                self.min,
                self.max,
                self.max_side
            )
        } else {
            format!("{}%{}%{}", self.balance.unwrap_or(0), self.min, self.max)
        }
    }
    fn show_player(&self, i: usize, out: &mut Actions) -> Result<(), Error> {
        if !self.player(i)?.connected {
            return Ok(());
        }
        if self.kind == 1 {
            self.send_binary(out, i, "roulette_player", self.roulette_limits(i)?);
            if self.phase == BETTING {
                self.send_binary(out, i, "roulette_new_game", self.roulette_limits(i)?);
                self.sync_my_bets(i, out)?;
                self.sync_neighbors(None, out)?;
            } else if self.phase == SPINNING {
                self.send_text(out, i, "roulette_spin", format!("{}%0", self.winning));
            }
            return Ok(());
        }
        let mut data = vec![i.to_string(), self.min.to_string(), self.max.to_string()];
        if self.kind == 3 {
            data.push(self.max_side.to_string());
        }
        data.push(self.dealer.map_or(0, |m| m.avatar_object).to_string());
        self.send_binary(
            out,
            i,
            if self.kind == 2 {
                "blackjack_player_show"
            } else {
                "holdemcasino_player_show"
            },
            strings(&data),
        );
        self.sync_bets(out);
        if !matches!(self.phase, CLOSED | WAITING | BETTING | MANAGING) {
            let reveal = if self.kind == 2 {
                matches!(self.phase, DEALER | FINALE | INTERMISSION | SETTLING) || self.active == 4
            } else {
                self.settlement_stage == 2 && self.board.len() == 5
            };
            self.send_binary(
                out,
                i,
                if self.kind == 2 {
                    "blackjack_sync_all_hands"
                } else {
                    "holdemcasino_sync_hands_up"
                },
                strings(&self.cards_for(i, reveal, false)?),
            );
            if self.kind == 3 {
                self.send_binary(
                    out,
                    i,
                    "holdemcasino_sync_community",
                    strings(&self.board.iter().map(|c| c.name()).collect::<Vec<_>>()),
                );
            }
        }
        self.send_binary(
            out,
            i,
            if self.kind == 2 {
                "blackjack_toggle_betting"
            } else {
                "holdemcasino_toggle_betting"
            },
            vec![u8::from(
                self.phase == BETTING && self.next.is_none() && !self.player(i)?.submitted,
            )],
        );
        if self.phase == PLAYER && self.active == i as i8 {
            if self.kind == 2 {
                self.enable_blackjack(i, out)?;
            } else {
                self.send_binary(out, i, "holdemcasino_allow_input", vec![]);
            }
        }
        if self.phase == INSURANCE_ROUND && self.player(i)?.prompted {
            self.send_binary(
                out,
                i,
                "blackjack_insurance_prompt",
                ((self.player(i)?.bet / 2) as i32).to_le_bytes().to_vec(),
            );
        }
        Ok(())
    }
    fn refund(&mut self, i: usize, out: &mut Actions) -> Result<(), Error> {
        let p = self.player(i)?;
        if p.done || self.finance.player_busy(i as u8) {
            return Ok(());
        }
        let amount = p.contributed.saturating_sub(p.received);
        self.player_mut(i)?.done = true;
        self.player_mut(i)?.refunding = true;
        if amount > 0 {
            self.request(REFUND, i, amount, 0, out)?;
        }
        Ok(())
    }
    fn abort(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.closing {
            return Ok(());
        }
        self.closing = true;
        // Closing no longer awaits the gameplay move or animation phase.
        // Pending debit receipts remain valid through the closing exception.
        self.phase = CLOSED;
        self.next = None;
        self.expected = None;
        self.events.clear();
        for i in 0..4 {
            if let Some(p) = self.players[i].as_mut() {
                p.prompted = false;
                if p.connected {
                    out.object(
                        Target::Controller,
                        if self.kind == 1 { 11 } else { 20 },
                        vec![p.member.avatar_object],
                    );
                    out.close(p.target());
                    p.connected = false;
                }
                if p.accepted {
                    self.refund(i, out)?;
                }
            }
        }
        if let Some(d) = self.dealer.take() {
            out.object(
                Target::Controller,
                if self.kind == 1 { 3 } else { 19 },
                vec![d.avatar_object],
            );
            out.close(Target::Member(d.seat));
        }
        if self.owner_connected {
            if let Some(o) = self.owner {
                out.close(Target::Member(o.seat));
            }
            self.owner_connected = false;
        }
        out.close(Target::Controller);
        Ok(())
    }
    pub fn leave(&mut self, m: Member, _r: &Roster) -> Result<Actions, Error> {
        if self.closing {
            return Ok(Actions::default());
        }
        let mut out = Actions::default();
        if self.dealer == Some(m) {
            self.dealer = None;
            self.abort(&mut out)?;
            return Ok(out);
        }
        if self.owner == Some(m) && self.owner_connected {
            self.owner_connected = false;
            self.phase = CLOSED;
            if !self.finance.player_busy(OWNER_SEAT) {
                self.owner = None;
            }
            return Ok(out);
        }
        let i = self.player_index(m)?;
        self.player_mut(i)?.connected = false;
        if self.kind != 1 {
            out.object(Target::Controller, 18, vec![(i + 1) as i16]);
        }
        self.broadcast_players(&mut out);
        if self.player(i)?.accepted && matches!(self.phase, BETTING | PENDING) {
            self.refund(i, &mut out)?;
        } else if self.kind == 2 && self.player(i)?.accepted {
            if self.active == i as i8 && self.expected == Some(SPLIT_CALLBACK) {
                self.split_cards(&mut out)?;
                self.expected = None;
                self.next = Some(PLAYER);
            }
            if !self
                .finance
                .pending
                .values()
                .any(|p| p.seat == i as u8 && matches!(p.kind, SPLIT | DOUBLE))
            {
                for h in &mut self.player_mut(i)?.hands {
                    h.stand();
                }
            }
            if self.active == i as i8
                && (self.phase == PLAYER || self.expected == Some(PLAYER_CALLBACK))
            {
                self.expected = None;
                self.next = Some(PLAYER);
            }
        } else if self.kind == 3 && self.player(i)?.accepted {
            if self.player(i)?.call == 0 {
                self.player_mut(i)?.folded = true;
            }
            if self.active == i as i8 && self.phase == PLAYER {
                self.next = Some(PLAYER);
            }
        }
        if !self.player(i)?.accepted && !self.finance.player_busy(i as u8) {
            self.players[i] = None;
        }
        if self.kind == 1 && self.phase == BETTING {
            self.sync_neighbors(None, &mut out)?;
        }
        Ok(out)
    }
    pub fn rebind(&mut self, m: Member, _r: &Roster) -> Result<Actions, Error> {
        let mut out = Actions::default();
        if self.owner == Some(m) && self.owner_connected {
            if self.balance.is_some() {
                out.text(
                    Target::Member(m.seat),
                    match self.kind {
                        1 => "roulette_manage",
                        2 => "blackjack_owner_show",
                        _ => "holdemcasino_owner_show",
                    },
                    self.manage_text(),
                );
            }
        } else if self.dealer != Some(m) {
            let i = self.player_index(m)?;
            if self.player(i)?.ready {
                self.show_player(i, &mut out)?;
            }
        }
        Ok(out)
    }
    pub fn shutdown(&mut self, _r: &Roster) -> Result<Actions, Error> {
        let mut out = Actions::default();
        self.abort(&mut out)?;
        Ok(out)
    }
    pub fn can_close(&self) -> bool {
        !self.finance.busy() && self.players.iter().flatten().all(|p| !p.accepted || p.done)
    }
    pub fn pending_operations(&self) -> Vec<(u64, Operation)> {
        self.finance
            .pending
            .iter()
            .map(|(id, p)| (*id, p.operation.clone()))
            .collect()
    }

    pub fn validate(&self, r: &Roster) -> bool {
        if !(1..=3).contains(&self.kind)
            || !Account::Object(self.object).valid()
            || self.min < 1
            || self.min > self.max
            || self.max > 1000
            || self.max_side > 1000
            || self.kind != 3 && self.max_side != 0
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
            || self.phase > MOVE_PENDING
            || self.next.is_some_and(|n| {
                !matches!(
                    n,
                    CLOSED
                        | WAITING
                        | BETTING
                        | PENDING
                        | PLAYER
                        | DEALER
                        | SIDE
                        | FINALE
                        | HOLD
                        | INTERMISSION
                        | INSURANCE_ROUND
                )
            })
            || self.active < -1
            || self.active > 4
            || self.frame > 360
            || self.seconds > 30
            || self.winning != 255 && self.winning > 36 && self.winning != 100
            || self.settlement_stage > 2
            || self.expected.is_some_and(|n| !(1..=9).contains(&n))
            || self.events.len() > 4
            || self.balance.is_some_and(|b| b > i32::MAX as u32)
        {
            return false;
        }
        if self.kind == 1
            && (self.expected.is_some()
                || matches!(
                    self.phase,
                    ANIMATION
                        | PLAYER
                        | DEALER
                        | SIDE
                        | FINALE
                        | HOLD
                        | INSURANCE_ROUND
                        | MOVE_PENDING
                ))
            || self.kind != 1 && self.phase == SPINNING
            || self.kind != 3 && matches!(self.phase, SIDE | HOLD)
            || self.kind != 2 && self.phase == INSURANCE_ROUND
            || !self.closing
                && self.phase == ANIMATION
                && self.expected.is_none()
                && self.next.is_none()
            || !self.closing && self.phase == SETTLING && !self.finance.owes()
            || self.closing
                && (self.expected.is_some() || self.next.is_some() || !self.events.is_empty())
        {
            return false;
        }
        if let Some(expected) = self.expected {
            let valid = match expected {
                INITIAL_CALLBACK | PLAYER_CALLBACK | DEALER_CALLBACK | SPLIT_CALLBACK
                | CHECK_CALLBACK | COMMUNITY_CALLBACK => self.phase == ANIMATION,
                SIDE_CALLBACK => self.kind == 3 && self.phase == SIDE && self.settlement_stage == 1,
                PAY_CALLBACK | COLLECT_CALLBACK => {
                    self.kind != 1 && self.phase == INTERMISSION && self.settlement_stage == 2
                }
                _ => false,
            };
            if !valid {
                return false;
            }
        }
        if self
            .owner
            .is_some_and(|m| !valid_member(m) || m.input.role != 2 || !m.input.owner_authorized)
        {
            return false;
        }
        if self.owner_connected
            && !self.owner.is_some_and(|m| {
                m.input.role == 2 && m.input.owner_authorized && member_present(m, r)
            })
        {
            return false;
        }
        if self
            .dealer
            .is_some_and(|m| m.input.role != 1 || !member_present(m, r))
        {
            return false;
        }
        for m in r.iter().flatten() {
            if self.dealer == Some(*m) || self.owner_connected && self.owner == Some(*m) {
                continue;
            }
            if !self
                .players
                .iter()
                .flatten()
                .any(|p| p.connected && p.member == *m)
            {
                return false;
            }
        }
        for (i, p) in self.players.iter().enumerate() {
            let Some(p) = p else { continue };
            if !valid_member(p.member)
                || p.member.input.role != 0
                || p.connected && !member_present(p.member, r)
                || self.kind != 1 && p.member.input.registers[3] != (i + 1) as i16
                || self.players[..i]
                    .iter()
                    .flatten()
                    .any(|old| old.member.avatar_id == p.member.avatar_id)
            {
                return false;
            }
            if p.balance > i32::MAX as u32
                || p.bet > 1000
                || p.side > 1000
                || p.call > 2000
                || p.contributed > 20000
                || p.received > 100000
                || p.hands.len() > 4
                || p.active_hand > 3
                || !p.hands.is_empty() && p.active_hand >= p.hands.len()
                || p.hole.len() > 2
                || p.hole.iter().any(|c| c.0 >= 52)
                || p.bets.len() > roulette::MAX_BETS
                || p.total_bets() > 1000
                || p.bets
                    .iter()
                    .enumerate()
                    .any(|(j, b)| p.bets[..j].iter().any(|old| old.same(b)))
            {
                return false;
            }
            if p.accepted
                && (!p.ready
                    || !p.submitted
                    || p.bet < self.min
                    || p.bet > self.max
                    || p.side > self.max_side
                    || p.contributed < p.bet + p.side)
                || p.insured && (!p.accepted || self.kind != 2)
                || p.prompted && (!p.accepted || self.phase != INSURANCE_ROUND)
                || p.refunding && (!p.done || !p.accepted)
                || p.side_done && (!p.accepted || self.kind != 3)
                || p.call != 0 && (self.kind != 3 || !p.accepted || p.call != 2 * p.bet)
                || !p.accepted
                    && (p.received != 0 || p.contributed != 0 || p.bet != 0 || p.side != 0)
                || self.kind != 2 && !p.hands.is_empty()
                || self.kind != 3 && !p.hole.is_empty()
                || self.kind != 1 && !p.bets.is_empty()
            {
                return false;
            }
            let side_due = if p.side_done {
                match self.payout_amount(i, true) {
                    Ok(n) => n,
                    Err(_) => return false,
                }
            } else {
                0
            };
            let total_due = if p.refunding {
                side_due.max(p.contributed)
            } else if p.done {
                match self
                    .payout_amount(i, false)
                    .and_then(|n| n.checked_add(side_due).ok_or(Error::InvalidCheckpoint))
                {
                    Ok(n) => n,
                    Err(_) => return false,
                }
            } else {
                side_due
            };
            let pending_due: u64 = self
                .finance
                .pending
                .values()
                .chain(&self.finance.denied)
                .filter(|pending| pending.seat == i as u8 && pending.kind >= PAYOUT)
                .map(|pending| u64::from(pending.primary))
                .sum();
            if u64::from(p.received) + pending_due != u64::from(total_due) {
                return false;
            }
        }
        if self.dealer_hole.len() > 2
            || self.board.len() > 5
            || self
                .dealer_hole
                .iter()
                .chain(&self.board)
                .any(|c| c.0 >= 52)
            || self.kind != 3 && (!self.dealer_hole.is_empty() || !self.board.is_empty())
            || self.kind != 2 && self.dealer_hand.is_some()
        {
            return false;
        }
        if self.kind == 3 {
            let mut seen = Vec::new();
            for c in self
                .dealer_hole
                .iter()
                .chain(&self.board)
                .chain(self.players.iter().flatten().flat_map(|p| p.hole.iter()))
            {
                if seen.contains(c) {
                    return false;
                }
                seen.push(*c);
            }
            if !self.board.is_empty() && ![3, 5].contains(&self.board.len()) {
                return false;
            }
        }
        let mut occupied = [false; 16];
        for p in self
            .finance
            .pending
            .values()
            .chain(self.finance.denied.iter())
        {
            if p.seat >= 16 || occupied[p.seat as usize] {
                return false;
            }
            occupied[p.seat as usize] = true;
            let member = if p.seat == OWNER_SEAT {
                match self.owner {
                    Some(m) => m,
                    None => return false,
                }
            } else {
                match self.players.get(p.seat as usize).and_then(Option::as_ref) {
                    Some(player) => player.member,
                    None => return false,
                }
            };
            let object = Account::Object(self.object);
            let avatar = Account::Avatar(member.avatar_id);
            let expected = match p.kind {
                PROBE_PLAYER => Operation::QueryBalances {
                    source: object,
                    target: avatar,
                },
                PROBE_OWNER => Operation::QueryBalances {
                    source: Account::System,
                    target: object,
                },
                WITHDRAW | PAYOUT | SIDE_PAYOUT | REFUND => Operation::Transfer {
                    source: object,
                    target: avatar,
                    amount: p.primary,
                },
                _ => Operation::Transfer {
                    source: avatar,
                    target: object,
                    amount: match p.primary.checked_add(p.secondary) {
                        Some(n) => n,
                        None => return false,
                    },
                },
            };
            if p.operation != expected {
                return false;
            }
            if p.seat == OWNER_SEAT && !matches!(p.kind, PROBE_OWNER | DEPOSIT | WITHDRAW)
                || p.seat != OWNER_SEAT && matches!(p.kind, PROBE_OWNER | DEPOSIT | WITHDRAW)
            {
                return false;
            }
            let reachable = if p.seat == OWNER_SEAT {
                p.secondary == 0
                    && match p.kind {
                        PROBE_OWNER => p.primary == 1,
                        DEPOSIT | WITHDRAW => p.primary > 0,
                        _ => false,
                    }
            } else {
                let player = self.players[p.seat as usize]
                    .as_ref()
                    .expect("validated player position");
                match p.kind {
                    PROBE_PLAYER => !player.ready && p.primary == 1 && p.secondary == 0,
                    BET => {
                        !player.accepted
                            && player.submitted
                            && (self.min..=self.max).contains(&p.primary)
                            && p.secondary <= self.max_side
                            && (self.kind == 3 || p.secondary == 0)
                            && (self.kind != 1 || p.primary == player.total_bets())
                            && (self.closing || matches!(self.phase, BETTING | PENDING))
                    }
                    DOUBLE | SPLIT | CALL => {
                        player.accepted
                            && !player.done
                            && p.secondary == 0
                            && p.primary
                                == if p.kind == CALL {
                                    2 * player.bet
                                } else {
                                    player.bet
                                }
                            && (if p.kind == CALL {
                                self.kind == 3
                            } else {
                                self.kind == 2
                            })
                            && (self.closing
                                || self.phase == MOVE_PENDING && self.active == p.seat as i8)
                    }
                    INSURANCE => {
                        self.kind == 2
                            && player.accepted
                            && !player.insured
                            && p.primary == player.bet / 2
                            && p.secondary == 0
                            && (self.closing || self.phase == INSURANCE_ROUND && player.prompted)
                    }
                    PAYOUT => {
                        player.accepted
                            && player.done
                            && !player.refunding
                            && p.secondary == 0
                            && self.payout_amount(p.seat as usize, false) == Ok(p.primary)
                    }
                    SIDE_PAYOUT => {
                        player.accepted
                            && player.side_done
                            && !player.refunding
                            && p.secondary == 0
                            && self.payout_amount(p.seat as usize, true) == Ok(p.primary)
                    }
                    REFUND => {
                        player.accepted
                            && player.done
                            && player.refunding
                            && p.secondary == 0
                            && p.primary == player.contributed.saturating_sub(player.received)
                    }
                    _ => false,
                }
            };
            if !reachable {
                return false;
            }
        }
        if self.phase == MOVE_PENDING
            && !self
                .finance
                .pending
                .values()
                .any(|p| matches!(p.kind, DOUBLE | SPLIT | CALL))
        {
            return false;
        }
        true
    }
    pub fn save(&self, w: &mut Writer) {
        w.u32(self.object);
        w.u32(self.min);
        w.u32(self.max);
        w.u32(self.max_side);
        w.string(&self.name);
        w.bool(self.balance.is_some());
        if let Some(b) = self.balance {
            w.u32(b)
        }
        for m in [self.dealer, self.owner] {
            w.bool(m.is_some());
            if let Some(m) = m {
                save_member(w, &m)
            }
        }
        w.bool(self.owner_connected);
        for p in &self.players {
            w.bool(p.is_some());
            if let Some(p) = p {
                p.save(w)
            }
        }
        w.u8(self.phase);
        w.bool(self.next.is_some());
        if let Some(n) = self.next {
            w.u8(n)
        }
        w.u32(self.frame);
        w.u32(self.seconds);
        w.i16(self.active as i16);
        w.bool(self.expected.is_some());
        if let Some(e) = self.expected {
            w.u8(e)
        }
        w.u32(self.events.len() as u32);
        for (code, args) in &self.events {
            w.i16(*code);
            w.u32(args.len() as u32);
            for a in args {
                w.i16(*a)
            }
        }
        w.u8(self.settlement_stage);
        w.bool(self.broken);
        w.bool(self.enabled);
        w.bool(self.closing);
        self.rng.save(w);
        self.shoe.save(w);
        w.bool(self.dealer_hand.is_some());
        if let Some(h) = &self.dealer_hand {
            h.save(w)
        }
        w.bytes(&self.dealer_hole.iter().map(|c| c.0).collect::<Vec<_>>());
        w.bytes(&self.board.iter().map(|c| c.0).collect::<Vec<_>>());
        w.u8(self.winning);
        self.finance.save(w)
    }
    pub fn restore(plugin: PluginId, r: &mut Reader<'_>) -> Result<Self, Error> {
        let kind = if plugin == ROULETTE {
            1
        } else if plugin == BLACKJACK {
            2
        } else if plugin == HOLDEM {
            3
        } else {
            return Err(Error::WrongPlugin);
        };
        let object = r.u32()?;
        let min = r.u32()?;
        let max = r.u32()?;
        let max_side = r.u32()?;
        let name = r.string(128)?;
        let balance = if r.bool()? { Some(r.u32()?) } else { None };
        let dealer = if r.bool()? {
            Some(restore_member(r)?)
        } else {
            None
        };
        let owner = if r.bool()? {
            Some(restore_member(r)?)
        } else {
            None
        };
        let owner_connected = r.bool()?;
        let mut players = std::array::from_fn(|_| None);
        for p in &mut players {
            if r.bool()? {
                *p = Some(Player::restore(r)?);
            }
        }
        let phase = r.u8()?;
        let next = if r.bool()? { Some(r.u8()?) } else { None };
        let frame = r.u32()?;
        let seconds = r.u32()?;
        let active = r.i16()?;
        if !(-1..=4).contains(&active) {
            return Err(Error::InvalidCheckpoint);
        }
        let expected = if r.bool()? { Some(r.u8()?) } else { None };
        let n = r.count(4)?;
        let mut events = Vec::with_capacity(n);
        for _ in 0..n {
            let code = r.i16()?;
            if code != 2 && code != 3 {
                return Err(Error::InvalidCheckpoint);
            }
            let n = r.count(1)?;
            if n != 1 {
                return Err(Error::InvalidCheckpoint);
            }
            events.push((code, vec![r.i16()?]));
        }
        let settlement_stage = r.u8()?;
        let broken = r.bool()?;
        let enabled = r.bool()?;
        let closing = r.bool()?;
        let rng = Rng::restore(r)?;
        let shoe = Shoe::restore(r, if kind == 2 { 6 } else { 1 })?;
        let dealer_hand = if r.bool()? {
            Some(BjHand::restore(r)?)
        } else {
            None
        };
        let dealer_hole = r.bytes(2)?.into_iter().map(Card).collect();
        let board = r.bytes(5)?.into_iter().map(Card).collect();
        let winning = r.u8()?;
        let finance = Finance::restore(r)?;
        Ok(Self {
            kind,
            object,
            min,
            max,
            max_side,
            name,
            balance,
            dealer,
            owner,
            owner_connected,
            players,
            phase,
            next,
            frame,
            seconds,
            active: active as i8,
            expected,
            events,
            settlement_stage,
            broken,
            enabled,
            closing,
            rng,
            shoe,
            dealer_hand,
            dealer_hole,
            board,
            winning,
            finance,
        })
    }
}
