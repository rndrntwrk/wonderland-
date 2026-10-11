// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Native-only, private plugin-data provider boundary. This module supplies no
//! database, encryption, authentication service or durable provider. The host
//! checkpoints an immutable intent before this boundary can dispatch its write.

use crate::{ActorId, HostIdentity, HostScopeId, InstanceId, PluginId};
use std::fmt;

/// The persistent object identity comes from the authoritative VM, never a UI
/// frame. A host scope and plugin namespace are part of every storage key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PluginDataKey {
    pub scope: HostScopeId,
    pub plugin: PluginId,
    pub persistent_object: u32,
}

/// Immutable request identity. `origin_epoch` is preserved on restore/retry;
/// the provider separately fences the *current* host identity on each call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PluginWriteId {
    pub key: PluginDataKey,
    pub origin_epoch: u64,
    pub instance: InstanceId,
    pub operation: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PluginDataWrite {
    pub(crate) id: PluginWriteId,
    pub(crate) actor: ActorId,
    pub(crate) expected_revision: u64,
    pub(crate) bytes: Vec<u8>,
}
impl PluginDataWrite {
    pub fn id(&self) -> PluginWriteId {
        self.id
    }
    pub fn key(&self) -> PluginDataKey {
        self.id.key
    }
    pub fn actor(&self) -> ActorId {
        self.actor
    }
    pub fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
    /// Private provider access only; never include this in VM snapshots/logs.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl fmt::Debug for PluginDataWrite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PluginDataWrite([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginDataRead {
    /// Zero denotes a missing key. Existing records use a nonzero revision.
    pub revision: u64,
    pub exists: bool,
    pub bytes_written: usize,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistenceFailure {
    /// Outcome may be unknown; retry must retain the same immutable identity.
    Retryable,
    /// Provider guarantees no write was applied; a trusted adapter can abort.
    Denied,
    Corrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginWriteDecision {
    /// The provider atomically advanced expected_revision by exactly one.
    Applied { revision: u64 },
    /// Provider guarantees this operation was not applied. No overwrite occurs.
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PluginWriteReceipt {
    pub id: PluginWriteId,
    pub decision: PluginWriteDecision,
}

/// Every call must authorize/fence `host`; writes must also authorize the actor,
/// object and plugin. Before applying any write, atomically journal/deduplicate
/// the complete request by id and compare its expected revision. Repeating an
/// identical id returns its original terminal receipt, even after a response was
/// lost or a newer host epoch took over. Reusing an id with different bytes,
/// actor or expected revision must never apply another write. The implementation
/// must retain this journal across restarts and protect all bytes privately.
///
/// A successful bounded load represents one consistent record. Return
/// `complete: false` for data that does not fit the supplied destination. The
/// host rejects invalid counts and never allocates a provider-specified length.
pub trait PluginDataProvider {
    fn load(
        &mut self,
        host: HostIdentity,
        key: PluginDataKey,
        destination: &mut [u8],
    ) -> Result<PluginDataRead, PersistenceFailure>;

    fn write(
        &mut self,
        host: HostIdentity,
        request: &PluginDataWrite,
    ) -> Result<PluginWriteReceipt, PersistenceFailure>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersistenceProgress {
    pub loaded: usize,
    pub applied: usize,
    pub retry_pending: usize,
    pub conflicted: usize,
    pub waiting_for_checkpoint: usize,
    pub reconciled: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingStatus {
    Loading,
    Ready,
    Reconcile,
}

#[derive(Clone)]
pub(crate) struct Binding {
    pub(crate) key: PluginDataKey,
    pub(crate) revision: u64,
    pub(crate) exists: bool,
    pub(crate) bytes: Vec<u8>,
    pub(crate) status: BindingStatus,
}
impl Binding {
    pub(crate) fn loading(key: PluginDataKey) -> Self {
        Self {
            key,
            revision: 0,
            exists: false,
            bytes: vec![],
            status: BindingStatus::Loading,
        }
    }
}

#[derive(Clone)]
pub(crate) struct WriteRecord {
    pub(crate) request: PluginDataWrite,
    /// True only after a successful private checkpoint containing this intent.
    pub(crate) prepared: bool,
    pub(crate) conflicted: bool,
}
