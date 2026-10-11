// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Two-party source secure-trade offers, 150-tick acceptance delay and an
//! immutable atomic provider transaction. No funds/inventory effects run in UI.
use super::{types::*, wire::*};
use crate::{
    plugins::{
        ProviderOperation,
        codec::{Reader, Writer},
        common::*,
    },
    *,
};
use std::collections::BTreeMap;
#[derive(Clone)]
struct Pending {
    operation: Operation,
    stage: u8,
    player: u8,
    slot: u8,
}
#[derive(Clone)]
pub(crate) struct Trade {
    untradable: Vec<u32>,
    seats: [Option<u8>; 2],
    offers: [Option<TradeOffer>; 2],
    had_two: bool,
    locked: bool,
    kill: bool,
    timer: u16,
    next: u64,
    pending: BTreeMap<u64, Pending>,
}
impl Trade {
    pub(super) fn new(mut untradable: Vec<u32>) -> Result<Self, Error> {
        if untradable.len() > 4096 {
            return Err(Error::InvalidPluginInput);
        }
        untradable.sort_unstable();
        untradable.dedup();
        Ok(Self {
            untradable,
            seats: [None; 2],
            offers: [None, None],
            had_two: false,
            locked: false,
            kill: false,
            timer: 0,
            next: 1,
            pending: BTreeMap::new(),
        })
    }
    pub(super) fn plugin(&self) -> PluginId {
        PluginId(0x897F82F5)
    }
    fn queue(&mut self, a: &mut Actions, p: Pending) -> Result<(), Error> {
        if !self.pending.is_empty() {
            return Err(Error::PersistencePending);
        }
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::CounterExhausted)?;
        a.provider(id, ProviderOperation::Service(p.operation.clone()));
        self.pending.insert(id, p);
        Ok(())
    }
    fn player(&self, seat: u8) -> Result<usize, Error> {
        self.seats
            .iter()
            .position(|s| *s == Some(seat))
            .ok_or(Error::RecipientMismatch)
    }
    fn full(&self) -> bool {
        self.seats.iter().all(Option::is_some)
    }
    fn players(&self, roster: &Roster, a: &mut Actions) {
        let avatars: Vec<_> = self
            .seats
            .iter()
            .map(|s| {
                s.and_then(|s| roster.get(s as usize).and_then(|m| *m))
                    .map_or(0, |m| m.avatar_object)
                    .to_string()
            })
            .collect();
        a.text(Target::All, "trade_players", avatars.join("\n"));
    }
    pub(super) fn join(&mut self, m: Member, roster: &Roster, _: u64) -> Result<Actions, Error> {
        if self.kill || self.locked || self.had_two && !self.full() {
            return Err(Error::PluginNotReady);
        }
        let player = self
            .seats
            .iter()
            .position(Option::is_none)
            .ok_or(Error::ParticipantLimit)?;
        self.seats[player] = Some(m.seat);
        self.offers[player] = Some(TradeOffer::empty(m.avatar_id));
        let mut a = Actions::default();
        a.text(Target::Member(m.seat), "trade_show", String::new());
        self.players(roster, &mut a);
        if self.full() {
            self.had_two = true;
            self.broadcast(&mut a, false)?;
        }
        Ok(a)
    }
    fn broadcast(&mut self, a: &mut Actions, clear: bool) -> Result<(), Error> {
        if !self.full() {
            return Ok(());
        }
        if clear {
            for offer in self.offers.iter_mut().flatten() {
                offer.accepted = false;
            }
        }
        for player in 0..2 {
            let seat = self.seats[player].ok_or(Error::InvalidCheckpoint)?;
            a.binary(
                Target::Member(seat),
                "trade_me",
                offer_wire(
                    self.offers[player]
                        .as_ref()
                        .ok_or(Error::InvalidCheckpoint)?,
                ),
            );
            a.binary(
                Target::Member(seat),
                "trade_other",
                offer_wire(
                    self.offers[player ^ 1]
                        .as_ref()
                        .ok_or(Error::InvalidCheckpoint)?,
                ),
            );
        }
        Ok(())
    }
    fn reset(&mut self, a: &mut Actions) {
        self.timer = 150;
        a.text(Target::All, "trade_time", "5".into());
    }
    pub(super) fn allows(&self, event: &str, binary: bool) -> bool {
        !binary && matches!(event, "close" | "trade_offer")
    }
    pub(super) fn message(
        &mut self,
        m: Member,
        event: &str,
        bytes: &[u8],
        _: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if event == "close" {
            a.close(Target::All);
            return Ok(a);
        }
        if event != "trade_offer" {
            return Err(Error::EventNotAllowed);
        }
        if bytes.is_empty() || !self.full() || self.locked || self.kill {
            return Ok(a);
        }
        if !self.pending.is_empty() {
            return Err(Error::PersistencePending);
        }
        let player = self.player(m.seat)?;
        let body = std::str::from_utf8(bytes).map_err(|_| Error::InvalidMessage)?;
        match bytes[0] {
            b'i' => {
                let ss: Vec<_> = body[1..].split('|').collect();
                if ss.len() < 2 {
                    return Ok(a);
                }
                let (Some(item), Some(slot)) = (
                    parse_u32(ss[0].as_bytes()),
                    parse_i32(ss[1].as_bytes()).filter(|n| (0..5).contains(n)),
                ) else {
                    return Ok(a);
                };
                if item == 0 {
                    self.reset(&mut a);
                    self.offers[player]
                        .as_mut()
                        .ok_or(Error::InvalidCheckpoint)?
                        .items[slot as usize] = None;
                    self.broadcast(&mut a, true)?;
                } else {
                    self.queue(
                        &mut a,
                        Pending {
                            operation: Operation::ReadInventory {
                                avatar: m.avatar_id,
                                item,
                            },
                            stage: 0,
                            player: player as u8,
                            slot: slot as u8,
                        },
                    )?
                }
            }
            b'm' => {
                if let Some(amount) = parse_i32(&bytes[1..]).filter(|a| *a >= 0) {
                    if amount == 0 {
                        self.reset(&mut a);
                        self.offers[player]
                            .as_mut()
                            .ok_or(Error::InvalidCheckpoint)?
                            .money = 0;
                        self.broadcast(&mut a, true)?;
                    } else {
                        let other = self.offers[player ^ 1]
                            .as_ref()
                            .ok_or(Error::InvalidCheckpoint)?
                            .avatar;
                        self.queue(
                            &mut a,
                            Pending {
                                operation: Operation::CheckFunds {
                                    avatar: m.avatar_id,
                                    other,
                                    amount,
                                },
                                stage: 1,
                                player: player as u8,
                                slot: 0,
                            },
                        )?
                    }
                }
            }
            b'p' => {
                if bytes.len() < 3 {
                    return Ok(a);
                }
                let Some(slot) = parse_i32(&bytes[2..]).filter(|v| (0..5).contains(v)) else {
                    return Ok(a);
                };
                if self.offers[player]
                    .as_ref()
                    .ok_or(Error::InvalidCheckpoint)?
                    .items
                    .iter()
                    .enumerate()
                    .any(|(i, o)| o.as_ref().is_some_and(|o| o.lot_id != 0) && i != slot as usize)
                {
                    a.text(Target::Member(m.seat), "trade_error", "4".into());
                    return Ok(a);
                }
                self.queue(
                    &mut a,
                    Pending {
                        operation: Operation::ReadProperty {
                            avatar: m.avatar_id,
                            with_objects: bytes[1] == b'o',
                            untradable: self.untradable.clone(),
                        },
                        stage: 2,
                        player: player as u8,
                        slot: slot as u8,
                    },
                )?
            }
            b'a' => {
                if self.timer > 0 {
                    return Ok(a);
                }
                self.offers[player]
                    .as_mut()
                    .ok_or(Error::InvalidCheckpoint)?
                    .accepted = true;
                if self.offers[player ^ 1]
                    .as_ref()
                    .ok_or(Error::InvalidCheckpoint)?
                    .accepted
                {
                    self.locked = true;
                    a.text(Target::All, "trade_inprogress", String::new());
                    self.queue(
                        &mut a,
                        Pending {
                            operation: Operation::SecureTrade {
                                offers: [
                                    self.offers[0].clone().ok_or(Error::InvalidCheckpoint)?,
                                    self.offers[1].clone().ok_or(Error::InvalidCheckpoint)?,
                                ],
                                untradable: self.untradable.clone(),
                            },
                            stage: 3,
                            player: player as u8,
                            slot: 0,
                        },
                    )?
                } else {
                    self.broadcast(&mut a, false)?
                }
            }
            _ => {}
        }
        Ok(a)
    }
    pub(super) fn reply(
        &mut self,
        callback: u64,
        reply: &Reply,
        roster: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let p = self
            .pending
            .get(&callback)
            .cloned()
            .ok_or(Error::ProviderReceiptMismatch)?;
        let mut a = Actions::default();
        let player = usize::from(p.player);
        let active = self.seats[player].and_then(|s| roster.get(s as usize).and_then(|m| *m));
        let target = Target::Member(self.seats[player].unwrap_or(0));
        match (p.stage, reply) {
            (0, Reply::Inventory(item)) => {
                let Operation::ReadInventory {
                    avatar,
                    item: expected,
                } = p.operation
                else {
                    return Err(Error::InvalidCheckpoint);
                };
                if let Some(item) = item
                    && (!item.valid() || item.persistent_id != expected || item.lot_id != 0)
                {
                    return Err(Error::InvalidPluginData);
                }
                if active.is_some_and(|m| m.avatar_id == avatar) {
                    if let Some(item) = item {
                        self.reset(&mut a);
                        let offer = self.offers[player]
                            .as_mut()
                            .ok_or(Error::InvalidCheckpoint)?;
                        if offer
                            .items
                            .iter()
                            .flatten()
                            .any(|v| v.persistent_id == item.persistent_id)
                        {
                            a.text(target, "trade_error", "4".into());
                            self.broadcast(&mut a, false)?;
                        } else if self.untradable.binary_search(&item.guid).is_ok() {
                            a.text(target, "trade_error", "7".into());
                            self.broadcast(&mut a, false)?;
                        } else {
                            offer.items[usize::from(p.slot)] = Some(item.clone());
                            self.broadcast(&mut a, true)?;
                        }
                    } else {
                        a.text(target, "trade_error", "2".into());
                        self.broadcast(&mut a, false)?;
                    }
                }
            }
            (
                1,
                Reply::Funds {
                    avatar,
                    other,
                    amount,
                    success,
                },
            ) => {
                if p.operation
                    != (Operation::CheckFunds {
                        avatar: *avatar,
                        other: *other,
                        amount: *amount,
                    })
                {
                    return Err(Error::ProviderReceiptMismatch);
                }
                if active.is_some_and(|m| m.avatar_id == *avatar) {
                    if *success {
                        self.reset(&mut a);
                        self.offers[player]
                            .as_mut()
                            .ok_or(Error::InvalidCheckpoint)?
                            .money = *amount;
                        self.broadcast(&mut a, true)?
                    } else {
                        a.text(target, "trade_error", "1".into());
                        self.broadcast(&mut a, false)?
                    }
                }
            }
            (
                2,
                Reply::Property {
                    lot_id,
                    object_count,
                    object_value,
                    lot_name,
                },
            ) => {
                if *object_count < 0 || *object_value < 0 || lot_name.encode_utf16().count() > 128 {
                    return Err(Error::InvalidPluginData);
                }
                let Operation::ReadProperty {
                    avatar,
                    with_objects,
                    ..
                } = p.operation
                else {
                    return Err(Error::InvalidCheckpoint);
                };
                if active.is_some_and(|m| m.avatar_id == avatar) {
                    if *lot_id == 0 {
                        a.text(target, "trade_error", "1".into());
                        self.broadcast(&mut a, false)?
                    } else {
                        self.reset(&mut a);
                        let offer = self.offers[player]
                            .as_mut()
                            .ok_or(Error::InvalidCheckpoint)?;
                        if offer.items.iter().enumerate().any(|(i, o)| {
                            o.as_ref().is_some_and(|o| o.lot_id != 0) && i != usize::from(p.slot)
                        }) {
                            a.text(target, "trade_error", "4".into());
                        } else {
                            offer.items[usize::from(p.slot)] = Some(TradeObject {
                                guid: if with_objects { 2 } else { 1 },
                                persistent_id: 1,
                                data: vec![],
                                lot_id: *lot_id,
                                object_count: if with_objects { *object_count } else { 0 },
                                object_value: if with_objects { *object_value } else { 0 },
                                lot_name: lot_name.clone(),
                            });
                            self.broadcast(&mut a, true)?
                        }
                    }
                }
            }
            (3, Reply::Trade { error }) if *error <= 9 => {
                self.locked = false;
                self.kill = true;
                a.text(
                    Target::All,
                    "trade_message",
                    if *error == 0 { "0|14" } else { "0|16" }.into(),
                );
            }
            (_, Reply::ProviderDenied) => {
                if p.stage == 3 {
                    self.locked = false;
                    self.kill = true;
                    a.text(Target::All, "trade_message", "0|16".into())
                } else if active.is_some() {
                    a.text(target, "trade_error", "9".into());
                    self.broadcast(&mut a, false)?
                }
            }
            _ => return Err(Error::ProviderReceiptMismatch),
        }
        self.pending.remove(&callback);
        Ok(a)
    }
    pub(super) fn tick(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.timer > 0 {
            self.timer -= 1;
            if self.timer.is_multiple_of(30) {
                a.text(Target::All, "trade_time", (self.timer / 30).to_string());
            }
        }
        if self.kill || (self.had_two && !self.full()) {
            a.close(Target::All);
        }
        Ok(a)
    }
    pub(super) fn rebind(&mut self, m: Member, roster: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        a.text(Target::Member(m.seat), "trade_show", String::new());
        self.players(roster, &mut a);
        self.broadcast(&mut a, false)?;
        a.text(
            Target::Member(m.seat),
            "trade_time",
            (self.timer / 30).to_string(),
        );
        if self.locked {
            a.text(Target::Member(m.seat), "trade_inprogress", String::new());
        }
        Ok(a)
    }
    pub(super) fn leave(&mut self, m: Member, roster: &Roster, _: u64) -> Result<Actions, Error> {
        let player = self.player(m.seat)?;
        self.seats[player] = None;
        let mut a = Actions::default();
        self.players(roster, &mut a);
        if self.had_two || self.seats.iter().all(Option::is_none) {
            a.close(Target::Controller);
        }
        Ok(a)
    }
    pub(super) fn shutdown(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        self.kill = true;
        Ok(Actions::default())
    }
    pub(super) fn can_close(&self) -> bool {
        !self.locked
    }
    pub(super) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        self.pending
            .iter()
            .map(|(id, p)| (*id, p.operation.clone()))
            .collect()
    }
    fn valid_pending(&self, p: &Pending) -> bool {
        if p.player >= 2 || p.slot >= 5 {
            return false;
        }
        let Some(me) = self.offers[usize::from(p.player)].as_ref() else {
            return false;
        };
        let Some(other) = self.offers[usize::from(p.player) ^ 1].as_ref() else {
            return false;
        };
        match (&p.operation, p.stage) {
            (Operation::ReadInventory { avatar, item }, 0) => {
                *avatar == me.avatar && *item != 0 && !self.locked
            }
            (
                Operation::CheckFunds {
                    avatar,
                    other: recipient,
                    amount,
                },
                1,
            ) => *avatar == me.avatar && *recipient == other.avatar && *amount > 0 && !self.locked,
            (
                Operation::ReadProperty {
                    avatar, untradable, ..
                },
                2,
            ) => *avatar == me.avatar && *untradable == self.untradable && !self.locked,
            (Operation::SecureTrade { offers, untradable }, 3) => {
                self.locked
                    && self.timer == 0
                    && self.had_two
                    && offers.iter().all(|o| o.accepted)
                    && self.offers[0].as_ref() == Some(&offers[0])
                    && self.offers[1].as_ref() == Some(&offers[1])
                    && *untradable == self.untradable
            }
            _ => false,
        }
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        self.untradable.len() <= 4096
            && self.untradable.windows(2).all(|p| p[0] < p[1])
            && self.next > 0
            && self.timer <= 150
            && self.pending.len() <= 1
            && self
                .pending
                .iter()
                .all(|(id, p)| *id > 0 && *id < self.next && self.valid_pending(p))
            && self.seats.iter().enumerate().all(|(i, s)| {
                s.is_none_or(|s| {
                    roster.get(s as usize).and_then(|m| *m).is_some_and(|m| {
                        self.offers[i]
                            .as_ref()
                            .is_some_and(|o| o.avatar == m.avatar_id)
                    })
                })
            })
            && self.offers.iter().flatten().all(TradeOffer::valid)
            && self.offers[0]
                .as_ref()
                .zip(self.offers[1].as_ref())
                .is_none_or(|(a, b)| a.avatar != b.avatar)
            && (self.had_two || self.offers.iter().flatten().count() <= 1)
            && (!self.locked || self.pending.values().any(|p| p.stage == 3))
            && roster.iter().flatten().count() == self.seats.iter().flatten().count()
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u32(self.untradable.len() as u32);
        for id in &self.untradable {
            w.u32(*id)
        }
        for s in self.seats {
            w.bool(s.is_some());
            if let Some(s) = s {
                w.u8(s)
            }
        }
        for o in &self.offers {
            w.bool(o.is_some());
            if let Some(o) = o {
                o.save(w)
            }
        }
        w.bool(self.had_two);
        w.bool(self.locked);
        w.bool(self.kill);
        w.u16(self.timer);
        w.u64(self.next);
        w.u32(self.pending.len() as u32);
        for (id, p) in &self.pending {
            w.u64(*id);
            p.operation.save(w);
            w.u8(p.stage);
            w.u8(p.player);
            w.u8(p.slot)
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let n = r.count(4096)?;
        let mut untradable = Vec::with_capacity(n);
        for _ in 0..n {
            untradable.push(r.u32()?)
        }
        let mut seats = [None; 2];
        for s in &mut seats {
            if r.bool()? {
                *s = Some(r.u8()?)
            }
        }
        let mut offers = [None, None];
        for o in &mut offers {
            if r.bool()? {
                *o = Some(TradeOffer::restore(r)?)
            }
        }
        let had_two = r.bool()?;
        let locked = r.bool()?;
        let kill = r.bool()?;
        let timer = r.u16()?;
        let next = r.u64()?;
        let n = r.count(1)?;
        let mut pending = BTreeMap::new();
        for _ in 0..n {
            let id = r.u64()?;
            pending.insert(
                id,
                Pending {
                    operation: Operation::restore(r)?,
                    stage: r.u8()?,
                    player: r.u8()?,
                    slot: r.u8()?,
                },
            );
        }
        Ok(Self {
            untradable,
            seats,
            offers,
            had_two,
            locked,
            kill,
            timer,
            next,
            pending,
        })
    }
}
fn offer_wire(offer: &TradeOffer) -> Vec<u8> {
    let mut b = offer.avatar.to_le_bytes().to_vec();
    for item in &offer.items {
        b.push(u8::from(item.is_some()));
        if let Some(o) = item {
            b.extend(o.guid.to_le_bytes());
            b.extend(o.persistent_id.to_le_bytes());
            b.extend((o.data.len() as i32).to_le_bytes());
            b.extend(&o.data);
            b.extend(o.lot_id.to_le_bytes());
            if o.lot_id != 0 {
                b.extend(o.object_count.to_le_bytes());
                b.extend(o.object_value.to_le_bytes());
                write_string(&mut b, &o.lot_name);
            }
        }
    }
    b.extend(offer.money.to_le_bytes());
    b.push(u8::from(offer.accepted));
    b
}

