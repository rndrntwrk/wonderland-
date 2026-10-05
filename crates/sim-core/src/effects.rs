//! Pure, durable-operation request/response boundary.
//!
//! The legacy `VMNetAsyncResponseCmd` accepts server responses only, looks up a
//! local object ID, and checks the exact blocking-state type. This contract adds
//! a persisted operation ID, entity generation, bounded typed values, an exact
//! application tick, and separate commit/delivery epochs. It deliberately does
//! not execute transactions, call a network service, or store ledger truth.
//!
//! The adapter must authenticate accepted deliveries and reconcile durable
//! outcomes by operation ID. A new owner may deliver an old committed outcome
//! with its current delivery epoch. Callers must obtain `live_target` from the
//! authoritative entity table, never from the received response itself.
//!
//! Payloads are public VM material only. Opaque bytes require a bounded semantic
//! decoder in the runtime; they do not authorize register writes or private EOD
//! state in a replica snapshot. Validate decoded snapshots before publication.

use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// Absolute count ceilings also apply to limits received in a snapshot.
pub const MAX_PENDING_OPERATIONS: u32 = 4_096;
pub const MAX_TERMINAL_OPERATIONS: u32 = 4_096;
/// A single request or response may carry at most 64 KiB of value data.
pub const MAX_EFFECT_BYTES: u32 = 65_536;

/// Big-endian namespace followed by a monotonically allocated, nonzero nonce.
///
/// The namespace belongs to one persistent lot/operation stream and must not be
/// reassigned to an unrelated stream. Epochs are separate fencing credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OperationId(pub [u8; 16]);

impl OperationId {
    pub fn from_parts(namespace: u64, nonce: u64) -> Self {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&namespace.to_be_bytes());
        bytes[8..].copy_from_slice(&nonce.to_be_bytes());
        Self(bytes)
    }

    pub fn namespace(self) -> u64 {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&self.0[..8]);
        u64::from_be_bytes(bytes)
    }

    pub fn nonce(self) -> u64 {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(&self.0[8..]);
        u64::from_be_bytes(bytes)
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectKind {
    Bool,
    Int32,
    Bytes,
}

/// An intent to be interpreted by the authorized external adapter.
///
/// Encoding an account ID or an amount in a payload grants no financial
/// authority. These values are requests, never restored balances or ownership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectPayload {
    Bool(bool),
    Int32(i32),
    Bytes(Vec<u8>),
}

impl EffectPayload {
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Bool(_) => 1,
            Self::Int32(_) => 4,
            Self::Bytes(bytes) => bytes.len(),
        }
    }
}

/// A typed continuation value. No variant contains unrestricted VM writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectValue {
    Bool(bool),
    Int32(i32),
    Bytes(Vec<u8>),
}

impl EffectValue {
    pub fn kind(&self) -> EffectKind {
        match self {
            Self::Bool(_) => EffectKind::Bool,
            Self::Int32(_) => EffectKind::Int32,
            Self::Bytes(_) => EffectKind::Bytes,
        }
    }

