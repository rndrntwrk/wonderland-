//! A's interpreter connected to B's source interaction query and queue rules.
//! Provider snapshots are owned bounded bytes. Only accepted tick commands and
//! VM calls install in-tick check writes or queue transitions.
use super::*;
use crate::interactions as ix;
use ix::adapters::{AuthorityOperation, CheckTreeProvider, QueueRuntime, WorldProvider};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionAccess {
    pub principal: ix::PrincipalKey,
    pub allow_hidden: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BehaviorStateOutcome {
    pub state: SimState,
    pub thread: VmThread,
    pub instructions: u32,
}

pub fn entity_key(reference: EntityRef) -> ix::EntityKey {
    ix::EntityKey {
        slot: reference.object_id.0 as u32,
        generation: reference.generation,
    }
}
pub fn entity_ref(key: ix::EntityKey) -> ix::Result<EntityRef> {
    let id = i16::try_from(key.slot)
        .ok()
        .filter(|id| *id > 0)
        .ok_or(ix::Error::StaleEntity(key))?;
    if key.generation == 0 {
        return Err(ix::Error::StaleEntity(key));
    }
    Ok(EntityRef {
        object_id: ObjectId(id),
        generation: key.generation,
    })
}
fn dialect(mode: VmMode) -> ix::LegacyMode {
    match mode {
        VmMode::Tso => ix::LegacyMode::Tso,
        VmMode::Ts1 => ix::LegacyMode::Ts1,
    }
}
fn ix_failure(_: impl std::fmt::Debug) -> ix::Error {
    ix::Error::RuntimeFailure("source interpreter rejected interaction")
}

impl SimRuntime {
    /// Run with all writable state detached, preserving real register, RNG,
    /// advertisement, action-string and entity changes in the returned value.
    /// Returning this result does not commit state or dispatch an external effect.
    pub fn query_behavior_state(
        &self,
        entity: EntityRef,
        routine: RoutineKey,
        context: FrameContext,
        args: Vec<i16>,
        budget: u32,
    ) -> Result<BehaviorStateOutcome, RuntimeError> {
        run_detached_check(
            self.state.clone(),
            &self.content,
            entity,
            routine,
            context,
            args,
            budget,
        )
    }
    pub fn interaction_offers(
        &self,
        principal: ix::PrincipalKey,
        actor: EntityRef,
        target: EntityRef,
        options: ix::QueryOptions,
    ) -> ix::Result<ix::OfferBatch> {
        let limits = ix::InteractionLimits::default();
        let world = RuntimeWorld {
            state: &self.state,
            content: &self.content,
        };
        let actor = world
            .entity_version(entity_key(actor))
            .ok_or(ix::Error::StaleEntity(entity_key(actor)))?;
        let target = world
            .entity_version(entity_key(target))
            .ok_or(ix::Error::StaleEntity(entity_key(target)))?;
        ix::query_offers(
            &world,
            &RuntimeChecks::new(&self.content, limits),
            ix::OfferQuery {
                principal,
                seen: ix::ViewStamp {
                    world_revision: world.revision(),
                    actor,
                    target,
                },
                options,
            },
            &limits,
        )
    }
}

fn run_detached_check(
    mut state: SimState,
    content: &ContentSet,
    actor: EntityRef,
    routine: RoutineKey,
    context: FrameContext,
    args: Vec<i16>,
    budget: u32,
) -> Result<BehaviorStateOutcome, RuntimeError> {
    live_entity(&state, actor)?;
    live_entity(&state, context.caller)?;
    live_entity(&state, context.callee)?;
    if let Some(target) = context.stack_object_ref {
        live_entity(&state, target)?;
    }
    if budget == 0 || budget > state.limits.instruction_budget_per_entity {
        return Err(invalid("check instruction budget"));
    }
    let mut thread = VmThread::new(actor, state.mode);
    thread.is_check = true;
    thread.action_strings = Some(Vec::new());
    if let Some(original) = state.threads.get(&actor.object_id) {
        thread.temps = original.temps;
        thread.temp_xl = original.temp_xl;
    }
    if !thread.push_entry(content.routines(), routine, context, args)? {
        thread.stop = VmStop::Completed(PrimitiveExit::ReturnFalse);
        return Ok(BehaviorStateOutcome {
            state,
            thread,
            instructions: 0,
        });
    }
    let mut work = TickWork::default();
    let report = run_synchronous_thread(
        &mut state,
        content,
        &mut work,
        actor,
        0,
        &mut thread,
        budget,
        &mut BTreeMap::new(),
        true,
    )?;
    thread.stop = report.stop;
    Ok(BehaviorStateOutcome {
        state,
        thread,
        instructions: report.instructions.saturating_add(work.instructions),
    })
}

struct RuntimeWorld<'a> {
    state: &'a SimState,
    content: &'a ContentSet,
}
impl WorldProvider for RuntimeWorld<'_> {
    fn revision(&self) -> u64 {
        self.state.completed_tick
    }
    fn entity_version(&self, key: ix::EntityKey) -> Option<ix::EntityVersion> {
        let reference = entity_ref(key).ok()?;
        let item = self.state.entities.get(&reference.object_id)?;
        (self.state.ids.is_live(reference) && item.info.reference == reference && !item.info.dead)
            .then_some(ix::EntityVersion {
                key,
                revision: item.revision,
            })
    }
    fn authorize(
        &self,
        principal: ix::PrincipalKey,
        actor: ix::EntityKey,
        operation: AuthorityOperation,
    ) -> bool {
        let Ok(actor) = entity_ref(actor) else {
            return false;
        };
        self.state.ids.is_live(actor)
            && self
                .state
                .interaction_access
                .get(&actor)
                .is_some_and(|access| {
                    access.principal == principal
                        && match operation {
                            AuthorityOperation::Query {
                                include_hidden: true,
                            } => access.allow_hidden,
                            _ => true,
                        }
                })
    }
    fn snapshot(
        &self,
        actor: ix::EntityKey,
        target: ix::EntityKey,
        limits: &ix::InteractionLimits,
    ) -> ix::Result<ix::InteractionSnapshot> {
        let actor_version = self
            .entity_version(actor)
            .ok_or(ix::Error::StaleEntity(actor))?;
        let target_version = self
            .entity_version(target)
            .ok_or(ix::Error::StaleEntity(target))?;
        let actor_ref = entity_ref(actor)?;
        let target_ref = entity_ref(target)?;
        let actor_item = live_entity(self.state, actor_ref).map_err(ix_failure)?;
        let target_item = live_entity(self.state, target_ref).map_err(ix_failure)?;
        let avatar = actor_item.avatar.as_ref();
        let mut check = ix::CheckState::default();
        check.rng_seed = self.state.rng.state();
        check.hidden = actor_item.object_data[34] == 1;
        check.out_of_world =
            actor_item.info.position.x == i16::MIN && actor_item.info.position.y == i16::MIN;
        check.target_occupied =
            crate::runtime_memory::is_in_use(self.state, target_ref, None).map_err(ix_failure)?;
        if let Some(thread) = self.state.threads.get(&actor_ref.object_id) {
            check.temp_registers = thread.temps;
            check.temp_xl = thread.temp_xl;
        }
        let bytes = canonical_bytes(self.state, limits.max_provider_state_bytes as u64)
            .map_err(ix_failure)?;
        check.set_provider_state(&bytes, limits)?;
        let actor = ix::ActorFacts {
            version: actor_version,
            is_avatar: avatar.is_some(),
            species: if avatar.is_some_and(|a| a.is_cat()) {
                ix::Species::Cat
            } else if avatar.is_some_and(|a| a.is_dog()) {
                ix::Species::Dog
            } else {
                ix::Species::Human
            },
            permission: match avatar.map(|a| a.permissions) {
                Some(crate::avatars::state::AvatarPermissions::Roommate) => {
                    ix::AvatarPermission::Roommate
                }
                Some(crate::avatars::state::AvatarPermissions::BuildBuyRoommate) => {
                    ix::AvatarPermission::BuildBuyRoommate
                }
                Some(crate::avatars::state::AvatarPermissions::Owner) => {
                    ix::AvatarPermission::Owner
                }
                Some(crate::avatars::state::AvatarPermissions::Admin) => {
                    ix::AvatarPermission::Admin
                }
                _ => ix::AvatarPermission::Visitor,
            },
            carrying: actor_item.slots.first().is_some_and(Option::is_some),
            ghost: avatar.is_some_and(|a| a.person_data.values[68] > 0),
            owns_target: !target_item.info.is_avatar
                && self
                    .state
                    .world
                    .object(target_ref)
                    .and_then(|o| o.owner)
                    .is_some_and(|owner| owner.0 != 0 && owner.0 == actor_item.info.persistent_id),
            ts1_ungreeted_visitor: avatar
                .is_some_and(|a| a.person_data.values[32] == 1 && a.person_data.values[34] < 2),
            age: avatar.map_or(0, |a| a.person_data.values[58]),
        };
        let target = ix::TargetFacts {
            version: target_version,
            is_game_object: !target_item.info.is_avatar,
            broken: self
                .state
                .entities
                .get(&target_item.info.base_object)
                .is_some_and(|base| base.broken),
            disabled: target_item.disabled_flags != 0,
        };
        let mut snapshot = ix::InteractionSnapshot::new(
            dialect(self.state.mode),
            self.revision(),
            actor,
            target,
            check,
            limits,
        )?;
        if let Some(table) = self.content.interaction_table(target_item.info.guid) {
            snapshot.set_local_table_present(table.local_table_present)?;
            for definition in &table.definitions {
                snapshot.add_definition(definition.clone(), limits)?;
            }
        } else {
            snapshot.set_local_table_present(false)?;
        }
        Ok(snapshot)
    }
}

