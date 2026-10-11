// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Authoritative durable-effect request boundary. No database, funds transfer,
//! inventory mutation or production provider exists in this crate. The provider
//! must authorize every request and atomically deduplicate by its stable key.
//! Timer does not emit these requests. Effectful plugins remain unsupported.
//!
//! This bounded outbox retains retry identities only for its in-memory lifetime.
//! Future effectful plugin integration MUST persist keys/intents/receipts at a
//! private checkpoint barrier before it can be enabled. Recreating this outbox
//! after a crash does not by itself provide durable exactly-once behavior.

use crate::{ActorId, Error, HostScopeId, InstanceId, PluginId, registry};
use std::{collections::BTreeMap, fmt};

/// Stable across connection and host epochs; operation allocation is owned by
/// the authoritative durable caller, never by an incoming client message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EffectKey {
    pub scope: HostScopeId,
    pub plugin: PluginId,
    pub instance: InstanceId,
    pub operation: u64,
}

/// Borrowed input ensures limits are checked before this crate copies bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EffectOperation<'a> {
    WritePluginData(&'a [u8]),
    ReserveFunds {
        account: ActorId,
        amount: u64,
    },
    SettleReservation {
        reservation: EffectKey,
        recipient: ActorId,
        amount: u64,
    },
    RefundReservation {
        reservation: EffectKey,
    },
}

impl fmt::Debug for EffectOperation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EffectOperation([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq)]
enum OwnedOperation {
    WritePluginData(Vec<u8>),
    ReserveFunds {
        account: ActorId,
        amount: u64,
    },
    SettleReservation {
        reservation: EffectKey,
        recipient: ActorId,
        amount: u64,
    },
    RefundReservation {
        reservation: EffectKey,
    },
}

#[derive(Clone, PartialEq, Eq)]
pub struct DurableEffectRequest {
    key: EffectKey,
    operation: OwnedOperation,
}

impl DurableEffectRequest {
    pub fn key(&self) -> EffectKey {
        self.key
    }
    pub fn operation(&self) -> EffectOperation<'_> {
        match &self.operation {
            OwnedOperation::WritePluginData(bytes) => EffectOperation::WritePluginData(bytes),
            OwnedOperation::ReserveFunds { account, amount } => EffectOperation::ReserveFunds {
                account: *account,
                amount: *amount,
            },
            OwnedOperation::SettleReservation {
                reservation,
                recipient,
                amount,
            } => EffectOperation::SettleReservation {
                reservation: *reservation,
                recipient: *recipient,
                amount: *amount,
            },
            OwnedOperation::RefundReservation { reservation } => {
                EffectOperation::RefundReservation {
                    reservation: *reservation,
                }
            }
        }
    }
}

