// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Native registered-family integration, atomic cross-plugin messages and
//! checkpoint-prepared durable provider dispatch.
use crate::{
    host::{HandlerState, Instance, Participant, State, authenticate, push_private},
    native_provider::*,
    plugins::{
        FamilyState, NativeCommand, NativeGroup, NativePluginInput, NativeVmInput, common::*,
    },
    protocol::PrivateBody,
    *,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug)]
pub struct NativeCreateRequest {
    pub object: u32,
    /// Trusted VM grouping for related buzzer/club objects, never UI-controlled.
    pub cluster: u64,
    pub invoker: InvokerId,
    pub input: NativePluginInput,
}
#[derive(Clone, Copy, Debug)]
pub struct NativeJoinRequest {
    pub connection: ConnectionId,
    pub group: InstanceAddress,
    pub invoker: InvokerId,
    pub avatar_object: i16,
    pub avatar_id: u32,
    pub input: MemberInput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeControllerTicket {
    pub host_scope: HostScopeId,
    pub host_epoch: u64,
    pub instance: InstanceId,
}
impl NativeControllerTicket {
    pub fn instance_address(self) -> InstanceAddress {
        InstanceAddress {
            host_scope: self.host_scope,
            instance: self.instance,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeVmCommand {
    pub group: InstanceAddress,
    pub invoker: InvokerId,
    pub plugin: PluginId,
    pub command: NativeCommand,
}

impl NativeHost {
    pub fn connect_native(
        &mut self,
        request: NativeCreateRequest,
    ) -> Result<NativeControllerTicket, Error> {
        if request.object == 0 || request.cluster == 0 || request.invoker.0 == 0 {
            return Err(Error::InvalidIdentity);
        }
        if invoker_used(&self.state, request.invoker) {
            return Err(Error::ParticipantAlreadyConnected);
        }
        let mut handler = FamilyState::new(request.input)?;
        if handler
            .trusted_object()
            .is_some_and(|object| object != request.object)
        {
            return Err(Error::InvalidPluginInput);
        }
        let plugin = handler.plugin();
        if self
            .state
            .native_groups
            .values()
            .any(|g| g.object == request.object && g.handler.plugin() == plugin)
        {
            return Err(Error::ControllerAlreadyConnected);
        }
        self.check_capacity(true)?;
        let mut next = self.state.clone();
        let id = InstanceId(next.next_instance);
        next.next_instance = next
            .next_instance
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let roster = [None; MAX_MEMBERS];
        let actions = handler.start(&roster, next.tick)?;
        next.native_groups.insert(
            id,
            NativeGroup {
                object: request.object,
                cluster: request.cluster,
                invoker: request.invoker,
                attached: true,
                closing: false,
                members: [None; MAX_MEMBERS],
                roster,
                handler,
            },
        );
        next.public.push(PublicVmEvent::Connected {
            invoker: request.invoker,
        });
        emit(&mut next, self.identity, id, actions)?;
        self.commit(next)?;
        Ok(NativeControllerTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: id,
        })
    }
    pub fn join_native(
        &mut self,
        authority: &impl ConnectionAuthority,
        request: NativeJoinRequest,
    ) -> Result<SessionTicket, Error> {
        let actor = authenticate(authority, request.connection)?;
        if request.group.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        if request.invoker.0 == 0 || request.avatar_object <= 0 || request.avatar_id == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self.state.native_groups.values().any(|group| {
            group
                .handler
                .reserved_avatars()
                .contains(&request.avatar_id)
        }) {
            return Err(Error::PersistencePending);
        }
        let group = self
            .state
            .native_groups
            .get(&request.group.instance)
            .ok_or(Error::StaleSession)?;
        if group.closing || !ready(&self.state, group) {
            return Err(Error::PluginNotReady);
        }
        let seat = group
            .members
            .iter()
            .position(Option::is_none)
            .ok_or(Error::ParticipantLimit)?;
        if invoker_used(&self.state, request.invoker)
            || self.state.instances.values().any(|i| {
                i.participant.actor == actor
                    || i.participant.connection == Some(request.connection)
                    || i.handler.avatar_object() == Some(request.avatar_object)
            })
            || self.state.native_groups.values().any(|g| {
                g.roster
                    .iter()
                    .flatten()
                    .any(|m| m.avatar_id == request.avatar_id)
            })
        {
            return Err(Error::ParticipantAlreadyConnected);
        }
        self.check_capacity(false)?;
        self.state
            .tick
            .checked_add(self.limits.idle_timeout_ticks)
            .ok_or(Error::CounterExhausted)?;
        let mut next = self.state.clone();
        let mut group = next
            .native_groups
            .remove(&request.group.instance)
            .ok_or(Error::StaleSession)?;
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
        let member = Member {
            seat: seat as u8,
            actor,
            avatar_object: request.avatar_object,
            avatar_id: request.avatar_id,
            input: request.input,
        };
        let instance = Instance {
            plugin: group.handler.plugin(),
            object: group.object,
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
            handler: HandlerState::NativeParticipant {
                group: request.group.instance,
                seat: seat as u8,
                avatar_object: request.avatar_object,
            },
            persistence: None,
        };
        group.members[seat] = Some(id);
        group.roster[seat] = Some(member);
        let actions = group.handler.join(member, &group.roster, next.tick)?;
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
        next.instances.insert(id, instance);
        next.native_groups.insert(request.group.instance, group);
        emit(&mut next, self.identity, request.group.instance, actions)?;
        self.commit(next)?;
        Ok(ticket)
    }
    pub fn rebind_native_controller(
        &mut self,
        address: InstanceAddress,
        invoker: InvokerId,
    ) -> Result<NativeControllerTicket, Error> {
        if address.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        let group = self
            .state
            .native_groups
            .get_mut(&address.instance)
            .ok_or(Error::StaleSession)?;
        if group.invoker != invoker {
            return Err(Error::RecipientMismatch);
        }
        if group.attached {
            return Err(Error::AlreadyBound);
        }
        group.attached = true;
        Ok(NativeControllerTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: address.instance,
        })
    }
    pub fn deliver_native_event(
        &mut self,
        ticket: NativeControllerTicket,
        invoker: InvokerId,
        input: NativeVmInput,
    ) -> Result<(), Error> {
        if ticket.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        if ticket.host_epoch != self.identity.epoch {
            return Err(Error::WrongEpoch);
        }
        let group = self
            .state
            .native_groups
            .get(&ticket.instance)
            .ok_or(Error::StaleSession)?;
        if group.invoker != invoker {
            return Err(Error::RecipientMismatch);
        }
        if !ready(&self.state, group) {
            return Err(Error::PluginNotReady);
        }
        let mut next = self.state.clone();
        let group = next
            .native_groups
            .get_mut(&ticket.instance)
            .ok_or(Error::StaleSession)?;
        let actions = group.handler.vm_event(&input, &group.roster, next.tick)?;
        emit(&mut next, self.identity, ticket.instance, actions)?;
        self.commit(next)
    }
    pub fn take_native_commands(&mut self) -> Vec<NativeVmCommand> {
        std::mem::take(&mut self.state.native_commands)
    }
    /// Dispatch only immutable journal entries already included in a successful
    /// private checkpoint. Reply application is separately admitted; a full output
    /// queue retains the same operation identity for exact provider replay.
    pub fn drive_native_provider(
        &mut self,
        provider: &mut impl NativeProvider,
    ) -> Result<NativeProviderProgress, Error> {
        let mut progress = NativeProviderProgress::default();
        let mut blocked = BTreeSet::new();
        let ids: Vec<_> = self.state.native_operations.keys().copied().collect();
        for id in ids {
            let record = self
                .state
                .native_operations
                .get(&id)
                .ok_or(Error::UnknownEffect)?;
            if blocked.contains(&id.group) {
                if record.prepared {
                    progress.retry_pending += 1;
                } else {
                    progress.waiting_for_checkpoint += 1;
                }
                continue;
            }
            if !record.prepared {
                blocked.insert(id.group);
                progress.waiting_for_checkpoint += 1;
                continue;
            }
            let group = self
                .state
                .native_groups
                .get(&id.group)
                .ok_or(Error::InvalidCheckpoint)?;
            if !group.closing && !ready(&self.state, group) {
                blocked.insert(id.group);
                progress.retry_pending += 1;
                continue;
            }
            let mut bytes = vec![0; self.limits.max_plugin_data_bytes.min(MAX_OPERATION_BYTES)];
            let receipt = match provider.execute(self.identity, &record.request, &mut bytes) {
                Ok(r) => r,
                Err(NativeProviderFailure::Retryable) => {
                    blocked.insert(id.group);
                    progress.retry_pending += 1;
                    continue;
                }
                Err(NativeProviderFailure::Denied) => return Err(Error::NotAuthorized),
                Err(NativeProviderFailure::Corrupt) => return Err(Error::InvalidPluginData),
            };
            if receipt.host != self.identity || receipt.id != id {
                return Err(Error::ProviderReceiptMismatch);
            }
            if !receipt.complete || receipt.bytes_written > bytes.len() {
                return Err(Error::PersistenceTooLarge);
            }
            let reply = decode_reply(&bytes[..receipt.bytes_written])?;
            let callback = record.request.callback;
            let mut next = self.state.clone();
            let group = next
                .native_groups
                .get_mut(&id.group)
                .ok_or(Error::UnknownEffect)?;
            let actions = group
                .handler
                .reply(callback, &reply, &group.roster, next.tick)?;
            next.native_operations.remove(&id);
            emit(&mut next, self.identity, id.group, actions)?;
            reap(&mut next);
            self.commit(next)?;
            progress.completed += 1;
        }
        Ok(progress)
    }
}
fn invoker_used(state: &State, invoker: InvokerId) -> bool {
    state
        .instances
        .values()
        .any(|i| i.participant.invoker == invoker)
        || state.controllers.values().any(|g| g.invoker == invoker)
        || state.games.values().any(|g| g.invoker == invoker)
        || state.native_groups.values().any(|g| g.invoker == invoker)
}
pub(crate) fn ready(state: &State, group: &NativeGroup) -> bool {
    if !locally_ready(state, group) {
        return false;
    }
    // A silent tick can still read a retained peer and advance private timers
    // or RNG. Check the entire recorded dependency closure before invoking a
    // kernel, independently of the actions that transition happens to emit.
    // Buzzer/club links may be cyclic; unrelated co-located groups remain free
    // to progress. Private validation checks the link identity/type/cluster.
    let mut pending = group.handler.required_peers();
    let mut visited = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Some(peer) = state.native_groups.get(&id) else {
            return false;
        };
        if !locally_ready(state, peer) {
            return false;
        }
        pending.extend(peer.handler.required_peers());
    }
    true
}
fn locally_ready(state: &State, group: &NativeGroup) -> bool {
    group.attached
        && group.members.iter().flatten().all(|id| {
            state
                .instances
                .get(id)
                .is_some_and(|i| i.participant.connection.is_some())
        })
}
pub(crate) fn allows(state: &State, instance: &Instance, event: &str, binary: bool) -> bool {
    if let HandlerState::NativeParticipant { group, .. } = instance.handler {
        state
            .native_groups
            .get(&group)
            .is_some_and(|g| g.handler.allows(event, binary))
    } else {
        false
    }
}
pub(crate) fn receive_message(
    state: &mut State,
    identity: HostIdentity,
    id: InstanceId,
    event: &str,
    body: &[u8],
) -> Result<bool, Error> {
    let instance = state.instances.get(&id).ok_or(Error::StaleSession)?;
    let HandlerState::NativeParticipant { group, seat, .. } = instance.handler else {
        return Err(Error::WrongPlugin);
    };
    let g = state.native_groups.get(&group).ok_or(Error::StaleSession)?;
    if g.closing || !ready(state, g) {
        return Err(Error::PluginNotReady);
    }
    let g = state
        .native_groups
        .get_mut(&group)
        .ok_or(Error::StaleSession)?;
    let member = g.roster[usize::from(seat)].ok_or(Error::StaleSession)?;
    let actions = g
        .handler
        .message(member, event, body, &g.roster, state.tick)?;
    emit(state, identity, group, actions)?;
    Ok(!state.instances.contains_key(&id))
}
pub(crate) fn rebind_outputs(
    state: &mut State,
    identity: HostIdentity,
    id: InstanceId,
) -> Result<(), Error> {
    let instance = state.instances.get(&id).ok_or(Error::StaleSession)?;
    let HandlerState::NativeParticipant { group, seat, .. } = instance.handler else {
        return Err(Error::WrongPlugin);
    };
    let g = state
        .native_groups
        .get_mut(&group)
        .ok_or(Error::StaleSession)?;
    if g.closing {
        return Err(Error::PluginNotReady);
    }
    let member = g.roster[usize::from(seat)].ok_or(Error::StaleSession)?;
    let actions = g.handler.rebind(member, &g.roster, state.tick)?;
    emit(state, identity, group, actions)
}
pub(crate) fn tick_groups(state: &mut State, identity: HostIdentity) -> Result<(), Error> {
    let ids: Vec<_> = state.native_groups.keys().copied().collect();
    for id in ids {
        let Some(g) = state.native_groups.get(&id) else {
            continue;
        };
        if g.closing || !ready(state, g) {
            continue;
        }
        let g = state
            .native_groups
            .get_mut(&id)
            .ok_or(Error::StaleSession)?;
        let actions = g.handler.tick(&g.roster, state.tick)?;
        emit(state, identity, id, actions)?;
    }
    reap(state);
    Ok(())
}
pub(crate) fn participant_left(
    state: &mut State,
    identity: HostIdentity,
    instance: &Instance,
) -> Result<(), Error> {
    let HandlerState::NativeParticipant { group, seat, .. } = instance.handler else {
        return Ok(());
    };
    let Some(id) = state
        .native_groups
        .get(&group)
        .and_then(|g| g.members[usize::from(seat)])
    else {
        return Ok(());
    };
    state.instances.insert(id, instance.clone());
    let mut emission = Emission::new(state, identity);
    emission.close_member(state, group, seat)?;
    emission.drain(state)?;
    reap(state);
    Ok(())
}
pub(crate) fn shutdown(
    state: &mut State,
    identity: HostIdentity,
    id: InstanceId,
) -> Result<(), Error> {
    let Some(g) = state.native_groups.get(&id) else {
        return Err(Error::StaleSession);
    };
    if g.closing {
        return Ok(());
    }
    let mut emission = Emission::new(state, identity);
    emission.shutdown_group(state, id)?;
    emission.drain(state)?;
    reap(state);
    Ok(())
}

struct Emission {
    identity: HostIdentity,
    queue: VecDeque<(InstanceId, Action)>,
    remaining: usize,
    old_private: BTreeMap<(InstanceId, u64), usize>,
    departed: BTreeMap<(InstanceId, u8), (InstanceId, Instance)>,
    departure_order: Vec<(InstanceId, u8)>,
    controller_departures: Vec<InstanceId>,
}
impl Emission {
    fn new(state: &State, identity: HostIdentity) -> Self {
        let mut old_private = BTreeMap::new();
        for q in &state.private {
            *old_private
                .entry((q.message.ticket.instance, q.message.ticket.generation))
                .or_default() += 1;
        }
        Self {
            identity,
            queue: VecDeque::new(),
            remaining: 8192,
            old_private,
            departed: BTreeMap::new(),
            departure_order: Vec::new(),
            controller_departures: Vec::new(),
        }
    }
    fn execute(
        &mut self,
        state: &mut State,
        group: InstanceId,
        actions: Actions,
    ) -> Result<(), Error> {
        self.append(group, actions)?;
        self.drain(state)
    }
    fn append(&mut self, group: InstanceId, actions: Actions) -> Result<(), Error> {
        if actions.items.len() > self.remaining
            || self
                .queue
                .len()
                .checked_add(actions.items.len())
                .is_none_or(|n| n > 8192)
        {
            return Err(Error::QueueFull);
        }
        self.queue
            .extend(actions.items.into_iter().map(|a| (group, a)));
        Ok(())
    }
    fn member_instance(&self, state: &State, gid: InstanceId, seat: u8) -> Option<Instance> {
        if usize::from(seat) >= MAX_MEMBERS {
            return None;
        }
        state
            .native_groups
            .get(&gid)
            .and_then(|g| g.members[usize::from(seat)])
            .and_then(|id| state.instances.get(&id))
            .cloned()
            .or_else(|| {
                self.departed
                    .get(&(gid, seat))
                    .map(|(_, instance)| instance.clone())
            })
    }
    fn targets(
        &self,
        state: &State,
        gid: InstanceId,
        target: Target,
    ) -> Vec<(Option<Instance>, InvokerId)> {
        let Some(g) = state.native_groups.get(&gid) else {
            return vec![];
        };
        match target {
            Target::Controller => {
                if g.attached {
                    vec![(None, g.invoker)]
                } else {
                    vec![]
                }
            }
            Target::Member(seat) => self
                .member_instance(state, gid, seat)
                .map(|i| {
                    let inv = i.participant.invoker;
                    vec![(Some(i), inv)]
                })
                .unwrap_or_default(),
            Target::All => g
                .roster
                .iter()
                .flatten()
                .filter_map(|m| self.member_instance(state, gid, m.seat))
                .map(|i| {
                    let inv = i.participant.invoker;
                    (Some(i), inv)
                })
                .collect(),
        }
    }
    fn drain(&mut self, state: &mut State) -> Result<(), Error> {
        while let Some((gid, action)) = self.queue.pop_front() {
            self.remaining = self.remaining.checked_sub(1).ok_or(Error::QueueFull)?;
            let Some(g) = state.native_groups.get(&gid) else {
                continue;
            };
            let plugin = g.handler.plugin();
            let object = g.object;
            let cluster = g.cluster;
            let invoker = g.invoker;
            let closing_source = g.closing;
            match action {
                Action::Ui {
                    target,
                    event,
                    body,
                } => {
                    if event.is_empty() || event.len() > 256 || !event.is_ascii() {
                        return Err(Error::InvalidMessage);
                    }
                    for (instance, _) in self.targets(state, gid, target) {
                        if let Some(instance) = instance {
                            // Departed members receive only outputs whose original
                            // instance is retained in this emission's explicit map.
                            let id = state.instances.iter().find_map(|(id, i)| {
                                (i.participant.generation == instance.participant.generation)
                                    .then_some(*id)
                            });
                            let id = id.or_else(|| {
                                self.departed.values().find_map(|(id, i)| {
                                    (i.participant.generation == instance.participant.generation)
                                        .then_some(*id)
                                })
                            });
                            if let Some(id) = id {
                                let ticket = SessionTicket {
                                    host_scope: self.identity.scope,
                                    host_epoch: self.identity.epoch,
                                    instance: id,
                                    generation: instance.participant.generation,
                                };
                                push_private(state, &instance, ticket, event, body.clone());
                            }
                        }
                    }
                }
                Action::Object { target, code, args } => {
                    if args.len() > 32 || code == -1 || code == -2 {
                        return Err(Error::InvalidMessage);
                    }
                    // A recorded VM continuation cannot be discarded after a
                    // durable effect commits. Keep its immutable provider entry
                    // for replay until the trusted controller rebinds.
                    if target == Target::Controller && !g.attached {
                        return Err(Error::PluginNotReady);
                    }
                    for (_, invoker) in self.targets(state, gid, target) {
                        state.public.push(PublicVmEvent::NativePlugin {
                            invoker,
                            plugin,
                            code,
                            args: args.clone(),
                        });
                    }
                }
                Action::Command(command) => {
                    if !g.attached {
                        return Err(Error::PluginNotReady);
                    }
                    if !command.valid() {
                        return Err(Error::InvalidPluginInput);
                    }
                    state.native_commands.push(NativeVmCommand {
                        group: InstanceAddress {
                            host_scope: self.identity.scope,
                            instance: gid,
                        },
                        invoker,
                        plugin,
                        command,
                    });
                }
                Action::Provider {
                    callback,
                    operation,
                } => {
                    if callback == 0
                        || state
                            .native_operations
                            .values()
                            .any(|r| r.request.id.group == gid && r.request.callback == callback)
                    {
                        return Err(Error::EffectConflict);
                    }
                    operation.private_bytes()?;
                    let id = NativeOperationId {
                        scope: self.identity.scope,
                        origin_epoch: self.identity.epoch,
                        group: gid,
                        operation: state.next_native_operation,
                    };
                    state.next_native_operation = state
                        .next_native_operation
                        .checked_add(1)
                        .ok_or(Error::CounterExhausted)?;
                    state.native_operations.insert(
                        id,
                        NativeOperationRecord {
                            request: NativeProviderRequest {
                                id,
                                plugin,
                                object,
                                callback,
                                operation: *operation,
                            },
                            prepared: false,
                        },
                    );
                }
                Action::Peer {
                    plugin: recipient,
                    signal,
                } => {
                    if signal.bytes.len() > MAX_PEER_BYTES || signal.numbers.len() > 4096 {
                        return Err(Error::MessageTooLarge);
                    }
                    let source_roster = state
                        .native_groups
                        .get(&gid)
                        .ok_or(Error::StaleSession)?
                        .roster;
                    let recipients: Vec<_> = state
                        .native_groups
                        .iter()
                        .filter_map(|(id, g)| {
                            (*id != gid
                                && g.cluster == cluster
                                && g.handler.plugin() == recipient
                                && !g.closing)
                                .then_some(*id)
                        })
                        .collect();
                    for id in recipients {
                        let g = state.native_groups.get(&id).ok_or(Error::StaleSession)?;
                        // Trusted shutdown actions must be able to clear the
                        // very detached dependency that pauses advancement.
                        // The recipient itself must still have all bindings;
                        // ordinary peer transitions retain the full closure gate.
                        if !(if closing_source {
                            locally_ready(state, g)
                        } else {
                            ready(state, g)
                        }) {
                            return Err(Error::PluginNotReady);
                        }
                        let source = PeerSource {
                            destination: id,
                            plugin,
                            object,
                            group: gid,
                            roster: source_roster,
                        };
                        let g = state
                            .native_groups
                            .get_mut(&id)
                            .ok_or(Error::StaleSession)?;
                        let actions = g.handler.peer(&source, &signal, &g.roster, state.tick)?;
                        self.append(id, actions)?;
                    }
                }
                Action::Close { target } => match target {
                    Target::Controller | Target::All => self.shutdown_group(state, gid)?,
                    Target::Member(seat) => self.close_member(state, gid, seat)?,
                },
            }
        }
        for (gid, seat) in &self.departure_order {
            let (id, instance) = self
                .departed
                .get(&(*gid, *seat))
                .ok_or(Error::InvalidCheckpoint)?;
            let ticket = SessionTicket {
                host_scope: self.identity.scope,
                host_epoch: self.identity.epoch,
                instance: *id,
                generation: instance.participant.generation,
            };
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
        for gid in &self.controller_departures {
            if let Some(g) = state.native_groups.get_mut(gid) {
                g.attached = false;
                state
                    .public
                    .push(PublicVmEvent::Disconnected { invoker: g.invoker });
            }
        }
        self.departure_order.clear();
        self.controller_departures.clear();
        Ok(())
    }
    fn close_member(&mut self, state: &mut State, gid: InstanceId, seat: u8) -> Result<(), Error> {
        if usize::from(seat) >= MAX_MEMBERS {
            return Err(Error::InvalidPluginInput);
        }
        let Some(g) = state.native_groups.get_mut(&gid) else {
            return Ok(());
        };
        let Some(id) = g.members[usize::from(seat)].take() else {
            return Ok(());
        };
        let member = g.roster[usize::from(seat)]
            .take()
            .ok_or(Error::InvalidCheckpoint)?;
        let actions = g.handler.leave(member, &g.roster, state.tick)?;
        let instance = state
            .instances
            .remove(&id)
            .ok_or(Error::InvalidCheckpoint)?;
        self.departed.insert((gid, seat), (id, instance.clone()));
        self.departure_order.push((gid, seat));
        let ticket = SessionTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: id,
            generation: instance.participant.generation,
        };
        let mut old = self
            .old_private
            .remove(&(id, ticket.generation))
            .unwrap_or(0);
        state.private.retain(|q| {
            if q.message.ticket == ticket && old > 0 {
                old -= 1;
                false
            } else {
                true
            }
        });
        self.append(gid, actions)
    }
    fn shutdown_group(&mut self, state: &mut State, gid: InstanceId) -> Result<(), Error> {
        let Some(g) = state.native_groups.get(&gid) else {
            return Ok(());
        };
        if g.closing {
            return Ok(());
        }
        for seat in 0..MAX_MEMBERS {
            self.close_member(state, gid, seat as u8)?;
        }
        let g = state
            .native_groups
            .get_mut(&gid)
            .ok_or(Error::StaleSession)?;
        let actions = g.handler.shutdown(&g.roster, state.tick)?;
        g.closing = true;
        self.controller_departures.push(gid);
        self.append(gid, actions)
    }
}
pub(crate) fn emit(
    state: &mut State,
    identity: HostIdentity,
    group: InstanceId,
    actions: Actions,
) -> Result<(), Error> {
    Emission::new(state, identity).execute(state, group, actions)
}
fn reap(state: &mut State) {
    let remove: Vec<_> = state
        .native_groups
        .iter()
        .filter_map(|(id, g)| {
            (g.closing
                && g.handler.can_close()
                && g.members.iter().all(Option::is_none)
                && !state.native_operations.keys().any(|k| k.group == *id))
            .then_some(*id)
        })
        .collect();
    for id in remove {
        state.native_groups.remove(&id);
    }
}
