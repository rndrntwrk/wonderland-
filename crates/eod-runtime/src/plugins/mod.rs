// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source-backed native EOD families and private typed provider operations.

pub mod casino;
pub(crate) mod codec;
pub mod common;
pub mod service;
pub mod social;
use crate::*;
use codec::{Reader, Writer};
use common::*;
use std::fmt;

#[derive(Clone)]
pub enum NativePluginInput {
    Casino(casino::Config),
    Social(social::Config),
    Service(service::Config),
}
#[derive(Clone)]
pub enum NativeVmInput {
    Casino(casino::VmInput),
    Social(social::VmInput),
    Service(service::VmInput),
}
#[derive(Clone, PartialEq, Eq)]
// Keep the typed provider API value-based; private queued actions are boxed and
// both operation records and retained bytes have explicit host limits.
#[allow(clippy::large_enum_variant)]
pub enum ProviderOperation {
    Casino(casino::Operation),
    Social(social::Operation),
    Service(service::Operation),
}
#[derive(Clone, PartialEq, Eq)]
pub enum ProviderReply {
    Casino(casino::Reply),
    Social(social::Reply),
    Service(service::Reply),
}
macro_rules! redacted { ($($ty:ident),+ $(,)?) => {$(
impl fmt::Debug for $ty {
    fn fmt(&self, f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str(concat!(stringify!($ty),"([REDACTED])"))}
}
)+}; }
redacted!(
    NativePluginInput,
    NativeVmInput,
    ProviderOperation,
    ProviderReply
);

/// Explicit VM-visible effects. The accepted-tick adapter applies these values;
/// private cards, provider requests and kernel checkpoints cannot convert here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeCommand {
    BatchGraphics {
        objects: Vec<i16>,
        graphics: Vec<u8>,
    },
    ForceInteraction {
        caller: i16,
        callee: i16,
        interaction: u16,
    },
    SetOutfit {
        avatar_id: u32,
        scope: OutfitScope,
        outfit: u64,
    },
    /// At the accepted VM barrier, replace this default only if its current
    /// asset still equals `expected`. A late durable deletion must not overwrite
    /// a newer default choice. The adapter validates replacement ownership.
    SetOutfitIfCurrent {
        avatar_id: u32,
        scope: OutfitScope,
        expected: u64,
        outfit: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutfitScope {
    DefaultDaywear,
    DefaultSleepwear,
    DefaultSwimwear,
    DynamicDaywear,
    DynamicSleepwear,
    DynamicSwimwear,
    DynamicCostume,
    DecorationHead,
    DecorationBack,
    DecorationShoes,
    DecorationTail,
}
impl OutfitScope {
    /// Numeric VMPersonSuits scope in the pinned original VM command contract.
    pub fn source_value(self) -> u8 {
        match self {
            Self::DefaultDaywear => 0,
            Self::DefaultSleepwear => 5,
            Self::DefaultSwimwear => 2,
            Self::DynamicDaywear => 22,
            Self::DynamicSleepwear => 24,
            Self::DynamicSwimwear => 23,
            Self::DynamicCostume => 25,
            Self::DecorationHead => 8,
            Self::DecorationBack => 9,
            Self::DecorationShoes => 10,
            Self::DecorationTail => 11,
        }
    }
}
impl NativeCommand {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::BatchGraphics { objects, graphics } => {
                !objects.is_empty()
                    && objects.len() <= 4096
                    && objects.len() == graphics.len()
                    && objects.iter().all(|v| *v > 0)
            }
            Self::ForceInteraction { caller, callee, .. } => *caller > 0 && *callee > 0,
            Self::SetOutfit {
                avatar_id, outfit, ..
            } => *avatar_id != 0 && *outfit != 0,
            Self::SetOutfitIfCurrent {
                avatar_id,
                scope,
                expected,
                outfit,
            } => {
                *avatar_id != 0
                    && *expected != 0
                    && *outfit != 0
                    && matches!(
                        scope,
                        OutfitScope::DefaultDaywear
                            | OutfitScope::DefaultSleepwear
                            | OutfitScope::DefaultSwimwear
                    )
            }
        }
    }
}

