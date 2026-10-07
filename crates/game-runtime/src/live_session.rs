//! Live replica lifecycle for A's native checkpoint/accepted-tick protocol.
//!
//! The transport must authenticate its authority and enforce a byte limit before
//! decoding. Tokens belong to socket callbacks, not to fields supplied by peers.
//! This module never executes local intents or translates FreeSO FSOv/VMNet data.
use crate::{
    AcceptedTick, ActionId, CancelIntent, EntityRef, GameRuntime, GameRuntimeError,
    InteractionIntent, OfferBatch, PrincipalKey, QueryOptions, RuntimeProjection, RuntimeRole,
    SimRuntime, TickOutcome,
};
use bincode::Options;
use serde::Serialize;
use sim_core::interactions::{EntityVersion, InteractionKey};
use sim_core::snapshot::{
    MAX_SNAPSHOT_PAYLOAD_BYTES, SNAPSHOT_CHECKSUM_LEN, SNAPSHOT_HEADER_LEN, SnapshotExpectation,
};
use std::io::{self, Write};

const MAX_CHECKPOINT_BYTES: usize =
    MAX_SNAPSHOT_PAYLOAD_BYTES as usize + SNAPSHOT_HEADER_LEN + SNAPSHOT_CHECKSUM_LEN;

/// A fresh identity is required for each selected Sim/lot/source admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamIdentity {
    pub browser_epoch: u64,
    pub source_epoch: u64,
    pub lot_incarnation: u64,
}

/// Capture this in the connection's callbacks; do not read a new token inside
/// an old callback. Fields are deliberately private and not deserializable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionToken {
    identity: StreamIdentity,
    generation: u64,
}

/// Correlates a checkpoint response to one outstanding recovery request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointTicket {
    connection: ConnectionToken,
    request: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    AwaitingCheckpoint,
    Live,
    Suspended,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayLimits {
    pub max_batch_ticks: usize,
    pub max_batch_commands: usize,
    pub max_batch_bytes: usize,
    pub max_checkpoint_bytes: usize,
}
impl Default for ReplayLimits {
    fn default() -> Self {
        Self {
            max_batch_ticks: 64,
            max_batch_commands: 4096,
            max_batch_bytes: 8 * 1024 * 1024,
            max_checkpoint_bytes: MAX_CHECKPOINT_BYTES,
        }
    }
}
impl ReplayLimits {
    fn validate(self) -> Result<(), LiveError> {
        if self.max_batch_ticks == 0
            || self.max_batch_ticks > 256
            || self.max_batch_commands == 0
            || self.max_batch_commands > 65536
            || self.max_batch_bytes == 0
            || self.max_batch_bytes > 8 * 1024 * 1024
            || self.max_checkpoint_bytes <= SNAPSHOT_HEADER_LEN + SNAPSHOT_CHECKSUM_LEN
            || self.max_checkpoint_bytes > MAX_CHECKPOINT_BYTES
        {
            return Err(LiveError::InvalidLimits);
        }
        Ok(())
    }
}

/// The hash is the authority's post-tick hash, not a value computed by the UI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TickFrame {
    pub accepted: AcceptedTick,
    pub state_hash: [u8; 32],
}

/// A completed-tick checkpoint: a checkpoint at N is followed by tick N+1.
/// Bytes are borrowed and are not retained after a successful handoff.
#[derive(Clone, Copy, Debug)]
pub struct Checkpoint<'a> {
    pub completed_tick: u64,
    pub state_hash: [u8; 32],
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplicaCursor {
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub completed_tick: u64,
    pub state_hash: [u8; 32],
}

/// A source menu selection. This only prepares an intent for server admission.
#[derive(Clone, Copy, Debug)]
pub struct InteractionSelection {
    pub principal: PrincipalKey,
    pub actor: EntityRef,
    pub target: EntityRef,
    pub interaction: InteractionKey,
    pub param0: i16,
    pub command_sequence: u64,
}

/// An exact visible queue item, never its display position or target object ID.
#[derive(Clone, Copy, Debug)]
pub struct CancelSelection {
    pub principal: PrincipalKey,
    pub actor: EntityRef,
    pub action: ActionId,
    pub command_sequence: u64,
}

