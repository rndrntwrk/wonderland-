//! C-local, presentation-only audio. Playback never acknowledges simulation work.
#![forbid(unsafe_code)]

pub mod cue;
pub mod mixer;
pub mod pcm;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    Invalid(&'static str),
    Limit(&'static str),
    Truncated(u64),
    Unsupported(&'static str),
    Stale,
    Missing(&'static str),
    Fault(&'static str),
    Io(String),
}
impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for AudioError {}
pub type Result<T> = std::result::Result<T, AudioError>;
pub mod hit;
pub mod runtime;
pub mod codec;
pub mod ambience;
pub mod fsc;
pub mod station;
pub mod system;
pub mod projection;
