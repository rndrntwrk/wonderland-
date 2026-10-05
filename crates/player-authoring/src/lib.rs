//! Original VMNet and dresser/rack authoring boundaries, DOM-free.
#![forbid(unsafe_code)]
pub mod codec;
pub use codec::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoringError {
    Invalid(&'static str),
    Unsupported(u8),
    WrongActor,
    Permission,
    Stale,
    Pending,
    Missing(&'static str),
    EodOwnership,
    Price,
}
impl std::fmt::Display for AuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AuthoringError {}
pub mod source_data;
pub use source_data::*;
pub mod state;
pub use state::*;
pub mod eod;
pub use eod::*;
pub mod build_draft;
pub mod world_projection;
pub use build_draft::*;
