//! Source routing-primitive adapters for the shared headless runtime.
//!
//! Operand layouts and decision order follow the six C# primitive handlers.
//! World path search and facing use the documented bounded integer model. Script
//! queries run on isolated state; asynchronous route scripts cross the typed B seam.
use crate::{
    avatars::timeline::AnimationState,
    ids::{EntityRef, ObjectId},
    runtime::{
        avatar_mut, begin_route, check_intersection, detach, live_entity, live_entity_mut,
        lot_position, move_entity, place_in_slot, ScriptBudget,
    },
    state::{ContentSet, ContinuationKind, RoutingSlot, SimState},
    vm::{HostResponse, PrimitiveExit, RouteKind, RouteRequest, VmFault},
    world::{
        self, Facing, IntersectionCall, LotPosition, PlaceRequestFlags, PlacementError,
        PlacementRequest, PlacementResult, RouteFailCode, RouteGoal, SlotKey, TilePos, WorldObject,
    },
};

const MAX_PLACEMENT_PROBES: usize = 8_192;
const ZERO_EXTENT: u16 = 1 << 2;

fn fault(message: impl std::fmt::Debug) -> VmFault {
    VmFault::HostUnsupported(format!("{message:?}"))
}
fn complete(success: bool) -> HostResponse {
    HostResponse::Complete(PrimitiveExit::branch(success))
}
fn word(operand: &[u8; 8], offset: usize) -> u16 {
    u16::from_le_bytes([operand[offset], operand[offset + 1]])
}
fn unsupported(opcode: u16, case: &str) -> VmFault {
    VmFault::UnsupportedPrimitive {
        opcode,
        case: case.into(),
    }
}
fn parameter(request: &RouteRequest, index: u16) -> Result<i16, VmFault> {
    request
        .parameters
        .get(usize::from(index))
        .copied()
        .ok_or_else(|| VmFault::Bounds {
            area: "routing parameter".into(),
            index: i32::from(index),
            len: request.parameters.len(),
        })
}

/// A provider fault cannot leave a partial detach, container, animation or RNG edit.
pub(crate) fn handle_route(
    state: &mut SimState,
    content: &ContentSet,
    owner: EntityRef,
    request: RouteRequest,
    budget: &mut ScriptBudget,
) -> Result<HostResponse, VmFault> {
    let mut staged = state.clone();
    let response = handle_inner(&mut staged, content, owner, &request, budget)?;
    *state = staged;
    Ok(response)
}

fn handle_inner(
    state: &mut SimState,
    content: &ContentSet,
    owner: EntityRef,
    request: &RouteRequest,
    budget: &mut ScriptBudget,
) -> Result<HostResponse, VmFault> {
    request.context.validate()?;
    live_entity(state, owner)?;
    live_entity(state, request.context.caller)?;
    if request.kind == RouteKind::LookTowards {
        return look_towards(state, content, owner, request);
    }
    let Some(target) = request
        .context
        .stack_object_ref
        .filter(|r| state.ids.is_live(*r))
    else {
        return Ok(complete(false));
    };
    live_entity(state, target)?;
    match request.kind {
        RouteKind::FindLocation => find_location(state, content, request, target, budget),
        RouteKind::Reach => reach(state, content, request, target),
        _ => route_slot(state, content, owner, request, target, budget),
    }
}

fn source_slot() -> RoutingSlot {
    RoutingSlot {
        search: world::slots::SlotSearch {
            min_proximity: 0,
            max_proximity: 0,
            optimal_proximity: 0,
            resolution: 16,
            directions: 0,
            ..world::slots::SlotSearch::default()
        },
        facing: -2,
        snap_to_direction: false,
        snap_target_slot: None,
    }
}
fn resolve_slot(
    content: &ContentSet,
    request: &RouteRequest,
    guid: u32,
    scope: u16,
    index: u16,
) -> Result<Option<RoutingSlot>, VmFault> {
    let (owner, index) = match scope {
        0 => {
            let index = parameter(request, index)?;
            if index < 0 {
                return Ok(None);
            }
            (guid, index as u16)
        }
        1 => (guid, index),
        2 => (0, index),
        _ => return Ok(None),
    };
    Ok(content.routing_slot(owner, index).cloned())
}

