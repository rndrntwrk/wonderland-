//! Accepted build commands bridge source geometry to real simulation entities.
//! Preview is detached. Durable accounting and receipt authentication remain the
//! server adapter's responsibility; this module never manufactures a receipt.
use super::*;
use crate::world::build::{
    BuildAuthority, BuildBegin, BuildCommitStatus, BuildEdit, BuildError, BuildIntent,
    BuildPreview, ServerBuildConfirmation,
};

impl SimRuntime {
    pub fn preview_build(
        &self,
        intent: &BuildIntent,
        authority: &BuildAuthority,
    ) -> Result<BuildPreview, RuntimeError> {
        let mut budget = ScriptBudget {
            remaining: self.state.limits.max_tick_instructions,
            depth: 0,
            used: 0,
        };
        preview(&self.state, &self.content, intent, authority, &mut budget)
    }
}

/// Replace all purchase geometry/ownership fields with source content. The
/// caller supplies only a catalog identity, reserved entity, position and facing.
fn canonical_intent(
    state: &SimState,
    content: &ContentSet,
    intent: &BuildIntent,
    authority: &BuildAuthority,
) -> Result<BuildIntent, RuntimeError> {
    live_entity(state, intent.actor)?;
    if intent.edits.len() > crate::world::build::MAX_BUILD_EDITS {
        return Err(invalid("build edit limit"));
    }
    let mut allocator = state.ids.clone();
    let mut canonical = intent.clone();
    let mut count = state.entities.len();
    for edit in &mut canonical.edits {
        match edit {
            BuildEdit::PlaceObject { catalog_id, object } => {
                let guid = content
                    .catalog_guid(*catalog_id)
                    .ok_or_else(|| invalid(BuildError::UnknownCatalogItem))?;
                let definition = content
                    .object(guid)
                    .ok_or_else(|| invalid("catalog source object missing"))?;
                if definition.placement_rules.is_avatar
                    || definition.master_guid.is_some()
                    || definition.level_offset != 0
                {
                    return Err(invalid(
                        "purchase requires normalized single-tile object content",
                    ));
                }
                let expected = allocator.allocate().map_err(invalid)?;
                if expected != object.entity {
                    return Err(invalid("purchase entity reservation is stale"));
                }
                count += 1;
                if count > state.limits.max_entities as usize {
                    return Err(invalid("entity capacity"));
                }
                vm_position(object.position)?;
                let mut source = WorldObject::new(expected, object.position);
                source.facing = object.facing;
                source.owner = Some(authority.owner);
                source.footprint = definition.footprint.clone();
                source.rules = definition.placement_rules.clone();
                source.entrypoints = definition.entry_points.keys().copied().collect();
                *object = source;
            }
            BuildEdit::MoveObject { entity, .. } | BuildEdit::DeleteObject { entity, .. } => {
                let item = live_entity(state, *entity)?;
                if item.info.is_avatar
                    || item.info.multi_tile
                    || item.container.is_some()
                    || item.slots.iter().any(Option::is_some)
                {
                    return Err(invalid(
                        "build move/delete requires an independent non-avatar object",
                    ));
                }
                if let BuildEdit::MoveObject { position, .. } = edit {
                    vm_position(*position)?;
                }
            }
            _ => {}
        }
    }
    Ok(canonical)
}

fn preview(
    state: &SimState,
    content: &ContentSet,
    intent: &BuildIntent,
    authority: &BuildAuthority,
    budget: &mut ScriptBudget,
) -> Result<BuildPreview, RuntimeError> {
    let canonical = canonical_intent(state, content, intent, authority)?;
    let mut fault = None;
    let mut scripts =
        |call: &world::IntersectionCall| match check_intersection(state, content, call, budget) {
            Ok(allowed) => allowed,
            Err(error) => {
                fault = Some(error);
                false
            }
        };
    let result = state
        .world
        .preview_build(canonical, authority, &mut scripts);
    if let Some(fault) = fault {
        return Err(fault.into());
    }
    result.map_err(invalid)
}

