// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Bounded private cooperative game state. No VM, transport or durable provider.

mod maze;
mod paperchase;
mod pizza;
mod rng;

use crate::{protocol::PrivateBody, registry, *};
use std::fmt;

pub use maze::MazeRole;
pub use pizza::PizzaTuning;
pub(crate) const MAX_GAME_BYTES: usize = 8192;
pub(crate) type Roster = [i16; 4];

/// Supplied by a trusted native seed source. Never derive this from public VM
/// state or accept it from UI messages. It is not a System.Random seed contract.
#[derive(Clone, Copy)]
pub struct PrivateSeed(u64);
impl PrivateSeed {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
}
impl fmt::Debug for PrivateSeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateSeed([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug)]
pub enum GameControllerInput {
    PaperChase { seed: PrivateSeed },
    PizzaMaker { seed: PrivateSeed },
    Maze { seed: PrivateSeed },
}

#[derive(Clone, Copy, Debug)]
pub struct GameControllerRequest {
    pub object: u32,
    pub invoker: InvokerId,
    pub input: GameControllerInput,
}

/// Native controller capability, separate from every UI session/envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameControllerTicket {
    pub host_scope: HostScopeId,
    pub host_epoch: u64,
    pub instance: InstanceId,
}
impl GameControllerTicket {
    pub fn instance_address(self) -> InstanceAddress {
        InstanceAddress {
            host_scope: self.host_scope,
            instance: self.instance,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum GamePlayerInput {
    /// Source temp0: 1..=3 chooses a role; 0 chooses the first free role.
    PaperChase {
        slot: i16,
    },
    /// Source temp0 is zero-based; tuning corresponds to source temp1..=7.
    PizzaMaker {
        station: u8,
        tuning: PizzaTuning,
    },
    Maze {
        role: MazeRole,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct GamePlayerRequest {
    pub connection: ConnectionId,
    pub game: InstanceAddress,
    pub invoker: InvokerId,
    pub avatar_object: i16,
    pub input: GamePlayerInput,
}

/// These are the only incoming SimAntics events used by these three handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameVmInput {
    PizzaRespondPhone,
    PizzaAllContributed,
    PizzaRespondBake,
}
impl GameVmInput {
    pub fn source_code(self) -> i16 {
        match self {
            Self::PizzaRespondPhone => 6,
            Self::PizzaAllContributed => 7,
            Self::PizzaRespondBake => 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameObjectEvent {
    PaperChaseSetResult(i16),
    PaperChaseSetLetter(i16),
    PaperChaseShowResult,
    PaperChaseIdle,
    PizzaRingPhone(i16),
    PizzaContribute(i16),
    PizzaBake(i16),
    PizzaPayoutResult(i16),
    PizzaRestart,
    MazeFailure,
    MazeSuccess,
}
impl GameObjectEvent {
    pub fn source_event(&self) -> (i16, Vec<i16>) {
        match *self {
            Self::PaperChaseSetResult(value) => (1, vec![value]),
            Self::PaperChaseSetLetter(value) => (2, vec![value]),
            Self::PaperChaseShowResult => (3, vec![]),
            Self::PaperChaseIdle => (4, vec![]),
            Self::PizzaRingPhone(value) => (1, vec![value]),
            Self::PizzaContribute(value) => (2, vec![value]),
            Self::PizzaBake(value) => (3, vec![value]),
            Self::PizzaPayoutResult(value) => (4, vec![value]),
            Self::PizzaRestart => (5, vec![]),
            Self::MazeFailure => (1, vec![]),
            Self::MazeSuccess => (2, vec![]),
        }
    }
}

#[derive(Clone)]
pub(crate) struct SharedGame {
    pub(crate) object: u32,
    pub(crate) invoker: InvokerId,
    pub(crate) attached: bool,
    pub(crate) seats: [Option<InstanceId>; 4],
    pub(crate) handler: GameState,
}

#[derive(Clone)]
pub(crate) enum GameState {
    PaperChase(paperchase::PaperChase),
    Pizza(pizza::Pizza),
    Maze(Box<maze::Maze>),
}

pub(crate) enum Target {
    Controller,
    Seat(usize),
}
pub(crate) enum Action {
    Ui {
        seat: usize,
        event: &'static str,
        body: PrivateBody,
    },
    Object {
        target: Target,
        event: GameObjectEvent,
    },
}
#[derive(Default)]
pub(crate) struct Actions {
    pub(crate) items: Vec<Action>,
    pub(crate) shutdown: bool,
}
impl Actions {
    pub(crate) fn text(&mut self, seat: usize, event: &'static str, body: String) {
        self.items.push(Action::Ui {
            seat,
            event,
            body: PrivateBody::Text(body),
        });
    }
    pub(crate) fn binary(&mut self, seat: usize, event: &'static str, body: Vec<u8>) {
        self.items.push(Action::Ui {
            seat,
            event,
            body: PrivateBody::Binary(body),
        });
    }
    pub(crate) fn broadcast_text(&mut self, roster: &Roster, event: &'static str, body: String) {
        for (seat, avatar) in roster.iter().enumerate() {
            if *avatar != 0 {
                self.text(seat, event, body.clone());
            }
        }
    }
    pub(crate) fn broadcast_binary(&mut self, roster: &Roster, event: &'static str, body: Vec<u8>) {
        for (seat, avatar) in roster.iter().enumerate() {
            if *avatar != 0 {
                self.binary(seat, event, body.clone());
            }
        }
    }
    pub(crate) fn object(&mut self, event: GameObjectEvent) {
        self.items.push(Action::Object {
            target: Target::Controller,
            event,
        });
    }
}

impl GameState {
    pub(crate) fn new(input: GameControllerInput) -> Result<Self, Error> {
        Ok(match input {
            GameControllerInput::PaperChase { seed } => {
                Self::PaperChase(paperchase::PaperChase::new(seed.0))
            }
            GameControllerInput::PizzaMaker { seed } => Self::Pizza(pizza::Pizza::new(seed.0)?),
            GameControllerInput::Maze { seed } => Self::Maze(Box::new(maze::Maze::new(seed.0)?)),
        })
    }
    pub(crate) fn plugin(&self) -> PluginId {
        match self {
            Self::PaperChase(_) => registry::PAPER_CHASE_PLUGIN,
            Self::Pizza(_) => registry::PIZZA_MAKER_PLUGIN,
            Self::Maze(_) => registry::MAZE_PLUGIN,
        }
    }
    pub(crate) fn seats(&self) -> usize {
        match self {
            Self::PaperChase(_) => 3,
            Self::Pizza(_) => 4,
            Self::Maze(_) => 2,
        }
    }
    pub(crate) fn select_seat(
        &self,
        input: GamePlayerInput,
        seats: &[Option<InstanceId>; 4],
    ) -> Result<usize, Error> {
        let seat = match (self, input) {
            (Self::PaperChase(_), GamePlayerInput::PaperChase { slot: 0 }) => seats[..3]
                .iter()
                .position(Option::is_none)
                .ok_or(Error::ParticipantLimit)?,
            (Self::PaperChase(_), GamePlayerInput::PaperChase { slot: 1..=3 }) => {
                if let GamePlayerInput::PaperChase { slot } = input {
                    (slot - 1) as usize
                } else {
                    unreachable!()
                }
            }
            (Self::Pizza(_), GamePlayerInput::PizzaMaker { station: 0..=3, .. }) => {
                if let GamePlayerInput::PizzaMaker { station, .. } = input {
                    usize::from(station)
                } else {
                    unreachable!()
                }
            }
            (Self::Maze(_), GamePlayerInput::Maze { role }) => role.seat(),
            (Self::PaperChase(_), GamePlayerInput::PaperChase { .. })
            | (Self::Pizza(_), GamePlayerInput::PizzaMaker { .. }) => {
                return Err(Error::InvalidPluginInput);
            }
            _ => return Err(Error::WrongPlugin),
        };
        if seats[seat].is_some() {
            return Err(Error::ParticipantAlreadyConnected);
        }
        Ok(seat)
    }
    pub(crate) fn join(
        &mut self,
        seat: usize,
        input: GamePlayerInput,
        roster: &Roster,
    ) -> Result<Actions, Error> {
        match (self, input) {
            (Self::PaperChase(state), GamePlayerInput::PaperChase { .. }) => {
                state.join(seat, roster)
            }
            (Self::Pizza(state), GamePlayerInput::PizzaMaker { tuning, .. }) => {
                state.join(seat, tuning, roster)
            }
            (Self::Maze(state), GamePlayerInput::Maze { role }) => Ok(state.join(role, roster)),
            _ => Err(Error::WrongPlugin),
        }
    }
    pub(crate) fn message(
        &mut self,
        seat: usize,
        event: &str,
        body: &[u8],
        roster: &Roster,
    ) -> Result<Actions, Error> {
        match self {
            Self::PaperChase(state) => state.message(seat, event, body, roster),
            Self::Pizza(state) => state.message(seat, event, body, roster),
            Self::Maze(state) => Ok(state.message(seat, body)),
        }
    }
    pub(crate) fn tick(&mut self, roster: &Roster) -> Result<Actions, Error> {
        match self {
            Self::PaperChase(state) => state.tick(roster),
            Self::Pizza(state) => state.tick(roster),
            Self::Maze(state) => state.tick(roster),
        }
    }
    pub(crate) fn vm_event(
        &mut self,
        event: GameVmInput,
        roster: &Roster,
    ) -> Result<Actions, Error> {
        match self {
            Self::Pizza(state) => state.vm_event(event, roster),
            _ => Err(Error::WrongPlugin),
        }
    }
    pub(crate) fn leave(&mut self, seat: usize, roster: &Roster) -> Result<Actions, Error> {
        match self {
            Self::PaperChase(state) => state.leave(roster),
            Self::Pizza(state) => state.leave(seat, roster),
            Self::Maze(state) => Ok(state.leave(roster)),
        }
    }
    pub(crate) fn rebind(&self, seat: usize, roster: &Roster) -> Actions {
        match self {
            Self::PaperChase(state) => state.rebind(seat, roster),
            Self::Pizza(state) => state.rebind(seat, roster),
            Self::Maze(state) => state.rebind(seat),
        }
    }
    pub(crate) fn save_private(&self) -> Vec<u8> {
        let mut writer = Writer(Vec::new());
        match self {
            Self::PaperChase(state) => state.save(&mut writer),
            Self::Pizza(state) => state.save(&mut writer),
            Self::Maze(state) => state.save(&mut writer),
        }
        writer.0
    }
    pub(crate) fn restore_private(plugin: PluginId, bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_GAME_BYTES {
            return Err(Error::InvalidCheckpoint);
        }
        let mut reader = Reader { bytes, at: 0 };
        let state = match plugin {
            registry::PAPER_CHASE_PLUGIN => {
                Self::PaperChase(paperchase::PaperChase::restore(&mut reader)?)
            }
            registry::PIZZA_MAKER_PLUGIN => Self::Pizza(pizza::Pizza::restore(&mut reader)?),
            registry::MAZE_PLUGIN => Self::Maze(Box::new(maze::Maze::restore(&mut reader)?)),
            _ => return Err(Error::InvalidCheckpoint),
        };
        if reader.at != bytes.len() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(state)
    }
    pub(crate) fn validate_roster(&self, roster: &Roster) -> bool {
        match self {
            Self::PaperChase(state) => state.validate_roster(roster),
            Self::Pizza(state) => state.validate_roster(roster),
            Self::Maze(state) => state.validate_roster(roster),
        }
    }
}

pub(crate) fn source_integer(bytes: &[u8]) -> Option<i16> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_end_matches('\0')
        .trim_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

pub(crate) struct Writer(pub(crate) Vec<u8>);
impl Writer {
    pub(crate) fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    pub(crate) fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(crate) fn i16(&mut self, value: i16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    pub(crate) fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
}
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    pub(crate) fn take(&mut self, len: usize) -> Result<&[u8], Error> {
        let end = self.at.checked_add(len).ok_or(Error::InvalidCheckpoint)?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or(Error::InvalidCheckpoint)?;
        self.at = end;
        Ok(bytes)
    }
    pub(crate) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn bool(&mut self) -> Result<bool, Error> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidCheckpoint),
        }
    }
    pub(crate) fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
    pub(crate) fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        ))
    }
}
