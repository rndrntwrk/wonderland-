// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Shared-game authority, output routing and detached recovery barrier.
use crate::{
    games::{Action, Actions, GameState, Roster, SharedGame, Target},
    host::{
        HandlerState, Instance, Participant, State, authenticate, close_instance, push_private,
    },
    protocol::PrivateBody,
    *,
};

impl NativeHost {
    /// Trusted Invoke Plugin no-avatar connection. Constructed state and seeds
    /// are private; the request cannot be sent through the inbound UI protocol.
    pub fn connect_game_controller(
        &mut self,
        request: GameControllerRequest,
    ) -> Result<GameControllerTicket, Error> {
        if request.object == 0 || request.invoker.0 == 0 {
            return Err(Error::InvalidIdentity);
        }
        if self
            .state
            .games
            .values()
            .any(|game| game.object == request.object)
        {
            return Err(Error::ControllerAlreadyConnected);
        }
        if invoker_used(&self.state, request.invoker) {
            return Err(Error::ParticipantAlreadyConnected);
        }
        self.check_capacity(true)?;
        let next_id = self
            .state
            .next_instance
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let handler = GameState::new(request.input)?;
        let mut next = self.state.clone();
        let id = InstanceId(next.next_instance);
        next.next_instance = next_id;
        next.games.insert(
            id,
            SharedGame {
                object: request.object,
                invoker: request.invoker,
                attached: true,
                seats: [None; 4],
                handler,
            },
        );
        next.public.push(PublicVmEvent::Connected {
            invoker: request.invoker,
        });
        self.commit(next)?;
        Ok(GameControllerTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: id,
        })
    }

    /// Reattach the exact recorded VM invoker after restoring its VM checkpoint.
    pub fn rebind_game_controller(
        &mut self,
        address: InstanceAddress,
        invoker: InvokerId,
    ) -> Result<GameControllerTicket, Error> {
        if address.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        let game = self
            .state
            .games
            .get_mut(&address.instance)
            .ok_or(Error::StaleSession)?;
        if game.invoker != invoker {
            return Err(Error::RecipientMismatch);
        }
        if game.attached {
            return Err(Error::AlreadyBound);
        }
        game.attached = true;
        Ok(GameControllerTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: address.instance,
        })
    }

    /// Native VM callback, fenced by controller scope, epoch, identity and the
    /// recovery barrier. There is deliberately no ClientMessage equivalent.
    pub fn deliver_game_event(
        &mut self,
        ticket: GameControllerTicket,
        invoker: InvokerId,
        event: GameVmInput,
    ) -> Result<(), Error> {
        if ticket.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        if ticket.host_epoch != self.identity.epoch {
            return Err(Error::WrongEpoch);
        }
        let game = self
            .state
            .games
            .get(&ticket.instance)
            .ok_or(Error::StaleSession)?;
        if game.invoker != invoker {
            return Err(Error::RecipientMismatch);
        }
        if !ready(&self.state, game) {
            return Err(Error::PluginNotReady);
        }
        let mut next = self.state.clone();
        let mut game = next
            .games
            .remove(&ticket.instance)
            .ok_or(Error::StaleSession)?;
        let roster = roster(&next, &game)?;
        let actions = game.handler.vm_event(event, &roster)?;
        emit(&mut next, self.identity, &game, actions)?;
        next.games.insert(ticket.instance, game);
        self.commit(next)
    }

    /// Authenticated UI seat with role/avatar/tuning supplied by the native VM.
    /// Restored groups reserve all retained seats until rebind or explicit abort.
    pub fn join_game(
        &mut self,
        authority: &impl ConnectionAuthority,
        request: GamePlayerRequest,
    ) -> Result<SessionTicket, Error> {
        let actor = authenticate(authority, request.connection)?;
        if request.game.host_scope != self.identity.scope {
            return Err(Error::WrongScope);
        }
        if request.invoker.0 == 0 || request.avatar_object <= 0 {
            return Err(Error::InvalidIdentity);
        }
        let game = self
            .state
            .games
            .get(&request.game.instance)
            .ok_or(Error::StaleSession)?;
        if !ready(&self.state, game) {
            return Err(Error::PluginNotReady);
        }
        let slot = game.handler.select_seat(request.input, &game.seats)?;
        if invoker_used(&self.state, request.invoker)
            || self.state.instances.values().any(|instance| {
                instance.participant.actor == actor
                    || instance.participant.connection == Some(request.connection)
                    || instance.handler.avatar_object() == Some(request.avatar_object)
            })
        {
            return Err(Error::ParticipantAlreadyConnected);
        }
        self.check_capacity(false)?;
        self.state
            .tick
            .checked_add(self.limits.idle_timeout_ticks)
            .ok_or(Error::CounterExhausted)?;
        let next_id = self
            .state
            .next_instance
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let next_session = self
            .state
            .next_session
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let mut next = self.state.clone();
        let mut game = next
            .games
            .remove(&request.game.instance)
            .ok_or(Error::StaleSession)?;
        let id = InstanceId(next.next_instance);
        let generation = next.next_session;
        next.next_instance = next_id;
        next.next_session = next_session;
        let ticket = SessionTicket {
            host_scope: self.identity.scope,
            host_epoch: self.identity.epoch,
            instance: id,
            generation,
        };
        let instance = Instance {
            plugin: game.handler.plugin(),
            object: game.object,
            handler: HandlerState::GameParticipant {
                game: request.game.instance,
                slot: slot as u8,
                avatar_object: request.avatar_object,
            },
            persistence: None,
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
        next.instances.insert(id, instance);
        game.seats[slot] = Some(id);
        let roster = roster(&next, &game)?;
        let actions = game.handler.join(slot, request.input, &roster)?;
        emit(&mut next, self.identity, &game, actions)?;
        next.games.insert(request.game.instance, game);
        self.commit(next)?;
        Ok(ticket)
    }
}

