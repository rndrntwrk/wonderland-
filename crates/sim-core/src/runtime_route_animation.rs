//! Source VMRoutingFrame animation provider and authoritative runtime integration.
//! Missing source metadata leaves the already-qualified movement-only route
//! intact and emits a durable reason plus a one-shot runtime diagnostic.
use crate::{
    avatars::timeline::AnimationState,
    ids::EntityRef,
    route_motion::{
        MotionAnimation, MotionResources, RouteMotionRecord, SourceRouteMotion, SourceWalkLayer,
    },
    runtime::{avatar_mut, live_entity, live_entity_mut, lot_position, RuntimeError, RuntimeEvent},
    state::{ContentSet, ContinuationKind, SimState},
    vm::VmMode,
    world::{footprints::ObstacleMotion, Facing, RouteContinuation, RouteStep},
};

fn invalid(message: impl std::fmt::Debug) -> RuntimeError {
    RuntimeError::InvalidState(format!("source routing: {message:?}"))
}

pub(crate) fn set_auto_run(state: &mut SimState, id: u64, callee: EntityRef) {
    let eligible = live_entity(state, callee).is_ok_and(|e| e.info.guid != 0x0000_07c4);
    if let Some(RouteMotionRecord::Pending { auto_run, .. }) =
        state.source_routing.routes.get_mut(&id)
    {
        *auto_run = eligible;
    }
}

fn resolve_resources(
    state: &SimState,
    content: &ContentSet,
    route: &RouteContinuation,
    auto_run: bool,
) -> Result<(MotionResources, Option<i16>), String> {
    let actor = live_entity(state, route.actor()).map_err(|e| e.to_string())?;
    let avatar = actor.avatar.as_ref().ok_or("route actor has no avatar")?;
    if !route.source_motion_supported() {
        return Err(
            "scripted, nested, portal or cross-level route timing is not implemented".into(),
        );
    }
    let position = route.position();
    // VMRoutingFrame uses IsPool only, not the separate IsWater room flag.
    let in_pool = state
        .world
        .lot
        .room_at(position)
        .and_then(|r| state.world.lot.rooms().rooms.get(&r))
        .is_some_and(|r| r.is_pool);
    let mut style = if in_pool { 0 } else { actor.object_data[17] };
    let restore = if auto_run && avatar.persistent_id.0 > 0 && state.mode == VmMode::Tso {
        let original = style;
        let goal = &route.request().goals[0];
        let dx = (goal.position.x - position.x) as f32 / 16.;
        let dy = (goal.position.y - position.y) as f32 / 16.;
        // Preserve LotTilePos subtraction then ToVector3's (Level-1) expression.
        let dz = (i16::from(goal.position.level) - i16::from(position.level) - 1) as f32 * 2.95;
        if (dx * dx + dy * dy + dz * dz).sqrt() > 10. && !in_pool {
            style = 1;
        }
        Some(original)
    } else {
        None
    };
    let table = if in_pool {
        &avatar.swim_animations
    } else {
        &avatar.walk_animations
    };
    let full = if style == 1 { 21 } else { 20 };
    let half = if style == 1 || in_pool {
        21
    } else if avatar.is_pet() {
        20
    } else {
        25
    };
    let half = if table[half].is_empty() { full } else { half };
    let indices = [3, full, half, 4, 5, 6, 7, 8, 9];
    let mut names: [String; 9] = std::array::from_fn(|_| String::new());
    let mut frames = [0; 6];
    for (i, index) in indices.into_iter().enumerate() {
        if table[index].is_empty() {
            return Err(format!(
                "missing {} STR animation index {index}",
                if in_pool { "swim" } else { "walk" }
            ));
        }
        let requested = format!("{}.anim", table[index]);
        let metadata = content
            .animation_by_resource(&requested)
            .ok_or_else(|| format!("missing original animation metadata: {requested}"))?;
        if metadata.num_frames <= 0 {
            return Err(format!("source route animation has no frames: {requested}"));
        }
        names[i] = metadata.resource.clone();
        if i >= 3 {
            frames[i - 3] = metadata.num_frames;
        }
    }
    Ok((
        MotionResources {
            names,
            turn_frames: frames,
            walk_style: style,
            in_pool,
            pet_slow: avatar.is_pet() && avatar.person_data.values[32] < 254,
            is_pet: avatar.is_pet(),
        },
        restore,
    ))
}

