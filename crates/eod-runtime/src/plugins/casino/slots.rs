// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::{cards::Rng, finance::*, *};
const GUIDS: [u32; 5] = [2448255364, 3106786481, 2906162829, 82792878, 82792879];
const DENOMS: [u32; 5] = [1, 5, 10, 25, 100];
const MIN: [u32; 5] = [2500, 12500, 25000, 62500, 250000];
const MAX: [u32; 5] = [15000, 37500, 75000, 187500, 750000];
const LOADING: u8 = 0;
const READY: u8 = 1;
const DEBITING: u8 = 2;
const SPINNING: u8 = 3;
const SETTLING: u8 = 4;
const WAIT_GAME: u8 = 5;
const OFF: u8 = 6;
const MANAGING: u8 = 7;
const REFUNDING: u8 = 8;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::casino::tests::{Harness, member};
    #[test]
    fn checkpoint_rejects_idle_phase_with_an_uncompleted_wager() {
        let mut h = Harness::new(
            Config::Slots {
                object: 99,
                object_guid: GUIDS[0],
                seed: 2,
                machine_type: 0,
                payback_percent: 100,
                enabled: true,
            },
            10000,
        );
        let m = member(0, 1, 0);
        h.join(m);
        h.pay_all();
        h.msg(m, "slots_execute_bet", b"1").unwrap();
        let Game::Slots(s) = &mut h.state.game else {
            panic!()
        };
        s.phase = READY;
        assert!(!h.state.validate(&h.roster));
    }
    #[test]
    fn checkpoint_rejects_payout_not_equal_to_frozen_reels() {
        let mut h = Harness::new(
            Config::Slots {
                object: 99,
                object_guid: GUIDS[0],
                seed: 2,
                machine_type: 0,
                payback_percent: 100,
                enabled: true,
            },
            10000,
        );
        let m = member(0, 1, 0);
        h.join(m);
        h.pay_all();
        h.msg(m, "slots_execute_bet", b"1").unwrap();
        h.pay_all();
        h.msg(m, "slots_wheels_stopped", b"").unwrap();
        let Game::Slots(s) = &mut h.state.game else {
            panic!()
        };
        let pending = s.finance.pending.values_mut().next().unwrap();
        pending.primary += 1;
        let Operation::Transfer { amount, .. } = &mut pending.operation else {
            panic!()
        };
        *amount += 1;
        assert!(!h.state.validate(&h.roster));
    }
}

