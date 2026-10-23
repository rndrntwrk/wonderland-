// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Private, candidate-only Timer delivery custody. This is deliberately separate
//! from the output-barrier checkpoint contract. A sink stages one opaque value;
//! the coordinator must atomically commit it with its simulation cursor before
//! publishing this candidate or releasing any output.

use crate::{host::HandlerState, protocol::PrivateBody, registry::TIMER_PLUGIN, *};
use std::{collections::BTreeMap, fmt};

/// A host-scope-local monotonic delivery identity, preserved across host epochs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrivateTimerOutputId(pub u64);

/// Historical destination evidence, never a grant of transport authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateTimerRecipient {
    pub actor: ActorId,
    pub connection: ConnectionId,
    pub ticket: SessionTicket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateTimerDispositionKind {
    Delivered,
    Superseded,
    Suppressed,
}

/// Durable terminal evidence supplied by the authoritative delivery coordinator.
/// The bytes remain private and are bounded; Debug never prints them.
#[derive(Clone, PartialEq, Eq)]
pub struct PrivateTimerDisposition {
    kind: PrivateTimerDispositionKind,
    receipt_key: u64,
    receipt: Vec<u8>,
}
impl PrivateTimerDisposition {
    pub fn kind(&self) -> PrivateTimerDispositionKind {
        self.kind
    }
    pub fn receipt_key(&self) -> u64 {
        self.receipt_key
    }
    /// Available only on the explicitly private delivery boundary.
    pub fn private_receipt(&self) -> &[u8] {
        &self.receipt
    }
}
impl fmt::Debug for PrivateTimerDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateTimerDisposition([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq)]
enum DeliveryBody {
    Public(PublicVmEvent),
    Private {
        recipient: PrivateTimerRecipient,
        message: PrivateUiMessage,
    },
}

/// An immutable original output plus an optional exact terminal receipt. The
/// origin ticket is captured at emission, including for close/timeout outputs.
/// Its instance/generation must be mapped to the source association at creation,
/// never by looking up the invoker's latest association at delivery time.
#[derive(Clone, PartialEq, Eq)]
pub struct PrivateTimerDelivery {
    id: PrivateTimerOutputId,
    origin: SessionTicket,
    body: DeliveryBody,
    disposition: Option<PrivateTimerDisposition>,
}
impl PrivateTimerDelivery {
    pub fn id(&self) -> PrivateTimerOutputId {
        self.id
    }
    pub fn origin_ticket(&self) -> SessionTicket {
        self.origin
    }
    pub fn public_event(&self) -> Option<&PublicVmEvent> {
        match &self.body {
            DeliveryBody::Public(event) => Some(event),
            _ => None,
        }
    }
    pub fn private_message(&self) -> Option<&PrivateUiMessage> {
        match &self.body {
            DeliveryBody::Private { message, .. } => Some(message),
            _ => None,
        }
    }
    pub fn private_recipient(&self) -> Option<PrivateTimerRecipient> {
        match self.body {
            DeliveryBody::Private { recipient, .. } => Some(recipient),
            _ => None,
        }
    }
    pub fn disposition(&self) -> Option<&PrivateTimerDisposition> {
        self.disposition.as_ref()
    }
}
impl fmt::Debug for PrivateTimerDelivery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateTimerDelivery([REDACTED])")
    }
}

/// One callback contains BOTH exact handler state and output custody. The sink
/// must keep these bytes private, stage them atomically with its own association
/// and receipt mappings, and compare its trusted predecessor before commit.
/// Success means staged, not delivered and not committed to the simulation.
/// No public VM snapshot/output channel may implement this private boundary.
pub trait PrivateTimerTransactionSink {
    fn stage_private_transaction(
        &mut self,
        stamp: CheckpointStamp,
        opaque_checkpoint: &[u8],
        deliveries: &[PrivateTimerDelivery],
    ) -> Result<(), StoreError>;
}

/// A bounded reader of the single opaque value previously passed to the sink.
/// Authenticate the stored value and obtain `expected` from trusted latest
/// coordinator state. A client-supplied stamp is not rollback protection.
pub trait PrivateTimerTransactionStore {
    fn read_private_transaction(
        &mut self,
        scope: HostScopeId,
        destination: &mut [u8],
    ) -> Result<PrivateRead, StoreError>;
}

