// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
#![forbid(unsafe_code)]
//! Native EOD foundations. Source translation is not original-runtime verification.

#[cfg(target_family = "wasm")]
compile_error!("authoritative private EOD state is native-only");

mod checkpoint;
pub mod effects;
mod game_host;
mod games;
mod host;
pub mod persistence;
pub mod protocol;
pub mod registry;
mod source_plugins;
mod timer;

pub use checkpoint::{
    CheckpointKind, CheckpointStamp, PrivateCheckpointKey, PrivateCheckpointStore, PrivateRead,
    StoreError,
};
pub use games::{
    GameControllerInput, GameControllerRequest, GameControllerTicket, GameObjectEvent,
    GamePlayerInput, GamePlayerRequest, GameVmInput, MazeRole, PizzaTuning, PrivateSeed,
};
pub use host::{
    ConnectRequest, ConnectionAuthority, HostIdentity, HostLimits, NativeHost,
    PluginConnectRequest, PluginInput, RegisterSource,
};
pub use protocol::{
    ActorId, ClientMessage, ConnectionId, DispatchOutcome, Error, HostScopeId, InstanceAddress,
    InstanceId, InvokerId, PluginId, PrivateUiMessage, PublicVmEvent, SessionTicket, UiBody,
    VmProjection, WirePayload,
};
pub use source_plugins::{DoorInput, DoorMode, SignsInput, SignsMode, SourceObjectEvent};

/// The adapter calls `NativeHost::tick` once per authoritative simulation tick.
pub const SIMULATION_TICKS_PER_SECOND: u32 = 30;

/// The first four invoker temporary registers, supplied by the authoritative VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerRegisters(pub [i16; 4]);

/// Only synchronized object events belong in VM command/snapshot channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimerVmEvent {
    Update,
    ToggleStopwatch,
    SetTime { packed_time: i16 },
    Start,
    Pause,
}

impl TimerVmEvent {
    /// Source `VMEODTimerEvents` code and temp-register arguments.
    pub fn source_event(&self) -> (i16, Vec<i16>) {
        match *self {
            Self::Update => (0, vec![]),
            Self::ToggleStopwatch => (1, vec![]),
            Self::SetTime { packed_time } => (2, vec![packed_time]),
            Self::Start => (3, vec![]),
            Self::Pause => (4, vec![]),
        }
    }
}
