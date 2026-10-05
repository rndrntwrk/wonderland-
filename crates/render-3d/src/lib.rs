#![forbid(unsafe_code)]
//! Pure, disposable presentation geometry. No simulation or service authority.
pub mod camera;
pub mod city;
pub mod environment;
mod legacy_random;
pub mod lot;
pub mod objects;
pub mod reconstruction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput(&'static str),
    BudgetExceeded(&'static str),
    Unsupported(&'static str),
    StaleTransition,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
