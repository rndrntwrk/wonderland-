//! Sorted object scheduling and deferred deletion (FreeSO VMScheduler.cs).
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scheduler {
    current_tick: u64,
    scheduled: BTreeMap<u64, BTreeSet<EntityRef>>,
    wake: BTreeMap<EntityRef, u64>,
    ready: BTreeSet<EntityRef>,
    current_object: Option<EntityRef>,
    running: bool,
    pending_deletion: BTreeSet<EntityRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    InvalidEntity,
    PastTick,
    TickOverflow,
    AlreadyRunning,
    NotRunning,
    TickGap,
    CorruptCalendar,
    NotDrained,
}

impl Scheduler {
    pub fn new(completed_tick: u64) -> Self {
        Self {
            current_tick: completed_tick,
            scheduled: BTreeMap::new(),
            wake: BTreeMap::new(),
            ready: BTreeSet::new(),
            current_object: None,
            running: false,
            pending_deletion: BTreeSet::new(),
        }
    }
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
    pub fn scheduled_tick(&self, entity: EntityRef) -> Option<u64> {
        self.wake.get(&entity).copied()
    }
    pub fn schedule(&mut self, entity: EntityRef, tick: u64) -> Result<(), ScheduleError> {
        if !entity.is_valid() {
            return Err(ScheduleError::InvalidEntity);
        }
        let first = EntityRef {
            object_id: entity.object_id,
            generation: 0,
        };
        let last = EntityRef {
            object_id: entity.object_id,
            generation: u32::MAX,
        };
        if self
            .wake
            .range(first..=last)
            .any(|(existing, _)| *existing != entity)
        {
            return Err(ScheduleError::InvalidEntity);
        }
        let same_tick = self.running
            && tick == self.current_tick
            && self
                .current_object
                .map_or(true, |current| entity.object_id > current.object_id);
        if tick <= self.current_tick && !same_tick {
            return Err(ScheduleError::PastTick);
        }
        if self.wake.get(&entity) == Some(&tick) {
            return Ok(());
        }
        self.cancel(entity);
        self.wake.insert(entity, tick);
        if same_tick {
            self.ready.insert(entity);
        } else {
            self.scheduled.entry(tick).or_default().insert(entity);
        }
        Ok(())
    }
    pub fn schedule_in(
        &mut self,
        entity: EntityRef,
        delay: u32,
        every_frame: bool,
    ) -> Result<(), ScheduleError> {
        let delay = if every_frame && delay > 1 { 1 } else { delay };
        let tick = self
            .current_tick
            .checked_add(u64::from(delay))
            .ok_or(ScheduleError::TickOverflow)?;
        self.schedule(entity, tick)
    }
    pub fn cancel(&mut self, entity: EntityRef) {
        self.ready.remove(&entity);
        if let Some(tick) = self.wake.remove(&entity) {
            if let Some(entries) = self.scheduled.get_mut(&tick) {
                entries.remove(&entity);
                if entries.is_empty() {
                    self.scheduled.remove(&tick);
                }
            }
        }
    }
    pub fn interrupt(&mut self, entity: EntityRef) -> Result<Option<u64>, ScheduleError> {
        if !entity.is_valid() {
            return Err(ScheduleError::InvalidEntity);
        }
        let Some(wake) = self.wake.get(&entity).copied() else {
            return Ok(None);
        };
        if wake == self.current_tick || wake == 0 {
            return Ok(None);
        }
        let tick = if self.running
            && self
                .current_object
                .map_or(true, |current| entity.object_id > current.object_id)
        {
            self.current_tick
        } else {
            self.current_tick
                .checked_add(1)
                .ok_or(ScheduleError::TickOverflow)?
        };
        self.schedule(entity, tick)?;
        Ok(Some(tick))
    }
    pub fn begin_tick(&mut self, tick: u64) -> Result<(), ScheduleError> {
        if self.running {
            return Err(ScheduleError::AlreadyRunning);
        }
        if tick
            != self
                .current_tick
                .checked_add(1)
                .ok_or(ScheduleError::TickOverflow)?
        {
            return Err(ScheduleError::TickGap);
        }
        self.current_tick = tick;
        self.running = true;
        self.current_object = None;
        self.ready = self.scheduled.remove(&tick).unwrap_or_default();
        Ok(())
    }
    pub fn next_entity(&mut self) -> Option<EntityRef> {
        if !self.running {
            return None;
        }
        let next = self.ready.pop_first()?;
        self.current_object = Some(next);
        Some(next)
    }
    pub fn delete_at_end(&mut self, entity: EntityRef) {
        self.pending_deletion.insert(entity);
    }
    pub fn is_pending_deletion(&self, entity: EntityRef) -> bool {
        self.pending_deletion.contains(&entity)
    }
    pub fn finish_tick(&mut self) -> Result<Vec<EntityRef>, ScheduleError> {
        if !self.running {
            return Err(ScheduleError::NotRunning);
        }
        if !self.ready.is_empty() {
            return Err(ScheduleError::NotDrained);
        }
        // The source uses HashSet iteration here. Ordering deletion by local ID
        // is an intentional deterministic bug fix, recorded in the handoff.
        let deletions: Vec<_> = std::mem::take(&mut self.pending_deletion)
            .into_iter()
            .collect();
        for entity in &deletions {
            self.cancel(*entity);
        }
        self.wake.retain(|_, tick| *tick != self.current_tick);
        self.ready.clear();
        self.current_object = None;
        self.running = false;
        Ok(deletions)
    }
    pub fn validate(&self, completed_tick: u64) -> Result<(), ScheduleError> {
        if self.current_tick != completed_tick
            || self.running
            || !self.ready.is_empty()
            || self.current_object.is_some()
            || !self.pending_deletion.is_empty()
            || self.wake.len() > i16::MAX as usize
        {
            return Err(ScheduleError::CorruptCalendar);
        }
        let mut seen = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for (tick, entities) in &self.scheduled {
            if *tick <= completed_tick || entities.is_empty() {
                return Err(ScheduleError::CorruptCalendar);
            }
            for entity in entities {
                if !entity.is_valid()
                    || !ids.insert(entity.object_id)
                    || seen.insert(*entity, *tick).is_some()
                {
                    return Err(ScheduleError::CorruptCalendar);
                }
            }
        }
        if seen != self.wake {
            return Err(ScheduleError::CorruptCalendar);
        }
        Ok(())
    }
    pub fn scheduled_entities(&self) -> impl Iterator<Item = EntityRef> + '_ {
        self.wake.keys().copied()
    }
}