#[derive(Debug)]
pub enum LiveError {
    InvalidIdentity,
    InvalidLimits,
    NotReplica,
    StaleConnection,
    StaleCheckpoint,
    NotLive,
    Closed,
    CounterExhausted,
    Limit(&'static str),
    Checkpoint(String),
    CheckpointMetadata,
    ChangedConfiguration,
    HistoryConflict,
    RecoveryBehind,
    HashMismatch { tick: u64 },
    UnexpectedEffects,
    SelectionUnavailable,
    MissingQueue,
    ReplayedSequence { received: u64, last: u64 },
    RollbackFailed,
    Runtime(GameRuntimeError),
}
impl std::fmt::Display for LiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "live replica: {self:?}")
    }
}
impl std::error::Error for LiveError {}
impl From<GameRuntimeError> for LiveError {
    fn from(error: GameRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

pub struct LiveReplica {
    runtime: Option<GameRuntime>,
    identity: StreamIdentity,
    connection_generation: u64,
    checkpoint_generation: u64,
    ticket: Option<CheckpointTicket>,
    status: SessionStatus,
    limits: ReplayLimits,
    anchor: Option<ReplicaCursor>,
}

impl LiveReplica {
    pub fn new(
        runtime: GameRuntime,
        identity: StreamIdentity,
        limits: ReplayLimits,
    ) -> Result<Self, LiveError> {
        limits.validate()?;
        if identity.browser_epoch == 0
            || identity.source_epoch == 0
            || identity.lot_incarnation == 0
        {
            return Err(LiveError::InvalidIdentity);
        }
        if runtime.sim().role() != RuntimeRole::Replica {
            return Err(LiveError::NotReplica);
        }
        let mut result = Self {
            runtime: Some(runtime),
            identity,
            connection_generation: 1,
            checkpoint_generation: 0,
            ticket: None,
            status: SessionStatus::AwaitingCheckpoint,
            limits,
            anchor: None,
        };
        result.request_checkpoint(result.connection())?;
        Ok(result)
    }

    pub fn status(&self) -> SessionStatus {
        self.status
    }
    pub fn connection(&self) -> ConnectionToken {
        ConnectionToken {
            identity: self.identity,
            generation: self.connection_generation,
        }
    }
    pub fn checkpoint_ticket(&self) -> Option<CheckpointTicket> {
        self.ticket
    }
    /// Last committed cursor, also available while recovery is in progress.
    pub fn cursor(&self) -> Option<ReplicaCursor> {
        self.anchor
    }
    /// Read-only diagnostics. During recovery this is LAST KNOWN state, not live.
    pub fn runtime(&self) -> Option<&GameRuntime> {
        self.runtime.as_ref()
    }
    pub fn projection(&self) -> Result<RuntimeProjection, LiveError> {
        self.require_live(self.connection())?;
        Ok(self.runtime.as_ref().ok_or(LiveError::Closed)?.projection())
    }

    fn require_connection(&self, token: ConnectionToken) -> Result<(), LiveError> {
        if self.status == SessionStatus::Closed {
            return Err(LiveError::Closed);
        }
        if token != self.connection() {
            return Err(LiveError::StaleConnection);
        }
        Ok(())
    }
    fn require_live(&self, token: ConnectionToken) -> Result<(), LiveError> {
        self.require_connection(token)?;
        if self.status != SessionStatus::Live {
            return Err(LiveError::NotLive);
        }
        Ok(())
    }

    pub fn suspend(&mut self, token: ConnectionToken) -> Result<(), LiveError> {
        self.require_connection(token)?;
        self.ticket = None;
        self.status = SessionStatus::Suspended;
        Ok(())
    }
    /// Reconnecting fences every callback from the previous socket, even for
    /// the same lot. The new socket must recover before exposing live state.
    pub fn reconnect(&mut self) -> Result<ConnectionToken, LiveError> {
        if self.status == SessionStatus::Closed {
            return Err(LiveError::Closed);
        }
        let Some(next) = self.connection_generation.checked_add(1) else {
            self.close();
            return Err(LiveError::CounterExhausted);
        };
        self.connection_generation = next;
        self.status = SessionStatus::AwaitingCheckpoint;
        self.request_checkpoint(self.connection())?;
        Ok(self.connection())
    }
    pub fn request_checkpoint(
        &mut self,
        token: ConnectionToken,
    ) -> Result<CheckpointTicket, LiveError> {
        self.require_connection(token)?;
        if self.status == SessionStatus::Suspended {
            return Err(LiveError::NotLive);
        }
        let Some(next) = self.checkpoint_generation.checked_add(1) else {
            self.close();
            return Err(LiveError::CounterExhausted);
        };
        self.checkpoint_generation = next;
        let ticket = CheckpointTicket {
            connection: token,
            request: next,
        };
        self.ticket = Some(ticket);
        self.status = SessionStatus::AwaitingCheckpoint;
        Ok(ticket)
    }
    pub fn close(&mut self) {
        self.runtime = None;
        self.ticket = None;
        self.anchor = None;
        self.status = SessionStatus::Closed;
    }

    /// Install a validated native checkpoint and its contiguous accepted tail
    /// together. Historical outcomes are intentionally NOT returned: recovery
    /// must not replay old audio, headlines, UI events or durable operations.
    pub fn install_checkpoint(
        &mut self,
        ticket: CheckpointTicket,
        checkpoint: Checkpoint<'_>,
        tail: &[TickFrame],
    ) -> Result<ReplicaCursor, LiveError> {
        self.require_connection(ticket.connection)?;
        if self.status != SessionStatus::AwaitingCheckpoint || self.ticket != Some(ticket) {
            return Err(LiveError::StaleCheckpoint);
        }
        if checkpoint.bytes.len() > self.limits.max_checkpoint_bytes {
            return Err(LiveError::Limit("checkpoint bytes"));
        }
        preflight(tail, self.limits)?;
        let current = self.runtime.as_ref().ok_or(LiveError::Closed)?;
        let expected = current.sim().state();
        let mut expectation = SnapshotExpectation::new(expected.lot_id, expected.authority_epoch);
        expectation.limits.max_payload_bytes =
            (self.limits.max_checkpoint_bytes - SNAPSHOT_HEADER_LEN - SNAPSHOT_CHECKSUM_LEN) as u64;
        let state =
            sim_core::snapshot::decode(checkpoint.bytes, current.sim().content(), expectation)
                .map_err(|error| LiveError::Checkpoint(error.to_string()))?;
        if state.mode != expected.mode
            || state.limits != expected.limits
            || state.effects.namespace() != expected.effects.namespace()
            || state.effects.limits() != expected.effects.limits()
        {
            return Err(LiveError::ChangedConfiguration);
        }
        if state.completed_tick != checkpoint.completed_tick {
            return Err(LiveError::CheckpointMetadata);
        }
        let sim =
            SimRuntime::from_state(state, current.sim().content().clone(), RuntimeRole::Replica)
                .map_err(|error| LiveError::Runtime(error.into()))?;
        let mut candidate = GameRuntime { sim };
        let checkpoint_cursor = cursor(&candidate)?;
        if checkpoint_cursor.state_hash != checkpoint.state_hash {
            return Err(LiveError::CheckpointMetadata);
        }
        check_anchor(checkpoint_cursor, self.anchor)?;
        replay(
            &mut candidate,
            tail,
            self.anchor,
            false,
            &mut crate::avatar_projection::AvatarCapture::new(false),
        )?;
        let completed = cursor(&candidate)?;
        if self
            .anchor
            .is_some_and(|anchor| completed.completed_tick < anchor.completed_tick)
        {
            return Err(LiveError::RecoveryBehind);
        }
        self.runtime = Some(candidate);
        self.anchor = Some(completed);
        self.ticket = None;
        self.status = SessionStatus::Live;
        Ok(completed)
    }

    /// Tick transitions run through the actual runtime. A bad later tick rolls
    /// back the WHOLE batch and publishes no outcomes. Immutable content is not
    /// cloned on the successful tick path; the rollback copy is simulation state.
    pub fn apply_batch(
        &mut self,
        token: ConnectionToken,
        frames: &[TickFrame],
    ) -> Result<Vec<TickOutcome>, LiveError> {
        self.apply_batch_inner(token, frames, false)
            .map(|(outcomes, _)| outcomes)
    }

    /// Optional read-only per-tick avatar inputs are published only after the
    /// entire accepted batch validates. A None trace exceeds its presentation
    /// budget and requires a visual reset; it never drops simulation commands.
    pub fn apply_batch_with_avatar_frames(
        &mut self,
        token: ConnectionToken,
        frames: &[TickFrame],
    ) -> Result<(Vec<TickOutcome>, Option<Vec<crate::AvatarVisualFrame>>), LiveError> {
        self.apply_batch_inner(token, frames, true)
    }

    fn apply_batch_inner(
        &mut self,
        token: ConnectionToken,
        frames: &[TickFrame],
        capture: bool,
    ) -> Result<(Vec<TickOutcome>, Option<Vec<crate::AvatarVisualFrame>>), LiveError> {
        self.require_live(token)?;
        if let Err(error) = preflight(frames, self.limits) {
            self.request_checkpoint(token)?;
            return Err(error);
        }
        if frames.is_empty() {
            return Ok((Vec::new(), capture.then(Vec::new)));
        }
        let mut capture = crate::avatar_projection::AvatarCapture::new(capture);
        let runtime = self.runtime.as_mut().ok_or(LiveError::Closed)?;
        let before = runtime.sim().state().clone();
        let result = replay(runtime, frames, None, true, &mut capture).and_then(|outcomes| {
            let completed = cursor(runtime)?;
            Ok((outcomes, completed))
        });
        match result {
            Ok((outcomes, completed)) => {
                self.anchor = Some(completed);
                Ok((outcomes, capture.finish()))
            }
            Err(error) => {
                let restored = SimRuntime::from_state(
                    before,
                    runtime.sim().content().clone(),
                    RuntimeRole::Replica,
                );
                match restored {
                    Ok(sim) => runtime.sim = sim,
                    Err(_) => {
                        self.close();
                        return Err(LiveError::RollbackFailed);
                    }
                }
                self.request_checkpoint(token)?;
                Err(error)
            }
        }
    }

    pub fn offers(
        &self,
        token: ConnectionToken,
        principal: PrincipalKey,
        actor: EntityRef,
        target: EntityRef,
        options: QueryOptions,
    ) -> Result<OfferBatch, LiveError> {
        self.require_live(token)?;
        Ok(self
            .runtime
            .as_ref()
            .ok_or(LiveError::Closed)?
            .offers(principal, actor, target, options)?)
    }

    /// Re-query on selection rather than expiring a menu on every 30 Hz tick.
    /// Exact entity generations, source action/Param0 and the current queue
    /// revision are retained. The server still revalidates this unaccepted intent.
    pub fn prepare_interaction(
        &self,
        token: ConnectionToken,
        selection: InteractionSelection,
    ) -> Result<InteractionIntent, LiveError> {
        self.require_live(token)?;
        if selection.command_sequence == 0 {
            return Err(LiveError::SelectionUnavailable);
        }
        let batch = self.offers(
            token,
            selection.principal,
            selection.actor,
            selection.target,
            QueryOptions::default(),
        )?;
        if !batch.offers.iter().any(|offer| {
            offer.interaction == selection.interaction && offer.param0 == selection.param0
        }) {
            return Err(LiveError::SelectionUnavailable);
        }
        let queue = self
            .runtime
            .as_ref()
            .ok_or(LiveError::Closed)?
            .sim()
            .state()
            .interaction_queues
            .get(&selection.actor.object_id)
            .ok_or(LiveError::MissingQueue)?;
        check_sequence(selection.command_sequence, queue.last_command_sequence())?;
        Ok(InteractionIntent {
            principal: selection.principal,
            seen: batch.seen,
            queue_revision: queue.revision(),
            command_sequence: selection.command_sequence,
            interaction: selection.interaction,
            param0: selection.param0,
        })
    }
    /// Prepare cancellation against the CURRENT visible queue and actor version.
    /// Cancellation is not an optimistic deletion; the accepted stream owns its
    /// source cancellation/idle notification and eventual frame completion.
    pub fn prepare_cancel(
        &self,
        token: ConnectionToken,
        selection: CancelSelection,
    ) -> Result<CancelIntent, LiveError> {
        self.require_live(token)?;
        let state = self
            .runtime
            .as_ref()
            .ok_or(LiveError::Closed)?
            .sim()
            .state();
        if selection.command_sequence == 0 || !state.ids.is_live(selection.actor) {
            return Err(LiveError::SelectionUnavailable);
        }
        let actor = state
            .entities
            .get(&selection.actor.object_id)
            .filter(|entity| entity.info.reference == selection.actor && !entity.info.dead)
            .ok_or(LiveError::SelectionUnavailable)?;
        if !state
            .interaction_access
            .get(&selection.actor)
            .is_some_and(|access| access.principal == selection.principal)
        {
            return Err(LiveError::SelectionUnavailable);
        }
        let queue = state
            .interaction_queues
            .get(&selection.actor.object_id)
            .ok_or(LiveError::MissingQueue)?;
        if !queue.entries().iter().enumerate().any(|(index, entry)| {
            entry.id == selection.action && queue.entry_visible(index) == Some(true)
        }) {
            return Err(LiveError::SelectionUnavailable);
        }
        check_sequence(selection.command_sequence, queue.last_command_sequence())?;
        Ok(CancelIntent {
            principal: selection.principal,
            world_revision: state.completed_tick,
            actor: EntityVersion {
                key: crate::entity_key(selection.actor),
                revision: actor.revision,
            },
            queue_revision: queue.revision(),
            command_sequence: selection.command_sequence,
            action: selection.action,
        })
    }
}

fn check_sequence(received: u64, last: Option<u64>) -> Result<(), LiveError> {
    if let Some(last) = last
        && received <= last
    {
        return Err(LiveError::ReplayedSequence { received, last });
    }
    Ok(())
}
fn cursor(runtime: &GameRuntime) -> Result<ReplicaCursor, LiveError> {
    let state = runtime.sim().state();
    Ok(ReplicaCursor {
        lot_id: state.lot_id,
        authority_epoch: state.authority_epoch,
        completed_tick: state.completed_tick,
        state_hash: runtime
            .sim()
            .state_hash()
            .map_err(|error| LiveError::Runtime(error.into()))?,
    })
}
fn check_anchor(current: ReplicaCursor, anchor: Option<ReplicaCursor>) -> Result<(), LiveError> {
    if anchor.is_some_and(|anchor| {
        current.completed_tick == anchor.completed_tick && current.state_hash != anchor.state_hash
    }) {
        return Err(LiveError::HistoryConflict);
    }
    Ok(())
}
fn replay(
    runtime: &mut GameRuntime,
    frames: &[TickFrame],
    anchor: Option<ReplicaCursor>,
    publish: bool,
    capture: &mut crate::avatar_projection::AvatarCapture,
) -> Result<Vec<TickOutcome>, LiveError> {
    let mut outcomes = Vec::new();
    for frame in frames {
        let outcome = runtime.apply_accepted(&frame.accepted)?;
        if outcome.state_hash != frame.state_hash {
            return Err(LiveError::HashMismatch { tick: outcome.tick });
        }
        if !outcome.effects.is_empty() {
            return Err(LiveError::UnexpectedEffects);
        }
        if let Some(anchor) = anchor
            && outcome.tick == anchor.completed_tick
            && outcome.state_hash != anchor.state_hash
        {
            return Err(LiveError::HistoryConflict);
        }
        if publish && !outcome.duplicate {
            capture.observe(runtime);
            outcomes.push(outcome);
        }
    }
    Ok(outcomes)
}

// Size counting streams directly into a bounded sink: it never allocates a
// complete binary copy merely to measure nested command payloads. The fixed
// integer, little-endian encoding matches A's native serializer and accepts
// tuple map keys. This measures decoded inputs; it is not a network decoder.
fn preflight(frames: &[TickFrame], limits: ReplayLimits) -> Result<(), LiveError> {
    if frames.len() > limits.max_batch_ticks {
        return Err(LiveError::Limit("batch ticks"));
    }
    let mut commands = 0usize;
    for frame in frames {
        commands = commands
            .checked_add(frame.accepted.commands.len())
            .filter(|total| *total <= limits.max_batch_commands)
            .ok_or(LiveError::Limit("batch commands"))?;
    }
    let mut sink = BudgetWriter(limits.max_batch_bytes);
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(limits.max_batch_bytes as u64)
        .serialize_into(&mut sink, frames)
        .map_err(|_| LiveError::Limit("batch bytes"))?;
    Ok(())
}
struct BudgetWriter(usize);
impl Write for BudgetWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("live batch byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
