use super::{
    footprints::{WorldObject, WorldRect},
    slots::SlotKey,
    tiles::{Cardinal, Diagonal, Facing, LotPosition, TilePos, FLOOR_POOL, FLOOR_WATER},
    WorldQuery,
};
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// These discriminants are the source cst 137 placement error indices.
#[repr(i16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementError {
    Success = -1,
    LocationOutOfBounds = 0,
    LevelOutOfBounds = 1,
    MustBeAtTileCenter = 2,
    MustBeInCorner = 3,
    CantBeInCorner = 4,
    MustBeAgainstWall = 5,
    CantBeThroughWall = 6,
    MustBeOnUpstairsFloorHole = 7,
    MustBeOutside = 8,
    MustBeInside = 9,
    CantIntersectOtherObjects = 10,
    MustBeOnGround = 11,
    CantFindSlot = 12,
    CantSupportWeight = 13,
    CantSupportSize = 14,
    CantPlaceOnTop = 15,
    CantPlaceOnSlope = 16,
    CantPlaceInAir = 17,
    NotAllowedOnFloor = 18,
    NotAllowedOnTerrain = 19,
    CantAfford = 20,
    MustBeAgainstUnusedWall = 21,
    MustBeOnDiagonal = 22,
    HeightNotAllowed = 23,
    InsufficientFunds = 24,
    SpecialShowMoneyError = 25,
    HasWater = 26,
    NoWater = 27,
    CantPlaceOnWater = 28,
    MustPlaceOnWater = 29,
    MustPlaceOnPool = 30,
    MustBeOnFirstLevel = 31,
    Floor2NeedsSupport = 32,
    MustRemoveObjectsOnWall = 33,
    MustBeAgainst2ndFloorWall = 34,
    CannotDeleteObject = 35,
    ObjectNotOwnedByYou = 36,
    CounterHeight = 37,
    CantBePickedup = 38,
    InUse = 39,
    CantBePickedupOutOfBounds = 40,
    CantEffectFirstLevelFromSecondLevel = 41,
    CantPlaceInAirOtherLevel = 42,
    NotAllowedOnFloorOtherLevel = 43,
    CannotPlaceComputerOnEndTable = 44,
    CannotDeletePoolWhilePeopleAreInIt = 45,
    MustRemoveLadderOrDivingBoardBeforeDeletingPool = 46,
    CannotWallpaperFence = 47,
    MustHaveMoreSpaceFromCeiling = 48,
    TooManyObjectsOnTheLot = 49,
    CantPlaceOnForSaleObject = 50,
}

impl PlacementError {
    pub fn code(self) -> i16 {
        self as i16
    }
    pub fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            -1 => Self::Success,
            0 => Self::LocationOutOfBounds,
            1 => Self::LevelOutOfBounds,
            2 => Self::MustBeAtTileCenter,
            3 => Self::MustBeInCorner,
            4 => Self::CantBeInCorner,
            5 => Self::MustBeAgainstWall,
            6 => Self::CantBeThroughWall,
            7 => Self::MustBeOnUpstairsFloorHole,
            8 => Self::MustBeOutside,
            9 => Self::MustBeInside,
            10 => Self::CantIntersectOtherObjects,
            11 => Self::MustBeOnGround,
            12 => Self::CantFindSlot,
            13 => Self::CantSupportWeight,
            14 => Self::CantSupportSize,
            15 => Self::CantPlaceOnTop,
            16 => Self::CantPlaceOnSlope,
            17 => Self::CantPlaceInAir,
            18 => Self::NotAllowedOnFloor,
            19 => Self::NotAllowedOnTerrain,
            20 => Self::CantAfford,
            21 => Self::MustBeAgainstUnusedWall,
            22 => Self::MustBeOnDiagonal,
            23 => Self::HeightNotAllowed,
            24 => Self::InsufficientFunds,
            25 => Self::SpecialShowMoneyError,
            26 => Self::HasWater,
            27 => Self::NoWater,
            28 => Self::CantPlaceOnWater,
            29 => Self::MustPlaceOnWater,
            30 => Self::MustPlaceOnPool,
            31 => Self::MustBeOnFirstLevel,
            32 => Self::Floor2NeedsSupport,
            33 => Self::MustRemoveObjectsOnWall,
            34 => Self::MustBeAgainst2ndFloorWall,
            35 => Self::CannotDeleteObject,
            36 => Self::ObjectNotOwnedByYou,
            37 => Self::CounterHeight,
            38 => Self::CantBePickedup,
            39 => Self::InUse,
            40 => Self::CantBePickedupOutOfBounds,
            41 => Self::CantEffectFirstLevelFromSecondLevel,
            42 => Self::CantPlaceInAirOtherLevel,
            43 => Self::NotAllowedOnFloorOtherLevel,
            44 => Self::CannotPlaceComputerOnEndTable,
            45 => Self::CannotDeletePoolWhilePeopleAreInIt,
            46 => Self::MustRemoveLadderOrDivingBoardBeforeDeletingPool,
            47 => Self::CannotWallpaperFence,
            48 => Self::MustHaveMoreSpaceFromCeiling,
            49 => Self::TooManyObjectsOnTheLot,
            50 => Self::CantPlaceOnForSaleObject,
            _ => return None,
        })
    }
}