fn route_slot(
    state: &mut SimState,
    content: &ContentSet,
    owner: EntityRef,
    request: &RouteRequest,
    target: EntityRef,
    budget: &mut ScriptBudget,
) -> Result<HostResponse, VmFault> {
    let caller = request.context.caller;
    let actor = live_entity(state, caller)?.info.clone();
    let object = live_entity(state, target)?.info.clone();
    let is_snap = request.kind == RouteKind::Snap;
    if !is_snap {
        avatar_mut(state, caller)?;
        if caller != owner {
            return Err(unsupported(if request.kind == RouteKind::RelativePosition { 27 } else { 45 }, "asynchronous route actor differs from owning VM thread; separate owner/actor continuation binding is required"));
        }
    }
    if is_snap && word(&request.operand, 2) == 1 {
        return Ok(complete(contain(state, content, target, caller, 0)?));
    }
    let slot = match request.kind {
        RouteKind::RelativePosition => {
            let mut slot = source_slot();
            match request.operand[2] as i8 {
                -2 => {}
                -1 => {
                    slot.search.min_proximity = 16;
                    slot.search.max_proximity = 32;
                    slot.search.optimal_proximity = 16;
                    slot.search.directions = 255;
                }
                direction @ 0..=7 => {
                    slot.search.min_proximity = 16;
                    slot.search.max_proximity = 24;
                    slot.search.directions = 1 << direction;
                }
                _ => {
                    return Err(VmFault::InvalidOperand {
                        opcode: 27,
                        detail: "relative location must be -2, -1 or a direction notch".into(),
                    })
                }
            }
            slot.facing = match request.operand[3] as i8 {
                -1 => -3,
                value @ -2..=7 => value,
                _ => {
                    return Err(VmFault::InvalidOperand {
                        opcode: 27,
                        detail: "relative facing is outside the source cases".into(),
                    })
                }
            };
            slot
        }
        RouteKind::RoutingSlot => {
            let Some(slot) = resolve_slot(
                content,
                request,
                object.guid,
                word(&request.operand, 2),
                word(&request.operand, 0),
            )?
            else {
                return Ok(complete(false));
            };
            slot
        }
        RouteKind::Snap => {
            let mode = word(&request.operand, 2);
            if mode == 2 {
                let mut slot = source_slot();
                slot.search.min_proximity = 16;
                slot.search.directions = 1;
                slot
            } else {
                let scope = match mode {
                    0 => 0,
                    3 => 1,
                    4 => 2,
                    _ => return Ok(complete(false)),
                };
                let Some(slot) = resolve_slot(
                    content,
                    request,
                    object.guid,
                    scope,
                    word(&request.operand, 0),
                )?
                else {
                    return Ok(complete(false));
                };
                slot
            }
        }
        _ => return Err(fault("unexpected slot primitive")),
    };
    let target_position = lot_position(object.position)?;
    let actor_position = lot_position(actor.position)?;
    if !state.world.lot.contains_position(target_position)
        || (!is_snap && !state.world.lot.contains_position(actor_position))
    {
        return Ok(complete(false));
    }
    let actor_object = placement_object(state, caller)?;
    let mut rng = state.rng.clone();
    let mut query = state.clone();
    let mut query_error = None;
    let mut probes = 0;
    let mut search = slot.search.clone();
    if slot.snap_to_direction {
        // VMSlotParser handles this flag before ordinary range enumeration.
        // Offsets retain their source rotation; an exact point consumes no draw.
        search.directions = 0;
    }
    let scoring = world::slots::score_slots_with_rng(
        &search,
        target_position,
        Facing(object.direction),
        actor_position,
        &state.world.lot,
        &mut rng,
        |position, scoring_rng| {
            probes += 1;
            if probes > MAX_PLACEMENT_PROBES {
                query_error.get_or_insert_with(|| fault("SLOT placement query budget exceeded"));
                return blocked(RouteFailCode::NoValidGoals, None);
            }
            if query_error.is_some() {
                return blocked(RouteFailCode::NoValidGoals, None);
            }
            if slot.snap_target_slot.is_some() {
                return world::slots::SlotGoalValidity::Standing { congested: false };
            }
            let endpoint = LotPosition::new(target_position.x, target_position.y, position.level);
            if !world::routing::wall_line_clear(position, endpoint, &query.world) {
                return blocked(RouteFailCode::WallInWay, None);
            }
            query.rng = scoring_rng.clone();
            let mut placement =
                PlacementRequest::new(actor_object.clone(), position, Facing::NORTH);
            placement.flags.accept_slots = true;
            placement.flags.all_avatars_solid = true;
            placement.tick = state.scheduler.current_tick();
            let result = match check_placement(&query, content, placement, budget) {
                Ok(result) => result,
                Err(error) => {
                    query_error = Some(error);
                    return blocked(RouteFailCode::NoValidGoals, None);
                }
            };
            let congested = query.continuations.values().any(|continuation| {
                continuation.entity != caller
                    && match &continuation.kind {
                        ContinuationKind::Route(route) => route
                            .request()
                            .goals
                            .iter()
                            .any(|goal| goal.position == position),
                        _ => false,
                    }
            });
            if result.is_success() {
                return world::slots::SlotGoalValidity::Standing { congested };
            }
            match result.blocker.and_then(|r| query.world.object(r)) {
                Some(other) if other.rules.is_avatar => {
                    world::slots::SlotGoalValidity::Standing { congested: true }
                }
                Some(other) if slot.search.sitting > 0 && other.entrypoints.contains(&26) => {
                    let facing = slot_goal_facing(
                        &slot,
                        Facing(object.direction),
                        position,
                        target_position,
                    )
                    .unwrap_or(Facing::NORTH);
                    let difference = (other.facing.0 + 8 - facing.0) % 8;
                    if difference.min(8 - difference) > 1 {
                        blocked(RouteFailCode::NoChair, Some(other.entity))
                    } else {
                        world::slots::SlotGoalValidity::Chair {
                            entity: other.entity,
                            congested,
                        }
                    }
                }
                Some(other) => blocked(RouteFailCode::DestTileOccupied, Some(other.entity)),
                None => blocked(RouteFailCode::NoValidGoals, None),
            }
        },
    )
    .map_err(fault)?;
    if let Some(error) = query_error {
        return Err(error);
    }
    state.rng = rng;
    let goals: Vec<_> = scoring
        .choices
        .iter()
        .take(world::routing::MAX_ROUTE_GOALS)
        .map(|choice| RouteGoal {
            position: choice.position,
            facing: slot_goal_facing(
                &slot,
                Facing(object.direction),
                choice.position,
                target_position,
            ),
            chair: choice.chair,
            slot: choice.chair.and_then(|chair| {
                let key = SlotKey {
                    owner: chair,
                    index: 0,
                };
                state.world.slots.get(key).map(|_| key)
            }),
            follows_target: true,
        })
        .collect();
    if is_snap {
        if let Some(index) = slot.snap_target_slot {
            if !contain(state, content, target, caller, index)? {
                return Ok(complete(false));
            }
            if slot.snap_to_direction {
                if let Some(facing) = goals.first().and_then(|goal| goal.facing) {
                    move_entity(
                        state,
                        caller,
                        lot_position(live_entity(state, caller)?.info.position)?,
                        facing,
                    )
                    .map_err(fault)?;
                }
            }
            return Ok(complete(true));
        }
        let Some(goal) = goals.first() else {
            snap_failure(
                state,
                caller,
                scoring.fail_code == RouteFailCode::NoValidGoals,
                scoring.blocker,
            )?;
            return Ok(complete(false));
        };
        let facing = if slot.snap_to_direction {
            goal.facing.unwrap_or(Facing(actor.direction))
        } else {
            Facing(actor.direction)
        };
        let result = try_position(
            state,
            content,
            caller,
            goal.position,
            facing,
            PlaceRequestFlags::default(),
            budget,
        )?;
        if !result.is_success() {
            if request.operand[4] & 2 != 0
                && result
                    .blocker
                    .and_then(|r| state.world.object(r))
                    .is_some_and(|o| o.rules.is_avatar)
            {
                return Err(unsupported(
                    46,
                    "Snap Shoo requires B's queued goto-object interaction 3 adapter",
                ));
            }
            snap_failure(
                state,
                caller,
                result.status == PlacementError::LocationOutOfBounds,
                result.blocker,
            )?;
        }
        return Ok(complete(result.is_success()));
    }
    let failure_tree = if request.kind == RouteKind::RelativePosition {
        request.operand[6] & 2 == 0
    } else {
        request.operand[4] & 1 == 0
    };
    let failed = goals.is_empty();
    let goals = if failed {
        vec![RouteGoal::point(actor_position)]
    } else {
        goals
    };
    let id = begin_route(state, caller, Some(target), goals, true, failure_tree).map_err(fault)?;
    if failed {
        let continuation = state
            .continuations
            .get_mut(&id)
            .ok_or_else(|| fault("created route missing"))?;
        if let ContinuationKind::Route(route) = &mut continuation.kind {
            route
                .reject(scoring.fail_code, scoring.blocker)
                .map_err(fault)?;
        }
        avatar_mut(state, caller)?
            .write_person_data(62, scoring.fail_code.code())
            .map_err(fault)?;
    }
    Ok(HostResponse::Pending { request_id: id })
}

