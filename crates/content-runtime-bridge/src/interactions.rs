//! B-owned queue and content adapters for A's public runtime surface. This
//! module never supplies invented check output or pushes a substitute VM frame.
use crate::budget::ImportBudget;
use interaction_rules::{ActionQueue, EntityKey, LegacyMode};
use sim_core::{
    ids::{EntityRef, ObjectId},
    runtime::{AcceptedCommand, AcceptedTick, SimRuntime},
    vm::VmMode,
};
use std::collections::{BTreeMap, BTreeSet};
use wonderland_legacy_formats::Limits;

#[path = "interaction_queries.rs"]
mod queries;
pub use queries::*;

pub const MAX_PROJECTION_BYTES: usize = 8 * 1024 * 1024;

pub fn entity_key(entity: EntityRef) -> EntityKey {
    EntityKey {
        slot: entity.object_id.0 as u32,
        generation: entity.generation,
    }
}

pub fn entity_ref(entity: EntityKey) -> Result<EntityRef, String> {
    if entity.slot == 0 || entity.slot > i16::MAX as u32 || entity.generation == 0 {
        return Err(
            "interaction identity requires a positive i16 ObjectID and nonzero generation".into(),
        );
    }
    Ok(EntityRef {
        object_id: ObjectId(entity.slot as i16),
        generation: entity.generation,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectionLimits {
    pub max_queues: usize,
    pub max_active_entries: usize,
    pub max_bytes: usize,
}
impl Default for ProjectionLimits {
    fn default() -> Self {
        Self {
            max_queues: 32767,
            max_active_entries: 131072,
            max_bytes: MAX_PROJECTION_BYTES,
        }
    }
}

/// Detached, source-state-bound accepted command preparation. Constructing this
/// object is not authentication. The authoritative scheduler supplies all its
/// queues; there is deliberately no network/JSON constructor for queue state.
pub struct PreparedProjection {
    state_hash: [u8; 32],
    commands: Vec<AcceptedCommand>,
}
impl PreparedProjection {
    pub fn commands(&self) -> &[AcceptedCommand] {
        &self.commands
    }

    /// Projections precede the supplied trusted commands in the same real
    /// accepted transaction. A validates all commands and commits atomically.
    /// Any intervening state/epoch/tick change invalidates this preparation.
    pub fn into_tick(
        mut self,
        runtime: &SimRuntime,
        tail: Vec<AcceptedCommand>,
    ) -> Result<AcceptedTick, String> {
        let count = self
            .commands
            .len()
            .checked_add(tail.len())
            .ok_or("projection command count overflow")?;
        if count > runtime.state().limits.max_commands_per_tick as usize {
            return Err("projection plus command tail exceeds accepted command limit".into());
        }
        if runtime.state_hash().map_err(|error| error.to_string())? != self.state_hash {
            return Err("prepared projection belongs to an earlier runtime state".into());
        }
        self.commands.extend(tail);
        runtime
            .next_tick(self.commands)
            .map_err(|error| error.to_string())
    }
}

/// Project the COMPLETE authoritative B queue set into the real VM's UseCount
/// inputs. The active prefix includes suspended parent actions; future entries
/// do not count. Multiple active actions by one avatar on a target count once.
/// Missing queues clear prior users, including after finish/disconnect cleanup.
/// Retained adapter allocations are admitted before cloning. A's own canonical
/// state hashing and accepted transaction have their separate runtime limits.
pub fn project_active_queues(
    runtime: &SimRuntime,
    queues: &[&ActionQueue],
    limits: ProjectionLimits,
) -> Result<PreparedProjection, String> {
    if limits.max_bytes == 0
        || limits.max_bytes > MAX_PROJECTION_BYTES
        || limits.max_queues == 0
        || limits.max_queues > 32767
        || limits.max_active_entries == 0
        || limits.max_active_entries > 131072
        || queues.len() > limits.max_queues
    {
        return Err("queue projection limits exceeded".into());
    }
    let mut budget = ImportBudget::new(&Limits {
        max_total_decoded_bytes: limits.max_bytes,
        ..Limits::default()
    });
    budget.map_entries::<EntityRef, ()>(queues.len())?;
    let mut owners = BTreeSet::new();
    let mut users: BTreeMap<EntityRef, BTreeSet<EntityRef>> = BTreeMap::new();
    let mut count = 0usize;
    let mode = if runtime.state().mode == VmMode::Ts1 {
        LegacyMode::Ts1
    } else {
        LegacyMode::Tso
    };
    for queue in queues {
        let owner = entity_ref(queue.owner())?;
        if !runtime.state().ids.is_live(owner)
            || !runtime.state().entities[&owner.object_id].info.is_avatar
        {
            return Err("queue owner is not a live avatar generation".into());
        }
        if queue.mode() != mode {
            return Err("queue and runtime modes differ".into());
        }
        if !owners.insert(owner) {
            return Err("duplicate queue owner in complete projection".into());
        }
        count = count
            .checked_add(queue.active_entries().len())
            .filter(|count| *count <= limits.max_active_entries)
            .ok_or("active queue entry projection limit")?;
        for entry in queue.active_entries() {
            if entry.invocation.actor != queue.owner() {
                return Err("active queue actor differs from owner".into());
            }
            let target = entity_ref(entry.invocation.target)?;
            if !runtime.state().ids.is_live(target) {
                return Err("active queue target is not a live generation".into());
            }
            // Charge every edge, including duplicates, before map insertion.
            budget.map_entries::<EntityRef, BTreeSet<EntityRef>>(1)?;
            budget.map_entries::<EntityRef, ()>(1)?;
            users.entry(target).or_default().insert(owner);
        }
    }
    let empty = BTreeSet::new();
    let changed =
        || {
            runtime.state().entities.values().filter(|item| {
                users.get(&item.info.reference).unwrap_or(&empty) != &item.queued_users
            })
        };
    let count = changed().count();
    if count > runtime.state().limits.max_commands_per_tick as usize {
        return Err("queue projection exceeds accepted command limit".into());
    }
    budget.entries::<AcceptedCommand>(count)?;
    for item in changed() {
        budget.map_entries::<EntityRef, ()>(
            users.get(&item.info.reference).unwrap_or(&empty).len(),
        )?;
        budget.map_entries::<(u8, u16), i16>(
            item.active_advertisements.as_ref().map_or(0, BTreeMap::len),
        )?;
    }
    let commands = changed()
        .map(|item| AcceptedCommand::SetInteractionProjection {
            entity: item.info.reference,
            queued_users: users.get(&item.info.reference).unwrap_or(&empty).clone(),
            active_advertisements: item.active_advertisements.clone(),
        })
        .collect();
    Ok(PreparedProjection {
        state_hash: runtime.state_hash().map_err(|error| error.to_string())?,
        commands,
    })
}
