//! A deterministic, failure-atomic simulation transaction per accepted 30Hz tick.
//!
//! `AcceptedTick` is input from a trusted command-admission adapter. Constructing
//! one is not authentication or permission to spend money. Authorities and
//! replicas run the same transition; only authorities expose effect dispatches.
use crate::{
    avatars::{
        motives::MotiveDecay, AvatarPlatform, AvatarState, AvatarTickContext, AvatarTickOutput,
    },
    clock::SimClock,
    effects::*,
    ids::*,
    rng::SimRng,
    scheduler::Scheduler,
    state::*,
    vm::*,
    world::{
        self, Facing, LotModel, LotPosition, RouteContinuation, RouteGoal, RouteStep, WorldObject,
        WorldState,
    },
};
use bincode::Options;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[path = "runtime_build.rs"]
mod build_adapter;
#[path = "runtime_interactions.rs"]
pub mod interaction_adapter;

const MAX_TICK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CREATION_DEPTH: u16 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeRole {
    Authority,
    Replica,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub mode: VmMode,
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub seed: u64,
    pub utc_start_dotnet_ticks: i64,
    pub effect_namespace: u64,
    pub limits: SimulationLimits,
    pub effect_limits: EffectLimits,
}
impl RuntimeConfig {
    /// The namespace must identify this durable operation stream. Retain it and
    /// its nonce on recovery; an E-owned journal supplies a new namespace on reset.
    pub fn new(mode: VmMode, lot_id: u64, authority_epoch: u64, seed: u64) -> Self {
        Self {
            mode,
            lot_id,
            authority_epoch,
            seed,
            utc_start_dotnet_ticks: 0,
            effect_namespace: lot_id,
            limits: SimulationLimits::default(),
            effect_limits: EffectLimits::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnSpec {
    pub guid: u32,
    pub position: LotPosition,
    pub facing: Facing,
    pub persistent_id: PersistentId,
    pub avatar: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Keep accepted command payloads inline to preserve the public replay API and layout.
#[allow(clippy::large_enum_variant)]
pub enum AcceptedCommand {
    Spawn(SpawnSpec),
    Delete {
        entity: EntityRef,
    },
    Interrupt {
        entity: EntityRef,
    },
    StartBehavior {
        entity: EntityRef,
        routine: RoutineKey,
        context: FrameContext,
        args: Vec<i16>,
        replace: bool,
    },
    WriteMemory {
        address: MemoryAddress,
        value: i16,
    },
    SetInteractionProjection {
        entity: EntityRef,
        queued_users: BTreeSet<EntityRef>,
        active_advertisements: Option<BTreeMap<(u8, u16), i16>>,
    },
    SetEveryFrame {
        entity: EntityRef,
        enabled: bool,
    },
    SetObjectStatus {
        entity: EntityRef,
        disabled_flags: u8,
        broken: bool,
    },
    BeginRoute {
        entity: EntityRef,
        target: Option<EntityRef>,
        goals: Vec<RouteGoal>,
    },
    RouteCallback {
        continuation_id: u64,
        route_id: u64,
        token: u64,
        outcome: world::routing::CallbackOutcome,
    },
    DefineSlot(world::SlotDefinition),
    ReserveSlot(world::ReservationRequest),
    OccupySlot(world::ReservationToken),
    ReleaseSlot(world::ReservationToken),
    EffectResolved(EffectResolved),
    Disconnect {
        entity: EntityRef,
        session_epoch: u64,
    },
    Reconnect {
        entity: EntityRef,
        session_epoch: u64,
    },
    Reset {
        entity: EntityRef,
    },
    SetTs1FamilyBudget(Option<i32>),
    SetTs1Inventory {
        neighbor: i16,
        items: Vec<Ts1InventoryItem>,
    },
    /// Trusted authority grant. Never construct from an unauthenticated UI payload.
    SetInteractionAuthority {
        actor: EntityRef,
        access: Option<interaction_adapter::InteractionAccess>,
    },
    QueueInteraction(crate::interactions::InteractionIntent),
    CancelInteraction(crate::interactions::CancelIntent),
    /// Prepared from authoritative source permissions, prices and content.
    BeginBuild {
        preview: world::build::BuildPreview,
        authority: world::build::BuildAuthority,
    },
    /// Server-only durable receipt lane. A transport write is not this receipt.
    CompleteBuild(world::build::ServerBuildConfirmation),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AcceptedTick {
    pub lot_id: u64,
    pub epoch: u64,
    pub tick: u64,
    pub content: ContentDescriptor,
    pub rng_before: u64,
    pub commands: Vec<AcceptedCommand>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeEvent {
    Spawned(EntityRef),
    Deleted(EntityRef),
    ThreadFault {
        entity: EntityRef,
        fault: VmFault,
    },
    Avatar {
        entity: EntityRef,
        output: AvatarTickOutput,
    },
    Memory(crate::runtime_memory::MemorySignal),
    ExternalRequested {
        operation_id: OperationId,
        entity: EntityRef,
    },
    Presentation(ExternalRequest),
    Display(PresentationRequest),
    Appearance(AppearanceOperation),
    MoneyHeadline {
        entity: EntityRef,
        amount: i32,
    },
    RouteScript {
        continuation_id: u64,
        callback: world::routing::RouteCallback,
    },
    RouteFinished {
        continuation_id: u64,
        entity: EntityRef,
        success: bool,
        code: world::RouteFailCode,
        blocker: Option<EntityRef>,
    },
    SlotReserved(world::ReservationToken),
    Interaction(crate::interactions::QueueEvent),
    BuildRequested(world::build::DurableBuildRequest),
    BuildCompleted(world::build::BuildCommitResult),
    BuildRuntimeFault {
        operation: u64,
        error: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TickOutcome {
    pub tick: u64,
    pub duplicate: bool,
    pub state_hash: [u8; 32],
    pub instructions: u32,
    pub events: Vec<RuntimeEvent>,
    pub effects: Vec<EffectDispatch>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryOutcome {
    pub stop: VmStop,
    pub temps: [i16; 20],
    pub temp_xl: [i32; 2],
    pub instructions: u32,
    pub diagnostics: Vec<VmDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    InvalidState(String),
    InvalidCommand(String),
    Vm(VmFault),
    Snapshot(String),
    WrongLot,
    WrongEpoch,
    WrongContent,
    WrongRng,
    TickGap { expected: u64, received: u64 },
    ConflictingTick,
    TickOverflow,
    InstructionLimit,
    CreationDepth,
    ContinuationLimit,
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RuntimeError {}
impl From<VmFault> for RuntimeError {
    fn from(value: VmFault) -> Self {
        Self::Vm(value)
    }
}
fn invalid(value: impl std::fmt::Debug) -> RuntimeError {
    RuntimeError::InvalidCommand(format!("{value:?}"))
}
fn host_error(value: impl std::fmt::Debug) -> VmFault {
    VmFault::HostUnsupported(format!("{value:?}"))
}

pub struct SimRuntime {
    state: SimState,
    content: ContentSet,
    role: RuntimeRole,
}
impl SimRuntime {
    pub fn new(
        content: ContentSet,
        lot: LotModel,
        config: RuntimeConfig,
        role: RuntimeRole,
    ) -> Result<Self, RuntimeError> {
        content.validate().map_err(RuntimeError::InvalidState)?;
        config
            .limits
            .validate()
            .map_err(RuntimeError::InvalidState)?;
        if config.lot_id == 0 || config.authority_epoch == 0 {
            return Err(RuntimeError::InvalidState(
                "lot and epoch must be nonzero".into(),
            ));
        }
        let clock = SimClock::new(config.mode == VmMode::Ts1, config.utc_start_dotnet_ticks);
        clock.validate().map_err(invalid)?;
        let state = SimState {
            schema: SIMULATION_SCHEMA,
            mode: config.mode,
            lot_id: config.lot_id,
            authority_epoch: config.authority_epoch,
            completed_tick: 0,
            content: content.descriptor().map_err(RuntimeError::InvalidState)?,
            limits: config.limits,
            clock,
            rng: SimRng::new(config.seed),
            ids: IdAllocator::new(),
            entities: BTreeMap::new(),
            threads: BTreeMap::new(),
            globals: vec![0; 38],
            relationships: RelationshipBook::default(),
            ts1_family_budget: None,
            ts1_inventory: Ts1InventoryBook::default(),
            scheduler: Scheduler::new(0),
            world: WorldState::new(lot),
            effects: EffectBook::new(config.effect_namespace, config.effect_limits)
                .map_err(invalid)?,
            continuations: BTreeMap::new(),
            next_continuation: 1,
            last_tick_digest: None,
            interaction_queues: BTreeMap::new(),
            interaction_access: BTreeMap::new(),
        };
        Self::from_state(state, content, role)
    }
    pub fn from_state(
        state: SimState,
        content: ContentSet,
        role: RuntimeRole,
    ) -> Result<Self, RuntimeError> {
        crate::snapshot::validate_state(&state, &content)
            .map_err(|e| RuntimeError::Snapshot(format!("{e:?}")))?;
        Ok(Self {
            state,
            content,
            role,
        })
    }
    pub fn state(&self) -> &SimState {
        &self.state
    }
    pub fn content(&self) -> &ContentSet {
        &self.content
    }
    pub fn role(&self) -> RuntimeRole {
        self.role
    }
    pub fn state_hash(&self) -> Result<[u8; 32], RuntimeError> {
        state_hash(&self.state)
    }
    pub fn next_tick(&self, commands: Vec<AcceptedCommand>) -> Result<AcceptedTick, RuntimeError> {
        Ok(AcceptedTick {
            lot_id: self.state.lot_id,
            epoch: self.state.authority_epoch,
            tick: self
                .state
                .completed_tick
                .checked_add(1)
                .ok_or(RuntimeError::TickOverflow)?,
            content: self.state.content,
            rng_before: self.state.rng.state(),
            commands,
        })
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, RuntimeError> {
        crate::snapshot::encode(&self.state, &self.content)
            .map_err(|e| RuntimeError::Snapshot(format!("{e:?}")))
    }
    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        let expectation = crate::snapshot::SnapshotExpectation {
            lot_id: self.state.lot_id,
            authority_epoch: self.state.authority_epoch,
            limits: crate::snapshot::SnapshotLimits::default(),
        };
        let restored = crate::snapshot::decode(bytes, &self.content, expectation)
            .map_err(|e| RuntimeError::Snapshot(format!("{e:?}")))?;
        self.state = restored;
        Ok(())
    }
    /// Called only after the external lease/fencing protocol grants this epoch.
    /// Operation IDs and issued epochs survive; retries carry the new delivery epoch.
    pub fn adopt_epoch(&mut self, new_epoch: u64, role: RuntimeRole) -> Result<(), RuntimeError> {
        if new_epoch <= self.state.authority_epoch {
            return Err(RuntimeError::WrongEpoch);
        }
        let mut next = self.state.clone();
        next.authority_epoch = new_epoch;
        crate::snapshot::validate_state(&next, &self.content)
            .map_err(|e| RuntimeError::Snapshot(format!("{e:?}")))?;
        self.state = next;
        self.role = role;
        Ok(())
    }
    pub fn pending_effects(&self) -> Result<Vec<EffectDispatch>, RuntimeError> {
        if self.role == RuntimeRole::Replica {
            return Ok(Vec::new());
        }
        self.state
            .effects
            .redispatch_pending(self.state.authority_epoch)
            .map_err(invalid)
    }
    pub fn step(&mut self, input: &AcceptedTick) -> Result<TickOutcome, RuntimeError> {
        if input.lot_id != self.state.lot_id {
            return Err(RuntimeError::WrongLot);
        }
        if input.epoch != self.state.authority_epoch {
            return Err(RuntimeError::WrongEpoch);
        }
        if input.content != self.state.content {
            return Err(RuntimeError::WrongContent);
        }
        if input.commands.len() > self.state.limits.max_commands_per_tick as usize {
            return Err(invalid("command count limit"));
        }
        let digest =
            hash(&canonical_bytes(input, MAX_TICK_BYTES).map_err(RuntimeError::InvalidCommand)?);
        if input.tick == self.state.completed_tick && self.state.last_tick_digest == Some(digest) {
            return Ok(TickOutcome {
                tick: input.tick,
                duplicate: true,
                state_hash: self.state_hash()?,
                instructions: 0,
                events: Vec::new(),
                effects: Vec::new(),
            });
        }
        if input.tick <= self.state.completed_tick {
            return Err(RuntimeError::ConflictingTick);
        }
        let expected = self
            .state
            .completed_tick
            .checked_add(1)
            .ok_or(RuntimeError::TickOverflow)?;
        if input.tick != expected {
            return Err(RuntimeError::TickGap {
                expected,
                received: input.tick,
            });
        }
        if input.rng_before != self.state.rng.state() {
            return Err(RuntimeError::WrongRng);
        }
        let old_operations: BTreeSet<_> = self
            .state
            .effects
            .pending_requests()
            .map(|r| r.operation_id)
            .collect();
        let mut next = self.state.clone();
        let mut work = TickWork::default();
        for command in &input.commands {
            apply_command(&mut next, &self.content, &mut work, command, input.tick)?;
        }
        refresh_usage_projections(&mut next)?;
        next.scheduler.begin_tick(input.tick).map_err(invalid)?;
        next.clock.advance().map_err(invalid)?;
        next.world.expire_slots(input.tick).map_err(invalid)?;
        while let Some(entity) = next.scheduler.next_entity() {
            if !next.ids.is_live(entity) {
                continue;
            }
            dispatch_entity(&mut next, &self.content, &mut work, entity)?;
        }
        // Source VMScheduler.RunTick mixes count before processing pending deletion.
        next.rng.mix_entity_count(next.entities.len());
        let deleted = next.scheduler.finish_tick().map_err(invalid)?;
        for entity in deleted {
            remove_entity(&mut next, &mut work, entity)?;
        }
        refresh_usage_projections(&mut next)?;
        next.completed_tick = input.tick;
        next.last_tick_digest = Some(digest);
        crate::snapshot::validate_state(&next, &self.content)
            .map_err(|e| RuntimeError::Snapshot(format!("{e:?}")))?;
        let state_hash = state_hash(&next)?;
        let effects = if self.role == RuntimeRole::Authority {
            next.effects
                .pending_requests()
                .filter(|r| !old_operations.contains(&r.operation_id))
                .map(|r| {
                    next.effects
                        .redispatch(r.operation_id, next.authority_epoch)
                        .map_err(invalid)
                })
                .collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };
        self.state = next;
        Ok(TickOutcome {
            tick: input.tick,
            duplicate: false,
            state_hash,
            instructions: work.instructions,
            events: work.events,
            effects,
        })
    }
    /// UI/autonomy probes execute on isolated state and RNG. Any requested effect
    /// remains inside that clone and is never returned as executable work.
    pub fn query_behavior(
        &self,
        entity: EntityRef,
        routine: RoutineKey,
        context: FrameContext,
        args: Vec<i16>,
        budget: u32,
    ) -> Result<QueryOutcome, RuntimeError> {
        live_entity(&self.state, entity)?;
        if budget == 0 || budget > self.state.limits.instruction_budget_per_entity {
            return Err(invalid("query instruction budget"));
        }
        let mut state = self.state.clone();
        let mut work = TickWork::default();
        let mut thread = VmThread::new(entity, state.mode);
        thread.is_check = true;
        if let Some(original) = state.threads.get(&entity.object_id) {
            thread.temps = original.temps;
            thread.temp_xl = original.temp_xl;
        }
        thread.push_entry(self.content.routines(), routine, context, args)?;
        let report = run_synchronous_thread(
            &mut state,
            &self.content,
            &mut work,
            entity,
            0,
            &mut thread,
            budget,
            &mut BTreeMap::new(),
            true,
        )?;
        Ok(QueryOutcome {
            stop: report.stop,
            temps: thread.temps,
            temp_xl: thread.temp_xl,
            instructions: report.instructions.saturating_add(work.instructions),
            diagnostics: thread.diagnostics,
        })
    }
}

fn refresh_usage_projections(state: &mut SimState) -> Result<(), RuntimeError> {
    let mut used = BTreeSet::new();
    for item in state.entities.values() {
        if item.object_data[8] as u16 & (1 << 5) != 0 {
            used.insert(item.info.reference);
        }
    }
    for item in state
        .entities
        .values()
        .filter(|e| e.info.is_avatar && !e.info.dead)
    {
        if let Some(thread) = state.threads.get(&item.info.reference.object_id) {
            for frame in &thread.frames {
                if state.ids.is_live(frame.context.callee) {
                    used.insert(frame.context.callee);
                }
            }
        }
    }
    let mut groups = BTreeSet::new();
    for reference in used {
        let item = live_entity(state, reference)?;
        groups.insert(item.info.base_object);
    }
    let mut changes = Vec::new();
    for item in state.entities.values() {
        let object = state
            .world
            .object(item.info.reference)
            .ok_or_else(|| invalid("missing world projection"))?;
        let in_use = groups.contains(&item.info.base_object);
        let for_sale = !item.info.is_avatar && item.disabled_flags & 2 != 0;
        if object.in_use != in_use || object.for_sale != for_sale {
            let mut candidate = object.clone();
            candidate.in_use = in_use;
            candidate.for_sale = for_sale;
            changes.push(candidate);
        }
    }
    if !changes.is_empty() {
        let mut world = state.world.clone();
        for object in changes {
            world.replace_object(object).map_err(invalid)?;
        }
        state.world = world;
    }
    Ok(())
}

fn state_hash(state: &SimState) -> Result<[u8; 32], RuntimeError> {
    Ok(hash(
        &canonical_bytes(state, 64 * 1024 * 1024).map_err(RuntimeError::InvalidState)?,
    ))
}
#[derive(Clone, Default)]
struct TickWork {
    strict_source: bool,
    thread_controls: BTreeMap<EntityRef, (bool, u32, PrimitiveExit)>,
    detached_threads: BTreeSet<EntityRef>,
    instructions: u32,
    events: Vec<RuntimeEvent>,
    reset_requests: BTreeSet<EntityRef>,
    resetting: BTreeSet<EntityRef>,
    interrupt_requests: BTreeSet<EntityRef>,
    temp_writes: BTreeMap<EntityRef, BTreeMap<u16, i16>>,
    temp_xl_writes: BTreeMap<EntityRef, BTreeMap<u16, i32>>,
}
pub(crate) fn live_entity(state: &SimState, entity: EntityRef) -> Result<&EntityState, VmFault> {
    state
        .entities
        .get(&entity.object_id)
        .filter(|e| e.info.reference == entity && state.ids.is_live(entity))
        .ok_or(VmFault::StaleEntity(entity))
}
pub(crate) fn live_entity_mut(
    state: &mut SimState,
    entity: EntityRef,
) -> Result<&mut EntityState, VmFault> {
    if !state.ids.is_live(entity) {
        return Err(VmFault::StaleEntity(entity));
    }
    state
        .entities
        .get_mut(&entity.object_id)
        .filter(|e| e.info.reference == entity)
        .ok_or(VmFault::StaleEntity(entity))
}
pub(crate) fn avatar_mut(
    state: &mut SimState,
    entity: EntityRef,
) -> Result<&mut AvatarState, VmFault> {
    live_entity_mut(state, entity)?
        .avatar
        .as_mut()
        .ok_or_else(|| host_error("avatar required"))
}
pub(crate) fn vm_position(position: LotPosition) -> Result<VmPosition, VmFault> {
    Ok(VmPosition {
        x: i16::try_from(position.x).map_err(host_error)?,
        y: i16::try_from(position.y).map_err(host_error)?,
        level: i8::try_from(position.level).map_err(host_error)?,
    })
}
pub(crate) fn lot_position(position: VmPosition) -> Result<LotPosition, VmFault> {
    Ok(LotPosition::new(
        i32::from(position.x),
        i32::from(position.y),
        u8::try_from(position.level).map_err(host_error)?,
    ))
}

fn apply_command(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    command: &AcceptedCommand,
    tick: u64,
) -> Result<(), RuntimeError> {
    match command {
        AcceptedCommand::BeginBuild { preview, authority } => {
            build_adapter::begin(state, content, work, preview, authority)?
        }
        AcceptedCommand::CompleteBuild(confirmation) => {
            build_adapter::complete(state, content, work, confirmation)?
        }
        AcceptedCommand::SetInteractionAuthority { actor, access } => {
            live_entity(state, *actor)?;
            match access {
                Some(access) if access.principal.0 != 0 => {
                    state.interaction_access.insert(*actor, *access);
                }
                Some(_) => return Err(invalid("zero interaction principal")),
                None => {
                    state.interaction_access.remove(actor);
                }
            }
        }
        AcceptedCommand::QueueInteraction(intent) => {
            interaction_adapter::enqueue_intent(state, content, work, *intent)?
        }
        AcceptedCommand::CancelInteraction(intent) => {
            interaction_adapter::cancel_intent(state, content, work, *intent)?
        }
        AcceptedCommand::Spawn(spec) => {
            spawn_entity(
                state,
                content,
                work,
                spec.clone(),
                ObjectId::NULL,
                ObjectId::NULL,
                0,
            )?;
        }
        AcceptedCommand::Delete { entity } => {
            live_entity(state, *entity)?;
            state.scheduler.delete_at_end(*entity);
        }
        AcceptedCommand::Interrupt { entity } => {
            live_entity(state, *entity)?;
            if let Some(thread) = state.threads.get_mut(&entity.object_id) {
                thread.interrupt = true;
            }
            state.scheduler.interrupt(*entity).map_err(invalid)?;
        }
        AcceptedCommand::StartBehavior {
            entity,
            routine,
            context,
            args,
            replace,
        } => {
            live_entity(state, *entity)?;
            context.validate()?;
            if *replace {
                cancel_continuations(state, *entity)?;
            }
            let thread = state
                .threads
                .get_mut(&entity.object_id)
                .ok_or_else(|| invalid("missing thread"))?;
            if *replace {
                *thread = VmThread::new(*entity, state.mode);
            } else if !thread.frames.is_empty() {
                return Err(invalid("behavior is already active"));
            }
            thread.push_entry(content.routines(), *routine, context.clone(), args.clone())?;
            let entity_state = live_entity_mut(state, *entity)?;
            entity_state.pending_entrypoints.clear();
            entity_state.lifecycle = LifecyclePhase::Running;
            state.scheduler.schedule(*entity, tick).map_err(invalid)?;
        }
        AcceptedCommand::WriteMemory { address, value } => {
            let write = crate::runtime_memory::write_memory(state, content, address, *value)?;
            memory_signals(state, content, work, write.signals)?;
        }
        AcceptedCommand::SetInteractionProjection {
            entity,
            queued_users,
            active_advertisements,
        } => {
            if queued_users.len() > 32767
                || active_advertisements
                    .as_ref()
                    .is_some_and(|v| v.len() > 196608 || v.keys().any(|(kind, _)| *kind > 2))
            {
                return Err(invalid("interaction projection bounds"));
            }
            for user in queued_users {
                if !live_entity(state, *user)?.info.is_avatar {
                    return Err(invalid("queue user is not an avatar"));
                }
            }
            let item = live_entity_mut(state, *entity)?;
            item.queued_users = queued_users.clone();
            item.active_advertisements = active_advertisements.clone();
        }
        AcceptedCommand::SetEveryFrame { entity, enabled } => {
            let item = live_entity_mut(state, *entity)?;
            item.always_tick = *enabled || item.info.is_avatar;
            if item.always_tick {
                state.scheduler.schedule(*entity, tick).map_err(invalid)?;
            }
        }
        AcceptedCommand::SetObjectStatus {
            entity,
            disabled_flags,
            broken,
        } => {
            let item = live_entity_mut(state, *entity)?;
            item.disabled_flags = *disabled_flags;
            item.broken = *broken;
            if *disabled_flags > 2 {
                state.scheduler.schedule(*entity, tick).map_err(invalid)?;
            }
        }
        AcceptedCommand::BeginRoute {
            entity,
            target,
            goals,
        } => {
            live_entity(state, *entity)?;
            if state
                .threads
                .get(&entity.object_id)
                .is_some_and(|t| !t.frames.is_empty())
                || state.continuations.values().any(|c| c.entity == *entity)
            {
                return Err(invalid("standalone route requires an idle entity"));
            }
            begin_route(state, *entity, *target, goals.clone(), false, false)?;
            state.scheduler.schedule(*entity, tick).map_err(invalid)?;
        }
        AcceptedCommand::RouteCallback {
            continuation_id,
            route_id,
            token,
            outcome,
        } => {
            let continuation = state
                .continuations
                .get_mut(continuation_id)
                .ok_or_else(|| invalid("route continuation missing"))?;
            let ContinuationKind::Route(route) = &mut continuation.kind else {
                return Err(invalid("continuation is not a route"));
            };
            let stand = route
                .pending_callback()
                .filter(|call| {
                    call.route_id == *route_id
                        && call.token == *token
                        && matches!(call.kind, world::routing::RouteCallbackKind::Stand)
                })
                .map(|call| (call.actor, call.target));
            route
                .complete_callback_for(*route_id, *token, outcome.clone())
                .map_err(invalid)?;
            if let Some((actor, chair)) = stand {
                if outcome.success
                    && state.ids.is_live(chair)
                    && outcome
                        .position
                        .map_or(true, |p| state.world.lot.contains_position(p))
                {
                    detach(state, actor)?;
                    if let Some(avatar) = live_entity_mut(state, actor)?.avatar.as_mut() {
                        if avatar.person_data.values[0] == 1 {
                            avatar.write_person_data(0, 0).map_err(invalid)?;
                        }
                    }
                    crate::runtime_memory::sync_projection(state, actor)?;
                }
            }
        }
        AcceptedCommand::DefineSlot(definition) => {
            live_entity(state, definition.key.owner)?;
            state
                .world
                .define_slot(definition.clone())
                .map_err(invalid)?;
        }
        AcceptedCommand::ReserveSlot(request) => {
            if request.tick != tick {
                return Err(invalid("slot reservation tick mismatch"));
            }
            let token = state.world.reserve_slot(request.clone()).map_err(invalid)?;
            work.events.push(RuntimeEvent::SlotReserved(token));
        }
        AcceptedCommand::OccupySlot(token) => {
            state.world.occupy_slot(*token, tick).map_err(invalid)?;
        }
        AcceptedCommand::ReleaseSlot(token) => {
            state.world.slots.release(*token).map_err(invalid)?;
        }
        AcceptedCommand::EffectResolved(response) => resolve_effect(state, response, tick)?,
        AcceptedCommand::Disconnect {
            entity,
            session_epoch,
        } => {
            avatar_mut(state, *entity)?
                .lifecycle
                .disconnect(*session_epoch, tick)
                .map_err(invalid)?;
        }
        AcceptedCommand::Reconnect {
            entity,
            session_epoch,
        } => {
            avatar_mut(state, *entity)?
                .lifecycle
                .reconnect(*session_epoch)
                .map_err(invalid)?;
        }
        AcceptedCommand::Reset { entity } => {
            live_entity(state, *entity)?;
            reset_entity(state, content, work, *entity)?;
        }
        AcceptedCommand::SetTs1FamilyBudget(budget) => {
            if state.mode != VmMode::Ts1 {
                return Err(invalid("TS1 family projection in TSO"));
            }
            state.ts1_family_budget = *budget;
        }
        AcceptedCommand::SetTs1Inventory { neighbor, items } => {
            if state.mode != VmMode::Ts1 {
                return Err(invalid("TS1 inventory projection in TSO"));
            }
            state.ts1_inventory.write(*neighbor, items.clone())?;
        }
    }
    Ok(())
}

fn spawn_entity(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    spec: SpawnSpec,
    main_parameter: ObjectId,
    main_stack: ObjectId,
    depth: u16,
) -> Result<EntityRef, RuntimeError> {
    spawn_entity_impl(
        state,
        content,
        work,
        spec,
        main_parameter,
        main_stack,
        depth,
        None,
    )
}

// Keep source creation context explicit, including the independently reserved durable entity ID.
#[allow(clippy::too_many_arguments)]
fn spawn_entity_impl(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    spec: SpawnSpec,
    main_parameter: ObjectId,
    main_stack: ObjectId,
    depth: u16,
    committed: Option<EntityRef>,
) -> Result<EntityRef, RuntimeError> {
    if depth > MAX_CREATION_DEPTH {
        return Err(RuntimeError::CreationDepth);
    }
    if state.entities.len() >= state.limits.max_entities as usize {
        return Err(invalid("entity capacity"));
    }
    let definition = content
        .object(spec.guid)
        .ok_or_else(|| invalid("object GUID is not in content"))?
        .clone();
    if !spec.facing.valid()
        || (!spec.position.is_out_of_world() && !state.world.lot.contains_position(spec.position))
    {
        return Err(invalid("spawn position or facing"));
    }
    if spec.persistent_id.0 != 0
        && state
            .entities
            .values()
            .any(|e| e.info.persistent_id == spec.persistent_id.0)
    {
        return Err(invalid("persistent ID already in lot"));
    }
    if spec.avatar && state.mode == VmMode::Tso && content.tuning().tso_motives.is_none() {
        return Err(invalid("TSO avatar requires source motive tuning"));
    }
    let entity = if let Some(entity) = committed {
        state.ids.allocate_reserved(entity).map_err(invalid)?;
        entity
    } else {
        state.ids.allocate().map_err(invalid)?
    };
    let avatar = if spec.avatar {
        let mut a = AvatarState::new(
            entity,
            spec.persistent_id,
            if state.mode == VmMode::Ts1 {
                AvatarPlatform::Ts1
            } else {
                AvatarPlatform::Tso
            },
        );
        a.object_guid = spec.guid;
        crate::runtime_avatars::initialize_walk_animations(&mut a, spec.guid, content)?;
        a.motives.limits = content.tuning().motive_limits;
        if state.mode == VmMode::Tso {
            a.decay = MotiveDecay::tso(content.tuning().tso_motives.clone());
        }
        Some(a)
    } else {
        None
    };
    let info = EntityInfo {
        reference: entity,
        guid: spec.guid,
        master_guid: definition.master_guid,
        semiglobal: content.routines().semiglobal(spec.guid),
        persistent_id: spec.persistent_id.0,
        is_avatar: spec.avatar,
        dead: false,
        category: definition.object_data[59],
        family: definition.family,
        position: vm_position(spec.position)?,
        direction: spec.facing.0,
        level_offset: definition.level_offset,
        base_object: entity.object_id,
        multi_tile: false,
        group: vec![entity.object_id],
    };
    let mut object = WorldObject::new(entity, spec.position);
    object.facing = spec.facing;
    object.footprint = definition.footprint.clone();
    object.rules = definition.placement_rules.clone();
    object.rules.is_avatar = spec.avatar;
    object.entrypoints = definition.entry_points.keys().copied().collect();
    object.motion = if spec.avatar {
        world::ObstacleMotion::StationaryAvatar
    } else {
        world::ObstacleMotion::Static
    };
    if committed.is_some() {
        let existing = state
            .world
            .object(entity)
            .ok_or_else(|| invalid("committed purchase has no world object"))?;
        if existing.position != object.position
            || existing.facing != object.facing
            || existing.footprint != object.footprint
            || existing.entrypoints != object.entrypoints
        {
            return Err(invalid(
                "committed purchase content differs from prepared object",
            ));
        }
    } else {
        state.world.insert_object(object).map_err(invalid)?;
    }
    state.entities.insert(
        entity.object_id,
        EntityState {
            info,
            attributes: definition.attributes,
            object_data: definition.object_data,
            list: Vec::new(),
            dynamic_sprite_flags: vec![false; 128],
            type_attributes: BTreeMap::new(),
            tuning_overrides: BTreeMap::new(),
            active_advertisements: None,
            slots: vec![
                None;
                if spec.avatar {
                    3
                } else {
                    usize::from(definition.slot_count)
                }
            ],
            container: None,
            initial_price: 0,
            lockout_started: 0,
            avatar,
            lifecycle: LifecyclePhase::Initializing,
            pending_entrypoints: VecDeque::from([0, 8, 1]),
            main_parameter,
            main_stack_object: main_stack,
            queued_users: BTreeSet::new(),
            always_tick: spec.avatar,
            disabled_flags: 0,
            broken: false,
            headline: None,
            revision: 0,
        },
    );
    state
        .threads
        .insert(entity.object_id, VmThread::new(entity, state.mode));
    state.interaction_queues.insert(
        entity.object_id,
        interaction_adapter::new_queue(entity, state.mode)?,
    );
    state
        .scheduler
        .schedule_in(entity, 1, spec.avatar)
        .map_err(invalid)?;
    crate::runtime_memory::sync_projection(state, entity)?;
    work.events.push(RuntimeEvent::Spawned(entity));
    // Source init and dynamic-multitile entrypoints run immediately; main is
    // pushed for the next scheduled tick. A yielding init preserves its stage.
    run_initialization(state, content, work, entity, depth)?;
    Ok(entity)
}

fn entry_context(state: &SimState, entity: EntityRef, main: bool) -> Result<FrameContext, VmFault> {
    let item = live_entity(state, entity)?;
    let mut context = FrameContext::for_entity(entity, item.info.guid);
    context.stack_object = ObjectId::NULL;
    context.stack_object_ref = None;
    if main && item.main_stack_object != ObjectId::NULL {
        if let Ok(reference) = state.ids.resolve(item.main_stack_object) {
            context.stack_object = reference.object_id;
            context.stack_object_ref = Some(reference);
        }
    }
    Ok(context)
}
fn entry_args(
    content: &ContentSet,
    key: RoutineKey,
    parameter: ObjectId,
) -> Result<Vec<i16>, VmFault> {
    let routine = content
        .routines()
        .get(key)
        .ok_or_else(|| host_error("entry routine missing"))?;
    let mut args = vec![0; usize::from(routine.arguments()).max(4)];
    args[0] = parameter.0;
    Ok(args)
}
fn run_initialization(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    entity: EntityRef,
    depth: u16,
) -> Result<(), RuntimeError> {
    loop {
        if state
            .threads
            .get(&entity.object_id)
            .is_some_and(|t| !t.frames.is_empty())
        {
            return Ok(());
        }
        let stage = live_entity_mut(state, entity)?
            .pending_entrypoints
            .pop_front();
        let Some(stage) = stage else {
            live_entity_mut(state, entity)?.lifecycle = LifecyclePhase::Running;
            return Ok(());
        };
        let item = live_entity(state, entity)?;
        let definition = content
            .object(item.info.guid)
            .ok_or_else(|| invalid("missing definition"))?;
        let key = definition.entry_points.get(&stage).copied();
        let context = entry_context(state, entity, stage == 1)?;
        let parameter = if stage == 1 {
            item.main_parameter
        } else {
            ObjectId::NULL
        };
        if stage == 1 {
            let item = live_entity_mut(state, entity)?;
            item.main_parameter = ObjectId::NULL;
            item.main_stack_object = ObjectId::NULL;
        }
        let Some(key) = key else {
            continue;
        };
        let args = entry_args(content, key, parameter)?;
        state
            .threads
            .get_mut(&entity.object_id)
            .ok_or_else(|| invalid("missing thread"))?
            .push_entry(content.routines(), key, context, args)?;
        if stage == 1 {
            live_entity_mut(state, entity)?.lifecycle = LifecyclePhase::Running;
            return Ok(());
        }
        run_thread(state, content, work, entity, depth)?;
        if matches!(state.threads[&entity.object_id].stop, VmStop::Faulted(_)) {
            return Ok(());
        }
    }
}

fn run_thread(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    entity: EntityRef,
    depth: u16,
) -> Result<(), RuntimeError> {
    work.detached_threads.insert(entity);
    let mut thread = state
        .threads
        .remove(&entity.object_id)
        .ok_or_else(|| invalid("missing thread"))?;
    if work.interrupt_requests.remove(&entity) {
        thread.interrupt = true;
    }
    if let Some(writes) = work.temp_writes.remove(&entity) {
        for (index, value) in writes {
            thread.temps[usize::from(index)] = value;
        }
    }
    if let Some(writes) = work.temp_xl_writes.remove(&entity) {
        for (index, value) in writes {
            thread.temp_xl[usize::from(index)] = value;
        }
    }
    if let Some((interrupt, idle, last_exit)) = work.thread_controls.remove(&entity) {
        thread.interrupt = interrupt;
        thread.schedule_idle_start = idle;
        thread.last_exit = last_exit;
    }
    let remaining = state
        .limits
        .max_tick_instructions
        .checked_sub(work.instructions)
        .ok_or(RuntimeError::InstructionLimit)?;
    if remaining == 0 {
        return Err(RuntimeError::InstructionLimit);
    }
    let budget = state.limits.instruction_budget_per_entity.min(remaining);
    let nested_start = work.instructions;
    let mut host = RuntimeHost::new(state, content, work, entity, depth);
    host.dispatch_budget = budget;
    let report = thread.run(content.routines(), &mut host, budget);
    let dispatch_spent = report
        .instructions
        .saturating_add(work.instructions.saturating_sub(nested_start));
    work.instructions = work
        .instructions
        .checked_add(report.instructions)
        .ok_or(RuntimeError::InstructionLimit)?;
    if work.instructions > state.limits.max_tick_instructions {
        return Err(RuntimeError::InstructionLimit);
    }
    if report.stop == VmStop::BudgetExhausted || dispatch_spent > budget {
        if remaining < state.limits.instruction_budget_per_entity {
            return Err(RuntimeError::InstructionLimit);
        }
        thread.stop = VmStop::Faulted(VmFault::InvalidContent(
            "source dispatch instruction limit exceeded".into(),
        ));
    }
    if let VmStop::Faulted(fault) = &thread.stop {
        live_entity_mut(state, entity)?.lifecycle = LifecyclePhase::Faulted;
        work.events.push(RuntimeEvent::ThreadFault {
            entity,
            fault: fault.clone(),
        });
    }
    if work.interrupt_requests.remove(&entity) {
        thread.interrupt = true;
    }
    if let Some(writes) = work.temp_writes.remove(&entity) {
        for (index, value) in writes {
            thread.temps[usize::from(index)] = value;
        }
    }
    if let Some(writes) = work.temp_xl_writes.remove(&entity) {
        for (index, value) in writes {
            thread.temp_xl[usize::from(index)] = value;
        }
    }
    if let Some((interrupt, idle, last_exit)) = work.thread_controls.remove(&entity) {
        thread.interrupt = interrupt;
        thread.schedule_idle_start = idle;
        thread.last_exit = last_exit;
    }
    interaction_adapter::finish_faulted(state, work, entity, &thread)?;
    state.threads.insert(entity.object_id, thread);
    work.detached_threads.remove(&entity);
    if work.reset_requests.remove(&entity) {
        reset_entity(state, content, work, entity)?;
    }
    refresh_usage_projections(state)?;
    Ok(())
}

fn dispatch_entity(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    entity: EntityRef,
) -> Result<(), RuntimeError> {
    let route_id = state.continuations.iter().find_map(|(id, c)| {
        if c.entity == entity && matches!(c.kind, ContinuationKind::Route(_)) {
            Some(*id)
        } else {
            None
        }
    });
    if let Some(id) = route_id {
        advance_route(state, work, id)?;
    }
    let has_route = state
        .continuations
        .values()
        .any(|c| c.entity == entity && matches!(c.kind, ContinuationKind::Route(_)));
    if !has_route && live_entity(state, entity)?.lifecycle != LifecyclePhase::Faulted {
        let immediate_only = !state.threads[&entity.object_id].frames.is_empty();
        interaction_adapter::poll_queue(state, content, work, entity, immediate_only)?;
        if state.threads[&entity.object_id].frames.is_empty() {
            if !live_entity(state, entity)?.pending_entrypoints.is_empty() {
                run_initialization(state, content, work, entity, 0)?;
            } else {
                let item = live_entity(state, entity)?;
                if let Some(key) = content
                    .object(item.info.guid)
                    .and_then(|d| d.entry_points.get(&1))
                    .copied()
                {
                    let context = entry_context(state, entity, false)?;
                    let args = entry_args(content, key, ObjectId::NULL)?;
                    state
                        .threads
                        .get_mut(&entity.object_id)
                        .expect("live thread")
                        .push_entry(content.routines(), key, context, args)?;
                }
            }
        }
        state
            .threads
            .get_mut(&entity.object_id)
            .expect("live thread")
            .wake();
        run_thread(state, content, work, entity, 0)?;
    }
    let scheduled_every_frame = {
        let item = live_entity(state, entity)?;
        item.always_tick
            || item.info.is_avatar
            || item.headline.is_some()
            || item.disabled_flags > 2
    };
    if let Some(headline) = live_entity_mut(state, entity)?.headline.as_mut() {
        headline.anim = headline.anim.wrapping_add(1);
        if headline.duration >= 0 {
            headline.duration -= 1;
            if headline.duration <= 0 {
                live_entity_mut(state, entity)?.headline = None;
            }
        }
    }
    tick_avatar(state, content, work, entity)?;
    if state.scheduler.is_pending_deletion(entity) {
        return Ok(());
    }
    let every_frame = scheduled_every_frame;
    let thread = &state.threads[&entity.object_id];
    let route_pending = state
        .continuations
        .values()
        .any(|c| c.entity == entity && matches!(c.kind, ContinuationKind::Route(_)));
    let delay = if route_pending {
        1
    } else {
        match thread.stop {
            VmStop::Sleeping { until_tick } => until_tick
                .wrapping_sub(state.scheduler.current_tick() as u32)
                .max(1),
            VmStop::Waiting { .. } if !every_frame => return Ok(()),
            VmStop::Faulted(_) if !every_frame => return Ok(()),
            _ => 1,
        }
    };
    state
        .scheduler
        .schedule_in(entity, delay, every_frame)
        .map_err(invalid)?;
    Ok(())
}

fn tick_avatar(
    state: &mut SimState,
    _content: &ContentSet,
    work: &mut TickWork,
    entity: EntityRef,
) -> Result<(), RuntimeError> {
    let item = live_entity(state, entity)?;
    let Some(avatar) = item.avatar.as_ref() else {
        return Ok(());
    };
    let head_target = state
        .ids
        .resolve(ObjectId(avatar.person_data.values[41]))
        .ok();
    let context = AvatarTickContext {
        minute: state.clock.minutes,
        hour: state.clock.hours,
        room_score: 100,
        category: avatar.lot_category,
        has_thread: true,
        thread_paused: false,
        hidden: item.object_data[34],
        out_of_world: lot_position(item.info.position)?.is_out_of_world(),
        container_out_of_world: item
            .container
            .and_then(|(r, _)| state.entities.get(&r.object_id))
            .map_or(true, |e| {
                e.info.position.x == i16::MIN && e.info.position.y == i16::MIN
            }),
        leave_action_available: false,
        leave_action_already_queued: false,
    };
    let avatar = avatar_mut(state, entity)?;
    avatar
        .resolve_head_seek_target(head_target)
        .map_err(invalid)?;
    let output = avatar.tick(&context).map_err(invalid)?;
    for request in &output.lifecycle_requests {
        match request {
            crate::avatars::lifecycle::LifecycleRequest::Delete { .. } => {
                state.scheduler.delete_at_end(entity)
            }
            crate::avatars::lifecycle::LifecycleRequest::SetHidden(value) => {
                live_entity_mut(state, entity)?.object_data[34] = *value
            }
            crate::avatars::lifecycle::LifecycleRequest::SetDisplayFlags(value) => {
                avatar_mut(state, entity)?.display_flags = *value
            }
            _ => {}
        }
    }
    if !output.animation_cues.is_empty()
        || !output.lifecycle_requests.is_empty()
        || output.message_cleared
    {
        work.events.push(RuntimeEvent::Avatar { entity, output });
    }
    Ok(())
}

fn next_continuation(state: &mut SimState) -> Result<u64, RuntimeError> {
    if state.continuations.len() >= state.limits.max_continuations as usize {
        return Err(RuntimeError::ContinuationLimit);
    }
    let id = state.next_continuation;
    state.next_continuation = id.checked_add(1).ok_or(RuntimeError::ContinuationLimit)?;
    Ok(id)
}
pub(crate) fn begin_route(
    state: &mut SimState,
    entity: EntityRef,
    target: Option<EntityRef>,
    goals: Vec<RouteGoal>,
    resumes_vm: bool,
    failure_tree: bool,
) -> Result<u64, RuntimeError> {
    let start = lot_position(live_entity(state, entity)?.info.position)?;
    if let Some(target) = target {
        live_entity(state, target)?;
    }
    let id = next_continuation(state)?;
    let mut request = world::RouteRequest::new(id, entity, start, goals);
    request.target = target;
    request.seated_on = live_entity(state, entity)?
        .container
        .map(|(owner, _)| owner);
    request.config.call_failure_tree = failure_tree;
    let route = RouteContinuation::new(request, &state.world).map_err(invalid)?;
    state.continuations.insert(
        id,
        RuntimeContinuation {
            id,
            entity,
            kind: ContinuationKind::Route(route),
            resumes_vm,
        },
    );
    Ok(id)
}
fn advance_route(state: &mut SimState, work: &mut TickWork, id: u64) -> Result<(), RuntimeError> {
    let mut continuation = state
        .continuations
        .remove(&id)
        .ok_or_else(|| invalid("route missing"))?;
    let ContinuationKind::Route(route) = &mut continuation.kind else {
        return Err(invalid("not a route"));
    };
    let before = route.clone();
    let old_callback = route.pending_callback().cloned();
    let step = route.step(
        &state.world,
        &mut state.rng,
        state
            .limits
            .route_search_budget
            .min(world::routing::MAX_DISPATCH_BUDGET),
    );
    let mut completion = None;
    match step {
        RouteStep::Progress { to, facing, .. } => {
            match move_entity(state, continuation.entity, to, facing) {
                Ok(()) => {}
                Err(RuntimeError::InvalidCommand(message)) if message.contains("BuildLocked") => {
                    *route = before;
                }
                Err(error) => return Err(error),
            }
        }
        RouteStep::Script(callback) => {
            if let world::routing::RouteCallbackKind::Failure { code, .. } = callback.kind {
                if let Some(avatar) = live_entity_mut(state, continuation.entity)?.avatar.as_mut() {
                    avatar.write_person_data(62, code.code()).map_err(invalid)?;
                }
            }
            if old_callback.as_ref() != Some(&callback) {
                work.events.push(RuntimeEvent::RouteScript {
                    continuation_id: id,
                    callback,
                });
            }
        }
        RouteStep::Arrived {
            position,
            goal_index,
            ..
        } => {
            let facing = route
                .request()
                .goals
                .get(usize::from(goal_index))
                .and_then(|g| g.facing)
                .unwrap_or(Facing(
                    live_entity(state, continuation.entity)?.info.direction,
                ));
            move_entity(state, continuation.entity, position, facing)?;
            completion = Some((true, world::RouteFailCode::Success, None));
        }
        RouteStep::Failed { code, blocker } => completion = Some((false, code, blocker)),
        _ => {}
    }
    if let Some((success, code, blocker)) = completion {
        if !success {
            if let Some(avatar) = live_entity_mut(state, continuation.entity)?.avatar.as_mut() {
                avatar.write_person_data(62, code.code()).map_err(invalid)?;
            }
        }
        if continuation.resumes_vm {
            let resolution = VmResolution::complete(PrimitiveExit::branch(success));
            state
                .threads
                .get_mut(&continuation.entity.object_id)
                .ok_or_else(|| invalid("route thread missing"))?
                .resume(id, resolution)?;
        }
        work.events.push(RuntimeEvent::RouteFinished {
            continuation_id: id,
            entity: continuation.entity,
            success,
            code,
            blocker,
        });
    } else {
        state.continuations.insert(id, continuation);
    }
    Ok(())
}

fn validate_resolution(thread: &VmThread, resolution: &VmResolution) -> Result<(), RuntimeError> {
    if !matches!(
        resolution.response,
        HostResponse::Complete(
            PrimitiveExit::GotoTrue
                | PrimitiveExit::GotoFalse
                | PrimitiveExit::GotoTrueNextTick
                | PrimitiveExit::GotoFalseNextTick
        )
    ) {
        return Err(invalid("external result must select a true/false branch"));
    }
    if resolution.writes.len() > 1024 {
        return Err(invalid("too many external register writes"));
    }
    let frame = thread.top()?;
    for write in &resolution.writes {
        let len = match write.target {
            RegisterTarget::Temp => 20,
            RegisterTarget::TempXl => 2,
            RegisterTarget::Local => frame.locals.len(),
            RegisterTarget::Parameter => frame.args.len(),
            RegisterTarget::StackObjectId => 1,
        };
        if usize::from(write.index) >= len {
            return Err(invalid("external register index"));
        }
    }
    Ok(())
}
/// Canonical bytes for a public, typed VM effect completion.
pub fn encode_effect_resolution(resolution: &VmResolution) -> Result<Vec<u8>, RuntimeError> {
    canonical_bytes(resolution, 65536).map_err(RuntimeError::InvalidCommand)
}
fn resolve_effect(
    state: &mut SimState,
    response: &EffectResolved,
    tick: u64,
) -> Result<(), RuntimeError> {
    let live = state.ids.resolve(response.target.object_id).ok();
    let id = state.continuations.iter().find_map(|(id, c)| {
        if c.kind == ContinuationKind::Effect(response.operation_id) {
            Some(*id)
        } else {
            None
        }
    });
    let resolution = if let Some(id) = id {
        let continuation = &state.continuations[&id];
        if continuation.entity != response.target {
            return Err(invalid("effect continuation target"));
        }
        let EffectValue::Bytes(bytes) = &response.value else {
            return Err(invalid("VM effect response requires Bytes"));
        };
        if bytes.len() > state.effects.limits().max_response_bytes as usize {
            return Err(invalid("effect response too large"));
        }
        let value: VmResolution = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_little_endian()
            .with_limit(u64::from(state.effects.limits().max_response_bytes))
            .reject_trailing_bytes()
            .deserialize(bytes)
            .map_err(invalid)?;
        if canonical_bytes(&value, 65536).map_err(RuntimeError::InvalidCommand)? != *bytes {
            return Err(invalid("noncanonical effect response"));
        }
        validate_resolution(
            state
                .threads
                .get(&response.target.object_id)
                .ok_or_else(|| invalid("effect thread missing"))?,
            &value,
        )?;
        Some((id, value))
    } else {
        None
    };
    match state
        .effects
        .resolve(response.clone(), tick, state.authority_epoch, live)
        .map_err(invalid)?
    {
        EffectResolution::Duplicate => {}
        EffectResolution::Applied { .. } => {
            let (id, value) = resolution.ok_or_else(|| invalid("effect continuation missing"))?;
            state
                .threads
                .get_mut(&response.target.object_id)
                .expect("validated")
                .resume(id, value)?;
            state.continuations.remove(&id);
            state
                .scheduler
                .schedule(response.target, tick)
                .map_err(invalid)?;
        }
    }
    Ok(())
}

fn cancel_continuations(state: &mut SimState, entity: EntityRef) -> Result<(), RuntimeError> {
    state
        .effects
        .cancel_target(
            entity,
            state.scheduler.current_tick(),
            state.authority_epoch,
        )
        .map_err(invalid)?;
    state.continuations.retain(|_, c| c.entity != entity);
    Ok(())
}
fn remove_entity(
    state: &mut SimState,
    work: &mut TickWork,
    entity: EntityRef,
) -> Result<(), RuntimeError> {
    if !state.ids.is_live(entity) {
        return Ok(());
    }
    cancel_continuations(state, entity)?;
    let contained: Vec<_> = state
        .entities
        .values()
        .filter(|e| e.container.is_some_and(|(owner, _)| owner == entity))
        .map(|e| e.info.reference)
        .collect();
    for child in contained {
        detach(state, child)?;
        move_entity(
            state,
            child,
            LotPosition::OUT_OF_WORLD,
            Facing(live_entity(state, child)?.info.direction),
        )?;
    }
    detach(state, entity)?;
    state.world.remove_object(entity).map_err(invalid)?;
    state.scheduler.cancel(entity);
    state.threads.remove(&entity.object_id);
    state.interaction_queues.remove(&entity.object_id);
    state.interaction_access.remove(&entity);
    state.entities.remove(&entity.object_id);
    state.ids.release(entity).map_err(invalid)?;
    state.relationships.delete_entity(entity);
    let mut group_changes = Vec::new();
    for item in state.entities.values_mut() {
        item.queued_users.remove(&entity);
        if let Some(avatar) = item.avatar.as_mut() {
            avatar.relationships.remove_local(entity);
        }
        let was_member = item.info.group.contains(&entity.object_id)
            || item.info.base_object == entity.object_id;
        item.info.group.retain(|id| *id != entity.object_id);
        if item.info.base_object == entity.object_id {
            item.info.base_object = *item
                .info
                .group
                .iter()
                .min()
                .unwrap_or(&item.info.reference.object_id);
        }
        if was_member {
            group_changes.push(item.info.reference);
        }
    }
    for reference in group_changes {
        crate::runtime_memory::sync_projection(state, reference)?;
    }
    for continuation in state.continuations.values_mut() {
        if let ContinuationKind::Route(route) = &mut continuation.kind {
            if route.target() == Some(entity) {
                route.cancel();
            }
        }
    }
    work.events.push(RuntimeEvent::Deleted(entity));
    Ok(())
}
pub(crate) fn detach(state: &mut SimState, entity: EntityRef) -> Result<(), RuntimeError> {
    let container = live_entity(state, entity)?.container;
    if let Some((owner, index)) = container {
        if let Some(parent) = state
            .entities
            .get_mut(&owner.object_id)
            .filter(|e| e.info.reference == owner)
        {
            if parent.slots.get(usize::from(index)) == Some(&Some(entity)) {
                parent.slots[usize::from(index)] = None;
                if let Some(avatar) = parent.avatar.as_mut() {
                    avatar.animations.carry = None;
                }
            }
        }
        live_entity_mut(state, entity)?.container = None;
    }
    Ok(())
}
pub(crate) fn move_entity(
    state: &mut SimState,
    entity: EntityRef,
    position: LotPosition,
    facing: Facing,
) -> Result<(), RuntimeError> {
    live_entity(state, entity)?;
    let vm_pos = vm_position(position)?;
    let mut descendants = vec![entity];
    let mut at = 0;
    while at < descendants.len() {
        let parent = descendants[at];
        for child in live_entity(state, parent)?.slots.iter().flatten() {
            if descendants.contains(child) {
                return Err(invalid("container cycle"));
            }
            descendants.push(*child);
        }
        at += 1;
    }
    let mut candidates = Vec::with_capacity(descendants.len());
    let room = state
        .world
        .lot
        .room_at(position)
        .map_or(-1, |room| room.0.wrapping_sub(1) as i16);
    for item in &descendants {
        let mut e = live_entity(state, *item)?.clone();
        e.revision = e
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("entity revision overflow"))?;
        e.info.position = vm_pos;
        e.object_data[29] = room;
        if *item == entity {
            e.info.direction = facing.0;
        }
        candidates.push(e);
    }
    let mut world = state.world.clone();
    for e in &candidates {
        world
            .move_object(e.info.reference, position, Facing(e.info.direction))
            .map_err(invalid)?;
    }
    state.world = world;
    for e in candidates {
        state.entities.insert(e.info.reference.object_id, e);
    }
    Ok(())
}
pub(crate) fn place_in_slot(
    state: &mut SimState,
    container: EntityRef,
    object: Option<EntityRef>,
    slot: i16,
    clean_old: bool,
) -> Result<bool, RuntimeError> {
    let owner = live_entity(state, container)?;
    let index = usize::try_from(slot).map_err(invalid)?;
    let previous = *owner
        .slots
        .get(index)
        .ok_or_else(|| invalid("container slot index"))?;
    if previous == object {
        return Ok(true);
    }
    if previous.is_some() {
        return Ok(false);
    }
    let _ = clean_old; // Only source-position cleanup, never destination replacement.
    if let Some(object) = object {
        live_entity(state, object)?;
        let mut next = Some(container);
        let mut visited = BTreeSet::new();
        while let Some(parent) = next {
            if parent == object || !visited.insert(parent) {
                return Ok(false);
            }
            next = live_entity(state, parent)?.container.map(|c| c.0);
        }
    }
    if let Some(old) = previous {
        detach(state, old)?;
        crate::runtime_memory::sync_projection(state, old)?;
    }
    if let Some(object) = object {
        detach(state, object)?;
        let position = lot_position(live_entity(state, container)?.info.position)?;
        let direction = Facing(live_entity(state, object)?.info.direction);
        move_entity(state, object, position, direction)?;
        live_entity_mut(state, object)?.container = Some((container, index as u16));
        crate::runtime_memory::sync_projection(state, object)?;
    }
    live_entity_mut(state, container)?.slots[index] = object;
    Ok(true)
}
fn reset_entity(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    entity: EntityRef,
) -> Result<(), RuntimeError> {
    if !work.resetting.insert(entity) {
        return Ok(());
    }
    cancel_continuations(state, entity)?;
    let prior = state
        .threads
        .get(&entity.object_id)
        .ok_or_else(|| invalid("reset thread missing"))?;
    let mut replacement = VmThread::new(entity, state.mode);
    replacement.temps = prior.temps;
    replacement.temp_xl = prior.temp_xl;
    replacement.interrupt = prior.interrupt;
    state.threads.insert(entity.object_id, replacement);
    let item = live_entity_mut(state, entity)?;
    item.active_advertisements = None;
    item.pending_entrypoints.clear();
    item.lifecycle = LifecyclePhase::Running;
    let runs_reset = item.avatar.as_ref().map_or(true, AvatarState::is_pet);
    if runs_reset {
        if let Some(key) = content
            .object(item.info.guid)
            .and_then(|d| d.entry_points.get(&3))
            .copied()
        {
            let context = entry_context(state, entity, false)?;
            let thread = &state.threads[&entity.object_id];
            let request = RoutineCheck {
                routine: BoundRoutine {
                    routine: key,
                    code_owner: context.code_owner,
                    arguments: content
                        .routines()
                        .get(key)
                        .expect("validated content")
                        .arguments(),
                },
                context,
                args: vec![0; 4],
                temps: thread.temps,
                temp_xl: thread.temp_xl,
                run_in_owner: None,
                use_current_thread: false,
            };
            let mut host = RuntimeHost::new(state, content, work, entity, 0);
            let result = host.evaluate_check(request)?;
            if state.scheduler.is_running() {
                let thread = state
                    .threads
                    .get_mut(&entity.object_id)
                    .expect("reset thread");
                thread.temps = result.temps;
                thread.temp_xl = result.temp_xl;
            }
        }
    }
    let item = live_entity_mut(state, entity)?;
    if let Some(avatar) = item.avatar.as_mut() {
        let requests = avatar.reset();
        item.headline = None;
        work.events.push(RuntimeEvent::Avatar {
            entity,
            output: AvatarTickOutput {
                lifecycle_requests: requests,
                ..AvatarTickOutput::default()
            },
        });
    }
    if let Some(key) = content
        .object(item.info.guid)
        .and_then(|d| d.entry_points.get(&1))
        .copied()
    {
        let context = entry_context(state, entity, false)?;
        let args = entry_args(content, key, ObjectId::NULL)?;
        state
            .threads
            .get_mut(&entity.object_id)
            .expect("reset thread")
            .push_entry(content.routines(), key, context, args)?;
    }
    for item in state.entities.values_mut() {
        item.queued_users.remove(&entity);
    }
    state
        .scheduler
        .schedule_in(entity, 1, true)
        .map_err(invalid)?;
    work.resetting.remove(&entity);
    Ok(())
}
fn memory_signals(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    signals: Vec<crate::runtime_memory::MemorySignal>,
) -> Result<(), RuntimeError> {
    for signal in signals {
        if let crate::runtime_memory::MemorySignal::MoneyHeadline { entity, amount } = &signal {
            live_entity_mut(state, *entity)?.headline = Some(HeadlineState {
                duration: 60,
                anim: 0,
                kind: HeadlineKind::Money(*amount),
            });
        }
        if let crate::runtime_memory::MemorySignal::OutfitRequest { entity, suit } = &signal {
            crate::runtime_avatars::apply_person_outfit(state, content, *entity, *suit)?;
        }
        if let crate::runtime_memory::MemorySignal::ResetRequested { entity } = &signal {
            if work.resetting.contains(entity) {
                continue;
            }
            if work.detached_threads.contains(entity)
                || !state.threads.contains_key(&entity.object_id)
            {
                work.reset_requests.insert(*entity);
            } else {
                reset_entity(state, content, work, *entity)?;
            }
        }
        work.events.push(RuntimeEvent::Memory(signal));
    }
    Ok(())
}

fn validate_relationship_key(state: &SimState, key: RelationshipKey) -> Result<(), VmFault> {
    if let RelationshipOwner::Entity(owner) = key.owner {
        live_entity(state, owner)?;
    }
    if let RelationshipTarget::Local(target) = key.target {
        live_entity(state, target)?;
    }
    Ok(())
}
fn avatar_relationship_target(
    target: RelationshipTarget,
) -> crate::avatars::social::RelationshipTarget {
    match target {
        RelationshipTarget::Local(entity) => {
            crate::avatars::social::RelationshipTarget::Local(entity)
        }
        RelationshipTarget::Persistent(id) => {
            crate::avatars::social::RelationshipTarget::Persistent(PersistentId(id))
        }
        RelationshipTarget::Neighbor(id) => {
            crate::avatars::social::RelationshipTarget::Neighbor(id)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScriptBudget {
    pub remaining: u32,
    pub depth: u16,
    pub used: u32,
}

/// Speculative entrypoint-five probe used by placement previews. The proposed
/// pose is visible to the script, while writes, effects and RNG stay isolated.
pub(crate) fn check_intersection(
    state: &SimState,
    content: &ContentSet,
    call: &world::IntersectionCall,
    budget: &mut ScriptBudget,
) -> Result<bool, VmFault> {
    if budget.depth >= MAX_CREATION_DEPTH {
        return Err(host_error("scripted placement query depth"));
    }
    if budget.remaining == 0 {
        return Err(host_error("scripted placement query budget"));
    }
    let owner = live_entity(state, call.caller)?.info.guid;
    let Some(key) = content
        .object(owner)
        .and_then(|d| d.entry_points.get(&call.entrypoint))
        .copied()
    else {
        return Ok(false);
    };
    let mut staged = state.clone();
    if staged.ids.is_live(call.proposed_object) {
        move_entity(
            &mut staged,
            call.proposed_object,
            call.proposed_position,
            call.proposed_facing,
        )
        .map_err(host_error)?;
    }
    let mut work = TickWork::default();
    let mut thread = VmThread::new(call.caller, staged.mode);
    thread.is_check = true;
    if let Some(original) = staged.threads.get(&call.caller.object_id) {
        thread.temps = original.temps;
        thread.temp_xl = original.temp_xl;
    }
    let context = FrameContext {
        caller: call.caller,
        callee: call.caller,
        stack_object: call.other.object_id,
        stack_object_ref: Some(call.other),
        code_owner: owner,
    };
    let mut args = entry_args(content, key, call.other.object_id)?;
    args[1] = i16::from(call.ghost);
    thread.push_entry(content.routines(), key, context, args)?;
    let limit = budget
        .remaining
        .min(staged.limits.instruction_budget_per_entity);
    let report = run_synchronous_thread(
        &mut staged,
        content,
        &mut work,
        call.caller,
        budget.depth + 1,
        &mut thread,
        limit,
        &mut BTreeMap::new(),
        true,
    )?;
    let used = report.instructions.saturating_add(work.instructions);
    budget.used = budget.used.saturating_add(used);
    budget.remaining = budget.remaining.saturating_sub(used);
    match report.stop {
        VmStop::Completed(exit) => Ok(exit == PrimitiveExit::ReturnTrue),
        VmStop::Faulted(error) => Err(error),
        _ => Err(host_error(
            "intersection check did not finish synchronously",
        )),
    }
}

/// Source EvaluateCheck repeatedly ticks its temporary thread at the current
/// simulation time. Timer yields can therefore finish without a wall-clock wait.
/// A single total budget also bounds animations or other non-progressing checks.
// Keep synchronous source-check execution inputs explicit at the VM host boundary.
#[allow(clippy::too_many_arguments)]
fn run_synchronous_thread(
    state: &mut SimState,
    content: &ContentSet,
    work: &mut TickWork,
    owner: EntityRef,
    depth: u16,
    thread: &mut VmThread,
    budget: u32,
    entity_threads: &mut BTreeMap<ObjectId, VmThread>,
    separate_check: bool,
) -> Result<RunReport, VmFault> {
    let nested_start = work.instructions;
    let mut instructions = 0u32;
    loop {
        let remaining = budget.saturating_sub(
            instructions.saturating_add(work.instructions.saturating_sub(nested_start)),
        );
        if remaining == 0 {
            return Ok(RunReport {
                instructions,
                stop: VmStop::BudgetExhausted,
            });
        }
        let mut host = RuntimeHost::new(state, content, work, owner, depth);
        host.entity_threads = std::mem::take(entity_threads);
        host.separate_check = separate_check;
        host.dispatch_budget = remaining;
        let report = thread.run(content.routines(), &mut host, remaining);
        *entity_threads = std::mem::take(&mut host.entity_threads);
        drop(host);
        instructions = instructions.saturating_add(report.instructions);
        if instructions.saturating_add(work.instructions.saturating_sub(nested_start)) > budget {
            return Ok(RunReport {
                instructions,
                stop: VmStop::BudgetExhausted,
            });
        }
        if let VmStop::Sleeping { until_tick } = report.stop {
            let scheduled = (|| -> Result<(), VmFault> {
                let caller = thread.top()?.context.caller;
                let item = live_entity(state, caller)?;
                let every_frame = item.always_tick
                    || item.info.is_avatar
                    || item.headline.is_some()
                    || item.disabled_flags > 2;
                let delay = until_tick
                    .wrapping_sub(state.scheduler.current_tick() as u32)
                    .max(1);
                state
                    .scheduler
                    .schedule_in(caller, delay, every_frame)
                    .map_err(host_error)
            })();
            if let Err(fault) = scheduled {
                return Ok(RunReport {
                    instructions,
                    stop: VmStop::Faulted(fault),
                });
            }
            thread.wake();
        } else {
            return Ok(RunReport {
                instructions,
                stop: report.stop,
            });
        }
    }
}

struct RuntimeHost<'a> {
    state: &'a mut SimState,
    content: &'a ContentSet,
    work: &'a mut TickWork,
    owner: EntityRef,
    creation_depth: u16,
    active: Option<VmThread>,
    entity_threads: BTreeMap<ObjectId, VmThread>,
    separate_check: bool,
    dispatch_budget: u32,
    observed_instructions: u32,
    nested_start: u32,
}
impl<'a> RuntimeHost<'a> {
    fn new(
        state: &'a mut SimState,
        content: &'a ContentSet,
        work: &'a mut TickWork,
        owner: EntityRef,
        creation_depth: u16,
    ) -> Self {
        let dispatch_budget = state.limits.instruction_budget_per_entity.min(
            state
                .limits
                .max_tick_instructions
                .saturating_sub(work.instructions),
        );
        let nested_start = work.instructions;
        Self {
            state,
            content,
            work,
            owner,
            creation_depth,
            active: None,
            entity_threads: BTreeMap::new(),
            separate_check: false,
            dispatch_budget,
            observed_instructions: 0,
            nested_start,
        }
    }
}
impl RuntimeHost<'_> {
    fn thread_view(&self) -> crate::runtime_memory::ThreadView<'_> {
        crate::runtime_memory::ThreadView {
            current: self.active.as_ref(),
            entity_threads: Some(&self.entity_threads),
        }
    }
    fn script_budget(&self) -> ScriptBudget {
        let local_spent = self
            .observed_instructions
            .saturating_add(self.work.instructions.saturating_sub(self.nested_start));
        ScriptBudget {
            remaining: self.dispatch_budget.saturating_sub(local_spent).min(
                self.state
                    .limits
                    .max_tick_instructions
                    .saturating_sub(self.work.instructions),
            ),
            depth: self.creation_depth,
            used: 0,
        }
    }
    fn projected_world_state(&self) -> SimState {
        let mut state = self.state.clone();
        for (id, thread) in &self.entity_threads {
            state.threads.insert(*id, thread.clone());
        }
        state
    }
    fn commit_world_state(&mut self, mut staged: SimState) {
        for id in self.entity_threads.keys() {
            if let Some(original) = self.state.threads.get(id) {
                staged.threads.insert(*id, original.clone());
            } else {
                staged.threads.remove(id);
            }
        }
        *self.state = staged;
    }
    fn update_entity_banks(
        &mut self,
        owner: EntityRef,
        temps: [i16; 20],
        temp_xl: [i32; 2],
        queue: bool,
    ) {
        if let Some(real) = self.state.threads.get_mut(&owner.object_id) {
            real.temps = temps;
            real.temp_xl = temp_xl;
        }
        if let Some(real) = self.entity_threads.get_mut(&owner.object_id) {
            real.temps = temps;
            real.temp_xl = temp_xl;
            if queue {
                self.work
                    .temp_writes
                    .entry(owner)
                    .or_default()
                    .extend(temps.into_iter().enumerate().map(|(i, v)| (i as u16, v)));
                self.work
                    .temp_xl_writes
                    .entry(owner)
                    .or_default()
                    .extend(temp_xl.into_iter().enumerate().map(|(i, v)| (i as u16, v)));
            }
        }
    }
}
impl VmHost for RuntimeHost<'_> {
    fn requires_supported_behavior(&self, owner: EntityRef) -> bool {
        self.separate_check
            || self.work.strict_source
            || self
                .state
                .interaction_queues
                .get(&owner.object_id)
                .is_some_and(|q| q.active_len() > 0)
    }
    fn interaction_returned(
        &mut self,
        owner: EntityRef,
        exit: PrimitiveExit,
        _run_immediately: bool,
    ) -> Result<(), VmFault> {
        self.queue_returned(owner, exit)
    }
    fn relationship_read(&self, key: RelationshipKey) -> Result<Option<Vec<i16>>, VmFault> {
        validate_relationship_key(self.state, key)?;
        Ok(self.state.relationships.read(key))
    }
    fn relationship_write(
        &mut self,
        key: RelationshipKey,
        values: Vec<i16>,
    ) -> Result<(), VmFault> {
        validate_relationship_key(self.state, key)?;
        self.state.relationships.write(key, values.clone())?;
        if let RelationshipOwner::Entity(owner) = key.owner {
            if let Some(avatar) = live_entity_mut(self.state, owner)?.avatar.as_mut() {
                avatar
                    .relationships
                    .values
                    .insert(avatar_relationship_target(key.target), values);
            }
        }
        Ok(())
    }
    fn relationship_mark(
        &mut self,
        key: RelationshipKey,
        persistent_dirty: bool,
    ) -> Result<(), VmFault> {
        validate_relationship_key(self.state, key)?;
        self.state.relationships.mark(key, persistent_dirty)?;
        if persistent_dirty {
            if let (RelationshipOwner::Entity(owner), RelationshipTarget::Persistent(target)) =
                (key.owner, key.target)
            {
                if let Some(avatar) = live_entity_mut(self.state, owner)?.avatar.as_mut() {
                    avatar
                        .relationships
                        .changed_persistent
                        .insert(PersistentId(target));
                }
            }
        }
        Ok(())
    }
    fn relationship_multiplier(&self) -> Result<f32, VmFault> {
        let category = live_entity(self.state, self.owner)?
            .avatar
            .as_ref()
            .map_or(0, |a| a.lot_category);
        Ok(*self
            .content
            .tuning()
            .relationship_multipliers
            .get(&category)
            .unwrap_or(&1.0))
    }
    fn behavior_entry(&self, entity: EntityRef, entry: u8) -> Result<BehaviorEntry, VmFault> {
        let item = live_entity(self.state, entity)?;
        let definition = self
            .content
            .object(item.info.guid)
            .ok_or_else(|| host_error("definition missing"))?;
        if u16::from(entry) >= definition.entry_point_count {
            return Err(VmFault::Bounds {
                area: "entrypoint".into(),
                index: i32::from(entry),
                len: usize::from(definition.entry_point_count),
            });
        }
        let bind = |key: &RoutineKey| {
            self.content.routines().get(*key).map(|r| BoundRoutine {
                routine: *key,
                code_owner: item.info.guid,
                arguments: r.arguments(),
            })
        };
        Ok(BehaviorEntry {
            action_declared: definition.entry_points.contains_key(&entry),
            action: definition.entry_points.get(&entry).and_then(bind),
            condition_declared: definition.entry_conditions.contains_key(&entry),
            condition: definition.entry_conditions.get(&entry).and_then(bind),
        })
    }
    fn evaluate_check(&mut self, request: RoutineCheck) -> Result<CheckResult, VmFault> {
        if self.creation_depth >= MAX_CREATION_DEPTH {
            return Err(host_error("nested check depth"));
        }
        let owner = request.run_in_owner.unwrap_or(request.context.caller);
        live_entity(self.state, owner)?;
        let named = request.run_in_owner.is_some();
        let current = named && request.use_current_thread;
        if current && owner != self.owner {
            return Err(host_error("current-thread check owner mismatch"));
        }
        let aliases_current = owner == self.owner && self.entity_temps_alias(owner);
        let real_original = self
            .entity_threads
            .get(&owner.object_id)
            .or_else(|| self.state.threads.get(&owner.object_id))
            .cloned();
        let original = if current {
            self.active.clone()
        } else {
            real_original.clone()
        };
        let fresh_request = current || aliases_current;
        let (mut temps, mut temp_xl) = if fresh_request {
            (request.temps, request.temp_xl)
        } else {
            real_original
                .as_ref()
                .map_or((request.temps, request.temp_xl), |t| (t.temps, t.temp_xl))
        };
        if !current || aliases_current {
            if let Some(writes) = self.work.temp_writes.get(&owner) {
                for (index, value) in writes {
                    temps[usize::from(*index)] = *value;
                }
            }
            if let Some(writes) = self.work.temp_xl_writes.get(&owner) {
                for (index, value) in writes {
                    temp_xl[usize::from(*index)] = *value;
                }
            }
        }
        let shared_check = self.state.scheduler.is_running();
        let copy_back = current || aliases_current && (named || shared_check);
        let writes_entity = if named {
            !current || aliases_current
        } else {
            shared_check
        };
        let mut context = request.context;
        let action_tree = if named {
            // Source RunInMyStack discards its sole child when the old stack is empty.
            let Some(parent) = original.as_ref().and_then(|t| t.frames.last()) else {
                return Ok(CheckResult {
                    accepted: false,
                    aborting: false,
                    exit: PrimitiveExit::ReturnFalse,
                    temps,
                    temp_xl,
                    copy_back,
                    thread_control: None,
                });
            };
            context.caller = parent.context.caller;
            context.code_owner = request.routine.code_owner;
            parent.action_tree
        } else {
            false
        };
        let mut staged = self.state.clone();
        let mut work = TickWork {
            resetting: self.work.resetting.clone(),
            detached_threads: self.work.detached_threads.clone(),
            ..TickWork::default()
        };
        let mut thread = VmThread::new(owner, staged.mode);
        thread.is_check = true;
        thread.temps = temps;
        thread.temp_xl = temp_xl;
        if named {
            if let Some(original) = original.as_ref() {
                thread.interrupt = original.interrupt;
                thread.schedule_idle_start = original.schedule_idle_start;
                thread.last_exit = original.last_exit;
            }
            if !current || !self.separate_check {
                if self.work.interrupt_requests.remove(&owner) {
                    thread.interrupt = true;
                }
                if let Some((interrupt, idle, last_exit)) = self.work.thread_controls.remove(&owner)
                {
                    thread.interrupt = interrupt;
                    thread.schedule_idle_start = idle;
                    thread.last_exit = last_exit;
                }
            }
        }
        thread.push_entry(
            self.content.routines(),
            request.routine.routine,
            context,
            request.args,
        )?;
        thread.top_mut()?.action_tree = action_tree;
        let budget = self.script_budget().remaining;
        if budget == 0 {
            return Err(host_error("check budget"));
        }
        let separate = if named {
            current && self.separate_check
        } else {
            true
        };
        let mut overlay = self.entity_threads.clone();
        let report = run_synchronous_thread(
            &mut staged,
            self.content,
            &mut work,
            owner,
            self.creation_depth + 1,
            &mut thread,
            budget,
            &mut overlay,
            separate,
        )?;
        let spent = work.instructions.saturating_add(report.instructions);
        self.work.instructions = self.work.instructions.saturating_add(spent);
        if self.work.instructions > self.state.limits.max_tick_instructions {
            return Err(host_error("tick check budget"));
        }
        let exit = match report.stop {
            VmStop::Completed(exit) => exit,
            VmStop::Faulted(_) if named => PrimitiveExit::Error,
            VmStop::Faulted(fault) => return Err(fault),
            _ => {
                return Err(host_error(
                    "synchronous check yielded or exhausted its budget",
                ))
            }
        };
        if named && !separate {
            if let Some(mut real) = real_original {
                real.temps = thread.temps;
                real.temp_xl = thread.temp_xl;
                real.interrupt = thread.interrupt;
                real.schedule_idle_start = thread.schedule_idle_start;
                real.last_exit = thread.last_exit;
                overlay.insert(owner.object_id, real);
            }
        }
        *self.state = staged;
        self.entity_threads = overlay;
        self.work.events.extend(work.events);
        self.work.reset_requests.extend(work.reset_requests);
        self.work.interrupt_requests.extend(work.interrupt_requests);
        self.work.thread_controls.extend(work.thread_controls);
        for (owner, writes) in work.temp_writes {
            self.work
                .temp_writes
                .entry(owner)
                .or_default()
                .extend(writes);
        }
        for (owner, writes) in work.temp_xl_writes {
            self.work
                .temp_xl_writes
                .entry(owner)
                .or_default()
                .extend(writes);
        }
        if writes_entity {
            self.update_entity_banks(owner, thread.temps, thread.temp_xl, !copy_back);
        }
        if named && writes_entity {
            for real in [
                self.state.threads.get_mut(&owner.object_id),
                self.entity_threads.get_mut(&owner.object_id),
            ]
            .into_iter()
            .flatten()
            {
                real.interrupt = thread.interrupt;
                real.schedule_idle_start = thread.schedule_idle_start;
                real.last_exit = thread.last_exit;
            }
            if !copy_back && self.work.detached_threads.contains(&owner) {
                self.work.thread_controls.insert(
                    owner,
                    (
                        thread.interrupt,
                        thread.schedule_idle_start,
                        thread.last_exit,
                    ),
                );
            }
        }
        if copy_back {
            if let Some(active) = self.active.as_mut() {
                active.temps = thread.temps;
                active.temp_xl = thread.temp_xl;
                if named {
                    active.interrupt = thread.interrupt;
                    active.schedule_idle_start = thread.schedule_idle_start;
                    active.last_exit = thread.last_exit;
                }
            }
        }
        Ok(CheckResult {
            accepted: exit == PrimitiveExit::ReturnTrue,
            aborting: false,
            exit,
            temps: thread.temps,
            temp_xl: thread.temp_xl,
            copy_back,
            thread_control: named.then_some((thread.interrupt, thread.schedule_idle_start)),
        })
    }
    fn named_tree(&self, request: NameLookup) -> Result<Option<BoundRoutine>, VmFault> {
        let target = request
            .context
            .stack_object_ref
            .ok_or(VmFault::MissingEntity(request.context.stack_object))?;
        let target = live_entity(self.state, target)?;
        let owner = if request.string_scope == 1 {
            0
        } else if request.current_routine.id >= 8192 {
            self.content
                .routines()
                .semiglobal(request.context.code_owner)
                .filter(|owner| self.content.has_string_table(*owner, request.string_table))
                .unwrap_or(request.context.code_owner)
        } else {
            request.context.code_owner
        };
        let Some(name) = self
            .content
            .string(owner, request.string_table, request.string_index)
        else {
            return Ok(None);
        };
        Ok(self
            .content
            .named_tree(target.info.guid, name)
            .and_then(|key| {
                self.content.routines().get(key).map(|r| BoundRoutine {
                    routine: key,
                    code_owner: target.info.guid,
                    arguments: r.arguments(),
                })
            }))
    }
    fn function_status(&self, entity: EntityRef) -> Result<FunctionEntityState, VmFault> {
        let item = live_entity(self.state, entity)?;
        let base = self
            .state
            .entities
            .get(&item.info.base_object)
            .ok_or(VmFault::MissingEntity(item.info.base_object))?;
        let in_use =
            crate::runtime_memory::is_in_use_with_threads(self.state, entity, self.thread_view())?;
        Ok(FunctionEntityState {
            disabled: item.disabled_flags != 0,
            broken: base.broken,
            in_use,
        })
    }
    fn idle_for_input(
        &mut self,
        context: &FrameContext,
        allow_push: bool,
        action_tree: bool,
        _mode: VmMode,
    ) -> Result<IdleDecision, VmFault> {
        self.queue_idle(context, allow_push, action_tree)
    }
    fn attempt_push(
        &mut self,
        owner: EntityRef,
        _context: &FrameContext,
    ) -> Result<Option<RoutineCall>, VmFault> {
        self.queue_attempt_push(owner)
    }
    fn interaction_available(
        &self,
        source: EntityRef,
        target: EntityRef,
        interaction: u8,
    ) -> Result<bool, VmFault> {
        self.queue_available(source, target, interaction)
    }
    fn push_interaction(&mut self, request: PushInteractionRequest) -> Result<bool, VmFault> {
        self.queue_push(request)
    }
    fn change_interaction_icon(
        &mut self,
        caller: EntityRef,
        icon: EntityRef,
    ) -> Result<(), VmFault> {
        self.queue_icon(caller, icon)
    }
    fn interaction_state(&self, entity: EntityRef) -> Result<InteractionState, VmFault> {
        live_entity(self.state, entity)?;
        let thread = self
            .active
            .as_ref()
            .filter(|t| t.owner == entity)
            .or_else(|| {
                self.entity_threads
                    .get(&entity.object_id)
                    .or_else(|| self.state.threads.get(&entity.object_id))
            });
        Ok(InteractionState {
            action_tree: thread
                .and_then(|t| t.frames.last())
                .is_some_and(|f| f.action_tree),
            callee: self
                .state
                .interaction_queues
                .get(&entity.object_id)
                .and_then(|q| q.active())
                .map(|entry| interaction_adapter::entity_ref(entry.invocation.target))
                .transpose()
                .map_err(host_error)?,
        })
    }
    fn resolve_dialog_string(&self, request: StringLookup) -> Result<Option<String>, VmFault> {
        let owner = match request.source {
            StringSource::Global => 0,
            StringSource::CodeOwner => request.context.code_owner,
            StringSource::CodeOwnerSemiGlobal => self
                .content
                .routines()
                .semiglobal(request.context.code_owner)
                .unwrap_or(request.context.code_owner),
            StringSource::CalleeSemiGlobal => self
                .content
                .routines()
                .semiglobal(live_entity(self.state, request.context.callee)?.info.guid)
                .unwrap_or(0),
        };
        let Some(text) = self.content.string(owner, request.table, request.index) else {
            return Ok(None);
        };
        if text.contains('$') {
            return Err(host_error(
                "dynamic dialog string interpolation requires the content text provider",
            ));
        }
        Ok(Some(text.to_owned()))
    }
    fn presentation(&mut self, request: PresentationRequest) -> Result<(), VmFault> {
        match &request {
            PresentationRequest::ShowString {
                target, message, ..
            } => avatar_mut(self.state, *target)?
                .set_message(message.clone())
                .map_err(host_error)?,
            PresentationRequest::Balloon {
                target,
                icon,
                index,
                group,
                duration,
                headline_type,
                flags,
                clear,
                preserve_money_on_zero_duration,
            } => {
                let item = live_entity_mut(self.state, *target)?;
                if *clear {
                    if !(*preserve_money_on_zero_duration
                        && item
                            .headline
                            .as_ref()
                            .is_some_and(|h| matches!(h.kind, HeadlineKind::Money(_))))
                    {
                        item.headline = None;
                    }
                } else {
                    item.headline = Some(HeadlineState {
                        duration: if flags & 8 != 0 && *duration != -1 {
                            i32::from(*duration) * 15
                        } else {
                            i32::from(*duration)
                        },
                        anim: 0,
                        kind: HeadlineKind::Balloon {
                            icon: *icon,
                            index: *index,
                            group: *group,
                            headline_type: *headline_type,
                            flags: *flags,
                        },
                    });
                }
            }
            PresentationRequest::Refresh { target, .. } => {
                live_entity(self.state, *target)?;
            }
            PresentationRequest::ActionName { caller, .. } => {
                live_entity(self.state, *caller)?;
            }
        }
        self.work.events.push(RuntimeEvent::Display(request));
        Ok(())
    }
    fn resolve_suit(&self, request: SuitLookup) -> Result<Option<ResolvedSuit>, VmFault> {
        let caller = live_entity(self.state, request.context.caller)?;
        let avatar = caller
            .avatar
            .as_ref()
            .ok_or_else(|| host_error("suit caller is not avatar"))?;
        if request.scope == 1 {
            let result = crate::runtime_avatars::resolve_person_outfit(
                self.state,
                self.content,
                request.context.caller,
                u16::from(request.resolved_data),
            )?;
            return Ok(result.map(|outfit| {
                if avatar.platform == AvatarPlatform::Tso {
                    ResolvedSuit::Id(outfit.legacy_id())
                } else {
                    ResolvedSuit::Reference(outfit)
                }
            }));
        }
        let owner = if request.scope == 0 {
            0
        } else if request.scope == 2 {
            if request.default_update {
                live_entity(self.state, request.context.callee)?.info.guid
            } else {
                request.context.code_owner
            }
        } else {
            caller.info.guid
        };
        if request.default_update {
            let Some(value) = self
                .content
                .string(owner, 304, i32::from(request.update_index))
            else {
                return Err(host_error("default daywear string missing"));
            };
            return Ok(Some(ResolvedSuit::Reference(
                crate::avatars::outfits::OutfitReference::parse_source(value, avatar.platform)
                    .map_err(host_error)?,
            )));
        }
        Ok(self
            .content
            .suit(owner, request.scope, u16::from(request.resolved_data))
            .cloned())
    }
    fn apply_appearance(&mut self, operation: AppearanceOperation) -> Result<(), VmFault> {
        match &operation {
            AppearanceOperation::DefaultDaywear { target, outfit } => {
                avatar_mut(self.state, *target)?
                    .set_default_daywear(outfit.clone())
                    .map_err(host_error)?
            }
            AppearanceOperation::Body {
                target,
                outfit,
                current_outfit,
            } => avatar_mut(self.state, *target)?
                .set_body_outfit(outfit.clone(), *current_outfit)
                .map_err(host_error)?,
            AppearanceOperation::AccessoryName {
                target,
                appearance,
                remove,
            } => avatar_mut(self.state, *target)?
                .set_accessory_name(appearance.clone(), *remove)
                .map_err(host_error)?,
            AppearanceOperation::Decoration { target, .. } => {
                live_entity(self.state, *target)?;
            }
        }
        self.work.events.push(RuntimeEvent::Appearance(operation));
        Ok(())
    }
    fn ts1_family_budget(&self) -> Result<Option<i32>, VmFault> {
        if self.state.mode != VmMode::Ts1 {
            return Err(host_error("TS1 family provider in TSO"));
        }
        Ok(self.state.ts1_family_budget)
    }
    fn set_ts1_family_budget(&mut self, value: i32) -> Result<(), VmFault> {
        if self.state.mode != VmMode::Ts1 || self.state.ts1_family_budget.is_none() {
            return Err(host_error("TS1 current family missing"));
        }
        self.state.ts1_family_budget = Some(value);
        Ok(())
    }
    fn ts1_inventory_read(&self, neighbor: i16) -> Result<Option<Vec<Ts1InventoryItem>>, VmFault> {
        if self.state.mode != VmMode::Ts1 {
            return Err(host_error("TS1 inventory provider in TSO"));
        }
        Ok(self.state.ts1_inventory.read(neighbor))
    }
    fn ts1_inventory_write(
        &mut self,
        neighbor: i16,
        items: Vec<Ts1InventoryItem>,
    ) -> Result<(), VmFault> {
        if self.state.mode != VmMode::Ts1 {
            return Err(host_error("TS1 inventory provider in TSO"));
        }
        self.state.ts1_inventory.write(neighbor, items)
    }
    fn fire_state(&self) -> Result<FireState, VmFault> {
        Ok(FireState {
            enabled: self.content.tuning().fire_enabled,
            percent: self.state.clock.fire_percent,
            width: self.state.world.lot.width(),
            height: self.state.world.lot.height(),
        })
    }
    fn set_fire_percent(&mut self, percent: i32) -> Result<(), VmFault> {
        self.state.clock.fire_percent = percent;
        Ok(())
    }
    fn room_is_pool(&self, entity: EntityRef) -> Result<bool, VmFault> {
        let position = lot_position(live_entity(self.state, entity)?.info.position)?;
        Ok(self
            .state
            .world
            .lot
            .room_at(position)
            .and_then(|id| self.state.world.lot.rooms().rooms.get(&id))
            .is_some_and(|room| room.is_pool || room.is_water))
    }
    fn current_tick(&self) -> u32 {
        self.state.scheduler.current_tick() as u32
    }
    fn next_random(&mut self, bound: u64) -> u64 {
        self.state.rng.next(bound)
    }
    fn observe_thread(&mut self, thread: &VmThread) {
        self.observed_instructions = self.observed_instructions.saturating_add(1);
        self.active = Some(thread.clone());
        if !self.separate_check {
            self.entity_threads
                .insert(thread.owner.object_id, thread.clone());
        } else if self.entity_temps_alias(thread.owner) {
            if !self.entity_threads.contains_key(&thread.owner.object_id) {
                if let Some(real) = self.state.threads.get(&thread.owner.object_id) {
                    self.entity_threads
                        .insert(thread.owner.object_id, real.clone());
                }
            }
            self.update_entity_banks(thread.owner, thread.temps, thread.temp_xl, false);
        }
    }
    fn entity_temps_alias(&self, owner: EntityRef) -> bool {
        owner == self.owner && (!self.separate_check || self.state.scheduler.is_running())
    }
    fn take_temp_writes(&mut self, owner: EntityRef) -> Vec<(u16, i16)> {
        if !self.entity_temps_alias(owner) {
            return Vec::new();
        }
        self.work
            .temp_writes
            .remove(&owner)
            .map(|v| v.into_iter().collect())
            .unwrap_or_default()
    }
    fn take_temp_xl_writes(&mut self, owner: EntityRef) -> Vec<(u16, i32)> {
        if !self.entity_temps_alias(owner) {
            return Vec::new();
        }
        self.work
            .temp_xl_writes
            .remove(&owner)
            .map(|v| v.into_iter().collect())
            .unwrap_or_default()
    }
    fn take_interrupt_request(&mut self, owner: EntityRef) -> bool {
        !self.separate_check && self.work.interrupt_requests.remove(&owner)
    }
    fn take_thread_control(&mut self, owner: EntityRef) -> Option<(bool, u32, PrimitiveExit)> {
        if self.separate_check {
            None
        } else {
            self.work.thread_controls.remove(&owner)
        }
    }
    fn take_reset_request(&mut self, owner: EntityRef) -> bool {
        self.work.reset_requests.contains(&owner)
    }
    fn resolve_entity(&self, id: ObjectId) -> Option<EntityRef> {
        self.state.ids.resolve(id).ok()
    }
    fn entity_info(&self, entity: EntityRef) -> Result<EntityInfo, VmFault> {
        Ok(live_entity(self.state, entity)?.info.clone())
    }
    fn entity_ids(&self) -> Result<Vec<ObjectId>, VmFault> {
        Ok(self.state.entities.keys().copied().collect())
    }
    fn read_memory(&self, address: &MemoryAddress) -> Result<i16, VmFault> {
        crate::runtime_memory::read_memory_with_threads(
            self.state,
            self.content,
            address,
            self.thread_view(),
        )
    }
    fn write_memory(&mut self, address: &MemoryAddress, value: i16) -> Result<bool, VmFault> {
        if let MemoryAddress::Entity {
            entity,
            field: EntityField::Temp,
            index,
        } = address
        {
            live_entity(self.state, *entity)?;
            if *index >= 20 {
                return Err(VmFault::Bounds {
                    area: "other thread temp".into(),
                    index: i32::from(*index),
                    len: 20,
                });
            }
            if *entity == self.owner
                || self.entity_threads.contains_key(&entity.object_id)
                || self.work.detached_threads.contains(entity)
            {
                if !self.entity_threads.contains_key(&entity.object_id) {
                    let thread = self
                        .state
                        .threads
                        .get(&entity.object_id)
                        .or(self.active.as_ref())
                        .ok_or_else(|| host_error("thread temp owner missing"))?
                        .clone();
                    self.entity_threads.insert(entity.object_id, thread);
                }
                self.entity_threads
                    .get_mut(&entity.object_id)
                    .expect("thread view")
                    .temps[usize::from(*index)] = value;
                if let Some(real) = self.state.threads.get_mut(&entity.object_id) {
                    real.temps[usize::from(*index)] = value;
                }
                self.work
                    .temp_writes
                    .entry(*entity)
                    .or_default()
                    .insert(*index, value);
                return Ok(true);
            }
        }
        let view = crate::runtime_memory::ThreadView {
            current: self.active.as_ref(),
            entity_threads: Some(&self.entity_threads),
        };
        let result = crate::runtime_memory::write_memory_with_threads(
            self.state,
            self.content,
            address,
            value,
            view,
        )?;
        memory_signals(self.state, self.content, self.work, result.signals).map_err(host_error)?;
        Ok(result.written)
    }
    fn read_list(&self, entity: EntityRef) -> Result<Vec<i16>, VmFault> {
        Ok(live_entity(self.state, entity)?.list.clone())
    }
    fn replace_list(&mut self, entity: EntityRef, values: Vec<i16>) -> Result<(), VmFault> {
        if values.len() > self.state.limits.max_list_items as usize {
            return Err(host_error("list capacity"));
        }
        live_entity_mut(self.state, entity)?.list = values;
        Ok(())
    }
    fn semiglobal_for_guid(&self, guid: u32) -> Result<Option<u32>, VmFault> {
        Ok(self.content.routines().semiglobal(guid))
    }
    fn show_money_headline(&mut self, caller: EntityRef, amount: i32) -> Result<(), VmFault> {
        live_entity_mut(self.state, caller)?.headline = Some(HeadlineState {
            duration: 60,
            anim: 0,
            kind: HeadlineKind::Money(amount),
        });
        self.work.events.push(RuntimeEvent::MoneyHeadline {
            entity: caller,
            amount,
        });
        Ok(())
    }
    fn entity_operation(
        &mut self,
        operation: EntityOperation,
    ) -> Result<EntityOperationResult, VmFault> {
        match operation {
            EntityOperation::Notify { target } => {
                live_entity(self.state, target)?;
                self.work.interrupt_requests.insert(target);
                if let Some(thread) = self.state.threads.get_mut(&target.object_id) {
                    thread.interrupt = true;
                }
                if let Some(thread) = self.entity_threads.get_mut(&target.object_id) {
                    thread.interrupt = true;
                }
                self.state.scheduler.interrupt(target).map_err(host_error)?;
                Ok(EntityOperationResult::Bool(true))
            }
            EntityOperation::Delete {
                target,
                cleanup_all,
                ..
            } => {
                let targets = if cleanup_all {
                    live_entity(self.state, target)?
                        .info
                        .group
                        .iter()
                        .filter_map(|id| self.state.ids.resolve(*id).ok())
                        .collect()
                } else {
                    vec![target]
                };
                for entity in targets {
                    live_entity(self.state, entity)?;
                    self.state.scheduler.delete_at_end(entity);
                }
                Ok(EntityOperationResult::Bool(true))
            }
            EntityOperation::PlaceInSlot {
                container,
                object,
                slot,
                clean_old,
            } => {
                let mut staged = self.state.clone();
                let result = place_in_slot(&mut staged, container, object, slot, clean_old)
                    .map_err(host_error)?;
                if result {
                    *self.state = staged;
                }
                Ok(EntityOperationResult::Bool(result))
            }
            EntityOperation::ChangePosition {
                target,
                position,
                direction,
            } => {
                let mut staged = self.projected_world_state();
                let mut budget = self.script_budget();
                let result = crate::runtime_routes::move_checked(
                    &mut staged,
                    self.content,
                    target,
                    lot_position(position)?,
                    Facing(direction),
                    &mut budget,
                );
                self.work.instructions = self.work.instructions.saturating_add(budget.used);
                let result = result?;
                if result {
                    self.commit_world_state(staged);
                }
                Ok(EntityOperationResult::Bool(result))
            }
            EntityOperation::Create {
                guid,
                position,
                direction,
                main_parameter,
                main_stack_object,
            } => {
                let mut staged = self.projected_world_state();
                let mut staged_work = TickWork {
                    strict_source: self.work.strict_source,
                    thread_controls: self.work.thread_controls.clone(),
                    detached_threads: self.work.detached_threads.clone(),
                    instructions: self.work.instructions,
                    events: Vec::new(),
                    reset_requests: self.work.reset_requests.clone(),
                    resetting: self.work.resetting.clone(),
                    interrupt_requests: self.work.interrupt_requests.clone(),
                    temp_writes: self.work.temp_writes.clone(),
                    temp_xl_writes: self.work.temp_xl_writes.clone(),
                };
                let mut budget = self.script_budget();
                let result = spawn_entity(
                    &mut staged,
                    self.content,
                    &mut staged_work,
                    SpawnSpec {
                        guid,
                        position: lot_position(position)?,
                        facing: Facing(direction),
                        persistent_id: PersistentId(0),
                        avatar: false,
                    },
                    main_parameter,
                    main_stack_object,
                    self.creation_depth + 1,
                )
                .and_then(|entity| {
                    if crate::runtime_routes::move_checked(
                        &mut staged,
                        self.content,
                        entity,
                        lot_position(position)?,
                        Facing(direction),
                        &mut budget,
                    )? {
                        Ok(entity)
                    } else {
                        Err(invalid("creation position rejected"))
                    }
                });
                self.work.instructions = staged_work.instructions.saturating_add(budget.used);
                match result {
                    Ok(entity) => {
                        self.commit_world_state(staged);
                        self.work.events.extend(staged_work.events);
                        self.work.reset_requests = staged_work.reset_requests;
                        self.work.interrupt_requests = staged_work.interrupt_requests;
                        self.work.temp_writes = staged_work.temp_writes;
                        self.work.temp_xl_writes = staged_work.temp_xl_writes;
                        self.work.thread_controls = staged_work.thread_controls;
                        Ok(EntityOperationResult::Created(Some(entity)))
                    }
                    Err(RuntimeError::CreationDepth | RuntimeError::InstructionLimit) => {
                        Err(host_error("recursive creation budget"))
                    }
                    Err(_) => Ok(EntityOperationResult::Created(None)),
                }
            }
        }
    }
    fn request(&mut self, request: HostRequest) -> Result<HostResponse, VmFault> {
        match request {
            HostRequest::Animation(request) => self.animation_request(request),
            HostRequest::MotiveChange(request) => {
                avatar_mut(self.state, request.context.caller)?
                    .apply_motive_change(
                        request.motive,
                        request.raw_rate,
                        request.raw_max,
                        request.clear_all,
                        request.once,
                    )
                    .map_err(host_error)?;
                Ok(HostResponse::Complete(PrimitiveExit::GotoTrue))
            }
            HostRequest::Route(request) => self.route_request(request),
            HostRequest::External(request) => {
                if matches!(
                    request.kind,
                    ExternalKind::Sound | ExternalKind::SpecialEffect
                ) {
                    self.work.events.push(RuntimeEvent::Presentation(request));
                    return Ok(HostResponse::Complete(PrimitiveExit::GotoTrue));
                }
                let id = next_continuation(self.state).map_err(host_error)?;
                let bytes = canonical_bytes(
                    &request,
                    u64::from(self.state.effects.limits().max_request_bytes),
                )
                .map_err(host_error)?;
                let effect = self
                    .state
                    .effects
                    .issue(
                        self.owner,
                        self.state.scheduler.current_tick(),
                        self.state.authority_epoch,
                        EffectKind::Bytes,
                        EffectPayload::Bytes(bytes),
                    )
                    .map_err(host_error)?;
                self.state.continuations.insert(
                    id,
                    RuntimeContinuation {
                        id,
                        entity: self.owner,
                        kind: ContinuationKind::Effect(effect.operation_id),
                        resumes_vm: true,
                    },
                );
                self.work.events.push(RuntimeEvent::ExternalRequested {
                    operation_id: effect.operation_id,
                    entity: self.owner,
                });
                Ok(HostResponse::Pending { request_id: id })
            }
        }
    }
}

impl RuntimeHost<'_> {
    fn animation_request(&mut self, request: AnimationRequest) -> Result<HostResponse, VmFault> {
        crate::runtime_avatars::animate(self.state, self.content, &request)
    }
    fn route_request(&mut self, request: crate::vm::RouteRequest) -> Result<HostResponse, VmFault> {
        let mut staged = self.projected_world_state();
        let mut budget = self.script_budget();
        let result = crate::runtime_routes::handle_route(
            &mut staged,
            self.content,
            self.owner,
            request,
            &mut budget,
        );
        self.work.instructions = self.work.instructions.saturating_add(budget.used);
        let response = result?;
        self.commit_world_state(staged);
        Ok(response)
    }
}
