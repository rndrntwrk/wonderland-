// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Authenticated native boundary. VM invocation and transport authority are
//! trusted integrations; no client-provided actor or recipient is authoritative.

use crate::{
    protocol::{self, PrivateBody},
    registry::{self, RuntimeStatus},
    timer::{Output, Timer},
    *,
};
use std::{collections::BTreeMap, fmt};

/// Implemented by the authenticated transport, outside client-controlled input.
/// Connection IDs identify a transport incarnation and must never be reused.
/// Authentication lookup must remain stable during one host call.
pub trait ConnectionAuthority {
    fn authenticated_actor(&self, connection: ConnectionId) -> Option<ActorId>;
}

/// Read-only authoritative VM input, never a live reference exposed to a plugin.
pub trait RegisterSource {
    fn timer_registers(&self, invoker: InvokerId) -> Option<TimerRegisters>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostIdentity {
    pub scope: HostScopeId,
    pub epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostLimits {
    pub max_instances: usize,
    pub max_participants: usize,
    pub max_timers: usize,
    pub max_message_bytes: usize,
    pub max_event_bytes: usize,
    pub max_private_messages: usize,
    pub max_private_output_bytes: usize,
    pub max_public_events: usize,
    pub max_messages_per_tick: u32,
    pub idle_timeout_ticks: u64,
    pub max_checkpoint_bytes: usize,
}

impl Default for HostLimits {
    fn default() -> Self {
        Self {
            max_instances: 64,
            max_participants: 64,
            max_timers: 64,
            max_message_bytes: 4096,
            max_event_bytes: 64,
            max_private_messages: 1024,
            max_private_output_bytes: 1024 * 1024,
            max_public_events: 1024,
            max_messages_per_tick: 32,
            idle_timeout_ticks: 30 * 60,
            max_checkpoint_bytes: 1024 * 1024,
        }
    }
}

impl HostLimits {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=1024).contains(&self.max_instances)
            || !(1..=1024).contains(&self.max_participants)
            || !(1..=1024).contains(&self.max_timers)
            || !(64..=1024 * 1024).contains(&self.max_message_bytes)
            || !(1..=256).contains(&self.max_event_bytes)
            || !(1..=65536).contains(&self.max_private_messages)
            || !(32..=16 * 1024 * 1024).contains(&self.max_private_output_bytes)
            || !(1..=65536).contains(&self.max_public_events)
            || !(1..=4096).contains(&self.max_messages_per_tick)
            || !(1..=30 * 86400).contains(&self.idle_timeout_ticks)
            || !(60..=16 * 1024 * 1024).contains(&self.max_checkpoint_bytes)
        {
            return Err(Error::InvalidLimits);
        }
        Ok(())
    }
}

/// Supplied by the authoritative Invoke Plugin adapter, not an inbound UI frame.
#[derive(Clone, Copy, Debug)]
pub struct ConnectRequest {
    pub connection: ConnectionId,
    pub plugin: PluginId,
    pub object: u32,
    pub invoker: InvokerId,
    pub registers: TimerRegisters,
}

#[derive(Clone)]
pub(crate) struct Participant {
    pub(crate) actor: ActorId,
    pub(crate) connection: Option<ConnectionId>,
    pub(crate) invoker: InvokerId,
    pub(crate) generation: u64,
    pub(crate) last_activity: u64,
    pub(crate) next_sequence: u64,
    pub(crate) rate_tick: u64,
    pub(crate) messages_this_tick: u32,
}

#[derive(Clone)]
pub(crate) struct Instance {
    pub(crate) plugin: PluginId,
    pub(crate) object: u32,
    pub(crate) participant: Participant,
    pub(crate) timer: Timer,
}

#[derive(Clone)]
pub(crate) struct QueuedPrivate {
    connection: ConnectionId,
    actor: ActorId,
    message: PrivateUiMessage,
}

