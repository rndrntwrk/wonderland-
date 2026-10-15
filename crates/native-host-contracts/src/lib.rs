//! Internal supervisor protocol. These messages are never browser admission.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DecimalId(pub u64);
impl TryFrom<String> for DecimalId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty()
            || value.len() > 20
            || !value.bytes().all(|b| b.is_ascii_digit())
            || (value.len() > 1 && value.starts_with('0'))
        {
            return Err("identity must be canonical decimal text");
        }
        value.parse().map(Self).map_err(|_| "identity outside u64")
    }
}
impl From<DecimalId> for String {
    fn from(value: DecimalId) -> Self {
        value.0.to_string()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CursorV1 {
    pub lot_id: DecimalId,
    pub authority_epoch: DecimalId,
    pub completed_tick: DecimalId,
    pub state_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostBindingV1 {
    pub protocol: u32,
    pub source_pin: String,
    pub binding_sha256: String,
    pub lot_incarnation: DecimalId,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RequestV1 {
    Inspect,
    ActivateRecovery {
        marker: TakeoverMarkerV1,
    },
    Prepare {
        batch_id: DecimalId,
        binding: HostBindingV1,
        predecessor: CursorV1,
        commands_hex: String,
    },
    Resolve {
        batch_id: DecimalId,
        digest: String,
        decision: DecisionV1,
    },
    Lookup {
        batch_id: DecimalId,
    },
}
#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionV1 {
    Committed { cursor: CursorV1 },
    Rejected,
    Unknown,
}

/// Private coordinator-issued recovery record; not a browser credential.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryGrantV1 {
    pub schema_version: u32,
    pub old_binding: HostBindingV1,
    pub old_cursor: CursorV1,
    pub old_checkpoint_sha256: String,
    pub new_binding: HostBindingV1,
    pub new_authority_epoch: DecimalId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeoverMarkerV1 {
    pub schema_version: u32,
    pub grant: RecoveryGrantV1,
    pub new_cursor: CursorV1,
    pub checkpoint_sha256: String,
}

mod control;
pub use control::ControlStateV2;