struct RuntimeChecks<'a> {
    content: &'a ContentSet,
    limits: ix::InteractionLimits,
    spent: std::cell::Cell<u32>,
}
impl<'a> RuntimeChecks<'a> {
    fn new(content: &'a ContentSet, limits: ix::InteractionLimits) -> Self {
        Self {
            content,
            limits,
            spent: std::cell::Cell::new(0),
        }
    }
}
impl CheckTreeProvider for RuntimeChecks<'_> {
    fn evaluate(
        &self,
        request: ix::CheckRequest<'_>,
        check: &mut ix::CheckState,
        output: &mut ix::CheckOutput,
        budget: &mut ix::CheckBudget,
    ) -> ix::Result<ix::CheckExit> {
        let mut state = crate::snapshot::decode_owned_check_state(
            check.provider_state(),
            self.limits.max_provider_state_bytes as u64,
        )
        .map_err(ix_failure)?;
        let actor = entity_ref(request.actor)?;
        let target = entity_ref(request.target)?;
        live_entity(&state, actor).map_err(ix_failure)?;
        live_entity(&state, target).map_err(ix_failure)?;
        state.rng = SimRng::new(check.rng_seed);
        let thread = state
            .threads
            .get_mut(&actor.object_id)
            .ok_or(ix::Error::StaleEntity(request.actor))?;
        thread.temps = check.temp_registers;
        thread.temp_xl = check.temp_xl;
        live_entity_mut(&mut state, actor)
            .map_err(ix_failure)?
            .object_data[50] = 0;
        // The source network verifier alone ignores target occupancy. Every
        // in-tick check sees the real group occupancy again.
        if request.origin == ix::CheckOrigin::IntentValidation && !check.target_occupied {
            let group = live_entity(&state, target)
                .map_err(ix_failure)?
                .info
                .group
                .clone();
            for id in group.into_iter().chain(std::iter::once(target.object_id)) {
                if let Some(item) = state.entities.get_mut(&id) {
                    item.object_data[8] &= !(1 << 5);
                    item.queued_users.clear();
                }
            }
        }
        let binding = request.definition.check.ok_or(ix::Error::Unavailable)?;
        let routine = self
            .content
            .routines()
            .resolve(binding.code_owner_guid, binding.routine_id)
            .ok_or(ix::Error::Unavailable)?;
        let available = u32::try_from(budget.remaining())
            .unwrap_or(u32::MAX)
            .min(state.limits.instruction_budget_per_entity);
        if available == 0 {
            return Err(ix::Error::LimitExceeded("check instruction budget"));
        }
        let original_effects = state.effects.clone();
        let result = run_detached_check(
            state,
            self.content,
            actor,
            routine,
            FrameContext {
                caller: actor,
                callee: target,
                stack_object: target.object_id,
                stack_object_ref: Some(target),
                code_owner: request.definition.action.code_owner_guid,
            },
            request.args.to_vec(),
            available,
        )
        .map_err(ix_failure)?;
        budget.spend(u64::from(result.instructions))?;
        self.spent
            .set(self.spent.get().saturating_add(result.instructions));
        if result.thread.diagnostics.iter().any(|d| {
            matches!(
                d,
                VmDiagnostic::MissingPrimitive { .. } | VmDiagnostic::MissingRoutine { .. }
            )
        }) {
            return Err(ix::Error::Unsupported(
                "original check uses an unsupported opcode or missing routine",
            ));
        }
        if result.state.effects != original_effects {
            return Err(ix::Error::Unsupported(
                "check requested an asynchronous external effect",
            ));
        }
        let exit = match result.thread.stop {
            VmStop::Completed(PrimitiveExit::ReturnTrue) => ix::CheckExit::ReturnTrue,
            VmStop::Completed(_) => ix::CheckExit::ReturnFalse,
            VmStop::BudgetExhausted => {
                return Err(ix::Error::LimitExceeded("check instruction budget"))
            }
            VmStop::Faulted(_) => {
                return Err(ix::Error::Unsupported(
                    "original check needs an unsupported runtime operation",
                ))
            }
            _ => ix::CheckExit::Yielded,
        };
        check.rng_seed = result.state.rng.state();
        check.temp_registers = result.thread.temps;
        check.temp_xl = result.thread.temp_xl;
        check.hide_interaction = result.state.entities[&actor.object_id].object_data[50] == 1;
        let actor_item = &result.state.entities[&actor.object_id];
        check.hidden = actor_item.object_data[34] == 1;
        check.out_of_world =
            actor_item.info.position.x == i16::MIN && actor_item.info.position.y == i16::MIN;
        for ((kind, index), value) in &result.thread.tree_advertisements {
            check.set_advertisement(
                (i32::from(*kind) << 16) | i32::from(*index),
                *value,
                &self.limits,
            )?;
        }
        for variant in result.thread.action_strings.as_deref().unwrap_or(&[]) {
            output.push_variant(Some(&variant.name), variant.parameter0)?;
        }
        let mut final_state = result.state;
        if request.origin == ix::CheckOrigin::InTick {
            if let Some(thread) = final_state.threads.get_mut(&actor.object_id) {
                thread.temps = result.thread.temps;
                thread.temp_xl = result.thread.temp_xl;
            }
        }
        let bytes = canonical_bytes(&final_state, self.limits.max_provider_state_bytes as u64)
            .map_err(ix_failure)?;
        check.set_provider_state(&bytes, &self.limits)?;
        Ok(exit)
    }
}