#[derive(Clone)]
pub(crate) struct State {
    pub(crate) tick: u64,
    pub(crate) next_instance: u64,
    pub(crate) next_session: u64,
    pub(crate) checkpoint_revision: u64,
    pub(crate) instances: BTreeMap<InstanceId, Instance>,
    pub(crate) public: Vec<PublicVmEvent>,
    pub(crate) private: Vec<QueuedPrivate>,
}

/// All plugin state is private. Cloning/serializing a VM projection cannot copy it.
/// This host currently accepts the single-participant timer registration only.
pub struct NativeHost {
    pub(crate) identity: HostIdentity,
    pub(crate) limits: HostLimits,
    pub(crate) state: State,
}

impl fmt::Debug for NativeHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeHost")
            .field("public", &self.public_projection())
            .field("private", &"[REDACTED]")
            .finish()
    }
}

impl NativeHost {
    pub fn new(identity: HostIdentity, limits: HostLimits) -> Result<Self, Error> {
        limits.validate()?;
        if identity.scope.0 == 0 || identity.epoch == 0 {
            return Err(Error::InvalidIdentity);
        }
        Ok(Self {
            identity,
            limits,
            state: State {
                tick: 0,
                next_instance: 1,
                next_session: 1,
                checkpoint_revision: 0,
                instances: BTreeMap::new(),
                public: vec![],
                private: vec![],
            },
        })
    }

    pub fn public_projection(&self) -> VmProjection {
        VmProjection {
            tick: self.state.tick,
        }
    }
    pub fn take_public_events(&mut self) -> Vec<PublicVmEvent> {
        std::mem::take(&mut self.state.public)
    }

