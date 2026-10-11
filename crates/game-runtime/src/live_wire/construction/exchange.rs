//! Bounded construction exchange; authentication and durable effect execution are host-owned.
use super::session::{AdmittedBuild, ConstructionSession, SessionError};
use super::{ConstructionConsent, ConstructionGrant, ConstructionRequest, PlayerBinding};
use crate::GameRuntime;
use crate::sim_core::world::build::BuildCommitStatus;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Quote(ConstructionRequest),
    Confirm {
        operation: u64,
        consent: ConstructionConsent,
    },
    Cancel {
        request_id: u64,
        operation: u64,
    },
    Status {
        request_id: u64,
        operation: Option<u64>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub binding: PlayerBinding,
    pub call_id: u64,
    pub command: Command,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WirePhase {
    Quoted,
    Expired,
    Cancelled,
    Pending,
    Unknown,
    Outcome(BuildCommitStatus),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub binding: PlayerBinding,
    pub operation: u64,
    pub consent: ConstructionConsent,
    pub expires_at_ms: u64,
    pub tick: u64,
    pub phase: WirePhase,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub request: Vec<u8>,
    pub snapshot: Option<Snapshot>,
}
#[derive(Debug)]
pub enum ExchangeError {
    InvalidPacket,
    AdmissionChanged,
    CorrelationMismatch,
    MissingOperation,
    NotConnected,
    Busy,
    InvalidState,
    CounterExhausted,
    StaleConnection,
    OutcomeMismatch,
    Session(SessionError),
}
impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "construction exchange: {self:?}")
    }
}
impl std::error::Error for ExchangeError {}
pub type Result<T> = std::result::Result<T, ExchangeError>;
/// The host must persist/dispatch the admitted effect and broadcast the frame
/// even when response serialization or delivery fails. Neither action is done here.
#[must_use = "an admitted frame/effect must not be discarded when replying fails"]
pub struct Handled {
    pub reply: Result<Vec<u8>>,
    pub admitted: Option<AdmittedBuild>,
}

use super::super::{CompareWriter, codec, guard};
use super::session::{Phase, SessionView};
use bincode::Options;
use serde::de::DeserializeOwned;

pub const MAX_CALL_BYTES: usize = super::MAX_REQUEST_BYTES + 128;
pub const MAX_REPLY_BYTES: usize = MAX_CALL_BYTES + 128 * 1024;
const CALL_MAGIC: &[u8; 8] = b"WLBQ\x01\r\n\x1a";
const REPLY_MAGIC: &[u8; 8] = b"WLBR\x01\r\n\x1a";
impl From<SessionError> for ExchangeError {
    fn from(value: SessionError) -> Self {
        Self::Session(value)
    }
}

