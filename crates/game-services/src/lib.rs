//! DOM-free contracts and parsers for the original account and Aries services.
pub mod account;
pub mod directory;
pub mod gateway;
pub mod protocol;
pub mod session;

pub use account::*;
pub use directory::*;
pub use gateway::*;
pub use session::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    AuthenticationFailed,
    AccountLocked,
    Unauthorized,
    InvalidRequest,
    InvalidResponse,
    ResponseTooLarge,
    Transport,
    NotConfigured,
    DestinationRejected,
    StaleEpoch,
    SessionConflict,
    OperationPending,
    Unsupported,
    Disconnected,
    Timeout,
    Rejected,
    ResourceBusy,
}

/// Bounded, safe error text. Upstream response/credential contents are never copied here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceError {
    pub code: ErrorCode,
    pub message: String,
}

impl ServiceError {
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.chars().take(256).collect(),
        }
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ServiceError {}

pub type ServiceResult<T> = Result<T, ServiceError>;