/// Candidate state, not a live/published host. Keep the committed predecessor
/// alive until the outer coordinator has atomically committed this candidate.
/// All retained delivery records, including receipts, count against the host's
/// queue bounds. Exhaustion fails closed; there is no lossy receipt eviction.
pub struct PrivateTimerTransaction {
    host: NativeHost,
    next_output: u64,
    deliveries: Vec<PrivateTimerDelivery>,
    staged: Option<CheckpointStamp>,
}
impl fmt::Debug for PrivateTimerTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateTimerTransaction([REDACTED])")
    }
}

impl NativeHost {
    /// Same-epoch fork in memory, not a restore-authorization exception. Legacy
    /// native queues have no public origin ticket and are therefore ambiguous;
    /// start this boundary before producing outputs, or at a genuine completed
    /// output barrier. Merely draining legacy queues does not prove delivery.
    pub fn fork_private_timer_transaction(&self) -> Result<PrivateTimerTransaction, Error> {
        ensure_timer_only(self)?;
        if !self.state.public.is_empty() || !self.state.private.is_empty() {
            return Err(Error::CheckpointBusy);
        }
        Ok(PrivateTimerTransaction {
            host: copy_host(self),
            next_output: 1,
            deliveries: vec![],
            staged: None,
        })
    }
}

impl PrivateTimerTransaction {
    pub fn fork(&self) -> Self {
        Self {
            host: copy_host(&self.host),
            next_output: self.next_output,
            deliveries: self.deliveries.clone(),
            staged: self.staged,
        }
    }
    pub fn identity(&self) -> HostIdentity {
        self.host.identity
    }
    pub fn public_projection(&self) -> VmProjection {
        self.host.public_projection()
    }
    /// Read-only private delivery custody; reading never acknowledges anything.
    pub fn private_deliveries(&self) -> &[PrivateTimerDelivery] {
        &self.deliveries
    }
    pub fn current_ticket(&self, invoker: InvokerId) -> Option<SessionTicket> {
        bindings(&self.host).get(&invoker).copied()
    }

    pub fn connect(
        &mut self,
        authority: &impl ConnectionAuthority,
        request: ConnectRequest,
    ) -> Result<SessionTicket, Error> {
        if request.plugin != TIMER_PLUGIN {
            return Err(Error::WrongPlugin);
        }
        self.mutate(|host| host.connect(authority, request))
    }
    pub fn receive(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        message: ClientMessage<'_>,
    ) -> Result<DispatchOutcome, Error> {
        self.mutate(|host| host.receive(authority, connection, message))
    }
    pub fn receive_bytes(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        bytes: &[u8],
    ) -> Result<DispatchOutcome, Error> {
        let message = protocol::decode(bytes, &self.host.limits)?;
        self.receive(authority, connection, message)
    }
    pub fn rebind(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        address: InstanceAddress,
        registers: TimerRegisters,
    ) -> Result<SessionTicket, Error> {
        self.mutate(|host| host.rebind(authority, connection, address, registers))
    }
    /// The authoritative adapter must first fence its source association. This
    /// additional exact native ticket prevents a stale close from affecting a
    /// reopened invoker. It invokes the existing NativeHost disconnect handler.
    pub fn disconnect_invoker(
        &mut self,
        invoker: InvokerId,
        expected: SessionTicket,
    ) -> Result<(), Error> {
        self.check_domain(expected)?;
        if self.current_ticket(invoker) != Some(expected) {
            return Err(Error::StaleSession);
        }
        self.mutate(|host| host.disconnect_invoker(invoker))
    }
    pub fn tick(
        &mut self,
        authority: &impl ConnectionAuthority,
        registers: &impl RegisterSource,
    ) -> Result<(), Error> {
        self.mutate(|host| host.tick(authority, registers))
    }