fn invoker_used(state: &State, invoker: InvokerId) -> bool {
    state
        .instances
        .values()
        .any(|instance| instance.participant.invoker == invoker)
        || state
            .controllers
            .values()
            .any(|controller| controller.invoker == invoker)
        || state.games.values().any(|game| game.invoker == invoker)
}
pub(crate) fn roster(state: &State, game: &SharedGame) -> Result<Roster, Error> {
    let mut roster = [0; 4];
    for (slot, id) in game.seats.iter().enumerate() {
        if let Some(id) = id {
            let instance = state.instances.get(id).ok_or(Error::InvalidCheckpoint)?;
            roster[slot] = instance
                .handler
                .avatar_object()
                .ok_or(Error::InvalidCheckpoint)?;
        }
    }
    Ok(roster)
}
fn ready(state: &State, game: &SharedGame) -> bool {
    game.attached
        && game.seats.iter().flatten().all(|id| {
            state
                .instances
                .get(id)
                .is_some_and(|instance| instance.participant.connection.is_some())
        })
}
fn ticket(identity: HostIdentity, id: InstanceId, instance: &Instance) -> SessionTicket {
    SessionTicket {
        host_scope: identity.scope,
        host_epoch: identity.epoch,
        instance: id,
        generation: instance.participant.generation,
    }
}
fn emit(
    state: &mut State,
    identity: HostIdentity,
    game: &SharedGame,
    actions: Actions,
) -> Result<(), Error> {
    for action in actions.items {
        match action {
            Action::Ui { seat, event, body } => {
                let Some(id) = game.seats[seat] else {
                    continue;
                };
                let instance = state
                    .instances
                    .get(&id)
                    .ok_or(Error::InvalidCheckpoint)?
                    .clone();
                // Unbound recipients have no transport. Rebind reconstructs their
                // view; ordinary game actions only run after the full barrier.
                if instance.participant.connection.is_some() {
                    push_private(
                        state,
                        &instance,
                        ticket(identity, id, &instance),
                        event,
                        body,
                    );
                }
            }
            Action::Object { target, event } => {
                let invoker = match target {
                    Target::Controller => game.invoker,
                    Target::Seat(seat) => {
                        let id = game.seats[seat].ok_or(Error::InvalidCheckpoint)?;
                        state
                            .instances
                            .get(&id)
                            .ok_or(Error::InvalidCheckpoint)?
                            .participant
                            .invoker
                    }
                };
                state
                    .public
                    .push(PublicVmEvent::CooperativeGame { invoker, event });
            }
        }
    }
    Ok(())
}
pub(crate) fn receive_message(
    state: &mut State,
    identity: HostIdentity,
    participant: InstanceId,
    event: &str,
    body: &[u8],
) -> Result<(), Error> {
    let HandlerState::GameParticipant { game: id, slot, .. } = state
        .instances
        .get(&participant)
        .ok_or(Error::StaleSession)?
        .handler
    else {
        return Err(Error::WrongPlugin);
    };
    let current = state.games.get(&id).ok_or(Error::StaleSession)?;
    if !ready(state, current) {
        return Err(Error::PluginNotReady);
    }
    let mut game = state.games.remove(&id).ok_or(Error::StaleSession)?;
    let roster = roster(state, &game)?;
    let actions = game
        .handler
        .message(usize::from(slot), event, body, &roster)?;
    emit(state, identity, &game, actions)?;
    state.games.insert(id, game);
    Ok(())
}
pub(crate) fn rebind_outputs(
    state: &mut State,
    identity: HostIdentity,
    participant: InstanceId,
) -> Result<(), Error> {
    let HandlerState::GameParticipant { game: id, slot, .. } = state
        .instances
        .get(&participant)
        .ok_or(Error::StaleSession)?
        .handler
    else {
        return Err(Error::WrongPlugin);
    };
    let game = state.games.get(&id).ok_or(Error::StaleSession)?.clone();
    let roster = roster(state, &game)?;
    emit(
        state,
        identity,
        &game,
        game.handler.rebind(usize::from(slot), &roster),
    )
}
pub(crate) fn tick_games(state: &mut State, identity: HostIdentity) -> Result<(), Error> {
    let ids: Vec<_> = state.games.keys().copied().collect();
    for id in ids {
        let game = state.games.get(&id).ok_or(Error::StaleSession)?;
        if !ready(state, game) {
            continue;
        }
        let mut game = state.games.remove(&id).ok_or(Error::StaleSession)?;
        let roster = roster(state, &game)?;
        let actions = game.handler.tick(&roster)?;
        emit(state, identity, &game, actions)?;
        state.games.insert(id, game);
    }
    Ok(())
}
pub(crate) fn participant_left(
    state: &mut State,
    identity: HostIdentity,
    instance: &Instance,
) -> Result<(), Error> {
    let HandlerState::GameParticipant { game: id, slot, .. } = instance.handler else {
        return Ok(());
    };
    let mut game = state.games.remove(&id).ok_or(Error::StaleSession)?;
    game.seats[usize::from(slot)] = None;
    // Native COOP-RECOVERY-ABORT: a lost retained seat cannot mutate a paused VM
    // game's phase behind its disconnected controller. Abort the whole group.
    if !game.attached || instance.participant.connection.is_none() || !ready(state, &game) {
        state.games.insert(id, game);
        return shutdown(state, identity, id);
    }
    let roster = roster(state, &game)?;
    let actions = game.handler.leave(usize::from(slot), &roster)?;
    let shutdown_requested = actions.shutdown;
    emit(state, identity, &game, actions)?;
    state.games.insert(id, game);
    if shutdown_requested {
        shutdown(state, identity, id)?;
    }
    Ok(())
}
pub(crate) fn shutdown(
    state: &mut State,
    identity: HostIdentity,
    id: InstanceId,
) -> Result<(), Error> {
    let game = state.games.remove(&id).ok_or(Error::StaleSession)?;
    state.public.push(PublicVmEvent::Disconnected {
        invoker: game.invoker,
    });
    for id in game.seats.into_iter().flatten() {
        let instance = state
            .instances
            .remove(&id)
            .ok_or(Error::InvalidCheckpoint)?;
        close_instance(state, &instance, ticket(identity, id, &instance));
    }
    Ok(())
}
