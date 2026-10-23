//! Source VMInvokePlugin / VMNetEODEventCmd reducer for the bounded Timer slice.
use super::*;
use crate::eod::*;

pub(super) fn deliver(
    state: &mut SimState,
    event: &EodEventDelivery,
) -> Result<Option<bool>, RuntimeError> {
    event.validate().map_err(invalid)?;
    let key = (event.session.session_id, event.event_id);
    if let Some(existing) = state.eod.delivered.get(&key) {
        return if existing == event {
            Ok(None)
        } else {
            Err(invalid("conflicting EOD event retry"))
        };
    }
    let session = state
        .eod
        .sessions
        .get(&event.session.session_id)
        .ok_or_else(|| invalid("unknown EOD session"))?;
    if session.identity != event.session || session.ended || event.event_id != session.next_event_id
    {
        return Err(invalid(
            "stale, ended or out-of-order EOD event association",
        ));
    }
    for entity in [
        event.session.thread,
        event.session.invoker,
        event.session.actor,
        event.session.object,
    ] {
        live_entity(state, entity)?;
    }
    if state
        .owned_admission
        .actors()
        .and_then(|actors| actors.values().find(|c| c.actor == event.session.actor))
        .map(|c| c.control_generation)
        != Some(event.session.control_generation)
    {
        return Err(invalid("stale EOD owned actor"));
    }
    let attached =
        state.eod.connections.get(&event.session.thread) == Some(&event.session.session_id);
    if state.eod.delivered.len() >= MAX_EOD_EVENTS
        || (attached && session.events.len() >= MAX_QUEUED_EOD_EVENTS)
    {
        return Err(invalid("EOD public event capacity"));
    }
    let next = event
        .event_id
        .checked_add(1)
        .ok_or_else(|| invalid("EOD event counter exhausted"))?;
    let session = state
        .eod
        .sessions
        .get_mut(&event.session.session_id)
        .expect("checked session");
    session.next_event_id = next;
    if attached {
        session.events.push_back(event.clone());
    }
    if event.code == -1 {
        session.ended = true;
    }
    state.eod.delivered.insert(key, event.clone());
    Ok(Some(attached))
}

fn disconnect_session(
    state: &mut SimState,
    work: &mut TickWork,
    id: u64,
) -> Result<(), RuntimeError> {
    let session = state
        .eod
        .sessions
        .get(&id)
        .ok_or_else(|| invalid("EOD disconnect session absent"))?;
    if session.ended || session.disconnect_request.is_some() {
        return Ok(());
    }
    let identity = session.identity;
    let request_id = state.eod.request_id().map_err(invalid)?;
    let tick = state.completed_tick + 1;
    state
        .eod
        .sessions
        .get_mut(&id)
        .expect("checked session")
        .disconnect_request = Some((request_id, tick));
    work.events.push(RuntimeEvent::EodDisconnectRequested {
        session: identity,
        request_id,
        tick,
    });
    Ok(())
}

/// Source ForceDisconnect(avatar): associations may have a different object invoker.
pub(super) fn disconnect_actor(
    state: &mut SimState,
    work: &mut TickWork,
    actor: EntityRef,
) -> Result<(), RuntimeError> {
    let ids: Vec<_> = state
        .eod
        .sessions
        .iter()
        .filter(|(_, s)| s.identity.actor == actor && !s.ended)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        disconnect_session(state, work, id)?;
    }
    Ok(())
}

/// Source ForceDisconnectObj(caller), followed by clearing this thread pointer.
fn disconnect_invoker(
    state: &mut SimState,
    work: &mut TickWork,
    invoker: EntityRef,
) -> Result<(), RuntimeError> {
    let ids: Vec<_> = state
        .eod
        .sessions
        .iter()
        .filter(|(_, s)| s.identity.invoker == invoker && !s.ended)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        disconnect_session(state, work, id)?;
    }
    clear_source_connection(state, invoker);
    Ok(())
}

/// VMEntity.Reset and VMInvokePlugin clear the source pointer explicitly.
/// This is not ForceDisconnectObj and does not prove native session teardown.
pub(super) fn clear_source_connection(state: &mut SimState, thread: EntityRef) {
    if let Some(id) = state.eod.connections.remove(&thread) {
        let tick = state.completed_tick + 1;
        let session = state.eod.sessions.get_mut(&id).expect("connected session");
        session.events.clear();
        session.source_detached_tick = Some(tick);
    }
}

