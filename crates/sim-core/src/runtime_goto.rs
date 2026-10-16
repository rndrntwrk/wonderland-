//! Source VMNetGotoCmd / UILotControl terrain actions. A terrain click selects
//! the original local interaction of GUID 0x7C4; it never commands a route or
//! changes the avatar position. Ghost queries own all temporary state and RNG.
use super::*;
use crate::interactions as ix;
use interaction_adapter::{entity_key, entity_ref};

pub const SOURCE_GOTO_GUID: u32 = 0x0000_07c4;

/// Fixed-width append-only player payload. Keep declaration order stable.
/// Position and local interaction widths match VMNetGotoCmd exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceGotoIntent {
    pub principal: ix::PrincipalKey,
    pub actor: ix::EntityVersion,
    pub world_revision: u64,
    pub queue_revision: u64,
    pub command_sequence: u64,
    pub x: i16,
    pub y: i16,
    pub level: i8,
    pub interaction: u16,
    pub param0: i16,
}
impl SourceGotoIntent {
    pub fn position(self) -> Result<LotPosition, RuntimeError> {
        Ok(LotPosition::new(
            i32::from(self.x),
            i32::from(self.y),
            u8::try_from(self.level).map_err(invalid)?,
        ))
    }
}

/// No provisional ghost entity ID escapes to the browser or accepted journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainOffers {
    pub world_revision: u64,
    pub actor: ix::EntityVersion,
    pub queue_revision: u64,
    pub position: LotPosition,
    pub offers: Vec<ix::InteractionOffer>,
}
impl SimRuntime {
    /// Source GetAction availability only. Authentication and observation/queue
    /// stamps belong to the caller; the real CheckAction runs in the queue.
    pub fn validate_source_goto_selection(&self, interaction: u16) -> Result<(), RuntimeError> {
        validate_selection(&self.content, interaction)
    }
    pub fn terrain_offers(
        &self,
        principal: ix::PrincipalKey,
        actor: EntityRef,
        position: LotPosition,
    ) -> Result<TerrainOffers, RuntimeError> {
        terrain_offers(&self.state, &self.content, principal, actor, position)
            .map(|(batch, _)| batch)
    }
}

fn actor_stamp(
    state: &SimState,
    principal: ix::PrincipalKey,
    actor: EntityRef,
) -> Result<ix::EntityVersion, RuntimeError> {
    let entity = live_entity(state, actor)?;
    if state.mode != VmMode::Tso
        || entity.info.dead
        || entity.avatar.is_none()
        || !state
            .interaction_access
            .get(&actor)
            .is_some_and(|a| a.principal == principal)
    {
        return Err(invalid(
            "source GoTo requires an authenticated live TSO avatar",
        ));
    }
    let queue = state
        .interaction_queues
        .get(&actor.object_id)
        .ok_or_else(|| invalid("source GoTo actor queue missing"))?;
    if queue.owner() != entity_key(actor) || queue.entries().len() >= 20 {
        return Err(invalid("source GoTo user queue is full or stale"));
    }
    Ok(ix::EntityVersion {
        key: entity_key(actor),
        revision: entity.revision,
    })
}

/// VMContext.SolidToAvatars uses tile membership and raw intersection flags,
/// not a route query or the helper object's own zero-extent footprint.
fn check_tile(state: &SimState, position: LotPosition) -> Result<(), RuntimeError> {
    vm_position(position)?;
    let tile = position
        .tile()
        .ok_or_else(|| invalid("source GoTo tile missing"))?;
    if position.is_out_of_world()
        || !state.world.lot.contains_position(position)
        || position.x.rem_euclid(16) != 8
        || position.y.rem_euclid(16) != 8
        || (position.level != 1 && state.world.lot.floor_at(position).unwrap_or(0) == 0)
    {
        return Err(invalid("source GoTo tile is outside the traversable lot"));
    }
    for object in state.world.objects().values() {
        if object.rules.ghost
            || object.position.is_out_of_world()
            || object.position.tile() != Some(tile)
        {
            continue;
        }
        if object.rules.disallow_person_intersection
            || !(object.rules.allow_person_intersection || object.rules.zero_extent)
        {
            return Err(invalid("source GoTo tile is solid to avatars"));
        }
    }
    Ok(())
}

