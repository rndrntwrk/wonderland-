//! Source-width identifiers and deterministic, generation-aware allocation.
//!
//! Raw signed legacy IDs remain representable. Only positive local IDs and
//! nonzero generations identify live entities at authority boundaries.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_LOCAL_OBJECT_ID: i16 = i16::MAX;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ObjectId(pub i16);

impl ObjectId {
    pub const NULL: Self = Self(0);

    pub const fn is_valid(self) -> bool {
        self.0 > 0
    }

    pub fn new(value: i16) -> Result<Self, IdError> {
        let id = Self(value);
        if id.is_valid() {
            Ok(id)
        } else {
            Err(IdError::InvalidObjectId(id))
        }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct PersistentId(pub u32);

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct EntityRef {
    pub object_id: ObjectId,
    pub generation: u32,
}

impl EntityRef {
    pub const fn is_valid(self) -> bool {
        self.object_id.is_valid() && self.generation != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdError {
    InvalidObjectId(ObjectId),
    InvalidGeneration,
    Exhausted,
    UnknownObject(ObjectId),
    NotAllocated(ObjectId),
    StaleReference {
        object_id: ObjectId,
        expected: u32,
        actual: u32,
    },
    AlreadyReleased(EntityRef),
    InvalidAllocatorState,
}

impl fmt::Display for IdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidObjectId(id) => write!(formatter, "local object ID {id} is not positive"),
            Self::InvalidGeneration => formatter.write_str("entity generation must be nonzero"),
            Self::Exhausted => formatter.write_str("no positive local object IDs remain"),
            Self::UnknownObject(id) => {
                write!(formatter, "local object ID {id} has never been allocated")
            }
            Self::NotAllocated(id) => write!(formatter, "local object ID {id} is not live"),
            Self::StaleReference {
                object_id,
                expected,
                actual,
            } => write!(
                formatter,
                "object {object_id} has generation {expected}, but the reference has {actual}"
            ),
            Self::AlreadyReleased(reference) => write!(
                formatter,
                "object {} generation {} was already released",
                reference.object_id, reference.generation
            ),
            Self::InvalidAllocatorState => {
                formatter.write_str("inconsistent serialized ID allocator state")
            }
        }
    }
}

impl std::error::Error for IdError {}

/// Smallest-free allocation with explicit stale-reference rejection.
///
/// Previously allocated slots retain their generation while released. Reuse
/// increments it; a released slot at `u32::MAX` is permanently retired. These
/// generation protections and rejection of exhausted ID space are intentional
/// safety changes from FreeSO's raw short allocator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "IdAllocatorState")]
pub struct IdAllocator {
    next_unused: i32,
    generations: BTreeMap<ObjectId, u32>,
    live: BTreeSet<ObjectId>,
    free: BTreeSet<ObjectId>,
}

#[derive(Deserialize)]
struct IdAllocatorState {
    next_unused: i32,
    generations: BTreeMap<ObjectId, u32>,
    live: BTreeSet<ObjectId>,
    free: BTreeSet<ObjectId>,
}

impl TryFrom<IdAllocatorState> for IdAllocator {
    type Error = IdError;