    /// Only call after the exact public delivery was durably applied to the
    /// source association. The receipt is the coordinator's application proof.
    pub fn record_public_delivery(
        &mut self,
        id: PrivateTimerOutputId,
        receipt_key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        if self.delivery(id)?.public_event().is_none() {
            return Err(Error::WrongPlugin);
        }
        self.record(
            id,
            PrivateTimerDispositionKind::Delivered,
            receipt_key,
            receipt,
        )
    }

    /// The coordinator must prove the exact original source association was
    /// durably cleared, so this event cannot be applied. This is supersession,
    /// never successful VM delivery or an inference from a missing invoker.
    pub fn supersede_public(
        &mut self,
        id: PrivateTimerOutputId,
        expected_origin: SessionTicket,
        receipt_key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        let delivery = self.delivery(id)?;
        if delivery.public_event().is_none() {
            return Err(Error::WrongPlugin);
        }
        if delivery.origin != expected_origin {
            return Err(Error::StaleSession);
        }
        self.record(
            id,
            PrivateTimerDispositionKind::Superseded,
            receipt_key,
            receipt,
        )
    }

    /// An authenticated acknowledgement, never a socket-send acknowledgement.
    /// Old-epoch outputs cannot be acknowledged by a successor transport.
    pub fn acknowledge_private(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        ticket: SessionTicket,
        id: PrivateTimerOutputId,
        receipt_key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        self.check_domain(ticket)?;
        let actor = authority
            .authenticated_actor(connection)
            .filter(|actor| actor.0 != 0)
            .ok_or(Error::Unauthenticated)?;
        let expected = PrivateTimerRecipient {
            actor,
            connection,
            ticket,
        };
        if self.delivery(id)?.private_recipient() != Some(expected) {
            return Err(Error::RecipientMismatch);
        }
        self.record(
            id,
            PrivateTimerDispositionKind::Delivered,
            receipt_key,
            receipt,
        )
    }

    /// Authoritative, association-fenced coordinator evidence that this original
    /// output was replaced by a committed resync. This does not mean delivered.
    pub fn supersede_private(
        &mut self,
        id: PrivateTimerOutputId,
        expected: PrivateTimerRecipient,
        receipt_key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        self.dispose_private(
            id,
            expected,
            PrivateTimerDispositionKind::Superseded,
            receipt_key,
            receipt,
        )
    }
    /// Authoritative revocation/obsolete-association evidence. No inference from
    /// connection absence or elapsed time is sufficient to call this method.
    pub fn suppress_private(
        &mut self,
        id: PrivateTimerOutputId,
        expected: PrivateTimerRecipient,
        receipt_key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        self.dispose_private(
            id,
            expected,
            PrivateTimerDispositionKind::Suppressed,
            receipt_key,
            receipt,
        )
    }

    /// The only checkpoint export is this explicitly private sink callback. A
    /// retry without a mutation produces exactly the same stamp and bytes. A
    /// failed sink leaves the candidate's revision and all output custody intact.
    pub fn stage_private_transaction(
        &mut self,
        sink: &mut impl PrivateTimerTransactionSink,
    ) -> Result<CheckpointStamp, Error> {
        let stamp = match self.staged {
            Some(stamp) => stamp,
            None => CheckpointStamp {
                scope: self.host.identity.scope,
                epoch: self.host.identity.epoch,
                revision: self
                    .host
                    .state
                    .checkpoint_revision
                    .checked_add(1)
                    .ok_or(Error::CounterExhausted)?,
            },
        };
        let bytes = encode(self, stamp)?;
        sink.stage_private_transaction(stamp, &bytes, &self.deliveries)
            .map_err(store_error)?;
        self.host.state.checkpoint_revision = stamp.revision;
        self.staged = Some(stamp);
        Ok(stamp)
    }

