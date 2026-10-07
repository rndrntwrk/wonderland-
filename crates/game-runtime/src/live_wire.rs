//! Versioned native server packets and generation-bound delivery to `LiveReplica`.
//!
//! This is NOT the FreeSO FSOv/VMNet ABI. The caller authenticates its authority,
//! caps WebSocket message sizes before buffering, and binds callbacks to the
//! connection token captured when that socket was created. JS must pass bytes,
//! never parse native u64 fields through Number. No local intent is executed here.
use crate::live_session::{
    Checkpoint, CheckpointTicket, ConnectionToken, LiveError, LiveReplica, ReplayLimits,
    ReplicaCursor, SessionStatus, TickFrame,
};
use crate::sim_core::snapshot::{SNAPSHOT_CHECKSUM_LEN, SNAPSHOT_HEADER_LEN};
use crate::{AcceptedTick, TickOutcome};
use bincode::Options;

mod guard;
pub mod player;
pub const HEADER_LEN: usize = 32;
pub const MAGIC: [u8; 8] = *b"WLR1\r\n\x1a\n";
const CHECKPOINT: u8 = 1;
const TICKS: u8 = 2;
const CHECKPOINT_PREFIX: usize = 44;
const RECORD_PREFIX: usize = 36;

#[derive(Clone, Copy, Debug)]
pub struct WireLimits {
    pub replay: ReplayLimits,
    pub max_tick_bytes: usize,
    /// Work and conservative allocation-admission credit across the entire batch.
    /// This is not a whole-process allocator/RSS limit.
    pub max_decode_items: usize,
    pub max_decode_credit: usize,
    pub max_decode_depth: usize,
}
impl Default for WireLimits {
    fn default() -> Self {
        Self {
            replay: ReplayLimits::default(),
            max_tick_bytes: 1024 * 1024,
            max_decode_items: 262_144,
            max_decode_credit: 32 * 1024 * 1024,
            max_decode_depth: 64,
        }
    }
}
impl WireLimits {
    fn validate(self) -> Result<(), WireError> {
        let defaults = ReplayLimits::default();
        if self.replay.max_batch_ticks == 0
            || self.replay.max_batch_ticks > 256
            || self.replay.max_batch_commands == 0
            || self.replay.max_batch_commands > 65536
            || self.replay.max_batch_bytes < 8
            || self.replay.max_batch_bytes > defaults.max_batch_bytes
            || self.replay.max_checkpoint_bytes <= SNAPSHOT_HEADER_LEN + SNAPSHOT_CHECKSUM_LEN
            || self.replay.max_checkpoint_bytes > defaults.max_checkpoint_bytes
            || self.max_tick_bytes == 0
            || self.max_tick_bytes > defaults.max_batch_bytes
            || self.max_decode_items == 0
            || self.max_decode_items > 1_048_576
            || self.max_decode_credit == 0
            || self.max_decode_credit > 128 * 1024 * 1024
            || self.max_decode_depth == 0
            || self.max_decode_depth > 128
        {
            return Err(WireError::InvalidLimits);
        }
        Ok(())
    }
    pub fn max_packet_bytes(self) -> Result<usize, WireError> {
        self.validate()?;
        self.replay
            .max_checkpoint_bytes
            .checked_add(self.replay.max_batch_bytes)
            .and_then(|n| n.checked_add(CHECKPOINT_PREFIX + HEADER_LEN))
            .ok_or(WireError::Bounds)
    }
}
#[derive(Debug)]
pub enum WireError {
    InvalidLimits,
    Header,
    Kind,
    Correlation,
    Bounds,
    Allocation,
    Decode(String),
    StaleConnection,
    StaleResponse,
    NotConnected,
    CounterExhausted,
    Live(LiveError),
}
impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native wire: {self:?}")
    }
}
impl std::error::Error for WireError {}
impl From<LiveError> for WireError {
    fn from(error: LiveError) -> Self {
        Self::Live(error)
    }
}
fn codec(limit: usize) -> impl Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(limit as u64)
        .reject_trailing_bytes()
}
fn decode_error(error: impl std::fmt::Display) -> WireError {
    WireError::Decode(error.to_string())
}