fn blocked(code: RouteFailCode, blocker: Option<EntityRef>) -> world::slots::SlotGoalValidity {
    world::slots::SlotGoalValidity::Blocked { code, blocker }
}
fn slot_goal_facing(
    slot: &RoutingSlot,
    target: Facing,
    position: LotPosition,
    center: LotPosition,
) -> Option<Facing> {
    if !slot.snap_to_direction {
        return goal_facing(slot.facing, target, position, center);
    }
    let search = &slot.search;
    let on_point = search.directions == 0
        || search.offset_x != 0
        || search.offset_y != 0
        || search.offset_z != 0;
    let base = if search.absolute && !on_point {
        Facing::NORTH
    } else {
        target
    };
    let mut bits = search.directions;
    if slot.facing >= 0 {
        bits |= 1 << slot.facing;
    } else if bits == 0 {
        bits = 1;
    }
    // Source FlagsAsRad uses round(log2(bits)), including multi-bit masks.
    // Squaring compares against the exact half-exponent without float math.
    let floor = 7 - bits.leading_zeros();
    let round_up = u32::from(bits) * u32::from(bits) > (1_u32 << (floor * 2 + 1));
    let notch = floor + u32::from(round_up);
    Some(Facing((base.0 + notch as u8) & 7))
}
fn goal_facing(
    value: i8,
    target: Facing,
    position: LotPosition,
    center: LotPosition,
) -> Option<Facing> {
    match value {
        -3 => None,
        -2 => Some(facing_to(position, center)),
        -1 => Some(facing_to(center, position)),
        _ => Some(Facing((target.0 + value as u8) & 7)),
    }
}
/// Canonical eight-notch authoritative heading, with a fixed integer diagonal basis.
fn facing_to(from: LotPosition, to: LotPosition) -> Facing {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let vectors = [
        (0, -65_536),
        (46_341, -46_341),
        (65_536, 0),
        (46_341, 46_341),
        (0, 65_536),
        (-46_341, 46_341),
        (-65_536, 0),
        (-46_341, -46_341),
    ];
    let mut best = (i64::MIN, 0_u8);
    for (direction, (x, y)) in vectors.iter().enumerate() {
        let score = dx * x + dy * y;
        if score > best.0 {
            best = (score, direction as u8);
        }
    }
    Facing(best.1)
}