pub(crate) fn new_queue(actor: EntityRef, mode: VmMode) -> Result<ix::ActionQueue, RuntimeError> {
    ix::ActionQueue::new(
        entity_key(actor),
        dialect(mode),
        ix::InteractionLimits::default(),
    )
    .map_err(invalid)
}

pub(super) fn enqueue_intent(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    intent: ix::InteractionIntent,
) -> Result<(), RuntimeError> {
    let actor = entity_ref(intent.seen.actor.key).map_err(invalid)?;
    let mut queue = state
        .interaction_queues
        .remove(&actor.object_id)
        .ok_or_else(|| invalid("actor queue missing"))?;
    let limits = ix::InteractionLimits::default();
    let world = RuntimeWorld { state, content };
    let checks = RuntimeChecks::new(content, limits);
    let validated =
        ix::validate_intent(&world, &checks, &queue, intent, &limits).map_err(invalid)?;
    work.instructions = work
        .instructions
        .checked_add(checks.spent.get())
        .ok_or(RuntimeError::InstructionLimit)?;
    if work.instructions > state.limits.max_tick_instructions {
        return Err(RuntimeError::InstructionLimit);
    }
    let transition = queue
        .enqueue_validated(&world, &ReadQueue(state), validated)
        .map_err(invalid)?;
    state.interaction_queues.insert(actor.object_id, queue);
    install_events(state, work, transition.events)?;
    Ok(())
}
pub(super) fn cancel_intent(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    intent: ix::CancelIntent,
) -> Result<(), RuntimeError> {
    let actor = entity_ref(intent.actor.key).map_err(invalid)?;
    let mut queue = state
        .interaction_queues
        .remove(&actor.object_id)
        .ok_or_else(|| invalid("actor queue missing"))?;
    let transition = queue
        .cancel_intent(&RuntimeWorld { state, content }, intent)
        .map_err(invalid)?;
    state.interaction_queues.insert(actor.object_id, queue);
    state.scheduler.interrupt(actor).map_err(invalid)?;
    install_events(state, work, transition.events)
}
struct ReadQueue<'a>(&'a SimState);
impl QueueRuntime for ReadQueue<'_> {
    fn target_is_alive(&self, target: ix::EntityKey) -> bool {
        entity_ref(target).is_ok_and(|r| {
            self.0.ids.is_live(r)
                && self
                    .0
                    .entities
                    .get(&r.object_id)
                    .is_some_and(|e| !e.info.dead)
        })
    }
    fn check_action(&mut self, _: &ix::ActionInvocation) -> ix::Result<bool> {
        Err(ix::Error::Unsupported("read-only queue adapter"))
    }
    fn start_action(&mut self, _: &ix::ActionInvocation) -> ix::Result<bool> {
        Err(ix::Error::Unsupported("read-only queue adapter"))
    }
}