pub(super) fn invoke(
    host: &mut RuntimeHost<'_>,
    request: InvokePluginRequest,
) -> Result<HostResponse, VmFault> {
    let bytes = request.operand;
    if host.state.mode != VmMode::Tso
        || bytes[3] != 0
        || host.owner != request.context.caller
        || host.separate_check
        || request.is_check
    {
        return Err(VmFault::UnsupportedPrimitive {
            opcode: 62,
            case:
                "Timer requires nonjoinable owned-avatar association and caller-owned source thread"
                    .into(),
        });
    }
    let event_index = usize::from(bytes[2]);
    let local_evt = *request
        .locals
        .get(event_index)
        .ok_or_else(|| VmFault::Bounds {
            area: "EOD event local".into(),
            index: i32::from(bytes[2]),
            len: request.locals.len(),
        })?;
    if local_evt == -1 || local_evt == -2 {
        disconnect_invoker(host.state, host.work, request.context.caller).map_err(host_error)?;
    }
    if let Some(id) = host.state.eod.connections.get(&host.owner).copied() {
        // The supported association is Timer-only (EodSessionIdentity::validate).
        // Original VMEODServer::SimanticsDeliver only calls a registered handler;
        // VMEODTimerPlugin registers none, for ANY local event code. Therefore
        // this call has no effect and needs no native request or receipt. Keep
        // polling/FIFO/Ended behavior below, and retain the legacy event variant
        // and receipts so earlier checkpoints are never rewritten or forgotten.
        let session = host
            .state
            .eod
            .sessions
            .get_mut(&id)
            .expect("connected session");
        while let Some(event) = session.events.pop_front() {
            if event.code < 0 {
                continue;
            }
            let mut writes = vec![RegisterWrite {
                target: RegisterTarget::Local,
                index: u16::from(bytes[2]),
                value: i32::from(event.code),
            }];
            writes.extend(
                event
                    .data
                    .iter()
                    .enumerate()
                    .map(|(index, value)| RegisterWrite {
                        target: RegisterTarget::Temp,
                        index: index as u16,
                        value: i32::from(*value),
                    }),
            );
            return Ok(HostResponse::CompleteWithWrites {
                exit: PrimitiveExit::GotoFalse,
                writes,
            });
        }
        if session.ended {
            clear_source_connection(host.state, host.owner);
            return Ok(HostResponse::Complete(PrimitiveExit::GotoTrue));
        }
    } else if local_evt == -2 || local_evt == -5 {
        // Exact source order: object local, then person local. Do not read args.
        let local = |index: u8, area: &str| {
            request
                .locals
                .get(usize::from(index))
                .copied()
                .ok_or_else(|| VmFault::Bounds {
                    area: area.into(),
                    index: i32::from(index),
                    len: request.locals.len(),
                })
        };
        let object_id = local(bytes[1], "EOD object local")?;
        let actor_id = local(bytes[0], "EOD person local")?;
        let object = host
            .state
            .ids
            .resolve(ObjectId(object_id))
            .map_err(host_error)?;
        let actor = host
            .state
            .ids
            .resolve(ObjectId(actor_id))
            .map_err(host_error)?;
        live_entity(host.state, request.context.caller).map_err(host_error)?;
        if live_entity(host.state, actor)
            .map_err(host_error)?
            .avatar
            .is_none()
        {
            return Err(VmFault::UnsupportedPrimitive {
                opcode: 62,
                case: "Timer object-only association is unsupported".into(),
            });
        }
        let control_generation = host
            .state
            .owned_admission
            .actors()
            .and_then(|actors| actors.values().find(|c| c.actor == actor))
            .map(|c| c.control_generation)
            .ok_or_else(|| VmFault::UnsupportedPrimitive {
                opcode: 62,
                case: "Timer requires installed owned actor control".into(),
            })?;
        if host.state.eod.sessions.values().any(|session| {
            session.identity.actor == actor
                && !session.ended
                && session.disconnect_request.is_none()
        }) {
            return Err(VmFault::UnsupportedPrimitive {
                opcode: 62,
                case: "overlapping Timer avatar associations are unsupported".into(),
            });
        }
        if host.state.eod.sessions.len() >= MAX_EOD_SESSIONS {
            return Err(host_error("EOD session capacity"));
        }
        let id = host.state.eod.next_session_id;
        host.state.eod.next_session_id = id
            .checked_add(1)
            .ok_or_else(|| host_error("EOD session counter exhausted"))?;
        let request_id = host.state.eod.request_id().map_err(host_error)?;
        let tick = host.state.completed_tick + 1;
        let identity = EodSessionIdentity {
            session_id: id,
            thread: host.owner,
            invoker: request.context.caller,
            actor,
            control_generation,
            object,
            plugin_id: TIMER_PLUGIN_ID,
        };
        host.state.eod.sessions.insert(
            id,
            EodSession {
                identity,
                created_tick: tick,
                connect_request_id: request_id,
                connect_temps: request.temps,
                disconnect_request: None,
                source_detached_tick: None,
                next_event_id: 1,
                ended: false,
                events: VecDeque::new(),
            },
        );
        host.state.eod.connections.insert(host.owner, id);
        host.work.events.push(RuntimeEvent::EodConnectRequested {
            session: identity,
            request_id,
            tick,
            temps: request.temps,
        });
    } else {
        return Ok(HostResponse::Complete(PrimitiveExit::GotoTrue));
    }
    Ok(HostResponse::CompleteWithWrites {
        exit: PrimitiveExit::GotoFalseNextTick,
        writes: vec![
            RegisterWrite {
                target: RegisterTarget::Local,
                index: u16::from(bytes[2]),
                value: 0,
            },
            RegisterWrite {
                target: RegisterTarget::Temp,
                index: 0,
                value: 0,
            },
        ],
    })
}