fn look_towards(
    state: &mut SimState,
    _content: &ContentSet,
    owner: EntityRef,
    request: &RouteRequest,
) -> Result<HostResponse, VmFault> {
    let caller = request.context.caller;
    avatar_mut(state, caller)?;
    let mode = request.operand[0];
    if mode == 0 {
        let avatar = avatar_mut(state, caller)?;
        for (index, value) in [
            (41, request.context.stack_object.0),
            (42, 1),
            (43, 0),
            (44, 1),
            (45, 0),
        ] {
            avatar.write_person_data(index, value).map_err(fault)?;
        }
        return Ok(complete(true));
    }
    if mode == 1 {
        return Ok(complete(true));
    }
    if mode == 255 {
        return Err(unsupported(22, "FSO direct-control routing requires accepted directional input and its dedicated continuation"));
    }
    if !(2..=5).contains(&mode) {
        return Err(VmFault::InvalidOperand {
            opcode: 22,
            detail: "unknown Look Towards mode".into(),
        });
    }
    if caller != owner {
        return Err(unsupported(
            22,
            "body-turn caller differs from owning VM thread",
        ));
    }
    let target = request
        .context
        .stack_object_ref
        .ok_or(VmFault::MissingEntity(request.context.stack_object))?;
    let item = live_entity(state, target)?;
    let mut target_position = lot_position(item.info.position)?;
    if mode >= 4 {
        if item.info.group.is_empty() {
            return Err(fault("empty multitile group"));
        }
        let mut x = 0_i16;
        let mut y = 0_i16;
        for id in &item.info.group {
            let reference = state.ids.resolve(*id).map_err(fault)?;
            let position = live_entity(state, reference)?.info.position;
            // LotTilePos.operator+ narrows after each source addition.
            x = x.wrapping_add(position.x);
            y = y.wrapping_add(position.y);
        }
        target_position.x = i32::from(x) / item.info.group.len() as i32;
        target_position.y = i32::from(y) / item.info.group.len() as i32;
    }
    let position = lot_position(live_entity(state, caller)?.info.position)?;
    if !state.world.lot.contains_position(position) || target_position.is_out_of_world() {
        return Ok(complete(false));
    }
    let mut facing = facing_to(position, target_position);
    if mode == 3 || mode == 5 {
        facing.0 = (facing.0 + 4) & 7;
    }
    let goal = RouteGoal {
        position,
        facing: Some(facing),
        chair: None,
        slot: None,
        follows_target: false,
    };
    let id = begin_route(state, caller, Some(target), vec![goal], true, false).map_err(fault)?;
    Ok(HostResponse::Pending { request_id: id })
}