fn require_content(content: &ContentSet) -> Result<(), RuntimeError> {
    let object = content
        .object(SOURCE_GOTO_GUID)
        .ok_or_else(|| invalid("source GoTo GUID missing"))?;
    if object.master_guid.is_some()
        || object.level_offset != 0
        || object.footprint.rects.is_empty()
        || !object.entry_points.contains_key(&0)
        || !object.entry_points.contains_key(&1)
    {
        return Err(invalid(
            "source GoTo creation metadata missing or unsupported",
        ));
    }
    let table = content
        .interaction_table(SOURCE_GOTO_GUID)
        .filter(|t| t.local_table_present)
        .ok_or_else(|| invalid("source GoTo TTAB missing"))?;
    for definition in &table.definitions {
        if content
            .routines()
            .resolve(
                definition.action.code_owner_guid,
                definition.action.routine_id,
            )
            .is_none()
            || definition.check.is_some_and(|b| {
                content
                    .routines()
                    .resolve(b.code_owner_guid, b.routine_id)
                    .is_none()
            })
        {
            return Err(invalid("source GoTo action/check routine missing"));
        }
    }
    Ok(())
}

fn validate_selection(content: &ContentSet, interaction: u16) -> Result<(), RuntimeError> {
    require_content(content)?;
    let found = content
        .interaction_table(SOURCE_GOTO_GUID)
        .is_some_and(|table| {
            table.definitions.iter().any(|d| {
                d.key.scope == ix::InteractionScope::Local
                    && d.key.tta_index == u32::from(interaction)
            })
        });
    if !found {
        return Err(invalid("source GoTo local interaction missing"));
    }
    Ok(())
}

fn terrain_offers(
    state: &SimState,
    content: &ContentSet,
    principal: ix::PrincipalKey,
    actor: EntityRef,
    position: LotPosition,
) -> Result<(TerrainOffers, u32), RuntimeError> {
    let actor_version = actor_stamp(state, principal, actor)?;
    // Patched source selection checks can write SocialTargetID and dispatch
    // named trees when CurrentObject is set. Detached UI queries cannot yet
    // preserve that persistent selection-mode state. This is a query support
    // bound, never an extra restriction on the source authority command.
    if live_entity(state, actor)?
        .avatar
        .as_ref()
        .is_some_and(|avatar| avatar.person_data.values[76] != 0)
    {
        return Err(invalid(
            "source terrain selection mode is not supported by detached queries",
        ));
    }
    check_tile(state, position)?;
    require_content(content)?;
    let queue_revision = state.interaction_queues[&actor.object_id].revision();
    let mut detached = state.clone();
    let mut work = TickWork {
        strict_source: true,
        ..Default::default()
    };
    let ghost = create_source_object(&mut detached, content, &mut work, position, true)?;
    let (mut batch, checked) = interaction_adapter::offers_in_state_counted(
        &detached,
        content,
        principal,
        actor,
        ghost,
        ix::QueryOptions::default(),
    )
    .map_err(invalid)?;
    // UIPieMenu's terrain command has no Global field. Expose only actions it
    // can actually send, preserving source TTAIndex and dynamic Param0.
    batch.offers.retain(|offer| {
        offer.interaction.scope == ix::InteractionScope::Local
            && u16::try_from(offer.interaction.tta_index).is_ok()
    });
    let spent = work
        .instructions
        .checked_add(checked)
        .ok_or(RuntimeError::InstructionLimit)?;
    if spent > state.limits.max_tick_instructions {
        return Err(RuntimeError::InstructionLimit);
    }
    Ok((
        TerrainOffers {
            world_revision: state.completed_tick,
            actor: actor_version,
            queue_revision,
            position,
            offers: batch.offers,
        },
        spent,
    ))
}

