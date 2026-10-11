// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Server-owned casino handlers translated from the pinned original handlers.
//! The native host supplies identity, controller authority, ticks and durable
//! provider dispatch. No UI message can supply a balance, deck or receipt.

mod cards;
mod finance;
mod roulette;
mod slots;
mod table;

use crate::plugins::{codec::*, common::*};
use crate::{Error, PluginId};
use std::fmt;

pub const SLOTS: PluginId = PluginId(0xCB2819CB);
pub const ROULETTE: PluginId = PluginId(0x0B2A6B83);
pub const BLACKJACK: PluginId = PluginId(0x2B2FC514);
pub const HOLDEM: PluginId = PluginId(0x1001);

/// Config is populated by the authoritative object adapter. `seed` and dealer
/// state remain exclusively in the private native checkpoint.
#[derive(Clone)]
pub enum Config {
    Slots {
        object: u32,
        object_guid: u32,
        seed: u64,
        machine_type: u8,
        payback_percent: i16,
        enabled: bool,
    },
    Roulette {
        object: u32,
        min_bet: i32,
        max_bet: i32,
        seed: u64,
    },
    Blackjack {
        object: u32,
        min_bet: i32,
        max_bet: i32,
        seed: u64,
        dealer_name: String,
    },
    HoldEm {
        object: u32,
        min_ante: i32,
        max_ante: i32,
        max_side: i32,
        seed: u64,
        dealer_name: String,
    },
}
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CasinoConfig([REDACTED])")
    }
}

/// No wire decoding exists for this enum; only a current native controller
/// capability can deliver animation completions or operate settlement recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmInput {
    SourceEvent { code: i16 },
    SetBroken(bool),
    SetEnabled(bool),
    RetrySettlements,
}

/// Account namespaces are explicit even when their numerical IDs coincide.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Account {
    System,
    Object(u32),
    Avatar(u32),
}
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CasinoAccount([REDACTED])")
    }
}
impl Account {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::System => true,
            Self::Object(n) | Self::Avatar(n) => n != 0 && n != u32::MAX,
        }
    }
    fn save(self, w: &mut Writer) {
        match self {
            Self::System => w.u8(0),
            Self::Object(n) => {
                w.u8(1);
                w.u32(n)
            }
            Self::Avatar(n) => {
                w.u8(2);
                w.u32(n)
            }
        }
    }
    fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let a = match r.u8()? {
            0 => Self::System,
            1 => Self::Object(r.u32()?),
            2 => Self::Avatar(r.u32()?),
            _ => return Err(Error::InvalidCheckpoint),
        };
        if !a.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(a)
    }
}

/// QueryBalances corresponds to the original `testOnly` $1 transfer. A real
/// provider must inspect both authorized accounts without moving any money.
#[derive(Clone, PartialEq, Eq)]
pub enum Operation {
    QueryBalances {
        source: Account,
        target: Account,
    },
    Transfer {
        source: Account,
        target: Account,
        amount: u32,
    },
}
impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CasinoOperation([REDACTED])")
    }
}
impl Operation {
    fn details(&self) -> (Account, Account, u32) {
        match *self {
            Self::QueryBalances { source, target } => (source, target, 1),
            Self::Transfer {
                source,
                target,
                amount,
            } => (source, target, amount),
        }
    }
    pub(crate) fn valid(&self) -> bool {
        let (s, t, a) = self.details();
        s.valid()
            && t.valid()
            && s != t
            && a > 0
            && a <= i32::MAX as u32
            && (!matches!(self, Self::Transfer { .. })
                || (!matches!(s, Account::System) && !matches!(t, Account::System)))
    }
    pub(crate) fn save(&self, w: &mut Writer) {
        w.u8(u8::from(matches!(self, Self::Transfer { .. })));
        let (s, t, a) = self.details();
        s.save(w);
        t.save(w);
        w.u32(a);
    }
    pub(crate) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let tag = r.u8()?;
        let source = Account::restore(r)?;
        let target = Account::restore(r)?;
        let amount = r.u32()?;
        let op = match tag {
            0 if amount == 1 => Self::QueryBalances { source, target },
            1 => Self::Transfer {
                source,
                target,
                amount,
            },
            _ => return Err(Error::InvalidCheckpoint),
        };
        if !op.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(op)
    }
}