fn valid_binding(binding: PlayerBinding) -> bool {
    binding.avatar_id != 0
        && binding.lot_location != 0
        && binding.source_epoch != 0
        && binding.lot_incarnation != 0
}
fn validate_call(call: &Call) -> Result<()> {
    if call.call_id == 0 || !valid_binding(call.binding) {
        return Err(ExchangeError::InvalidPacket);
    }
    let valid = match &call.command {
        Command::Quote(request) => {
            request.binding == call.binding && super::valid_request(request).is_ok()
        }
        Command::Confirm { operation, consent } => {
            *operation != 0 && consent.request_id != 0 && consent.cost >= 0
        }
        Command::Cancel {
            request_id,
            operation,
        } => *request_id != 0 && *operation != 0,
        Command::Status {
            request_id,
            operation,
        } => *request_id != 0 && operation.is_none_or(|n| n != 0),
    };
    if !valid {
        return Err(ExchangeError::InvalidPacket);
    }
    Ok(())
}
fn encode<T: Serialize>(magic: &[u8; 8], value: &T, limit: usize) -> Result<Vec<u8>> {
    let len = usize::try_from(
        codec(limit)
            .serialized_size(value)
            .map_err(|_| ExchangeError::InvalidPacket)?,
    )
    .map_err(|_| ExchangeError::InvalidPacket)?;
    if len > limit - 16 {
        return Err(ExchangeError::InvalidPacket);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len + 16)
        .map_err(|_| ExchangeError::InvalidPacket)?;
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&(len as u64).to_le_bytes());
    codec(len)
        .serialize_into(&mut bytes, value)
        .map_err(|_| ExchangeError::InvalidPacket)?;
    Ok(bytes)
}
fn decode<T: DeserializeOwned + Serialize>(
    magic: &[u8; 8],
    bytes: &[u8],
    limit: usize,
) -> Result<T> {
    if bytes.len() < 16 || bytes.len() > limit || &bytes[..8] != magic {
        return Err(ExchangeError::InvalidPacket);
    }
    let length = u64::from_le_bytes(
        bytes[8..16]
            .try_into()
            .map_err(|_| ExchangeError::InvalidPacket)?,
    );
    if length != (bytes.len() - 16) as u64 {
        return Err(ExchangeError::InvalidPacket);
    }
    let raw = &bytes[16..];
    // One bounded payload. Byte-vector echoes still consume item/credit budget;
    // this is admission accounting, not a process-wide allocator/RSS guarantee.
    let budget = guard::Budget::new(1_048_576, 64 * 1024 * 1024, 64);
    let value: T = codec(raw.len())
        .deserialize_seed(guard::ValueSeed::new(&budget), raw)
        .map_err(|_| ExchangeError::InvalidPacket)?;
    let mut canonical = CompareWriter { remaining: raw };
    codec(raw.len())
        .serialize_into(&mut canonical, &value)
        .map_err(|_| ExchangeError::InvalidPacket)?;
    if !canonical.remaining.is_empty() {
        return Err(ExchangeError::InvalidPacket);
    }
    Ok(value)
}
pub fn encode_call(call: &Call) -> Result<Vec<u8>> {
    validate_call(call)?;
    encode(CALL_MAGIC, call, MAX_CALL_BYTES)
}
pub fn decode_call(bytes: &[u8]) -> Result<Call> {
    let call = decode(CALL_MAGIC, bytes, MAX_CALL_BYTES)?;
    validate_call(&call)?;
    Ok(call)
}
fn validate_reply(reply: &Reply) -> Result<()> {
    let call = decode_call(&reply.request)?;
    let Some(snapshot) = &reply.snapshot else {
        return if matches!(call.command, Command::Status { .. }) {
            Ok(())
        } else {
            Err(ExchangeError::InvalidPacket)
        };
    };
    if snapshot.binding != call.binding
        || snapshot.operation == 0
        || snapshot.consent.request_id == 0
        || snapshot.consent.cost < 0
        || snapshot.expires_at_ms == 0
    {
        return Err(ExchangeError::InvalidPacket);
    }
    if let WirePhase::Outcome(BuildCommitStatus::Committed {
        created_objects, ..
    }) = &snapshot.phase
        && created_objects.len() > crate::sim_core::world::build::MAX_BUILD_EDITS
    {
        return Err(ExchangeError::InvalidPacket);
    }
    let matching = match call.command {
        Command::Quote(request) => {
            snapshot.consent.request_id == request.request_id && snapshot.phase == WirePhase::Quoted
        }
        Command::Confirm { operation, consent } => {
            snapshot.operation == operation
                && snapshot.consent == consent
                && matches!(
                    snapshot.phase,
                    WirePhase::Pending | WirePhase::Unknown | WirePhase::Outcome(_)
                )
        }
        Command::Cancel {
            request_id,
            operation,
        } => {
            snapshot.operation == operation
                && snapshot.consent.request_id == request_id
                && snapshot.phase == WirePhase::Cancelled
        }
        Command::Status {
            request_id,
            operation,
        } => {
            snapshot.consent.request_id == request_id
                && operation.is_none_or(|op| snapshot.operation == op)
        }
    };
    if !matching {
        return Err(ExchangeError::CorrelationMismatch);
    }
    Ok(())
}
pub fn encode_reply(reply: &Reply) -> Result<Vec<u8>> {
    validate_reply(reply)?;
    encode(REPLY_MAGIC, reply, MAX_REPLY_BYTES)
}
pub fn decode_reply(bytes: &[u8]) -> Result<Reply> {
    let reply = decode(REPLY_MAGIC, bytes, MAX_REPLY_BYTES)?;
    validate_reply(&reply)?;
    Ok(reply)
}
impl Snapshot {
    fn from_session(view: SessionView, tick: u64) -> Self {
        Self {
            binding: view.quote.binding,
            operation: view.quote.operation,
            consent: view.quote.consent,
            expires_at_ms: view.quote.expires_at_ms,
            tick,
            phase: match view.phase {
                Phase::Quoted => WirePhase::Quoted,
                Phase::Expired => WirePhase::Expired,
                Phase::Cancelled => WirePhase::Cancelled,
                Phase::Pending => WirePhase::Pending,
                Phase::Unknown => WirePhase::Unknown,
                Phase::Outcome(value) => WirePhase::Outcome(value),
            },
        }
    }
}
fn snapshot(
    session: &mut ConstructionSession,
    runtime: &GameRuntime,
    grant: &ConstructionGrant,
    now_ms: u64,
) -> Result<Option<Snapshot>> {
    Ok(session
        .refresh(runtime, grant, now_ms)?
        .map(|v| Snapshot::from_session(v, runtime.sim().state().completed_tick)))
}
/// Execute only on the authenticated, serialized lot executor. `quote_operation`
/// comes from the durable provider's idempotent reservation, never the call ID.
/// It must be Some for Quote and None otherwise. This does not implement HTTP,
/// CSRF/origin checks, a database journal, crash-safe deduplication or effect delivery.
/// The caller must cap bytes before buffering and deliver/record `admitted` even
/// when `reply` is an error. A transport failure is not permission to retry a write.
pub fn dispatch(
    session: &mut ConstructionSession,
    runtime: &mut GameRuntime,
    grant: &ConstructionGrant,
    bytes: &[u8],
    quote_operation: Option<u64>,
    now_ms: u64,
) -> Result<Handled> {
    let call = decode_call(bytes)?;
    if call.binding != grant.binding {
        return Err(ExchangeError::AdmissionChanged);
    }
    // Authenticate against the session's pinned principal, actor and world before
    // inspecting even a read-only quote. A client binding is not authentication.
    session.refresh(runtime, grant, now_ms)?;
    if matches!(call.command, Command::Quote(_)) != quote_operation.is_some()
        || quote_operation == Some(0)
    {
        return Err(ExchangeError::MissingOperation);
    }
    let mut admitted = None;
    match call.command {
        Command::Quote(request) => {
            session.offer(
                runtime,
                grant,
                quote_operation.ok_or(ExchangeError::MissingOperation)?,
                &request,
                now_ms,
            )?;
        }
        Command::Confirm { operation, consent } => {
            admitted = session.submit(runtime, grant, operation, consent, now_ms)?;
        }
        Command::Cancel {
            request_id,
            operation,
        } => {
            session.cancel(runtime, grant, request_id, operation, now_ms)?;
        }
        Command::Status {
            request_id,
            operation,
        } => {
            if let Some(view) = session.view()
                && (view.quote.consent.request_id != request_id
                    || operation.is_some_and(|op| op != view.quote.operation))
            {
                return Err(ExchangeError::CorrelationMismatch);
            }
        }
    }
    // Keep admitted outcomes available even if later status/encoding fails.
    let reply = snapshot(session, runtime, grant, now_ms).and_then(|snapshot| {
        let mut request = Vec::new();
        request
            .try_reserve_exact(bytes.len())
            .map_err(|_| ExchangeError::InvalidPacket)?;
        request.extend_from_slice(bytes);
        encode_reply(&Reply { request, snapshot })
    });
    Ok(Handled { reply, admitted })
}

pub mod client;