    pub fn byte_len(&self) -> usize {
        match self {
            Self::Bool(_) => 1,
            Self::Int32(_) => 4,
            Self::Bytes(bytes) => bytes.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectLimits {
    pub max_pending: u32,
    pub max_terminal: u32,
    pub max_request_bytes: u32,
    pub max_response_bytes: u32,
}

impl Default for EffectLimits {
    fn default() -> Self {
        Self {
            max_pending: 256,
            max_terminal: 1_024,
            max_request_bytes: 16_384,
            max_response_bytes: 16_384,
        }
    }
}

impl EffectLimits {
    pub fn validate(self) -> Result<(), EffectError> {
        if self.max_pending == 0 || self.max_pending > MAX_PENDING_OPERATIONS {
            return Err(EffectError::InvalidLimits("pending operation capacity"));
        }
        if self.max_terminal == 0 || self.max_terminal > MAX_TERMINAL_OPERATIONS {
            return Err(EffectError::InvalidLimits("terminal operation capacity"));
        }
        if self.max_request_bytes > MAX_EFFECT_BYTES {
            return Err(EffectError::InvalidLimits("request byte capacity"));
        }
        if self.max_response_bytes > MAX_EFFECT_BYTES {
            return Err(EffectError::InvalidLimits("response byte capacity"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectRequest {
    pub operation_id: OperationId,
    pub target: EntityRef,
    pub issued_tick: u64,
    pub issued_epoch: u64,
    pub response_kind: EffectKind,
    pub payload: EffectPayload,
}

/// Retry envelope: `request` remains identical across authority takeovers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectDispatch {
    pub request: EffectRequest,
    pub dispatch_epoch: u64,
}

/// An outcome in an authenticated, ordered, accepted simulation delivery.
///
/// `committed_epoch` identifies when the durable outcome was committed.
/// `delivery_epoch` identifies the current owner delivering that existing
/// outcome. Neither field is a substitute for adapter authentication/fencing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectResolved {
    pub operation_id: OperationId,
    pub target: EntityRef,
    pub apply_tick: u64,
    pub delivery_epoch: u64,
    pub committed_epoch: u64,
    pub value: EffectValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectResolution {
    Applied {
        operation_id: OperationId,
        target: EntityRef,
        value: EffectValue,
    },
    /// Contains no resume value: applying it cannot wake a continuation twice.
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelResult {
    Cancelled,
    AlreadyCancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectError {
    InvalidLimits(&'static str),
    InvalidState(&'static str),
    InvalidTarget(EntityRef),
    PendingLimit {
        limit: u32,
    },
    RequestTooLarge {
        bytes: usize,
        limit: u32,
    },
    ResponseTooLarge {
        bytes: usize,
        limit: u32,
    },
    OperationIdExhausted,
    UnknownOperation(OperationId),
    /// Previously allocated, now outside the bounded terminal cache.
    RetiredOperation(OperationId),
    TargetMismatch {
        expected: EntityRef,
        actual: EntityRef,
    },
    StaleTarget {
        expected: EntityRef,
        live: Option<EntityRef>,
    },
    ResponseTypeMismatch {
        expected: EffectKind,
        actual: EffectKind,
    },
    ApplyTickMismatch {
        scheduled: u64,
        current: u64,
    },
    BeforeIssueTick {
        issued: u64,
        current: u64,
    },
    DeliveryEpochMismatch {
        delivered: u64,
        current: u64,
    },
    StaleEpoch {
        issued: u64,
        current: u64,
    },
    InvalidCommittedEpoch {
        issued: u64,
        committed: u64,
        current: u64,
    },
    ConflictingDuplicate(OperationId),
    OperationCancelled(OperationId),
    AlreadyResolved(OperationId),
}

impl fmt::Display for EffectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "effect boundary: {self:?}")
    }
}

impl std::error::Error for EffectError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RequestMetadata {
    target: EntityRef,
    issued_tick: u64,
    issued_epoch: u64,
    response_kind: EffectKind,
}

impl From<&EffectRequest> for RequestMetadata {
    fn from(request: &EffectRequest) -> Self {
        Self {
            target: request.target,
            issued_tick: request.issued_tick,
            issued_epoch: request.issued_epoch,
            response_kind: request.response_kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum TerminalOutcome {
    Resolved {
        applied_tick: u64,
        delivery_epoch: u64,
        committed_epoch: u64,
        value: EffectValue,
    },
    Cancelled {
        cancelled_tick: u64,
        cancelled_epoch: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TerminalRecord {
    metadata: RequestMetadata,
    outcome: TerminalOutcome,
}

/// Serializable bounded continuation state, with no persistence or I/O handle.
///
/// Methods returning an error leave the book byte-for-byte unchanged. The
/// private high-water nonce is never reduced, including on cancellation and
/// terminal eviction. Restorers must coordinate its snapshot with the accepted
/// command/effect journal; validation cannot prove a snapshot is the newest one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectBook {
    // Keep the two fixed-width identity fields first in the serialized schema.
    namespace: u64,
    last_nonce: u64,
    limits: EffectLimits,
    pending: BTreeMap<OperationId, EffectRequest>,
    terminal: BTreeMap<OperationId, TerminalRecord>,
    terminal_order: VecDeque<OperationId>,
}

impl EffectBook {
    pub fn new(namespace: u64, limits: EffectLimits) -> Result<Self, EffectError> {
        limits.validate()?;
        Ok(Self {
            namespace,
            last_nonce: 0,
            limits,
            pending: BTreeMap::new(),
            terminal: BTreeMap::new(),
            terminal_order: VecDeque::new(),
        })
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// Persisted allocation high-water mark; zero means nothing was issued.
    pub fn last_nonce(&self) -> u64 {
        self.last_nonce
    }

    pub fn limits(&self) -> EffectLimits {
        self.limits
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn terminal_len(&self) -> usize {
        self.terminal.len()
    }

    pub fn pending_requests(&self) -> impl ExactSizeIterator<Item = &EffectRequest> {
        self.pending.values()
    }

    pub fn request(&self, operation_id: OperationId) -> Option<&EffectRequest> {
        self.pending.get(&operation_id)
    }

    /// Records an intent and returns its immutable logical request.
    ///
    /// The caller resolves the live target before issuing. This method validates
    /// structural identity and bounds, and performs no durable action.
    pub fn issue(
        &mut self,
        target: EntityRef,
        issued_tick: u64,
        issued_epoch: u64,
        response_kind: EffectKind,
        payload: EffectPayload,
    ) -> Result<EffectRequest, EffectError> {
        self.validate()?;
        validate_target(target)?;
        self.validate_payload(&payload)?;
        if self.pending.len() >= self.limits.max_pending as usize {
            return Err(EffectError::PendingLimit {
                limit: self.limits.max_pending,
            });
        }
        let nonce = self
            .last_nonce
            .checked_add(1)
            .ok_or(EffectError::OperationIdExhausted)?;
        let operation_id = OperationId::from_parts(self.namespace, nonce);
        let request = EffectRequest {
            operation_id,
            target,
            issued_tick,
            issued_epoch,
            response_kind,
            payload,
        };
        // Every fallible check precedes the first mutation.
        self.pending.insert(operation_id, request.clone());
        self.last_nonce = nonce;
        Ok(request)
    }

    /// Produces a retry/reconciliation envelope without allocating a new ID.
    pub fn redispatch(
        &self,
        operation_id: OperationId,
        current_epoch: u64,
    ) -> Result<EffectDispatch, EffectError> {
        self.validate()?;
        let request = match self.pending.get(&operation_id) {
            Some(request) => request,
            None => return Err(self.closed_or_unknown(operation_id)),
        };
        validate_epoch(request.issued_epoch, current_epoch)?;
        Ok(EffectDispatch {
            request: request.clone(),
            dispatch_epoch: current_epoch,
        })
    }

    /// Bounded, operation-ID-ordered recovery of every pending logical request.
    pub fn redispatch_pending(
        &self,
        current_epoch: u64,
    ) -> Result<Vec<EffectDispatch>, EffectError> {
        self.validate()?;
        for request in self.pending.values() {
            validate_epoch(request.issued_epoch, current_epoch)?;
        }
        Ok(self
            .pending
            .values()
            .map(|request| EffectDispatch {
                request: request.clone(),
                dispatch_epoch: current_epoch,
            })
            .collect())
    }

    /// Validates and closes one pending operation, returning a typed VM value.
    ///
    /// A duplicate compares the durable outcome (including `committed_epoch`),
    /// while delivery tick/epoch may change when the adapter reschedules it.
    /// The new envelope still must match the current accepted tick and epoch.
    /// A cached duplicate needs no live entity because it never resumes one.
    ///
    /// Runtime-specific byte decoding and VM register checks belong in the
    /// caller's cloned/transactional state before it publishes this transition.
    pub fn resolve(
        &mut self,
        response: EffectResolved,
        current_tick: u64,
        current_epoch: u64,
        live_target: Option<EntityRef>,
    ) -> Result<EffectResolution, EffectError> {
        self.validate()?;
        let operation_id = response.operation_id;
        if let Some(request) = self.pending.get(&operation_id) {
            let metadata = RequestMetadata::from(request);
            self.validate_response(metadata, &response, current_tick, current_epoch)?;
            if live_target != Some(metadata.target) {
                return Err(EffectError::StaleTarget {
                    expected: metadata.target,
                    live: live_target,
                });
            }
            let resolution = EffectResolution::Applied {
                operation_id,
                target: metadata.target,
                value: response.value.clone(),
            };
            let record = TerminalRecord {
                metadata,
                outcome: TerminalOutcome::Resolved {
                    applied_tick: response.apply_tick,
                    delivery_epoch: response.delivery_epoch,
                    committed_epoch: response.committed_epoch,
                    value: response.value,
                },
            };
            self.pending.remove(&operation_id);
            self.insert_terminal(operation_id, record);
            return Ok(resolution);
        }
        if let Some(record) = self.terminal.get(&operation_id) {
            self.validate_response(record.metadata, &response, current_tick, current_epoch)?;
            return match &record.outcome {
                TerminalOutcome::Resolved {
                    committed_epoch,
                    value,
                    ..
                } => {
                    if *committed_epoch == response.committed_epoch && *value == response.value {
                        Ok(EffectResolution::Duplicate)
                    } else {
                        Err(EffectError::ConflictingDuplicate(operation_id))
                    }
                }
                TerminalOutcome::Cancelled { .. } => {
                    Err(EffectError::OperationCancelled(operation_id))
                }
            };
        }
        Err(self.unknown(operation_id))
    }

    /// Closes a continuation. This does not undo an already committed external
    /// transaction; the adapter remains responsible for durable reconciliation.
    pub fn cancel(
        &mut self,
        operation_id: OperationId,
        target: EntityRef,
        current_tick: u64,
        current_epoch: u64,
    ) -> Result<CancelResult, EffectError> {
        self.validate()?;
        validate_target(target)?;
        if let Some(request) = self.pending.get(&operation_id) {
            let metadata = RequestMetadata::from(request);
            validate_cancellation(metadata, target, current_tick, current_epoch)?;
            let record = TerminalRecord {
                metadata,
                outcome: TerminalOutcome::Cancelled {
                    cancelled_tick: current_tick,
                    cancelled_epoch: current_epoch,
                },
            };
            self.pending.remove(&operation_id);
            self.insert_terminal(operation_id, record);
            return Ok(CancelResult::Cancelled);
        }
        if let Some(record) = self.terminal.get(&operation_id) {
            validate_cancellation(record.metadata, target, current_tick, current_epoch)?;
            return match record.outcome {
                TerminalOutcome::Cancelled { .. } => Ok(CancelResult::AlreadyCancelled),
                TerminalOutcome::Resolved { .. } => Err(EffectError::AlreadyResolved(operation_id)),
            };
        }
        Err(self.unknown(operation_id))
    }

    /// Atomically cancels every pending operation for exactly this generation.
    /// Returns IDs in deterministic order; the count cannot exceed max_pending.
    pub fn cancel_target(
        &mut self,
        target: EntityRef,
        current_tick: u64,
        current_epoch: u64,
    ) -> Result<Vec<OperationId>, EffectError> {
        self.validate()?;
        validate_target(target)?;
        let mut selected = Vec::new();
        for (operation_id, request) in &self.pending {
            if request.target == target {
                let metadata = RequestMetadata::from(request);
                validate_cancellation(metadata, target, current_tick, current_epoch)?;
                selected.push((*operation_id, metadata));
            }
        }
        let mut cancelled = Vec::with_capacity(selected.len());
        for (operation_id, metadata) in selected {
            self.pending.remove(&operation_id);
            self.insert_terminal(
                operation_id,
                TerminalRecord {
                    metadata,
                    outcome: TerminalOutcome::Cancelled {
                        cancelled_tick: current_tick,
                        cancelled_epoch: current_epoch,
                    },
                },
            );
            cancelled.push(operation_id);
        }
        Ok(cancelled)
    }

    /// Checks limits, IDs/counter, map consistency, terminal ordering, all
    /// payload sizes, response kinds, target generations, and temporal records.
    /// A bounded outer decoder is also required before loading untrusted bytes.
    pub fn validate(&self) -> Result<(), EffectError> {
        self.limits.validate()?;
        if self.pending.len() > self.limits.max_pending as usize {
            return Err(EffectError::InvalidState("too many pending operations"));
        }
        if self.terminal.len() > self.limits.max_terminal as usize {
            return Err(EffectError::InvalidState("too many terminal operations"));
        }
        if self.terminal_order.len() != self.terminal.len() {
            return Err(EffectError::InvalidState("terminal order length mismatch"));
        }
        for (operation_id, request) in &self.pending {
            self.validate_id(*operation_id)?;
            if request.operation_id != *operation_id {
                return Err(EffectError::InvalidState(
                    "pending map key differs from request ID",
                ));
            }
            if self.terminal.contains_key(operation_id) {
                return Err(EffectError::InvalidState(
                    "operation is both pending and terminal",
                ));
            }
            validate_target(request.target)?;
            self.validate_payload(&request.payload)?;
        }
        for (operation_id, record) in &self.terminal {
            self.validate_id(*operation_id)?;
            validate_target(record.metadata.target)?;
            match &record.outcome {
                TerminalOutcome::Resolved {
                    applied_tick,
                    delivery_epoch,
                    committed_epoch,
                    value,
                } => {
                    validate_tick(record.metadata.issued_tick, *applied_tick)?;
                    validate_epoch(record.metadata.issued_epoch, *delivery_epoch)?;
                    validate_commit_epoch(
                        record.metadata.issued_epoch,
                        *committed_epoch,
                        *delivery_epoch,
                    )?;
                    self.validate_value(record.metadata.response_kind, value)?;
                }
                TerminalOutcome::Cancelled {
                    cancelled_tick,
                    cancelled_epoch,
                } => {
                    validate_tick(record.metadata.issued_tick, *cancelled_tick)?;
                    validate_epoch(record.metadata.issued_epoch, *cancelled_epoch)?;
                }
            }
        }
        let mut ordered = BTreeSet::new();
        for operation_id in &self.terminal_order {
            if !self.terminal.contains_key(operation_id) || !ordered.insert(*operation_id) {
                return Err(EffectError::InvalidState(
                    "terminal order has an absent or duplicate ID",
                ));
            }
        }
        Ok(())
    }

    /// Adds snapshot/runtime tick and epoch checks to the intrinsic validation.
    /// Liveness of pending targets is checked by the owning runtime entity map.
    pub fn validate_context(
        &self,
        current_tick: u64,
        current_epoch: u64,
    ) -> Result<(), EffectError> {
        self.validate()?;
        let check = |tick, epoch| {
            if tick > current_tick {
                return Err(EffectError::InvalidState(
                    "effect record exceeds snapshot tick",
                ));
            }
            if epoch > current_epoch {
                return Err(EffectError::InvalidState(
                    "effect record exceeds snapshot epoch",
                ));
            }
            Ok(())
        };
        for request in self.pending.values() {
            check(request.issued_tick, request.issued_epoch)?;
        }
        for record in self.terminal.values() {
            check(record.metadata.issued_tick, record.metadata.issued_epoch)?;
            match record.outcome {
                TerminalOutcome::Resolved {
                    applied_tick,
                    delivery_epoch,
                    ..
                } => {
                    check(applied_tick, delivery_epoch)?;
                }
                TerminalOutcome::Cancelled {
                    cancelled_tick,
                    cancelled_epoch,
                } => {
                    check(cancelled_tick, cancelled_epoch)?;
                }
            }
        }
        Ok(())
    }

    fn validate_id(&self, operation_id: OperationId) -> Result<(), EffectError> {
        if operation_id.namespace() != self.namespace {
            return Err(EffectError::InvalidState("operation namespace mismatch"));
        }
        let nonce = operation_id.nonce();
        if nonce == 0 || nonce > self.last_nonce {
            return Err(EffectError::InvalidState(
                "operation nonce exceeds persisted high-water mark",
            ));
        }
        Ok(())
    }

    fn validate_payload(&self, payload: &EffectPayload) -> Result<(), EffectError> {
        if payload.byte_len() > self.limits.max_request_bytes as usize {
            return Err(EffectError::RequestTooLarge {
                bytes: payload.byte_len(),
                limit: self.limits.max_request_bytes,
            });
        }
        Ok(())
    }

    fn validate_value(&self, expected: EffectKind, value: &EffectValue) -> Result<(), EffectError> {
        if value.kind() != expected {
            return Err(EffectError::ResponseTypeMismatch {
                expected,
                actual: value.kind(),
            });
        }
        if value.byte_len() > self.limits.max_response_bytes as usize {
            return Err(EffectError::ResponseTooLarge {
                bytes: value.byte_len(),
                limit: self.limits.max_response_bytes,
            });
        }
        Ok(())
    }

    fn validate_response(
        &self,
        metadata: RequestMetadata,
        response: &EffectResolved,
        current_tick: u64,
        current_epoch: u64,
    ) -> Result<(), EffectError> {
        validate_target(response.target)?;
        if response.target != metadata.target {
            return Err(EffectError::TargetMismatch {
                expected: metadata.target,
                actual: response.target,
            });
        }
        self.validate_value(metadata.response_kind, &response.value)?;
        if response.apply_tick != current_tick {
            return Err(EffectError::ApplyTickMismatch {
                scheduled: response.apply_tick,
                current: current_tick,
            });
        }
        validate_tick(metadata.issued_tick, current_tick)?;
        if response.delivery_epoch != current_epoch {
            return Err(EffectError::DeliveryEpochMismatch {
                delivered: response.delivery_epoch,
                current: current_epoch,
            });
        }
        validate_epoch(metadata.issued_epoch, current_epoch)?;
        validate_commit_epoch(
            metadata.issued_epoch,
            response.committed_epoch,
            current_epoch,
        )
    }

    fn insert_terminal(&mut self, operation_id: OperationId, record: TerminalRecord) {
        // Only called for an already validated pending ID after all fallible
        // checks. Completion order, rather than ID order, determines eviction.
        self.terminal.insert(operation_id, record);
        self.terminal_order.push_back(operation_id);
        while self.terminal.len() > self.limits.max_terminal as usize {
            if let Some(expired) = self.terminal_order.pop_front() {
                self.terminal.remove(&expired);
            }
        }
    }

    fn unknown(&self, operation_id: OperationId) -> EffectError {
        let nonce = operation_id.nonce();
        if operation_id.namespace() == self.namespace && nonce != 0 && nonce <= self.last_nonce {
            EffectError::RetiredOperation(operation_id)
        } else {
            EffectError::UnknownOperation(operation_id)
        }
    }

    fn closed_or_unknown(&self, operation_id: OperationId) -> EffectError {
        match self.terminal.get(&operation_id) {
            Some(TerminalRecord {
                outcome: TerminalOutcome::Cancelled { .. },
                ..
            }) => EffectError::OperationCancelled(operation_id),
            Some(TerminalRecord {
                outcome: TerminalOutcome::Resolved { .. },
                ..
            }) => EffectError::AlreadyResolved(operation_id),
            None => self.unknown(operation_id),
        }
    }
}

fn validate_target(target: EntityRef) -> Result<(), EffectError> {
    if target.object_id.0 <= 0 || target.generation == 0 {
        return Err(EffectError::InvalidTarget(target));
    }
    Ok(())
}

fn validate_tick(issued: u64, current: u64) -> Result<(), EffectError> {
    if current < issued {
        return Err(EffectError::BeforeIssueTick { issued, current });
    }
    Ok(())
}

fn validate_epoch(issued: u64, current: u64) -> Result<(), EffectError> {
    if current < issued {
        return Err(EffectError::StaleEpoch { issued, current });
    }
    Ok(())
}

fn validate_commit_epoch(issued: u64, committed: u64, current: u64) -> Result<(), EffectError> {
    if committed < issued || committed > current {
        return Err(EffectError::InvalidCommittedEpoch {
            issued,
            committed,
            current,
        });
    }
    Ok(())
}

fn validate_cancellation(
    metadata: RequestMetadata,
    target: EntityRef,
    current_tick: u64,
    current_epoch: u64,
) -> Result<(), EffectError> {
    if target != metadata.target {
        return Err(EffectError::TargetMismatch {
            expected: metadata.target,
            actual: target,
        });
    }
    validate_tick(metadata.issued_tick, current_tick)?;
    validate_epoch(metadata.issued_epoch, current_epoch)
}