    /// Strict successor restore. Active transport associations are detached;
    /// original queued tickets and recipients remain immutable historical data.
    pub fn restore_private_timer_transaction(
        store: &mut impl PrivateTimerTransactionStore,
        identity: HostIdentity,
        expected: CheckpointStamp,
        limits: HostLimits,
    ) -> Result<Self, Error> {
        limits.validate()?;
        if identity.scope.0 == 0
            || identity.epoch == 0
            || expected.epoch == 0
            || expected.revision == 0
        {
            return Err(Error::InvalidIdentity);
        }
        if identity.scope != expected.scope {
            return Err(Error::CheckpointStampMismatch);
        }
        if identity.epoch <= expected.epoch {
            return Err(Error::WrongEpoch);
        }
        let mut bytes = vec![0; limits.max_checkpoint_bytes];
        let read = store
            .read_private_transaction(identity.scope, &mut bytes)
            .map_err(store_error)?;
        if !read.complete || read.bytes_written > bytes.len() {
            return Err(Error::CheckpointTooLarge);
        }
        bytes.truncate(read.bytes_written);
        decode(&bytes, identity, expected, limits)
    }

    fn mutate<T>(
        &mut self,
        operation: impl FnOnce(&mut NativeHost) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut next = self.fork();
        let before = bindings(&next.host);
        let result = operation(&mut next.host)?;
        let after = bindings(&next.host);
        // Capture before another operation can reuse any invoker. Native queues
        // start empty; previously retained output cannot be purged by a handler.
        for event in std::mem::take(&mut next.host.state.public) {
            let invoker = match &event {
                PublicVmEvent::Connected { invoker }
                | PublicVmEvent::Disconnected { invoker }
                | PublicVmEvent::Timer { invoker, .. } => *invoker,
                _ => return Err(Error::WrongPlugin),
            };
            let origin = match &event {
                PublicVmEvent::Connected { .. } => after.get(&invoker),
                _ => before.get(&invoker),
            }
            .copied()
            .ok_or(Error::InvalidCheckpoint)?;
            next.append(origin, DeliveryBody::Public(event))?;
        }
        for queued in std::mem::take(&mut next.host.state.private) {
            let origin = queued.message.ticket();
            next.append(
                origin,
                DeliveryBody::Private {
                    recipient: PrivateTimerRecipient {
                        actor: queued.actor,
                        connection: queued.connection,
                        ticket: origin,
                    },
                    message: queued.message,
                },
            )?;
        }
        next.validate_ledger()?;
        next.staged = None;
        let stamp = CheckpointStamp {
            scope: next.host.identity.scope,
            epoch: next.host.identity.epoch,
            revision: next
                .host
                .state
                .checkpoint_revision
                .checked_add(1)
                .ok_or(Error::CounterExhausted)?,
        };
        encode(&next, stamp)?;
        *self = next;
        Ok(result)
    }
    fn append(&mut self, origin: SessionTicket, body: DeliveryBody) -> Result<(), Error> {
        let id = PrivateTimerOutputId(self.next_output);
        self.next_output = self
            .next_output
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        self.deliveries.push(PrivateTimerDelivery {
            id,
            origin,
            body,
            disposition: None,
        });
        Ok(())
    }
    fn check_domain(&self, ticket: SessionTicket) -> Result<(), Error> {
        if ticket.host_scope != self.host.identity.scope {
            return Err(Error::WrongScope);
        }
        if ticket.host_epoch != self.host.identity.epoch {
            return Err(Error::WrongEpoch);
        }
        Ok(())
    }
    fn delivery(&self, id: PrivateTimerOutputId) -> Result<&PrivateTimerDelivery, Error> {
        self.deliveries
            .iter()
            .find(|delivery| delivery.id == id)
            .ok_or(Error::UnknownEffect)
    }
    fn dispose_private(
        &mut self,
        id: PrivateTimerOutputId,
        expected: PrivateTimerRecipient,
        kind: PrivateTimerDispositionKind,
        key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        if self.delivery(id)?.private_recipient() != Some(expected) {
            return Err(Error::RecipientMismatch);
        }
        self.record(id, kind, key, receipt)
    }
    fn record(
        &mut self,
        id: PrivateTimerOutputId,
        kind: PrivateTimerDispositionKind,
        receipt_key: u64,
        receipt: &[u8],
    ) -> Result<(), Error> {
        if receipt_key == 0 || receipt.is_empty() {
            return Err(Error::InvalidIdentity);
        }
        if receipt.len() > MAX_RECEIPT_BYTES {
            return Err(Error::MessageTooLarge);
        }
        let delivery = self.delivery(id)?;
        if let Some(existing) = &delivery.disposition {
            return if existing.kind == kind
                && existing.receipt_key == receipt_key
                && existing.receipt == receipt
            {
                Ok(())
            } else {
                Err(Error::EffectConflict)
            };
        }
        if self.deliveries.iter().any(|other| {
            other
                .disposition
                .as_ref()
                .is_some_and(|entry| entry.receipt_key == receipt_key)
        }) {
            return Err(Error::EffectConflict);
        }
        let mut next = self.fork();
        let delivery = next
            .deliveries
            .iter_mut()
            .find(|delivery| delivery.id == id)
            .ok_or(Error::UnknownEffect)?;
        delivery.disposition = Some(PrivateTimerDisposition {
            kind,
            receipt_key,
            receipt: receipt.to_vec(),
        });
        // A receipt that makes the exact next checkpoint unstageable is rejected
        // without retiring the output or changing the original candidate.
        next.staged = None;
        let stamp = CheckpointStamp {
            scope: next.host.identity.scope,
            epoch: next.host.identity.epoch,
            revision: next
                .host
                .state
                .checkpoint_revision
                .checked_add(1)
                .ok_or(Error::CounterExhausted)?,
        };
        encode(&next, stamp)?;
        *self = next;
        Ok(())
    }
    fn validate_ledger(&self) -> Result<(), Error> {
        let limits = &self.host.limits;
        let mut public_count = 0usize;
        let mut private_count = 0usize;
        let mut private_bytes = 0usize;
        let mut keys = std::collections::BTreeSet::new();
        let mut invokers = BTreeMap::new();
        let mut actors = BTreeMap::new();
        let mut recipients = BTreeMap::new();
        let mut generations = BTreeMap::new();
        let mut latest_epoch = 0;
        for (index, delivery) in self.deliveries.iter().enumerate() {
            if delivery.id.0 != index as u64 + 1 {
                return Err(Error::InvalidCheckpoint);
            }
            let ticket = delivery.origin;
            if ticket.host_scope != self.host.identity.scope
                || ticket.host_epoch == 0
                || ticket.host_epoch > self.host.identity.epoch
                || ticket.instance.0 == 0
                || ticket.instance.0 >= self.host.state.next_instance
                || ticket.generation == 0
                || ticket.generation >= self.host.state.next_session
            {
                return Err(Error::InvalidCheckpoint);
            }
            if ticket.host_epoch < latest_epoch
                || generations
                    .insert(ticket.generation, ticket.instance)
                    .is_some_and(|instance| instance != ticket.instance)
            {
                return Err(Error::InvalidCheckpoint);
            }
            latest_epoch = ticket.host_epoch;
            let live = self.host.state.instances.get(&ticket.instance);
            if live.is_some_and(|instance| ticket.generation > instance.participant.generation) {
                return Err(Error::InvalidCheckpoint);
            }
            match &delivery.body {
                DeliveryBody::Public(event) => {
                    public_count += 1;
                    let invoker = match event {
                        PublicVmEvent::Connected { invoker }
                        | PublicVmEvent::Disconnected { invoker }
                        | PublicVmEvent::Timer { invoker, .. } => invoker,
                        _ => return Err(Error::WrongPlugin),
                    };
                    if invoker.0 == 0
                        || invokers
                            .insert(ticket.instance, *invoker)
                            .is_some_and(|other| other != *invoker)
                        || live.is_some_and(|instance| instance.participant.invoker != *invoker)
                    {
                        return Err(Error::InvalidCheckpoint);
                    }
                    if delivery
                        .disposition
                        .as_ref()
                        .is_some_and(|d| d.kind == PrivateTimerDispositionKind::Suppressed)
                    {
                        return Err(Error::InvalidCheckpoint);
                    }
                }
                DeliveryBody::Private { recipient, message } => {
                    private_count += 1;
                    if recipient.ticket != ticket
                        || recipient.actor.0 == 0
                        || recipient.connection.0 == 0
                        || message.ticket() != ticket
                        || message.plugin() != TIMER_PLUGIN
                        || actors
                            .insert(ticket.instance, recipient.actor)
                            .is_some_and(|other| other != recipient.actor)
                        || recipients
                            .insert(
                                (ticket.instance, ticket.generation),
                                (recipient.actor, recipient.connection),
                            )
                            .is_some_and(|other| other != (recipient.actor, recipient.connection))
                        || live
                            .is_some_and(|instance| instance.participant.actor != recipient.actor)
                    {
                        return Err(Error::InvalidCheckpoint);
                    }
                    validate_message(message)?;
                    private_bytes = private_bytes
                        .checked_add(message.size())
                        .ok_or(Error::QueueFull)?;
                }
            }
            if let Some(d) = &delivery.disposition {
                if d.receipt_key == 0
                    || d.receipt.is_empty()
                    || d.receipt.len() > MAX_RECEIPT_BYTES
                    || !keys.insert(d.receipt_key)
                {
                    return Err(Error::InvalidCheckpoint);
                }
            }
        }
        if self.next_output != self.deliveries.len() as u64 + 1 {
            return Err(Error::InvalidCheckpoint);
        }
        if public_count > limits.max_public_events
            || private_count > limits.max_private_messages
            || private_bytes > limits.max_private_output_bytes
        {
            return Err(Error::QueueFull);
        }
        Ok(())
    }
}

