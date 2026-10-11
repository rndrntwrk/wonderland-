//! Runtime-side memory semantics from VMMemory, VMEntity, VMAvatar, and VM.
//!
//! The VM owns its current registers and scope indirection. This adapter owns
//! generation checks, source-computed entity values, and atomic state/projection
//! writes. Signals describe synchronous runtime work or presentation requests;
//! they never spend money or perform external operations.
use crate::ids::{EntityRef, ObjectId};
use crate::state::{ContentSet, EntityState, SimState};
use crate::vm::{ClockKind, EntityField, MemoryAddress, VmFault, VmMode, VmThread};
use crate::world::{Diagonal, Facing, LotPosition, RoomId, WorldObject};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const OBJECT_DATA_COUNT: usize = 80;
pub const GLOBAL_COUNT: usize = 38;
pub const MAX_ATTRIBUTES: usize = 4096;
const DYNAMIC_SPRITE_COUNT: usize = 128;
const DISALLOW_PERSON_INTERSECTION: u16 = 1 << 1;
const ZERO_EXTENT: u16 = 1 << 2;
const ALLOW_PERSON_INTERSECTION: u16 = 1 << 4;
const OCCUPIED: u16 = 1 << 5;
const BURNING: u16 = 1 << 9;

/// Borrowed transient entity-thread views used while owned threads are removed
/// from `SimState` for dispatch. The current evaluation thread is deliberately
/// separate: source EvaluateCheck does not replace `Entity.Thread.Stack`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadView<'a> {
    /// Executing register/advertisement context; not automatically an entity's
    /// real stack when an explicit entity-thread overlay is supplied.
    pub current: Option<&'a VmThread>,
    /// With an overlay, lookup prefers its ObjectId entry, then stored state.
    /// Without one, `current` retains the legacy single-active-thread behavior.
    pub entity_threads: Option<&'a BTreeMap<ObjectId, VmThread>>,
}

