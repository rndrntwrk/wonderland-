// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Authenticated native boundary. VM invocation and transport authority are
//! trusted integrations; no client-provided actor or recipient is authoritative.

use crate::{
    persistence::{Binding, BindingStatus, WriteRecord, *},
    protocol::{self, PrivateBody},
    registry::{self, RuntimeStatus},
    source_plugins::{Actions, PermissionDoor, Scoreboard, Signs, SourceError, SourceUi},
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
    pub max_persistence_records: usize,
    pub max_plugin_data_bytes: usize,
    pub max_total_persistence_bytes: usize,
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
            max_persistence_records: 256,
            max_plugin_data_bytes: 64 * 1024,
            max_total_persistence_bytes: 4 * 1024 * 1024,
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
            || !(1..=4096).contains(&self.max_persistence_records)
            || !(6..=1024 * 1024).contains(&self.max_plugin_data_bytes)
            || !(6..=16 * 1024 * 1024).contains(&self.max_total_persistence_bytes)
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

/// All fields are supplied by an authoritative Invoke Plugin adapter. In
/// particular avatar ObjectID, mode, permissions and persistent-object identity
/// are not extracted from UI frames. Source owner modes require explicit VM
/// authorization; relationship status is an authoritative snapshot.
#[derive(Clone, Copy, Debug)]
pub enum PluginInput {
    DanceFloor {
        avatar_object: i16,
    },
    Signs {
        persistent_object: u32,
        input: SignsInput,
    },
    Scoreboard {
        persistent_object: u32,
    },
    PermissionDoor {
        persistent_object: u32,
        input: DoorInput,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct PluginConnectRequest {
    pub connection: ConnectionId,
    pub object: u32,
    pub invoker: InvokerId,
    pub input: PluginInput,
}

#[derive(Clone)]
pub(crate) enum HandlerState {
    Timer(Timer),
    DanceFloor { avatar_object: i16 },
    Signs(Signs),
    Scoreboard(Scoreboard),
    PermissionDoor(PermissionDoor),
}

impl HandlerState {
    pub(crate) fn load(&mut self, bytes: Option<&[u8]>) -> Result<Actions, SourceError> {
        match self {
            Self::Signs(state) => state.load(bytes),
            Self::Scoreboard(state) => state.load(bytes),
            Self::PermissionDoor(state) => state.load(bytes),
            _ => Err(SourceError::InvalidInput),
        }
    }

    fn source_message(&mut self, event: &str, bytes: &[u8]) -> Result<Actions, SourceError> {
        match self {
            Self::Signs(state) => state.message(event, bytes),
            Self::Scoreboard(state) => state.message(event, bytes),
            Self::PermissionDoor(state) => state.message(event, bytes),
            _ => Err(SourceError::InvalidInput),
        }
    }
}

#[derive(Clone)]
pub(crate) struct DanceController {
    pub(crate) object: u32,
    pub(crate) invoker: InvokerId,
    pub(crate) attached: bool,
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
    pub(crate) handler: HandlerState,
    pub(crate) persistence: Option<Binding>,
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
    pub(crate) next_write: u64,
    pub(crate) instances: BTreeMap<InstanceId, Instance>,
    pub(crate) controllers: BTreeMap<InstanceId, DanceController>,
    pub(crate) writes: BTreeMap<PluginWriteId, WriteRecord>,
    pub(crate) public: Vec<PublicVmEvent>,
    pub(crate) private: Vec<QueuedPrivate>,
}

/// All plugin state is private. Cloning/serializing a VM projection cannot copy it.
/// Each UI participant has its own scoped session. DanceFloor controllers are
/// separate native VM bindings and can be shared by UI participants on a floor.
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
                next_write: 1,
                instances: BTreeMap::new(),
                controllers: BTreeMap::new(),
                writes: BTreeMap::new(),
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

    /// Legacy timer entry retained for existing authoritative VM adapters.
    pub fn connect(
        &mut self,
        authority: &impl ConnectionAuthority,
        request: ConnectRequest,
    ) -> Result<SessionTicket, Error> {
        let registration = registry::lookup(request.plugin).ok_or(Error::UnregisteredPlugin)?;
        match registration.runtime {
            RuntimeStatus::UnsupportedUnverified => return Err(Error::UnverifiedPlugin),
            RuntimeStatus::SourceTranslatedNative => return Err(Error::PluginInputRequired),
            RuntimeStatus::SourceTranslatedTimer => {}
        }
        let (timer, output) = Timer::connect(request.registers);
        self.connect_instance(
            authority,
            request.connection,
            request.plugin,
            request.object,
            request.invoker,
            HandlerState::Timer(timer),
            None,
            output,
            vec![],
        )
    }

    /// Typed native invocation for the four additional source translations.
    /// Persisted handlers begin loading; `drive_persistence` resolves bounded
    /// provider reads, and the authoritative tick emits Signs/Door initialization.
    pub fn connect_plugin(
        &mut self,
        authority: &impl ConnectionAuthority,
        request: PluginConnectRequest,
    ) -> Result<SessionTicket, Error> {
        let (plugin, handler, persistent_object, ui) = match request.input {
            PluginInput::DanceFloor { avatar_object } => {
                if avatar_object <= 0 {
                    return Err(Error::InvalidIdentity);
                }
                if self.state.instances.values().any(|instance| {
                    matches!(instance.handler,
                    HandlerState::DanceFloor { avatar_object: other } if other == avatar_object)
                }) {
                    return Err(Error::ParticipantAlreadyConnected);
                }
                (
                    registry::DANCE_FLOOR_PLUGIN,
                    HandlerState::DanceFloor { avatar_object },
                    None,
                    vec![SourceUi::Text("dance_show", String::new())],
                )
            }
            PluginInput::Signs {
                persistent_object,
                input,
            } => (
                registry::SIGNS_PLUGIN,
                HandlerState::Signs(Signs::new(input).map_err(source_error)?),
                Some(persistent_object),
                vec![],
            ),
            PluginInput::Scoreboard { persistent_object } => (
                registry::SCOREBOARD_PLUGIN,
                HandlerState::Scoreboard(Scoreboard::new().map_err(source_error)?),
                Some(persistent_object),
                vec![SourceUi::Text("scoreboard_show", String::new())],
            ),
            PluginInput::PermissionDoor {
                persistent_object,
                input,
            } => (
                registry::PERMISSION_DOOR_PLUGIN,
                HandlerState::PermissionDoor(PermissionDoor::new(input).map_err(source_error)?),
                Some(persistent_object),
                vec![],
            ),
        };
        let persistence = if let Some(persistent_object) = persistent_object {
            if persistent_object == 0 {
                return Err(Error::InvalidIdentity);
            }
            let key = PluginDataKey {
                scope: self.identity.scope,
                plugin,
                persistent_object,
            };
            if self.state.instances.values().any(|instance| {
                instance
                    .persistence
                    .as_ref()
                    .is_some_and(|binding| binding.key == key)
            }) || self
                .state
                .writes
                .values()
                .any(|record| record.request.key() == key)
            {
                return Err(Error::PersistedObjectBusy);
            }
            Some(Binding::loading(key))
        } else {
            None
        };
        self.connect_instance(
            authority,
            request.connection,
            plugin,
            request.object,
            request.invoker,
            handler,
            persistence,
            vec![],
            ui,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn connect_instance(
        &mut self,
        authority: &impl ConnectionAuthority,
        connection: ConnectionId,
        plugin: PluginId,
        object: u32,
        invoker: InvokerId,
        handler: HandlerState,
        persistence: Option<Binding>,
        timer_output: Vec<Output>,
        source_ui: Vec<SourceUi>,
    ) -> Result<SessionTicket, Error> {
        let actor = authenticate(authority, connection)?;
        if invoker.0 == 0 || object == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self.state.instances.values().any(|instance| {
            instance.participant.actor == actor
                || instance.participant.invoker == invoker
                || instance.participant.connection == Some(connection)
        }) || self
            .state
            .controllers
            .values()
            .any(|controller| controller.invoker == invoker)
        {
            return Err(Error::ParticipantAlreadyConnected);
        }
        self.check_capacity(matches!(handler, HandlerState::Timer(_)))?;
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
        let instance = Instance {
            plugin,
            object,
            handler,
            persistence,
            participant: Participant {
                actor,
                connection: Some(connection),
                invoker,
                generation,
                last_activity: next.tick,
                next_sequence: 1,
                rate_tick: next.tick,
                messages_this_tick: 0,
            },
        };
        next.public.push(PublicVmEvent::Connected { invoker });
        push_private(
            &mut next,
            &instance,
            ticket,
            "eod_enter",
            PrivateBody::Text(String::new()),
        );
        push_outputs(&mut next, &instance, ticket, timer_output);
        push_source_outputs(
            &mut next,
            &instance,
            ticket,
            Actions {
                ui: source_ui,
                ..Actions::default()
            },
        );
        next.instances.insert(id, instance);
        self.commit(next)?;
        Ok(ticket)
    }

    fn check_capacity(&self, timer: bool) -> Result<(), Error> {
        let participants = self.state.instances.len() + self.state.controllers.len();
        if participants >= self.limits.max_participants {
            return Err(Error::ParticipantLimit);
        }
        if participants >= self.limits.max_instances {
            return Err(Error::InstanceLimit);
        }
        if timer
            && self
                .state
                .instances
                .values()
                .filter(|instance| matches!(instance.handler, HandlerState::Timer(_)))
                .count()
                >= self.limits.max_timers
        {
            return Err(Error::TimerLimit);
        }
        Ok(())
    }

    /// Source no-avatar DanceFloor controller connection. Only the authoritative
    /// VM may call this; it creates no UI session or transport authority.
    pub fn connect_dance_controller(
        &mut self,
        object: u32,
        invoker: InvokerId,
    ) -> Result<InstanceAddress, Error> {
        if object == 0 || invoker.0 == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self
            .state
            .controllers
            .values()
            .any(|controller| controller.object == object)
        {
            return Err(Error::ControllerAlreadyConnected);
        }
        if self
            .state
            .controllers
            .values()
            .any(|controller| controller.invoker == invoker)
            || self
                .state
                .instances
                .values()
                .any(|instance| instance.participant.invoker == invoker)
        {
            return Err(Error::ParticipantAlreadyConnected);
        }
        self.check_capacity(false)?;
        let mut next = self.state.clone();
        let id = InstanceId(next.next_instance);
        next.next_instance = next
            .next_instance
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        next.controllers.insert(
            id,
            DanceController {
                object,
                invoker,
                attached: true,
            },
        );
        next.public.push(PublicVmEvent::Connected { invoker });
        self.commit(next)?;
        Ok(InstanceAddress {
            host_scope: self.identity.scope,
            instance: id,
        })
    }

    /// Restore a native controller only after the authoritative VM has rebound
    /// the recorded invoker. A detached controller emits no player button event.
    pub fn rebind_dance_controller(
        &mut self,
        address: InstanceAddress,
        invoker: InvokerId,
    ) -> Result<(), Error> {
        if address.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        let controller = self
            .state
            .controllers
            .get_mut(&address.instance)
            .ok_or(Error::StaleSession)?;
        if controller.invoker != invoker {
            return Err(Error::RecipientMismatch);
        }
        if controller.attached {
            return Err(Error::AlreadyBound);
        }
        controller.attached = true;
        Ok(())
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
        if current
            .persistence
            .as_ref()
            .is_some_and(|binding| binding.status != BindingStatus::Ready)
            || self
                .state
                .writes
                .values()
                .any(|record| record.request.id.instance == instance_id)
        {
            return Err(Error::ReconciliationRequired);
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
        match &mut instance.handler {
            HandlerState::Timer(_) => {
                push_outputs(&mut next, &instance, ticket, Timer::show(registers))
            }
            HandlerState::DanceFloor { .. } => push_private(
                &mut next,
                &instance,
                ticket,
                "dance_show",
                PrivateBody::Text(String::new()),
            ),
            handler => {
                let actions = match handler {
                    HandlerState::Signs(state) => state.rebind(),
                    HandlerState::Scoreboard(state) => state.rebind(),
                    HandlerState::PermissionDoor(state) => state.rebind(),
                    _ => unreachable!(),
                };
                push_source_outputs(&mut next, &instance, ticket, actions);
            }
        }
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
        if current
            .persistence
            .as_ref()
            .is_some_and(|binding| binding.status == BindingStatus::Reconcile)
        {
            return Err(Error::ReconciliationRequired);
        }
        if is_persistent_write(message.plugin, message.event)
            && self
                .state
                .writes
                .values()
                .any(|record| record.request.id.instance == message.ticket.instance)
        {
            return Err(Error::PersistencePending);
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
        if message.event == "Timer_Close"
            || (message.plugin == registry::DANCE_FLOOR_PLUGIN && message.event == "close")
        {
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
        let mut closing = false;
        match &mut instance.handler {
            HandlerState::Timer(timer) => {
                if let WirePayload::Binary(bytes) = message.payload {
                    let output = timer.binary(message.event, bytes);
                    push_outputs(&mut next, &instance, message.ticket, output);
                }
            }
            HandlerState::DanceFloor { avatar_object } => {
                if let WirePayload::Text(body) = message.payload
                    && let Some(button) = parse_source_byte(body)
                    && let Some(controller) = next.controllers.values().find(|controller| {
                        controller.object == instance.object && controller.attached
                    })
                {
                    next.public.push(PublicVmEvent::DanceFloor {
                        controller: controller.invoker,
                        button,
                        avatar_object: *avatar_object,
                    });
                }
            }
            handler => {
                let mut actions =
                    match handler.source_message(message.event, message.payload.bytes()) {
                        Ok(actions) => actions,
                        // Source malformed Signs data is caught and ignored; numeric
                        // TryParse failures in the other handlers are also no-ops.
                        Err(SourceError::InvalidData) => Actions::default(),
                        Err(error) => return Err(source_error(error)),
                    };
                if let Some(bytes) = actions.persist.take() {
                    queue_persistence(
                        &mut next,
                        self.identity,
                        message.ticket.instance,
                        &instance,
                        bytes,
                    )?;
                }
                closing = actions.close;
                push_source_outputs(&mut next, &instance, message.ticket, actions);
            }
        }
        if closing {
            close_instance(&mut next, &instance, message.ticket);
            self.commit(next)?;
            return Ok(DispatchOutcome::Closed);
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
        if let Some(id) = self
            .state
            .controllers
            .iter()
            .find_map(|(id, controller)| (controller.invoker == invoker).then_some(*id))
        {
            let mut next = self.state.clone();
            next.controllers.remove(&id);
            next.public.push(PublicVmEvent::Disconnected { invoker });
            return self.commit(next);
        }
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
                    match &mut instance.handler {
                        HandlerState::Timer(timer) => {
                            let values = registers
                                .timer_registers(participant.invoker)
                                .ok_or(Error::MissingRegisters)?;
                            let output = timer.tick(values);
                            push_outputs(&mut next, &instance, ticket, output);
                        }
                        HandlerState::DanceFloor { .. } => {}
                        handler
                            if instance
                                .persistence
                                .as_ref()
                                .is_some_and(|binding| binding.status == BindingStatus::Ready) =>
                        {
                            let actions = match handler {
                                HandlerState::Signs(state) => state.tick(),
                                HandlerState::Scoreboard(state) => state.tick(),
                                HandlerState::PermissionDoor(state) => state.tick(),
                                _ => unreachable!(),
                            };
                            push_source_outputs(&mut next, &instance, ticket, actions);
                        }
                        _ => {}
                    }
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

    pub(crate) fn commit(&mut self, next: State) -> Result<(), Error> {
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
        let records = next
            .instances
            .values()
            .filter(|instance| instance.persistence.is_some())
            .count()
            .checked_add(next.writes.len())
            .ok_or(Error::PersistenceLimit)?;
        if records > self.limits.max_persistence_records {
            return Err(Error::PersistenceLimit);
        }
        let mut persistence_bytes = 0usize;
        for bytes in next
            .instances
            .values()
            .filter_map(|instance| instance.persistence.as_ref())
            .map(|binding| &binding.bytes)
            .chain(next.writes.values().map(|record| &record.request.bytes))
        {
            if bytes.len() > self.limits.max_plugin_data_bytes {
                return Err(Error::PersistenceTooLarge);
            }
            persistence_bytes = persistence_bytes
                .checked_add(bytes.len())
                .ok_or(Error::PersistenceLimit)?;
        }
        if persistence_bytes > self.limits.max_total_persistence_bytes {
            return Err(Error::PersistenceLimit);
        }
        self.state = next;
        Ok(())
    }
}

impl NativeHost {
    /// Advance bounded private provider work. Writes are dispatched only after
    /// `checkpoint_to` durably records the exact intent at a VM/UI barrier.
    /// Provider calls commit one operation at a time: a later error never rolls
    /// back an earlier durable acknowledgement. Counts describe this call only.
    pub fn drive_persistence(
        &mut self,
        provider: &mut impl PluginDataProvider,
    ) -> Result<PersistenceProgress, Error> {
        let mut progress = PersistenceProgress::default();
        let ids: Vec<_> = self.state.writes.keys().copied().collect();
        for id in ids {
            let record = self.state.writes.get(&id).ok_or(Error::UnknownEffect)?;
            if record.conflicted {
                progress.conflicted += 1;
                continue;
            }
            if !record.prepared {
                progress.waiting_for_checkpoint += 1;
                continue;
            }
            let request = record.request.clone();
            let receipt = match provider.write(self.identity, &request) {
                Ok(receipt) => receipt,
                Err(PersistenceFailure::Retryable) => {
                    progress.retry_pending += 1;
                    continue;
                }
                Err(PersistenceFailure::Denied) => {
                    self.state
                        .writes
                        .get_mut(&id)
                        .ok_or(Error::UnknownEffect)?
                        .conflicted = true;
                    progress.conflicted += 1;
                    continue;
                }
                Err(PersistenceFailure::Corrupt) => return Err(Error::InvalidPluginData),
            };
            if receipt.id != id {
                return Err(Error::ProviderReceiptMismatch);
            }
            match receipt.decision {
                PluginWriteDecision::Applied { revision } => {
                    if request.expected_revision.checked_add(1) != Some(revision) {
                        return Err(Error::ProviderReceiptMismatch);
                    }
                    if let Some(instance) = self.state.instances.get_mut(&id.instance) {
                        let binding = instance
                            .persistence
                            .as_mut()
                            .ok_or(Error::InvalidPluginData)?;
                        if binding.key != id.key || binding.revision != request.expected_revision {
                            return Err(Error::ProviderReceiptMismatch);
                        }
                        binding.revision = revision;
                        binding.exists = true;
                        binding.bytes = request.bytes;
                    }
                    self.state.writes.remove(&id);
                    progress.applied += 1;
                }
                PluginWriteDecision::Conflict => {
                    self.state
                        .writes
                        .get_mut(&id)
                        .ok_or(Error::UnknownEffect)?
                        .conflicted = true;
                    progress.conflicted += 1;
                }
            }
        }
        let instances: Vec<_> = self.state.instances.keys().copied().collect();
        for id in instances {
            let instance = self.state.instances.get(&id).ok_or(Error::StaleSession)?;
            let Some(binding) = &instance.persistence else {
                continue;
            };
            if binding.status == BindingStatus::Ready
                || self
                    .state
                    .writes
                    .values()
                    .any(|record| record.request.key() == binding.key)
            {
                continue;
            }
            let mut data = vec![0; self.limits.max_plugin_data_bytes];
            let read = provider
                .load(self.identity, binding.key, &mut data)
                .map_err(|failure| match failure {
                    PersistenceFailure::Retryable => Error::PersistenceUnavailable,
                    PersistenceFailure::Denied => Error::NotAuthorized,
                    PersistenceFailure::Corrupt => Error::InvalidPluginData,
                })?;
            if !read.complete || read.bytes_written > data.len() {
                return Err(Error::PersistenceTooLarge);
            }
            if (!read.exists && (read.revision != 0 || read.bytes_written != 0))
                || (read.exists && read.revision == 0)
            {
                return Err(Error::InvalidPluginData);
            }
            data.truncate(read.bytes_written);
            if binding.status == BindingStatus::Reconcile {
                if binding.revision != read.revision
                    || binding.exists != read.exists
                    || binding.bytes != data
                {
                    return Err(Error::PersistenceDiverged);
                }
                self.state
                    .instances
                    .get_mut(&id)
                    .ok_or(Error::StaleSession)?
                    .persistence
                    .as_mut()
                    .ok_or(Error::InvalidPluginData)?
                    .status = BindingStatus::Ready;
                progress.reconciled += 1;
            } else {
                let mut next = self.state.clone();
                let mut instance = next.instances.remove(&id).ok_or(Error::StaleSession)?;
                let actions = instance
                    .handler
                    .load(read.exists.then_some(data.as_slice()))
                    .map_err(source_error)?;
                let binding = instance
                    .persistence
                    .as_mut()
                    .ok_or(Error::InvalidPluginData)?;
                binding.revision = read.revision;
                binding.exists = read.exists;
                binding.bytes = data;
                binding.status = BindingStatus::Ready;
                let ticket = self.ticket_for(id, &instance);
                push_source_outputs(&mut next, &instance, ticket, actions);
                next.instances.insert(id, instance);
                self.commit(next)?;
                progress.loaded += 1;
            }
        }
        Ok(progress)
    }

    /// Explicit conflict policy: accept the provider's terminal *not applied*
    /// result, abandon that write and disconnect its session. This cannot drop an
    /// ambiguous or retry-pending request. A new invocation can reload provider
    /// state; this method never merges/overwrites it or fabricates a refund.
    pub fn abort_conflicted_writes(&mut self) -> Result<usize, Error> {
        let ids: Vec<_> = self
            .state
            .writes
            .iter()
            .filter_map(|(id, record)| record.conflicted.then_some(*id))
            .collect();
        let mut next = self.state.clone();
        for id in &ids {
            next.writes.remove(id);
            if let Some(instance) = next.instances.remove(&id.instance) {
                close_instance(
                    &mut next,
                    &instance,
                    self.ticket_for(id.instance, &instance),
                );
            }
        }
        self.commit(next)?;
        Ok(ids.len())
    }

    /// After a provider read reports divergence, the trusted VM adapter can
    /// abandon the detached snapshot participant and invoke afresh. Pending
    /// intent reconciliation must finish first; it cannot be discarded here.
    pub fn abort_unreconciled(&mut self, address: InstanceAddress) -> Result<(), Error> {
        if address.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        let instance = self
            .state
            .instances
            .get(&address.instance)
            .ok_or(Error::StaleSession)?;
        if instance.participant.connection.is_some()
            || !instance
                .persistence
                .as_ref()
                .is_some_and(|binding| binding.status == BindingStatus::Reconcile)
        {
            return Err(Error::ReconciliationRequired);
        }
        if self
            .state
            .writes
            .values()
            .any(|record| record.request.id.instance == address.instance)
        {
            return Err(Error::PersistencePending);
        }
        self.close(self.ticket_for(address.instance, instance))
    }

    pub(crate) fn ticket_for(&self, id: InstanceId, instance: &Instance) -> SessionTicket {
        SessionTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: id,
            generation: instance.participant.generation,
        }
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
    match plugin {
        registry::TIMER_PLUGIN => matches!(
            (event, payload),
            (
                "Timer_State_Change" | "Timer_IsRunning_Change" | "Timer_Set",
                WirePayload::Binary(_)
            ) | ("Timer_Close", WirePayload::Text(_))
        ),
        registry::DANCE_FLOOR_PLUGIN => matches!(
            (event, payload),
            ("close" | "press_button", WirePayload::Text(_))
        ),
        registry::SIGNS_PLUGIN => matches!(
            (event, payload),
            ("close", WirePayload::Text(_)) | ("set_message", WirePayload::Binary(_))
        ),
        registry::SCOREBOARD_PLUGIN => matches!(
            (event, payload),
            (
                "close"
                    | "scoreboard_updatecolor"
                    | "scoreboard_updatescore"
                    | "scoreboard_setscore",
                WirePayload::Text(_)
            )
        ),
        registry::PERMISSION_DOOR_PLUGIN => matches!(
            (event, payload),
            (
                "close" | "set_code" | "set_state" | "set_fee" | "set_flags" | "try_code",
                WirePayload::Text(_)
            )
        ),
        _ => false,
    }
}

fn is_persistent_write(plugin: PluginId, event: &str) -> bool {
    match plugin {
        registry::SIGNS_PLUGIN => event == "set_message",
        registry::SCOREBOARD_PLUGIN => event != "close",
        registry::PERMISSION_DOOR_PLUGIN => event == "set_code",
        _ => false,
    }
}

fn parse_source_byte(body: &str) -> Option<u8> {
    crate::source_plugins::parse_source_u32(body.as_bytes())
        .ok()?
        .try_into()
        .ok()
}

fn source_error(error: SourceError) -> Error {
    match error {
        SourceError::InvalidInput => Error::InvalidPluginInput,
        SourceError::InvalidData => Error::InvalidPluginData,
        SourceError::NotReady => Error::PluginNotReady,
        SourceError::NotAuthorized => Error::NotAuthorized,
    }
}

fn queue_persistence(
    state: &mut State,
    identity: HostIdentity,
    instance_id: InstanceId,
    instance: &Instance,
    bytes: Vec<u8>,
) -> Result<(), Error> {
    let binding = instance
        .persistence
        .as_ref()
        .ok_or(Error::InvalidPluginInput)?;
    if binding.status != BindingStatus::Ready {
        return Err(Error::ReconciliationRequired);
    }
    if state
        .writes
        .values()
        .any(|record| record.request.key() == binding.key)
    {
        return Err(Error::PersistencePending);
    }
    binding
        .revision
        .checked_add(1)
        .ok_or(Error::CounterExhausted)?;
    let id = PluginWriteId {
        key: binding.key,
        origin_epoch: identity.epoch,
        instance: instance_id,
        operation: state.next_write,
    };
    state.next_write = state
        .next_write
        .checked_add(1)
        .ok_or(Error::CounterExhausted)?;
    state.writes.insert(
        id,
        WriteRecord {
            request: PluginDataWrite {
                id,
                actor: instance.participant.actor,
                expected_revision: binding.revision,
                bytes,
            },
            prepared: false,
            conflicted: false,
        },
    );
    Ok(())
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

fn push_source_outputs(
    state: &mut State,
    instance: &Instance,
    ticket: SessionTicket,
    actions: Actions,
) {
    for event in actions.events {
        state.public.push(PublicVmEvent::SourcePlugin {
            invoker: instance.participant.invoker,
            event,
        });
    }
    for ui in actions.ui {
        match ui {
            SourceUi::Text(event, text) => {
                push_private(state, instance, ticket, event, PrivateBody::Text(text))
            }
            SourceUi::Binary(event, bytes) => {
                push_private(state, instance, ticket, event, PrivateBody::Binary(bytes))
            }
        }
    }
}