pub(super) fn execute(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    intent: SourceGotoIntent,
) -> Result<(), RuntimeError> {
    let actor = entity_ref(intent.actor.key).map_err(invalid)?;
    let current = actor_stamp(state, intent.principal, actor)?;
    if current != intent.actor
        || intent.world_revision != state.completed_tick
        || intent.command_sequence == 0
    {
        return Err(invalid("source GoTo actor/world stamp is stale"));
    }
    state.interaction_queues[&actor.object_id]
        .validate_command(
            intent.actor.key,
            intent.queue_revision,
            intent.command_sequence,
        )
        .map_err(invalid)?;
    let position = intent.position()?;
    // Source authority never runs the UI's ghost menu or SolidToAvatars here.
    // Keep exact Param0 and defer CheckAction until the real queued action runs.
    validate_selection(content, intent.interaction)?;
    let interaction = ix::InteractionKey {
        tta_index: u32::from(intent.interaction),
        scope: ix::InteractionScope::Local,
    };
    // Source actions must not inherit the partial-content legacy fallback for
    // an unsupported Main, queued behavior, primitive or routing resource.
    work.strict_source = true;
    let target = create_source_object(state, content, work, position, false)?;
    let seen = ix::ViewStamp {
        world_revision: state.completed_tick,
        actor: ix::EntityVersion {
            key: entity_key(actor),
            revision: live_entity(state, actor)?.revision,
        },
        target: ix::EntityVersion {
            key: entity_key(target),
            revision: live_entity(state, target)?.revision,
        },
    };
    interaction_adapter::enqueue_source_goto(
        state,
        content,
        work,
        ix::InteractionIntent {
            principal: intent.principal,
            seen,
            queue_revision: intent.queue_revision,
            command_sequence: intent.command_sequence,
            interaction,
            param0: intent.param0,
        },
    )
}

/// CreateObjectInstance initializes at OOW, queues Main, then changes position,
/// runs placement entry 9 and sets birth fields. A ghost uses the same source
/// Init/dynamic checks in its detached sandbox, without Main or placement.
fn create_source_object(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    position: LotPosition,
    ghost: bool,
) -> Result<EntityRef, RuntimeError> {
    require_content(content)?;
    if ghost {
        return create_ghost(state, content, work, position);
    }
    let entity = spawn_entity_impl(
        state,
        content,
        work,
        SpawnSpec {
            guid: SOURCE_GOTO_GUID,
            position: LotPosition::OUT_OF_WORLD,
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        },
        ObjectId::NULL,
        ObjectId::NULL,
        0,
        None,
        true,
    )?;
    // VMTSOObjectState defaults only for this newly allocated transient object.
    // Existing imported ownership/donation metadata remains untouched.
    let mut object = state
        .world
        .object(entity)
        .ok_or_else(|| invalid("source GoTo world object missing"))?
        .clone();
    object.owner = Some(PersistentId(0));
    object.rules.ghost = false;
    state.world.replace_object(object).map_err(invalid)?;
    if let crate::owned_admission::OwnedAdmissionState::Initialized { source, .. } =
        &mut state.owned_admission
    {
        source.object_flags.insert(entity, 0);
        // Persistent ID zero is never a registered persistent group.
    }
    source_callback(state, content, work, entity, 0)?;
    source_callback(state, content, work, entity, 8)?;
    {
        let main = content
            .object(SOURCE_GOTO_GUID)
            .and_then(|d| d.entry_points.get(&1))
            .copied()
            .ok_or_else(|| invalid("source GoTo Main missing"))?;
        let context = entry_context(state, entity, true)?;
        let args = entry_args(content, main, ObjectId::NULL)?;
        if !state
            .threads
            .get_mut(&entity.object_id)
            .ok_or_else(|| invalid("source GoTo thread missing"))?
            .push_entry(content.routines(), main, context, args)?
        {
            return Err(invalid("source GoTo Main could not start"));
        }
    }
    live_entity_mut(state, entity)?.lifecycle = LifecyclePhase::Running;
    let mut budget = ScriptBudget {
        remaining: state
            .limits
            .max_tick_instructions
            .saturating_sub(work.instructions),
        depth: 0,
        used: 0,
    };
    let placed = crate::runtime_routes::move_checked(
        state,
        content,
        entity,
        position,
        Facing::NORTH,
        &mut budget,
    )?;
    work.instructions = work
        .instructions
        .checked_add(budget.used)
        .ok_or(RuntimeError::InstructionLimit)?;
    if !placed {
        return Err(invalid("source GoTo placement failed"));
    }
    source_callback(state, content, work, entity, 9)?;
    let birth = [
        (21, state.clock.minutes),
        (24, state.clock.hours),
        (45, state.clock.year),
        (46, state.clock.month),
        (47, state.clock.day_of_month),
    ];
    for (index, value) in birth {
        live_entity_mut(state, entity)?.object_data[index] = value as i16;
    }
    Ok(entity)
}