impl<'a> ThreadView<'a> {
    pub fn active(current: Option<&'a VmThread>) -> Self {
        Self {
            current,
            entity_threads: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySignal {
    MoneyHeadline {
        entity: EntityRef,
        amount: i32,
    },
    QueueDirty {
        entity: EntityRef,
    },
    OutfitRequest {
        entity: EntityRef,
        suit: u16,
    },
    ProjectionChanged {
        entity: EntityRef,
    },
    /// The runtime applies the source entity/thread/queue reset before its next
    /// instruction. Merely clearing avatar animation state is insufficient.
    ResetRequested {
        entity: EntityRef,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryWrite {
    /// Source setter return value, including source-successful ignored writes.
    pub written: bool,
    pub signals: Vec<MemorySignal>,
}

impl MemoryWrite {
    fn accepted() -> Self {
        Self {
            written: true,
            signals: Vec::new(),
        }
    }
}

fn invalid(detail: impl Into<String>) -> VmFault {
    VmFault::InvalidContent(detail.into())
}

fn bounds(area: &str, index: u16, len: usize) -> VmFault {
    VmFault::Bounds {
        area: area.into(),
        index: i32::from(index),
        len,
    }
}

fn checked_index(area: &str, index: u16, len: usize) -> Result<usize, VmFault> {
    let index_usize = usize::from(index);
    if index_usize < len {
        Ok(index_usize)
    } else {
        Err(bounds(area, index, len))
    }
}

fn entity(state: &SimState, reference: EntityRef) -> Result<&EntityState, VmFault> {
    let value = state
        .entities
        .get(&reference.object_id)
        .ok_or(VmFault::MissingEntity(reference.object_id))?;
    if value.info.reference != reference || value.info.dead || !state.ids.is_live(reference) {
        return Err(VmFault::StaleEntity(reference));
    }
    Ok(value)
}

fn entity_by_id(state: &SimState, id: ObjectId) -> Result<&EntityState, VmFault> {
    let reference = state
        .ids
        .resolve(id)
        .map_err(|_| VmFault::MissingEntity(id))?;
    entity(state, reference)
}

fn object_data(value: &EntityState) -> Result<&[i16], VmFault> {
    if value.object_data.len() != OBJECT_DATA_COUNT {
        return Err(invalid("ObjectData must contain exactly eighty shorts"));
    }
    Ok(&value.object_data)
}

fn active_thread<'a>(
    state: &'a SimState,
    reference: EntityRef,
    threads: ThreadView<'a>,
) -> Result<Option<&'a VmThread>, VmFault> {
    let thread = match threads.entity_threads {
        Some(overlay) => overlay.get(&reference.object_id),
        None => threads
            .current
            .filter(|thread| thread.owner.object_id == reference.object_id),
    }
    .or_else(|| state.threads.get(&reference.object_id));
    if thread.is_some_and(|thread| thread.owner != reference) {
        return Err(VmFault::StaleEntity(reference));
    }
    Ok(thread)
}

fn group(state: &SimState, reference: EntityRef) -> Result<Vec<&EntityState>, VmFault> {
    let value = entity(state, reference)?;
    let mut ids: BTreeSet<_> = value.info.group.iter().copied().collect();
    ids.insert(reference.object_id);
    ids.insert(value.info.base_object);
    ids.into_iter().map(|id| entity_by_id(state, id)).collect()
}

fn initial_price(state: &SimState, value: &EntityState) -> Result<i16, VmFault> {
    let base = entity_by_id(state, value.info.base_object)?;
    Ok(base.initial_price as i16)
}

fn use_count(
    state: &SimState,
    reference: EntityRef,
    active: ThreadView<'_>,
) -> Result<i16, VmFault> {
    if active_thread(state, reference, active)?.is_none() {
        return Ok(0);
    }
    let mut users = BTreeSet::new();
    for member in group(state, reference)? {
        for user in &member.queued_users {
            // The B-owned projection is the source queue's active prefix, not
            // every queued future interaction. Source skips the tile itself.
            if *user != member.info.reference
                && entity(state, *user).is_ok_and(|user| user.info.is_avatar)
            {
                users.insert(*user);
            }
        }
    }
    Ok(users.len() as i16)
}

fn in_use(
    state: &SimState,
    reference: EntityRef,
    replacement: Option<&EntityState>,
    active: ThreadView<'_>,
) -> Result<bool, VmFault> {
    let members = group(state, reference)?;
    let references: BTreeSet<_> = members.iter().map(|member| member.info.reference).collect();
    for member in members {
        let member = replacement
            .filter(|value| value.info.reference == member.info.reference)
            .unwrap_or(member);
        if object_data(member)?[8] as u16 & OCCUPIED != 0 {
            return Ok(true);
        }
    }
    for avatar in state
        .entities
        .values()
        .filter(|value| value.info.is_avatar && !value.info.dead)
    {
        entity(state, avatar.info.reference)?;
        if let Some(thread) = active_thread(state, avatar.info.reference, active)? {
            if thread
                .frames
                .iter()
                .any(|frame| references.contains(&frame.context.callee))
            {
                return Ok(true);
            }
        }
    }
    // Source IsInUse(context, true, true) recursively calls the two-argument
    // overload, so stack-object-only references do not participate here.
    Ok(false)
}

fn safe_to_delete(
    state: &SimState,
    reference: EntityRef,
    active: ThreadView<'_>,
) -> Result<bool, VmFault> {
    let value = entity(state, reference)?;
    if let Some((container, _)) = value.container {
        if entity(state, container)?.info.is_avatar {
            return Ok(false);
        }
    }
    Ok(!in_use(state, reference, None, active)?)
}

/// Source `IsInUse(context, true)`: occupied flags across the multitile group
/// or an avatar frame whose callee belongs to that group. This is independent
/// of the stored EngineQuery result and accepts the removed executing thread.
pub fn is_in_use(
    state: &SimState,
    reference: EntityRef,
    active: Option<&VmThread>,
) -> Result<bool, VmFault> {
    is_in_use_with_threads(state, reference, ThreadView::active(active))
}

pub fn is_in_use_with_threads(
    state: &SimState,
    reference: EntityRef,
    threads: ThreadView<'_>,
) -> Result<bool, VmFault> {
    in_use(state, reference, None, threads)
}

pub fn read_memory(
    state: &SimState,
    content: &ContentSet,
    address: &MemoryAddress,
) -> Result<i16, VmFault> {
    read_memory_with_active(state, content, address, None)
}

/// Supply the current instruction's thread view when its owned thread has
/// temporarily been removed from `SimState` for dispatch.
pub fn read_memory_with_active(
    state: &SimState,
    content: &ContentSet,
    address: &MemoryAddress,
    active: Option<&VmThread>,
) -> Result<i16, VmFault> {
    read_memory_with_threads(state, content, address, ThreadView::active(active))
}

pub fn read_memory_with_threads(
    state: &SimState,
    content: &ContentSet,
    address: &MemoryAddress,
    active: ThreadView<'_>,
) -> Result<i16, VmFault> {
    match *address {
        MemoryAddress::Entity {
            entity: reference,
            field,
            index,
        } => {
            let value = entity(state, reference)?;
            match field {
                EntityField::Attribute => Ok(value
                    .attributes
                    .get(usize::from(index))
                    .copied()
                    .unwrap_or(0)),
                EntityField::ObjectData => read_object_data(state, value, index, active),
                EntityField::Temp => {
                    let at = checked_index("other entity temp", index, 20)?;
                    let thread = active_thread(state, reference, active)?.ok_or_else(|| {
                        invalid("entity has no thread for temporary-register access")
                    })?;
                    Ok(thread.temps[at])
                }
                EntityField::Motive => {
                    checked_index("motive", index, 16)?;
                    value
                        .avatar
                        .as_ref()
                        .ok_or_else(|| invalid("motive access requires an avatar"))?
                        .read_motive(index as u8)
                        .map_err(|error| invalid(format!("avatar motive: {error:?}")))
                }
                EntityField::PersonData => {
                    checked_index("person data", index, 101)?;
                    let avatar = value
                        .avatar
                        .as_ref()
                        .ok_or_else(|| invalid("person-data access requires an avatar"))?;
                    if index == 70 {
                        let mut policy = avatar.skill_policy();
                        policy.has_thread = active_thread(state, reference, active)?.is_some();
                        return Ok(policy.script_lock_mask());
                    }
                    avatar
                        .read_person_data(index)
                        .map_err(|error| invalid(format!("avatar person data: {error:?}")))
                }
                EntityField::Slot => {
                    // Game-object GetSlot tolerates any missing slot; avatar
                    // GetSlot indexes its three-element contained array.
                    if value.info.is_avatar {
                        if value.slots.len() != 3 {
                            return Err(invalid("avatar must have exactly three slots"));
                        }
                        checked_index("avatar slot", index, 3)?;
                    }
                    match value.slots.get(usize::from(index)).copied().flatten() {
                        Some(occupant) => Ok(entity(state, occupant)?.info.reference.object_id.0),
                        None => Ok(0),
                    }
                }
                EntityField::Definition | EntityField::MasterDefinition => {
                    let definition = content
                        .object(value.info.guid)
                        .ok_or_else(|| invalid("entity object definition is missing"))?;
                    let values = if field == EntityField::Definition {
                        &definition.definition
                    } else if !definition.master_definition.is_empty() {
                        &definition.master_definition
                    } else if let Some(master) = value.info.master_guid {
                        &content
                            .object(master)
                            .ok_or_else(|| invalid("master object definition is missing"))?
                            .definition
                    } else {
                        &definition.definition
                    };
                    Ok(values[checked_index("object definition", index, values.len())?])
                }
                EntityField::DynamicSpriteFlag => {
                    if value.dynamic_sprite_flags.len() != DYNAMIC_SPRITE_COUNT {
                        return Err(invalid("dynamic sprite state must contain 128 flags"));
                    }
                    Ok(i16::from(
                        value.dynamic_sprite_flags[dynamic_sprite_index(index)],
                    ))
                }
                EntityField::Function => {
                    let definition = content
                        .object(value.info.guid)
                        .ok_or_else(|| invalid("entity object definition is missing"))?;
                    let len = usize::from(definition.entry_point_count);
                    checked_index("entrypoint table", index, len)?;
                    Ok(definition
                        .entry_points
                        .get(&(index as u8))
                        .map_or(0, |routine| routine.id as i16))
                }
                EntityField::TypeAttribute if state.mode == VmMode::Tso => Ok(0),
                EntityField::TypeAttribute => Err(VmFault::HostUnsupported(
                    "TS1 neighborhood TATT/type-attribute data".into(),
                )),
            }
        }
        MemoryAddress::Global(index) => {
            if state.globals.len() != GLOBAL_COUNT {
                return Err(invalid("global state must contain 38 shorts"));
            }
            checked_index("global", index, GLOBAL_COUNT)?;
            match index {
                0 => game_component(state, 2),
                1 => game_component(state, 4),
                4 => game_component(state, 3),
                5 => game_component(state, 1),
                6 => game_component(state, 0),
                7 => game_component(state, 5),
                8 => game_component(state, 6),
                _ => Ok(state.globals[usize::from(index)]),
            }
        }
        MemoryAddress::Tuning {
            callee,
            code_owner,
            table_id,
            key_id,
            resource_mode,
        } => {
            let callee = entity(state, callee)?;
            if resource_mode > 2 {
                return Err(invalid("invalid tuning resource mode"));
            }
            if let Some(value) = callee.tuning_overrides.get(&(table_id, key_id)) {
                return Ok(*value);
            }
            let owner = match resource_mode {
                0 => code_owner,
                1 => content
                    .routines()
                    .semiglobal(code_owner)
                    .unwrap_or(code_owner),
                _ => 0,
            };
            Ok(content
                .tuning()
                .values
                .get(&(owner, table_id, key_id))
                .copied()
                .unwrap_or(0))
        }
        MemoryAddress::Room { room, index } => read_room(state, room, index),
        MemoryAddress::Clock {
            kind: ClockKind::Standard,
            index,
        } => state
            .clock
            .standard_component(index)
            .map_err(|error| VmFault::Arithmetic(format!("UTC clock: {error:?}"))),
        MemoryAddress::Clock { index, .. } => game_component(state, index),
        MemoryAddress::TreeAdvertisement { kind, index } => {
            if kind > 2 {
                return Err(invalid("invalid tree advertisement kind"));
            }
            let Some(active) = active.current else {
                return Ok(0);
            };
            let owner = entity(state, active.owner)?;
            if let Some(value) = active.tree_advertisements.get(&(kind, index)) {
                return Ok(*value);
            }
            Ok(owner
                .active_advertisements
                .as_ref()
                .and_then(|values| values.get(&(kind, index)))
                .copied()
                .unwrap_or(0))
        }
        MemoryAddress::Neighborhood { .. } => Err(VmFault::HostUnsupported(
            "TS1 neighborhood/neighbor/career memory".into(),
        )),
        MemoryAddress::MotiveLimit(index) => {
            let index = checked_index("motive limit", index as u16, 16)?;
            Ok(content.tuning().motive_limits[index])
        }
    }
}

fn read_object_data(
    state: &SimState,
    value: &EntityState,
    index: u16,
    active: ThreadView<'_>,
) -> Result<i16, VmFault> {
    let data = object_data(value)?;
    let at = checked_index("object data", index, OBJECT_DATA_COUNT)?;
    match index {
        1 => {
            if value.info.direction >= 8 {
                return Err(invalid("direction must be a canonical notch"));
            }
            Ok(i16::from(value.info.direction))
        }
        2 | 26 => match value.container {
            Some((owner, _)) => Ok(entity(state, owner)?.info.reference.object_id.0),
            None => Ok(0),
        },
        3 => match value.container {
            Some((owner, slot)) => {
                entity(state, owner)?;
                Ok(slot as i16)
            }
            None => Ok(-1),
        },
        11 => Ok(value.info.reference.object_id.0),
        25 => {
            if active_thread(state, value.info.reference, active)?.is_none() {
                return Ok(data[at]);
            }
            let elapsed = state
                .scheduler
                .current_tick()
                .checked_sub(value.lockout_started)
                .ok_or_else(|| invalid("lockout timestamp is in the future"))?;
            Ok((i128::from(data[at]) - i128::from(elapsed)).max(0) as i16)
        }
        29 if position(value)?.is_out_of_world() => Ok(-5),
        41 => initial_price(state, value),
        62 => use_count(state, value.info.reference, active),
        67 => Ok(if value.info.is_avatar {
            3
        } else {
            value.slots.len() as i16
        }),
        _ => Ok(data[at]),
    }
}

fn game_component(state: &SimState, index: i16) -> Result<i16, VmFault> {
    state
        .clock
        .validate()
        .map_err(|error| VmFault::Arithmetic(format!("game clock: {error:?}")))?;
    Ok(match index {
        0 => state
            .clock
            .seconds()
            .map_err(|error| VmFault::Arithmetic(format!("game seconds: {error:?}")))?
            as i16,
        1 => state.clock.minutes as i16,
        2 => state.clock.hours as i16,
        3 => 0, // Source VMClock.TimeOfDay deliberately returns zero.
        4 => state.clock.day_of_month as i16,
        5 => state.clock.month as i16,
        6 => state.clock.year as i16,
        _ => {
            return Err(VmFault::Bounds {
                area: "game clock".into(),
                index: i32::from(index),
                len: 7,
            })
        }
    })
}

fn read_room(state: &SimState, requested: i32, index: i16) -> Result<i16, VmFault> {
    if !(0..=4).contains(&index) {
        return Err(VmFault::Bounds {
            area: "room variable".into(),
            index: i32::from(index),
            len: 5,
        });
    }
    if index == 0 {
        return Ok(100);
    }
    let map = state.world.lot.rooms();
    let maximum = map.rooms.keys().next_back().map_or(0, |id| id.0);
    let room_id = RoomId(i64::from(requested).clamp(0, i64::from(maximum)) as u32);
    if room_id.0 == 0 {
        return Ok(0);
    } // Source default/dummy room record.
    let selected = map
        .room(room_id)
        .ok_or_else(|| invalid("room map has a missing room ID"))?;
    if index == 4 {
        return Ok(i16::from(selected.is_pool || selected.is_water));
    }
    let mut visited = BTreeSet::new();
    let mut remaining = vec![room_id];
    while let Some(id) = remaining.pop() {
        if !visited.insert(id) {
            continue;
        }
        let room = map
            .room(id)
            .ok_or_else(|| invalid("room adjacency references missing room"))?;
        remaining.extend(
            room.adjacent
                .iter()
                .filter(|id| !visited.contains(id))
                .copied(),
        );
    }
    let base_id = *visited
        .iter()
        .next()
        .ok_or_else(|| invalid("empty room component"))?;
    let base = map
        .room(base_id)
        .ok_or_else(|| invalid("room base is missing"))?;
    Ok(match index {
        1 => i16::from(base.outside),
        2 => i16::from(base.level) - 1,
        _ => {
            // Source floods full tiles once and each diagonal side once. The
            // semantic world represents every full tile as two half-cells.
            let mut source_cells = BTreeSet::new();
            for (cell, id) in &map.cells {
                if *id != base_id {
                    continue;
                }
                let tile = state
                    .world
                    .lot
                    .tile(cell.tile)
                    .ok_or_else(|| invalid("room cell is outside lot"))?;
                source_cells.insert((
                    cell.tile,
                    if tile.wall.diagonal == Diagonal::None {
                        0
                    } else {
                        cell.half
                    },
                ));
            }
            source_cells.len() as i16
        }
    })
}

fn dynamic_sprite_index(index: u16) -> usize {
    if index < 64 {
        usize::from(index)
    } else {
        64 + usize::from((index - 64) & 63)
    }
}

pub fn write_memory(
    state: &mut SimState,
    content: &ContentSet,
    address: &MemoryAddress,
    value: i16,
) -> Result<MemoryWrite, VmFault> {
    write_memory_with_active(state, content, address, value, None)
}

pub fn write_memory_with_active(
    state: &mut SimState,
    content: &ContentSet,
    address: &MemoryAddress,
    value: i16,
    active: Option<&VmThread>,
) -> Result<MemoryWrite, VmFault> {
    write_memory_with_threads(state, content, address, value, ThreadView::active(active))
}

pub fn write_memory_with_threads(
    state: &mut SimState,
    content: &ContentSet,
    address: &MemoryAddress,
    value: i16,
    active: ThreadView<'_>,
) -> Result<MemoryWrite, VmFault> {
    match *address {
        MemoryAddress::Entity {
            entity: reference,
            field,
            index,
        } => write_entity(state, content, reference, field, index, value, active),
        MemoryAddress::Global(index) => {
            if state.globals.len() != GLOBAL_COUNT {
                return Err(invalid("global state must contain 38 shorts"));
            }
            let at = checked_index("global", index, GLOBAL_COUNT)?;
            // Source stores clock-backed globals too, although GetGlobalValue
            // subsequently returns the clock rather than this raw slot.
            state.globals[at] = value;
            Ok(MemoryWrite::accepted())
        }
        MemoryAddress::Tuning { .. } | MemoryAddress::Clock { .. } => Ok(MemoryWrite::default()),
        MemoryAddress::Room { .. } => Err(VmFault::HostUnsupported("room-variable writes".into())),
        MemoryAddress::Neighborhood { .. } => Err(VmFault::HostUnsupported(
            "TS1 neighborhood/neighbor/career writes".into(),
        )),
        MemoryAddress::TreeAdvertisement { .. } => Err(VmFault::HostUnsupported(
            "tree advertisement writes belong to the executing VMThread".into(),
        )),
        MemoryAddress::MotiveLimit(_) => {
            Err(VmFault::HostUnsupported("motive-limit writes".into()))
        }
    }
}

fn write_entity(
    state: &mut SimState,
    content: &ContentSet,
    reference: EntityRef,
    field: EntityField,
    index: u16,
    value: i16,
    active: ThreadView<'_>,
) -> Result<MemoryWrite, VmFault> {
    let old = entity(state, reference)?;
    if matches!(
        field,
        EntityField::Definition | EntityField::MasterDefinition | EntityField::Function
    ) {
        return Ok(MemoryWrite::default());
    }
    if field == EntityField::Slot {
        return Err(VmFault::HostUnsupported(
            "slot memory writes; use PlaceInSlot".into(),
        ));
    }
    if field == EntityField::TypeAttribute {
        return if state.mode == VmMode::Tso {
            Ok(MemoryWrite::accepted())
        } else {
            Err(VmFault::HostUnsupported(
                "TS1 neighborhood TATT/type-attribute writes".into(),
            ))
        };
    }
    if field == EntityField::Temp {
        let at = checked_index("other entity temp", index, 20)?;
        if active
            .current
            .is_some_and(|thread| thread.owner == reference)
        {
            return Err(VmFault::HostUnsupported(
                "the VM must write its removed current thread's own temporaries".into(),
            ));
        }
        let thread = active_thread(state, reference, active)?
            .ok_or_else(|| invalid("entity has no thread for temporary-register access"))?;
        if active
            .entity_threads
            .is_some_and(|overlay| overlay.contains_key(&reference.object_id))
        {
            return Err(VmFault::HostUnsupported(
                "the runtime must write its mutable entity-thread overlay's temporaries".into(),
            ));
        }
        let next_revision = if thread.temps[at] != value {
            Some(next_revision(old)?)
        } else {
            None
        };
        state
            .threads
            .get_mut(&reference.object_id)
            .ok_or_else(|| invalid("thread disappeared before write"))?
            .temps[at] = value;
        if let Some(revision) = next_revision {
            state
                .entities
                .get_mut(&reference.object_id)
                .expect("checked entity")
                .revision = revision;
        }
        return Ok(MemoryWrite::accepted());
    }
    if field == EntityField::ObjectData && index == 41 {
        return write_group_price(state, reference, value);
    }

    let mut candidate = old.clone();
    let mut result = MemoryWrite::accepted();
    let mut projection = false;
    let mut visual_change = false;
    match field {
        EntityField::Attribute => {
            let at = checked_index("attribute capacity", index, MAX_ATTRIBUTES)?;
            if candidate.attributes.len() > MAX_ATTRIBUTES {
                return Err(invalid("attribute state exceeds capacity"));
            }
            if candidate.attributes.len() <= at {
                candidate.attributes.resize(at + 1, 0);
            }
            candidate.attributes[at] = value;
        }
        EntityField::ObjectData => {
            object_data(&candidate)?;
            let at = checked_index("object data", index, OBJECT_DATA_COUNT)?;
            let mut stored = value;
            match index {
                1 => {
                    stored = value.rem_euclid(8);
                    candidate.info.direction = stored as u8;
                    projection = true;
                }
                8 => {
                    if candidate.info.is_avatar
                        && (candidate.object_data[at] ^ value) as u16 & BURNING != 0
                    {
                        result
                            .signals
                            .push(MemorySignal::ResetRequested { entity: reference });
                    }
                    projection = true;
                }
                4 | 13 | 27 | 42 | 63 => projection = true,
                25 if active_thread(state, reference, active)?.is_some() => {
                    candidate.lockout_started = state.scheduler.current_tick()
                }
                59 => {
                    candidate.info.category = value;
                    projection = true;
                }
                79 if value == 1 => stored = i16::from(safe_to_delete(state, reference, active)?),
                _ => {}
            }
            visual_change =
                matches!(index, 0 | 1 | 8 | 34 | 40 | 59) && candidate.object_data[at] != stored;
            candidate.object_data[at] = stored;
        }
        EntityField::DynamicSpriteFlag => {
            if candidate.dynamic_sprite_flags.len() != DYNAMIC_SPRITE_COUNT {
                return Err(invalid("dynamic sprite state must contain 128 flags"));
            }
            let at = dynamic_sprite_index(index);
            visual_change = candidate.dynamic_sprite_flags[at] != (value > 0);
            candidate.dynamic_sprite_flags[at] = value > 0;
        }
        EntityField::Motive => {
            let at = checked_index("motive", index, 16)?;
            let has_thread = active_thread(state, reference, active)?.is_some();
            let avatar = candidate
                .avatar
                .as_mut()
                .ok_or_else(|| invalid("motive write requires an avatar"))?;
            avatar.motives.limits[at] = if has_thread {
                content.tuning().motive_limits[at]
            } else {
                100
            };
            avatar
                .write_motive(index as u8, value)
                .map_err(|error| invalid(format!("avatar motive: {error:?}")))?;
        }
        EntityField::PersonData => {
            checked_index("person data", index, 101)?;
            let has_thread = active_thread(state, reference, active)?.is_some();
            let avatar = candidate
                .avatar
                .as_mut()
                .ok_or_else(|| invalid("person-data write requires an avatar"))?;
            if index == 91 && value > 5 {
                return Ok(MemoryWrite::default());
            }
            avatar.has_thread = has_thread;
            let write = avatar
                .write_person_data(index, value)
                .map_err(|error| invalid(format!("avatar person data: {error:?}")))?;
            // PersonWrite.written describes mutation. SetPersonData's return
            // value is true even for blocked skills, PD70/74, and missing jobs.
            if let Some(amount) = write.money_headline {
                result.signals.push(MemorySignal::MoneyHeadline {
                    entity: reference,
                    amount: i32::from(amount),
                });
            }
            if write.queue_dirty {
                result
                    .signals
                    .push(MemorySignal::QueueDirty { entity: reference });
            }
            if let Some(suit) = write.outfit_request {
                result.signals.push(MemorySignal::OutfitRequest {
                    entity: reference,
                    suit,
                });
            }
            visual_change = matches!(index, 8 | 63 | 65 | 68 | 74 | 85);
        }
        _ => return Err(invalid("unhandled mutable entity field")),
    }
    let world_changed = if projection {
        let world = project(state, &mut candidate, active, false)?;
        prepare_revision(old, &mut candidate)?;
        state
            .world
            .replace_object(world)
            .map_err(|error| invalid(format!("world projection rejected: {error:?}")))?
    } else {
        prepare_revision(old, &mut candidate)?;
        false
    };
    // All fallible work finished before either authoritative entity commit.
    state.entities.insert(reference.object_id, candidate);
    if world_changed || visual_change {
        result
            .signals
            .push(MemorySignal::ProjectionChanged { entity: reference });
    }
    Ok(result)
}

fn next_revision(value: &EntityState) -> Result<u64, VmFault> {
    value
        .revision
        .checked_add(1)
        .ok_or_else(|| VmFault::Arithmetic("entity revision exhausted".into()))
}

fn prepare_revision(old: &EntityState, candidate: &mut EntityState) -> Result<(), VmFault> {
    if old != candidate {
        candidate.revision = next_revision(old)?;
    }
    Ok(())
}

fn write_group_price(
    state: &mut SimState,
    reference: EntityRef,
    value: i16,
) -> Result<MemoryWrite, VmFault> {
    let mut candidates = Vec::new();
    for member in group(state, reference)? {
        object_data(member)?;
        let mut candidate = member.clone();
        candidate.initial_price = i32::from(value);
        if candidate.info.reference == reference {
            candidate.object_data[41] = value;
        }
        prepare_revision(member, &mut candidate)?;
        candidates.push(candidate);
    }
    for candidate in candidates {
        state
            .entities
            .insert(candidate.info.reference.object_id, candidate);
    }
    Ok(MemoryWrite::accepted())
}

fn position(value: &EntityState) -> Result<LotPosition, VmFault> {
    let at = value.info.position;
    let level = u8::try_from(at.level).map_err(|_| invalid("negative entity floor"))?;
    Ok(LotPosition::new(i32::from(at.x), i32::from(at.y), level))
}

fn project(
    state: &SimState,
    candidate: &mut EntityState,
    active: ThreadView<'_>,
    refresh_room: bool,
) -> Result<WorldObject, VmFault> {
    let reference = candidate.info.reference;
    let data = object_data(candidate)?;
    let flags = data[8] as u16;
    let mut world = state
        .world
        .object(reference)
        .ok_or_else(|| invalid("entity has no matching world projection"))?
        .clone();
    if candidate.info.direction >= 8 {
        return Err(invalid("direction must be a canonical notch"));
    }
    world.position = position(candidate)?;
    world.facing = Facing(candidate.info.direction);
    world.rules.flags = data[42] as u16 & 0x3fff;
    world.rules.wall_flags = data[13] as u16 & 0x0fff;
    world.rules.allowed_heights = data[4] as u16;
    world.rules.weight = data[27];
    world.rules.exclusive_wall = data[63] & 2 != 0;
    world.rules.is_avatar = candidate.info.is_avatar;
    world.rules.level_offset = candidate.info.level_offset;
    world.rules.allow_person_intersection = flags & ALLOW_PERSON_INTERSECTION != 0;
    world.rules.disallow_person_intersection = flags & DISALLOW_PERSON_INTERSECTION != 0;
    world.rules.zero_extent = flags & ZERO_EXTENT != 0 || candidate.container.is_some();
    // GhostImage preview and dynamic avoidance motion are separate from avatar
    // PD68 and the raw FSODynamicFootprint bit; preserve those world-owned values.
    world.in_use = in_use(state, reference, Some(candidate), active)?;
    world.for_sale = !candidate.info.is_avatar && candidate.disabled_flags & 2 != 0;
    world.multitile_group = if candidate.info.multi_tile {
        Some(
            entity_by_id(state, candidate.info.base_object)?
                .info
                .reference,
        )
    } else {
        None
    };
    let old_position = candidate.info.position;
    let mut seen = BTreeSet::from([reference]);
    let mut container = candidate.container;
    while let Some((parent, _)) = container {
        if !seen.insert(parent) {
            return Err(invalid("cyclic entity containment"));
        }
        let parent = entity(state, parent)?;
        world.position = position(parent)?;
        candidate.info.position = parent.info.position;
        container = parent.container;
    }
    if refresh_room || old_position != candidate.info.position {
        candidate.object_data[29] = state
            .world
            .lot
            .room_at(world.position)
            .map_or(-1, |room| room.0.wrapping_sub(1) as i16);
    }
    Ok(world)
}

/// Rebuild an existing world projection after an entity/container position
/// change. The caller invokes this for affected descendants too. Both entity
/// metadata and world geometry remain unchanged on a rejected projection.
pub fn sync_projection(state: &mut SimState, reference: EntityRef) -> Result<bool, VmFault> {
    sync_projection_with_active(state, reference, None)
}

pub fn sync_projection_with_active(
    state: &mut SimState,
    reference: EntityRef,
    active: Option<&VmThread>,
) -> Result<bool, VmFault> {
    sync_projection_with_threads(state, reference, ThreadView::active(active))
}

pub fn sync_projection_with_threads(
    state: &mut SimState,
    reference: EntityRef,
    active: ThreadView<'_>,
) -> Result<bool, VmFault> {
    let old = entity(state, reference)?;
    let mut candidate = old.clone();
    candidate.info.category = object_data(&candidate)?[59];
    let projected = project(state, &mut candidate, active, true)?;
    prepare_revision(old, &mut candidate)?;
    let changed = state
        .world
        .replace_object(projected)
        .map_err(|error| invalid(format!("world projection rejected: {error:?}")))?;
    state.entities.insert(reference.object_id, candidate);
    Ok(changed)
}
