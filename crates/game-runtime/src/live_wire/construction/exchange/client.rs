//! Bounded client transaction model. No sockets, automatic retries or local edits.
//!
//! Keep this value scoped to one admitted NativePlayer. Only pass projections
//! from that same player's validated stream. A response is not proof of a world
//! commit; a matching native BuildCommitResult is. Persist operation correlation
//! in the host's durable journal before discarding an unresolved client lifetime.
use super::*;
use crate::RuntimeProjection;

/// Capture on request creation; never obtain a new token inside an old callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connection(u64);
#[must_use = "send once or mark the connection interrupted; never silently retry"]
pub struct Prepared {
    pub connection: Connection,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientStage {
    Idle,
    Quoting,
    Reviewing,
    Sending,
    Cancelling,
    Pending,
    Unknown,
    AwaitingReplica,
    Committed,
    Rejected,
    Reconciliation,
    Cancelled,
    Expired,
    Closed,
}
impl ClientStage {
    fn terminal(self) -> bool {
        matches!(
            self,
            Self::Committed
                | Self::Rejected
                | Self::Reconciliation
                | Self::Cancelled
                | Self::Expired
        )
    }
    fn replaceable(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Committed | Self::Rejected | Self::Cancelled | Self::Expired
        )
    }
}

pub struct Client {
    binding: PlayerBinding,
    lot_id: u64,
    epoch: u64,
    generation: u64,
    connected: bool,
    last_call: u64,
    last_request: u64,
    last_response_tick: u64,
    request: Option<ConstructionRequest>,
    quote: Option<Snapshot>,
    pending: Option<Vec<u8>>,
    submitted: bool,
    stage: ClientStage,
}
impl Client {
    pub fn new(binding: PlayerBinding, lot_id: u64, epoch: u64) -> Result<Self> {
        if !valid_binding(binding) || lot_id == 0 || epoch == 0 {
            return Err(ExchangeError::AdmissionChanged);
        }
        Ok(Self {
            binding,
            lot_id,
            epoch,
            generation: 1,
            connected: true,
            last_call: 0,
            last_request: 0,
            last_response_tick: 0,
            request: None,
            quote: None,
            pending: None,
            submitted: false,
            stage: ClientStage::Idle,
        })
    }
    pub fn connection(&self) -> Connection {
        Connection(self.generation)
    }
    pub fn stage(&self) -> ClientStage {
        self.stage
    }
    /// Last correlated server quote, not a permission grant or local countdown.
    /// expires_at_ms is in the SERVER clock domain. The server enforces expiry.
    pub fn quote(&self) -> Option<&Snapshot> {
        self.quote.as_ref()
    }
    pub fn request(&self) -> Option<&ConstructionRequest> {
        self.request.as_ref()
    }
    pub fn waiting_for_reply(&self) -> bool {
        self.pending.is_some()
    }
    fn require_connection(&self, token: Connection) -> Result<()> {
        if token != self.connection() {
            return Err(ExchangeError::StaleConnection);
        }
        if !self.connected || self.stage == ClientStage::Closed {
            return Err(ExchangeError::NotConnected);
        }
        Ok(())
    }
    fn prepare(&mut self, command: Command) -> Result<Prepared> {
        self.require_connection(self.connection())?;
        if self.pending.is_some() {
            return Err(ExchangeError::Busy);
        }
        let call_id = self
            .last_call
            .checked_add(1)
            .ok_or(ExchangeError::CounterExhausted)?;
        let bytes = encode_call(&Call {
            binding: self.binding,
            call_id,
            command,
        })?;
        let mut saved = Vec::new();
        saved
            .try_reserve_exact(bytes.len())
            .map_err(|_| ExchangeError::InvalidPacket)?;
        saved.extend_from_slice(&bytes);
        self.last_call = call_id;
        self.pending = Some(saved);
        Ok(Prepared {
            connection: self.connection(),
            bytes,
        })
    }
    /// Explicit new player selection. The caller supplies a strictly increasing
    /// request ID scoped to the server session, not a reserved build operation.
    pub fn begin(&mut self, request: ConstructionRequest) -> Result<Prepared> {
        if !self.stage.replaceable() {
            return Err(ExchangeError::Busy);
        }
        if request.binding != self.binding
            || request.lot_id != self.lot_id
            || request.authority_epoch != self.epoch
            || request.request_id <= self.last_request
        {
            return Err(ExchangeError::AdmissionChanged);
        }
        let prepared = self.prepare(Command::Quote(request.clone()))?;
        self.last_request = request.request_id;
        self.request = Some(request);
        self.quote = None;
        self.submitted = false;
        self.stage = ClientStage::Quoting;
        Ok(prepared)
    }
    /// Call only after the player reviews this exact hash/price and confirms it.
    pub fn confirm(&mut self, reviewed: ConstructionConsent) -> Result<Prepared> {
        if self.stage != ClientStage::Reviewing {
            return Err(ExchangeError::InvalidState);
        }
        let quote = self.quote.as_ref().ok_or(ExchangeError::InvalidState)?;
        if reviewed != quote.consent {
            return Err(ExchangeError::CorrelationMismatch);
        }
        let prepared = self.prepare(Command::Confirm {
            operation: quote.operation,
            consent: reviewed,
        })?;
        self.submitted = true;
        self.stage = ClientStage::Sending;
        Ok(prepared)
    }
    /// Cancellation applies to an unsubmitted quote, never an admitted debit.
    pub fn cancel(&mut self) -> Result<Prepared> {
        if self.stage != ClientStage::Reviewing || self.submitted {
            return Err(ExchangeError::InvalidState);
        }
        let quote = self.quote.as_ref().ok_or(ExchangeError::InvalidState)?;
        let prepared = self.prepare(Command::Cancel {
            request_id: quote.consent.request_id,
            operation: quote.operation,
        })?;
        self.stage = ClientStage::Cancelling;
        Ok(prepared)
    }
    /// Prepare a READ-ONLY reconciliation query. No confirm bytes are retained
    /// for a retry, and no automatic network request is made by this type.
    pub fn poll(&mut self) -> Result<Prepared> {
        let request_id = self
            .request
            .as_ref()
            .ok_or(ExchangeError::InvalidState)?
            .request_id;
        self.prepare(Command::Status {
            request_id,
            operation: self.quote.as_ref().map(|q| q.operation),
        })
    }
    /// Correlate a reply to the whole outstanding call, including its selections
    /// and revisions. Failures leave the previous state and pending call intact.
    pub fn accept(&mut self, token: Connection, bytes: &[u8]) -> Result<()> {
        self.require_connection(token)?;
        let expected = self
            .pending
            .as_ref()
            .ok_or(ExchangeError::CorrelationMismatch)?;
        let reply = decode_reply(bytes)?;
        if reply.request != *expected {
            return Err(ExchangeError::CorrelationMismatch);
        }
        let request = self.request.as_ref().ok_or(ExchangeError::InvalidState)?;
        let Some(snapshot) = reply.snapshot else {
            // A missing RAM record is never evidence that an attempted write did
            // not execute. Only an unsubmitted selection can safely be abandoned.
            if !self.stage.terminal() {
                self.stage = if self.submitted {
                    ClientStage::Unknown
                } else {
                    ClientStage::Cancelled
                };
            }
            self.pending = None;
            return Ok(());
        };
        if snapshot.binding != self.binding
            || snapshot.consent.request_id != request.request_id
            || snapshot.tick < self.last_response_tick
        {
            return Err(ExchangeError::CorrelationMismatch);
        }
        if let Some(old) = &self.quote {
            if snapshot.operation != old.operation
                || snapshot.consent != old.consent
                || snapshot.expires_at_ms != old.expires_at_ms
            {
                return Err(ExchangeError::CorrelationMismatch);
            }
            if self.stage.terminal() && snapshot.phase != old.phase {
                return Err(ExchangeError::OutcomeMismatch);
            }
        }
        let next = match snapshot.phase {
            WirePhase::Quoted => ClientStage::Reviewing,
            WirePhase::Expired => ClientStage::Expired,
            WirePhase::Cancelled => ClientStage::Cancelled,
            WirePhase::Pending | WirePhase::Unknown | WirePhase::Outcome(_) if !self.submitted => {
                return Err(ExchangeError::InvalidState);
            }
            WirePhase::Pending => ClientStage::Pending,
            WirePhase::Unknown => ClientStage::Unknown,
            WirePhase::Outcome(_) => {
                if matches!(
                    self.stage,
                    ClientStage::Committed | ClientStage::Rejected | ClientStage::Reconciliation
                ) {
                    self.stage
                } else {
                    ClientStage::AwaitingReplica
                }
            }
        };
        // A trusted status saying Quoted/Cancelled/Expired is a declaration by
        // the SAME retained server session that BeginBuild was never submitted.
        // Process-restart correctness requires the host's durable journal.
        if matches!(
            snapshot.phase,
            WirePhase::Quoted | WirePhase::Cancelled | WirePhase::Expired
        ) {
            self.submitted = false;
        }
        self.last_response_tick = snapshot.tick;
        self.quote = Some(snapshot);
        self.stage = next;
        self.pending = None;
        Ok(())
    }
    /// Only a matching outcome from the currently admitted replica can complete
    /// the UI transaction. A generic new checkpoint or unrelated build cannot.
    /// Read-only: this never edits geometry, balance, IDs, queues or build state.
    pub fn observe(&mut self, token: Connection, projection: &RuntimeProjection) -> Result<bool> {
        self.require_connection(token)?;
        if projection.lot_id != self.lot_id || projection.epoch != self.epoch {
            return Err(ExchangeError::AdmissionChanged);
        }
        if !self.submitted {
            return Ok(false);
        }
        let quote = self.quote.as_ref().ok_or(ExchangeError::InvalidState)?;
        if projection.tick < quote.tick {
            return Ok(false);
        }
        let Some(result) = projection.world.builds.outcome(quote.operation) else {
            return Ok(false);
        };
        let expected_cost = if matches!(result.status, BuildCommitStatus::Rejected { .. }) {
            0
        } else {
            quote.consent.cost
        };
        if result.operation != quote.operation
            || result.preview_hash != quote.consent.preview_hash
            || result.cost != expected_cost
            || matches!(&quote.phase,WirePhase::Outcome(old) if *old != result.status)
        {
            return Err(ExchangeError::OutcomeMismatch);
        }
        let next = match result.status {
            BuildCommitStatus::Committed { .. } => ClientStage::Committed,
            BuildCommitStatus::Rejected { .. } => ClientStage::Rejected,
            BuildCommitStatus::NeedsReconciliation { .. } => ClientStage::Reconciliation,
        };
        let changed = self.stage != next;
        self.quote
            .as_mut()
            .ok_or(ExchangeError::InvalidState)?
            .phase = WirePhase::Outcome(result.status.clone());
        self.stage = next;
        self.pending = None;
        Ok(changed)
    }
    /// Fence pending callbacks and retain unknown operations. This emits no request.
    pub fn disconnect(&mut self) -> Result<()> {
        if self.stage == ClientStage::Closed {
            return Err(ExchangeError::NotConnected);
        }
        if !self.connected {
            return Ok(());
        }
        let Some(next) = self.generation.checked_add(1) else {
            self.connected = false;
            self.stage = ClientStage::Closed;
            self.pending = None;
            return Err(ExchangeError::CounterExhausted);
        };
        self.generation = next;
        self.connected = false;
        self.pending = None;
        if !self.stage.replaceable() && self.stage != ClientStage::Reconciliation {
            self.stage = ClientStage::Unknown;
        }
        Ok(())
    }
    pub fn reconnect(&mut self) -> Result<Connection> {
        if self.stage == ClientStage::Closed {
            return Err(ExchangeError::NotConnected);
        }
        if self.connected {
            return Err(ExchangeError::InvalidState);
        }
        self.connected = true;
        Ok(self.connection())
    }
    /// Closing this presentation lifetime does not cancel or settle a durable
    /// obligation. The host must retain its journal/recovery record independently.
    pub fn close(&mut self) {
        self.connected = false;
        self.pending = None;
        self.stage = ClientStage::Closed;
    }
}

