//! Replicated relationship matrices; durable persistence remains an external authority concern.
use super::VmFault;
use crate::ids::EntityRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_RELATIONSHIP_MATRICES: usize = 65_536;
pub const MAX_RELATIONSHIP_BOOKKEEPING: usize = 65_536;
pub const MAX_RELATIONSHIP_COLUMNS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationshipOwner {
    Entity(EntityRef),
    Neighbor(i16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationshipTarget {
    Local(EntityRef),
    Persistent(u32),
    Neighbor(i16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelationshipKey {
    pub owner: RelationshipOwner,
    pub target: RelationshipTarget,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipBook {
    pub matrices: BTreeMap<RelationshipKey, Vec<i16>>,
    pub changed_persistent: BTreeSet<(EntityRef, u32)>,
    pub local_reverse: BTreeMap<EntityRef, BTreeSet<EntityRef>>,
}
impl RelationshipBook {
    pub fn read(&self, key: RelationshipKey) -> Option<Vec<i16>> {
        self.matrices.get(&key).cloned()
    }
    pub fn write(&mut self, key: RelationshipKey, values: Vec<i16>) -> Result<(), VmFault> {
        validate_key(key)?;
        if values.len() > MAX_RELATIONSHIP_COLUMNS {
            return Err(VmFault::InvalidContent(
                "Relationship variable count exceeds byte index".into(),
            ));
        }
        if !self.matrices.contains_key(&key) && self.matrices.len() >= MAX_RELATIONSHIP_MATRICES {
            return Err(VmFault::InvalidContent(
                "Relationship matrix count exceeds limit".into(),
            ));
        }
        self.matrices.insert(key, values);
        Ok(())
    }
    pub fn mark(&mut self, key: RelationshipKey, persistent_dirty: bool) -> Result<(), VmFault> {
        validate_key(key)?;
        if let RelationshipOwner::Entity(owner) = key.owner {
            match key.target {
                RelationshipTarget::Persistent(target) if persistent_dirty => {
                    if !self.changed_persistent.contains(&(owner, target))
                        && self.changed_persistent.len() >= MAX_RELATIONSHIP_BOOKKEEPING
                    {
                        return Err(VmFault::InvalidContent(
                            "Changed relationship count exceeds limit".into(),
                        ));
                    }
                    self.changed_persistent.insert((owner, target));
                }
                RelationshipTarget::Local(target) => {
                    let owners = self.local_reverse.get(&target);
                    if owners.is_none() && self.local_reverse.len() >= MAX_RELATIONSHIP_BOOKKEEPING
                    {
                        return Err(VmFault::InvalidContent(
                            "Relationship reverse target count exceeds limit".into(),
                        ));
                    }
                    if !owners
                        .map(|values| values.contains(&owner))
                        .unwrap_or(false)
                        && self.reverse_edge_count() >= MAX_RELATIONSHIP_BOOKKEEPING
                    {
                        return Err(VmFault::InvalidContent(
                            "Relationship reverse edge count exceeds limit".into(),
                        ));
                    }
                    self.local_reverse.entry(target).or_default().insert(owner);
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn delete_entity(&mut self, entity: EntityRef) {
        self.matrices.retain(|key, _| {
            key.owner != RelationshipOwner::Entity(entity)
                && key.target != RelationshipTarget::Local(entity)
        });
        self.changed_persistent
            .retain(|(owner, _)| *owner != entity);
        self.local_reverse.remove(&entity);
        for owners in self.local_reverse.values_mut() {
            owners.remove(&entity);
        }
    }
    pub fn validate(&self) -> Result<(), VmFault> {
        if self.matrices.len() > MAX_RELATIONSHIP_MATRICES
            || self.changed_persistent.len() > MAX_RELATIONSHIP_BOOKKEEPING
            || self.local_reverse.len() > MAX_RELATIONSHIP_BOOKKEEPING
            || self.reverse_edge_count() > MAX_RELATIONSHIP_BOOKKEEPING
        {
            return Err(VmFault::InvalidContent(
                "Relationship collection bounds".into(),
            ));
        }
        for (key, values) in &self.matrices {
            validate_key(*key)?;
            if values.len() > MAX_RELATIONSHIP_COLUMNS {
                return Err(VmFault::InvalidContent("Relationship list bounds".into()));
            }
        }
        for (owner, target) in &self.changed_persistent {
            validate_key(RelationshipKey {
                owner: RelationshipOwner::Entity(*owner),
                target: RelationshipTarget::Persistent(*target),
            })?;
        }
        // These are historical bookkeeping supersets: GenericTSOCall ClearRelationships
        // clears matrices without clearing either ChangedRels or MayHaveRelToMe.
        for (target, owners) in &self.local_reverse {
            if !valid_entity(*target) || owners.iter().any(|owner| !valid_entity(*owner)) {
                return Err(VmFault::InvalidContent(
                    "Relationship reverse identity bounds".into(),
                ));
            }
        }
        Ok(())
    }
    fn reverse_edge_count(&self) -> usize {
        self.local_reverse
            .values()
            .fold(0usize, |count, owners| count.saturating_add(owners.len()))
    }
}

fn valid_entity(entity: EntityRef) -> bool {
    entity.object_id.0 > 0 && entity.generation != 0
}
fn validate_key(key: RelationshipKey) -> Result<(), VmFault> {
    let valid = match (key.owner, key.target) {
        (RelationshipOwner::Entity(owner), RelationshipTarget::Local(target)) => {
            valid_entity(owner) && valid_entity(target)
        }
        (RelationshipOwner::Entity(owner), RelationshipTarget::Persistent(target)) => {
            valid_entity(owner) && target != 0
        }
        (RelationshipOwner::Neighbor(_), RelationshipTarget::Neighbor(_)) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(VmFault::InvalidContent(
            "Relationship identity bounds".into(),
        ))
    }
}
