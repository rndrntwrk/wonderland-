//! Deterministic lot semantics and resumable world operations.
//! Content scripts and durable storage are supplied by adapters, never executed by this module.
pub mod build;
pub mod footprints;
pub mod invalidation;
pub mod lot;
pub mod placement;
pub mod rooms;
pub mod routing;
pub mod slots;
pub mod tiles;

pub use build::BuildState;
pub use footprints::{Footprint, FootprintRect, ObstacleMotion, WorldObject, WorldRect};
pub use invalidation::{ArchitectureRevision, DirtyRegions};
pub use lot::{LotError, LotModel};
pub use placement::{
    validate_placement, IntersectionCall, IntersectionScripts, PlaceRequestFlags,
    PlacementContinuation, PlacementError, PlacementRequest, PlacementResult, PlacementRules,
    PlacementStep,
};
pub use rooms::{Portal, PortalId, PortalTraversal, Room, RoomCell, RoomId};
pub use routing::{RouteContinuation, RouteFailCode, RouteGoal, RouteRequest, RouteStep};
pub use slots::{
    ReservationRequest, ReservationToken, SlotDefinition, SlotError, SlotKey, SlotState,
};
pub use tiles::{BuildBounds, Cardinal, Diagonal, Facing, LotPosition, Tile, TilePos, WallTile};

