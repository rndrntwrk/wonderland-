//! Wonderland's headless deterministic simulation.
//!
//! Source behavior is traced to the FreeSO baseline in `docs/swarm-a`.
//! No renderer, browser, network, database, wall clock, or operating-system RNG
//! participates in authoritative state transitions.
#![forbid(unsafe_code)]

pub mod avatars;
pub mod clock;
pub mod effects;
pub mod ids;
pub mod interactions;
pub mod numeric;
pub mod primitives;
pub mod rng;
pub mod runtime;
pub mod runtime_memory;
pub mod scheduler;
pub mod snapshot;
pub mod state;
pub mod vm;
pub mod world;

pub const TICKS_PER_SECOND: u32 = 30;

mod runtime_routes;

pub mod runtime_avatars;