pub(crate) fn prepare(
    state: &mut SimState,
    content: &ContentSet,
    id: u64,
    route: &RouteContinuation,
    events: &mut Vec<RuntimeEvent>,
) -> Result<RouteMotionRecord, RuntimeError> {
    let record = state
        .source_routing
        .routes
        .remove(&id)
        .unwrap_or(RouteMotionRecord::Pending {
            actor: route.actor(),
            auto_run: false,
        });
    // Legacy in-flight routes have no saved timing: caller supplies a record only
    // for new routes; absence is explicitly marked by the dispatch adapter.
    match record {
        RouteMotionRecord::Pending { actor, auto_run } => {
            match resolve_resources(state, content, route, auto_run) {
                Ok((resources, restore)) => {
                    let mut value = SourceRouteMotion::new(
                        actor,
                        route.position(),
                        Facing(live_entity(state, actor)?.info.direction),
                        resources,
                    );
                    if let Some(visual) = state.source_routing.avatars.get(&actor) {
                        value.radians = visual.radians;
                        value.target_radians = f64::from(visual.radians);
                        value.visual = visual.clone();
                    }
                    value.restore_walk_style = restore;
                    if restore.is_some() {
                        live_entity_mut(state, actor)?.object_data[17] = value.resources.walk_style;
                    }
                    Ok(RouteMotionRecord::Active(Box::new(value)))
                }
                Err(reason) => {
                    events.push(RuntimeEvent::RouteAnimationUnavailable {
                        continuation_id: id,
                        entity: actor,
                        reason: reason.clone(),
                    });
                    Ok(RouteMotionRecord::Unavailable { actor, reason })
                }
            }
        }
        RouteMotionRecord::Active(value) => match resolve_resources(state, content, route, false) {
            Ok((resources, _)) if resources == value.resources => {
                Ok(RouteMotionRecord::Active(value))
            }
            other => {
                finish(state, &value)?;
                // Keep the accepted source position visible for this boundary.
                // The next movement-only step invalidates this retained pose.
                if lot_position(live_entity(state, value.actor)?.info.position)?
                    == value.visual.position
                {
                    state
                        .source_routing
                        .avatars
                        .insert(value.actor, value.visual.clone());
                }
                let reason = match other {
                        Err(reason) => reason,
                        Ok(_) => "walk style, avatar type or pool context changed during a source-timed route; new source branch is not implemented".into(),
                    };
                events.push(RuntimeEvent::RouteAnimationUnavailable {
                    continuation_id: id,
                    entity: value.actor,
                    reason: reason.clone(),
                });
                Ok(RouteMotionRecord::Unavailable {
                    actor: value.actor,
                    reason,
                })
            }
        },
        other => Ok(other),
    }
}

pub(crate) fn turn_ended(
    state: &SimState,
    motion: &SourceRouteMotion,
) -> Result<bool, RuntimeError> {
    let avatar = live_entity(state, motion.actor)?
        .avatar
        .as_ref()
        .ok_or_else(|| invalid("missing motion avatar"))?;
    Ok(avatar
        .animations
        .animations
        .first()
        .is_some_and(|a| a.end_reached))
}