use crate::ids::{EntityRef, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_WORLD_OBJECTS: usize = 32_767;

/// Query adapters return read-only semantic data. No borrow survives a callback yield.
pub trait WorldQuery {
    fn lot(&self) -> &LotModel;
    fn object(&self, entity: EntityRef) -> Option<&WorldObject>;
    fn objects(&self) -> Vec<&WorldObject>;
    fn slots(&self) -> &SlotState;
    fn object_revision(&self) -> u64;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldError {
    InvalidObject,
    IdOccupied,
    StaleEntity,
    ObjectLimit,
    OutOfBounds,
    BuildLocked,
    RevisionExhausted,
    Lot(LotError),
    Slot(SlotError),
    InvalidBuildState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldState {
    pub lot: LotModel,
    pub slots: SlotState,
    pub builds: BuildState,
    objects: BTreeMap<ObjectId, WorldObject>,
    object_revision: u64,
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new(LotModel::new(16, 16, 1).expect("valid default lot"))
    }
}

impl WorldState {
    pub fn new(lot: LotModel) -> Self {
        Self {
            lot,
            slots: SlotState::default(),
            builds: BuildState::default(),
            objects: BTreeMap::new(),
            object_revision: 0,
        }
    }
    pub fn objects(&self) -> &BTreeMap<ObjectId, WorldObject> {
        &self.objects
    }
    pub fn object_revision(&self) -> u64 {
        self.object_revision
    }
    pub fn object(&self, entity: EntityRef) -> Option<&WorldObject> {
        self.objects
            .get(&entity.object_id)
            .filter(|object| object.entity == entity)
    }
    pub fn insert_object(&mut self, mut object: WorldObject) -> Result<(), WorldError> {
        if !object.valid() {
            return Err(WorldError::InvalidObject);
        }
        if !self.lot.contains_position(object.position) && !object.position.is_out_of_world() {
            return Err(WorldError::OutOfBounds);
        }
        if self.builds.is_locked(object.entity, None, Some(&object)) {
            return Err(WorldError::BuildLocked);
        }
        if self.objects.contains_key(&object.entity.object_id) {
            return Err(WorldError::IdOccupied);
        }
        if self.objects.len() >= MAX_WORLD_OBJECTS {
            return Err(WorldError::ObjectLimit);
        }
        let revision = self
            .object_revision
            .checked_add(1)
            .ok_or(WorldError::RevisionExhausted)?;
        object.revision = revision;
        self.objects.insert(object.entity.object_id, object);
        self.object_revision = revision;
        Ok(())
    }
    /// Authoritative projection update. Call placement/build validation before user-initiated moves.
    pub fn replace_object(&mut self, mut object: WorldObject) -> Result<bool, WorldError> {
        if !object.valid() {
            return Err(WorldError::InvalidObject);
        }
        if !self.lot.contains_position(object.position) && !object.position.is_out_of_world() {
            return Err(WorldError::OutOfBounds);
        }
        let old = self.object(object.entity).ok_or(WorldError::StaleEntity)?;
        object.revision = old.revision;
        if *old == object {
            return Ok(false);
        }
        if self
            .builds
            .is_locked(object.entity, Some(old), Some(&object))
        {
            return Err(WorldError::BuildLocked);
        }
        let revision = self
            .object_revision
            .checked_add(1)
            .ok_or(WorldError::RevisionExhausted)?;
        object.revision = revision;
        self.objects.insert(object.entity.object_id, object);
        self.object_revision = revision;
        Ok(true)
    }
    pub fn move_object(
        &mut self,
        entity: EntityRef,
        position: LotPosition,
        facing: Facing,
    ) -> Result<bool, WorldError> {
        let mut object = self.object(entity).ok_or(WorldError::StaleEntity)?.clone();
        object.position = position;
        object.facing = facing;
        self.replace_object(object)
    }
    pub fn remove_object(&mut self, entity: EntityRef) -> Result<bool, WorldError> {
        if self.object(entity).is_none() {
            return Ok(false);
        }
        if self.builds.is_locked(entity, self.object(entity), None) {
            return Err(WorldError::BuildLocked);
        }
        let revision = self
            .object_revision
            .checked_add(1)
            .ok_or(WorldError::RevisionExhausted)?;
        let mut lot = self.lot.clone();
        let removed_portals: Vec<_> = lot
            .portals()
            .iter()
            .filter(|(_, p)| p.entity == entity)
            .map(|(id, _)| *id)
            .collect();
        for id in removed_portals {
            lot.remove_portal(id).map_err(WorldError::Lot)?;
        }
        let mut objects = self.objects.clone();
        objects.remove(&entity.object_id);
        let mut slots = self.slots.clone();
        slots
            .expire_and_remove_stale(0, &objects)
            .map_err(WorldError::Slot)?;
        self.objects = objects;
        self.slots = slots;
        self.lot = lot;
        self.object_revision = revision;
        Ok(true)
    }
    pub fn define_slot(&mut self, definition: SlotDefinition) -> Result<bool, SlotError> {
        self.slots.define(definition, &self.objects)
    }
    pub fn reserve_slot(
        &mut self,
        request: ReservationRequest,
    ) -> Result<ReservationToken, SlotError> {
        self.slots.reserve(request, &self.objects)
    }
    pub fn occupy_slot(&mut self, token: ReservationToken, tick: u64) -> Result<bool, SlotError> {
        self.slots.occupy(token, tick, &self.objects)
    }
    pub fn expire_slots(&mut self, tick: u64) -> Result<usize, SlotError> {
        self.slots.expire_and_remove_stale(tick, &self.objects)
    }
    pub fn validate(&self) -> Result<(), WorldError> {
        self.lot.validate().map_err(WorldError::Lot)?;
        if self.objects.len() > MAX_WORLD_OBJECTS
            || self.objects.iter().any(|(id, o)| {
                *id != o.entity.object_id
                    || !o.valid()
                    || (!self.lot.contains_position(o.position) && !o.position.is_out_of_world())
                    || o.revision > self.object_revision
            })
        {
            return Err(WorldError::InvalidObject);
        }
        if self
            .lot
            .portals()
            .values()
            .any(|p| self.object(p.entity).is_none())
        {
            return Err(WorldError::StaleEntity);
        }
        self.slots
            .validate(&self.objects)
            .map_err(WorldError::Slot)?;
        self.builds
            .validate_against(self)
            .map_err(|_| WorldError::InvalidBuildState)?;
        Ok(())
    }
}

impl WorldQuery for WorldState {
    fn lot(&self) -> &LotModel {
        &self.lot
    }
    fn object(&self, entity: EntityRef) -> Option<&WorldObject> {
        self.object(entity)
    }
    fn objects(&self) -> Vec<&WorldObject> {
        self.objects.values().collect()
    }
    fn slots(&self) -> &SlotState {
        &self.slots
    }
    fn object_revision(&self) -> u64 {
        self.object_revision
    }
}