/// VM.Sandbox replaces entity registries and object queries and starts IDs at
/// one for EACH ghost callback; SandboxRestore restores the original RNG.
/// Only the singleton destination ghost is supported. Dynamic creation or
/// containment cannot be silently collapsed into a fake single object.
fn create_ghost(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    position: LotPosition,
) -> Result<EntityRef, RuntimeError> {
    let mut sandbox = state.clone();
    sandbox.ids = IdAllocator::new();
    sandbox.entities.clear();
    sandbox.threads.clear();
    sandbox.interaction_queues.clear();
    sandbox.interaction_access.clear();
    sandbox.continuations.clear();
    sandbox.scheduler = Scheduler::new(state.completed_tick);
    sandbox.world = WorldState::new(state.world.lot.clone());
    sandbox.owned_admission = crate::owned_admission::OwnedAdmissionState::Uninitialized;
    let temporary = spawn_entity_impl(
        &mut sandbox,
        content,
        work,
        SpawnSpec {
            guid: SOURCE_GOTO_GUID,
            position: LotPosition::OUT_OF_WORLD,
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        },
        ObjectId::NULL,
        ObjectId::NULL,
        0,
        None,
        true,
    )?;
    if temporary.object_id != ObjectId(1) {
        return Err(invalid("source ghost sandbox ID"));
    }
    let mut projection = sandbox
        .world
        .object(temporary)
        .ok_or_else(|| invalid("ghost projection missing"))?
        .clone();
    projection.rules.ghost = true;
    projection.owner = Some(PersistentId(0));
    sandbox.world.replace_object(projection).map_err(invalid)?;
    sandbox.scheduler.cancel(temporary);
    for stage in [0, 8] {
        sandbox.rng = state.rng.clone();
        source_callback(&mut sandbox, content, work, temporary, stage)?;
        sandbox.rng = state.rng.clone();
        if sandbox.entities.len() != 1
            || !sandbox.continuations.is_empty()
            || sandbox
                .entities
                .get(&temporary.object_id)
                .map_or(true, |e| {
                    e.info.dead
                        || e.info.group != vec![temporary.object_id]
                        || !lot_position(e.info.position).is_ok_and(|p| p.is_out_of_world())
                        || e.container.is_some()
                        || e.slots.iter().any(Option::is_some)
                })
        {
            return Err(invalid(
                "source ghost callback needs unsupported object topology",
            ));
        }
        sandbox.scheduler = Scheduler::new(state.completed_tick);
    }
    // The source restores registries, not global registers or architecture.
    // These writes remain detached from the real simulation throughout a query.
    state.globals = sandbox.globals;
    state.world.lot = sandbox.world.lot;
    let mut initialized = sandbox
        .entities
        .remove(&temporary.object_id)
        .ok_or_else(|| invalid("source ghost was removed"))?;
    let proxy = spawn_entity_impl(
        state,
        content,
        work,
        SpawnSpec {
            guid: SOURCE_GOTO_GUID,
            position: LotPosition::OUT_OF_WORLD,
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        },
        ObjectId::NULL,
        ObjectId::NULL,
        0,
        None,
        true,
    )?;
    initialized.info.reference = proxy;
    initialized.info.base_object = proxy.object_id;
    initialized.info.group = vec![proxy.object_id];
    initialized.lifecycle = LifecyclePhase::Running;
    initialized.object_data[29] = -1;
    state.entities.insert(proxy.object_id, initialized);
    let mut object = state
        .world
        .object(proxy)
        .ok_or_else(|| invalid("ghost mirror missing"))?
        .clone();
    object.rules.ghost = true;
    object.owner = Some(PersistentId(0));
    state.world.replace_object(object).map_err(invalid)?;
    if let crate::owned_admission::OwnedAdmissionState::Initialized { source, .. } =
        &mut state.owned_admission
    {
        source.object_flags.insert(proxy, 0);
    }
    crate::runtime_memory::sync_projection(state, proxy)?;
    let mut budget = ScriptBudget {
        remaining: state
            .limits
            .max_tick_instructions
            .saturating_sub(work.instructions),
        depth: 0,
        used: 0,
    };
    if !crate::runtime_routes::move_checked(
        state,
        content,
        proxy,
        position,
        Facing::NORTH,
        &mut budget,
    )? {
        return Err(invalid("source ghost placement failed"));
    }
    work.instructions = work
        .instructions
        .checked_add(budget.used)
        .ok_or(RuntimeError::InstructionLimit)?;
    state.scheduler.cancel(proxy);
    let birth = [
        (21, state.clock.minutes),
        (24, state.clock.hours),
        (45, state.clock.year),
        (46, state.clock.month),
        (47, state.clock.day_of_month),
    ];
    for (index, value) in birth {
        live_entity_mut(state, proxy)?.object_data[index] = value as i16;
    }
    Ok(proxy)
}