impl fmt::Debug for DurableEffectRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DurableEffectRequest([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderDecision {
    Applied,
    Declined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderReceipt {
    pub key: EffectKey,
    pub decision: ProviderDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderFailure {
    Retryable,
}

/// A retry may follow an already committed effect whose response was lost.
/// Implementations MUST persist the immutable request identity and decision in
/// the same transaction as their effect, validate key/body reuse, and independently
/// authorize accounts, reservations, ownership and refunds. Client mirrors never
/// receive this trait or invoke a provider. No implementation is supplied here.
pub trait DurableEffectProvider {
    fn request(
        &mut self,
        request: &DurableEffectRequest,
    ) -> Result<ProviderReceipt, ProviderFailure>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectLimits {
    pub max_records: usize,
    pub max_payload_bytes: usize,
    pub max_total_payload_bytes: usize,
}
impl Default for EffectLimits {
    fn default() -> Self {
        Self {
            max_records: 256,
            max_payload_bytes: 4096,
            max_total_payload_bytes: 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueResult {
    Queued,
    AlreadyQueued,
    AlreadyFinal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectDispatch {
    RetryPending,
    Complete(ProviderReceipt),
}

struct Record {
    request: DurableEffectRequest,
    receipt: Option<ProviderReceipt>,
}

pub struct EffectOutbox {
    limits: EffectLimits,
    records: BTreeMap<EffectKey, Record>,
    total_payload_bytes: usize,
}

impl fmt::Debug for EffectOutbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EffectOutbox([REDACTED])")
    }
}

impl EffectOutbox {
    pub fn new(limits: EffectLimits) -> Result<Self, Error> {
        if !(1..=4096).contains(&limits.max_records)
            || !(1..=1024 * 1024).contains(&limits.max_payload_bytes)
            || !(1..=16 * 1024 * 1024).contains(&limits.max_total_payload_bytes)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            limits,
            records: BTreeMap::new(),
            total_payload_bytes: 0,
        })
    }

    pub fn queue(
        &mut self,
        key: EffectKey,
        operation: EffectOperation<'_>,
    ) -> Result<QueueResult, Error> {
        validate_key(key)?;
        let payload_bytes = match operation {
            EffectOperation::WritePluginData(bytes) => bytes.len(),
            EffectOperation::ReserveFunds { account, amount } => {
                if account.0 == 0 || amount == 0 {
                    return Err(Error::InvalidMessage);
                }
                0
            }
            EffectOperation::SettleReservation {
                reservation,
                recipient,
                amount,
            } => {
                validate_reservation(key, reservation)?;
                if recipient.0 == 0 || amount == 0 {
                    return Err(Error::InvalidMessage);
                }
                0
            }
            EffectOperation::RefundReservation { reservation } => {
                validate_reservation(key, reservation)?;
                0
            }
        };
        if payload_bytes > self.limits.max_payload_bytes {
            return Err(Error::MessageTooLarge);
        }
        if let Some(existing) = self.records.get(&key) {
            if existing.request.operation() != operation {
                return Err(Error::EffectConflict);
            }
            return Ok(if existing.receipt.is_some() {
                QueueResult::AlreadyFinal
            } else {
                QueueResult::AlreadyQueued
            });
        }
        let total = self
            .total_payload_bytes
            .checked_add(payload_bytes)
            .ok_or(Error::EffectLimit)?;
        if self.records.len() >= self.limits.max_records
            || total > self.limits.max_total_payload_bytes
        {
            return Err(Error::EffectLimit);
        }
        let operation = match operation {
            EffectOperation::WritePluginData(bytes) => {
                OwnedOperation::WritePluginData(bytes.to_vec())
            }
            EffectOperation::ReserveFunds { account, amount } => {
                OwnedOperation::ReserveFunds { account, amount }
            }
            EffectOperation::SettleReservation {
                reservation,
                recipient,
                amount,
            } => OwnedOperation::SettleReservation {
                reservation,
                recipient,
                amount,
            },
            EffectOperation::RefundReservation { reservation } => {
                OwnedOperation::RefundReservation { reservation }
            }
        };
        self.records.insert(
            key,
            Record {
                request: DurableEffectRequest { key, operation },
                receipt: None,
            },
        );
        self.total_payload_bytes = total;
        Ok(QueueResult::Queued)
    }

    pub fn dispatch(
        &mut self,
        key: EffectKey,
        provider: &mut impl DurableEffectProvider,
    ) -> Result<EffectDispatch, Error> {
        let record = self.records.get_mut(&key).ok_or(Error::UnknownEffect)?;
        if let Some(receipt) = record.receipt {
            return Ok(EffectDispatch::Complete(receipt));
        }
        match provider.request(&record.request) {
            Err(ProviderFailure::Retryable) => Ok(EffectDispatch::RetryPending),
            Ok(receipt) => {
                if receipt.key != key {
                    return Err(Error::ProviderReceiptMismatch);
                }
                record.receipt = Some(receipt);
                Ok(EffectDispatch::Complete(receipt))
            }
        }
    }
}

fn validate_key(key: EffectKey) -> Result<(), Error> {
    if key.scope.0 == 0 || key.instance.0 == 0 || key.operation == 0 {
        return Err(Error::InvalidIdentity);
    }
    if registry::lookup(key.plugin).is_none() {
        return Err(Error::UnregisteredPlugin);
    }
    Ok(())
}

fn validate_reservation(key: EffectKey, reservation: EffectKey) -> Result<(), Error> {
    validate_key(reservation)?;
    if key.scope != reservation.scope
        || key.plugin != reservation.plugin
        || key.instance != reservation.instance
        || key == reservation
    {
        return Err(Error::InvalidMessage);
    }
    Ok(())
}
