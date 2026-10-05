use super::{
    footprints::WorldObject,
    lot::LotModel,
    routing::RouteFailCode,
    tiles::{integer_sqrt, round_div_even, Facing, LotPosition},
};
use crate::{
    ids::{EntityRef, ObjectId},
    rng::SimRng,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_SLOTS: usize = 32_768;
pub const MAX_SLOT_CAPACITY: u16 = 64;
pub const MAX_RESERVATION_TICKS: u32 = 9_000;
pub const MAX_SLOT_CANDIDATES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SlotKey {
    pub owner: EntityRef,
    pub index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotDefinition {
    pub key: SlotKey,
    pub capacity: u16,
    /// Source height is one-based; allowed height flag is 1 << (height - 1).
    pub height: u8,
    pub support_strength: i16,
    pub max_size: u16,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ReservationToken {
    pub slot: SlotKey,
    pub actor: EntityRef,
    pub operation: u64,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    pub token: ReservationToken,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationRequest {
    pub slot: SlotKey,
    pub actor: EntityRef,
    pub operation: u64,
    pub expected_revision: u64,
    pub tick: u64,
    pub duration_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotError {
    InvalidDefinition,
    MissingOwner,
    MissingActor,
    MissingSlot,
    StaleRevision,
    Full,
    InvalidRequest,
    TokenMismatch,
    Expired,
    Limit,
    RevisionExhausted,
    InvalidState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotEntry {
    pub definition: Option<SlotDefinition>,
    pub reservations: BTreeMap<ReservationToken, Reservation>,
    pub occupants: BTreeMap<EntityRef, ReservationToken>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotState {
    entries: BTreeMap<SlotKey, SlotEntry>,
    next_sequence: u64,
    revision: u64,
}

fn live(objects: &BTreeMap<ObjectId, WorldObject>, entity: EntityRef) -> bool {
    objects
        .get(&entity.object_id)
        .map_or(false, |o| o.entity == entity)
}

impl SlotState {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn entries(&self) -> &BTreeMap<SlotKey, SlotEntry> {
        &self.entries
    }
    pub fn get(&self, key: SlotKey) -> Option<&SlotEntry> {
        self.entries.get(&key)
    }
    pub fn define(
        &mut self,
        mut definition: SlotDefinition,
        objects: &BTreeMap<ObjectId, WorldObject>,
    ) -> Result<bool, SlotError> {
        if !live(objects, definition.key.owner) {
            return Err(SlotError::MissingOwner);
        }
        if definition.capacity == 0
            || definition.capacity > MAX_SLOT_CAPACITY
            || definition.height == 0
            || definition.height > 16
            || definition.max_size == 0
        {
            return Err(SlotError::InvalidDefinition);
        }
        if let Some(old) = self.entries.get(&definition.key) {
            if usize::from(definition.capacity) < old.reservations.len() + old.occupants.len() {
                return Err(SlotError::Full);
            }
            definition.revision = old
                .definition
                .as_ref()
                .ok_or(SlotError::InvalidState)?
                .revision;
            if old.definition.as_ref() == Some(&definition) {
                return Ok(false);
            }
        } else if self.entries.len() >= MAX_SLOTS {
            return Err(SlotError::Limit);
        }
        self.bump()?;
        definition.revision = self.revision;
        let key = definition.key;
        self.entries.entry(key).or_default().definition = Some(definition);
        Ok(true)
    }
    fn bump(&mut self) -> Result<(), SlotError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(SlotError::RevisionExhausted)?;
        Ok(())
    }
    pub fn has_capacity(&self, key: SlotKey, actor: EntityRef, tick: u64) -> bool {
        self.entries.get(&key).map_or(false, |entry| {
            if entry.occupants.contains_key(&actor)
                || entry
                    .reservations
                    .values()
                    .any(|r| r.token.actor == actor && r.expires_at > tick)
            {
                return true;
            }
            entry.definition.as_ref().map_or(false, |d| {
                entry.occupants.len()
                    + entry
                        .reservations
                        .values()
                        .filter(|r| r.expires_at > tick)
                        .count()
                    < usize::from(d.capacity)
            })
        })
    }
    pub fn reserve(
        &mut self,
        request: ReservationRequest,
        objects: &BTreeMap<ObjectId, WorldObject>,
    ) -> Result<ReservationToken, SlotError> {
        if request.operation == 0
            || request.duration_ticks == 0
            || request.duration_ticks > MAX_RESERVATION_TICKS
        {
            return Err(SlotError::InvalidRequest);
        }
        if !live(objects, request.slot.owner) {
            return Err(SlotError::MissingOwner);
        }
        if !live(objects, request.actor) {
            return Err(SlotError::MissingActor);
        }
        let expires_at = request
            .tick
            .checked_add(u64::from(request.duration_ticks))
            .ok_or(SlotError::InvalidRequest)?;
        let entry = self
            .entries
            .get(&request.slot)
            .ok_or(SlotError::MissingSlot)?;
        if let Some(token) = entry
            .occupants
            .get(&request.actor)
            .filter(|t| t.operation == request.operation)
        {
            return Ok(*token);
        }
        if let Some(r) = entry.reservations.values().find(|r| {
            r.token.actor == request.actor
                && r.token.operation == request.operation
                && r.expires_at > request.tick
        }) {
            return Ok(r.token);
        }
        if entry
            .definition
            .as_ref()
            .ok_or(SlotError::InvalidState)?
            .revision
            != request.expected_revision
        {
            return Err(SlotError::StaleRevision);
        }
        if entry.occupants.contains_key(&request.actor)
            || entry
                .reservations
                .values()
                .any(|r| r.token.actor == request.actor && r.expires_at > request.tick)
        {
            return Err(SlotError::Full);
        }
        if entry.occupants.len()
            + entry
                .reservations
                .values()
                .filter(|r| r.expires_at > request.tick && live(objects, r.token.actor))
                .count()
            >= usize::from(entry.definition.as_ref().expect("checked").capacity)
        {
            return Err(SlotError::Full);
        }
        let sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(SlotError::RevisionExhausted)?;
        self.bump()?;
        self.next_sequence = sequence;
        let token = ReservationToken {
            slot: request.slot,
            actor: request.actor,
            operation: request.operation,
            sequence,
        };
        let entry = self.entries.get_mut(&request.slot).expect("checked");
        entry
            .reservations
            .retain(|_, r| r.expires_at > request.tick && live(objects, r.token.actor));
        entry
            .reservations
            .insert(token, Reservation { token, expires_at });
        Ok(token)
    }
    pub fn occupy(
        &mut self,
        token: ReservationToken,
        tick: u64,
        objects: &BTreeMap<ObjectId, WorldObject>,
    ) -> Result<bool, SlotError> {
        if !live(objects, token.slot.owner) {
            return Err(SlotError::MissingOwner);
        }
        if !live(objects, token.actor) {
            return Err(SlotError::MissingActor);
        }
        let entry = self
            .entries
            .get(&token.slot)
            .ok_or(SlotError::MissingSlot)?;
        if entry.occupants.get(&token.actor) == Some(&token) {
            return Ok(false);
        }
        let reservation = entry
            .reservations
            .get(&token)
            .ok_or(SlotError::TokenMismatch)?;
        if reservation.expires_at <= tick {
            return Err(SlotError::Expired);
        }
        self.bump()?;
        let entry = self.entries.get_mut(&token.slot).expect("checked");
        entry.reservations.remove(&token);
        entry.occupants.insert(token.actor, token);
        Ok(true)
    }
    pub fn release(&mut self, token: ReservationToken) -> Result<bool, SlotError> {
        let Some(entry) = self.entries.get(&token.slot) else {
            return Ok(false);
        };
        let reserved = entry.reservations.contains_key(&token);
        let occupied = entry.occupants.get(&token.actor) == Some(&token);
        if !reserved && !occupied {
            return Ok(false);
        }
        self.bump()?;
        let entry = self.entries.get_mut(&token.slot).expect("checked");
        entry.reservations.remove(&token);
        if occupied {
            entry.occupants.remove(&token.actor);
        }
        Ok(true)
    }
    pub fn expire_and_remove_stale(
        &mut self,
        tick: u64,
        objects: &BTreeMap<ObjectId, WorldObject>,
    ) -> Result<usize, SlotError> {
        let mut next = self.entries.clone();
        let before = self.count_references();
        next.retain(|key, entry| {
            if !live(objects, key.owner) {
                return false;
            }
            entry
                .reservations
                .retain(|_, r| r.expires_at > tick && live(objects, r.token.actor));
            entry.occupants.retain(|actor, _| live(objects, *actor));
            true
        });
        let after: usize = next
            .values()
            .map(|e| e.occupants.len() + e.reservations.len() + 1)
            .sum();
        if next != self.entries {
            self.bump()?;
            self.entries = next;
        }
        Ok(before.saturating_sub(after))
    }
    fn count_references(&self) -> usize {
        self.entries
            .values()
            .map(|e| e.occupants.len() + e.reservations.len() + 1)
            .sum()
    }
    pub fn validate(&self, objects: &BTreeMap<ObjectId, WorldObject>) -> Result<(), SlotError> {
        if self.entries.len() > MAX_SLOTS {
            return Err(SlotError::InvalidState);
        }
        let mut sequences = BTreeSet::new();
        for (key, entry) in &self.entries {
            let d = entry.definition.as_ref().ok_or(SlotError::InvalidState)?;
            if d.key != *key
                || !live(objects, key.owner)
                || d.capacity == 0
                || d.capacity > MAX_SLOT_CAPACITY
                || d.height == 0
                || d.height > 16
                || d.max_size == 0
                || d.revision > self.revision
                || entry.reservations.len() + entry.occupants.len() > usize::from(d.capacity)
            {
                return Err(SlotError::InvalidState);
            }
            let mut actors = BTreeSet::new();
            for token in entry.reservations.keys().chain(entry.occupants.values()) {
                if token.slot != *key
                    || !live(objects, token.actor)
                    || token.operation == 0
                    || token.sequence == 0
                    || token.sequence > self.next_sequence
                    || !sequences.insert(token.sequence)
                    || !actors.insert(token.actor)
                {
                    return Err(SlotError::InvalidState);
                }
            }
            if entry
                .reservations
                .iter()
                .any(|(token, r)| *token != r.token)
                || entry.occupants.iter().any(|(a, t)| *a != t.actor)
            {
                return Err(SlotError::InvalidState);
            }
        }
        Ok(())
    }
}

/// Integer SLOT projection. Distances are subtile units; scores use 1/1024 units.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSearch {
    pub min_proximity: i32,
    pub max_proximity: i32,
    pub optimal_proximity: i32,
    pub resolution: i32,
    pub directions: u8,
    pub offset_x: i32,
    pub offset_y: i32,
    pub offset_z: i32,
    pub absolute: bool,
    pub ignore_rooms: bool,
    pub square: bool,
    pub equal_proximity_score: bool,
    pub random_scoring: bool,
    pub standing: i32,
    pub sitting: i32,
}

impl Default for SlotSearch {
    fn default() -> Self {
        Self {
            min_proximity: 16,
            max_proximity: 16,
            optimal_proximity: 16,
            resolution: 4,
            directions: 255,
            offset_x: 0,
            offset_y: 0,
            offset_z: 0,
            absolute: false,
            ignore_rooms: false,
            square: false,
            equal_proximity_score: false,
            random_scoring: false,
            standing: 1,
            sitting: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotGoalValidity {
    Standing {
        congested: bool,
    },
    Chair {
        entity: EntityRef,
        congested: bool,
    },
    Blocked {
        code: RouteFailCode,
        blocker: Option<EntityRef>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoredSlot {
    pub position: LotPosition,
    pub score: i64,
    pub chair: Option<EntityRef>,
    pub entry_flags: u8,
    pub enumeration_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotSearchResult {
    pub choices: Vec<ScoredSlot>,
    pub fail_code: RouteFailCode,
    pub blocker: Option<EntityRef>,
    pub scoring_draws: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotSearchError {
    InvalidParameters,
    TooManyCandidates,
    InvalidTarget,
}

pub fn score_slots<F: FnMut(LotPosition) -> SlotGoalValidity>(
    search: &SlotSearch,
    target: LotPosition,
    facing: Facing,
    caller: LotPosition,
    lot: &LotModel,
    rng: &mut SimRng,
    mut verify: F,
) -> Result<SlotSearchResult, SlotSearchError> {
    score_slots_with_rng(search, target, facing, caller, lot, rng, |position, _| {
        verify(position)
    })
}

/// Read-only script adapters can observe the scoring draw's RNG position while
/// keeping any random draws inside an intersection check isolated from the route.
pub fn score_slots_with_rng<F: FnMut(LotPosition, &SimRng) -> SlotGoalValidity>(
    search: &SlotSearch,
    target: LotPosition,
    facing: Facing,
    caller: LotPosition,
    lot: &LotModel,
    rng: &mut SimRng,
    mut verify: F,
) -> Result<SlotSearchResult, SlotSearchError> {
    if !lot.contains_position(target)
        || (!lot.contains_position(caller) && !caller.is_out_of_world())
        || !facing.valid()
    {
        return Err(SlotSearchError::InvalidTarget);
    }
    let min = search.min_proximity;
    let max = if search.max_proximity == 0 {
        min
    } else {
        search.max_proximity
    };
    let optimal = if search.optimal_proximity == 0 {
        min
    } else {
        search.optimal_proximity
    };
    if min < 0
        || max < min
        || max > 1_024
        || optimal < min
        || optimal > max
        || search.resolution <= 0
        || search.resolution > 1_024
        || search.standing < 0
        || search.sitting < 0
        || search.standing > 1_024
        || search.sitting > 1_024
        || [search.offset_x, search.offset_y, search.offset_z]
            .iter()
            .any(|n| n.unsigned_abs() > 4_096)
    {
        return Err(SlotSearchError::InvalidParameters);
    }
    let on_point = search.directions == 0
        || search.offset_x != 0
        || search.offset_y != 0
        || search.offset_z != 0;
    let rotation = if search.absolute && !on_point {
        Facing::NORTH
    } else {
        facing
    };
    let (ox, oy) = rotation.rotate(search.offset_x, search.offset_y);
    let level_offset = i8::try_from(search.offset_z.div_euclid(15))
        .map_err(|_| SlotSearchError::InvalidParameters)?;
    let center = target
        .offset(ox, oy, level_offset)
        .ok_or(SlotSearchError::InvalidTarget)?;
    let mut candidates = Vec::new();
    if on_point {
        candidates.push((0, 0, 0_i64));
    } else if max == min {
        let diagonal = round_div_even(i64::from(min) * 46_341, 65_536) as i32;
        for (x, y) in [
            (0, min),
            (diagonal, diagonal),
            (min, 0),
            (diagonal, -diagonal),
            (0, -min),
            (-diagonal, -diagonal),
            (-min, 0),
            (-diagonal, diagonal),
        ] {
            candidates.push((x, y, i64::from(min) * 1_024));
        }
    } else {
        let bound = max / search.resolution * search.resolution;
        let side = (2 * bound / search.resolution + 1) as usize;
        if side * side > MAX_SLOT_CANDIDATES {
            return Err(SlotSearchError::TooManyCandidates);
        }
        let mut x = -bound;
        while x <= bound {
            let mut y = -bound;
            while y <= bound {
                let distance = if search.square {
                    i64::from(x.abs().max(y.abs())) * 1_024
                } else {
                    integer_sqrt(
                        ((i64::from(x) * i64::from(x) + i64::from(y) * i64::from(y)) * 1_048_576)
                            as u64,
                    ) as i64
                };
                candidates.push((x, y, distance));
                y += search.resolution;
            }
            x += search.resolution;
        }
    }
    let source_room = lot.room_at(target);
    let only_sit = search.sitting > 0 && search.standing == 0;
    let mut result = SlotSearchResult {
        choices: Vec::new(),
        fail_code: if only_sit {
            RouteFailCode::NoChair
        } else {
            RouteFailCode::NoValidGoals
        },
        blocker: None,
        scoring_draws: 0,
    };
    let max_score = i64::from((optimal - min).max(max - optimal)) * 1_024
        + (integer_sqrt(target.distance_squared(caller) * 1_048_576) as i64
            + i64::from(max) * 1_024)
            / 3
        + 2 * 1_024;
    for (index, (x, y, distance)) in candidates.into_iter().enumerate() {
        let Some(position) = center.offset(x, y, 0) else {
            continue;
        };
        let entry_flags = if on_point {
            search.directions
        } else {
            slot_search_directions(center, position, rotation) & search.directions
        };
        if !on_point
            && (distance < i64::from(min) * 1_024 - 512
                || distance > i64::from(max) * 1_024 + 512
                || (!search.ignore_rooms && lot.room_at(position) != source_room)
                || entry_flags == 0)
        {
            continue;
        }
        // Source draws here, before out-of-bounds/wall/occupied verification.
        let mut score = if on_point {
            i64::MAX / 4
        } else if search.equal_proximity_score {
            max_score
        } else {
            result.scoring_draws += 1;
            let random = rng.next(1_024) as i64;
            if search.random_scoring {
                random
            } else {
                random + max_score - (i64::from(optimal) * 1_024 - distance).abs()
            }
        };
        if !lot.contains_position(position) {
            continue;
        }
        if !search.random_scoring && !on_point {
            score -= integer_sqrt(position.distance_squared(caller) * 1_048_576) as i64 / 3;
        }
        let (chair, congested, weight) = match verify(position, rng) {
            SlotGoalValidity::Standing { congested } if !only_sit => {
                (None, congested, search.standing)
            }
            SlotGoalValidity::Chair { entity, congested } if search.sitting > 0 => {
                (Some(entity), congested, search.sitting)
            }
            SlotGoalValidity::Blocked { code, blocker } => {
                if fail_priority(code) > fail_priority(result.fail_code) {
                    result.fail_code = code;
                    result.blocker = blocker;
                }
                continue;
            }
            _ => continue,
        };
        score = score.saturating_mul(i64::from(weight));
        if congested {
            score /= 100_000;
        }
        result.choices.push(ScoredSlot {
            position,
            score,
            chair,
            entry_flags,
            enumeration_index: index as u32,
        });
    }
    result.choices.sort_by(|a, b| b.score.cmp(&a.score));
    if !result.choices.is_empty() {
        result.fail_code = RouteFailCode::Success;
        result.blocker = None;
    }
    Ok(result)
}

fn fail_priority(code: RouteFailCode) -> i8 {
    match code {
        RouteFailCode::NoValidGoals => 0,
        RouteFailCode::NoChair => 1,
        RouteFailCode::DestTileOccupiedPerson => 2,
        RouteFailCode::DestTileOccupied => 3,
        _ => -1,
    }
}

/// Source GetSearchDirection floors both positions to whole tiles and returns
/// overlapping 89.8-degree directional sectors (ANGLE_ERROR is -0.1 degrees).
/// The fixed tan(44.9 degrees) comparison removes platform atan2 dependence.
pub fn slot_search_directions(center: LotPosition, position: LotPosition, facing: Facing) -> u8 {
    if !facing.valid() {
        return 0;
    }
    let mut x = i64::from(position.x.div_euclid(16)) - i64::from(center.x.div_euclid(16));
    let mut y = i64::from(position.y.div_euclid(16)) - i64::from(center.y.div_euclid(16));
    if x == 0 && y == 0 {
        x = 0;
        y = -1;
    } // Math.Atan2(0,0) is zero (north).
    let directions = [
        (0_i64, -1_i64),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ];
    let mut result = 0;
    for relative in 0_u8..8 {
        let (dx, dy) = directions[usize::from((relative + facing.0) & 7)];
        let dot = x * dx + y * dy;
        let cross = (x * dy - y * dx).abs();
        if dot > 0 && cross * 1_000_000 <= dot * 996_515 {
            result |= 1 << relative;
        }
    }
    result
}