const MAX_RECEIPT_BYTES: usize = 4096;
fn copy_host(host: &NativeHost) -> NativeHost {
    NativeHost {
        identity: host.identity,
        limits: host.limits.clone(),
        state: host.state.clone(),
    }
}
fn ensure_timer_only(host: &NativeHost) -> Result<(), Error> {
    let state = &host.state;
    if !state.controllers.is_empty()
        || !state.games.is_empty()
        || !state.native_groups.is_empty()
        || !state.native_operations.is_empty()
        || !state.native_commands.is_empty()
        || !state.writes.is_empty()
        || state.instances.values().any(|instance| {
            instance.plugin != TIMER_PLUGIN
                || instance.persistence.is_some()
                || !matches!(instance.handler, HandlerState::Timer(_))
        })
    {
        return Err(Error::WrongPlugin);
    }
    Ok(())
}
fn bindings(host: &NativeHost) -> BTreeMap<InvokerId, SessionTicket> {
    host.state
        .instances
        .iter()
        .map(|(id, instance)| {
            (
                instance.participant.invoker,
                SessionTicket {
                    host_scope: host.identity.scope,
                    host_epoch: host.identity.epoch,
                    instance: *id,
                    generation: instance.participant.generation,
                },
            )
        })
        .collect()
}
fn store_error(error: StoreError) -> Error {
    match error {
        StoreError::Unavailable => Error::StoreFailure,
        StoreError::TooLarge => Error::CheckpointTooLarge,
        StoreError::Corrupt => Error::InvalidCheckpoint,
    }
}
fn validate_message(message: &PrivateUiMessage) -> Result<(), Error> {
    let valid = match (message.event(), message.body()) {
        ("eod_enter" | "eod_leave", UiBody::Text(text)) => text.is_empty(),
        ("Timer_Show", UiBody::Binary(bytes)) => bytes.len() == 4,
        ("Timer_Off", UiBody::Binary([mode])) => *mode <= 1,
        ("Timer_Update", UiBody::Text(text)) => {
            text.split_once(':').is_some_and(|(minutes, seconds)| {
                match (minutes.parse::<i16>(), seconds.parse::<i16>()) {
                    (Ok(minutes), Ok(seconds)) => text == format!("{minutes}:{seconds}"),
                    _ => false,
                }
            })
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidCheckpoint)
    }
}