#[derive(Clone)]
pub(crate) enum FamilyState {
    Casino(casino::State),
    Social(social::State),
    Service(service::State),
}
impl FamilyState {
    pub(crate) fn required_peers(&self) -> Vec<InstanceId> {
        match self {
            Self::Social(v) => v.required_peers(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn reserved_avatars(&self) -> Vec<u32> {
        match self {
            Self::Service(v) => v.reserved_avatars(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn new(input: NativePluginInput) -> Result<Self, Error> {
        Ok(match input {
            NativePluginInput::Casino(v) => Self::Casino(casino::State::new(v)?),
            NativePluginInput::Social(v) => Self::Social(social::State::new(v)?),
            NativePluginInput::Service(v) => Self::Service(service::State::new(v)?),
        })
    }
    pub(crate) fn validate_peers(
        &self,
        group: InstanceId,
        cluster: u64,
        peers: &[PeerGroup],
    ) -> bool {
        match self {
            Self::Social(v) => v.validate_peers(group, cluster, peers),
            Self::Service(v) => peers
                .iter()
                .find(|p| p.group == group)
                .is_some_and(|p| v.validate_context(&p.roster, p.closing)),
            _ => true,
        }
    }
    pub(crate) fn trusted_object(&self) -> Option<u32> {
        match self {
            Self::Casino(v) => Some(v.trusted_object()),
            Self::Social(_) => None,
            Self::Service(v) => v.trusted_object(),
        }
    }
    pub(crate) fn plugin(&self) -> PluginId {
        match self {
            Self::Casino(v) => v.plugin(),
            Self::Social(v) => v.plugin(),
            Self::Service(v) => v.plugin(),
        }
    }
    pub(crate) fn start(&mut self, roster: &Roster, tick: u64) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.start(roster, tick),
            Self::Social(v) => v.start(roster, tick),
            Self::Service(v) => v.start(roster, tick),
        }
    }
    pub(crate) fn join(
        &mut self,
        member: Member,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.join(member, roster, tick),
            Self::Social(v) => v.join(member, roster, tick),
            Self::Service(v) => v.join(member, roster, tick),
        }
    }
    pub(crate) fn allows(&self, event: &str, binary: bool) -> bool {
        match self {
            Self::Casino(v) => v.allows(event, binary),
            Self::Social(v) => v.allows(event, binary),
            Self::Service(v) => v.allows(event, binary),
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
            Self::Casino(v) => v.message(member, event, payload, roster, tick),
            Self::Social(v) => v.message(member, event, payload, roster, tick),
            Self::Service(v) => v.message(member, event, payload, roster, tick),
        }
    }
    pub(crate) fn tick(&mut self, roster: &Roster, tick: u64) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.tick(roster, tick),
            Self::Social(v) => v.tick(roster, tick),
            Self::Service(v) => v.tick(roster, tick),
        }
    }
    pub(crate) fn peer(
        &mut self,
        source: &PeerSource,
        signal: &Signal,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.peer(source, signal, roster, tick),
            Self::Social(v) => v.peer(source, signal, roster, tick),
            Self::Service(v) => v.peer(source, signal, roster, tick),
        }
    }
    pub(crate) fn leave(
        &mut self,
        member: Member,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.leave(member, roster, tick),
            Self::Social(v) => v.leave(member, roster, tick),
            Self::Service(v) => v.leave(member, roster, tick),
        }
    }
    pub(crate) fn rebind(
        &mut self,
        member: Member,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.rebind(member, roster, tick),
            Self::Social(v) => v.rebind(member, roster, tick),
            Self::Service(v) => v.rebind(member, roster, tick),
        }
    }
    pub(crate) fn shutdown(&mut self, roster: &Roster, tick: u64) -> Result<Actions, Error> {
        match self {
            Self::Casino(v) => v.shutdown(roster, tick),
            Self::Social(v) => v.shutdown(roster, tick),
            Self::Service(v) => v.shutdown(roster, tick),
        }
    }
    pub(crate) fn can_close(&self) -> bool {
        match self {
            Self::Casino(v) => v.can_close(),
            Self::Social(v) => v.can_close(),
            Self::Service(v) => v.can_close(),
        }
    }
    pub(crate) fn validate(&self, roster: &Roster) -> bool {
        match self {
            Self::Casino(v) => v.validate(roster),
            Self::Social(v) => v.validate(roster),
            Self::Service(v) => v.validate(roster),
        }
    }
    pub(crate) fn vm_event(
        &mut self,
        input: &NativeVmInput,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match (self, input) {
            (Self::Casino(s), NativeVmInput::Casino(v)) => s.vm_event(v, roster, tick),
            (Self::Social(s), NativeVmInput::Social(v)) => s.vm_event(v, roster, tick),
            (Self::Service(s), NativeVmInput::Service(v)) => s.vm_event(v, roster, tick),
            _ => Err(Error::WrongPlugin),
        }
    }
    pub(crate) fn reply(
        &mut self,
        callback: u64,
        input: &ProviderReply,
        roster: &Roster,
        tick: u64,
    ) -> Result<Actions, Error> {
        match (self, input) {
            (Self::Casino(s), ProviderReply::Casino(v)) => s.reply(callback, v, roster, tick),
            (Self::Social(s), ProviderReply::Social(v)) => s.reply(callback, v, roster, tick),
            (Self::Service(s), ProviderReply::Service(v)) => s.reply(callback, v, roster, tick),
            _ => Err(Error::ProviderReceiptMismatch),
        }
    }
    pub(crate) fn pending_operations(&self) -> Vec<(u64, ProviderOperation)> {
        match self {
            Self::Casino(s) => s
                .pending_operations()
                .into_iter()
                .map(|(id, o)| (id, ProviderOperation::Casino(o)))
                .collect(),
            Self::Social(s) => s
                .pending_operations()
                .into_iter()
                .map(|(id, o)| (id, ProviderOperation::Social(o)))
                .collect(),
            Self::Service(s) => s
                .pending_operations()
                .into_iter()
                .map(|(id, o)| (id, ProviderOperation::Service(o)))
                .collect(),
        }
    }
    pub(crate) fn save_private(&self) -> Result<Vec<u8>, Error> {
        let mut w = Writer::default();
        match self {
            Self::Casino(v) => v.save(&mut w),
            Self::Social(v) => v.save(&mut w),
            Self::Service(v) => v.save(&mut w),
        };
        if w.0.len() > MAX_STATE_BYTES {
            return Err(Error::CheckpointTooLarge);
        }
        Ok(w.0)
    }
    pub(crate) fn restore_private(plugin: PluginId, bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_STATE_BYTES {
            return Err(Error::InvalidCheckpoint);
        }
        let mut r = Reader::new(bytes);
        let s = match plugin.0 {
            0xCB2819CB | 0x0B2A6B83 | 0x2B2FC514 | 0x00001001 => {
                Self::Casino(casino::State::restore(plugin, &mut r)?)
            }
            0x2D642D39 | 0x8ADFC7A2 | 0x00001005 | 0x00001006 | 0x6C5C7555 | 0x6D113845
            | 0xCCC5BC43 | 0xEC55D705 => Self::Social(social::State::restore(plugin, &mut r)?),
            0x00001000 | 0x00001003 | 0x00001004 | 0x00002000 | 0x2B58020B | 0xCB492685
            | 0x8B300068 | 0xAA5E36DC | 0x895C1CEB | 0x897F82F5 => {
                Self::Service(service::State::restore(plugin, &mut r)?)
            }
            _ => return Err(Error::UnverifiedPlugin),
        };
        r.finish()?;
        if s.plugin() != plugin {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(s)
    }
}
impl ProviderOperation {
    pub(crate) fn save(&self, w: &mut Writer) {
        match self {
            Self::Casino(v) => {
                w.u8(1);
                v.save(w)
            }
            Self::Social(v) => {
                w.u8(2);
                v.save(w)
            }
            Self::Service(v) => {
                w.u8(3);
                v.save(w)
            }
        }
    }
    pub(crate) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(match r.u8()? {
            1 => Self::Casino(casino::Operation::restore(r)?),
            2 => Self::Social(social::Operation::restore(r)?),
            3 => Self::Service(service::Operation::restore(r)?),
            _ => return Err(Error::InvalidCheckpoint),
        })
    }
    pub(crate) fn private_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut w = Writer::default();
        self.save(&mut w);
        if w.0.len() > MAX_OPERATION_BYTES {
            return Err(Error::PersistenceTooLarge);
        }
        Ok(w.0)
    }
    /// Native private-provider serialization only. This is not a network/UI wire message.
    pub fn write_private(&self, destination: &mut [u8]) -> Result<usize, Error> {
        let bytes = self.private_bytes()?;
        if bytes.len() > destination.len() {
            return Err(Error::PersistenceTooLarge);
        }
        destination[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}
impl ProviderReply {
    pub(crate) fn save(&self, w: &mut Writer) {
        match self {
            Self::Casino(v) => {
                w.u8(1);
                v.save(w)
            }
            Self::Social(v) => {
                w.u8(2);
                v.save(w)
            }
            Self::Service(v) => {
                w.u8(3);
                v.save(w)
            }
        }
    }
    pub(crate) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(match r.u8()? {
            1 => Self::Casino(casino::Reply::restore(r)?),
            2 => Self::Social(social::Reply::restore(r)?),
            3 => Self::Service(service::Reply::restore(r)?),
            _ => return Err(Error::InvalidCheckpoint),
        })
    }
    pub(crate) fn private_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut w = Writer::default();
        self.save(&mut w);
        if w.0.len() > MAX_OPERATION_BYTES {
            return Err(Error::PersistenceTooLarge);
        }
        Ok(w.0)
    }
    /// Native private-provider serialization only. This is not a network/UI wire message.
    pub fn write_private(&self, destination: &mut [u8]) -> Result<usize, Error> {
        let bytes = self.private_bytes()?;
        if bytes.len() > destination.len() {
            return Err(Error::PersistenceTooLarge);
        }
        destination[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}
#[derive(Clone)]
pub(crate) struct NativeGroup {
    pub(crate) object: u32,
    pub(crate) cluster: u64,
    pub(crate) invoker: InvokerId,
    pub(crate) attached: bool,
    pub(crate) closing: bool,
    pub(crate) members: [Option<InstanceId>; MAX_MEMBERS],
    pub(crate) roster: Roster,
    pub(crate) handler: FamilyState,
}
