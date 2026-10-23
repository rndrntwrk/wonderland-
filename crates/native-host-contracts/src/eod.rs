//! Private Timer checkpoint and UI transport contracts. These never grant player
//! authority and must not be embedded in public tick/snapshot packets.
use super::{CursorV1, DecimalId, HostBindingV1};
use serde::{Deserialize, Serialize};

pub const EOD_PRIVATE_CHECKPOINT_LIMIT: usize = 192 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEodCandidateV1 {
    pub schema_version: u32,
    pub revision_before: DecimalId,
    pub revision_after: DecimalId,
    /// None only for an uninitialized private stream. Absence does not certify
    /// that a historical simulation had no EOD association.
    pub predecessor_sha256: Option<String>,
    pub checkpoint_sha256: String,
    pub checkpoint_hex: String,
}
impl NativeEodCandidateV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.revision_before.0.checked_add(1) != Some(self.revision_after.0)
            || (self.revision_before.0 == 0) != self.predecessor_sha256.is_none()
            || self
                .predecessor_sha256
                .as_deref()
                .is_some_and(|x| !digest(x))
            || !digest(&self.checkpoint_sha256)
            || self.checkpoint_hex.is_empty()
            || self.checkpoint_hex.len() > EOD_PRIVATE_CHECKPOINT_LIMIT * 2
            || !self.checkpoint_hex.len().is_multiple_of(2)
            || !self.checkpoint_hex.bytes().all(hex_digit)
        {
            return Err("invalid private EOD candidate");
        }
        Ok(())
    }
}

/// Coordinator's latest atomic pairing. Bindings may advance only through the
/// existing controlled recovery transaction, never a browser restore payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEodCheckpointV1 {
    pub schema_version: u32,
    pub binding: HostBindingV1,
    pub cursor: CursorV1,
    pub revision: DecimalId,
    pub checkpoint_sha256: String,
    pub checkpoint_hex: String,
}
impl NativeEodCheckpointV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.revision.0 == 0
            || !digest(&self.checkpoint_sha256)
            || self.checkpoint_hex.is_empty()
            || self.checkpoint_hex.len() > EOD_PRIVATE_CHECKPOINT_LIMIT * 2
            || !self.checkpoint_hex.len().is_multiple_of(2)
            || !self.checkpoint_hex.bytes().all(hex_digit)
        {
            return Err("invalid private EOD checkpoint");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEodUiV1 {
    pub schema_version: u32,
    pub source_session_id: DecimalId,
    pub sequence: DecimalId,
    pub event: String,
    pub binary: bool,
    pub payload_hex: String,
}
impl NativeEodUiV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.source_session_id.0 == 0
            || self.sequence.0 == 0
            || self.event.is_empty()
            || self.event.len() > 64
            || !self.event.is_ascii()
            || self.payload_hex.len() > 8192
            || !self.payload_hex.len().is_multiple_of(2)
            || !self.payload_hex.bytes().all(hex_digit)
        {
            return Err("invalid private Timer UI message");
        }
        Ok(())
    }
}

fn hex_digit(x: u8) -> bool {
    x.is_ascii_digit() || (b'a'..=b'f').contains(&x)
}
fn digest(x: &str) -> bool {
    x.len() == 64 && x.bytes().all(hex_digit)
}