#[derive(Clone)]
pub(super) struct Slots {
    object: u32,
    machine: u8,
    payback: i16,
    enabled: bool,
    broken: bool,
    closing: bool,
    member: Option<Member>,
    connected: bool,
    phase: u8,
    balance: Option<u32>,
    bet: u32,
    winnings: u32,
    stops: [u8; 3],
    game_over: bool,
    rng: Rng,
    finance: Finance,
}
impl Slots {
    pub fn new(config: Config) -> Result<Self, Error> {
        let Config::Slots {
            object,
            object_guid,
            seed,
            machine_type,
            payback_percent,
            enabled,
        } = config
        else {
            return Err(Error::InvalidPluginInput);
        };
        let machine = GUIDS
            .iter()
            .position(|g| *g == object_guid)
            .map_or(machine_type, |i| i as u8);
        if !Account::Object(object).valid() || machine > 4 || !(80..=110).contains(&payback_percent)
        {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            object,
            machine,
            payback: payback_percent,
            enabled,
            broken: false,
            closing: false,
            member: None,
            connected: false,
            phase: LOADING,
            balance: None,
            bet: 0,
            winnings: 0,
            stops: [0; 3],
            game_over: false,
            rng: Rng::new(seed),
            finance: Finance::new(),
        })
    }
    pub fn allows(&self, event: &str, binary: bool) -> bool {
        if binary {
            event == "slots_new_odds"
        } else {
            matches!(
                event,
                "slots_toggle_onOff"
                    | "slots_execute_bet"
                    | "slots_wheels_stopped"
                    | "slots_close_UI"
                    | "slots_withdraw"
                    | "slots_deposit"
            )
        }
    }
    pub fn trusted_object(&self) -> u32 {
        self.object
    }
    fn target(&self) -> Result<Target, Error> {
        Ok(Target::Member(
            self.member.ok_or(Error::InvalidIdentity)?.seat,
        ))
    }
    fn player(&self, m: Member) -> Result<(), Error> {
        if self.member != Some(m) || !self.connected {
            return Err(Error::StaleSession);
        }
        Ok(())
    }
    fn ready(&self) -> bool {
        self.enabled
            && !self.broken
            && self
                .balance
                .is_some_and(|b| b >= MIN[self.machine as usize] && b < MAX[self.machine as usize])
            && !self.closing
    }
    fn request(&mut self, kind: u8, amount: u32, out: &mut Actions) -> Result<(), Error> {
        let m = self.member.ok_or(Error::InvalidIdentity)?;
        let object = Account::Object(self.object);
        let avatar = Account::Avatar(m.avatar_id);
        let operation = if kind == PROBE_PLAYER || kind == PROBE_OWNER {
            Operation::QueryBalances {
                source: Account::System,
                target: object,
            }
        } else if [BET, DEPOSIT].contains(&kind) {
            Operation::Transfer {
                source: avatar,
                target: object,
                amount,
            }
        } else {
            Operation::Transfer {
                source: object,
                target: avatar,
                amount,
            }
        };
        self.finance.request(
            Pending {
                kind,
                seat: m.seat,
                primary: amount,
                secondary: 0,
                operation,
            },
            out,
        )
    }
    pub fn join(&mut self, m: Member, _r: &Roster) -> Result<Actions, Error> {
        if !valid_member(m) || ![1, 2].contains(&m.input.role) {
            return Err(Error::InvalidPluginInput);
        }
        if m.input.role == 2 && !m.input.owner_authorized {
            return Err(Error::NotAuthorized);
        }
        if self.member.is_some() && (self.connected || self.finance.busy() || self.winnings != 0) {
            return Err(Error::ParticipantLimit);
        }
        self.member = Some(m);
        self.connected = true;
        self.phase = LOADING;
        self.closing = false;
        self.game_over = false;
        self.bet = 0;
        self.winnings = 0;
        let mut out = Actions::default();
        if m.input.role == 1 {
            out.binary(
                Target::Member(m.seat),
                "slots_player_init",
                vec![self.machine],
            );
        }
        self.request(
            if m.input.role == 1 {
                PROBE_PLAYER
            } else {
                PROBE_OWNER
            },
            1,
            &mut out,
        )?;
        Ok(out)
    }
    pub fn message(
        &mut self,
        m: Member,
        event: &str,
        bytes: &[u8],
        r: &Roster,
    ) -> Result<Actions, Error> {
        self.player(m)?;
        let mut out = Actions::default();
        let target = Target::Member(m.seat);
        match event {
            "slots_close_UI" => {
                out.close(target);
            }
            "slots_execute_bet" => {
                if m.input.role != 1 {
                    return Err(Error::NotAuthorized);
                }
                if self.phase != READY || !self.ready() || self.finance.busy() {
                    return Err(Error::PluginNotReady);
                }
                let denom = DENOMS[self.machine as usize];
                let amount = text_number(bytes).ok().and_then(|n| u32::try_from(n).ok());
                if !amount.is_some_and(|a| a >= denom && a <= denom * 5 && a % denom == 0) {
                    out.object(target, 5, vec![]);
                    return Ok(out);
                }
                self.bet = amount.ok_or(Error::InvalidPluginInput)?;
                let wheel = wheel(self.payback)?;
                for stop in &mut self.stops {
                    *stop = wheel[self.rng.below(wheel.len())?];
                }
                self.winnings = payout(self.bet, self.stops);
                self.phase = DEBITING;
                self.request(BET, self.bet, &mut out)?;
            }
            "slots_wheels_stopped" => {
                if m.input.role != 1 {
                    return Err(Error::NotAuthorized);
                }
                if self.phase != SPINNING {
                    return Err(Error::PluginNotReady);
                }
                self.finish_spin(&mut out)?;
            }
            "slots_new_odds" => {
                if m.input.role != 2 || !m.input.owner_authorized {
                    return Err(Error::NotAuthorized);
                }
                if self.phase != MANAGING || self.finance.busy() {
                    return Err(Error::PluginNotReady);
                }
                if bytes.len() != 1 {
                    return Err(Error::InvalidPluginInput);
                }
                self.payback = bytes[0].clamp(80, 110) as i16;
                out.object(target, 11, vec![self.payback]);
            }
            "slots_toggle_onOff" => {
                if m.input.role != 2 || !m.input.owner_authorized {
                    return Err(Error::NotAuthorized);
                }
                if self.phase != MANAGING || self.finance.busy() {
                    return Err(Error::PluginNotReady);
                }
                out.object(target, 12, vec![]);
            }
            "slots_deposit" | "slots_withdraw" => {
                if m.input.role != 2 || !m.input.owner_authorized {
                    return Err(Error::NotAuthorized);
                }
                if self.phase != MANAGING || self.finance.busy() {
                    return Err(Error::PluginNotReady);
                }
                let balance = self.balance.ok_or(Error::PluginNotReady)?;
                let amount = text_number(bytes);
                let deposit = event == "slots_deposit";
                let failure = match amount {
                    Ok(0) => Some("Null"),
                    Ok(n) if n < 0 => Some("Invalid"),
                    Err(_) => Some("Invalid"),
                    Ok(n)
                        if deposit
                            && balance
                                .checked_add(n as u32)
                                .is_none_or(|sum| sum > MAX[self.machine as usize]) =>
                    {
                        Some("Overflow")
                    }
                    Ok(n) if !deposit && n as u32 > balance => Some("Overflow"),
                    _ => None,
                };
                if let Some(f) = failure {
                    out.text(
                        target,
                        if deposit {
                            "slots_deposit_fail"
                        } else {
                            "slots_withdraw_fail"
                        },
                        f.to_owned(),
                    );
                } else {
                    self.request(
                        if deposit { DEPOSIT } else { WITHDRAW },
                        amount? as u32,
                        &mut out,
                    )?;
                }
            }
            _ => return Err(Error::EventNotAllowed),
        }
        let _ = r;
        Ok(out)
    }
    fn finish_spin(&mut self, out: &mut Actions) -> Result<(), Error> {
        if self.winnings > 0 {
            self.phase = SETTLING;
            self.request(PAYOUT, self.winnings, out)?;
        } else {
            self.phase = WAIT_GAME;
            if self.connected {
                let id = 30 + self.rng.below(5)?;
                out.text(self.target()?, "slots_display_loss", id.to_string());
            }
            if self.game_over {
                self.next_game(out)?;
            }
        }
        Ok(())
    }
    fn next_game(&mut self, out: &mut Actions) -> Result<(), Error> {
        self.bet = 0;
        self.winnings = 0;
        self.game_over = false;
        self.phase = if self.ready() { READY } else { OFF };
        if self.connected {
            out.text(
                self.target()?,
                if self.phase == READY {
                    "slots_new_game"
                } else {
                    "slots_close_machine"
                },
                String::new(),
            );
        }
        Ok(())
    }
    pub fn reply(&mut self, id: u64, reply: &Reply, _r: &Roster) -> Result<Actions, Error> {
        let (p, receipt) = self.finance.complete(id, reply)?;
        let mut out = Actions::default();
        if receipt.success {
            self.balance = if matches!(
                p.operation,
                Operation::Transfer {
                    source: Account::Object(_),
                    ..
                }
            ) {
                receipt.source_balance
            } else {
                receipt.target_balance
            };
        }
        let target = self.target()?;
        match p.kind {
            PROBE_PLAYER | PROBE_OWNER => {
                if !receipt.success {
                    self.phase = OFF;
                    if self.connected {
                        out.text(target, "slots_close_machine", String::new());
                    }
                } else if p.kind == PROBE_OWNER {
                    self.phase = MANAGING;
                    if self.connected {
                        out.text(
                            target,
                            "slots_owner_init",
                            format!(
                                "{}%{}%{}%{}",
                                self.payback,
                                self.balance.unwrap_or(0),
                                self.machine,
                                u8::from(self.enabled)
                            ),
                        );
                        if !self.ready() {
                            out.text(target, "slots_close_machine", String::new());
                        }
                    }
                } else {
                    self.next_game(&mut out)?;
                }
            }
            BET => {
                if receipt.success {
                    if self.connected && !self.closing {
                        self.phase = SPINNING;
                        if self.winnings > 0 {
                            out.object(target, 9, vec![self.winnings as i16]);
                            out.object(target, 4, vec![self.bet as i16]);
                        } else {
                            out.object(target, 8, vec![self.bet as i16]);
                        }
                        out.binary(target, "slots_spin", self.stops.to_vec());
                    } else {
                        // No paid spin was revealed before departure. Freeze
                        // the committed stake as a refund, never as reel winnings.
                        self.winnings = 0;
                        self.phase = REFUNDING;
                        self.request(REFUND, self.bet, &mut out)?;
                    }
                } else {
                    self.bet = 0;
                    self.winnings = 0;
                    self.phase = WAIT_GAME;
                    if self.connected {
                        out.object(target, 5, vec![]);
                    } else {
                        self.next_game(&mut out)?;
                    }
                }
            }
            PAYOUT => {
                if receipt.success {
                    self.winnings = 0;
                    self.phase = WAIT_GAME;
                    if self.connected {
                        let id = 25 + self.rng.below(5)?;
                        out.text(target, "slots_display_win", format!("{id}%{}", p.primary));
                    }
                    if self.game_over {
                        self.next_game(&mut out)?;
                    }
                } else {
                    self.finance.retain_denied(p)?;
                    self.phase = SETTLING;
                }
            }
            REFUND => {
                if receipt.success {
                    self.next_game(&mut out)?;
                } else {
                    self.finance.retain_denied(p)?;
                    self.phase = REFUNDING;
                }
            }
            DEPOSIT | WITHDRAW => {
                if self.connected {
                    if receipt.success {
                        out.text(
                            target,
                            "slots_resume_manage",
                            self.balance.unwrap_or(0).to_string(),
                        );
                    } else if p.kind == DEPOSIT {
                        out.text(target, "slots_deposit_NSF", p.primary.to_string());
                    } else {
                        out.text(target, "slots_withdraw_fail", "Unknown".to_owned());
                    }
                }
            }
            _ => return Err(Error::ProviderReceiptMismatch),
        }
        Ok(out)
    }
    pub fn vm_event(&mut self, input: &VmInput, _r: &Roster) -> Result<Actions, Error> {
        let mut out = Actions::default();
        match *input {
            VmInput::RetrySettlements => self.finance.retry(&mut out)?,
            VmInput::SetBroken(b) => {
                self.broken = b;
                if b && self.phase == READY {
                    self.next_game(&mut out)?;
                }
            }
            VmInput::SetEnabled(b) => {
                self.enabled = b;
                if matches!(self.phase, READY | OFF) {
                    self.next_game(&mut out)?;
                }
            }
            VmInput::SourceEvent { code: 5 | 6 } => {
                self.game_over = true;
                match self.phase {
                    SPINNING => self.finish_spin(&mut out)?,
                    WAIT_GAME => self.next_game(&mut out)?,
                    SETTLING => {}
                    _ => return Err(Error::PluginNotReady),
                }
            }
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(out)
    }
    pub fn tick(&mut self, _r: &Roster) -> Result<Actions, Error> {
        Ok(Actions::default())
    }
    pub fn leave(&mut self, m: Member, _r: &Roster) -> Result<Actions, Error> {
        if self.member == Some(m) && !self.connected {
            return Ok(Actions::default());
        }
        self.player(m)?;
        self.connected = false;
        self.game_over = true;
        let mut out = Actions::default();
        out.text(Target::Member(m.seat), "slots_cleanup", String::new());
        if self.phase == SPINNING {
            self.finish_spin(&mut out)?;
        }
        Ok(out)
    }
    pub fn rebind(&mut self, m: Member, _r: &Roster) -> Result<Actions, Error> {
        self.player(m)?;
        let mut out = Actions::default();
        if m.input.role == 1 {
            out.binary(
                Target::Member(m.seat),
                "slots_player_init",
                vec![self.machine],
            );
            match self.phase {
                READY => out.text(Target::Member(m.seat), "slots_new_game", String::new()),
                SPINNING => out.binary(Target::Member(m.seat), "slots_spin", self.stops.to_vec()),
                OFF => out.text(Target::Member(m.seat), "slots_close_machine", String::new()),
                _ => {}
            }
        } else if self.phase == MANAGING {
            out.text(
                Target::Member(m.seat),
                "slots_owner_init",
                format!(
                    "{}%{}%{}%{}",
                    self.payback,
                    self.balance.unwrap_or(0),
                    self.machine,
                    u8::from(self.enabled)
                ),
            );
        }
        Ok(out)
    }
    pub fn shutdown(&mut self, _r: &Roster) -> Result<Actions, Error> {
        self.closing = true;
        let mut out = Actions::default();
        if self.connected {
            out.close(self.target()?);
        }
        self.connected = false;
        self.game_over = true;
        if self.phase == SPINNING {
            self.finish_spin(&mut out)?;
        }
        Ok(out)
    }
    pub fn can_close(&self) -> bool {
        !self.finance.busy() && self.winnings == 0
    }
    pub fn pending_operations(&self) -> Vec<(u64, Operation)> {
        self.finance
            .pending
            .iter()
            .map(|(id, p)| (*id, p.operation.clone()))
            .collect()
    }
    pub fn validate(&self, r: &Roster) -> bool {
        if !Account::Object(self.object).valid()
            || self.machine > 4
            || !(80..=110).contains(&self.payback)
            || self.phase > REFUNDING
            || self.balance.is_some_and(|b| b > i32::MAX as u32)
            || self.stops.iter().any(|s| *s > 11)
            || self.finance.pending.len() + self.finance.denied.len() > 1
        {
            return false;
        }
        if self.connected && !self.member.is_some_and(|m| member_present(m, r)) {
            return false;
        }
        if r.iter()
            .flatten()
            .any(|m| Some(*m) != self.member || !self.connected)
        {
            return false;
        }
        if let Some(m) = self.member {
            if !valid_member(m)
                || ![1, 2].contains(&m.input.role)
                || (m.input.role == 2 && !m.input.owner_authorized)
            {
                return false;
            }
            for p in self
                .finance
                .pending
                .values()
                .chain(self.finance.denied.iter())
            {
                if p.seat != m.seat || p.secondary != 0 {
                    return false;
                }
                let obj = Account::Object(self.object);
                let avatar = Account::Avatar(m.avatar_id);
                let expected = match p.kind {
                    PROBE_PLAYER | PROBE_OWNER => Operation::QueryBalances {
                        source: Account::System,
                        target: obj,
                    },
                    BET | DEPOSIT => Operation::Transfer {
                        source: avatar,
                        target: obj,
                        amount: p.primary,
                    },
                    PAYOUT | REFUND | WITHDRAW => Operation::Transfer {
                        source: obj,
                        target: avatar,
                        amount: p.primary,
                    },
                    _ => return false,
                };
                if p.operation != expected {
                    return false;
                }
                let reachable = match p.kind {
                    PROBE_PLAYER | PROBE_OWNER => {
                        self.phase == LOADING
                            && p.primary == 1
                            && (p.kind == PROBE_OWNER) == (m.input.role == 2)
                    }
                    BET => m.input.role == 1 && self.phase == DEBITING && p.primary == self.bet,
                    PAYOUT => {
                        m.input.role == 1 && self.phase == SETTLING && p.primary == self.winnings
                    }
                    REFUND => m.input.role == 1 && self.phase == REFUNDING && p.primary == self.bet,
                    DEPOSIT | WITHDRAW => m.input.role == 2 && self.phase == MANAGING,
                    _ => false,
                };
                if !reachable {
                    return false;
                }
            }
        } else if self.connected || self.finance.busy() || self.bet != 0 || self.winnings != 0 {
            return false;
        }
        let denom = DENOMS[self.machine as usize];
        if self.bet != 0
            && (self.bet < denom || self.bet > denom * 5 || !self.bet.is_multiple_of(denom))
        {
            return false;
        }
        if matches!(self.phase, DEBITING | SPINNING | SETTLING)
            && (self.bet == 0 || self.winnings != payout(self.bet, self.stops))
        {
            return false;
        }
        if self.phase == SETTLING && (!self.finance.owes() || self.winnings == 0) {
            return false;
        }
        if self.phase == REFUNDING && (self.connected || !self.finance.owes() || self.bet == 0) {
            return false;
        }
        if self.phase == DEBITING && !self.finance.pending.values().any(|p| p.kind == BET) {
            return false;
        }
        if !matches!(self.phase, DEBITING | SPINNING | SETTLING) && self.winnings != 0
            || matches!(self.phase, READY | SPINNING | WAIT_GAME | OFF) && self.finance.busy()
            || self.connected && self.phase == READY && !self.ready()
            || self.phase == MANAGING && self.member.is_none_or(|m| m.input.role != 2)
        {
            return false;
        }
        true
    }
    pub fn save(&self, w: &mut Writer) {
        w.u32(self.object);
        w.u8(self.machine);
        w.i16(self.payback);
        w.bool(self.enabled);
        w.bool(self.broken);
        w.bool(self.closing);
        w.bool(self.member.is_some());
        if let Some(m) = &self.member {
            save_member(w, m)
        }
        w.bool(self.connected);
        w.u8(self.phase);
        w.bool(self.balance.is_some());
        if let Some(b) = self.balance {
            w.u32(b)
        }
        w.u32(self.bet);
        w.u32(self.winnings);
        w.fixed(&self.stops);
        w.bool(self.game_over);
        self.rng.save(w);
        self.finance.save(w)
    }
    pub fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            object: r.u32()?,
            machine: r.u8()?,
            payback: r.i16()?,
            enabled: r.bool()?,
            broken: r.bool()?,
            closing: r.bool()?,
            member: if r.bool()? {
                Some(restore_member(r)?)
            } else {
                None
            },
            connected: r.bool()?,
            phase: r.u8()?,
            balance: if r.bool()? { Some(r.u32()?) } else { None },
            bet: r.u32()?,
            winnings: r.u32()?,
            stops: r
                .take(3)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
            game_over: r.bool()?,
            rng: Rng::restore(r)?,
            finance: Finance::restore(r)?,
        })
    }
}