    /// Native-only trusted VM entry. No fallback stub marks unknown behavior done.
    pub fn connect(
        &mut self,
        authority: &impl ConnectionAuthority,
        request: ConnectRequest,
    ) -> Result<SessionTicket, Error> {
        let registration = registry::lookup(request.plugin).ok_or(Error::UnregisteredPlugin)?;
        if registration.runtime != RuntimeStatus::SourceTranslatedTimer {
            return Err(Error::UnverifiedPlugin);
        }
        let actor = authenticate(authority, request.connection)?;
        if request.invoker.0 == 0 || request.object == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self.state.instances.values().any(|instance| {
            instance.participant.actor == actor
                || instance.participant.invoker == request.invoker
                || instance.participant.connection == Some(request.connection)
        }) {
            return Err(Error::ParticipantAlreadyConnected);
        }
        if self.state.instances.len() >= self.limits.max_participants {
            return Err(Error::ParticipantLimit);
        }
        if self.state.instances.len() >= self.limits.max_instances {
            return Err(Error::InstanceLimit);
        }
        if self.state.instances.len() >= self.limits.max_timers {
            return Err(Error::TimerLimit);
        }
        self.state
            .tick
            .checked_add(self.limits.idle_timeout_ticks)
            .ok_or(Error::CounterExhausted)?;
        let mut next = self.state.clone();
        let id = InstanceId(next.next_instance);
        next.next_instance = next
            .next_instance
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let generation = next.next_session;
        next.next_session = next
            .next_session
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let ticket = SessionTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: id,
            generation,
        };
        let (timer, output) = Timer::connect(request.registers);
        let instance = Instance {
            plugin: request.plugin,
            object: request.object,
            timer,
            participant: Participant {
                actor,
                connection: Some(request.connection),
                invoker: request.invoker,
                generation,
                last_activity: next.tick,
                next_sequence: 1,
                rate_tick: next.tick,
                messages_this_tick: 0,
            },
        };
        next.public.push(PublicVmEvent::Connected {
            invoker: request.invoker,
        });
        push_private(
            &mut next,
            &instance,
            ticket,
            "eod_enter",
            PrivateBody::Text(String::new()),
        );
        push_outputs(&mut next, &instance, ticket, output);
        next.instances.insert(id, instance);
        self.commit(next)?;
        Ok(ticket)
    }

    /// Rebind an already restored private participant under fresh authentication
    /// and an explicitly scoped instance address. A prior ticket's epoch need not
    /// match after restore, but its instance_address must belong to this scope.
    /// Caller supplies current authoritative registers for UI reconstruction only;
    /// pending timer state/progress is preserved. This is not a new VM connection.
    pub fn rebind(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        address: InstanceAddress,
        registers: TimerRegisters,
    ) -> Result<SessionTicket, Error> {
        let actor = authenticate(authority, connection)?;
        if address.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        let instance_id = address.instance;
        let current = self
            .state
            .instances
            .get(&instance_id)
            .ok_or(Error::StaleSession)?;
        if current.participant.actor != actor {
            return Err(Error::RecipientMismatch);
        }
        if current.participant.connection.is_some() {
            return Err(Error::AlreadyBound);
        }
        if self
            .state
            .instances
            .values()
            .any(|instance| instance.participant.connection == Some(connection))
        {
            return Err(Error::ParticipantAlreadyConnected);
        }
        self.state
            .tick
            .checked_add(self.limits.idle_timeout_ticks)
            .ok_or(Error::CounterExhausted)?;
        let mut next = self.state.clone();
        let mut instance = next
            .instances
            .remove(&instance_id)
            .ok_or(Error::StaleSession)?;
        instance.participant.connection = Some(connection);
        instance.participant.generation = next.next_session;
        next.next_session = next
            .next_session
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        instance.participant.last_activity = next.tick;
        instance.participant.next_sequence = 1;
        instance.participant.rate_tick = next.tick;
        instance.participant.messages_this_tick = 0;
        let ticket = SessionTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: instance_id,
            generation: instance.participant.generation,
        };
        push_private(
            &mut next,
            &instance,
            ticket,
            "eod_enter",
            PrivateBody::Text(String::new()),
        );
        push_outputs(&mut next, &instance, ticket, Timer::show(registers));
        next.instances.insert(instance_id, instance);
        self.commit(next)?;
        Ok(ticket)
    }

    pub fn receive_bytes(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        bytes: &[u8],
    ) -> Result<DispatchOutcome, Error> {
        let message = protocol::decode(bytes, &self.limits)?;
        self.receive(authority, connection, message)
    }

    pub fn receive(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        message: ClientMessage<'_>,
    ) -> Result<DispatchOutcome, Error> {
        protocol::validate(&message, &self.limits)?;
        let actor = authenticate(authority, connection)?;
        self.check_session(connection, actor, message.ticket)?;
        let current = self
            .state
            .instances
            .get(&message.ticket.instance)
            .ok_or(Error::StaleSession)?;
        if current.plugin != message.plugin {
            return Err(Error::WrongPlugin);
        }
        if !event_allowed(current.plugin, message.event, message.payload) {
            return Err(Error::EventNotAllowed);
        }
        if message.sequence != current.participant.next_sequence {
            return Err(Error::UnexpectedSequence);
        }
        if current.participant.rate_tick == self.state.tick
            && current.participant.messages_this_tick >= self.limits.max_messages_per_tick
        {
            return Err(Error::RateLimited);
        }
        self.state
            .tick
            .checked_add(self.limits.idle_timeout_ticks)
            .ok_or(Error::CounterExhausted)?;
        let mut next = self.state.clone();
        let mut instance = next
            .instances
            .remove(&message.ticket.instance)
            .ok_or(Error::StaleSession)?;
        if message.event == "Timer_Close" {
            close_instance(&mut next, &instance, message.ticket);
            self.commit(next)?;
            return Ok(DispatchOutcome::Closed);
        }
        instance.participant.next_sequence = instance
            .participant
            .next_sequence
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        if instance.participant.rate_tick != next.tick {
            instance.participant.rate_tick = next.tick;
            instance.participant.messages_this_tick = 0;
        }
        instance.participant.messages_this_tick += 1;
        instance.participant.last_activity = next.tick;
        if let WirePayload::Binary(bytes) = message.payload {
            let output = instance.timer.binary(message.event, bytes);
            push_outputs(&mut next, &instance, message.ticket, output);
        }
        next.instances.insert(message.ticket.instance, instance);
        self.commit(next)?;
        Ok(DispatchOutcome::Accepted)
    }

    /// Returns outputs only for this authenticated connection and exact ticket.
    /// Callers cannot provide an alternate recipient. A terminal leave may be
    /// drained after its participant has closed; old queued private data is purged.
    pub fn take_private(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        ticket: SessionTicket,
    ) -> Result<Vec<PrivateUiMessage>, Error> {
        let actor = authenticate(authority, connection)?;
        self.check_ticket_domain(ticket)?;
        let queued = self
            .state
            .private
            .iter()
            .find(|queued| queued.message.ticket == ticket);
        if let Some(queued) = queued {
            if queued.actor != actor || queued.connection != connection {
                return Err(Error::RecipientMismatch);
            }
        } else {
            self.check_session(connection, actor, ticket)?;
        }
        let mut selected = Vec::new();
        let mut retained = Vec::new();
        for queued in std::mem::take(&mut self.state.private) {
            if queued.message.ticket == ticket
                && queued.actor == actor
                && queued.connection == connection
            {
                selected.push(queued.message);
            } else {
                retained.push(queued);
            }
        }
        self.state.private = retained;
        Ok(selected)
    }

    pub fn disconnect(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        ticket: SessionTicket,
    ) -> Result<(), Error> {
        let actor = authenticate(authority, connection)?;
        self.check_session(connection, actor, ticket)?;
        self.close(ticket)
    }

    /// Authoritative VM object/invoker teardown, not an inbound UI operation.
    pub fn disconnect_invoker(&mut self, invoker: InvokerId) -> Result<(), Error> {
        let (id, instance) = self
            .state
            .instances
            .iter()
            .find(|(_, instance)| instance.participant.invoker == invoker)
            .ok_or(Error::StaleSession)?;
        let ticket = SessionTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: *id,
            generation: instance.participant.generation,
        };
        self.close(ticket)
    }

    /// One authoritative 30 Hz tick. Iteration/timeout ordering is by instance ID.
    /// Missing registers or queue exhaustion reject the whole tick without partial
    /// state mutation. Providers must return stable snapshot data during the call.
    pub fn tick(
        &mut self,
        authority: &impl ConnectionAuthority,
        registers: &impl RegisterSource,
    ) -> Result<(), Error> {
        let mut next = self.state.clone();
        next.tick = next.tick.checked_add(1).ok_or(Error::CounterExhausted)?;
        next.private.retain(|queued| {
            authority.authenticated_actor(queued.connection) == Some(queued.actor)
        });
        let ids: Vec<_> = next.instances.keys().copied().collect();
        for id in ids {
            let mut instance = next.instances.remove(&id).ok_or(Error::StaleSession)?;
            let participant = &instance.participant;
            let ticket = SessionTicket {
                host_scope: self.identity.scope,
                host_epoch: self.identity.epoch,
                instance: id,
                generation: participant.generation,
            };
            let deadline = participant
                .last_activity
                .checked_add(self.limits.idle_timeout_ticks)
                .ok_or(Error::CounterExhausted)?;
            let revoked = participant.connection.is_some_and(|connection| {
                authority.authenticated_actor(connection) != Some(participant.actor)
            });
            if next.tick >= deadline || revoked {
                close_instance(&mut next, &instance, ticket);
                // Authentication revocation never keeps a UI payload queued for that transport.
                if revoked {
                    next.private
                        .retain(|queued| queued.message.ticket != ticket);
                }
            } else {
                if participant.connection.is_some() {
                    let values = registers
                        .timer_registers(participant.invoker)
                        .ok_or(Error::MissingRegisters)?;
                    let output = instance.timer.tick(values);
                    push_outputs(&mut next, &instance, ticket, output);
                }
                next.instances.insert(id, instance);
            }
        }
        self.commit(next)
    }

    fn check_session(
        &self,
        connection: ConnectionId,
        actor: ActorId,
        ticket: SessionTicket,
    ) -> Result<(), Error> {
        self.check_ticket_domain(ticket)?;
        let participant = &self
            .state
            .instances
            .get(&ticket.instance)
            .ok_or(Error::StaleSession)?
            .participant;
        if participant.generation != ticket.generation {
            return Err(Error::StaleSession);
        }
        if participant.actor != actor || participant.connection != Some(connection) {
            return Err(Error::RecipientMismatch);
        }
        Ok(())
    }

    fn check_ticket_domain(&self, ticket: SessionTicket) -> Result<(), Error> {
        if ticket.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        if ticket.host_epoch != self.identity.epoch {
            return Err(Error::WrongEpoch);
        }
        Ok(())
    }

    fn close(&mut self, ticket: SessionTicket) -> Result<(), Error> {
        self.check_ticket_domain(ticket)?;
        let mut next = self.state.clone();
        let instance = next
            .instances
            .remove(&ticket.instance)
            .ok_or(Error::StaleSession)?;
        close_instance(&mut next, &instance, ticket);
        self.commit(next)
    }

    fn commit(&mut self, next: State) -> Result<(), Error> {
        if next.public.len() > self.limits.max_public_events
            || next.private.len() > self.limits.max_private_messages
        {
            return Err(Error::QueueFull);
        }
        let bytes = next
            .private
            .iter()
            .try_fold(0usize, |sum, queued| sum.checked_add(queued.message.size()))
            .ok_or(Error::QueueFull)?;
        if bytes > self.limits.max_private_output_bytes {
            return Err(Error::QueueFull);
        }
        self.state = next;
        Ok(())
    }
}