/// The receipt repeats the immutable account and amount fields, allowing this
/// family to reject mismatched provider data before admitting any game output.
#[derive(Clone, PartialEq, Eq)]
pub enum Reply {
    Transaction {
        success: bool,
        source: Account,
        target: Account,
        amount: u32,
        source_balance: u32,
        target_balance: u32,
    },
    ProviderDenied,
}
impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CasinoReply([REDACTED])")
    }
}
impl Reply {
    pub(crate) fn save(&self, w: &mut Writer) {
        match self {
            Self::ProviderDenied => w.u8(0),
            Self::Transaction {
                success,
                source,
                target,
                amount,
                source_balance,
                target_balance,
            } => {
                w.u8(1);
                w.bool(*success);
                source.save(w);
                target.save(w);
                w.u32(*amount);
                w.u32(*source_balance);
                w.u32(*target_balance);
            }
        }
    }
    pub(crate) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        match r.u8()? {
            0 => Ok(Self::ProviderDenied),
            1 => Ok(Self::Transaction {
                success: r.bool()?,
                source: Account::restore(r)?,
                target: Account::restore(r)?,
                amount: r.u32()?,
                source_balance: r.u32()?,
                target_balance: r.u32()?,
            }),
            _ => Err(Error::InvalidCheckpoint),
        }
    }
}

#[derive(Clone)]
enum Game {
    Slots(slots::Slots),
    Table(Box<table::Table>),
}
#[derive(Clone)]
pub(crate) struct State {
    game: Game,
}
impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CasinoState([REDACTED])")
    }
}
macro_rules! dispatch {
    ($self:ident,$method:ident $(,$arg:expr)*) => {match &mut $self.game {Game::Slots(s)=>s.$method($($arg),*),Game::Table(t)=>t.$method($($arg),*)}};
}
impl State {
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            game: match config {
                Config::Slots { .. } => Game::Slots(slots::Slots::new(config)?),
                _ => Game::Table(Box::new(table::Table::new(config)?)),
            },
        })
    }
    pub(crate) fn plugin(&self) -> PluginId {
        match &self.game {
            Game::Slots(_) => SLOTS,
            Game::Table(t) => t.plugin(),
        }
    }
    pub(crate) fn trusted_object(&self) -> u32 {
        match &self.game {
            Game::Slots(s) => s.trusted_object(),
            Game::Table(t) => t.trusted_object(),
        }
    }
    pub(crate) fn start(&mut self, _roster: &Roster, _tick: u64) -> Result<Actions, Error> {
        Ok(Actions::default())
    }
    pub(crate) fn join(&mut self, m: Member, r: &Roster, _tick: u64) -> Result<Actions, Error> {
        dispatch!(self, join, m, r)
    }
    pub(crate) fn allows(&self, event: &str, binary: bool) -> bool {
        match &self.game {
            Game::Slots(s) => s.allows(event, binary),
            Game::Table(t) => t.allows(event, binary),
        }
    }
    pub(crate) fn message(
        &mut self,
        m: Member,
        event: &str,
        payload: &[u8],
        r: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        dispatch!(self, message, m, event, payload, r)
    }
    pub(crate) fn tick(&mut self, r: &Roster, _tick: u64) -> Result<Actions, Error> {
        dispatch!(self, tick, r)
    }
    pub(crate) fn vm_event(
        &mut self,
        input: &VmInput,
        r: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        dispatch!(self, vm_event, input, r)
    }
    pub(crate) fn reply(
        &mut self,
        callback: u64,
        reply: &Reply,
        r: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        dispatch!(self, reply, callback, reply, r)
    }
    pub(crate) fn peer(
        &mut self,
        _source: &PeerSource,
        _signal: &Signal,
        _roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        Err(Error::EventNotAllowed)
    }
    pub(crate) fn leave(&mut self, m: Member, r: &Roster, _tick: u64) -> Result<Actions, Error> {
        dispatch!(self, leave, m, r)
    }
    pub(crate) fn rebind(&mut self, m: Member, r: &Roster, _tick: u64) -> Result<Actions, Error> {
        dispatch!(self, rebind, m, r)
    }
    pub(crate) fn shutdown(&mut self, r: &Roster, _tick: u64) -> Result<Actions, Error> {
        dispatch!(self, shutdown, r)
    }
    pub(crate) fn can_close(&self) -> bool {
        match &self.game {
            Game::Slots(s) => s.can_close(),
            Game::Table(t) => t.can_close(),
        }
    }
    pub(crate) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        match &self.game {
            Game::Slots(s) => s.pending_operations(),
            Game::Table(t) => t.pending_operations(),
        }
    }
    pub(crate) fn validate(&self, r: &Roster) -> bool {
        match &self.game {
            Game::Slots(s) => s.validate(r),
            Game::Table(t) => t.validate(r),
        }
    }
    pub(crate) fn save(&self, w: &mut Writer) {
        w.u8(1);
        match &self.game {
            Game::Slots(s) => s.save(w),
            Game::Table(t) => t.save(w),
        }
    }
    pub(crate) fn restore(plugin: PluginId, r: &mut Reader<'_>) -> Result<Self, Error> {
        if r.u8()? != 1 {
            return Err(Error::UnsupportedPluginSchema);
        }
        Ok(Self {
            game: if plugin == SLOTS {
                Game::Slots(slots::Slots::restore(r)?)
            } else if [ROULETTE, BLACKJACK, HOLDEM].contains(&plugin) {
                Game::Table(Box::new(table::Table::restore(plugin, r)?))
            } else {
                return Err(Error::WrongPlugin);
            },
        })
    }
}