fn install_events(
    state: &mut SimState,
    work: &mut TickWork,
    events: Vec<ix::QueueEvent>,
) -> Result<(), RuntimeError> {
    for event in events {
        let actor = entity_ref(event.actor).map_err(invalid)?;
        match event.kind {
            ix::QueueEventKind::ActorCancellationChanged { value } => {
                let item = live_entity_mut(state, actor)?;
                item.object_data[8] =
                    (item.object_data[8] & !(1 << 7)) | if value { 1 << 7 } else { 0 };
            }
            ix::QueueEventKind::PriorityChanged { value } => {
                if let Some(avatar) = live_entity_mut(state, actor)?.avatar.as_mut() {
                    avatar.person_data.values[33] = value;
                }
            }
            ix::QueueEventKind::ResetAvatarInteractionStateRequested => {
                if let Some(avatar) = live_entity_mut(state, actor)?.avatar.as_mut() {
                    avatar.person_data.values[71] = 0;
                    avatar.motives.clear_changes();
                }
            }
            _ => {}
        }
        work.events.push(RuntimeEvent::Interaction(event));
    }
    Ok(())
}

struct TickQueue<'a> {
    state: &'a mut SimState,
    content: &'a ContentSet,
    work: &'a mut TickWork,
    thread: Option<VmThread>,
    pending: Option<RoutineCall>,
}
impl QueueRuntime for TickQueue<'_> {
    fn target_is_alive(&self, target: ix::EntityKey) -> bool {
        ReadQueue(self.state).target_is_alive(target)
    }
    fn check_action(&mut self, action: &ix::ActionInvocation) -> ix::Result<bool> {
        let limits = ix::InteractionLimits::default();
        let actor = entity_ref(action.actor)?;
        let mut owned = self.state.clone();
        if let Some(thread) = &self.thread {
            owned.threads.insert(actor.object_id, thread.clone());
        }
        let mut snapshot = RuntimeWorld {
            state: &owned,
            content: self.content,
        }
        .snapshot(action.actor, action.target, &limits)?;
        // Evaluate exactly the queued definition, retaining pushed continuation flags.
        let base = snapshot.clone();
        snapshot = ix::InteractionSnapshot::new(
            base.mode(),
            base.stamp().world_revision,
            base.actor_facts().clone(),
            base.target_facts().clone(),
            base.state().clone(),
            &limits,
        )?;
        snapshot.add_definition(action.definition.clone(), &limits)?;
        let checks = RuntimeChecks::new(self.content, limits);
        let offers = ix::query_in_tick(
            &mut snapshot,
            &checks,
            ix::QueryOptions {
                include_hidden: true,
                ..ix::QueryOptions::default()
            },
            &limits,
        )?;
        self.work.instructions = self
            .work
            .instructions
            .checked_add(checks.spent.get())
            .ok_or(ix::Error::LimitExceeded("tick instruction budget"))?;
        if self.work.instructions > self.state.limits.max_tick_instructions {
            return Err(ix::Error::LimitExceeded("tick instruction budget"));
        }
        let changed = crate::snapshot::decode_owned_check_state(
            snapshot.state().provider_state(),
            limits.max_provider_state_bytes as u64,
        )
        .map_err(ix_failure)?;
        // Detached caller thread snapshots are mirrors. Install legitimate register
        // writes while preserving the active interpreter's stack ownership.
        if let Some(thread) = &mut self.thread {
            thread.temps = snapshot.state().temp_registers;
            thread.temp_xl = snapshot.state().temp_xl;
        }
        let mut changed = changed;
        if self.thread.is_some() {
            changed.threads.remove(&actor.object_id);
        }
        *self.state = changed;
        Ok(!offers.offers.is_empty())
    }
    fn start_action(&mut self, action: &ix::ActionInvocation) -> ix::Result<bool> {
        let actor = entity_ref(action.actor)?;
        let target = entity_ref(action.target)?;
        let stack = entity_ref(action.stack_object)?;
        if !self.target_is_alive(action.target) || !self.target_is_alive(action.stack_object) {
            return Ok(false);
        }
        let binding = action.definition.action;
        let routine = self
            .content
            .routines()
            .resolve(binding.code_owner_guid, binding.routine_id)
            .ok_or(ix::Error::Unavailable)?;
        let call = RoutineCall {
            routine,
            context: FrameContext {
                caller: actor,
                callee: target,
                stack_object: stack.object_id,
                stack_object_ref: Some(stack),
                code_owner: binding.code_owner_guid,
            },
            args: action.args.to_vec(),
            action_tree: true,
            special_result: SpecialResult::Interaction {
                run_immediately: action
                    .definition
                    .flags
                    .has(ix::ActionFlags::RUN_IMMEDIATELY),
            },
        };
        let original = self
            .thread
            .as_ref()
            .or_else(|| self.state.threads.get(&actor.object_id))
            .ok_or(ix::Error::StaleEntity(action.actor))?;
        if original.continuation.is_some() || matches!(original.stop, VmStop::Faulted(_)) {
            return Ok(false);
        }
        let mut next = original.clone();
        if !next
            .push_entry(
                self.content.routines(),
                call.routine,
                call.context.clone(),
                call.args.clone(),
            )
            .map_err(ix_failure)?
        {
            return Ok(false);
        }
        let frame = next.top_mut().map_err(ix_failure)?;
        frame.action_tree = true;
        frame.special_result = call.special_result;
        if self.thread.is_some() {
            self.pending = Some(call);
        } else {
            self.state.threads.insert(actor.object_id, next);
        }
        Ok(true)
    }
}

