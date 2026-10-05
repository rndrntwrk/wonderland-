//! Directed source relationship variables and an adapter-level participant
//! coordinator. Social BHAVs and interaction queues remain owned by B.
use crate::ids::{EntityRef, PersistentId};
use crate::numeric::legacy_f64_to_i16;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationshipTarget {
    Local(EntityRef),
    Persistent(PersistentId),
    Neighbor(i16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipOperation {
    Read,
    Set(i16),
    Add(i16),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationshipRequest {
    pub target: RelationshipTarget,
    pub variable: u8,
    pub operation: RelationshipOperation,
    pub fail_if_too_small: bool,
    pub never_clamp: bool,
    pub category_multiplier: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipResult {
    Value(i16),
    TooSmall,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipState {
    pub values: BTreeMap<RelationshipTarget, Vec<i16>>,
    pub changed_persistent: BTreeSet<PersistentId>,
}
impl RelationshipState {
    pub fn validate(&self) -> Result<(), SocialError> {
        if self.values.len() > 65_536 || self.changed_persistent.len() > 65_536 {
            return Err(SocialError::InvalidContinuation);
        }
        for (target, value) in &self.values {
            let valid = match target {
                RelationshipTarget::Local(e) => valid_entity(*e),
                RelationshipTarget::Persistent(p) => p.0 > 0,
                RelationshipTarget::Neighbor(n) => *n > 0,
            };
            if !valid || value.len() > 256 {
                return Err(SocialError::InvalidContinuation);
            }
        }
        if self.changed_persistent.iter().any(|p| p.0 == 0) {
            return Err(SocialError::InvalidContinuation);
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        request: RelationshipRequest,
    ) -> Result<RelationshipResult, SocialError> {
        if !request.category_multiplier.is_finite() {
            return Err(SocialError::InvalidMultiplier);
        }
        let valid = match request.target {
            RelationshipTarget::Local(e) => valid_entity(e),
            RelationshipTarget::Persistent(p) => p.0 > 0,
            RelationshipTarget::Neighbor(n) => n > 0,
        };
        if !valid {
            return Err(SocialError::InvalidParticipant);
        }
        let existed = self.values.contains_key(&request.target);
        if !existed && request.fail_if_too_small {
            return Ok(RelationshipResult::TooSmall);
        }
        if !existed && self.values.len() >= 65_536 {
            return Err(SocialError::CapacityExceeded);
        }
        if let RelationshipTarget::Persistent(id) = request.target {
            if !matches!(request.operation, RelationshipOperation::Read)
                && !self.changed_persistent.contains(&id)
                && self.changed_persistent.len() >= 65_536
            {
                return Err(SocialError::CapacityExceeded);
            }
        }
        let values = self.values.entry(request.target).or_default();
        // Source dirties persistent writes before checking the variable length.
        if !matches!(request.operation, RelationshipOperation::Read) {
            if let RelationshipTarget::Persistent(id) = request.target {
                self.changed_persistent.insert(id);
            }
        }
        if values.len() <= usize::from(request.variable) {
            if request.fail_if_too_small {
                return Ok(RelationshipResult::TooSmall);
            }
            values.resize(usize::from(request.variable) + 1, 0);
        }
        let value = &mut values[usize::from(request.variable)];
        match request.operation {
            RelationshipOperation::Read => {}
            RelationshipOperation::Set(input) => {
                *value = if request.never_clamp {
                    input
                } else {
                    input.clamp(-100, 100)
                }
            }
            RelationshipOperation::Add(input) => {
                let delta =
                    legacy_f64_to_i16(f64::from(f32::from(input) * request.category_multiplier));
                *value = value.wrapping_add(delta);
                if !request.never_clamp {
                    *value = (*value).clamp(-100, 100);
                }
            }
        }
        Ok(RelationshipResult::Value(*value))
    }
    /// Both incoming/outgoing person-data variables use this outgoing matrix in
    /// TSO. Persistent object IDs >= 2^24 are excluded; LTR variable one >=60.
    pub fn outgoing_friend_count(&self) -> i16 {
        self.values
            .iter()
            .filter(|(target, values)| {
                matches!(target,RelationshipTarget::Persistent(id) if id.0<16_777_216)
                    && values.get(1).is_some_and(|v| *v >= 60)
            })
            .count() as i16
    }
    pub fn remove_local(&mut self, entity: EntityRef) {
        self.values.remove(&RelationshipTarget::Local(entity));
    }
}

/// Apply the source's persistent/neighborhood/local routing rule after the VM
/// adapter has resolved operand direction and object/neighbor IDs.
pub fn relationship_target(
    ts1: bool,
    never_persist: bool,
    use_neighbor: bool,
    source_neighbor: i16,
    target_neighbor: i16,
    target: EntityRef,
    persistent: PersistentId,
) -> RelationshipTarget {
    if !ts1 && persistent.0 > 0 && !never_persist {
        RelationshipTarget::Persistent(persistent)
    } else if ts1 && (use_neighbor || (source_neighbor > 0 && target_neighbor > 0)) {
        RelationshipTarget::Neighbor(target_neighbor)
    } else {
        RelationshipTarget::Local(target)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SocialId(pub u64);
/// A grant issued by W04's physical slot owner. This coordinator cannot mint or
/// validate physical grants; it only returns the exact token for release.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SocialReservation {
    pub resource: EntityRef,
    pub slot: u16,
    pub token: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub entity: EntityRef,
    pub action_id: u64,
    pub role: u16,
    pub session_epoch: u64,
    pub connected: bool,
    pub ready_barrier: Option<u32>,
    pub finished: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancellationReason {
    User,
    Departure,
    Reset,
    Death,
    ReservationLost,
    TargetDeleted,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialPhase {
    Waiting,
    Running { barrier: u32, released_tick: u64 },
    Complete,
    Cancelled(CancellationReason),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialInteraction {
    pub id: SocialId,
    pub participants: BTreeMap<EntityRef, Participant>,
    pub reservations: Vec<SocialReservation>,
    pub phase: SocialPhase,
    pub next_barrier: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialEvent {
    Released {
        social: SocialId,
        barrier: u32,
        tick: u64,
        participants: Vec<EntityRef>,
    },
    CancelAction {
        social: SocialId,
        entity: EntityRef,
        action_id: u64,
        reason: CancellationReason,
    },
    NotifyIdle {
        entity: EntityRef,
    },
    ForceEodDisconnect {
        entity: EntityRef,
    },
    ReleaseReservation(SocialReservation),
    Complete(SocialId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialError {
    InvalidParticipant,
    InvalidMultiplier,
    InvalidContinuation,
    CapacityExceeded,
    DuplicateSocial,
    ParticipantBusy(EntityRef),
    ReservationBusy,
    UnknownSocial,
    UnknownParticipant,
    StaleSession,
    WrongBarrier,
    AlreadyFinished,
    CounterExhausted,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialCoordinator {
    pub interactions: BTreeMap<SocialId, SocialInteraction>,
    pub active_participants: BTreeMap<EntityRef, SocialId>,
    pub active_reservations: BTreeMap<SocialReservation, SocialId>,
}
fn valid_entity(entity: EntityRef) -> bool {
    entity.object_id.0 > 0 && entity.generation > 0
}

impl SocialCoordinator {
    pub fn begin(
        &mut self,
        id: SocialId,
        participants: Vec<Participant>,
        reservations: Vec<SocialReservation>,
    ) -> Result<(), SocialError> {
        if self.interactions.contains_key(&id) {
            return Err(SocialError::DuplicateSocial);
        }
        if self.interactions.len() >= 65_536 {
            return Err(SocialError::CapacityExceeded);
        }
        if id.0 == 0
            || participants.len() < 2
            || participants.len() > 256
            || reservations.len() > 4096
        {
            return Err(SocialError::InvalidParticipant);
        }
        let mut entries = BTreeMap::new();
        let mut roles = BTreeSet::new();
        for mut participant in participants {
            if !valid_entity(participant.entity)
                || participant.action_id == 0
                || !roles.insert(participant.role)
                || entries.contains_key(&participant.entity)
            {
                return Err(SocialError::InvalidParticipant);
            }
            if self.active_participants.contains_key(&participant.entity) {
                return Err(SocialError::ParticipantBusy(participant.entity));
            }
            participant.ready_barrier = None;
            participant.finished = false;
            entries.insert(participant.entity, participant);
        }
        let mut seen = BTreeSet::new();
        for reservation in &reservations {
            if !valid_entity(reservation.resource)
                || reservation.token == 0
                || !seen.insert((reservation.resource, reservation.slot))
            {
                return Err(SocialError::InvalidParticipant);
            }
            if self
                .active_reservations
                .keys()
                .any(|r| r.resource == reservation.resource && r.slot == reservation.slot)
            {
                return Err(SocialError::ReservationBusy);
            }
        }
        // All checks precede mutation: failed contention cannot partially reserve.
        for entity in entries.keys() {
            self.active_participants.insert(*entity, id);
        }
        for reservation in &reservations {
            self.active_reservations.insert(reservation.clone(), id);
        }
        self.interactions.insert(
            id,
            SocialInteraction {
                id,
                participants: entries,
                reservations,
                phase: SocialPhase::Waiting,
                next_barrier: 0,
            },
        );
        Ok(())
    }
    pub fn ready(
        &mut self,
        id: SocialId,
        entity: EntityRef,
        session_epoch: u64,
        barrier: u32,
        tick: u64,
    ) -> Result<Vec<SocialEvent>, SocialError> {
        let interaction = self
            .interactions
            .get_mut(&id)
            .ok_or(SocialError::UnknownSocial)?;
        if matches!(
            interaction.phase,
            SocialPhase::Complete | SocialPhase::Cancelled(_)
        ) {
            return Err(SocialError::AlreadyFinished);
        }
        if barrier != interaction.next_barrier {
            return Err(SocialError::WrongBarrier);
        }
        let participant = interaction
            .participants
            .get(&entity)
            .ok_or(SocialError::UnknownParticipant)?;
        if participant.session_epoch != session_epoch {
            return Err(SocialError::StaleSession);
        }
        let release = interaction
            .participants
            .iter()
            .all(|(key, p)| *key == entity || p.ready_barrier == Some(barrier));
        let next = if release {
            Some(
                barrier
                    .checked_add(1)
                    .ok_or(SocialError::CounterExhausted)?,
            )
        } else {
            None
        };
        interaction
            .participants
            .get_mut(&entity)
            .ok_or(SocialError::UnknownParticipant)?
            .ready_barrier = Some(barrier);
        if let Some(next) = next {
            interaction.phase = SocialPhase::Running {
                barrier,
                released_tick: tick,
            };
            interaction.next_barrier = next;
            for participant in interaction.participants.values_mut() {
                participant.ready_barrier = None;
            }
            return Ok(vec![SocialEvent::Released {
                social: id,
                barrier,
                tick,
                participants: interaction.participants.keys().copied().collect(),
            }]);
        }
        Ok(Vec::new())
    }
    pub fn disconnect(&mut self, entity: EntityRef, epoch: u64) -> Result<(), SocialError> {
        if let Some(id) = self.active_participants.get(&entity) {
            let p = self
                .interactions
                .get_mut(id)
                .ok_or(SocialError::InvalidContinuation)?
                .participants
                .get_mut(&entity)
                .ok_or(SocialError::UnknownParticipant)?;
            if p.session_epoch != epoch {
                return Err(SocialError::StaleSession);
            }
            p.connected = false;
        }
        Ok(())
    }
    pub fn reconnect(&mut self, entity: EntityRef, new_epoch: u64) -> Result<(), SocialError> {
        if let Some(id) = self.active_participants.get(&entity) {
            let p = self
                .interactions
                .get_mut(id)
                .ok_or(SocialError::InvalidContinuation)?
                .participants
                .get_mut(&entity)
                .ok_or(SocialError::UnknownParticipant)?;
            if new_epoch <= p.session_epoch {
                return Err(SocialError::StaleSession);
            }
            p.session_epoch = new_epoch;
            p.connected = true;
        }
        Ok(())
    }
    pub fn cancel(
        &mut self,
        id: SocialId,
        reason: CancellationReason,
    ) -> Result<Vec<SocialEvent>, SocialError> {
        let interaction = self
            .interactions
            .get_mut(&id)
            .ok_or(SocialError::UnknownSocial)?;
        if matches!(
            interaction.phase,
            SocialPhase::Complete | SocialPhase::Cancelled(_)
        ) {
            return Ok(Vec::new());
        }
        interaction.phase = SocialPhase::Cancelled(reason);
        let mut events = Vec::new();
        for participant in interaction.participants.values() {
            self.active_participants.remove(&participant.entity);
            events.push(SocialEvent::CancelAction {
                social: id,
                entity: participant.entity,
                action_id: participant.action_id,
                reason,
            });
            events.push(SocialEvent::NotifyIdle {
                entity: participant.entity,
            });
            events.push(SocialEvent::ForceEodDisconnect {
                entity: participant.entity,
            });
        }
        for reservation in &interaction.reservations {
            self.active_reservations.remove(reservation);
            events.push(SocialEvent::ReleaseReservation(reservation.clone()));
        }
        Ok(events)
    }
    pub fn depart(
        &mut self,
        entity: EntityRef,
        reason: CancellationReason,
    ) -> Result<Vec<SocialEvent>, SocialError> {
        let ids: Vec<_> = self
            .interactions
            .iter()
            .filter(|(_, i)| {
                i.participants.contains_key(&entity)
                    || i.reservations.iter().any(|r| r.resource == entity)
            })
            .map(|(id, _)| *id)
            .collect();
        let mut events = Vec::new();
        for id in ids {
            events.extend(self.cancel(id, reason)?);
        }
        Ok(events)
    }
    pub fn finish(
        &mut self,
        id: SocialId,
        entity: EntityRef,
    ) -> Result<Vec<SocialEvent>, SocialError> {
        let interaction = self
            .interactions
            .get_mut(&id)
            .ok_or(SocialError::UnknownSocial)?;
        if matches!(
            interaction.phase,
            SocialPhase::Complete | SocialPhase::Cancelled(_)
        ) {
            return Ok(Vec::new());
        }
        interaction
            .participants
            .get_mut(&entity)
            .ok_or(SocialError::UnknownParticipant)?
            .finished = true;
        if !interaction.participants.values().all(|p| p.finished) {
            return Ok(Vec::new());
        }
        interaction.phase = SocialPhase::Complete;
        for entity in interaction.participants.keys() {
            self.active_participants.remove(entity);
        }
        let mut events = vec![SocialEvent::Complete(id)];
        for reservation in &interaction.reservations {
            self.active_reservations.remove(reservation);
            events.push(SocialEvent::ReleaseReservation(reservation.clone()));
        }
        Ok(events)
    }
    pub fn validate(&self) -> Result<(), SocialError> {
        if self.interactions.len() > 65_536 {
            return Err(SocialError::InvalidContinuation);
        }
        let mut participants = BTreeMap::new();
        let mut reservations = BTreeMap::new();
        let mut reserved_slots = BTreeSet::new();
        for (id, interaction) in &self.interactions {
            if id.0 == 0
                || *id != interaction.id
                || interaction.participants.len() < 2
                || interaction.participants.len() > 256
                || interaction.reservations.len() > 4096
            {
                return Err(SocialError::InvalidContinuation);
            }
            let active = !matches!(
                interaction.phase,
                SocialPhase::Complete | SocialPhase::Cancelled(_)
            );
            let mut roles = BTreeSet::new();
            for (entity, p) in &interaction.participants {
                if *entity != p.entity
                    || !valid_entity(*entity)
                    || p.action_id == 0
                    || !roles.insert(p.role)
                    || p.ready_barrier
                        .is_some_and(|b| b != interaction.next_barrier)
                {
                    return Err(SocialError::InvalidContinuation);
                }
                if active && participants.insert(*entity, *id).is_some() {
                    return Err(SocialError::InvalidContinuation);
                }
            }
            for reservation in &interaction.reservations {
                if !valid_entity(reservation.resource) || reservation.token == 0 {
                    return Err(SocialError::InvalidContinuation);
                }
                if active
                    && (!reserved_slots.insert((reservation.resource, reservation.slot))
                        || reservations.insert(reservation.clone(), *id).is_some())
                {
                    return Err(SocialError::InvalidContinuation);
                }
            }
        }
        if participants != self.active_participants || reservations != self.active_reservations {
            return Err(SocialError::InvalidContinuation);
        }
        Ok(())
    }
}
