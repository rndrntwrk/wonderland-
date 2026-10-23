//! Source UserLeaveLot queue semantics and durable, generation-bound departure.
//! Requests originate in the trusted owned-drain adapter, never a player packet.
use super::*;
use crate::avatars::{
    lifecycle::{LEAVE_LOT_INTERACTION, LEAVE_LOT_ROUTINE, LeaveContext, LifecycleRequest},
    state::AvatarPersistState,
};
use crate::interactions as ix;
use crate::owned_admission::OwnedControl;

pub const MAX_OWNED_DEPARTURES: usize = 4096;
const MAX_CAPTURE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedLeaveRequest {
    pub authority_id: [u8; 32],
    pub control: OwnedControl,
    pub departure_operation: String,
}
impl OwnedLeaveRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.control.validate()?;
        if self.authority_id == [0; 32] || !self.control.actor.is_valid() {
            return Err("invalid owned departure identity");
        }
        let authority: String = self
            .authority_id
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if self.departure_operation
            != format!(
                "{}_{}_{}",
                authority, self.control.avatar_id.0, self.control.control_generation
            )
        {
            return Err("departure operation is not generation-derived");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepartureRemovalReason {
    SourceLeave,
    ForcedCleanup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepartureBlocker {
    MissingSourceAction,
    UnsupportedBlockingWait,
    UnreconciledExternalEffect,
    UnsupportedEodSession,
    SourceExecutionFault,
    UnobservedSourceLeave,
    /// Supported source session still needs an exact close/event disposition.
    PendingEodTeardown,
}
impl DepartureBlocker {
    pub fn code(self) -> &'static str {
        match self {
            Self::MissingSourceAction => "missing_source_action",
            Self::UnsupportedBlockingWait => "unsupported_blocking_wait",
            Self::UnreconciledExternalEffect => "unreconciled_external_effect",
            Self::UnsupportedEodSession => "unsupported_eod_session",
            Self::SourceExecutionFault => "source_execution_fault",
            Self::UnobservedSourceLeave => "unobserved_source_leave",
            Self::PendingEodTeardown => "pending_eod_teardown",
        }
    }
}

/// The retained native observation is private. The host exports only the source
/// database whitelist, never this complete portable avatar representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLeaveCapture {
    pub captured_tick: u64,
    pub portable: AvatarPersistState,
    pub changed_relationships: BTreeMap<PersistentId, Vec<i16>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedDeparture {
    pub request: OwnedLeaveRequest,
    pub requested_tick: u64,
    pub capture: Option<SourceLeaveCapture>,
    pub detached_tick: Option<u64>,
    pub removal_reason: Option<DepartureRemovalReason>,
    pub blocker: Option<DepartureBlocker>,
    pub eod_disconnect_requested: bool,
    pub blocked_primitive: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedDepartureState {
    pub records: BTreeMap<EntityRef, OwnedDeparture>,
    /// Fresh native genesis establishes complete EOD coverage. Unsupported
    /// InvokePlugin clears it; supported typed Timer sessions are tracked in
    /// state.eod and must separately finish exact teardown. Legacy snapshots
    /// migrate to unknown/false and cannot regain coverage through an ack.
    pub eod_known_clear: bool,
    /// One durable disconnect consequence per exact owned actor generation.
    pub eod_disconnect_requests: BTreeSet<(EntityRef, u64)>,
}

/// Source GetAction checks local definition and both nonzero routine bindings.
/// Unknown behavior is unavailable, never replaced with a fabricated leave tree.
fn leave_definition(
    state: &SimState,
    content: &ContentSet,
    actor: EntityRef,
) -> Result<Option<ix::InteractionDefinition>, RuntimeError> {
    let item = live_entity(state, actor)?;
    if item.avatar.is_none() || state.mode != VmMode::Tso {
        return Err(invalid("source leave requires a TSO avatar"));
    }
    let Some(definition) = content.interaction_table(item.info.guid).and_then(|table| {
        table.definitions.iter().find(|definition| {
            definition.key.scope == ix::InteractionScope::Local
                && definition.key.tta_index == u32::from(LEAVE_LOT_INTERACTION)
        })
    }) else {
        return Ok(None);
    };
    if definition.action.routine_id == 0
        || content
            .routines()
            .resolve(
                definition.action.code_owner_guid,
                definition.action.routine_id,
            )
            .is_none()
        || definition.check.is_some_and(|check| {
            check.routine_id != 0
                && content
                    .routines()
                    .resolve(check.code_owner_guid, check.routine_id)
                    .is_none()
        })
    {
        return Ok(None);
    }
    Ok(Some(definition.clone()))
}

pub(super) fn leave_context(
    state: &SimState,
    content: &ContentSet,
    actor: EntityRef,
) -> Result<LeaveContext, RuntimeError> {
    let queue = state
        .interaction_queues
        .get(&actor.object_id)
        .ok_or_else(|| invalid("source leave actor queue missing"))?;
    // ActiveQueueBlock is the last active index; native active_len is one past it.
    // The source also recognizes the first pending leave at index active_len.
    let action_already_queued = queue.entries().iter().enumerate().any(|(index, entry)| {
        index <= queue.active_len()
            && entry.invocation.definition.action.routine_id == LEAVE_LOT_ROUTINE
    });
    Ok(LeaveContext {
        action_available: leave_definition(state, content, actor)?.is_some(),
        action_already_queued,
    })
}

/// Executes only queue-related reducer requests. Source queue IDs are snapshotted
/// before cancellation because cancelling parent-idle actions may remove children.
pub(super) fn apply_queue_requests(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    actor: EntityRef,
    requests: &[LifecycleRequest],
) -> Result<(), RuntimeError> {
    for request in requests {
        match request {
            LifecycleRequest::CancelInteractions => {
                let actions: Vec<_> = state
                    .interaction_queues
                    .get(&actor.object_id)
                    .ok_or_else(|| invalid("source leave actor queue missing"))?
                    .entries()
                    .iter()
                    .map(|entry| entry.id)
                    .collect();
                for action in actions {
                    interaction_adapter::cancel_internal_action(state, work, actor, action)?;
                }
            }
            LifecycleRequest::EnqueueLeave { .. } => {
                let mut definition = leave_definition(state, content, actor)?
                    .ok_or_else(|| invalid("source leave action became unavailable"))?;
                definition.flags.0 |= ix::ActionFlags::SKIP_PERMISSIONS;
                let actor_key = interaction_adapter::entity_key(actor);
                let action = ix::ActionInvocation::user(actor_key, actor_key, definition, 0);
                // GetAction's default is UserDriven=50, not maximum priority.
                interaction_adapter::enqueue_internal_action(state, work, actor, action)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn user_leave(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    actor: EntityRef,
) -> Result<(), RuntimeError> {
    let context = leave_context(state, content, actor)?;
    let requests = avatar_mut(state, actor)?.lifecycle.request_leave(context);
    apply_queue_requests(state, content, work, actor, &requests)
}

/// The current executing thread may have been detached from state. Source mode25
/// must inspect this complete view, rather than silently omitting active frames.
pub(super) fn unwind_other_avatars(
    state: &mut SimState,
    work: &mut TickWork,
    actor: EntityRef,
    executing_threads: &BTreeMap<ObjectId, VmThread>,
) -> Result<bool, RuntimeError> {
    live_entity(state, actor)?;
    let others: Vec<_> = state
        .entities
        .values()
        .filter(|entity| {
            entity.info.is_avatar && !entity.info.dead && entity.info.reference != actor
        })
        .map(|entity| entity.info.reference)
        .collect();
    let target = interaction_adapter::entity_key(actor);
    let mut can_leave = true;
    for other in others {
        let queue = state
            .interaction_queues
            .get(&other.object_id)
            .ok_or_else(|| invalid("source mode25 peer queue missing"))?;
        let cancel: Vec<_> = queue
            .entries()
            .iter()
            .filter(|entry| {
                entry.invocation.target == target
                    || entry.invocation.effective_icon_owner() == target
            })
            .map(|entry| entry.id)
            .collect();
        for action in cancel {
            interaction_adapter::cancel_internal_action(state, work, other, action)?;
        }
        let active = state.interaction_queues[&other.object_id].active().cloned();
        if let Some(active) = active {
            if active.invocation.target == target
                || active.invocation.effective_icon_owner() == target
            {
                can_leave = false;
                continue;
            }
            let thread = executing_threads
                .get(&other.object_id)
                .or_else(|| state.threads.get(&other.object_id))
                .ok_or_else(|| invalid("source mode25 peer thread unavailable"))?;
            if thread.frames.iter().any(|frame| {
                frame.context.callee == actor || frame.context.stack_object_ref == Some(actor)
            }) {
                interaction_adapter::cancel_internal_action(state, work, other, active.id)?;
                can_leave = false;
            }
        }
    }
    Ok(can_leave)
}

pub(super) fn capture_values(
    state: &SimState,
    actor: EntityRef,
    tick: u64,
) -> Result<SourceLeaveCapture, RuntimeError> {
    if tick == 0 {
        return Err(invalid("source save capture tick is zero"));
    }
    let avatar = live_entity(state, actor)?
        .avatar
        .as_ref()
        .ok_or_else(|| invalid("source save requires avatar"))?;
    let portable = avatar.persistent_state().map_err(invalid)?;
    let mut changed_relationships = BTreeMap::new();
    for (owner, target) in &state.relationships.changed_persistent {
        if *owner != actor {
            continue;
        }
        let key = RelationshipKey {
            owner: RelationshipOwner::Entity(actor),
            target: RelationshipTarget::Persistent(*target),
        };
        // C# ChangedRels skips a target with no MeToPersist matrix.
        let Some(values) = state.relationships.matrices.get(&key) else {
            continue;
        };
        if *target == 0 || values.len() > 256 {
            return Err(invalid("source save relationship bounds"));
        }
        if portable.relationships.get(&PersistentId(*target)) != Some(values) {
            return Err(invalid("source save relationship projections differ"));
        }
        changed_relationships.insert(PersistentId(*target), values.clone());
    }
    if changed_relationships.len() > 256 {
        return Err(invalid("source save changed-target capacity"));
    }
    let result = SourceLeaveCapture {
        captured_tick: tick,
        portable,
        changed_relationships,
    };
    canonical_bytes(&result, MAX_CAPTURE_BYTES).map_err(invalid)?;
    Ok(result)
}

impl OwnedDepartureState {
    pub fn fresh() -> Self {
        Self {
            records: BTreeMap::new(),
            eod_known_clear: true,
            eod_disconnect_requests: BTreeSet::new(),
        }
    }
    pub fn validate(&self, state: &SimState) -> Result<(), &'static str> {
        if self.records.len() > MAX_OWNED_DEPARTURES
            || self.eod_disconnect_requests.len() > MAX_OWNED_DEPARTURES
        {
            return Err("owned departure record capacity");
        }
        if self
            .eod_disconnect_requests
            .iter()
            .any(|(actor, generation)| !actor.is_valid() || *generation == 0)
        {
            return Err("invalid EOD disconnect identity");
        }
        for (actor, record) in &self.records {
            record.request.validate()?;
            if *actor != record.request.control.actor
                || record.requested_tick == 0
                || record.requested_tick > state.completed_tick
            {
                return Err("owned departure identity or tick mismatch");
            }
            if (record.blocker == Some(DepartureBlocker::SourceExecutionFault))
                != record.blocked_primitive.is_some()
                || record
                    .blocked_primitive
                    .is_some_and(|opcode| !matches!(opcode, 42 | 63))
            {
                return Err("owned departure fault classification mismatch");
            }
            if record.eod_disconnect_requested
                != self
                    .eod_disconnect_requests
                    .contains(&(*actor, record.request.control.control_generation))
            {
                return Err("owned departure EOD request history mismatch");
            }
            if let Some(capture) = &record.capture {
                if capture.captured_tick < record.requested_tick
                    || capture.captured_tick > state.completed_tick
                    || capture.portable.persistent_id != record.request.control.avatar_id
                    || capture.portable.motive_data[11] != 0
                    || capture.changed_relationships.len() > 256
                {
                    return Err("owned departure capture identity mismatch");
                }
                let mut reconstructed = AvatarState::new(
                    *actor,
                    record.request.control.avatar_id,
                    AvatarPlatform::Tso,
                );
                reconstructed
                    .apply_persistent_state(&capture.portable)
                    .map_err(|_| "invalid owned capture avatar")?;
                if capture
                    .changed_relationships
                    .iter()
                    .any(|(target, values)| {
                        target.0 == 0
                            || values.len() > 256
                            || capture.portable.relationships.get(target) != Some(values)
                    })
                {
                    return Err("invalid owned capture relationships");
                }
                canonical_bytes(capture, MAX_CAPTURE_BYTES)
                    .map_err(|_| "owned capture byte bounds")?;
            }
            match record.detached_tick {
                Some(tick) => {
                    if tick < record.requested_tick
                        || tick > state.completed_tick
                        || record
                            .capture
                            .as_ref()
                            .map_or(true, |capture| capture.captured_tick > tick)
                        || record.removal_reason.is_none()
                        || record.blocker.is_some()
                        || state.ids.is_live(*actor)
                        || state.owned_admission.actors().is_some_and(|actors| {
                            actors.values().any(|control| control.actor == *actor)
                        })
                    {
                        return Err("owned departure detach evidence mismatch");
                    }
                }
                None => {
                    if record.removal_reason.is_some()
                        || !state.ids.is_live(*actor)
                        || state
                            .owned_admission
                            .actors()
                            .and_then(|actors| actors.get(&record.request.control.principal))
                            != Some(&record.request.control)
                        || state.interaction_access.contains_key(actor)
                    {
                        return Err("owned departure live fence mismatch");
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) fn request_owned_leave(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    request: OwnedLeaveRequest,
    tick: u64,
) -> Result<(), RuntimeError> {
    request.validate().map_err(invalid)?;
    let actor = request.control.actor;
    if let Some(previous) = state.owned_departures.records.get(&actor) {
        return if previous.request == request {
            Ok(())
        } else {
            Err(invalid("conflicting generation-bound departure"))
        };
    }
    if state.owned_departures.records.len() >= MAX_OWNED_DEPARTURES {
        return Err(invalid("owned departure record capacity"));
    }
    if state
        .owned_admission
        .actors()
        .and_then(|actors| actors.get(&request.control.principal))
        != Some(&request.control)
    {
        return Err(invalid("owned departure control is not installed"));
    }
    let avatar = live_entity(state, actor)?
        .avatar
        .as_ref()
        .ok_or_else(|| invalid("owned leave avatar missing"))?;
    if avatar.lifecycle.kill_timeout != -1 {
        return Err(invalid(
            "unobserved prior source departure requires journal recovery",
        ));
    }
    let eod_disconnect_requested = state
        .owned_departures
        .eod_disconnect_requests
        .contains(&(actor, request.control.control_generation));
    state.owned_departures.records.insert(
        actor,
        OwnedDeparture {
            request,
            requested_tick: tick,
            capture: None,
            detached_tick: None,
            removal_reason: None,
            blocker: None,
            eod_disconnect_requested,
            blocked_primitive: None,
        },
    );
    // This exact generation loses player authority even when source support is
    // unavailable. Its installed ownership remains until proven source removal.
    state.interaction_access.remove(&actor);
    user_leave(state, content, work, actor)?;
    refresh_blocker(state, content, actor)?;
    if avatar_mut(state, actor)?.lifecycle.kill_timeout < 0 {
        state
            .owned_departures
            .records
            .get_mut(&actor)
            .expect("inserted departure")
            .blocker = Some(DepartureBlocker::UnobservedSourceLeave);
    }
    state.scheduler.interrupt(actor).map_err(invalid)?;
    Ok(())
}

pub(super) fn request_eod_disconnect(
    state: &mut SimState,
    work: &mut TickWork,
    actor: EntityRef,
) -> Result<(), RuntimeError> {
    eod_adapter::disconnect_actor(state, work, actor)?;
    let generation = state
        .owned_admission
        .actors()
        .and_then(|actors| actors.values().find(|control| control.actor == actor))
        .map(|control| control.control_generation);
    if let Some(control_generation) = generation {
        if let Some(record) = state.owned_departures.records.get_mut(&actor) {
            record.eod_disconnect_requested = true;
        }
        if state
            .owned_departures
            .eod_disconnect_requests
            .insert((actor, control_generation))
        {
            work.events.push(RuntimeEvent::OwnedEodDisconnectRequested {
                actor,
                control_generation,
            });
        }
    }
    Ok(())
}

/// Absence of an outbox request is not an absence-of-session proof. Unsupported
/// asynchronous waits retain their exact continuation and block custody release.
/// Generic effects do not expose participant associations, so another target
/// cannot be assumed unrelated to the departing avatar.
pub(super) fn refresh_blocker(
    state: &mut SimState,
    content: &ContentSet,
    actor: EntityRef,
) -> Result<bool, RuntimeError> {
    let Some(record) = state.owned_departures.records.get(&actor) else {
        return Ok(false);
    };
    if record.detached_tick.is_some() {
        return Ok(false);
    }
    if matches!(
        record.blocker,
        Some(DepartureBlocker::SourceExecutionFault | DepartureBlocker::UnobservedSourceLeave)
    ) {
        return Ok(true);
    }
    let blocker = if !leave_context(state, content, actor)?.action_available {
        Some(DepartureBlocker::MissingSourceAction)
    } else if !state.owned_departures.eod_known_clear {
        Some(DepartureBlocker::UnsupportedEodSession)
    } else if state.eod.actor_teardown_pending(actor) {
        Some(DepartureBlocker::PendingEodTeardown)
    } else if state.effects.pending_requests().next().is_some() {
        Some(DepartureBlocker::UnreconciledExternalEffect)
    } else if state
        .threads
        .get(&actor.object_id)
        .is_some_and(|thread| matches!(thread.stop, VmStop::Waiting { .. }))
        && !state.continuations.values().any(|continuation| {
            continuation.entity == actor && matches!(continuation.kind, ContinuationKind::Route(_))
        })
    {
        Some(DepartureBlocker::UnsupportedBlockingWait)
    } else {
        None
    };
    state
        .owned_departures
        .records
        .get_mut(&actor)
        .expect("known record")
        .blocker = blocker;
    Ok(blocker.is_some())
}

pub(super) fn capture_first_tick(
    state: &mut SimState,
    work: &mut TickWork,
    actor: EntityRef,
) -> Result<(), RuntimeError> {
    let Some(record) = state.owned_departures.records.get(&actor) else {
        return Ok(());
    };
    if record.capture.is_some() || record.blocker.is_some() {
        return Ok(());
    }
    if live_entity(state, actor)?
        .avatar
        .as_ref()
        .map_or(true, |avatar| avatar.lifecycle.kill_timeout != 1)
    {
        return Ok(());
    }
    let tick = state.scheduler.current_tick();
    let capture = capture_values(state, actor, tick)?;
    let record = state
        .owned_departures
        .records
        .get_mut(&actor)
        .expect("known record");
    record.capture = Some(capture);
    work.events.push(RuntimeEvent::OwnedDepartureCaptured {
        actor,
        control_generation: record.request.control.control_generation,
        tick,
    });
    Ok(())
}

pub(super) fn prepare_removal(
    state: &mut SimState,
    work: &mut TickWork,
    actor: EntityRef,
) -> Result<(), RuntimeError> {
    let Some(record) = state.owned_departures.records.get(&actor) else {
        return Ok(());
    };
    if record.blocker.is_some()
        || record.removal_reason.is_none()
        || !state.owned_departures.eod_known_clear
        || state.eod.actor_teardown_pending(actor)
        || state.effects.pending_requests().next().is_some()
    {
        return Err(invalid("owned removal lacks source cleanup proof"));
    }
    capture_first_tick(state, work, actor)?;
    if state.owned_departures.records[&actor].capture.is_none() {
        return Err(invalid("owned removal lacks first-leave capture"));
    }
    Ok(())
}

pub(super) fn finish_removal(state: &mut SimState, work: &mut TickWork, actor: EntityRef) {
    let tick = state.scheduler.current_tick();
    if let Some(record) = state.owned_departures.records.get_mut(&actor) {
        record.detached_tick = Some(tick);
        work.events.push(RuntimeEvent::OwnedDepartureDetached {
            actor,
            control_generation: record.request.control.control_generation,
            tick,
            reason: record.removal_reason.expect("source removal checked"),
        });
    }
}

/// An explicit native custody-safety boundary, not source exception equivalence.
/// Only typed create/terrain adapter gaps (42/63) are containable. Reach mode47
/// can report a deliberate source throw, so it is not silently classified here.
/// Invariants, budgets,
/// malformed content, and generic host failures continue to reject the tick.
pub(super) fn dispatch_fenced_actor(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    actor: EntityRef,
) -> Result<(), RuntimeError> {
    if !state
        .owned_departures
        .records
        .get(&actor)
        .is_some_and(|record| {
            record.detached_tick.is_none()
                && record.blocker.is_none()
                && !state.interaction_access.contains_key(&actor)
                && state
                    .owned_admission
                    .actors()
                    .and_then(|actors| actors.get(&record.request.control.principal))
                    == Some(&record.request.control)
        })
    {
        return dispatch_entity(state, content, work, actor);
    }
    let checkpoint = state.clone();
    let prior_work = work.clone();
    match dispatch_entity(state, content, work, actor) {
        Err(RuntimeError::Vm(VmFault::UnsupportedPrimitive {
            opcode: opcode @ (42 | 63),
            ..
        })) => {
            *state = checkpoint;
            *work = prior_work;
            let record = state
                .owned_departures
                .records
                .get_mut(&actor)
                .expect("checkpoint departure");
            record.blocker = Some(DepartureBlocker::SourceExecutionFault);
            record.blocked_primitive = Some(opcode);
            state
                .scheduler
                .schedule_in(actor, 1, true)
                .map_err(invalid)?;
            // Full snapshot validation runs after this scheduler transaction is
            // finished. No intermediate running scheduler is serializable.
            Ok(())
        }
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct QueueHarness;
    impl ix::adapters::QueueRuntime for QueueHarness {
        fn target_is_alive(&self, _: ix::EntityKey) -> bool {
            true
        }
        fn check_action(&mut self, _: &ix::ActionInvocation) -> ix::Result<bool> {
            Ok(true)
        }
        fn start_action(&mut self, _: &ix::ActionInvocation) -> ix::Result<bool> {
            Ok(true)
        }
    }
    fn fixture() -> (SimState, ContentSet, EntityRef, EntityRef) {
        let mut routines = RoutineStore::new();
        routines.bind_semiglobal(100, 900).unwrap();
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::SemiGlobal(900),
                    id: 8373,
                },
                VmRoutine::new(8373, 0, 4, vec![VmInstruction::new(15, 254, 255, [0; 8])]).unwrap(),
            )
            .unwrap();
        let definition = ix::InteractionDefinition {
            key: ix::InteractionKey {
                tta_index: 173,
                scope: ix::InteractionScope::Local,
            },
            action: ix::RoutineBinding {
                code_owner_guid: 100,
                routine_id: 8373,
            },
            check: None,
            flags: ix::ActionFlags(
                ix::ActionFlags::ALLOW_VISITORS | ix::ActionFlags::ALLOW_CONSECUTIVE,
            ),
            permissions: ix::PermissionFlags::default(),
            label: None,
        };
        let content = ContentSet::new(
            routines,
            vec![ObjectDefinition::new(100, 0)],
            vec![],
            TuningSet {
                tso_motives: Some(crate::avatars::motives::TsoMotiveTuning {
                    flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                    category_weights: [[1000; 7]; 11],
                }),
                ..Default::default()
            },
        )
        .unwrap()
        .with_interaction_tables(vec![(
            100,
            crate::state::InteractionTable {
                local_table_present: true,
                definitions: vec![definition],
            },
        )])
        .unwrap();
        let mut runtime = SimRuntime::new(
            content.clone(),
            LotModel::new(8, 8, 1).unwrap(),
            RuntimeConfig::new(VmMode::Tso, 1, 1, 1),
            RuntimeRole::Authority,
        )
        .unwrap();
        let tick = runtime
            .next_tick(
                vec![1, 2]
                    .into_iter()
                    .map(|id| {
                        AcceptedCommand::Spawn(SpawnSpec {
                            guid: 100,
                            position: LotPosition::new(id * 16 + 8, 24, 1),
                            facing: Facing::NORTH,
                            persistent_id: PersistentId(id as u32),
                            avatar: true,
                        })
                    })
                    .collect(),
            )
            .unwrap();
        runtime.step(&tick).unwrap();
        let a = runtime.state.entities[&ObjectId(1)].info.reference;
        let b = runtime.state.entities[&ObjectId(2)].info.reference;
        (runtime.state, content, a, b)
    }
    fn action(
        content: &ContentSet,
        actor: EntityRef,
        target: EntityRef,
        routine: u16,
    ) -> ix::ActionInvocation {
        let mut definition = content.interaction_table(100).unwrap().definitions[0].clone();
        definition.action.routine_id = routine;
        ix::ActionInvocation::user(
            interaction_adapter::entity_key(actor),
            interaction_adapter::entity_key(target),
            definition,
            0,
        )
    }
    #[test]
    fn source_leave_recognizes_active_prefix_and_first_pending_only() {
        let (mut state, content, actor, _) = fixture();
        let queue = state.interaction_queues.get_mut(&actor.object_id).unwrap();
        queue
            .enqueue_internal(action(&content, actor, actor, 8373), &QueueHarness)
            .unwrap();
        assert!(
            leave_context(&state, &content, actor)
                .unwrap()
                .action_already_queued
        );
        let queue = state.interaction_queues.get_mut(&actor.object_id).unwrap();
        let id = queue.entries()[0].id;
        queue.cancel_internal(id).unwrap();
        queue
            .enqueue_internal(action(&content, actor, actor, 4096), &QueueHarness)
            .unwrap();
        queue
            .enqueue_internal(action(&content, actor, actor, 8373), &QueueHarness)
            .unwrap();
        assert_eq!(state.interaction_queues[&actor.object_id].active_len(), 0);
        assert!(
            !leave_context(&state, &content, actor)
                .unwrap()
                .action_already_queued
        );
        state
            .interaction_queues
            .get_mut(&actor.object_id)
            .unwrap()
            .attempt_push(&mut QueueHarness)
            .unwrap();
        assert_eq!(state.interaction_queues[&actor.object_id].active_len(), 1);
        assert!(
            leave_context(&state, &content, actor)
                .unwrap()
                .action_already_queued
        );
    }
    #[test]
    fn mode25_checks_detached_peer_frames_and_active_target_and_icon() {
        for kind in 0..3 {
            let (mut state, content, leaver, peer) = fixture();
            let mut invocation =
                action(&content, peer, if kind == 0 { leaver } else { peer }, 8373);
            if kind == 1 {
                invocation.icon_owner = Some(interaction_adapter::entity_key(leaver));
            }
            let queue = state.interaction_queues.get_mut(&peer.object_id).unwrap();
            queue.enqueue_internal(invocation, &QueueHarness).unwrap();
            queue.attempt_push(&mut QueueHarness).unwrap();
            let mut detached = state.threads.remove(&peer.object_id).unwrap();
            let mut context = FrameContext::for_entity(peer, 100);
            if kind == 2 {
                context.stack_object = leaver.object_id;
                context.stack_object_ref = Some(leaver);
            }
            detached
                .push_entry(
                    content.routines(),
                    RoutineKey {
                        scope: RoutineScope::SemiGlobal(900),
                        id: 8373,
                    },
                    context,
                    vec![0; 4],
                )
                .unwrap();
            let mut work = TickWork::default();
            assert!(
                !unwind_other_avatars(
                    &mut state,
                    &mut work,
                    leaver,
                    &[(peer.object_id, detached)].into()
                )
                .unwrap()
            );
            let queue = &state.interaction_queues[&peer.object_id];
            assert!(queue.active().unwrap().notify_idle);
            assert!(work.events.iter().any(|event|matches!(event,RuntimeEvent::Interaction(event) if event.kind==ix::QueueEventKind::CancelRequested)));
        }
    }
}
