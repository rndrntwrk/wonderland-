//! Generation and pending-operation admission.
use crate::{ErrorCode, ServiceError, ServiceResult};
use std::collections::{BTreeMap, BTreeSet};

pub struct OperationLedger {
    epoch: u64,
    pending: BTreeMap<String, String>,
    seen: BTreeSet<String>,
}
impl OperationLedger {
    pub fn new(epoch: u64) -> Self {
        Self {
            epoch,
            pending: BTreeMap::new(),
            seen: BTreeSet::new(),
        }
    }
    pub fn begin(&mut self, epoch: u64, operation_id: &str, family: &str) -> ServiceResult<()> {
        if epoch != self.epoch {
            return Err(ServiceError::new(
                ErrorCode::StaleEpoch,
                "The operation belongs to an expired session",
            ));
        }
        if operation_id.is_empty()
            || operation_id.len() > 96
            || !operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
            || self.seen.contains(operation_id)
        {
            return Err(ServiceError::new(
                ErrorCode::InvalidRequest,
                "The operation identifier is invalid or was already submitted",
            ));
        }
        if self.pending.contains_key(family) {
            return Err(ServiceError::new(
                ErrorCode::OperationPending,
                "A request with this original response type is already pending",
            ));
        }
        if self.seen.len() >= 16_384 || self.pending.len() >= 128 {
            return Err(ServiceError::new(
                ErrorCode::ResourceBusy,
                "Session correlation resources are full; wait or reconnect without replaying writes",
            ));
        }
        self.pending.insert(family.into(), operation_id.into());
        self.seen.insert(operation_id.into());
        Ok(())
    }
    pub fn complete(&mut self, epoch: u64, family: &str) -> Option<String> {
        if epoch != self.epoch {
            return None;
        }
        self.pending.remove(family)
    }
    pub fn operation_id(&self, epoch: u64, family: &str) -> Option<&str> {
        if epoch != self.epoch {
            None
        } else {
            self.pending.get(family).map(String::as_str)
        }
    }
    pub fn reset(&mut self, epoch: u64) -> Vec<String> {
        if self.epoch != epoch {
            self.seen.clear();
        }
        self.epoch = epoch;
        std::mem::take(&mut self.pending).into_values().collect()
    }
}