pub(crate) fn apply(
    state: &mut SimState,
    content: &ContentSet,
    motion: &mut SourceRouteMotion,
    before_revision: u64,
    step: &RouteStep,
) -> Result<(), RuntimeError> {
    match step {
        RouteStep::Wait { reason, .. } if *reason != crate::world::routing::WaitReason::Script => {
            motion.wait_tick()
        }
        RouteStep::Replan { .. } => {
            motion.replan();
            let actor = live_entity(state, motion.actor)?;
            let position = lot_position(actor.info.position)?;
            if motion.visual.position != position
                || motion.visual.facing != Facing(actor.info.direction)
            {
                motion.radians = f32::from(actor.info.direction) * std::f32::consts::FRAC_PI_4;
                motion.target_radians = f64::from(motion.radians);
                motion.sync_visual(position);
                motion.visual.visual_position = [
                    position.x as f32 / 16.,
                    position.y as f32 / 16.,
                    f32::from(position.level - 1) * 2.95,
                ];
            }
        }
        _ => {}
    }
    let changed = before_revision != motion.animation_revision;
    let avatar = avatar_mut(state, motion.actor)?;
    if changed {
        let new_state = |index: usize| -> Result<AnimationState, RuntimeError> {
            let metadata = content
                .animation_by_resource(&motion.resources.names[index])
                .ok_or_else(|| invalid("admitted source clip disappeared"))?;
            AnimationState::new(metadata.clone(), false).map_err(invalid)
        };
        match motion.animation {
            MotionAnimation::Walk => {
                // Source reuse predicate checks only count, standing and full.
                let reuse = avatar.animations.animations.len() == 3
                    && avatar.animations.animations[0].metadata.resource
                        == motion.resources.names[0]
                    && avatar.animations.animations[1].metadata.resource
                        == motion.resources.names[1];
                motion.walk_reused = reuse;
                if !reuse {
                    let mut layers = vec![new_state(0)?, new_state(1)?, new_state(2)?];
                    for (i, layer) in layers.iter_mut().enumerate() {
                        layer.looping = true;
                        layer.speed = if i == 0 { 1. } else { 1.5 };
                        layer.weight = [0., 0.66, 0.33][i];
                    }
                    avatar.animations.animations = layers;
                }
                motion.walk_layers = Some(std::array::from_fn(|i| {
                    let layer = &avatar.animations.animations[i];
                    SourceWalkLayer {
                        resource: layer.metadata.resource.clone(),
                        speed: layer.speed,
                        looping: layer.looping,
                        backwards: layer.backwards,
                    }
                }));
            }
            MotionAnimation::Turn(index) => {
                let mut layer = new_state(3 + usize::from(index))?;
                if motion.resources.walk_style == 1 {
                    layer.speed = 2.;
                }
                avatar.animations.animations = vec![layer];
            }
            MotionAnimation::Stand => {
                let mut layer = new_state(0)?;
                layer.looping = true;
                avatar.animations.animations = vec![layer];
            }
            MotionAnimation::None => {}
        }
    }
    if motion.animation == MotionAnimation::Walk && avatar.animations.animations.len() == 3 {
        let velocity = f32::from(motion.blend_velocity) / 8.;
        avatar.animations.animations[0].weight = 1. - velocity;
        avatar.animations.animations[1].weight = velocity * 0.66;
        avatar.animations.animations[2].weight = velocity * 0.33;
    }
    if motion.animation == MotionAnimation::Walk {
        set_motion(state, motion.actor, ObstacleMotion::MovingAvatar)?;
    }
    state
        .source_routing
        .avatars
        .insert(motion.actor, motion.visual.clone());
    Ok(())
}
fn set_motion(
    state: &mut SimState,
    actor: EntityRef,
    motion: ObstacleMotion,
) -> Result<(), RuntimeError> {
    if let Some(mut object) = state.world.object(actor).cloned() {
        if object.motion != motion {
            object.motion = motion;
            state.world.replace_object(object).map_err(invalid)?;
        }
    }
    Ok(())
}
pub(crate) fn finish(state: &mut SimState, motion: &SourceRouteMotion) -> Result<(), RuntimeError> {
    let actor = motion.actor;
    if !state.ids.is_live(actor) {
        return Ok(());
    }
    if let Some(avatar) = live_entity_mut(state, actor)?.avatar.as_mut() {
        if avatar.animations.animations.len() > 1 {
            avatar.animations.animations.truncate(1);
            avatar.animations.animations[0].weight = 1.;
        }
    }
    if let Some(style) = motion.restore_walk_style {
        live_entity_mut(state, actor)?.object_data[17] = style;
    }
    set_motion(state, actor, ObstacleMotion::StationaryAvatar)
}

/// Invalidate/rebase actor-owned source pose at every external position write.
/// The route being advanced is temporarily detached, so its own accepted move
/// publishes the precise source pose through apply instead of this reset.
pub(crate) fn invalidate_pose(state: &mut SimState, actor: EntityRef) -> Result<(), RuntimeError> {
    state.source_routing.avatars.remove(&actor);
    let entity = live_entity(state, actor)?;
    let position = lot_position(entity.info.position)?;
    let direction = entity.info.direction;
    let mut retained = None;
    for record in state.source_routing.routes.values_mut() {
        if let RouteMotionRecord::Active(motion) = record {
            if motion.actor == actor {
                motion.replan();
                motion.begin_tick();
                motion.radians = f32::from(direction) * std::f32::consts::FRAC_PI_4;
                motion.target_radians = f64::from(motion.radians);
                motion.sync_visual(position);
                motion.visual.visual_position = [
                    position.x as f32 / 16.,
                    position.y as f32 / 16.,
                    f32::from(position.level - 1) * 2.95,
                ];
                retained = Some(motion.visual.clone());
            }
        }
    }
    if let Some(visual) = retained {
        state.source_routing.avatars.insert(actor, visual);
    }
    Ok(())
}

