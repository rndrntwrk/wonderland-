//! DOM-free integration of the pinned A simulation and B original-content loader.
//! Authority means permission to admit this local deterministic command stream;
//! it does not authenticate an account or decode the original FreeSO network ABI.
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
use sim_core::state::{ContentSet, InteractionTable, LifecyclePhase};
use wonderland_content_runtime_bridge::{
    content::{ImportedContent, ImportedInteraction},
    cooked::LoadedRelease,
};
pub mod live_session;
mod world_projection;

pub use sim_core;
pub use sim_core::{
    ids::{EntityRef, ObjectId, PersistentId},
    interactions::{
        self, ActionId, CancelIntent, InteractionIntent, OfferBatch, PrincipalKey, QueryOptions,
    },
    runtime::interaction_adapter::{InteractionAccess, entity_key, entity_ref},
    runtime::{
        AcceptedCommand, AcceptedTick, RuntimeConfig, RuntimeError, RuntimeEvent, RuntimeRole,
        SimRuntime, SpawnSpec, TickOutcome,
    },
    vm::{VmMode, VmStop},
    world::{Facing, LotModel, LotPosition, TilePos, WorldState},
};
pub use wonderland_content_runtime_bridge as content_bridge;
pub use wonderland_world_view as world_view;

#[derive(Debug)]
pub enum GameRuntimeError {
    Runtime(RuntimeError),
    Interaction(interactions::Error),
    Content(String),
    ReplicaCannotAdmit,
}
impl std::fmt::Display for GameRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for GameRuntimeError {}
impl From<RuntimeError> for GameRuntimeError {
    fn from(e: RuntimeError) -> Self {
        Self::Runtime(e)
    }
}
impl From<interactions::Error> for GameRuntimeError {
    fn from(e: interactions::Error) -> Self {
        Self::Interaction(e)
    }
}