pub(super) fn poll_queue(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    actor: EntityRef,
    immediate_only: bool,
) -> Result<(), RuntimeError> {
    let Some(mut queue) = state.interaction_queues.remove(&actor.object_id) else {
        return Ok(());
    };
    let mut adapter = TickQueue {
        state,
        content,
        work,
        thread: None,
        pending: None,
    };
    let transition = if immediate_only {
        queue.try_run_immediately(&mut adapter)
    } else {
        queue.attempt_push(&mut adapter)
    };
    state.interaction_queues.insert(actor.object_id, queue);
    install_events(state, work, transition.map_err(invalid)?.events)
}

pub(super) fn finish_faulted(
    state: &mut SimState,
    work: &mut TickWork,
    actor: EntityRef,
    thread: &VmThread,
) -> Result<(), RuntimeError> {
    if !matches!(thread.stop, VmStop::Faulted(_)) {
        return Ok(());
    }
    let Some(mut queue) = state.interaction_queues.remove(&actor.object_id) else {
        return Ok(());
    };
    for _ in 0..queue.active_len() {
        let transition = queue
            .finish_active(ix::FinishResult::Aborted, &ReadQueue(state))
            .map_err(invalid)?;
        install_events(state, work, transition.events)?;
    }
    state.interaction_queues.insert(actor.object_id, queue);
    Ok(())
}