#[cfg(test)]
mod private_invariant_tests {
    use super::*;
    fn pending_trade() -> Trade {
        let mut s = Trade::new(vec![99]).unwrap();
        let mut first = TradeOffer::empty(1);
        first.accepted = true;
        first.money = 50;
        let mut second = TradeOffer::empty(2);
        second.accepted = true;
        let offers = [first, second];
        s.offers = offers.clone().map(Some);
        s.locked = true;
        s.had_two = true;
        s.kill = true;
        s.next = 2;
        s.pending.insert(
            1,
            Pending {
                operation: Operation::SecureTrade {
                    offers,
                    untradable: vec![99],
                },
                stage: 3,
                player: 1,
                slot: 0,
            },
        );
        s
    }
    #[test]
    fn frozen_trade_operation_must_equal_retained_accepted_offers() {
        let mut s = pending_trade();
        assert!(s.validate(&[None; MAX_MEMBERS]));
        if let Operation::SecureTrade { offers, .. } = &mut s.pending.get_mut(&1).unwrap().operation
        {
            offers[0].money = 500;
        }
        assert!(!s.validate(&[None; MAX_MEMBERS]));
    }
    #[test]
    fn secure_trade_stage_requires_its_exact_operation_and_lock() {
        let mut s = pending_trade();
        s.pending.get_mut(&1).unwrap().operation = Operation::DynamicPayouts;
        assert!(!s.validate(&[None; MAX_MEMBERS]));
        let mut s = pending_trade();
        s.locked = false;
        assert!(!s.validate(&[None; MAX_MEMBERS]));
    }
}
