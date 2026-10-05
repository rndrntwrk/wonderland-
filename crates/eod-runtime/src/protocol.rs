// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Versioned inbound UI protocol. Connection identity comes from the transport
//! authority, never from these bytes. This is a new native boundary, not the
//! legacy `VMNetEODMessageCmd` wire format.

use crate::{HostLimits, SourceObjectEvent, TimerVmEvent};
use std::fmt;

macro_rules! id {
    ($name:ident, $integer:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $integer);
    };
}
id!(ActorId, u64);
id!(ConnectionId, u64);
id!(HostScopeId, u64);
id!(InstanceId, u64);
id!(InvokerId, u32);
id!(PluginId, u32);

pub const PROTOCOL_VERSION: u16 = 1;
const HEADER_BYTES: usize = 57;

/// A logical instance belongs to one authoritative host scope. This address is
/// required for restored-session rebinds; a local instance number is insufficient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceAddress {
    pub host_scope: HostScopeId,
    pub instance: InstanceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionTicket {
    pub host_scope: HostScopeId,
    pub host_epoch: u64,
    pub instance: InstanceId,
    pub generation: u64,
}

impl SessionTicket {
    pub fn instance_address(self) -> InstanceAddress {
        InstanceAddress {
            host_scope: self.host_scope,
            instance: self.instance,
        }
    }
}

#[derive(Clone, Copy)]
pub enum WirePayload<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
}

impl fmt::Debug for WirePayload<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WirePayload([REDACTED])")
    }
}

impl WirePayload<'_> {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Binary(bytes) => bytes,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ClientMessage<'a> {
    pub version: u16,
    pub ticket: SessionTicket,
    pub plugin: PluginId,
    pub sequence: u64,
    pub event: &'a str,
    pub payload: WirePayload<'a>,
}

impl fmt::Debug for ClientMessage<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientMessage([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidLimits,
    InvalidIdentity,
    Unauthenticated,
    RecipientMismatch,
    StaleSession,
    WrongScope,
    WrongEpoch,
    WrongPlugin,
    UnregisteredPlugin,
    UnverifiedPlugin,
    ParticipantAlreadyConnected,
    AlreadyBound,
    ParticipantLimit,
    InstanceLimit,
    TimerLimit,
    MessageTooLarge,
    InvalidMessage,
    UnsupportedProtocolVersion,
    EventNotAllowed,
    UnexpectedSequence,
    RateLimited,
    QueueFull,
    CounterExhausted,
    MissingRegisters,
    CheckpointBusy,
    CheckpointTooLarge,
    InvalidCheckpoint,
    UnsupportedCheckpointVersion,
    UnsupportedPluginSchema,
    CheckpointStampMismatch,
    StoreFailure,
    EffectLimit,
    EffectConflict,
    UnknownEffect,
    ProviderReceiptMismatch,
    PluginInputRequired,
    InvalidPluginInput,
    PluginNotReady,
    NotAuthorized,
    ControllerAlreadyConnected,
    PersistedObjectBusy,
    PersistencePending,
    PersistenceLimit,
    PersistenceTooLarge,
    PersistenceUnavailable,
    InvalidPluginData,
    ReconciliationRequired,
    PersistenceDiverged,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    Accepted,
    Closed,
}

/// Private payload access exists only on explicitly private UI outputs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiBody<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
}
impl fmt::Debug for UiBody<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UiBody([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum PrivateBody {
    Text(String),
    Binary(Vec<u8>),
}

/// Must be delivered only through a private UI transport. There is deliberately
/// no serializer, conversion to `PublicVmEvent`, or non-redacted Debug output.
///
/// ```compile_fail
/// use wonderland_eod_runtime::{PrivateUiMessage, PublicVmEvent};
/// fn publish(message: PrivateUiMessage) -> PublicVmEvent { message.into() }
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct PrivateUiMessage {
    pub(crate) ticket: SessionTicket,
    pub(crate) plugin: PluginId,
    pub(crate) event: &'static str,
    pub(crate) body: PrivateBody,
}

impl PrivateUiMessage {
    pub fn ticket(&self) -> SessionTicket {
        self.ticket
    }
    pub fn plugin(&self) -> PluginId {
        self.plugin
    }
    pub fn event(&self) -> &'static str {
        self.event
    }
    pub fn body(&self) -> UiBody<'_> {
        match &self.body {
            PrivateBody::Text(text) => UiBody::Text(text),
            PrivateBody::Binary(bytes) => UiBody::Binary(bytes),
        }
    }
    pub(crate) fn size(&self) -> usize {
        self.event.len()
            + match &self.body {
                PrivateBody::Text(text) => text.len(),
                PrivateBody::Binary(bytes) => bytes.len(),
            }
    }
}

