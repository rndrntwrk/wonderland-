//! Source-derived avatar state and deterministic behavior providers.
//!
//! Interaction offers, queue ownership, resources and physical slot grants are
//! supplied by adapters. No presentation callback drives a state transition.

pub mod advertisements;
pub mod autonomy;
pub mod events;
pub mod lifecycle;
pub mod motives;
pub mod outfits;
pub mod skills;
pub mod social;
pub mod state;
pub mod timeline;

pub use state::{AvatarState, AvatarTickContext, AvatarTickOutput};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvatarPlatform {
    Tso,
    Ts1,
}