impl RuntimeHost<'_> {
    pub(super) fn queue_returned(
        &mut self,
        actor: EntityRef,
        exit: PrimitiveExit,
    ) -> Result<(), VmFault> {
        let Some(mut queue) = self.state.interaction_queues.remove(&actor.object_id) else {
            return Err(host_error("returned source action has no queue"));
        };
        let result = if exit == PrimitiveExit::ReturnTrue {
            ix::FinishResult::Succeeded
        } else {
            ix::FinishResult::Failed
        };
        let transition = queue.finish_active(result, &ReadQueue(self.state));
        self.state.interaction_queues.insert(actor.object_id, queue);
        install_events(
            self.state,
            self.work,
            transition.map_err(host_error)?.events,
        )
        .map_err(host_error)
    }
    pub(super) fn queue_attempt_push(
        &mut self,
        owner: EntityRef,
    ) -> Result<Option<RoutineCall>, VmFault> {
        if self.separate_check {
            return Ok(None);
        }
        let thread = self
            .active
            .as_ref()
            .filter(|t| t.owner == owner)
            .cloned()
            .or_else(|| self.entity_threads.get(&owner.object_id).cloned())
            .or_else(|| self.state.threads.get(&owner.object_id).cloned())
            .ok_or_else(|| host_error("missing source queue thread"))?;
        let Some(mut queue) = self.state.interaction_queues.remove(&owner.object_id) else {
            return Ok(None);
        };
        let mut adapter = TickQueue {
            state: self.state,
            content: self.content,
            work: self.work,
            thread: Some(thread),
            pending: None,
        };
        let transition = queue.attempt_push(&mut adapter);
        let call = adapter.pending.take();
        let banks = adapter.thread.as_ref().map(|t| (t.temps, t.temp_xl));
        self.state.interaction_queues.insert(owner.object_id, queue);
        install_events(
            self.state,
            self.work,
            transition.map_err(host_error)?.events,
        )
        .map_err(host_error)?;
        if let Some((temps, temp_xl)) = banks {
            self.update_entity_banks(owner, temps, temp_xl, true);
        }
        Ok(call)
    }
    pub(super) fn queue_idle(
        &mut self,
        context: &FrameContext,
        allow_push: bool,
        action_tree: bool,
    ) -> Result<IdleDecision, VmFault> {
        if self.separate_check {
            return Ok(IdleDecision::Quiet);
        }
        if allow_push {
            if let Some(call) = self.queue_attempt_push(context.caller)? {
                return Ok(IdleDecision::Push(call));
            }
        }
        let notified = self
            .state
            .interaction_queues
            .get(&context.caller.object_id)
            .is_some_and(|q| {
                if action_tree {
                    q.active().is_some_and(|entry| entry.notify_idle)
                } else {
                    !q.entries().is_empty()
                }
            });
        Ok(if notified {
            IdleDecision::Notified
        } else {
            IdleDecision::Quiet
        })
    }
    pub(super) fn queue_available(
        &self,
        source: EntityRef,
        target: EntityRef,
        interaction: u8,
    ) -> Result<bool, VmFault> {
        let source = live_entity(self.state, source)?;
        live_entity(self.state, target)?;
        Ok(self
            .content
            .interaction_table(source.info.guid)
            .is_some_and(|table| {
                table.definitions.iter().any(|d| {
                    d.key.scope == ix::InteractionScope::Local
                        && d.key.tta_index == u32::from(interaction)
                })
            }))
    }
    pub(super) fn queue_push(&mut self, request: PushInteractionRequest) -> Result<bool, VmFault> {
        if self.separate_check {
            return Err(host_error(
                "source push from a detached check requires a check-local queue",
            ));
        }
        if request.immediate_result_chooser {
            return Err(host_error(
                "source cross-avatar interaction result chooser is not integrated",
            ));
        }
        let source = live_entity(self.state, request.source)?;
        live_entity(self.state, request.target)?;
        let Some(mut definition) = self
            .content
            .interaction_table(source.info.guid)
            .and_then(|table| {
                table.definitions.iter().find(|d| {
                    d.key.scope == ix::InteractionScope::Local
                        && d.key.tta_index == u32::from(request.interaction)
                })
            })
            .cloned()
        else {
            return Ok(false);
        };
        if request.skip_permissions {
            definition.flags.0 |= ix::ActionFlags::SKIP_PERMISSIONS;
        }
        if request.push_head {
            definition.flags.0 |= ix::ActionFlags::PUSH_HEAD;
        }
        if request.push_tail {
            definition.flags.0 |= ix::ActionFlags::PUSH_TAIL;
        }
        let action = ix::ActionInvocation {
            actor: entity_key(request.target),
            target: entity_key(request.source),
            stack_object: entity_key(request.source),
            icon_owner: if request.custom_icon {
                request.icon.map(entity_key)
            } else {
                None
            },
            definition,
            args: [0; 4],
            priority: request.priority,
            mode: match request.mode {
                QueueMode::Normal => ix::QueueMode::Normal,
                QueueMode::ParentIdle => ix::QueueMode::ParentIdle,
                QueueMode::ParentExit => ix::QueueMode::ParentExit,
                QueueMode::Idle => ix::QueueMode::Idle,
            },
            callback: None,
            interaction_result: -1,
            result_check_counter: 0,
        };
        let Some(mut queue) = self
            .state
            .interaction_queues
            .remove(&request.target.object_id)
        else {
            return Err(host_error("source push recipient queue missing"));
        };
        let transition = queue.enqueue_internal(action, &ReadQueue(self.state));
        self.state
            .interaction_queues
            .insert(request.target.object_id, queue);
        install_events(
            self.state,
            self.work,
            transition.map_err(host_error)?.events,
        )
        .map_err(host_error)?;
        self.state
            .scheduler
            .interrupt(request.target)
            .map_err(host_error)?;
        Ok(true)
    }
    pub(super) fn queue_icon(&mut self, actor: EntityRef, icon: EntityRef) -> Result<(), VmFault> {
        live_entity(self.state, icon)?;
        let Some(mut queue) = self.state.interaction_queues.remove(&actor.object_id) else {
            return Err(host_error("source icon queue missing"));
        };
        let transition = queue.set_active_icon(entity_key(icon));
        self.state.interaction_queues.insert(actor.object_id, queue);
        install_events(
            self.state,
            self.work,
            transition.map_err(host_error)?.events,
        )
        .map_err(host_error)
    }
}