fn encode(value: &PrivateTimerTransaction, stamp: CheckpointStamp) -> Result<Vec<u8>, Error> {
    ensure_timer_only(&value.host)?;
    value.validate_ledger()?;
    let host_bytes = crate::checkpoint::encode_v2(&value.host, stamp)?;
    let mut writer = Writer {
        bytes: vec![],
        limit: value.host.limits.max_checkpoint_bytes,
    };
    writer.put(b"EODT")?;
    writer.u16(1)?;
    writer.u32(u32::try_from(host_bytes.len()).map_err(|_| Error::CheckpointTooLarge)?)?;
    writer.put(&host_bytes)?;
    writer.u64(value.next_output)?;
    writer.u32(u32::try_from(value.deliveries.len()).map_err(|_| Error::CheckpointTooLarge)?)?;
    for delivery in &value.deliveries {
        writer.u64(delivery.id.0)?;
        let ticket = delivery.origin;
        for item in [
            ticket.host_scope.0,
            ticket.host_epoch,
            ticket.instance.0,
            ticket.generation,
        ] {
            writer.u64(item)?;
        }
        match &delivery.body {
            DeliveryBody::Public(event) => {
                writer.u8(0)?;
                let (invoker, code, args) = event.source_event();
                writer.u32(invoker.0)?;
                writer.i16(code)?;
                if let [arg] = args.as_slice() {
                    writer.i16(*arg)?;
                }
            }
            DeliveryBody::Private { recipient, message } => {
                writer.u8(1)?;
                writer.u64(recipient.actor.0)?;
                writer.u64(recipient.connection.0)?;
                let tag = match message.event() {
                    "eod_enter" => 0,
                    "eod_leave" => 1,
                    "Timer_Show" => 2,
                    "Timer_Off" => 3,
                    "Timer_Update" => 4,
                    _ => return Err(Error::InvalidCheckpoint),
                };
                writer.u8(tag)?;
                let bytes = match message.body() {
                    UiBody::Text(text) => text.as_bytes(),
                    UiBody::Binary(bytes) => bytes,
                };
                writer.u32(bytes.len() as u32)?;
                writer.put(bytes)?;
            }
        }
        match &delivery.disposition {
            None => writer.u8(0)?,
            Some(disposition) => {
                writer.u8(match disposition.kind {
                    PrivateTimerDispositionKind::Delivered => 1,
                    PrivateTimerDispositionKind::Superseded => 2,
                    PrivateTimerDispositionKind::Suppressed => 3,
                })?;
                writer.u64(disposition.receipt_key)?;
                writer.u32(disposition.receipt.len() as u32)?;
                writer.put(&disposition.receipt)?;
            }
        }
    }
    Ok(writer.bytes)
}