fn text_number(bytes: &[u8]) -> Result<i32, Error> {
    if bytes.len() > 32 {
        return Err(Error::InvalidPluginInput);
    }
    std::str::from_utf8(bytes)
        .map_err(|_| Error::InvalidPluginInput)?
        .trim()
        .parse()
        .map_err(|_| Error::InvalidPluginInput)
}
fn binary_number(bytes: &[u8]) -> Result<i32, Error> {
    Ok(i32::from_le_bytes(
        bytes.try_into().map_err(|_| Error::InvalidPluginInput)?,
    ))
}
fn strings(values: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    for s in values {
        let mut n = s.len();
        while n >= 128 {
            b.push((n as u8 & 127) | 128);
            n >>= 7;
        }
        b.push(n as u8);
        b.extend_from_slice(s.as_bytes());
    }
    b
}
fn parse_strings(bytes: &[u8], count: usize) -> Result<Vec<String>, Error> {
    if bytes.len() > 4096 {
        return Err(Error::InvalidPluginInput);
    }
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        if out.len() >= count {
            return Err(Error::InvalidPluginInput);
        }
        let mut n = 0usize;
        let mut shift = 0;
        loop {
            let b = *bytes.get(at).ok_or(Error::InvalidPluginInput)?;
            at += 1;
            if shift > 28 || (shift == 28 && b > 7) {
                return Err(Error::InvalidPluginInput);
            }
            n |= ((b & 127) as usize) << shift;
            if b < 128 {
                break;
            }
            shift += 7;
        }
        if n > 1024 {
            return Err(Error::InvalidPluginInput);
        }
        let end = at.checked_add(n).ok_or(Error::InvalidPluginInput)?;
        let s = std::str::from_utf8(bytes.get(at..end).ok_or(Error::InvalidPluginInput)?)
            .map_err(|_| Error::InvalidPluginInput)?;
        out.push(s.to_owned());
        at = end;
    }
    if out.len() != count {
        return Err(Error::InvalidPluginInput);
    }
    Ok(out)
}
fn save_member(w: &mut Writer, m: &Member) {
    w.u8(m.seat);
    w.u64(m.actor.0);
    w.i16(m.avatar_object);
    w.u32(m.avatar_id);
    w.u8(m.input.role);
    for v in m.input.registers {
        w.i16(v)
    }
    for v in m.input.skills {
        w.i16(v)
    }
    w.bool(m.input.owner_authorized);
    w.u8(m.input.gender);
    w.u8(m.input.skin);
}
fn restore_member(r: &mut Reader<'_>) -> Result<Member, Error> {
    let seat = r.u8()?;
    let actor = crate::ActorId(r.u64()?);
    let avatar_object = r.i16()?;
    let avatar_id = r.u32()?;
    let role = r.u8()?;
    let mut registers = [0; 16];
    for n in &mut registers {
        *n = r.i16()?
    }
    let mut skills = [0; 6];
    for n in &mut skills {
        *n = r.i16()?
    }
    let input = MemberInput {
        role,
        registers,
        skills,
        owner_authorized: r.bool()?,
        gender: r.u8()?,
        skin: r.u8()?,
    };
    let m = Member {
        seat,
        actor,
        avatar_object,
        avatar_id,
        input,
    };
    if !valid_member(m) {
        return Err(Error::InvalidCheckpoint);
    }
    Ok(m)
}
fn valid_member(m: Member) -> bool {
    m.seat < 16 && m.actor.0 != 0 && m.avatar_object > 0 && Account::Avatar(m.avatar_id).valid()
}
fn member_present(m: Member, r: &Roster) -> bool {
    r.get(m.seat as usize).is_some_and(|x| *x == Some(m))
}

#[cfg(test)]
mod tests;