    fn try_from(state: IdAllocatorState) -> Result<Self, Self::Error> {
        let allocator = Self {
            next_unused: state.next_unused,
            generations: state.generations,
            live: state.live,
            free: state.free,
        };
        allocator.validate_state()?;
        Ok(allocator)
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdAllocator {
    pub fn new() -> Self {
        Self {
            next_unused: 1,
            generations: BTreeMap::new(),
            live: BTreeSet::new(),
            free: BTreeSet::new(),
        }
    }

    pub fn allocate(&mut self) -> Result<EntityRef, IdError> {
        if let Some(id) = self.free.iter().next().copied() {
            let generation = self
                .generations
                .get(&id)
                .ok_or(IdError::InvalidAllocatorState)?
                .checked_add(1)
                .ok_or(IdError::InvalidAllocatorState)?;
            self.free.remove(&id);
            self.generations.insert(id, generation);
            self.live.insert(id);
            return Ok(EntityRef {
                object_id: id,
                generation,
            });
        }
        if self.next_unused > i32::from(MAX_LOCAL_OBJECT_ID) {
            return Err(IdError::Exhausted);
        }
        let id = ObjectId(self.next_unused as i16);
        // The validated cursor is in 1..=32767 here, so increment cannot overflow.
        self.next_unused += 1;
        self.generations.insert(id, 1);
        self.live.insert(id);
        Ok(EntityRef {
            object_id: id,
            generation: 1,
        })
    }

    pub fn release(&mut self, reference: EntityRef) -> Result<(), IdError> {
        self.check_generation(reference)?;
        if !self.live.remove(&reference.object_id) {
            return Err(IdError::AlreadyReleased(reference));
        }
        if reference.generation != u32::MAX {
            self.free.insert(reference.object_id);
        }
        Ok(())
    }

    /// Complete a build reservation even if an unrelated lower slot was released
    /// while its durable receipt was pending. No generation or unallocated gap
    /// may be supplied by a client.
    pub(crate) fn allocate_reserved(&mut self, reference: EntityRef) -> Result<(), IdError> {
        if !reference.is_valid() || self.live.contains(&reference.object_id) {
            return Err(IdError::InvalidAllocatorState);
        }
        if self.free.contains(&reference.object_id) {
            let expected = self
                .generations
                .get(&reference.object_id)
                .and_then(|g| g.checked_add(1))
                .ok_or(IdError::InvalidAllocatorState)?;
            if expected != reference.generation {
                return Err(IdError::InvalidAllocatorState);
            }
            self.free.remove(&reference.object_id);
        } else if i32::from(reference.object_id.0) == self.next_unused && reference.generation == 1
        {
            self.next_unused += 1;
        } else {
            return Err(IdError::InvalidAllocatorState);
        }
        self.generations
            .insert(reference.object_id, reference.generation);
        self.live.insert(reference.object_id);
        Ok(())
    }

    pub fn resolve(&self, id: ObjectId) -> Result<EntityRef, IdError> {
        if !id.is_valid() {
            return Err(IdError::InvalidObjectId(id));
        }
        let generation = *self
            .generations
            .get(&id)
            .ok_or(IdError::UnknownObject(id))?;
        if !self.live.contains(&id) {
            return Err(IdError::NotAllocated(id));
        }
        Ok(EntityRef {
            object_id: id,
            generation,
        })
    }

    pub fn validate(&self, reference: EntityRef) -> Result<(), IdError> {
        self.check_generation(reference)?;
        if self.live.contains(&reference.object_id) {
            Ok(())
        } else {
            Err(IdError::NotAllocated(reference.object_id))
        }
    }

    pub fn is_live(&self, reference: EntityRef) -> bool {
        self.validate(reference).is_ok()
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Live references in the source's signed local-ID order.
    pub fn live_refs(&self) -> impl Iterator<Item = EntityRef> + '_ {
        self.live.iter().map(|id| EntityRef {
            object_id: *id,
            generation: self.generations[id],
        })
    }

    /// Validate all serialized invariants before a restored allocator is used.
    /// Deserialization calls this automatically; snapshot validation may call it
    /// again when checking entity-table consistency.
    pub fn validate_state(&self) -> Result<(), IdError> {
        if !(1..=i32::from(MAX_LOCAL_OBJECT_ID) + 1).contains(&self.next_unused)
            || self.generations.len() != (self.next_unused - 1) as usize
        {
            return Err(IdError::InvalidAllocatorState);
        }
        for (&id, &generation) in &self.generations {
            let live = self.live.contains(&id);
            let free = self.free.contains(&id);
            if !id.is_valid()
                || i32::from(id.0) >= self.next_unused
                || generation == 0
                || (live && free)
                || (free && generation == u32::MAX)
                || (!live && !free && generation != u32::MAX)
            {
                return Err(IdError::InvalidAllocatorState);
            }
        }
        if self
            .live
            .iter()
            .chain(self.free.iter())
            .any(|id| !self.generations.contains_key(id))
        {
            return Err(IdError::InvalidAllocatorState);
        }
        Ok(())
    }

    fn check_generation(&self, reference: EntityRef) -> Result<(), IdError> {
        if !reference.object_id.is_valid() {
            return Err(IdError::InvalidObjectId(reference.object_id));
        }
        if reference.generation == 0 {
            return Err(IdError::InvalidGeneration);
        }
        let expected = *self
            .generations
            .get(&reference.object_id)
            .ok_or(IdError::UnknownObject(reference.object_id))?;
        if expected != reference.generation {
            return Err(IdError::StaleReference {
                object_id: reference.object_id,
                expected,
                actual: reference.generation,
            });
        }
        Ok(())
    }
}