fn source_callback(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    entity: EntityRef,
    stage: u8,
) -> Result<(), RuntimeError> {
    let key = content
        .object(SOURCE_GOTO_GUID)
        .and_then(|d| d.entry_points.get(&stage))
        .copied();
    let Some(key) = key else {
        return Ok(());
    };
    let context = entry_context(state, entity, false)?;
    let args = entry_args(content, key, ObjectId::NULL)?;
    let mut thread = VmThread::new(entity, state.mode);
    thread.is_check = true;
    if !thread.push_entry(content.routines(), key, context, args)? {
        return Err(invalid("source GoTo callback routine unavailable"));
    }
    let effects = state.effects.clone();
    let limit = state
        .limits
        .max_tick_instructions
        .saturating_sub(work.instructions)
        .min(state.limits.instruction_budget_per_entity);
    let old_strict = work.strict_source;
    work.strict_source = true;
    let result = run_synchronous_thread(
        state,
        content,
        work,
        entity,
        0,
        &mut thread,
        limit,
        &mut BTreeMap::new(),
        true,
    );
    work.strict_source = old_strict;
    let report = result?;
    work.instructions = work
        .instructions
        .checked_add(report.instructions)
        .ok_or(RuntimeError::InstructionLimit)?;
    if work.instructions > state.limits.max_tick_instructions {
        return Err(RuntimeError::InstructionLimit);
    }
    match report.stop {
        VmStop::Completed(_) => {}
        VmStop::Faulted(fault) => return Err(fault.into()),
        _ => return Err(invalid("source GoTo callback did not finish synchronously")),
    }
    if state.effects != effects
        || thread.diagnostics.iter().any(|d| {
            matches!(
                d,
                VmDiagnostic::MissingRoutine { .. } | VmDiagnostic::MissingPrimitive { .. }
            )
        })
    {
        return Err(invalid(
            "source GoTo callback requires unavailable behavior",
        ));
    }
    live_entity(state, entity)?;
    Ok(())
}