#[cfg(test)]
mod limits {
    use super::*;
    fn client() -> Client {
        Client::new(
            PlayerBinding {
                source_epoch: 1,
                lot_incarnation: 1,
                lot_location: 1,
                avatar_id: 1,
            },
            1,
            1,
        )
        .unwrap()
    }
    #[test]
    fn call_counter_exhaustion_never_wraps_or_creates_a_pending_packet() {
        let mut client = client();
        client.last_call = u64::MAX;
        assert!(matches!(
            client.prepare(Command::Status {
                request_id: 1,
                operation: None
            }),
            Err(ExchangeError::CounterExhausted)
        ));
        assert!(client.pending.is_none());
        assert_eq!(client.last_call, u64::MAX);
    }
    #[test]
    fn connection_counter_exhaustion_closes_without_reusing_a_generation() {
        let mut client = client();
        client.generation = u64::MAX;
        assert!(matches!(
            client.disconnect(),
            Err(ExchangeError::CounterExhausted)
        ));
        assert_eq!(client.stage(), ClientStage::Closed);
        assert!(client.reconnect().is_err());
    }
    #[test]
    fn explicit_close_never_reopens_or_prepares_a_query() {
        let mut client = client();
        let token = client.connection();
        client.close();
        assert!(client.reconnect().is_err());
        assert!(client.accept(token, b"late response").is_err());
        assert!(
            client
                .prepare(Command::Status {
                    request_id: 1,
                    operation: None
                })
                .is_err()
        );
    }
}