#[derive(Debug)]
pub enum Packet<'a> {
    Checkpoint {
        request_id: u64,
        checkpoint: Checkpoint<'a>,
        tail: Vec<TickFrame>,
    },
    Ticks(Vec<TickFrame>),
}
struct Reader<'a> {
    bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        if n > self.bytes.len() {
            return Err(WireError::Bounds);
        }
        let (value, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(value)
    }
    fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| WireError::Bounds)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| WireError::Bounds)?,
        ))
    }
    fn hash(&mut self) -> Result<[u8; 32], WireError> {
        self.take(32)?.try_into().map_err(|_| WireError::Bounds)
    }
}
fn header(bytes: &[u8], limits: WireLimits) -> Result<(u8, u64, &[u8]), WireError> {
    limits.validate()?;
    if bytes.len() < HEADER_LEN || bytes[..8] != MAGIC || bytes[9..16].iter().any(|b| *b != 0) {
        return Err(WireError::Header);
    }
    let kind = bytes[8];
    if kind != CHECKPOINT && kind != TICKS {
        return Err(WireError::Kind);
    }
    let mut input = Reader {
        bytes: &bytes[16..],
    };
    let request = input.u64()?;
    if (kind == CHECKPOINT && request == 0) || (kind == TICKS && request != 0) {
        return Err(WireError::Correlation);
    }
    let size = usize::try_from(input.u64()?).map_err(|_| WireError::Bounds)?;
    let maximum = if kind == CHECKPOINT {
        limits.max_packet_bytes()? - HEADER_LEN
    } else {
        limits.replay.max_batch_bytes
    };
    if size != input.bytes.len() || size > maximum {
        return Err(WireError::Bounds);
    }
    Ok((kind, request, input.bytes))
}
fn decode_block(bytes: &[u8], limits: WireLimits) -> Result<Vec<TickFrame>, WireError> {
    if bytes.len() > limits.replay.max_batch_bytes {
        return Err(WireError::Bounds);
    }
    let mut input = Reader { bytes };
    let count = input.u32()? as usize;
    if count > limits.replay.max_batch_ticks || count > input.bytes.len() / RECORD_PREFIX {
        return Err(WireError::Bounds);
    }
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(count)
        .map_err(|_| WireError::Allocation)?;
    let budget = guard::Budget::new(
        limits.max_decode_items,
        limits.max_decode_credit,
        limits.max_decode_depth,
    );
    let mut commands = 0usize;
    for _ in 0..count {
        let state_hash = input.hash()?;
        let length = input.u32()? as usize;
        if length == 0 || length > limits.max_tick_bytes {
            return Err(WireError::Bounds);
        }
        let raw = input.take(length)?;
        let accepted: AcceptedTick = codec(length)
            .deserialize_seed(guard::ValueSeed::new(&budget), raw)
            .map_err(decode_error)?;
        commands = commands
            .checked_add(accepted.commands.len())
            .ok_or(WireError::Bounds)?;
        if commands > limits.replay.max_batch_commands {
            return Err(WireError::Bounds);
        }
        // Exact encoding rejects map/set aliasing, duplicate keys and noncanonical
        // native representations before exposing them to the deterministic VM.
        let mut canonical = CompareWriter { remaining: raw };
        codec(length)
            .serialize_into(&mut canonical, &accepted)
            .map_err(decode_error)?;
        if !canonical.remaining.is_empty() {
            return Err(WireError::Decode("noncanonical accepted tick".into()));
        }
        frames.push(TickFrame {
            accepted,
            state_hash,
        });
    }
    if !input.bytes.is_empty() {
        return Err(WireError::Bounds);
    }
    Ok(frames)
}
pub fn decode_packet(bytes: &[u8], limits: WireLimits) -> Result<Packet<'_>, WireError> {
    let (kind, request_id, body) = header(bytes, limits)?;
    if kind == TICKS {
        return Ok(Packet::Ticks(decode_block(body, limits)?));
    }
    let mut input = Reader { bytes: body };
    let completed_tick = input.u64()?;
    let state_hash = input.hash()?;
    let length = input.u32()? as usize;
    if length == 0 || length > limits.replay.max_checkpoint_bytes {
        return Err(WireError::Bounds);
    }
    let bytes = input.take(length)?;
    let tail = decode_block(input.bytes, limits)?;
    Ok(Packet::Checkpoint {
        request_id,
        checkpoint: Checkpoint {
            completed_tick,
            state_hash,
            bytes,
        },
        tail,
    })
}
// Compare canonical serialization without allocating a second tick buffer.
struct CompareWriter<'a> {
    remaining: &'a [u8],
}
impl std::io::Write for CompareWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if !self.remaining.starts_with(bytes) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "noncanonical accepted tick",
            ));
        }
        self.remaining = &self.remaining[bytes.len()..];
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn block_size(frames: &[TickFrame], limits: WireLimits) -> Result<usize, WireError> {
    limits.validate()?;
    if frames.len() > limits.replay.max_batch_ticks {
        return Err(WireError::Bounds);
    }
    let mut size = 4usize;
    let mut commands = 0usize;
    for frame in frames {
        commands = commands
            .checked_add(frame.accepted.commands.len())
            .ok_or(WireError::Bounds)?;
        if commands > limits.replay.max_batch_commands {
            return Err(WireError::Bounds);
        }
        let n = usize::try_from(
            codec(limits.max_tick_bytes)
                .serialized_size(&frame.accepted)
                .map_err(decode_error)?,
        )
        .map_err(|_| WireError::Bounds)?;
        if n > limits.max_tick_bytes {
            return Err(WireError::Bounds);
        }
        size = size
            .checked_add(RECORD_PREFIX)
            .and_then(|s| s.checked_add(n))
            .ok_or(WireError::Bounds)?;
        if size > limits.replay.max_batch_bytes {
            return Err(WireError::Bounds);
        }
    }
    Ok(size)
}
fn start_packet(kind: u8, request: u64, size: usize) -> Result<Vec<u8>, WireError> {
    let total = size.checked_add(HEADER_LEN).ok_or(WireError::Bounds)?;
    let mut out = Vec::new();
    out.try_reserve_exact(total)
        .map_err(|_| WireError::Allocation)?;
    out.extend_from_slice(&MAGIC);
    out.push(kind);
    out.extend_from_slice(&[0; 7]);
    out.extend_from_slice(&request.to_le_bytes());
    out.extend_from_slice(&(size as u64).to_le_bytes());
    Ok(out)
}
fn write_block(
    out: &mut Vec<u8>,
    frames: &[TickFrame],
    limits: WireLimits,
) -> Result<(), WireError> {
    out.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for frame in frames {
        let n = codec(limits.max_tick_bytes)
            .serialized_size(&frame.accepted)
            .map_err(decode_error)?;
        let n = u32::try_from(n).map_err(|_| WireError::Bounds)?;
        out.extend_from_slice(&frame.state_hash);
        out.extend_from_slice(&n.to_le_bytes());
        codec(limits.max_tick_bytes)
            .serialize_into(&mut *out, &frame.accepted)
            .map_err(decode_error)?;
    }
    Ok(())
}
pub fn encode_ticks(frames: &[TickFrame], limits: WireLimits) -> Result<Vec<u8>, WireError> {
    let size = block_size(frames, limits)?;
    let mut out = start_packet(TICKS, 0, size)?;
    write_block(&mut out, frames, limits)?;
    Ok(out)
}
pub fn encode_checkpoint(
    request_id: u64,
    checkpoint: Checkpoint<'_>,
    tail: &[TickFrame],
    limits: WireLimits,
) -> Result<Vec<u8>, WireError> {
    if request_id == 0 {
        return Err(WireError::Correlation);
    }
    let block = block_size(tail, limits)?;
    if checkpoint.bytes.is_empty() || checkpoint.bytes.len() > limits.replay.max_checkpoint_bytes {
        return Err(WireError::Bounds);
    }
    let size = CHECKPOINT_PREFIX
        .checked_add(checkpoint.bytes.len())
        .and_then(|n| n.checked_add(block))
        .ok_or(WireError::Bounds)?;
    let mut out = start_packet(CHECKPOINT, request_id, size)?;
    out.extend_from_slice(&checkpoint.completed_tick.to_le_bytes());
    out.extend_from_slice(&checkpoint.state_hash);
    out.extend_from_slice(&(checkpoint.bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(checkpoint.bytes);
    write_block(&mut out, tail, limits)?;
    Ok(out)
}

#[derive(Clone, Copy, Debug)]
pub struct CheckpointRequest {
    pub id: u64,
    pub cursor: Option<ReplicaCursor>,
}
#[derive(Debug)]
pub enum Received {
    Checkpoint(ReplicaCursor),
    Ticks(Vec<TickOutcome>),
}

/// Owns one replica and its socket-generation/correlated-recovery handoff.
/// No authentication is inferred from native header fields, and no callback may
/// obtain a fresh token at delivery time. Capture `connection()` on socket creation.
pub struct NativeWire {
    replica: LiveReplica,
    limits: WireLimits,
    last_request: u64,
    pending: Option<(CheckpointRequest, CheckpointTicket)>,
}
impl NativeWire {
    pub fn new(mut replica: LiveReplica, limits: WireLimits) -> Result<Self, WireError> {
        limits.validate()?;
        let ticket = match replica.checkpoint_ticket() {
            Some(ticket) => ticket,
            None => replica.request_checkpoint(replica.connection())?,
        };
        let request = CheckpointRequest {
            id: 1,
            cursor: replica.cursor(),
        };
        Ok(Self {
            replica,
            limits,
            last_request: 1,
            pending: Some((request, ticket)),
        })
    }
    pub fn replica(&self) -> &LiveReplica {
        &self.replica
    }
    pub fn connection(&self) -> ConnectionToken {
        self.replica.connection()
    }
    pub fn checkpoint_request(&self) -> Option<CheckpointRequest> {
        self.pending.map(|p| p.0)
    }
    pub fn close(&mut self) {
        self.replica.close();
        self.pending = None;
    }
    fn bind_request(&mut self) -> Result<(), WireError> {
        self.pending = None;
        let Some(ticket) = self.replica.checkpoint_ticket() else {
            return Ok(());
        };
        let Some(id) = self.last_request.checked_add(1) else {
            self.close();
            return Err(WireError::CounterExhausted);
        };
        self.last_request = id;
        self.pending = Some((
            CheckpointRequest {
                id,
                cursor: self.replica.cursor(),
            },
            ticket,
        ));
        Ok(())
    }
    fn check_token(&self, token: ConnectionToken) -> Result<(), WireError> {
        if token != self.connection() {
            return Err(WireError::StaleConnection);
        }
        if matches!(
            self.replica.status(),
            SessionStatus::Closed | SessionStatus::Suspended
        ) {
            return Err(WireError::NotConnected);
        }
        Ok(())
    }
    pub fn disconnect(&mut self, token: ConnectionToken) -> Result<(), WireError> {
        self.check_token(token)?;
        self.replica.suspend(token)?;
        self.pending = None;
        Ok(())
    }
    pub fn reconnect(&mut self) -> Result<ConnectionToken, WireError> {
        let token = self.replica.reconnect()?;
        self.bind_request()?;
        Ok(token)
    }
    fn fault(&mut self, token: ConnectionToken, error: WireError) -> WireError {
        let _ = self.replica.suspend(token);
        self.pending = None;
        error
    }
    pub fn receive(&mut self, token: ConnectionToken, bytes: &[u8]) -> Result<Received, WireError> {
        self.receive_inner(token, bytes, false)
            .map(|(update, _)| update)
    }
    /// Like receive, plus bounded per-tick avatar projections for a presentation
    /// consumer. Checkpoints contain only the final recovered frame, never a
    /// speculative replay of pre-checkpoint visual history.
    pub fn receive_with_avatar_frames(
        &mut self,
        token: ConnectionToken,
        bytes: &[u8],
    ) -> Result<(Received, Option<Vec<crate::AvatarVisualFrame>>), WireError> {
        self.receive_inner(token, bytes, true)
    }
    fn receive_inner(
        &mut self,
        token: ConnectionToken,
        bytes: &[u8],
        capture: bool,
    ) -> Result<(Received, Option<Vec<crate::AvatarVisualFrame>>), WireError> {
        // Stale callbacks cannot parse or suspend a replacement connection.
        self.check_token(token)?;
        let (kind, id, _) = header(bytes, self.limits).map_err(|e| self.fault(token, e))?;
        if kind == CHECKPOINT && self.pending.is_none_or(|p| p.0.id != id) {
            return Err(WireError::StaleResponse);
        }
        if kind == TICKS && self.replica.status() != SessionStatus::Live {
            return Err(self.fault(token, WireError::NotConnected));
        }
        let packet = decode_packet(bytes, self.limits).map_err(|e| self.fault(token, e))?;
        match packet {
            Packet::Checkpoint {
                checkpoint, tail, ..
            } => {
                let Some((_, ticket)) = self.pending else {
                    return Err(WireError::StaleResponse);
                };
                let cursor = self
                    .replica
                    .install_checkpoint(ticket, checkpoint, &tail)
                    .map_err(|e| self.fault(token, e.into()))?;
                self.pending = None;
                let mut visuals = crate::avatar_projection::AvatarCapture::new(capture);
                if let Some(runtime) = self.replica.runtime() {
                    visuals.observe(runtime);
                }
                Ok((Received::Checkpoint(cursor), visuals.finish()))
            }
            Packet::Ticks(frames) => match if capture {
                self.replica.apply_batch_with_avatar_frames(token, &frames)
            } else {
                self.replica
                    .apply_batch(token, &frames)
                    .map(|outcomes| (outcomes, None))
            } {
                Ok((outcomes, frames)) => Ok((Received::Ticks(outcomes), frames)),
                Err(error) => {
                    self.bind_request()?;
                    Err(error.into())
                }
            },
        }
    }
}

/// Bounded client recovery request; its cursor is a hint, never authority.
/// A server must authenticate the socket and select the admitted lot itself.
pub fn encode_checkpoint_request(request: CheckpointRequest) -> Result<Vec<u8>, WireError> {
    if request.id == 0 {
        return Err(WireError::Correlation);
    }
    let mut out = start_packet(3, request.id, if request.cursor.is_some() { 57 } else { 1 })?;
    if let Some(cursor) = request.cursor {
        out.push(1);
        out.extend_from_slice(&cursor.lot_id.to_le_bytes());
        out.extend_from_slice(&cursor.authority_epoch.to_le_bytes());
        out.extend_from_slice(&cursor.completed_tick.to_le_bytes());
        out.extend_from_slice(&cursor.state_hash);
    } else {
        out.push(0);
    }
    Ok(out)
}

/// Decode the client-only request without allocating; reject server packet kinds.
/// The maximum complete request is 89 bytes, including its fixed 32-byte header.
pub fn decode_checkpoint_request(bytes: &[u8]) -> Result<CheckpointRequest, WireError> {
    if !matches!(bytes.len(), 33 | 89)
        || bytes[..8] != MAGIC
        || bytes[8] != 3
        || bytes[9..16].iter().any(|v| *v != 0)
    {
        return Err(WireError::Header);
    }
    let mut input = Reader {
        bytes: &bytes[16..],
    };
    let id = input.u64()?;
    if id == 0 {
        return Err(WireError::Correlation);
    }
    if input.u64()? != input.bytes.len() as u64 {
        return Err(WireError::Bounds);
    }
    let cursor = match input.take(1)?[0] {
        0 => None,
        1 => Some(ReplicaCursor {
            lot_id: input.u64()?,
            authority_epoch: input.u64()?,
            completed_tick: input.u64()?,
            state_hash: input.hash()?,
        }),
        _ => return Err(WireError::Header),
    };
    if !input.bytes.is_empty() {
        return Err(WireError::Bounds);
    }
    Ok(CheckpointRequest { id, cursor })
}