fn placement_object(state: &SimState, entity: EntityRef) -> Result<WorldObject, VmFault> {
    let item = live_entity(state, entity)?;
    let mut object = state
        .world
        .object(entity)
        .ok_or(VmFault::StaleEntity(entity))?
        .clone();
    // Containment removes the world footprint, but a relocation tests the real one.
    if item.container.is_some() {
        object.rules.zero_extent = item.object_data[8] as u16 & ZERO_EXTENT != 0;
    }
    Ok(object)
}
fn check_placement(
    state: &SimState,
    content: &ContentSet,
    request: PlacementRequest,
    budget: &mut ScriptBudget,
) -> Result<PlacementResult, VmFault> {
    let mut query_error = None;
    let result =
        world::validate_placement(request, &state.world, &mut |call: &IntersectionCall| {
            if query_error.is_some() {
                return false;
            }
            match check_intersection(state, content, call, budget) {
                Ok(value) => value,
                Err(error) => {
                    query_error = Some(error);
                    false
                }
            }
        })
        .map_err(fault)?;
    if let Some(error) = query_error {
        Err(error)
    } else {
        Ok(result)
    }
}

/// Shared checked EntityOperation/Snap/FindLocation relocation with atomic failure.
pub(crate) fn move_checked(
    state: &mut SimState,
    content: &ContentSet,
    entity: EntityRef,
    position: LotPosition,
    facing: Facing,
    budget: &mut ScriptBudget,
) -> Result<bool, VmFault> {
    let mut staged = state.clone();
    let result = try_position(
        &mut staged,
        content,
        entity,
        position,
        facing,
        PlaceRequestFlags::default(),
        budget,
    )?;
    if result.is_success() {
        *state = staged;
    }
    Ok(result.is_success())
}
fn try_position(
    state: &mut SimState,
    content: &ContentSet,
    entity: EntityRef,
    position: LotPosition,
    facing: Facing,
    flags: PlaceRequestFlags,
    budget: &mut ScriptBudget,
) -> Result<PlacementResult, VmFault> {
    if !facing.valid() {
        return Err(fault("invalid position direction"));
    }
    let result = if position.is_out_of_world() {
        live_entity(state, entity)?;
        PlacementResult::success()
    } else {
        let mut request = PlacementRequest::new(placement_object(state, entity)?, position, facing);
        request.flags = flags;
        request.tick = state.scheduler.current_tick();
        check_placement(state, content, request, budget)?
    };
    if !result.is_success() {
        return Ok(result);
    }
    if let Some(slot) = result.support_slot {
        if !contain(state, content, slot.owner, entity, slot.index)? {
            return Ok(PlacementResult::failure(
                PlacementError::CantFindSlot,
                Some(slot.owner),
            ));
        }
        let position = lot_position(live_entity(state, entity)?.info.position)?;
        move_entity(state, entity, position, facing).map_err(fault)?;
    } else {
        detach(state, entity).map_err(fault)?;
        move_entity(state, entity, position, facing).map_err(fault)?;
        crate::runtime_memory::sync_projection(state, entity)?;
    }
    Ok(result)
}
fn contain(
    state: &mut SimState,
    content: &ContentSet,
    container: EntityRef,
    entity: EntityRef,
    index: u16,
) -> Result<bool, VmFault> {
    let destination = live_entity(state, container)?
        .slots
        .get(usize::from(index))
        .copied();
    if destination == Some(Some(entity)) {
        return Ok(true);
    }
    if destination != Some(None) || live_entity(state, entity)?.info.dead {
        return Ok(false);
    }
    let success =
        place_in_slot(state, container, Some(entity), index as i16, true).map_err(fault)?;
    if success && index == 0 && live_entity(state, container)?.info.is_avatar {
        if let Some(metadata) = content
            .animation_metadata()
            .find(|m| m.resource == "a2o-rarm-carry-loop.anim")
        {
            avatar_mut(state, container)?.animations.carry =
                Some(AnimationState::new(metadata.clone(), false).map_err(fault)?);
        }
    }
    Ok(success)
}
fn snap_failure(
    state: &mut SimState,
    caller: EntityRef,
    no_location: bool,
    blocker: Option<EntityRef>,
) -> Result<(), VmFault> {
    let item = live_entity_mut(state, caller)?;
    if no_location {
        item.object_data[52] = 2;
    }
    item.object_data[54] = blocker.map_or(0, |r| r.object_id.0);
    item.revision = item
        .revision
        .checked_add(1)
        .ok_or_else(|| fault("entity revision overflow"))?;
    Ok(())
}

