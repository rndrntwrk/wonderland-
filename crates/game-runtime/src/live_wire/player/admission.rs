//! Revalidate a selected source action at the native authority boundary.
//!
//! Ordinary network latency must not make an otherwise available menu selection
//! expire at every tick. Only the observation stamps may be refreshed: the exact
//! target generation, source action/parameter, queue revision and sequence remain
//! the player's selection. The caller obtains principal/actor from authenticated
//! lot admission, not from the packet, and commits the returned intent under the
//! same authority lock. This helper does not execute or authenticate anything.
use super::{PlayerAction, Result, decode_action};
use crate::{
    EntityRef, GameRuntime, PrincipalKey, QueryOptions, RuntimeRole, entity_key, entity_ref,
};

/// Bounded tolerance for an observation captured before ordinary tick delivery.
/// Older selections must be explicitly chosen again; never automatically replayed.
pub const MAX_SELECTION_AGE_TICKS: u64 = 128;

pub fn revalidate_action(
    server: &GameRuntime,
    principal: PrincipalKey,
    actor: EntityRef,
    bytes: &[u8],
) -> Result<PlayerAction> {
    if server.sim().role() != RuntimeRole::Authority {
        return Err("Action admission requires the authority runtime");
    }
    let mut action = decode_action(bytes)?;
    let state = server.sim().state();
    let entity = state
        .entities
        .get(&actor.object_id)
        .filter(|entity| {
            state.ids.is_live(actor) && entity.info.reference == actor && !entity.info.dead
        })
        .ok_or("The admitted actor is no longer live")?;
    if !state
        .interaction_access
        .get(&actor)
        .is_some_and(|access| access.principal == principal)
    {
        return Err("The admitted actor is no longer authorized");
    }
    let queue = state
        .interaction_queues
        .get(&actor.object_id)
        .filter(|queue| queue.owner() == entity_key(actor))
        .ok_or("The admitted actor queue is unavailable")?;
    if queue
        .last_command_sequence()
        .is_some_and(|last| action.sequence() <= last)
    {
        return Err("The action sequence was already accepted");
    }
    let (submitted_principal, actor_stamp, observed_tick, queue_revision) = match &action {
        PlayerAction::Invoke(intent) => (
            intent.principal,
            intent.seen.actor,
            intent.seen.world_revision,
            intent.queue_revision,
        ),
        PlayerAction::Cancel(intent) => (
            intent.principal,
            intent.actor,
            intent.world_revision,
            intent.queue_revision,
        ),
    };
    if submitted_principal != principal || actor_stamp.key != entity_key(actor) {
        return Err("The action does not match authenticated actor admission");
    }
    if state
        .completed_tick
        .checked_sub(observed_tick)
        .is_none_or(|age| age > MAX_SELECTION_AGE_TICKS)
    {
        return Err("The action observation is future or expired");
    }
    if actor_stamp.revision > entity.revision {
        return Err("The action actor observation is from the future");
    }
    if queue_revision != queue.revision() {
        return Err("The action queue has changed");
    }
    match &mut action {
        PlayerAction::Invoke(intent) => {
            let target = entity_ref(intent.seen.target.key)
                .map_err(|_| "The selected target is no longer live")?;
            let target_entity = state
                .entities
                .get(&target.object_id)
                .filter(|target_entity| {
                    state.ids.is_live(target)
                        && target_entity.info.reference == target
                        && !target_entity.info.dead
                })
                .ok_or("The selected target is no longer live")?;
            if intent.seen.target.revision > target_entity.revision {
                return Err("The action target observation is from the future");
            }
            // This executes the actual source checks on detached state. There is
            // no fallback to an old offer, hidden action or invented Param0.
            let batch = server
                .offers(principal, actor, target, QueryOptions::default())
                .map_err(|_| "Source action revalidation failed")?;
            if !batch.offers.iter().any(|offer| {
                offer.interaction == intent.interaction && offer.param0 == intent.param0
            }) {
                return Err("The selected source action is no longer available");
            }
            intent.seen = batch.seen;
        }
        PlayerAction::Cancel(intent) => {
            if !queue.entries().iter().enumerate().any(|(index, entry)| {
                entry.id == intent.action && queue.entry_visible(index) == Some(true)
            }) {
                return Err("The selected queue action is no longer visible");
            }
            intent.world_revision = state.completed_tick;
            intent.actor.revision = entity.revision;
        }
    }
    Ok(action)
}
