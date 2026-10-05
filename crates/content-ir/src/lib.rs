//! Deterministic, bounded content identity and source-compatible resolution.
#![forbid(unsafe_code)]

pub mod manifest;
pub mod objects;
pub mod packs;
pub mod patches;
pub mod strings;
pub mod tuning;
pub mod tuning_pack;
pub mod visual;

pub use objects::{resolve_content, ResolveRequest, ResolvedContent};
pub use wonderland_legacy_formats::{Error, ErrorKind, Limits, Result};
