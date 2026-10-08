//! Bounded authority-side orchestration of one native construction operation.
//!
//! Own this value on the serialized lot executor. Authentication, operation-ID
//! allocation and durable accounting stay in the server's existing providers.
//! No method creates a durable receipt, retries an effect, or debits an account.
use super::{
    ConstructionConsent, ConstructionError, ConstructionGrant, ConstructionQuote,
    ConstructionRequest, PlayerBinding,
};
use crate::live_session::TickFrame;
use crate::sim_core::world::build::BuildCommitStatus;
use crate::{EntityRef, GameRuntime, PrincipalKey, RuntimeRole, TickOutcome};

pub const MAX_QUOTE_LIFETIME_MS: u64 = 300_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Quoted,
    Expired,
    Cancelled,
    /// BeginBuild was admitted; geometry and accounting are NOT yet complete.
    Pending,
    /// Consult the same operation in the native state and durable journal.
    /// Never allocate a replacement operation merely because its reply was lost.
    Unknown,
    Outcome(BuildCommitStatus),
}

/// A transport adapter may expose this projection; it is not an authority grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteView {
    pub binding: PlayerBinding,
    pub operation: u64,
    pub consent: ConstructionConsent,
    /// Host monotonic time, not a client-supplied wall clock or simulation tick.
    pub expires_at_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub quote: QuoteView,
    pub phase: Phase,
}

/// Broadcast the frame once and route the actual outcome to the trusted effect
/// provider. Returning this is NOT a claim that the durable operation completed.
pub struct AdmittedBuild {
    pub frame: TickFrame,
    pub outcome: TickOutcome,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    Closed,
    InvalidLifetime,
    AdmissionChanged,
    ClockRegressed,
    ClockOverflow,
    RuntimeRegressed,
    Busy,
    ReplayedRequest,
    CorrelationMismatch,
    NotQuoted,
    AlreadySubmitted,
    OutcomeMismatch,
    Construction(ConstructionError),
    Runtime(String),
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "construction session: {self:?}")
    }
}
impl std::error::Error for SessionError {}
impl From<ConstructionError> for SessionError {
    fn from(error: ConstructionError) -> Self {
        Self::Construction(error)
    }
}
pub type Result<T> = std::result::Result<T, SessionError>;

struct Slot {
    offer: ConstructionQuote,
    view: QuoteView,
    phase: Phase,
    submitted: bool,
}

/// One full quote and one immutable admission identity: no unbounded quote map.
/// Keep the session (or equivalent durable request/operation correlation) across
/// socket replacement. A new process cannot recover idempotency from this RAM
/// value: it must restore it from the operator's durable operation journal.
pub struct ConstructionSession {
    binding: PlayerBinding,
    principal: PrincipalKey,
    actor: EntityRef,
    lot_id: u64,
    epoch: u64,
    content: [u8; 32],
    lifetime_ms: u64,
    last_time: Option<u64>,
    last_tick: u64,
    last_request: u64,
    slot: Option<Slot>,
    closed: bool,
}
impl ConstructionSession {
    pub fn new(runtime: &GameRuntime, grant: &ConstructionGrant, lifetime_ms: u64) -> Result<Self> {
        if lifetime_ms == 0 || lifetime_ms > MAX_QUOTE_LIFETIME_MS {
            return Err(SessionError::InvalidLifetime);
        }
        let state = runtime.sim().state();
        let session = Self {
            binding: grant.binding,
            principal: grant.principal,
            actor: grant.authority.actor,
            lot_id: state.lot_id,
            epoch: state.authority_epoch,
            content: state.content.content_hash,
            lifetime_ms,
            last_time: None,
            last_tick: state.completed_tick,
            last_request: 0,
            slot: None,
            closed: false,
        };
        session.require_admission(runtime, grant)?;
        Ok(session)
    }

