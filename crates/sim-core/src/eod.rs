//! Public source EOD associations. Private plugin/UI state never enters the VM.
//! Session and event identities survive snapshots; absence is not legacy cleanup proof.
use crate::{ids::EntityRef, state::SimState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const TIMER_PLUGIN_ID: u32 = 0xAA65_FE9E;
pub const MAX_EOD_SESSIONS: usize = 4096;
pub const MAX_EOD_EVENTS: usize = 65536;
pub const MAX_QUEUED_EOD_EVENTS: usize = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EodSessionIdentity {
    /// Monotonic source connection generation, independent of actor custody.
    pub session_id: u64,
    pub thread: EntityRef,
    pub invoker: EntityRef,
    pub actor: EntityRef,
    pub control_generation: u64,
    pub object: EntityRef,
    pub plugin_id: u32,
}
impl EodSessionIdentity {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.session_id == 0
            || self.control_generation == 0
            || !self.thread.is_valid()
            || !self.invoker.is_valid()
            || !self.actor.is_valid()
            || !self.object.is_valid()
            || self.thread != self.invoker
            || self.plugin_id != TIMER_PLUGIN_ID
        {
            return Err("invalid supported EOD association");
        }
        Ok(())
    }
}

/// Server-only VMNetEODEventCmd analogue. Never accepted directly from a player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EodEventDelivery {
    pub session: EodSessionIdentity,
    pub event_id: u64,
    pub code: i16,
    pub data: Vec<i16>,
}
impl EodEventDelivery {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.session.validate()?;
        if self.event_id == 0 || self.data.len() > 20 {
            return Err("invalid EOD event identity or temporary register bounds");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EodSession {
    pub identity: EodSessionIdentity,
    pub created_tick: u64,
    pub connect_request_id: u64,
    /// Exact connect input is retained for deterministic native recovery.
    pub connect_temps: [i16; 20],
    /// One disconnect per association, never one per actor ownership generation.
    pub disconnect_request: Option<(u64, u64)>,
    /// Explicit source pointer/FIFO retirement; never a native teardown receipt.
    pub source_detached_tick: Option<u64>,
    pub next_event_id: u64,
    pub ended: bool,
    pub events: VecDeque<EodEventDelivery>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EodState {
    pub next_session_id: u64,
    pub next_request_id: u64,
    /// The source VMThread.EODConnection pointer, separate from retired receipts.
    pub connections: BTreeMap<EntityRef, u64>,
    pub sessions: BTreeMap<u64, EodSession>,
    pub delivered: BTreeMap<(u64, u64), EodEventDelivery>,
}
impl Default for EodState {
    fn default() -> Self {
        Self {
            next_session_id: 1,
            next_request_id: 1,
            connections: BTreeMap::new(),
            sessions: BTreeMap::new(),
            delivered: BTreeMap::new(),
        }
    }
}
impl EodState {
    pub fn actor_teardown_pending(&self, actor: EntityRef) -> bool {
        self.sessions.values().any(|s| {
            s.identity.actor == actor && (!s.ended || s.events.iter().any(|e| e.code >= 0))
        })
    }
    pub fn entity_teardown_pending(&self, entity: EntityRef) -> bool {
        self.sessions.values().any(|s| {
            (!s.ended || s.events.iter().any(|e| e.code >= 0))
                && [
                    s.identity.actor,
                    s.identity.thread,
                    s.identity.invoker,
                    s.identity.object,
                ]
                .contains(&entity)
        })
    }
    pub(crate) fn request_id(&mut self) -> Result<u64, &'static str> {
        let id = self.next_request_id;
        self.next_request_id = id.checked_add(1).ok_or("EOD request counter exhausted")?;
        Ok(id)
    }
    pub fn validate(&self, state: &SimState) -> Result<(), &'static str> {
        if self.next_session_id == 0
            || self.next_request_id == 0
            || self.sessions.len() > MAX_EOD_SESSIONS
            || self.delivered.len() > MAX_EOD_EVENTS
            || self.connections.len() > self.sessions.len()
            || self.sessions.len() as u64 != self.next_session_id - 1
        {
            return Err("EOD state bounds");
        }
        let mut requests = BTreeSet::new();
        let mut connected_actors = BTreeSet::new();
        for (id, session) in &self.sessions {
            let ident = session.identity;
            ident.validate()?;
            if *id != ident.session_id
                || *id >= self.next_session_id
                || session.created_tick == 0
                || session.created_tick > state.completed_tick
                || session.connect_request_id == 0
                || session.connect_request_id >= self.next_request_id
                || !requests.insert(session.connect_request_id)
                || session.next_event_id == 0
                || session.events.len() > MAX_QUEUED_EOD_EVENTS
            {
                return Err("EOD session identity, tick or capacity");
            }
            if let Some((request_id, tick)) = session.disconnect_request {
                if request_id <= session.connect_request_id
                    || request_id >= self.next_request_id
                    || tick < session.created_tick
                    || tick > state.completed_tick
                    || !requests.insert(request_id)
                {
                    return Err("EOD disconnect request identity or tick");
                }
            }
            if !session.ended
                && session.disconnect_request.is_none()
                && !connected_actors.insert(ident.actor)
            {
                return Err("overlapping active EOD avatar associations");
            }
            if !session.ended || session.events.iter().any(|event| event.code >= 0) {
                for entity in [ident.thread, ident.invoker, ident.actor, ident.object] {
                    if !state.ids.is_live(entity) {
                        return Err("stale active EOD entity");
                    }
                }
                let control = state
                    .owned_admission
                    .actors()
                    .and_then(|actors| actors.values().find(|c| c.actor == ident.actor));
                if control.map(|c| c.control_generation) != Some(ident.control_generation)
                    || state
                        .entities
                        .get(&ident.actor.object_id)
                        .and_then(|e| e.avatar.as_ref())
                        .is_none()
                {
                    return Err("EOD owned actor identity mismatch");
                }
            }
            let receipts: Vec<_> = self.delivered.range((*id, 0)..=(*id, u64::MAX)).collect();
            let ended_receipt = receipts.last().is_some_and(|(_, e)| e.code == -1);
            if session.ended != ended_receipt
                || receipts.iter().rev().skip(1).any(|(_, e)| e.code == -1)
            {
                return Err("EOD ended state lacks exact terminal teardown receipt");
            }
            let attached = self.connections.get(&ident.thread) == Some(id);
            if attached == session.source_detached_tick.is_some()
                || session
                    .source_detached_tick
                    .is_some_and(|tick| tick < session.created_tick || tick > state.completed_tick)
                || (!attached && !session.events.is_empty())
            {
                return Err("EOD source pointer retirement mismatch");
            }
            if receipts.len() as u64 != session.next_event_id - 1
                || receipts
                    .iter()
                    .enumerate()
                    .any(|(index, ((_, event_id), _))| *event_id != index as u64 + 1)
            {
                return Err("EOD receipt sequence mismatch");
            }
            let mut previous = 0;
            let mut queued = BTreeSet::new();
            for event in &session.events {
                if event.session != ident
                    || event.event_id <= previous
                    || !queued.insert(event.event_id)
                    || self.delivered.get(&(*id, event.event_id)) != Some(event)
                {
                    return Err("EOD FIFO receipt mismatch");
                }
                previous = event.event_id;
            }
        }
        for (thread, id) in &self.connections {
            let session = self
                .sessions
                .get(id)
                .ok_or("EOD connection without session")?;
            if session.identity.thread != *thread || !state.ids.is_live(*thread) {
                return Err("EOD thread association mismatch");
            }
        }
        for ((session_id, event_id), event) in &self.delivered {
            event.validate()?;
            if *event_id != event.event_id
                || *session_id != event.session.session_id
                || self.sessions.get(session_id).map(|s| s.identity) != Some(event.session)
            {
                return Err("EOD delivery receipt identity mismatch");
            }
        }
        Ok(())
    }
}
