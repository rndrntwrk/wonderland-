// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source-backed group games, buzzer controllers, and nightclub kernels.
//! All identities, peer links, random seeds, and VM observations enter through
//! native capabilities. No private state has a public VM serialization.

mod band;
mod buzzer_host;
mod buzzer_player;
mod club;
mod floor;
mod font;
mod util;
mod war;
mod war_rules;

use crate::plugins::{NativeCommand, codec::*, common::*};
use crate::{Error, InstanceId, PluginId};

pub const WAR_GAME: PluginId = PluginId(0x2D642D39);
pub const BAND: PluginId = PluginId(0x8ADFC7A2);
pub const BUZZER_PLAYER: PluginId = PluginId(0x1005);
pub const BUZZER_HOST: PluginId = PluginId(0x1006);
pub const DJ_STATION: PluginId = PluginId(0x6C5C7555);
pub const NC_FLOOR: PluginId = PluginId(0x6D113845);
pub const NIGHTCLUB: PluginId = PluginId(0xCCC5BC43);
pub const DANCE_PLATFORM: PluginId = PluginId(0xEC55D705);

/// Native-only seed. The replay stream is SplitMix64, not System.Random parity.
#[derive(Clone, Copy)]
pub struct Seed(u64);
impl Seed {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

/// A bounded, authoritative display-name snapshot for admitted avatars.
#[derive(Clone, PartialEq, Eq)]
pub struct AvatarName {
    pub avatar_id: u32,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloorTile {
    pub object: i16,
    pub x: i16,
    pub y: i16,
}

#[derive(Clone)]
pub enum Config {
    WarGame,
    Band {
        seed: Seed,
        names: Vec<AvatarName>,
    },
    BuzzerPlayer {
        names: Vec<AvatarName>,
    },
    BuzzerHost,
    DjStation {
        seed: Seed,
        group: u8,
        tile_x: i16,
        tile_y: i16,
    },
    NCDanceFloor {
        seed: Seed,
        tiles: Vec<FloorTile>,
    },
    Nightclub {
        seed: Seed,
        portals: Vec<i16>,
    },
    DancePlatform {
        group: u8,
    },
}

#[derive(Clone)]
pub enum VmInput {
    WarNextRound,
    WarNextGame,
    BandAnimationsFinished,
    BuzzerPlayerSync,
    BuzzerHostJudgmentFinished,
    BuzzerHostDeclareWinner,
    FloorDiscover {
        tiles: Vec<FloorTile>,
    },
    FloorAnimation {
        animation: u8,
        color: i16,
        direction: i16,
    },
    FloorRatings {
        ratings: [i16; 4],
    },
    NightclubRoundStart {
        dancers: Vec<i16>,
    },
    NightclubRoundEnd,
    /// Snapshot taken from the actual native avatar queue. A change of UID
    /// completes the previous observed interaction on the next accepted tick.
    DanceQueueObservation {
        avatar_id: u32,
        active_uid: Option<u16>,
        interaction: u16,
        on_this_platform: bool,
        queue_active: bool,
    },
}
impl VmInput {
    /// Original Simantics callback ID, when the source has one. Queue
    /// observations are native VM observations and are not fabricated events.
    pub fn source_code(&self) -> Option<i16> {
        Some(match self {
            Self::WarNextRound => 10,
            Self::WarNextGame => 11,
            Self::BandAnimationsFinished => 4,
            Self::BuzzerPlayerSync => 100,
            Self::BuzzerHostJudgmentFinished => 101,
            Self::BuzzerHostDeclareWinner => 102,
            Self::FloorDiscover { .. } => 1,
            Self::FloorAnimation { .. } | Self::NightclubRoundStart { .. } => 2,
            Self::FloorRatings { .. } => 4,
            Self::NightclubRoundEnd => 3,
            Self::DanceQueueObservation { .. } => return None,
        })
    }
}

/// These eight source handlers issue no external-provider operation. Payment
/// and animation requests remain typed controller outputs in their source order.
#[derive(Clone, PartialEq, Eq)]
pub enum Operation {}
#[derive(Clone, PartialEq, Eq)]
pub enum Reply {}
impl Operation {
    pub(crate) fn save(&self, _writer: &mut Writer) {
        match *self {}
    }
    pub(crate) fn restore(_reader: &mut Reader<'_>) -> Result<Self, Error> {
        Err(Error::InvalidCheckpoint)
    }
}
impl Reply {
    pub(crate) fn save(&self, _writer: &mut Writer) {
        match *self {}
    }
    pub(crate) fn restore(_reader: &mut Reader<'_>) -> Result<Self, Error> {
        Err(Error::InvalidCheckpoint)
    }
}

#[derive(Clone)]
enum Inner {
    War(war::War),
    Band(band::Band),
    Player(buzzer_player::Player),
    Host(buzzer_host::Host),
    Dj(club::Dj),
    Floor(floor::Floor),
    Controller(club::Controller),
    Platform(club::Platform),
}
#[derive(Clone)]
pub(crate) struct State {
    inner: Inner,
}

macro_rules! redacted {
    ($($name:ident),+ $(,)?) => { $(impl std::fmt::Debug for $name { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(concat!(stringify!($name), "([REDACTED])")) } })+ };
}
redacted!(Seed, AvatarName, Config, VmInput, Operation, Reply, State);

impl State {
    pub(crate) fn new(config: Config) -> Result<Self, Error> {
        let inner = match config {
            Config::WarGame => Inner::War(war::War::new()),
            Config::Band { seed, names } => Inner::Band(band::Band::new(seed, names)?),
            Config::BuzzerPlayer { names } => Inner::Player(buzzer_player::Player::new(names)?),
            Config::BuzzerHost => Inner::Host(buzzer_host::Host::new()),
            Config::DjStation {
                seed,
                group,
                tile_x,
                tile_y,
            } => Inner::Dj(club::Dj::new(seed, group, tile_x, tile_y)?),
            Config::NCDanceFloor { seed, tiles } => Inner::Floor(floor::Floor::new(seed, tiles)?),
            Config::Nightclub { seed, portals } => {
                Inner::Controller(club::Controller::new(seed, portals)?)
            }
            Config::DancePlatform { group } => Inner::Platform(club::Platform::new(group)?),
        };
        Ok(Self { inner })
    }
    pub(crate) fn plugin(&self) -> PluginId {
        match self.inner {
            Inner::War(_) => WAR_GAME,
            Inner::Band(_) => BAND,
            Inner::Player(_) => BUZZER_PLAYER,
            Inner::Host(_) => BUZZER_HOST,
            Inner::Dj(_) => DJ_STATION,
            Inner::Floor(_) => NC_FLOOR,
            Inner::Controller(_) => NIGHTCLUB,
            Inner::Platform(_) => DANCE_PLATFORM,
        }
    }
    pub(crate) fn start(&mut self, _roster: &Roster, _tick: u64) -> Result<Actions, Error> {
        Ok(match &self.inner {
            Inner::Host(s) => s.start(),
            Inner::Dj(s) => s.start(),
            Inner::Floor(s) => s.start(),
            Inner::Controller(s) => s.start(),
            Inner::Platform(s) => s.start(),
            _ => Actions::default(),
        })
    }
    pub(crate) fn join(
        &mut self,
        member: Member,
        roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        match &mut self.inner {
            Inner::War(s) => s.join(member, roster),
            Inner::Band(s) => s.join(member, roster),
            Inner::Player(s) => s.join(member, roster),
            Inner::Host(s) => s.join(member, roster),
            Inner::Dj(s) => {
                if !util::valid_single(roster) {
                    return Err(Error::InvalidPluginInput);
                }
                Ok(s.view(member))
            }
            Inner::Platform(s) => {
                if !util::valid_single(roster) {
                    return Err(Error::InvalidPluginInput);
                }
                Ok(s.view(member))
            }
            Inner::Floor(_) => {
                if !util::valid_single(roster) {
                    return Err(Error::InvalidPluginInput);
                }
                let mut a = Actions::default();
                a.text(Target::Member(member.seat), "dance_show", String::new());
                Ok(a)
            }
            Inner::Controller(_) => Err(Error::NotAuthorized),
        }
    }
    pub(crate) fn allows(&self, event: &str, binary: bool) -> bool {
        match &self.inner {
            Inner::War(_) => {
                if binary {
                    event == "WarGame_Piece_Selection"
                } else {
                    event == "WarGame_Close_UI"
                }
            }
            Inner::Band(_) => binary && matches!(event, "Band_Note" | "Band_Decision"),
            Inner::Player(_) => binary && event == "Buzzer_Player_Buzzed",
            Inner::Host(_) => {
                binary
                    && matches!(
                        event,
                        "Buzzer_Host_FindNewPlayer"
                            | "Buzzer_Host_MovePlayerRight"
                            | "Buzzer_Host_MovePlayerLeft"
                            | "Buzzer_Host_ToggleEnablePlayer"
                            | "Buzzer_Host_PlayerCorrect"
                            | "Buzzer_Host_PlayerIncorrect"
                            | "Buzzer_Host_A_ToggleMaster"
                            | "Buzzer_Host_A_DeclareWinner"
                            | "Buzzer_Host_A_Deduct"
                            | "Buzzer_Host_A_Disable"
                            | "Buzzer_Host_A_Enable"
                            | "Buzzer_Host_A_BuzzerTime"
                            | "Buzzer_Host_A_AnswerTime"
                            | "Buzzer_Host_A_PlayerScore0"
                            | "Buzzer_Host_A_PlayerScore1"
                            | "Buzzer_Host_A_PlayerScore2"
                            | "Buzzer_Host_A_PlayerScore3"
                            | "Buzzer_Host_A_GlobalScore"
                            | "Buzzer_Host_Request_Roster"
                    )
            }
            Inner::Dj(_) | Inner::Platform(_) => {
                !binary && matches!(event, "close" | "press_button")
            }
            _ => false,
        }
    }
    pub(crate) fn message(
        &mut self,
        member: Member,
        event: &str,
        payload: &[u8],
        roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        match &mut self.inner {
            Inner::War(s) => s.message(member, event, payload, roster),
            Inner::Band(s) => s.message(member, event, payload, roster),
            Inner::Player(s) => s.message(event),
            Inner::Host(s) => s.message(event, payload),
            Inner::Dj(s) => s.message(member, event, payload),
            Inner::Platform(s) => s.message(member, event, payload),
            _ => Err(Error::EventNotAllowed),
        }
    }
    pub(crate) fn tick(&mut self, roster: &Roster, _tick: u64) -> Result<Actions, Error> {
        match &mut self.inner {
            Inner::War(s) => s.tick(roster),
            Inner::Band(s) => s.tick(roster),
            Inner::Host(s) => Ok(s.tick()),
            Inner::Dj(s) => s.tick(),
            Inner::Floor(s) => s.tick(),
            Inner::Controller(s) => s.tick(),
            Inner::Platform(s) => s.tick(roster),
            Inner::Player(_) => Ok(Actions::default()),
        }
    }
    pub(crate) fn vm_event(
        &mut self,
        input: &VmInput,
        roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        match &mut self.inner {
            Inner::War(s) => s.vm_event(input, roster),
            Inner::Band(s) => s.vm_event(input, roster),
            Inner::Player(s) => s.vm_event(input),
            Inner::Host(s) => s.vm_event(input),
            Inner::Floor(s) => s.vm_event(input),
            Inner::Controller(s) => s.vm_event(input),
            Inner::Platform(s) => s.vm_event(input, roster),
            Inner::Dj(_) => Err(Error::InvalidPluginInput),
        }
    }
    pub(crate) fn reply(
        &mut self,
        _callback: u64,
        reply: &Reply,
        _roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        match *reply {}
    }
    pub(crate) fn peer(
        &mut self,
        source: &PeerSource,
        signal: &Signal,
        roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        match &mut self.inner {
            Inner::Player(s) => s.peer(source, signal, roster),
            Inner::Host(s) => s.peer(source, signal, roster),
            Inner::Dj(s) => s.peer(source, signal),
            Inner::Floor(s) => s.peer(source, signal),
            Inner::Controller(s) => s.peer(source, signal),
            Inner::Platform(s) => s.peer(source, signal),
            _ => Ok(Actions::default()),
        }
    }
    pub(crate) fn leave(
        &mut self,
        _member: Member,
        roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        Ok(match &mut self.inner {
            Inner::War(s) => s.leave(),
            Inner::Band(s) => s.leave(roster),
            Inner::Player(s) => s.leave(),
            Inner::Host(s) => s.leave(),
            Inner::Platform(s) => s.leave(),
            Inner::Dj(_) => {
                let mut a = Actions::default();
                a.close(Target::Controller);
                a
            }
            _ => Actions::default(),
        })
    }
    pub(crate) fn rebind(
        &mut self,
        member: Member,
        roster: &Roster,
        _tick: u64,
    ) -> Result<Actions, Error> {
        Ok(match &self.inner {
            Inner::War(s) => s.rebind(member, roster),
            Inner::Band(s) => s.rebind(member, roster),
            Inner::Player(s) => s.rebind(member),
            Inner::Host(s) => s.rebind(member),
            Inner::Dj(s) => s.view(member),
            Inner::Platform(s) => s.view(member),
            Inner::Floor(_) => {
                let mut a = Actions::default();
                a.text(Target::Member(member.seat), "dance_show", String::new());
                a
            }
            Inner::Controller(_) => return Err(Error::NotAuthorized),
        })
    }
    pub(crate) fn shutdown(&mut self, _roster: &Roster, _tick: u64) -> Result<Actions, Error> {
        let mut a = match &mut self.inner {
            Inner::Player(s) => s.leave(),
            Inner::Host(s) => s.leave(),
            Inner::Controller(s) => s.shutdown(),
            Inner::Floor(s) => s.shutdown(),
            _ => Actions::default(),
        };
        a.close(Target::All);
        Ok(a)
    }
    pub(crate) fn can_close(&self) -> bool {
        true
    }
    pub(crate) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        Vec::new()
    }
    /// Retained peer dependencies whose native bindings must be restored before
    /// the host permits source state to advance.
    pub(crate) fn required_peers(&self) -> Vec<InstanceId> {
        match &self.inner {
            Inner::War(_) | Inner::Band(_) => Vec::new(),
            Inner::Player(s) => s.required_peers(),
            Inner::Host(s) => s.required_peers(),
            Inner::Dj(s) => s.required_peers(),
            Inner::Floor(s) => s.required_peers(),
            Inner::Controller(s) => s.required_peers(),
            Inner::Platform(s) => s.required_peers(),
        }
    }
    pub(crate) fn validate_peers(
        &self,
        own_group: InstanceId,
        cluster: u64,
        peers: &[PeerGroup],
    ) -> bool {
        match &self.inner {
            Inner::Player(s) => s.validate_peers(cluster, peers),
            Inner::Host(s) => s.validate_peers(own_group, cluster, peers),
            Inner::Dj(s) => s.validate_peers(cluster, peers),
            Inner::Floor(s) => s.validate_peers(cluster, peers),
            Inner::Controller(s) => s.validate_peers(cluster, peers),
            Inner::Platform(s) => s.validate_peers(cluster, peers),
            Inner::War(_) | Inner::Band(_) => true,
        }
    }
    pub(crate) fn validate(&self, roster: &Roster) -> bool {
        match &self.inner {
            Inner::War(s) => s.validate(roster),
            Inner::Band(s) => s.validate(roster),
            Inner::Player(s) => s.validate(roster),
            Inner::Host(s) => s.validate(roster),
            Inner::Dj(s) => s.validate(roster),
            Inner::Floor(s) => s.validate(roster),
            Inner::Controller(s) => s.validate(roster),
            Inner::Platform(s) => s.validate(roster),
        }
    }
    pub(crate) fn save(&self, w: &mut Writer) {
        w.u16(1);
        match &self.inner {
            Inner::War(s) => s.save(w),
            Inner::Band(s) => s.save(w),
            Inner::Player(s) => s.save(w),
            Inner::Host(s) => s.save(w),
            Inner::Dj(s) => s.save(w),
            Inner::Floor(s) => s.save(w),
            Inner::Controller(s) => s.save(w),
            Inner::Platform(s) => s.save(w),
        }
    }
    pub(crate) fn restore(plugin: PluginId, r: &mut Reader<'_>) -> Result<Self, Error> {
        if r.u16()? != 1 {
            return Err(Error::UnsupportedPluginSchema);
        }
        let inner = match plugin {
            WAR_GAME => Inner::War(war::War::restore(r)?),
            BAND => Inner::Band(band::Band::restore(r)?),
            BUZZER_PLAYER => Inner::Player(buzzer_player::Player::restore(r)?),
            BUZZER_HOST => Inner::Host(buzzer_host::Host::restore(r)?),
            DJ_STATION => Inner::Dj(club::Dj::restore(r)?),
            NC_FLOOR => Inner::Floor(floor::Floor::restore(r)?),
            NIGHTCLUB => Inner::Controller(club::Controller::restore(r)?),
            DANCE_PLATFORM => Inner::Platform(club::Platform::restore(r)?),
            _ => return Err(Error::WrongPlugin),
        };
        Ok(Self { inner })
    }
}