    fn require_admission(&self, runtime: &GameRuntime, grant: &ConstructionGrant) -> Result<()> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        let state = runtime.sim().state();
        if runtime.sim().role() != RuntimeRole::Authority
            || grant.binding != self.binding
            || grant.principal != self.principal
            || grant.authority.actor != self.actor
            || !grant.authority.connected
            || grant.authority.owner.0 != self.binding.avatar_id
            || self.binding.source_epoch == 0
            || self.binding.lot_incarnation == 0
            || self.binding.lot_location == 0
            || self.binding.avatar_id == 0
            || self.principal.0 == 0
            || state.lot_id != self.lot_id
            || state.authority_epoch != self.epoch
            || state.content.content_hash != self.content
            || !state.ids.is_live(self.actor)
            || !state
                .interaction_access
                .get(&self.actor)
                .is_some_and(|a| a.principal == self.principal)
            || !state.entities.get(&self.actor.object_id).is_some_and(|e| {
                e.info.reference == self.actor
                    && e.info.is_avatar
                    && !e.info.dead
                    && e.info.persistent_id == self.binding.avatar_id
            })
        {
            return Err(SessionError::AdmissionChanged);
        }
        if state.completed_tick < self.last_tick {
            return Err(SessionError::RuntimeRegressed);
        }
        Ok(())
    }

    fn observe_time(
        &mut self,
        runtime: &GameRuntime,
        grant: &ConstructionGrant,
        now_ms: u64,
    ) -> Result<()> {
        self.require_admission(runtime, grant)?;
        if self.last_time.is_some_and(|last| now_ms < last) {
            return Err(SessionError::ClockRegressed);
        }
        self.last_time = Some(now_ms);
        self.last_tick = runtime.sim().state().completed_tick;
        if let Some(slot) = &mut self.slot
            && slot.phase == Phase::Quoted
            && now_ms >= slot.view.expires_at_ms
        {
            slot.phase = Phase::Expired;
        }
        Ok(())
    }

    pub fn view(&self) -> Option<SessionView> {
        self.slot.as_ref().map(|slot| SessionView {
            quote: slot.view.clone(),
            phase: slot.phase.clone(),
        })
    }

    /// Allocate `operation` in the trusted journal before calling this; its
    /// uniqueness is not derived from request_id. Repeats must use the same ID.
    /// A duplicate request cannot change price/selections or extend the lease.
    pub fn offer(
        &mut self,
        runtime: &GameRuntime,
        grant: &ConstructionGrant,
        operation: u64,
        request: &ConstructionRequest,
        now_ms: u64,
    ) -> Result<QuoteView> {
        self.observe_time(runtime, grant, now_ms)?;
        if let Some(slot) = &self.slot {
            if request.request_id == slot.view.consent.request_id {
                if slot.phase != Phase::Quoted {
                    return Err(SessionError::ReplayedRequest);
                }
                if operation != slot.view.operation || request != &slot.offer.request {
                    return Err(SessionError::CorrelationMismatch);
                }
                // Re-query source and current grants without executing a tick.
                super::confirm(runtime, grant, &slot.offer, slot.view.consent)?;
                return Ok(slot.view.clone());
            }
            if matches!(
                slot.phase,
                Phase::Quoted
                    | Phase::Pending
                    | Phase::Unknown
                    | Phase::Outcome(BuildCommitStatus::NeedsReconciliation { .. })
            ) {
                return Err(SessionError::Busy);
            }
        }
        if request.request_id <= self.last_request {
            return Err(SessionError::ReplayedRequest);
        }
        let expires_at_ms = now_ms
            .checked_add(self.lifetime_ms)
            .ok_or(SessionError::ClockOverflow)?;
        let offer = super::quote(runtime, grant, operation, request)?;
        let view = QuoteView {
            binding: self.binding,
            operation,
            consent: offer.consent(),
            expires_at_ms,
        };
        self.last_request = request.request_id;
        self.slot = Some(Slot {
            offer,
            view: view.clone(),
            phase: Phase::Quoted,
            submitted: false,
        });
        Ok(view)
    }

    pub fn cancel(
        &mut self,
        runtime: &GameRuntime,
        grant: &ConstructionGrant,
        request_id: u64,
        operation: u64,
        now_ms: u64,
    ) -> Result<()> {
        self.observe_time(runtime, grant, now_ms)?;
        let slot = self.slot.as_mut().ok_or(SessionError::NotQuoted)?;
        if slot.view.operation != operation || slot.view.consent.request_id != request_id {
            return Err(SessionError::CorrelationMismatch);
        }
        if slot.submitted {
            return Err(SessionError::AlreadySubmitted);
        }
        slot.phase = Phase::Cancelled;
        Ok(())
    }

    /// Run on the serialized authority executor, after explicit user consent.
    /// Revalidation and BeginBuild share one borrow of the authoritative runtime.
    /// A repeat never emits another frame or re-dispatches the durable request.
    pub fn submit(
        &mut self,
        runtime: &mut GameRuntime,
        grant: &ConstructionGrant,
        operation: u64,
        consent: ConstructionConsent,
        now_ms: u64,
    ) -> Result<Option<AdmittedBuild>> {
        self.observe_time(runtime, grant, now_ms)?;
        let slot = self.slot.as_mut().ok_or(SessionError::NotQuoted)?;
        if operation != slot.view.operation || consent != slot.view.consent {
            return Err(SessionError::CorrelationMismatch);
        }
        if slot.submitted {
            return Ok(None);
        }
        if slot.phase != Phase::Quoted {
            return Err(SessionError::NotQuoted);
        }
        let command = super::confirm(runtime, grant, &slot.offer, consent)?;
        let accepted = runtime
            .sim()
            .next_tick(vec![command])
            .map_err(|e| SessionError::Runtime(e.to_string()))?;
        // Once execution is attempted, no exception or lost reply makes the
        // original operation safe to re-issue. Reconcile its exact journal ID.
        slot.submitted = true;
        slot.phase = Phase::Unknown;
        let outcome = runtime
            .apply_accepted(&accepted)
            .map_err(|e| SessionError::Runtime(e.to_string()))?;
        self.last_tick = outcome.tick;
        slot.phase = Phase::Pending;
        Ok(Some(AdmittedBuild {
            frame: TickFrame {
                accepted,
                state_hash: outcome.state_hash,
            },
            outcome,
        }))
    }

    /// Observe only the real accepted build state, not a browser-supplied result.
    /// Losing permission to make NEW edits does not change an already admitted
    /// operation into a success/failure; the original account binding still holds.
    pub fn refresh(
        &mut self,
        runtime: &GameRuntime,
        grant: &ConstructionGrant,
        now_ms: u64,
    ) -> Result<Option<SessionView>> {
        self.observe_time(runtime, grant, now_ms)?;
        let Some(slot) = &mut self.slot else {
            return Ok(None);
        };
        if slot.submitted {
            let builds = &runtime.sim().state().world.builds;
            if let Some(outcome) = builds.outcome(slot.view.operation) {
                let expected_cost = if matches!(outcome.status, BuildCommitStatus::Rejected { .. })
                {
                    0
                } else {
                    slot.view.consent.cost
                };
                if outcome.operation != slot.view.operation
                    || outcome.preview_hash != slot.view.consent.preview_hash
                    || outcome.cost != expected_cost
                    || matches!(&slot.phase, Phase::Outcome(old) if old != &outcome.status)
                {
                    return Err(SessionError::OutcomeMismatch);
                }
                slot.phase = Phase::Outcome(outcome.status.clone());
            } else if let Some(pending) = builds
                .pending_effect()
                .filter(|p| p.operation == slot.view.operation)
            {
                if matches!(slot.phase, Phase::Outcome(_))
                    || pending.actor != self.actor
                    || pending.owner != slot.offer.preview.owner
                    || pending.preview_hash != slot.view.consent.preview_hash
                    || pending.cost != slot.view.consent.cost
                {
                    return Err(SessionError::OutcomeMismatch);
                }
                slot.phase = Phase::Pending;
            } else if !matches!(slot.phase, Phase::Outcome(_)) {
                slot.phase = Phase::Unknown;
            }
        }
        Ok(self.view())
    }

    /// Socket loss invalidates unconfirmed quotes, not accepted construction.
    /// Keep this object for the same admitted account; recovery must call refresh.
    pub fn disconnect(&mut self) {
        if let Some(slot) = &mut self.slot {
            match slot.phase {
                Phase::Quoted => slot.phase = Phase::Cancelled,
                Phase::Pending => slot.phase = Phase::Unknown,
                _ => {}
            }
        }
    }
    /// The durable provider still owns outstanding operations after this RAM
    /// lifetime ends. Closing does not cancel a debit or certify reconciliation.
    pub fn close(&mut self) {
        self.slot = None;
        self.closed = true;
    }
}