pub struct GameRuntime {
    sim: SimRuntime,
}
impl GameRuntime {
    /// Trusted source-content entry. Runtime metadata and non-BCON tuning must be
    /// supplied by the loader; TSO avatar spawn rejects absent original tuning.
    pub fn new(
        content: ContentSet,
        lot: LotModel,
        config: RuntimeConfig,
        role: RuntimeRole,
    ) -> Result<Self, GameRuntimeError> {
        Ok(Self {
            sim: SimRuntime::new(content, lot, config, role)?,
        })
    }
    pub fn from_imported(
        imported: ImportedContent,
        lot: LotModel,
        config: RuntimeConfig,
        role: RuntimeRole,
    ) -> Result<Self, GameRuntimeError> {
        let content = attach_tables(
            imported.content,
            imported.objects.iter().map(|object| {
                (
                    object.guid,
                    object.interactions.as_deref(),
                    object.global_interactions.as_slice(),
                )
            }),
        )?;
        Self::new(content, lot, config, role)
    }
    pub fn from_cooked(
        loaded: LoadedRelease,
        lot: LotModel,
        config: RuntimeConfig,
        role: RuntimeRole,
    ) -> Result<Self, GameRuntimeError> {
        let content = attach_tables(
            loaded.content,
            loaded.report.objects.iter().map(|object| {
                (
                    object.guid,
                    object.interactions.as_deref(),
                    object.global_interactions.as_slice(),
                )
            }),
        )?;
        Self::new(content, lot, config, role)
    }
    pub fn sim(&self) -> &SimRuntime {
        &self.sim
    }
    pub fn advance(
        &mut self,
        commands: Vec<AcceptedCommand>,
    ) -> Result<TickOutcome, GameRuntimeError> {
        if self.sim.role() != RuntimeRole::Authority {
            return Err(GameRuntimeError::ReplicaCannotAdmit);
        }
        let accepted = self.sim.next_tick(commands)?;
        self.apply_accepted(&accepted)
    }
    pub fn apply_accepted(
        &mut self,
        accepted: &AcceptedTick,
    ) -> Result<TickOutcome, GameRuntimeError> {
        Ok(self.sim.step(accepted)?)
    }
    pub fn offers(
        &self,
        principal: PrincipalKey,
        actor: EntityRef,
        target: EntityRef,
        options: QueryOptions,
    ) -> Result<OfferBatch, GameRuntimeError> {
        Ok(self
            .sim
            .interaction_offers(principal, actor, target, options)?)
    }
    pub fn invoke(&mut self, intent: InteractionIntent) -> Result<TickOutcome, GameRuntimeError> {
        self.advance(vec![AcceptedCommand::QueueInteraction(intent)])
    }
    pub fn cancel(&mut self, intent: CancelIntent) -> Result<TickOutcome, GameRuntimeError> {
        self.advance(vec![AcceptedCommand::CancelInteraction(intent)])
    }
    pub fn preview_build(
        &self,
        intent: &sim_core::world::build::BuildIntent,
        authority: &sim_core::world::build::BuildAuthority,
    ) -> Result<sim_core::world::build::BuildPreview, GameRuntimeError> {
        Ok(self.sim.preview_build(intent, authority)?)
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, GameRuntimeError> {
        Ok(self.sim.snapshot()?)
    }
    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), GameRuntimeError> {
        Ok(self.sim.restore(bytes)?)
    }
    pub fn projection(&self) -> RuntimeProjection {
        let state = self.sim.state();
        RuntimeProjection {
            lot_id: state.lot_id,
            epoch: state.authority_epoch,
            tick: state.completed_tick,
            mode: state.mode,
            architecture_revision: state.world.lot.revision().architecture,
            entities: state
                .entities
                .values()
                .map(|item| EntityProjection {
                    reference: item.info.reference,
                    guid: item.info.guid,
                    persistent_id: item.info.persistent_id,
                    position: LotPosition::new(
                        i32::from(item.info.position.x),
                        i32::from(item.info.position.y),
                        item.info.position.level as u8,
                    ),
                    facing: Facing(item.info.direction),
                    lifecycle: item.lifecycle,
                    raw_motives: item.avatar.as_ref().map(|a| a.motives.values),
                    needs: item
                        .avatar
                        .as_ref()
                        .map(|a| NeedsProjection::from_motives(a.motives.values)),
                    thread: state
                        .threads
                        .get(&item.info.reference.object_id)
                        .map(|t| t.stop.clone()),
                })
                .collect(),
            queues: state
                .interaction_queues
                .values()
                .map(|queue| QueueProjection {
                    actor: entity_ref(queue.owner()).expect("validated runtime queue owner"),
                    revision: queue.revision(),
                    entries: queue
                        .entries()
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| queue.entry_visible(*i) == Some(true))
                        .map(|(i, entry)| QueueEntryProjection {
                            id: entry.id.0,
                            target: entity_ref(entry.invocation.target)
                                .expect("validated source queue target width"),
                            interaction: entry.invocation.definition.key,
                            param0: entry.invocation.args[0],
                            label: entry.invocation.definition.label.clone(),
                            active: i < queue.active_len(),
                            cancellation_requested: entry.notify_idle
                                || (i < queue.active_len() && queue.interaction_cancelled()),
                        })
                        .collect(),
                })
                .collect(),
            world: state.world.clone(),
        }
    }
}
fn attach_tables<'a>(
    mut content: ContentSet,
    objects: impl Iterator<
        Item = (
            u32,
            Option<&'a [ImportedInteraction]>,
            &'a [ImportedInteraction],
        ),
    >,
) -> Result<ContentSet, GameRuntimeError> {
    let mut tables = Vec::new();
    let mut advertisements = Vec::new();
    for (guid, interactions, global) in objects {
        if content.interaction_table(guid).is_some() {
            continue;
        }
        let definitions = interactions
            .unwrap_or(&[])
            .iter()
            .map(|entry| (interactions::InteractionScope::Local, entry))
            .chain(
                global
                    .iter()
                    .map(|entry| (interactions::InteractionScope::Global, entry)),
            )
            .map(|(scope, entry)| {
                let key = interactions::InteractionKey {
                    tta_index: entry.tta_index,
                    scope,
                };
                advertisements.push(((guid, key), entry.advertisement.clone()));
                interactions::InteractionDefinition {
                    key,
                    action: interactions::RoutineBinding {
                        routine_id: entry.action.id,
                        code_owner_guid: entry.code_owner,
                    },
                    check: entry.check.map(|check| interactions::RoutineBinding {
                        routine_id: check.id,
                        code_owner_guid: entry.code_owner,
                    }),
                    flags: interactions::ActionFlags(entry.flags),
                    permissions: interactions::PermissionFlags(entry.permissions),
                    label: entry.label.clone(),
                }
            })
            .collect();
        tables.push((
            guid,
            InteractionTable {
                local_table_present: interactions.is_some(),
                definitions,
            },
        ));
    }
    if !tables.is_empty() {
        content = content
            .with_interaction_tables(tables)
            .map_err(GameRuntimeError::Content)?;
    }
    if !advertisements.is_empty() {
        content = content
            .with_interaction_advertisements(advertisements)
            .map_err(GameRuntimeError::Content)?;
    }
    Ok(content)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeProjection {
    pub lot_id: u64,
    pub epoch: u64,
    pub tick: u64,
    pub mode: VmMode,
    pub architecture_revision: u64,
    pub entities: Vec<EntityProjection>,
    pub queues: Vec<QueueProjection>,
    pub world: WorldState,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityProjection {
    pub reference: EntityRef,
    pub guid: u32,
    pub persistent_id: u32,
    pub position: LotPosition,
    pub facing: Facing,
    pub lifecycle: LifecyclePhase,
    pub raw_motives: Option<[i16; 16]>,
    pub needs: Option<NeedsProjection>,
    pub thread: Option<VmStop>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsProjection {
    pub energy: u8,
    pub comfort: u8,
    pub hunger: u8,
    pub hygiene: u8,
    pub bladder: u8,
    pub social: u8,
    pub fun: u8,
    /// A's inherited room-score constant has no source environment evidence.
    /// A live room-environment provider is required before exposing this need.
    pub room: Option<u8>,
}
impl NeedsProjection {
    fn from_motives(raw: [i16; 16]) -> Self {
        let percent = |i| ((i32::from(raw[i]) + 100) / 2).clamp(0, 100) as u8;
        Self {
            energy: percent(5),
            comfort: percent(6),
            hunger: percent(7),
            hygiene: percent(8),
            bladder: percent(9),
            social: percent(14),
            fun: percent(15),
            room: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueProjection {
    pub actor: EntityRef,
    pub revision: u64,
    pub entries: Vec<QueueEntryProjection>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntryProjection {
    pub id: u64,
    pub target: EntityRef,
    pub interaction: interactions::InteractionKey,
    pub param0: i16,
    pub label: Option<String>,
    pub active: bool,
    pub cancellation_requested: bool,
}
