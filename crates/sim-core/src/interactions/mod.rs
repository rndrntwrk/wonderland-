// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
#![forbid(unsafe_code)]
//! Source-backed interaction foundation, independent of a VM, renderer, or network.
//!
//! Provider-local keys below must be translated by the integrating simulation;
//! they are not shared W00 contract identifiers or a wire protocol.

pub mod adapters;
pub mod offers;
pub mod query;
pub mod queue;

pub use offers::*;
pub use query::*;
pub use queue::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LegacyMode {
    Tso,
    Ts1,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct EntityKey {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EntityVersion {
    pub key: EntityKey,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrincipalKey(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    LimitExceeded(&'static str),
    InvalidSnapshot(&'static str),
    StaleWorldRevision { expected: u64, actual: u64 },
    StaleEntity(EntityKey),
    Unauthorized,
    Unavailable,
    VariantUnavailable,
    Replay { sequence: u64, last: u64 },
    StaleQueueRevision { expected: u64, actual: u64 },
    QueueFull,
    UserQueueFull,
    CounterExhausted,
    MissingAction(u64),
    NoActiveAction,
    RuntimeFailure(&'static str),
    Unsupported(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "interaction error: {self:?}")
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InteractionLimits {
    pub max_definitions: usize,
    pub max_offers: usize,
    pub max_label_bytes: usize,
    pub max_advertisements: usize,
    pub max_provider_state_bytes: usize,
    pub max_check_steps: u64,
    pub max_queue_entries: usize,
    pub max_user_queue_entries: usize,
}

impl Default for InteractionLimits {
    fn default() -> Self {
        Self {
            max_definitions: 4096,
            max_offers: 4096,
            max_label_bytes: 2048,
            max_advertisements: 256,
            max_provider_state_bytes: 1024 * 1024,
            max_check_steps: 500_000,
            max_queue_entries: 128,
            // VMThread.MAX_USER_ACTIONS. This counts all entries, as the source does.
            max_user_queue_entries: 20,
        }
    }
}
