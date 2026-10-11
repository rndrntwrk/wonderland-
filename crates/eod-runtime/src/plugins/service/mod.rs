// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Native source-backed service, clothing, Carducopia and secure-trade handlers.
mod draw;
mod simple;
mod trade;
mod types;
mod wardrobe;
mod wire;
use crate::{
    plugins::{
        codec::{Reader, Writer},
        common::*,
    },
    *,
};
pub use simple::{
    CooldownDecision, CooldownRecord, encode_local_cooldowns, evaluate_local_cooldown,
};
pub use types::*;
#[derive(Clone)]
pub(crate) enum State {
    Simple(simple::Simple),
    Draw(draw::Draw),
    Wardrobe(wardrobe::Wardrobe),
    Trade(Box<trade::Trade>),
}
impl State {
    pub(crate) fn reserved_avatars(&self) -> Vec<u32> {
        match self {
            Self::Wardrobe(v) => v.reserved_avatars(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        Ok(match config {
            Config::DrawCard { object, seed } => Self::Draw(draw::Draw::new(object, seed)?),
            Config::Wardrobe {
                kind,
                object,
                rack_type,
                defaults,
            } => Self::Wardrobe(wardrobe::Wardrobe::new(kind, object, rack_type, defaults)?),
            Config::SecureTrade { untradable_guids } => {
                Self::Trade(Box::new(trade::Trade::new(untradable_guids)?))
            }
            c => Self::Simple(simple::Simple::new(c)?),
        })
    }
    pub(crate) fn validate_context(&self, roster: &Roster, closing: bool) -> bool {
        match self {
            Self::Simple(v) => v.validate_context(roster, closing),
            Self::Wardrobe(v) => v.validate_context(roster, closing),
            _ => true,
        }
    }
    pub(crate) fn trusted_object(&self) -> Option<u32> {
        match self {
            Self::Simple(v) => v.trusted_object(),
            Self::Draw(v) => v.trusted_object(),
            Self::Wardrobe(v) => v.trusted_object(),
            Self::Trade(_) => None,
        }
    }
    pub(crate) fn plugin(&self) -> PluginId {
        match self {
            Self::Simple(v) => v.plugin(),
            Self::Draw(v) => v.plugin(),
            Self::Wardrobe(v) => v.plugin(),
            Self::Trade(v) => v.plugin(),
        }
    }
    pub(crate) fn join(
        &mut self,
        member: Member,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.join(member, roster, tick),
            Self::Draw(v) => v.join(member, roster, tick),
            Self::Wardrobe(v) => v.join(member, roster, tick),
            Self::Trade(v) => v.join(member, roster, tick),
        }
    }
    pub(crate) fn allows(&self, event: &str, binary: bool) -> bool {
        match self {
            Self::Simple(v) => v.allows(event, binary),
            Self::Draw(v) => v.allows(event, binary),
            Self::Wardrobe(v) => v.allows(event, binary),
            Self::Trade(v) => v.allows(event, binary),
        }
    }
    pub(crate) fn message(
        &mut self,
        member: Member,
        event: &str,
        payload: &[u8],
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.message(member, event, payload, roster, tick),
            Self::Draw(v) => v.message(member, event, payload, roster, tick),
            Self::Wardrobe(v) => v.message(member, event, payload, roster, tick),
            Self::Trade(v) => v.message(member, event, payload, roster, tick),
        }
    }
    pub(crate) fn reply(
        &mut self,
        callback: u64,
        reply: &Reply,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.reply(callback, reply, roster, tick),
            Self::Draw(v) => v.reply(callback, reply, roster, tick),
            Self::Wardrobe(v) => v.reply(callback, reply, roster, tick),
            Self::Trade(v) => v.reply(callback, reply, roster, tick),
        }
    }
    pub(crate) fn leave(
        &mut self,
        member: Member,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.leave(member, roster, tick),
            Self::Draw(v) => v.leave(member, roster, tick),
            Self::Wardrobe(v) => v.leave(member, roster, tick),
            Self::Trade(v) => v.leave(member, roster, tick),
        }
    }
    pub(crate) fn rebind(
        &mut self,
        member: Member,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.rebind(member, roster, tick),
            Self::Draw(v) => v.rebind(member, roster, tick),
            Self::Wardrobe(v) => v.rebind(member, roster, tick),
            Self::Trade(v) => v.rebind(member, roster, tick),
        }
    }
    pub(crate) fn shutdown(&mut self, roster: &Roster, tick: u64) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.shutdown(roster, tick),
            Self::Draw(v) => v.shutdown(roster, tick),
            Self::Wardrobe(v) => v.shutdown(roster, tick),
            Self::Trade(v) => v.shutdown(roster, tick),
        }
    }
    pub(crate) fn validate(&self, roster: &Roster) -> bool {
        match self {
            Self::Simple(v) => v.validate(roster),
            Self::Draw(v) => v.validate(roster),
            Self::Wardrobe(v) => v.validate(roster),
            Self::Trade(v) => v.validate(roster),
        }
    }
    pub(crate) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        match self {
            Self::Simple(v) => v.pending_operations(),
            Self::Draw(v) => v.pending_operations(),
            Self::Wardrobe(v) => v.pending_operations(),
            Self::Trade(v) => v.pending_operations(),
        }
    }
    pub(crate) fn start(&mut self, roster: &Roster, tick: u64) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.start(roster, tick),
            Self::Draw(v) => v.start(roster, tick),
            Self::Wardrobe(v) => v.start(roster, tick),
            Self::Trade(_) => Ok(Actions::default()),
        }
    }
    pub(crate) fn tick(&mut self, roster: &Roster, tick: u64) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.tick(roster, tick),
            Self::Wardrobe(v) => v.tick(roster, tick),
            Self::Trade(v) => v.tick(roster, tick),
            Self::Draw(_) => Ok(Actions::default()),
        }
    }
    pub(crate) fn vm_event(
        &mut self,
        input: &VmInput,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Simple(v) => v.vm_event(input, roster, tick),
            Self::Wardrobe(v) => v.vm_event(input, roster, tick),
            Self::Draw(v) => v.vm_event(input, roster, tick),
            Self::Trade(_) => Err(Error::WrongPlugin),
        }
    }
    pub(crate) fn peer(
        &mut self,
        _: &PeerSource,
        _: &Signal,
        _: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        Err(Error::EventNotAllowed)
    }
    pub(crate) fn can_close(&self) -> bool {
        match self {
            Self::Simple(_) => true,
            Self::Draw(v) => v.can_close(),
            Self::Wardrobe(v) => v.can_close(),
            Self::Trade(v) => v.can_close(),
        }
    }
    pub(crate) fn save(&self, w: &mut Writer) {
        match self {
            Self::Simple(v) => {
                w.u8(0);
                v.save(w)
            }
            Self::Draw(v) => {
                w.u8(1);
                v.save(w)
            }
            Self::Wardrobe(v) => {
                w.u8(2);
                v.save(w)
            }
            Self::Trade(v) => {
                w.u8(3);
                v.save(w)
            }
        }
    }
    pub(crate) fn restore(plugin: PluginId, r: &mut Reader<'_>) -> Result<Self, Error> {
        let v = match r.u8()? {
            0 => Self::Simple(simple::Simple::restore(r)?),
            1 => Self::Draw(draw::Draw::restore(r)?),
            2 => Self::Wardrobe(wardrobe::Wardrobe::restore(r)?),
            3 => Self::Trade(Box::new(trade::Trade::restore(r)?)),
            _ => return Err(Error::InvalidCheckpoint),
        };
        if v.plugin() != plugin {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(v)
    }
}
