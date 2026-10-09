//! Atomic admission of immutable presentation frames and asynchronous pick tickets.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    ResetRequired,
    WrongBoundary,
    StaleTick,
    StaleArchitecture,
    DuplicateObjectId,
    ZeroGeneration,
    StaleGeneration,
    ReusedGeneration,
    StaleVisualRevision,
    UnversionedVisualChange,
    InvalidTransform,
    InconsistentSelection,
    Limit,
}
impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FrameError {}

/// An opaque ticket that resolves only against the currently displayed identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickTicket {
    pub reference: EntityRef,
    pub visual_revision: u64,
    lot_id: u64,
    epoch: u64,
    content: AssetKey,
    content_generation: u64,
    reset_generation: u64,
    device_generation: u64,
}

#[derive(Clone)]
pub struct FrameStore {
    limits: RenderLimits,
    boundary: Option<(u64, u64)>,
    current: Option<RenderFrame>,
    index: BTreeMap<u32, usize>,
    generations: BTreeMap<u32, u32>,
    reset_generation: u64,
    device_generation: u64,
    exhausted: bool,
    content_generation: u64,
}
impl FrameStore {
    pub fn new(limits: RenderLimits) -> Self {
        Self {
            limits,
            boundary: None,
            current: None,
            index: BTreeMap::new(),
            generations: BTreeMap::new(),
            reset_generation: 0,
            device_generation: 0,
            exhausted: false,
            content_generation: 0,
        }
    }
    /// Explicit lot/epoch boundary. Even resetting to the same values invalidates tickets.
    pub fn reset(&mut self, lot_id: u64, epoch: u64) {
        match self.reset_generation.checked_add(1) {
            Some(next) => self.reset_generation = next,
            None => self.exhausted = true,
        };
        self.boundary = Some((lot_id, epoch));
        self.current = None;
        self.index.clear();
        self.generations.clear();
    }
    pub fn device_reset(&mut self) {
        match self.device_generation.checked_add(1) {
            Some(next) => self.device_generation = next,
            None => self.exhausted = true,
        };
    }
    pub fn current(&self) -> Option<&RenderFrame> {
        self.current.as_ref()
    }
    pub fn entity(&self, reference: EntityRef) -> Option<&EntityProjection> {
        let e = self
            .current
            .as_ref()?
            .entities
            .get(*self.index.get(&reference.object_id)?)?;
        (e.reference == reference).then_some(e)
    }
    pub fn admit(&mut self, frame: RenderFrame) -> Result<(), FrameError> {
        let boundary = self.boundary.ok_or(FrameError::ResetRequired)?;
        if boundary != (frame.stamp.lot_id, frame.stamp.epoch) {
            return Err(FrameError::WrongBoundary);
        }
        if frame.entities.len() > self.limits.max_entities {
            return Err(FrameError::Limit);
        }
        if let Some(old) = &self.current {
            if &frame == old {
                return Ok(());
            }
            if frame.stamp.tick <= old.stamp.tick {
                return Err(FrameError::StaleTick);
            }
            if frame.stamp.architecture_revision < old.stamp.architecture_revision {
                return Err(FrameError::StaleArchitecture);
            }
        }
        let mut ids = BTreeSet::new();
        let mut index = BTreeMap::new();
        let mut new_ids = 0usize;
        for (i, e) in frame.entities.iter().enumerate() {
            if !ids.insert(e.reference.object_id) {
                return Err(FrameError::DuplicateObjectId);
            }
            if e.reference.generation == 0 {
                return Err(FrameError::ZeroGeneration);
            }
            if !e.transform.is_valid() || e.previous_transform.is_some_and(|t| !t.is_valid()) {
                return Err(FrameError::InvalidTransform);
            }
            if let Some(&generation) = self.generations.get(&e.reference.object_id) {
                if e.reference.generation < generation {
                    return Err(FrameError::StaleGeneration);
                }
                if let Some(old) = self.entity(e.reference) {
                    if e.visual_revision < old.visual_revision {
                        return Err(FrameError::StaleVisualRevision);
                    }
                    if e.visual_revision == old.visual_revision && !same_visual(old, e) {
                        return Err(FrameError::UnversionedVisualChange);
                    }
                } else if e.reference.generation == generation {
                    return Err(FrameError::ReusedGeneration);
                }
            } else {
                new_ids += 1;
            }
            index.insert(e.reference.object_id, i);
        }
        // Bounded tombstones prevent reused generations from reviving old tickets.
        if self
            .generations
            .len()
            .checked_add(new_ids)
            .map_or(true, |len| len > self.limits.max_entities)
        {
            return Err(FrameError::Limit);
        }
        if let Some(reference) = frame.selected {
            let e = index.get(&reference.object_id).map(|&i| &frame.entities[i]);
            if !e.is_some_and(|e| e.reference == reference && e.selectable) {
                return Err(FrameError::InconsistentSelection);
            }
        }
        // A content hash can return to a previous value. Accepted replacement is
        // a lifetime boundary, so equality alone cannot validate an old ticket.
        if self
            .current
            .as_ref()
            .is_some_and(|old| old.stamp.content != frame.stamp.content)
        {
            match self.content_generation.checked_add(1) {
                Some(next) => self.content_generation = next,
                None => self.exhausted = true,
            }
        }
        for e in &frame.entities {
            self.generations
                .insert(e.reference.object_id, e.reference.generation);
        }
        self.index = index;
        self.current = Some(frame);
        Ok(())
    }
    pub fn pick_ticket(&self, reference: EntityRef) -> Option<PickTicket> {
        if self.exhausted {
            return None;
        }
        let e = self.entity(reference)?;
        if !e.visible || !e.selectable {
            return None;
        }
        let stamp = self.current.as_ref()?.stamp;
        Some(PickTicket {
            reference,
            visual_revision: e.visual_revision,
            lot_id: stamp.lot_id,
            epoch: stamp.epoch,
            content: stamp.content,
            content_generation: self.content_generation,
            reset_generation: self.reset_generation,
            device_generation: self.device_generation,
        })
    }
    pub fn resolve_pick(&self, ticket: &PickTicket) -> Option<EntityRef> {
        if self.exhausted
            || ticket.reset_generation != self.reset_generation
            || ticket.device_generation != self.device_generation
            || ticket.content_generation != self.content_generation
        {
            return None;
        }
        let stamp = self.current.as_ref()?.stamp;
        if (ticket.lot_id, ticket.epoch, ticket.content)
            != (stamp.lot_id, stamp.epoch, stamp.content)
        {
            return None;
        }
        let e = self.entity(ticket.reference)?;
        (e.visible && e.selectable && e.visual_revision == ticket.visual_revision)
            .then_some(e.reference)
    }
}
fn same_visual(a: &EntityProjection, b: &EntityProjection) -> bool {
    a.transform == b.transform
        && a.previous_transform == b.previous_transform
        && a.asset == b.asset
        && a.level == b.level
        && a.visible == b.visible
        && a.selectable == b.selectable
}