pub(crate) fn validate_state(state: &SimState, content: &ContentSet) -> Result<(), String> {
    if state.interaction_queues.len() > state.entities.len()
        || state.interaction_access.len() > state.entities.len()
    {
        return Err("interaction state count".into());
    }
    for (id, queue) in &state.interaction_queues {
        queue.validate().map_err(|e| e.to_string())?;
        let actor = entity_ref(queue.owner()).map_err(|e| e.to_string())?;
        if actor.object_id != *id
            || !state.ids.is_live(actor)
            || queue.mode() != dialect(state.mode)
        {
            return Err("stale queue owner or dialect".into());
        }
        let thread = state.threads.get(id).ok_or("queue thread missing")?;
        let frames: Vec<_> = thread
            .frames
            .iter()
            .filter(|f| matches!(f.special_result, SpecialResult::Interaction { .. }))
            .collect();
        if !matches!(thread.stop, VmStop::Faulted(_)) && frames.len() != queue.active_len() {
            return Err("queue active prefix/frame mismatch".into());
        }
        for (index, entry) in queue.entries().iter().enumerate() {
            let binding = entry.invocation.definition.action;
            let routine = content
                .routines()
                .resolve(binding.code_owner_guid, binding.routine_id)
                .ok_or("queued action routine missing")?;
            if index < frames.len()
                && (frames[index].routine != routine
                    || frames[index].context.callee
                        != entity_ref(entry.invocation.target).map_err(|e| e.to_string())?)
            {
                return Err("queue action/frame binding mismatch".into());
            }
        }
    }
    for (actor, access) in &state.interaction_access {
        if !state.ids.is_live(*actor) || access.principal.0 == 0 {
            return Err("stale interaction authority".into());
        }
    }
    Ok(())
}