pub(super) fn begin(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    supplied: &BuildPreview,
    authority: &BuildAuthority,
) -> Result<(), RuntimeError> {
    // Pending/repeated commands are checked against the retained source preview
    // by BuildState. A fresh command must be recomputed, including check trees.
    if state
        .world
        .builds
        .outcome(supplied.intent.operation)
        .is_none()
        && state.world.builds.pending_operation().is_none()
    {
        let mut budget = ScriptBudget {
            remaining: state
                .limits
                .max_tick_instructions
                .checked_sub(work.instructions)
                .ok_or(RuntimeError::InstructionLimit)?,
            depth: 0,
            used: 0,
        };
        let verified = preview(state, content, &supplied.intent, authority, &mut budget)?;
        work.instructions = work
            .instructions
            .checked_add(budget.used)
            .ok_or(RuntimeError::InstructionLimit)?;
        if &verified != supplied {
            return Err(invalid(
                "build preview differs from current source validation",
            ));
        }
    }
    match state
        .world
        .begin_build_commit(supplied.clone(), authority)
        .map_err(invalid)?
    {
        BuildBegin::Effect(request) => work.events.push(RuntimeEvent::BuildRequested(request)),
        BuildBegin::AlreadyCompleted(result) => {
            work.events.push(RuntimeEvent::BuildCompleted(result))
        }
    }
    Ok(())
}

pub(super) fn complete(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    confirmation: &ServerBuildConfirmation,
) -> Result<(), RuntimeError> {
    let duplicate = state.world.builds.outcome(confirmation.operation).is_some();
    let intent = state
        .world
        .builds
        .pending_preview()
        .map(|p| p.intent.clone());
    let before = state.clone();
    let before_work = work.clone();
    let mut result = state
        .world
        .complete_build_commit(confirmation.clone())
        .map_err(invalid)?;
    if !duplicate && matches!(result.status, BuildCommitStatus::Committed { .. }) {
        let intent = intent.ok_or_else(|| invalid("committed build has no source intent"))?;
        if let Err(error) = install_entities(state, content, work, &intent, confirmation) {
            // Durable success cannot disappear because initialization failed.
            // Restore simulation geometry, retain the exact receipt for E's
            // reconciliation lane, and expose the actual source runtime error.
            *state = before;
            *work = before_work;
            result = state
                .world
                .builds
                .reconcile_runtime_failure(confirmation.clone(), BuildError::InvalidState);
            work.events.push(RuntimeEvent::BuildRuntimeFault {
                operation: confirmation.operation,
                error: error.to_string(),
            });
        }
    }
    work.events.push(RuntimeEvent::BuildCompleted(result));
    Ok(())
}

fn install_entities(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    intent: &BuildIntent,
    confirmation: &ServerBuildConfirmation,
) -> Result<(), RuntimeError> {
    for edit in &intent.edits {
        match edit {
            BuildEdit::PlaceObject { catalog_id, object } => {
                let guid = content
                    .catalog_guid(*catalog_id)
                    .ok_or_else(|| invalid("committed catalog binding missing"))?;
                let persistent_id = *confirmation
                    .created_objects
                    .get(&object.entity)
                    .ok_or_else(|| invalid("purchase receipt missing object"))?;
                let spec = SpawnSpec {
                    guid,
                    position: object.position,
                    facing: object.facing,
                    persistent_id,
                    avatar: false,
                };
                let old_strict = work.strict_source;
                work.strict_source = true;
                let created = spawn_entity_impl(
                    state,
                    content,
                    work,
                    spec,
                    ObjectId::NULL,
                    ObjectId::NULL,
                    0,
                    Some(object.entity),
                );
                work.strict_source = old_strict;
                created?;
                let thread = state
                    .threads
                    .get(&object.entity.object_id)
                    .ok_or_else(|| invalid("purchase initialization removed thread"))?;
                if matches!(thread.stop, VmStop::Faulted(_))
                    || thread.diagnostics.iter().any(|d| {
                        matches!(
                            d,
                            VmDiagnostic::MissingPrimitive { .. }
                                | VmDiagnostic::MissingRoutine { .. }
                        )
                    })
                {
                    return Err(invalid(
                        "purchase initialization encountered unsupported source behavior",
                    ));
                }
            }
            BuildEdit::MoveObject {
                entity,
                position,
                facing,
                ..
            } => {
                move_entity(state, *entity, *position, *facing)?;
            }
            BuildEdit::DeleteObject { entity, .. } => remove_entity(state, work, *entity)?,
            _ => {}
        }
    }
    // Source room membership is also an entity memory projection. Architecture
    // edits must update it before the next VM primitive observes the new lot.
    let references: Vec<_> = state.entities.values().map(|e| e.info.reference).collect();
    for entity in references {
        crate::runtime_memory::sync_projection(state, entity)?;
    }
    Ok(())
}