fn find_location(
    state: &mut SimState,
    content: &ContentSet,
    request: &RouteRequest,
    object: EntityRef,
    budget: &mut ScriptBudget,
) -> Result<HostResponse, VmFault> {
    let mode = request.operand[0];
    let reference_id = if request.operand[2] & 1 != 0 {
        let index = usize::from(request.operand[1]);
        let id = *request.locals.get(index).ok_or_else(|| VmFault::Bounds {
            area: "find-location local".into(),
            index: index as i32,
            len: request.locals.len(),
        })?;
        ObjectId(id)
    } else {
        request.context.caller.object_id
    };
    let flags = PlaceRequestFlags {
        accept_slots: true,
        user_buildable_limit: request.operand[2] & 4 != 0,
        ..PlaceRequestFlags::default()
    };
    if mode == 1 {
        return Ok(complete(
            try_position(
                state,
                content,
                object,
                LotPosition::OUT_OF_WORLD,
                Facing::NORTH,
                flags,
                budget,
            )?
            .is_success(),
        ));
    }
    if mode == 2 {
        let caller = live_entity(state, request.context.caller)?.info.position;
        let callee = live_entity(state, request.context.callee)?.info.position;
        let position = LotPosition::new(
            (i32::from(caller.x) + i32::from(callee.x)) / 2 - 8,
            (i32::from(caller.y) + i32::from(callee.y)) / 2 - 8,
            ((i16::from(caller.level) + i16::from(callee.level)) / 2) as u8,
        );
        return Ok(complete(
            try_position(
                state,
                content,
                object,
                position,
                Facing::NORTH,
                PlaceRequestFlags::default(),
                budget,
            )?
            .is_success(),
        ));
    }
    // Source resolves a nullable local reference before the switch, but out-of-world,
    // smoke and unknown modes never dereference it.
    if mode > 5 {
        return Ok(complete(false));
    }
    let reference = state
        .ids
        .resolve(reference_id)
        .map_err(|_| VmFault::MissingEntity(reference_id))?;
    let ref_info = live_entity(state, reference)?.info.clone();
    let ref_position = lot_position(ref_info.position)?;
    if !state.world.lot.contains_position(ref_position) {
        return Ok(complete(false));
    }
    if mode == 5 {
        let width = state.world.lot.width();
        let height = state.world.lot.height();
        if width <= 2 || height <= 2 {
            return Ok(complete(false));
        }
        for _ in 0..100 {
            let x = state.rng.next(u64::from(width - 2)) as i16 + 1;
            let y = state.rng.next(u64::from(height - 2)) as i16 + 1;
            if try_position(
                state,
                content,
                object,
                TilePos::new(x, y, ref_position.level).center(),
                Facing::NORTH,
                flags,
                budget,
            )?
            .is_success()
            {
                return Ok(complete(true));
            }
        }
        return Ok(complete(false));
    }
    let mut candidates = Vec::new();
    match mode {
        0 => {
            let base = ref_position
                .tile()
                .ok_or_else(|| fault("reference tile overflow"))?;
            for radius in 0_i16..10 {
                if radius == 0 {
                    for facing in [Facing::NORTH, Facing::EAST, Facing::SOUTH, Facing::WEST] {
                        candidates.push((ref_position, facing));
                    }
                } else {
                    for x in -radius..=radius {
                        for j in 0_i16..8 {
                            candidates.push((
                                TilePos::new(
                                    base.x + x,
                                    base.y + ((j % 2) * 2 - 1) * radius,
                                    base.level,
                                )
                                .center(),
                                Facing(((j / 2) * 2) as u8),
                            ));
                        }
                    }
                    for y in 1 - radius..radius {
                        for j in 0_i16..8 {
                            candidates.push((
                                TilePos::new(
                                    base.x + ((j % 2) * 2 - 1) * radius,
                                    base.y + y,
                                    base.level,
                                )
                                .center(),
                                Facing(((j / 2) * 2) as u8),
                            ));
                        }
                    }
                }
            }
        }
        3 | 4 => {
            let direction = (ref_info.direction + if mode == 4 { 2 } else { 0 }) & 7;
            let vectors = [
                (0, 16),
                (-16, 16),
                (-16, 0),
                (-16, -16),
                (0, -16),
                (16, -16),
                (16, 0),
                (16, 16),
            ];
            let (dx, dy) = vectors[usize::from(direction)];
            for index in 0..32 {
                let distance = index / 2;
                if let Some(position) = ref_position.offset(dx * distance, dy * distance, 0) {
                    candidates.push((position, Facing(direction)));
                }
                if index % 2 != 0 {
                    if let Some(position) = ref_position.offset(-dx * distance, -dy * distance, 0) {
                        candidates.push((position, Facing(direction)));
                    }
                }
            }
        }
        _ => return Ok(complete(false)),
    }
    let defer_occupied = request.operand[2] & 2 == 0;
    let mut deferred = Vec::new();
    for (position, facing) in candidates {
        if defer_occupied
            && state
                .world
                .objects()
                .values()
                .any(|o| o.position.tile() == position.tile())
        {
            deferred.push((position, facing));
            continue;
        }
        if try_position(state, content, object, position, facing, flags, budget)?.is_success() {
            return Ok(complete(true));
        }
    }
    for (position, facing) in deferred {
        if try_position(state, content, object, position, facing, flags, budget)?.is_success() {
            return Ok(complete(true));
        }
    }
    Ok(complete(false))
}