pub(crate) fn validate(state: &SimState, content: &ContentSet) -> Result<(), String> {
    if state.source_routing.routes.len() > state.continuations.len()
        || state.source_routing.avatars.len() > state.entities.len()
    {
        return Err("source motion map exceeds owner count".into());
    }
    for (id, record) in &state.source_routing.routes {
        let continuation = state.continuations.get(id).ok_or("orphan source motion")?;
        let ContinuationKind::Route(route) = &continuation.kind else {
            return Err("source motion attached to effect".into());
        };
        if continuation.entity != record.actor() || route.actor() != record.actor() {
            return Err("source route actor mismatch".into());
        }
        match record {
            RouteMotionRecord::Unavailable { reason, .. }
                if reason.is_empty() || reason.len() > 4096 =>
            {
                return Err("invalid source route diagnostic".into());
            }
            RouteMotionRecord::Active(motion) => {
                if motion.line.as_ref().is_some_and(|l| {
                    !state.world.lot.contains_position(l.from)
                        || !state.world.lot.contains_position(l.to)
                }) {
                    return Err("source line outside lot".into());
                }
                if !motion.validate() || !route.source_line_matches(motion) {
                    return Err("invalid source route frame/line state".into());
                }
                let entity = state
                    .entities
                    .get(&motion.actor.object_id)
                    .filter(|e| e.info.reference == motion.actor)
                    .ok_or("source route actor is stale")?;
                let avatar = entity.avatar.as_ref().ok_or("source route has no avatar")?;
                // These are the context values captured by the active route.
                // A later-dispatched actor may change room/style this tick; the
                // next route dispatch qualifies the new branch or downgrades it.
                let table = if motion.resources.in_pool {
                    &avatar.swim_animations
                } else {
                    &avatar.walk_animations
                };
                let full = if motion.resources.walk_style == 1 {
                    21
                } else {
                    20
                };
                let half = if motion.resources.walk_style == 1 || motion.resources.in_pool {
                    21
                } else if motion.resources.is_pet {
                    20
                } else {
                    25
                };
                let half = if table[half].is_empty() { full } else { half };
                for (i, index) in [3, full, half, 4, 5, 6, 7, 8, 9].into_iter().enumerate() {
                    let meta = content
                        .animation_by_resource(&format!("{}.anim", table[index]))
                        .ok_or("source route clip not admitted")?;
                    if meta.resource != motion.resources.names[i]
                        || meta.num_frames <= 0
                        || (i >= 3 && meta.num_frames != motion.resources.turn_frames[i - 3])
                    {
                        return Err(
                            "source resources or turn duration do not match saved source context"
                                .into(),
                        );
                    }
                }
                if state.source_routing.avatars.get(&motion.actor) != Some(&motion.visual)
                    || lot_position(entity.info.position).map_err(|e| e.to_string())?
                        != motion.visual.position
                    || Facing(entity.info.direction) != motion.visual.facing
                {
                    return Err("embedded source pose ownership mismatch".into());
                }
                let layers = &avatar.animations.animations;
                match motion.animation {
                    MotionAnimation::Walk => {
                        let velocity = f32::from(motion.blend_velocity) / 8.;
                        let weights = [1. - velocity, velocity * 0.66, velocity * 0.33];
                        let owned = motion
                            .walk_layers
                            .as_ref()
                            .ok_or("walk timeline ownership not captured")?;
                        if layers.len() != 3
                            || layers.iter().enumerate().any(|(i, layer)| {
                                // Source's reuse predicate deliberately does not
                                // inspect the third clip or retained playback flags.
                                let selected = i < 2 || !motion.walk_reused;
                                layer.metadata.resource != owned[i].resource
                                    || layer.speed != owned[i].speed
                                    || layer.looping != owned[i].looping
                                    || layer.backwards != owned[i].backwards
                                    || content.animation_by_resource(&owned[i].resource).is_none()
                                    || (selected
                                        && layer.metadata.resource != motion.resources.names[i])
                                    || layer.weight != weights[i]
                                    || (!motion.walk_reused
                                        && (layer.speed != if i == 0 { 1. } else { 1.5 }
                                            || !layer.looping
                                            || layer.backwards))
                            })
                        {
                            return Err("walk timeline ownership mismatch".into());
                        }
                    }
                    MotionAnimation::Turn(i)
                        if layers.len() != 1
                            || layers[0].metadata.resource
                                != motion.resources.names[3 + usize::from(i)]
                            || layers[0].speed
                                != if motion.resources.walk_style == 1 {
                                    2.
                                } else {
                                    1.
                                }
                            || layers[0].weight != 1.
                            || layers[0].looping
                            || layers[0].backwards =>
                    {
                        return Err("turn timeline ownership mismatch".into());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    for (actor, motion) in &state.source_routing.avatars {
        let entity = state
            .entities
            .get(&actor.object_id)
            .filter(|e| e.info.reference == *actor && e.avatar.is_some())
            .ok_or("orphan source avatar pose")?;
        if !motion.validate()
            || lot_position(entity.info.position).map_err(|e| e.to_string())? != motion.position
            || Facing(entity.info.direction) != motion.facing
        {
            return Err("source avatar pose is stale or invalid".into());
        }
    }
    Ok(())
}