pub(super) fn payout(bet: u32, stops: [u8; 3]) -> u32 {
    let [a, b, c] = stops;
    bet * match (a, b, c) {
        (11, 11, 11) => 500,
        (9, 9, 9) => 150,
        (7, 7, 7) => 75,
        (5, 5, 5) => 50,
        (3, 3, 3) => 25,
        (5, 3, 1) => 10,
        (1, 1, _) => 5,
        (1, _, _) => 2,
        _ => 0,
    }
}
pub(super) fn wheel(payback: i16) -> Result<Vec<u8>, Error> {
    if !(80..=110).contains(&payback) {
        return Err(Error::InvalidPluginInput);
    }
    let delta = 1f32 - payback as f32 / 100f32;
    let mut first = if payback < 100 {
        (42f32 - 42f32 * delta / 3f32).round_ties_even() as usize
    } else {
        (42f32 + 42f32 * delta / 3f32).round_ties_even() as usize
    };
    if payback < 100 && (payback < 83 || (payback > 85 && payback < 97) || payback == 98) {
        first += 1;
    }
    let blank = (75f32 + 75f32 * delta).round_ties_even() as usize + usize::from(payback == 110);
    let mut counts = [first, 49, 36, 24, 12, 6];
    for n in &mut counts[1..] {
        *n = (*n as i32 - (*n as f32 * delta).round_ties_even() as i32) as usize;
    }
    let mut out = Vec::new();
    for (i, n) in counts.into_iter().enumerate() {
        out.extend(std::iter::repeat_n(
            i as u8 * 2,
            blank / 6 + if i == 5 { blank % 6 } else { 0 },
        ));
        out.extend(std::iter::repeat_n(i as u8 * 2 + 1, n));
    }
    Ok(out)
}