fn slot_height(state: &SimState, container: EntityRef, index: u16) -> Result<Option<i32>, VmFault> {
    let Some(definition) = state
        .world
        .slots
        .get(SlotKey {
            owner: container,
            index,
        })
        .and_then(|slot| slot.definition.as_ref())
    else {
        return Ok(None);
    };
    // SLOT.HeightOffsets; source rounds 2.5 to even 2 before selecting the reach asset.
    Ok(Some(match definition.height {
        1 | 6 | 9 => 0,
        2 => 2,
        3 | 4 | 8 => 4,
        7 => 7,
        5 => {
            return Err(unsupported(
                47,
                "custom SLOT height requires imported type-0 OffsetZ metadata",
            ))
        }
        _ => {
            return Err(VmFault::InvalidOperand {
                opcode: 47,
                detail: "SLOT height exceeds the source offset table".into(),
            })
        }
    }))
}
fn reach(
    state: &mut SimState,
    content: &ContentSet,
    request: &RouteRequest,
    target: EntityRef,
) -> Result<HostResponse, VmFault> {
    let caller = request.context.caller;
    avatar_mut(state, caller)?;
    let mode = word(&request.operand, 0);
    let mut completed_pickup = false;
    let mut target_slot = None;
    let height = match mode {
        0 => match live_entity(state, target)?.container {
            None => 0,
            Some((container, index)) => {
                completed_pickup = container == caller;
                if live_entity(state, container)?.info.is_avatar {
                    0
                } else {
                    slot_height(state, container, index)?.unwrap_or(0)
                }
            }
        },
        1 => {
            let index = parameter(request, word(&request.operand, 4))?;
            if index < 0
                || live_entity(state, target)?
                    .slots
                    .get(index as usize)
                    .is_none()
            {
                return Ok(complete(false));
            }
            target_slot = Some(index as u16);
            let Some(height) = slot_height(state, target, index as u16)? else {
                return Ok(complete(false));
            };
            height
        }
        _ => {
            return Err(unsupported(
                47,
                "source Reach to Mouth and modes above 1 are unimplemented",
            ))
        }
    };
    let name = if live_entity(state, caller)?.container.is_some() {
        "a2o-sit-reach-table.anim"
    } else if height < 2 {
        "a2o-reach-floorht.anim"
    } else if height < 4 {
        "a2o-reach-seatht.anim"
    } else {
        "a2o-reach-tableht.anim"
    };
    let Some(metadata) = content.animation_metadata().find(|m| m.resource == name) else {
        return Ok(complete(false));
    };
    let avatar = avatar_mut(state, caller)?;
    if avatar
        .animations
        .animations
        .first()
        .map_or(true, |animation| {
            animation.metadata.resource != name && !completed_pickup
        })
    {
        avatar.animations.animations =
            vec![AnimationState::new(metadata.clone(), false).map_err(fault)?];
        avatar.animations.left_hand = 0;
        avatar.animations.right_hand = 0;
        return Ok(HostResponse::NextTick);
    }
    let current = &avatar.animations.animations[0];
    // Reach checks completion before queued xevts, unlike VMAnimateSim.
    if current.end_reached {
        avatar.animations.animations.clear();
        return Ok(complete(true));
    }
    let event = current.event_queue.front().copied();
    if event == Some(0) {
        let holding = live_entity(state, caller)?.slots.first().copied().flatten();
        if mode == 0 {
            if holding.is_none() {
                contain(state, content, caller, target, 0)?;
            }
        } else {
            let index = target_slot.expect("mode one checked");
            let item = live_entity(state, target)?.slots[usize::from(index)];
            match (holding, item) {
                (None, Some(item)) => {
                    contain(state, content, caller, item, 0)?;
                }
                (Some(item), None) => {
                    contain(state, content, target, item, index)?;
                }
                _ => {}
            }
        }
    }
    if event.is_some() {
        avatar_mut(state, caller)?.animations.animations[0]
            .event_queue
            .pop_front();
    }
    Ok(HostResponse::NextTick)
}