fn authenticate(
    authority: &impl ConnectionAuthority,
    connection: ConnectionId,
) -> Result<ActorId, Error> {
    authority
        .authenticated_actor(connection)
        .filter(|actor| actor.0 != 0 && connection.0 != 0)
        .ok_or(Error::Unauthenticated)
}

fn event_allowed(plugin: PluginId, event: &str, payload: WirePayload<'_>) -> bool {
    if plugin != registry::TIMER_PLUGIN {
        return false;
    }
    matches!(
        (event, payload),
        (
            "Timer_State_Change" | "Timer_IsRunning_Change" | "Timer_Set",
            WirePayload::Binary(_)
        ) | ("Timer_Close", WirePayload::Text(_))
    )
}

fn push_private(
    state: &mut State,
    instance: &Instance,
    ticket: SessionTicket,
    event: &'static str,
    body: PrivateBody,
) {
    if let Some(connection) = instance.participant.connection {
        state.private.push(QueuedPrivate {
            connection,
            actor: instance.participant.actor,
            message: PrivateUiMessage {
                ticket,
                plugin: instance.plugin,
                event,
                body,
            },
        });
    }
}

fn push_outputs(
    state: &mut State,
    instance: &Instance,
    ticket: SessionTicket,
    output: Vec<Output>,
) {
    for message in output {
        match message {
            Output::Public(event) => state.public.push(PublicVmEvent::Timer {
                invoker: instance.participant.invoker,
                event,
            }),
            Output::Binary(event, bytes) => {
                push_private(state, instance, ticket, event, PrivateBody::Binary(bytes))
            }
            Output::Text(event, text) => {
                push_private(state, instance, ticket, event, PrivateBody::Text(text))
            }
        }
    }
}

fn close_instance(state: &mut State, instance: &Instance, ticket: SessionTicket) {
    state
        .private
        .retain(|queued| queued.message.ticket != ticket);
    state.public.push(PublicVmEvent::Disconnected {
        invoker: instance.participant.invoker,
    });
    push_private(
        state,
        instance,
        ticket,
        "eod_leave",
        PrivateBody::Text(String::new()),
    );
}
