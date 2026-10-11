// SPDX-License-Identifier: MPL-2.0
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ErrorKind {
    Truncated,
    InvalidMagic,
    UnsupportedVersion,
    InvalidData,
    LimitExceeded,
    Overflow,
    Overlap,
    Duplicate,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: usize,
    pub context: String,
}

impl Error {
    pub fn new(kind: ErrorKind, offset: usize, context: impl Into<String>) -> Self {
        Self {
            kind,
            offset,
            context: context.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} at byte {}: {}",
            self.kind, self.offset, self.context
        )
    }
}

impl std::error::Error for Error {}