fn decode(
    bytes: &[u8],
    identity: HostIdentity,
    expected: CheckpointStamp,
    limits: HostLimits,
) -> Result<PrivateTimerTransaction, Error> {
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(4)? != b"EODT" {
        return Err(Error::InvalidCheckpoint);
    }
    if reader.u16()? != 1 {
        return Err(Error::UnsupportedCheckpointVersion);
    }
    let host_size = reader.u32()? as usize;
    let host_bytes = reader.take(host_size)?;
    // Only this Timer-only writer's canonical nested format is accepted.
    if host_bytes.get(..6) != Some(&b"EODP\x02\x00"[..]) {
        return Err(Error::InvalidCheckpoint);
    }
    let host = crate::checkpoint::decode(host_bytes, identity, expected, limits)?;
    ensure_timer_only(&host)?;
    let next_output = reader.u64()?;
    let count = reader.u32()? as usize;
    // Validate all attacker-controlled allocation sizes before allocation. Every
    // record needs at least 48 bytes, independent of payload/receipt lengths.
    if count > host.limits.max_public_events + host.limits.max_private_messages
        || count > reader.remaining() / 48
        || next_output != count as u64 + 1
    {
        return Err(Error::InvalidCheckpoint);
    }
    let mut deliveries = Vec::with_capacity(count);
    for _ in 0..count {
        let id = PrivateTimerOutputId(reader.u64()?);
        let ticket = SessionTicket {
            host_scope: HostScopeId(reader.u64()?),
            host_epoch: reader.u64()?,
            instance: InstanceId(reader.u64()?),
            generation: reader.u64()?,
        };
        if ticket.host_epoch > expected.epoch {
            return Err(Error::InvalidCheckpoint);
        }
        let body = match reader.u8()? {
            0 => {
                let invoker = InvokerId(reader.u32()?);
                let event = match reader.i16()? {
                    -2 => PublicVmEvent::Connected { invoker },
                    -1 => PublicVmEvent::Disconnected { invoker },
                    0 => PublicVmEvent::Timer {
                        invoker,
                        event: TimerVmEvent::Update,
                    },
                    1 => PublicVmEvent::Timer {
                        invoker,
                        event: TimerVmEvent::ToggleStopwatch,
                    },
                    2 => PublicVmEvent::Timer {
                        invoker,
                        event: TimerVmEvent::SetTime {
                            packed_time: reader.i16()?,
                        },
                    },
                    3 => PublicVmEvent::Timer {
                        invoker,
                        event: TimerVmEvent::Start,
                    },
                    4 => PublicVmEvent::Timer {
                        invoker,
                        event: TimerVmEvent::Pause,
                    },
                    _ => return Err(Error::InvalidCheckpoint),
                };
                DeliveryBody::Public(event)
            }
            1 => {
                let actor = ActorId(reader.u64()?);
                let connection = ConnectionId(reader.u64()?);
                let tag = reader.u8()?;
                let length = reader.u32()? as usize;
                // Timer's largest source-produced payload is two signed i16s
                // separated by a colon. Never allocate arbitrary event strings.
                if length > 13 {
                    return Err(Error::InvalidCheckpoint);
                }
                let payload = reader.take(length)?;
                let (event, body) = match tag {
                    0 | 1 | 4 => {
                        let text =
                            std::str::from_utf8(payload).map_err(|_| Error::InvalidCheckpoint)?;
                        (
                            match tag {
                                0 => "eod_enter",
                                1 => "eod_leave",
                                _ => "Timer_Update",
                            },
                            PrivateBody::Text(text.to_owned()),
                        )
                    }
                    2 | 3 => (
                        if tag == 2 { "Timer_Show" } else { "Timer_Off" },
                        PrivateBody::Binary(payload.to_vec()),
                    ),
                    _ => return Err(Error::InvalidCheckpoint),
                };
                let message = PrivateUiMessage {
                    ticket,
                    plugin: TIMER_PLUGIN,
                    event,
                    body,
                };
                validate_message(&message)?;
                DeliveryBody::Private {
                    recipient: PrivateTimerRecipient {
                        actor,
                        connection,
                        ticket,
                    },
                    message,
                }
            }
            _ => return Err(Error::InvalidCheckpoint),
        };
        let disposition = match reader.u8()? {
            0 => None,
            tag @ 1..=3 => {
                let receipt_key = reader.u64()?;
                let length = reader.u32()? as usize;
                if length == 0 || length > MAX_RECEIPT_BYTES {
                    return Err(Error::InvalidCheckpoint);
                }
                Some(PrivateTimerDisposition {
                    kind: match tag {
                        1 => PrivateTimerDispositionKind::Delivered,
                        2 => PrivateTimerDispositionKind::Superseded,
                        _ => PrivateTimerDispositionKind::Suppressed,
                    },
                    receipt_key,
                    receipt: reader.take(length)?.to_vec(),
                })
            }
            _ => return Err(Error::InvalidCheckpoint),
        };
        deliveries.push(PrivateTimerDelivery {
            id,
            origin: ticket,
            body,
            disposition,
        });
    }
    if reader.remaining() != 0 {
        return Err(Error::InvalidCheckpoint);
    }
    let value = PrivateTimerTransaction {
        host,
        next_output,
        deliveries,
        staged: None,
    };
    value.validate_ledger()?;
    // Active participant connections are already detached by the unchanged
    // checkpoint decoder. No historical output connection is installed there.
    Ok(value)
}

struct Writer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Writer {
    fn put(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > self.limit)
        {
            return Err(Error::CheckpointTooLarge);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn u8(&mut self, value: u8) -> Result<(), Error> {
        self.put(&[value])
    }
    fn u16(&mut self, value: u16) -> Result<(), Error> {
        self.put(&value.to_le_bytes())
    }
    fn i16(&mut self, value: i16) -> Result<(), Error> {
        self.put(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> Result<(), Error> {
        self.put(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> Result<(), Error> {
        self.put(&value.to_le_bytes())
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(Error::InvalidCheckpoint)?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or(Error::InvalidCheckpoint)?;
        self.position = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}
