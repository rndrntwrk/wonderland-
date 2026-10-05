// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Prepared private operation journal for source-backed service/casino handlers.
//! Providers must fence the current host and atomically retain immutable request
//! and reply identities across retries, including read snapshots and refusals.

use crate::{
    Error, HostIdentity, HostScopeId, InstanceId, PluginId,
    plugins::{
        ProviderOperation, ProviderReply,
        codec::{Reader, Writer},
        common::MAX_OPERATION_BYTES,
    },
};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NativeOperationId {
    pub scope: HostScopeId,
    pub origin_epoch: u64,
    pub group: InstanceId,
    pub operation: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub struct NativeProviderRequest {
    pub(crate) id: NativeOperationId,
    pub(crate) plugin: PluginId,
    pub(crate) object: u32,
    pub(crate) callback: u64,
    pub(crate) operation: ProviderOperation,
}
impl NativeProviderRequest {
    pub fn id(&self) -> NativeOperationId {
        self.id
    }
    pub fn plugin(&self) -> PluginId {
        self.plugin
    }
    pub fn object(&self) -> u32 {
        self.object
    }
    pub fn operation(&self) -> &ProviderOperation {
        &self.operation
    }
    /// Stable private bytes for atomic provider deduplication; these must never
    /// be exposed in a VM projection, public trace, UI channel or browser build.
    pub fn write_private(&self, destination: &mut [u8]) -> Result<usize, Error> {
        let mut w = Writer::default();
        w.u64(self.id.scope.0);
        w.u64(self.id.origin_epoch);
        w.u64(self.id.group.0);
        w.u64(self.id.operation);
        w.u32(self.plugin.0);
        w.u32(self.object);
        w.u64(self.callback);
        self.operation.save(&mut w);
        if w.0.len() > MAX_OPERATION_BYTES + 64 || w.0.len() > destination.len() {
            return Err(Error::PersistenceTooLarge);
        }
        destination[..w.0.len()].copy_from_slice(&w.0);
        Ok(w.0.len())
    }
}
impl fmt::Debug for NativeProviderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeProviderRequest([REDACTED])")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeProviderReceipt {
    pub host: HostIdentity,
    pub id: NativeOperationId,
    pub bytes_written: usize,
    pub complete: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeProviderFailure {
    Retryable,
    Denied,
    Corrupt,
}

/// Only a trusted native provider implements this trait. Authorize every account,
/// ownership, catalog item, lot/category and private storage namespace separately.
/// Atomically deduplicate the *whole* request and retain its exact terminal reply;
/// a retry can follow a committed effect with a lost response or a new host epoch.
/// Return at most destination.len() private encoded ProviderReply bytes. Financial
/// or inventory refusal belongs in that terminal typed reply; `Denied` here means
/// caller/host authority failure and leaves the operation pending for recovery.
/// This crate supplies no production database or funds/inventory implementation.
pub trait NativeProvider {
    fn execute(
        &mut self,
        host: HostIdentity,
        request: &NativeProviderRequest,
        destination: &mut [u8],
    ) -> Result<NativeProviderReceipt, NativeProviderFailure>;
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeProviderProgress {
    pub completed: usize,
    pub retry_pending: usize,
    pub waiting_for_checkpoint: usize,
}
#[derive(Clone)]
pub(crate) struct NativeOperationRecord {
    pub(crate) request: NativeProviderRequest,
    pub(crate) prepared: bool,
}
pub(crate) fn decode_reply(bytes: &[u8]) -> Result<ProviderReply, Error> {
    if bytes.len() > MAX_OPERATION_BYTES {
        return Err(Error::PersistenceTooLarge);
    }
    let mut r = Reader::new(bytes);
    let reply = ProviderReply::restore(&mut r)?;
    r.finish()?;
    Ok(reply)
}