pub mod flags {
    pub const ON_FLOOR: u16 = 1;
    pub const ON_TERRAIN: u16 = 1 << 1;
    pub const ON_WATER: u16 = 1 << 2;
    pub const REQUIRE_FIRST_LEVEL: u16 = 1 << 7;
    pub const ON_SLOPE: u16 = 1 << 8;
    pub const IN_AIR: u16 = 1 << 9;
    pub const IN_WALL: u16 = 1 << 10;
    pub const ALLOW_ON_POOL: u16 = 1 << 11;
    pub const REQUIRE_POOL: u16 = 1 << 12;
    pub const REQUIRE_WATER: u16 = 1 << 13;
    pub const DIAGONAL_REQUIRED: u16 = 1 << 6;
    pub const DIAGONAL_ALLOWED: u16 = 1 << 7;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementRules {
    pub flags: u16,
    pub wall_flags: u16,
    pub allowed_heights: u16,
    pub weight: i16,
    pub level_offset: i8,
    pub exclusive_wall: bool,
    pub require_center: bool,
    pub require_corner: bool,
    pub disallow_corner: bool,
    pub require_inside: bool,
    pub require_outside: bool,
    pub require_floor_hole: bool,
    pub is_avatar: bool,
    pub allow_person_intersection: bool,
    pub disallow_person_intersection: bool,
    pub zero_extent: bool,
    pub ghost: bool,
    pub ignored: BTreeSet<EntityRef>,
}

impl Default for PlacementRules {
    fn default() -> Self {
        Self {
            flags: flags::ON_FLOOR | flags::ON_TERRAIN,
            wall_flags: 0,
            allowed_heights: 1,
            weight: 0,
            level_offset: 0,
            exclusive_wall: false,
            require_center: false,
            require_corner: false,
            disallow_corner: false,
            require_inside: false,
            require_outside: false,
            require_floor_hole: false,
            is_avatar: false,
            allow_person_intersection: false,
            disallow_person_intersection: false,
            zero_extent: false,
            ghost: false,
            ignored: BTreeSet::new(),
        }
    }
}

impl PlacementRules {
    pub fn valid(&self) -> bool {
        self.wall_flags & !0x0fff == 0
            && self.flags & !0x3fff == 0
            && self.ignored.len() <= 32_768
            && self
                .ignored
                .iter()
                .all(|r| r.object_id.0 > 0 && r.generation > 0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceRequestFlags {
    pub user_buildable_limit: bool,
    pub allow_intersection: bool,
    pub accept_slots: bool,
    pub all_avatars_solid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementRequest {
    pub object: WorldObject,
    pub position: LotPosition,
    pub facing: Facing,
    pub flags: PlaceRequestFlags,
    pub tick: u64,
}

impl PlacementRequest {
    pub fn new(object: WorldObject, position: LotPosition, facing: Facing) -> Self {
        Self {
            object,
            position,
            facing,
            flags: PlaceRequestFlags::default(),
            tick: 0,
        }
    }
    fn proposed(&self) -> WorldObject {
        let mut o = self.object.clone();
        o.position = self.position;
        o.facing = self.facing;
        o
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementResult {
    pub status: PlacementError,
    pub blocker: Option<EntityRef>,
    pub support_slot: Option<SlotKey>,
}

impl PlacementResult {
    pub fn success() -> Self {
        Self {
            status: PlacementError::Success,
            blocker: None,
            support_slot: None,
        }
    }
    pub fn failure(status: PlacementError, blocker: Option<EntityRef>) -> Self {
        Self {
            status,
            blocker,
            support_slot: None,
        }
    }
    pub fn is_success(&self) -> bool {
        self.status == PlacementError::Success
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntersectionCall {
    pub token: u64,
    pub entrypoint: u8,
    pub caller: EntityRef,
    pub other: EntityRef,
    pub ghost: bool,
    pub proposed_object: EntityRef,
    pub proposed_position: LotPosition,
    pub proposed_facing: Facing,
}

pub trait IntersectionScripts {
    fn allows_intersection(&mut self, call: &IntersectionCall) -> bool;
}

impl<F: FnMut(&IntersectionCall) -> bool> IntersectionScripts for F {
    fn allows_intersection(&mut self, call: &IntersectionCall) -> bool {
        self(call)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementStep {
    Script(IntersectionCall),
    Complete(PlacementResult),
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementContinuationError {
    InvalidRequest,
    Stale,
    CallbackMismatch,
    InvalidState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementContinuation {
    pub request: PlacementRequest,
    architecture_revision: u64,
    world_revision: u64,
    slot_revision: u64,
    colliders: Vec<(EntityRef, u64)>,
    index: usize,
    side: u8,
    allowed: [bool; 2],
    pending: Option<IntersectionCall>,
    next_token: u64,
    result: PlacementResult,
    complete: bool,
}

impl PlacementContinuation {
    pub fn new<Q: WorldQuery + ?Sized>(
        request: PlacementRequest,
        query: &Q,
    ) -> Result<Self, PlacementContinuationError> {
        if !request.object.valid() || !request.facing.valid() {
            return Err(PlacementContinuationError::InvalidRequest);
        }
        let proposed = request.proposed();
        let geometry = validate_geometry(&request, query);
        let mut result = PlacementResult::success();
        let mut complete = request.flags.allow_intersection;
        if let Some(error) = geometry {
            result = PlacementResult::failure(error, None);
            complete = true;
        } else if !complete && !proposed.rules.is_avatar && proposed.rules.allowed_heights & 1 == 0
        {
            result.status = PlacementError::HeightNotAllowed;
        }
        let mut colliders: Vec<_> = if complete {
            Vec::new()
        } else {
            query
                .objects()
                .into_iter()
                .filter(|other| {
                    other.entity != proposed.entity
                        && !(proposed.multitile_group.is_some()
                            && proposed.multitile_group == other.multitile_group)
                        && !proposed.rules.ignored.contains(&other.entity)
                        && proposed.intersects(other)
                        && !(!proposed.rules.is_avatar
                            && other.rules.is_avatar
                            && proposed.rules.allow_person_intersection
                            && !proposed.rules.disallow_person_intersection
                            && !request.flags.all_avatars_solid)
                })
                .map(|other| (other.entity, other.revision))
                .collect()
        };
        colliders.sort_by_key(|(r, _)| *r);
        Ok(Self {
            request,
            architecture_revision: query.lot().revision().architecture,
            world_revision: query.object_revision(),
            slot_revision: query.slots().revision(),
            colliders,
            index: 0,
            side: 0,
            allowed: [false; 2],
            pending: None,
            next_token: 0,
            result,
            complete,
        })
    }
    pub fn pending_callback(&self) -> Option<&IntersectionCall> {
        self.pending.as_ref()
    }
    pub fn step<Q: WorldQuery + ?Sized>(&mut self, query: &Q) -> PlacementStep {
        if query.lot().revision().architecture != self.architecture_revision
            || query.object_revision() != self.world_revision
            || query.slots().revision() != self.slot_revision
        {
            return PlacementStep::Stale;
        }
        if self.complete {
            return PlacementStep::Complete(self.result.clone());
        }
        if let Some(call) = &self.pending {
            return PlacementStep::Script(call.clone());
        }
        while self.index < self.colliders.len() {
            let (entity, revision) = self.colliders[self.index];
            let Some(other) = query.object(entity) else {
                return PlacementStep::Stale;
            };
            if other.revision != revision {
                return PlacementStep::Stale;
            }
            while self.side < 2 {
                let side = usize::from(self.side);
                let (caller, second) = if side == 0 {
                    (&self.request.object, other)
                } else {
                    (other, &self.request.object)
                };
                if caller.entrypoints.contains(&5) {
                    self.next_token += 1;
                    let call = IntersectionCall {
                        token: self.next_token,
                        entrypoint: 5,
                        caller: caller.entity,
                        other: second.entity,
                        ghost: caller.rules.ghost || second.rules.ghost,
                        proposed_object: self.request.object.entity,
                        proposed_position: self.request.position,
                        proposed_facing: self.request.facing,
                    };
                    self.pending = Some(call.clone());
                    return PlacementStep::Script(call);
                }
                self.side += 1;
            }
            let permitted = self.allowed[0]
                || self.allowed[1]
                || avatar_intersection_allowed(
                    &self.request.object,
                    other,
                    self.request.flags.all_avatars_solid,
                );
            if !permitted {
                self.result = PlacementResult::failure(
                    PlacementError::CantIntersectOtherObjects,
                    Some(other.entity),
                );
                if !self.request.object.rules.is_avatar && self.request.flags.accept_slots {
                    if let Some(support) = validate_support(&self.request, other, query) {
                        self.result = support;
                        if self.result.is_success() {
                            self.complete = true;
                            return PlacementStep::Complete(self.result.clone());
                        }
                    }
                }
                if self.request.object.rules.is_avatar && other.entrypoints.contains(&26) {
                    self.complete = true;
                    return PlacementStep::Complete(self.result.clone());
                }
            }
            self.index += 1;
            self.side = 0;
            self.allowed = [false; 2];
        }
        self.complete = true;
        PlacementStep::Complete(self.result.clone())
    }
    pub fn complete_callback(
        &mut self,
        token: u64,
        allowed: bool,
    ) -> Result<(), PlacementContinuationError> {
        if self.pending.as_ref().map(|p| p.token) != Some(token) || self.side >= 2 {
            return Err(PlacementContinuationError::CallbackMismatch);
        }
        self.pending = None;
        self.allowed[usize::from(self.side)] = allowed;
        self.side += 1;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), PlacementContinuationError> {
        if !self.request.object.valid()
            || !self.request.facing.valid()
            || self.colliders.len() > 32_767
            || self.index > self.colliders.len()
            || self.side > 2
            || self.next_token > (self.colliders.len() as u64) * 2
            || self
                .pending
                .as_ref()
                .is_some_and(|p| p.token != self.next_token || self.side >= 2 || p.entrypoint != 5)
            || self.colliders.windows(2).any(|w| w[0].0 >= w[1].0)
        {
            Err(PlacementContinuationError::InvalidState)
        } else {
            Ok(())
        }
    }
}

pub fn validate_placement<Q: WorldQuery + ?Sized, S: IntersectionScripts + ?Sized>(
    request: PlacementRequest,
    query: &Q,
    scripts: &mut S,
) -> Result<PlacementResult, PlacementContinuationError> {
    let mut continuation = PlacementContinuation::new(request, query)?;
    loop {
        match continuation.step(query) {
            PlacementStep::Script(call) => {
                let allowed = scripts.allows_intersection(&call);
                continuation.complete_callback(call.token, allowed)?;
            }
            PlacementStep::Complete(result) => return Ok(result),
            PlacementStep::Stale => return Err(PlacementContinuationError::Stale),
        }
    }
}

fn avatar_intersection_allowed(target: &WorldObject, other: &WorldObject, all_solid: bool) -> bool {
    if !target.rules.is_avatar {
        return other.rules.is_avatar
            && target.rules.allow_person_intersection
            && !target.rules.disallow_person_intersection
            && !all_solid;
    }
    if other.rules.is_avatar && all_solid {
        return false;
    }
    (other.rules.is_avatar && target.rules.allow_person_intersection)
        || (other.rules.allow_person_intersection && !other.rules.disallow_person_intersection)
}

fn validate_support<Q: WorldQuery + ?Sized>(
    request: &PlacementRequest,
    other: &WorldObject,
    query: &Q,
) -> Option<PlacementResult> {
    let allowed = request.object.rules.allowed_heights;
    // VMContext holds this raw word in a signed short for the >1 surface gate.
    if (allowed as i16) <= 1 {
        return None;
    }
    let key = SlotKey {
        owner: other.entity,
        index: 0,
    };
    let definition = query.slots().get(key)?.definition.as_ref()?;
    if !query
        .slots()
        .has_capacity(key, request.object.entity, request.tick)
    {
        return Some(PlacementResult::failure(
            PlacementError::CantFindSlot,
            Some(other.entity),
        ));
    }
    if other.for_sale {
        return Some(PlacementResult::failure(
            PlacementError::CantPlaceOnForSaleObject,
            Some(other.entity),
        ));
    }
    if allowed & (1 << (definition.height - 1)) == 0 {
        return Some(PlacementResult::failure(
            if allowed & 1 != 0 {
                PlacementError::CantIntersectOtherObjects
            } else if allowed & (1 << 3) != 0 {
                PlacementError::CounterHeight
            } else if definition.height == 8 {
                PlacementError::CannotPlaceComputerOnEndTable
            } else {
                PlacementError::HeightNotAllowed
            },
            Some(other.entity),
        ));
    }
    if request.object.rules.weight >= definition.support_strength {
        return Some(PlacementResult::failure(
            PlacementError::CantSupportWeight,
            Some(other.entity),
        ));
    }
    if request.object.footprint.max_size() > definition.max_size {
        return Some(PlacementResult::failure(
            PlacementError::CantSupportSize,
            Some(other.entity),
        ));
    }
    Some(PlacementResult {
        status: PlacementError::Success,
        blocker: Some(other.entity),
        support_slot: Some(key),
    })
}

pub fn validate_geometry<Q: WorldQuery + ?Sized>(
    request: &PlacementRequest,
    query: &Q,
) -> Option<PlacementError> {
    let lot = query.lot();
    let position = request.position;
    let rules = &request.object.rules;
    if position.level == 0 || position.level > lot.levels() {
        return Some(PlacementError::LevelOutOfBounds);
    }
    let Some(tile) = position.tile() else {
        return Some(PlacementError::LocationOutOfBounds);
    };
    if !lot.contains(tile) || (request.flags.user_buildable_limit && !lot.buildable(tile)) {
        return Some(PlacementError::LocationOutOfBounds);
    }
    if rules.require_center && (position.x.rem_euclid(16) != 8 || position.y.rem_euclid(16) != 8) {
        return Some(PlacementError::MustBeAtTileCenter);
    }
    let corner = position.x.rem_euclid(16) == 0 && position.y.rem_euclid(16) == 0;
    if rules.require_corner && !corner {
        return Some(PlacementError::MustBeInCorner);
    }
    if rules.disallow_corner && corner {
        return Some(PlacementError::CantBeInCorner);
    }
    let wall = &lot.tile(tile).expect("bounded tile").wall;
    if !rules.is_avatar {
        let diag = wall.diagonal != Diagonal::None;
        if diag && rules.wall_flags & flags::DIAGONAL_ALLOWED == 0 {
            return Some(PlacementError::CantBeThroughWall);
        }
        if !diag && rules.wall_flags & flags::DIAGONAL_REQUIRED != 0 {
            return Some(PlacementError::MustBeOnDiagonal);
        }
        // VMEntity.DirectionToWallOff handles cardinals only; every diagonal uses zero.
        let wall_offset = match request.facing.0 {
            2 => 1,
            4 => 2,
            6 => 3,
            _ => 0,
        };
        let rotate = (wall_offset + 1) % 4;
        let rotate_sides =
            |sides: u8| ((u16::from(sides) << (4 - rotate)) & 15) | (u16::from(sides) >> rotate);
        let sides = rotate_sides(wall.sides);
        let occupied = rotate_sides(wall.occupied);
        if rules.wall_flags & sides != rules.wall_flags & 15 {
            return Some(PlacementError::MustBeAgainstWall);
        }
        if rules.exclusive_wall && rules.wall_flags & occupied != 0 {
            return Some(PlacementError::MustBeAgainstUnusedWall);
        }
        if rules.wall_flags & (sides << 8) != 0 {
            return Some(PlacementError::CantBeThroughWall);
        }
    }
    if rules.flags & flags::REQUIRE_FIRST_LEVEL != 0 && position.level != 1 {
        return Some(PlacementError::MustBeOnFirstLevel);
    }
    if rules.require_floor_hole && (position.level < 2 || lot.floor_at(position) != Some(0)) {
        return Some(PlacementError::MustBeOnUpstairsFloorHole);
    }
    if let Some(room) = lot.room_at(position).and_then(|r| lot.rooms().room(r)) {
        if rules.require_outside && !room.outside {
            return Some(PlacementError::MustBeOutside);
        }
        if rules.require_inside && room.outside {
            return Some(PlacementError::MustBeInside);
        }
    }
    let rects = if rules.zero_extent {
        Vec::new()
    } else {
        request.object.footprint.at(position, request.facing)
    };
    let mut floors = BTreeSet::new();
    floors.insert((tile, lot.floor_at(position).expect("bounded floor")));
    for rect in &rects {
        if rect.min_x < 0
            || rect.min_y < 0
            || rect.max_x > i32::from(lot.width()) * 16
            || rect.max_y > i32::from(lot.height()) * 16
        {
            return Some(PlacementError::LocationOutOfBounds);
        }
        for p in rect.tiles() {
            if request.flags.user_buildable_limit && !lot.buildable(p) {
                return Some(PlacementError::LocationOutOfBounds);
            }
            let Some(data) = lot.tile(p) else {
                return Some(PlacementError::LocationOutOfBounds);
            };
            let (x0, y0) = (i32::from(p.x) * 16, i32::from(p.y) * 16);
            let (left, right, top, bottom) = (
                rect.min_x.max(x0),
                rect.max_x.min(x0 + 16) - 1,
                rect.min_y.max(y0),
                rect.max_y.min(y0 + 16) - 1,
            );
            for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
                floors.insert((p, data.floor_in_half(data.wall.diagonal.half_at(x, y))));
            }
            if !rules.is_avatar
                && rules.flags & flags::IN_WALL == 0
                && crosses_cardinal_wall(*rect, p, lot)
            {
                return Some(PlacementError::CantBeThroughWall);
            }
            if !rules.is_avatar
                && data.wall.diagonal != Diagonal::None
                && rules.wall_flags & flags::DIAGONAL_ALLOWED == 0
            {
                return Some(PlacementError::CantBeThroughWall);
            }
        }
    }
    for (p, floor) in floors {
        if let Some(error) = validate_floor(rules, floor, p.level) {
            return Some(error);
        }
        if floor != 0
            && p.level > 1
            && !lot.tile(p).expect("bounded").supported
            && rules.flags & flags::IN_AIR == 0
        {
            return Some(PlacementError::CantPlaceInAir);
        }
        if !rules.is_avatar
            && lot.buildable(p)
            && lot.terrain_sloped(p)
            && rules.flags & flags::ON_FLOOR != 0
            && rules.flags & flags::ON_SLOPE == 0
        {
            return Some(PlacementError::CantPlaceOnSlope);
        }
    }
    None
}

fn crosses_cardinal_wall(rect: WorldRect, tile: TilePos, lot: &super::lot::LotModel) -> bool {
    let (x, y) = (i32::from(tile.x) * 16, i32::from(tile.y) * 16);
    [
        (Cardinal::West, x, rect.min_x, rect.max_x),
        (Cardinal::East, x + 16, rect.min_x, rect.max_x),
        (Cardinal::North, y, rect.min_y, rect.max_y),
        (Cardinal::South, y + 16, rect.min_y, rect.max_y),
    ]
    .iter()
    .any(|&(edge, coordinate, min, max)| {
        min < coordinate && max > coordinate && lot.edge_blocked(tile, edge)
    })
}

pub fn validate_floor(rules: &PlacementRules, floor: u16, level: u8) -> Option<PlacementError> {
    let flags = rules.flags;
    if floor == FLOOR_POOL {
        if flags & (flags::ALLOW_ON_POOL | flags::REQUIRE_POOL) == 0 {
            return Some(PlacementError::CantPlaceOnWater);
        }
    } else {
        if flags & flags::REQUIRE_POOL != 0 {
            return Some(PlacementError::MustPlaceOnPool);
        }
        if floor == FLOOR_WATER {
            if flags & (flags::ON_WATER | flags::REQUIRE_WATER) == 0 {
                return Some(PlacementError::CantPlaceOnWater);
            }
        } else {
            if flags & flags::REQUIRE_WATER != 0 {
                return Some(PlacementError::MustPlaceOnWater);
            }
            if floor == 0 {
                if level == 1 {
                    if flags & flags::ON_TERRAIN == 0 {
                        return Some(PlacementError::NotAllowedOnTerrain);
                    }
                } else if flags & flags::IN_AIR == 0 && rules.level_offset == 0 {
                    return Some(PlacementError::CantPlaceInAir);
                }
            } else if !rules.is_avatar
                && flags & flags::ON_FLOOR == 0
                && (rules.level_offset == 0 || flags & flags::IN_AIR == 0)
            {
                return Some(PlacementError::NotAllowedOnFloor);
            }
        }
    }
    None
}
