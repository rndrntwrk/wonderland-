// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Bounded native family messages. Peer and provider channels have no inbound UI encoding.

use super::{NativeCommand, ProviderOperation};
use crate::{ActorId, InstanceId, PluginId, protocol::PrivateBody};

pub const MAX_MEMBERS: usize = 16;
pub const MAX_STATE_BYTES: usize = 256 * 1024;
pub const MAX_OPERATION_BYTES: usize = 256 * 1024;
pub const MAX_PEER_BYTES: usize = 64 * 1024;

/// Trusted Invoke Plugin input snapshot. The host never accepts these values in
/// `ClientMessage`. A family's source adapter defines the role/register meaning.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct MemberInput {
    pub role: u8,
    pub registers: [i16; 16],
    pub skills: [i16; 6],
    pub owner_authorized: bool,
    pub gender: u8,
    pub skin: u8,
}
impl std::fmt::Debug for MemberInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemberInput([REDACTED])")
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub seat: u8,
    pub actor: ActorId,
    pub avatar_object: i16,
    pub avatar_id: u32,
    pub input: MemberInput,
}
impl std::fmt::Debug for Member {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Member([REDACTED])")
    }
}
pub type Roster = [Option<Member>; MAX_MEMBERS];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Controller,
    Member(u8),
    All,
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Signal {
    pub code: u16,
    pub numbers: Vec<i64>,
    pub bytes: Vec<u8>,
}
#[derive(Clone)]
pub struct PeerSource {
    pub destination: InstanceId,
    pub plugin: PluginId,
    pub object: u32,
    pub group: InstanceId,
    pub roster: Roster,
}
/// Host-resolved private context for retained cross-plugin references.
#[derive(Clone, Copy)]
pub(crate) struct PeerGroup {
    pub(crate) group: InstanceId,
    pub(crate) plugin: PluginId,
    pub(crate) object: u32,
    pub(crate) cluster: u64,
    pub(crate) closing: bool,
    pub(crate) roster: Roster,
}
pub(crate) enum Action {
    Ui {
        target: Target,
        event: &'static str,
        body: PrivateBody,
    },
    Object {
        target: Target,
        code: i16,
        args: Vec<i16>,
    },
    Provider {
        callback: u64,
        operation: Box<ProviderOperation>,
    },
    Peer {
        plugin: PluginId,
        signal: Signal,
    },
    Close {
        target: Target,
    },
    Command(NativeCommand),
}
#[derive(Default)]
pub(crate) struct Actions {
    pub(crate) items: Vec<Action>,
}
impl Actions {
    pub(crate) fn text(&mut self, target: Target, event: &'static str, body: String) {
        self.items.push(Action::Ui {
            target,
            event,
            body: PrivateBody::Text(body),
        });
    }
    pub(crate) fn binary(&mut self, target: Target, event: &'static str, body: Vec<u8>) {
        self.items.push(Action::Ui {
            target,
            event,
            body: PrivateBody::Binary(body),
        });
    }
    pub(crate) fn object(&mut self, target: Target, code: i16, args: Vec<i16>) {
        self.items.push(Action::Object { target, code, args });
    }
    pub(crate) fn provider(&mut self, callback: u64, operation: ProviderOperation) {
        self.items.push(Action::Provider {
            callback,
            operation: Box::new(operation),
        });
    }
    pub(crate) fn peer(&mut self, plugin: PluginId, signal: Signal) {
        self.items.push(Action::Peer { plugin, signal });
    }
    pub(crate) fn close(&mut self, target: Target) {
        self.items.push(Action::Close { target });
    }
    pub(crate) fn command(&mut self, command: NativeCommand) {
        self.items.push(Action::Command(command));
    }
}