impl fmt::Debug for PrivateUiMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateUiMessage([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicVmEvent {
    Connected {
        invoker: InvokerId,
    },
    Disconnected {
        invoker: InvokerId,
    },
    Timer {
        invoker: InvokerId,
        event: TimerVmEvent,
    },
    DanceFloor {
        controller: InvokerId,
        button: u8,
        avatar_object: i16,
    },
    SourcePlugin {
        invoker: InvokerId,
        event: SourceObjectEvent,
    },
}

impl PublicVmEvent {
    pub fn source_event(&self) -> (InvokerId, i16, Vec<i16>) {
        match self {
            Self::Connected { invoker } => (*invoker, -2, vec![]),
            Self::Disconnected { invoker } => (*invoker, -1, vec![]),
            Self::Timer { invoker, event } => {
                let (code, arguments) = event.source_event();
                (*invoker, code, arguments)
            }
            Self::DanceFloor {
                controller,
                button,
                avatar_object,
            } => (*controller, i16::from(*button), vec![*avatar_object]),
            Self::SourcePlugin { invoker, event } => {
                let (code, arguments) = event.source_event();
                (*invoker, code, arguments)
            }
        }
    }
}

/// Deliberately excludes recipients, sessions, timers, RNG, checkpoints and effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmProjection {
    pub tick: u64,
}

pub(crate) fn validate(message: &ClientMessage<'_>, limits: &HostLimits) -> Result<usize, Error> {
    if message.version != PROTOCOL_VERSION {
        return Err(Error::UnsupportedProtocolVersion);
    }
    if message.event.is_empty() || !message.event.is_ascii() {
        return Err(Error::InvalidMessage);
    }
    if message.event.len() > limits.max_event_bytes || message.event.len() > u16::MAX as usize {
        return Err(Error::MessageTooLarge);
    }
    let length = HEADER_BYTES
        .checked_add(message.event.len())
        .and_then(|v| v.checked_add(message.payload.bytes().len()))
        .ok_or(Error::MessageTooLarge)?;
    if length > limits.max_message_bytes || message.payload.bytes().len() > u32::MAX as usize {
        return Err(Error::MessageTooLarge);
    }
    Ok(length)
}

pub fn encode(message: &ClientMessage<'_>, limits: &HostLimits) -> Result<Vec<u8>, Error> {
    limits.validate()?;
    let size = validate(message, limits)?;
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(b"EODI");
    bytes.extend_from_slice(&message.version.to_le_bytes());
    bytes.extend_from_slice(&message.ticket.host_scope.0.to_le_bytes());
    bytes.extend_from_slice(&message.ticket.host_epoch.to_le_bytes());
    bytes.extend_from_slice(&message.ticket.instance.0.to_le_bytes());
    bytes.extend_from_slice(&message.ticket.generation.to_le_bytes());
    bytes.extend_from_slice(&message.plugin.0.to_le_bytes());
    bytes.extend_from_slice(&message.sequence.to_le_bytes());
    bytes.push(match message.payload {
        WirePayload::Text(_) => 0,
        WirePayload::Binary(_) => 1,
    });
    bytes.extend_from_slice(&(message.event.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(message.payload.bytes().len() as u32).to_le_bytes());
    bytes.extend_from_slice(message.event.as_bytes());
    bytes.extend_from_slice(message.payload.bytes());
    Ok(bytes)
}

/// Borrows input after checking the frame bound; no input-derived allocation.
/// Extra fields, including a forged recipient or Verified flag, are rejected.
pub fn decode<'a>(bytes: &'a [u8], limits: &HostLimits) -> Result<ClientMessage<'a>, Error> {
    limits.validate()?;
    if bytes.len() > limits.max_message_bytes {
        return Err(Error::MessageTooLarge);
    }
    if bytes.len() < HEADER_BYTES || &bytes[..4] != b"EODI" {
        return Err(Error::InvalidMessage);
    }
    let u64_at = |offset| {
        u64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .expect("fixed checked header"),
        )
    };
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != PROTOCOL_VERSION {
        return Err(Error::UnsupportedProtocolVersion);
    }
    let event_length = u16::from_le_bytes([bytes[51], bytes[52]]) as usize;
    let payload_length =
        u32::from_le_bytes(bytes[53..57].try_into().expect("fixed checked header")) as usize;
    if event_length > limits.max_event_bytes {
        return Err(Error::MessageTooLarge);
    }
    let payload_start = HEADER_BYTES
        .checked_add(event_length)
        .ok_or(Error::InvalidMessage)?;
    let end = payload_start
        .checked_add(payload_length)
        .ok_or(Error::InvalidMessage)?;
    if end != bytes.len() {
        return Err(Error::InvalidMessage);
    }
    let event = std::str::from_utf8(&bytes[HEADER_BYTES..payload_start])
        .map_err(|_| Error::InvalidMessage)?;
    let payload = match bytes[50] {
        0 => WirePayload::Text(
            std::str::from_utf8(&bytes[payload_start..end]).map_err(|_| Error::InvalidMessage)?,
        ),
        1 => WirePayload::Binary(&bytes[payload_start..end]),
        _ => return Err(Error::InvalidMessage),
    };
    let message = ClientMessage {
        version,
        ticket: SessionTicket {
            host_scope: HostScopeId(u64_at(6)),
            host_epoch: u64_at(14),
            instance: InstanceId(u64_at(22)),
            generation: u64_at(30),
        },
        plugin: PluginId(u32::from_le_bytes(
            bytes[38..42].try_into().expect("fixed checked header"),
        )),
        sequence: u64_at(42),
        event,
        payload,
    };
    validate(&message, limits)?;
    Ok(message)
}
