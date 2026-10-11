//! A bounded, explicit empty-lot harness for verified cooked content.
//!
//! Both runtimes have the Replica role and no external effect dispatcher. This
//! exercises a query and one accepted tick, not a server command-admission API,
//! queue/check-tree provider, instruction stepper or complete gameplay harness.
use crate::{
    budget::ImportBudget,
    cooked::{json_report, CookedReleaseReport, LoadedRelease},
    cooked_json,
    isolated::{IsolatedRuntime, RoutineQuery},
    SIM_CORE_REVISION,
};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use sim_core::{
    ids::PersistentId,
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    snapshot::SnapshotExpectation,
    vm::{EntityField, FrameContext, MemoryAddress, VmDiagnostic, VmMode, VmStop},
    world::{Facing, LotModel, TilePos},
};
use std::fmt;
use wonderland_content_ir::manifest::Digest;
use wonderland_legacy_formats::Limits;

pub const MAX_SCENARIO_BYTES: usize = 65_536;
pub const MAX_SCENARIO_ALLOCATION_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReplayScenarioV1 {
    pub schema_version: u32,
    pub mode: String,
    pub lot_id: u64,
    pub authority_epoch: u64,
    pub seed: u64,
    pub lot_width: u16,
    pub lot_height: u16,
    pub lot_levels: u8,
    pub object_guid: u32,
    pub tile_x: i16,
    pub tile_y: i16,
    pub level: i8,
    pub facing: u8,
    pub initial_attributes: Vec<i16>,
    pub routine_id: u16,
    pub args: Vec<i16>,
    pub instruction_budget: u32,
}
impl<'de> Deserialize<'de> for ReplayScenarioV1 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            schema_version: u32,
            mode: String,
            lot_id: u64,
            authority_epoch: u64,
            seed: u64,
            lot_width: u16,
            lot_height: u16,
            lot_levels: u8,
            object_guid: u32,
            tile_x: i16,
            tile_y: i16,
            level: i8,
            facing: u8,
            initial_attributes: Vec<i16>,
            routine_id: u16,
            args: Vec<i16>,
            instruction_budget: u32,
        }
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = ReplayScenarioV1;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a replay scenario object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let Fields {
                    schema_version,
                    mode,
                    lot_id,
                    authority_epoch,
                    seed,
                    lot_width,
                    lot_height,
                    lot_levels,
                    object_guid,
                    tile_x,
                    tile_y,
                    level,
                    facing,
                    initial_attributes,
                    routine_id,
                    args,
                    instruction_budget,
                } = Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(ReplayScenarioV1 {
                    schema_version,
                    mode,
                    lot_id,
                    authority_epoch,
                    seed,
                    lot_width,
                    lot_height,
                    lot_levels,
                    object_guid,
                    tile_x,
                    tile_y,
                    level,
                    facing,
                    initial_attributes,
                    routine_id,
                    args,
                    instruction_budget,
                })
            }
        }
        d.deserialize_map(Object)
    }
}
impl ReplayScenarioV1 {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let limits = Limits {
            max_total_decoded_bytes: MAX_SCENARIO_ALLOCATION_BYTES,
            ..Limits::default()
        };
        let mut budget = ImportBudget::new(&limits);
        let request: Self = cooked_json::decode(bytes, MAX_SCENARIO_BYTES, &mut budget)?;
        request.validate()?;
        Ok(request)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || !matches!(self.mode.as_str(), "ts1" | "tso") {
            return Err("unsupported replay scenario version or mode".into());
        }
        if self.lot_id == 0
            || self.authority_epoch == 0
            || self.object_guid == 0
            || !(1..=64).contains(&self.lot_width)
            || !(1..=64).contains(&self.lot_height)
            || !(1..=8).contains(&self.lot_levels)
            || self.tile_x < 0
            || self.tile_y < 0
            || self.tile_x as u16 >= self.lot_width
            || self.tile_y as u16 >= self.lot_height
            || self.level < 1
            || self.level as u8 > self.lot_levels
            || self.facing >= 8
        {
            return Err("replay scenario identity, lot or position is outside its bounds".into());
        }
        if self.initial_attributes.len() > 4096
            || self.args.len() > 255
            || !(1..=100_000).contains(&self.instruction_budget)
        {
            return Err("replay attributes, arguments or instruction budget exceed limits".into());
        }
        Ok(())
    }
}

/// Run from sealed content and return a bounded JSON report of actual VM results.
/// Snapshot and state comparisons are checked before success, including on a
/// fault/yield result. A result describes this one tick and does not imply the
/// behavior reached completion.
pub fn replay_json(loaded: LoadedRelease, request: &ReplayScenarioV1) -> Result<Vec<u8>, String> {
    request.validate()?;
    let LoadedRelease { content, report } = loaded;
    let object = content
        .object(request.object_guid)
        .ok_or("scenario object is absent from the verified content")?;
    if object.attributes.len() != request.initial_attributes.len() {
        return Err(
            "scenario initial attributes must exactly cover the object's attribute bank".into(),
        );
    }
    let routine = content
        .routines()
        .resolve(request.object_guid, request.routine_id)
        .ok_or("scenario routine is absent from its required namespace")?;
    let mode = if request.mode == "ts1" {
        VmMode::Ts1
    } else {
        VmMode::Tso
    };
    let mut config =
        RuntimeConfig::new(mode, request.lot_id, request.authority_epoch, request.seed);
    config.limits.max_entities = 256;
    config.limits.max_commands_per_tick = 4096;
    config.limits.max_continuations = 256;
    config.limits.instruction_budget_per_entity = request.instruction_budget;
    config.limits.max_tick_instructions = 100_000;
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(request.lot_width, request.lot_height, request.lot_levels)
            .map_err(|e| format!("lot: {e:?}"))?,
        config,
        RuntimeRole::Replica,
    )
    .map_err(|e| e.to_string())?;
    let input = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: request.object_guid,
            position: TilePos::new(request.tile_x, request.tile_y, request.level as u8).center(),
            facing: Facing(request.facing),
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .map_err(|e| e.to_string())?;
    let spawned = runtime.step(&input).map_err(|e| e.to_string())?;
    let actor = spawned
        .events
        .iter()
        .find_map(|event| match event {
            RuntimeEvent::Spawned(entity)
                if runtime.state().ids.is_live(*entity)
                    && runtime
                        .state()
                        .entities
                        .get(&entity.object_id)
                        .is_some_and(|object| object.info.guid == request.object_guid) =>
            {
                Some(*entity)
            }
            _ => None,
        })
        .ok_or("scenario object did not survive the spawn tick")?;
    let commands = request
        .initial_attributes
        .iter()
        .enumerate()
        .map(|(index, value)| AcceptedCommand::WriteMemory {
            address: MemoryAddress::Entity {
                entity: actor,
                field: EntityField::Attribute,
                index: index as u16,
            },
            value: *value,
        })
        .collect();
    let input = runtime.next_tick(commands).map_err(|e| e.to_string())?;
    runtime.step(&input).map_err(|e| e.to_string())?;
    let snapshot = runtime.snapshot().map_err(|e| e.to_string())?;
    let snapshot_tick = runtime.state().completed_tick;
    let mut replica = IsolatedRuntime::from_snapshot(
        &snapshot,
        runtime.content().clone(),
        SnapshotExpectation::new(request.lot_id, request.authority_epoch),
    )?;
    let query = replica.query(&RoutineQuery {
        actor,
        target: actor,
        code_owner: request.object_guid,
        routine_id: request.routine_id,
        args: request.args.clone(),
        instruction_budget: request.instruction_budget,
    })?;
    if replica.runtime().snapshot().map_err(|e| e.to_string())? != snapshot {
        return Err("isolated query mutated its input snapshot".into());
    }
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine,
            context: FrameContext::for_entity(actor, request.object_guid),
            args: request.args.clone(),
            replace: true,
        }])
        .map_err(|e| e.to_string())?;
    let original = runtime.step(&input).map_err(|e| e.to_string())?;
    let replayed = replica.advance(&input)?;
    let snapshot_after = replica.runtime().snapshot().map_err(|e| e.to_string())?;
    if original.state_hash != replayed.state_hash
        || runtime.snapshot().map_err(|e| e.to_string())? != snapshot_after
        || !replayed.effects.is_empty()
    {
        return Err("isolated accepted-tick replay did not match the source state".into());
    }
    let state = replica.runtime().state();
    if !state.ids.is_live(actor) {
        return Err("scenario object was deleted during the behavior tick".into());
    }
    let object = state
        .entities
        .get(&actor.object_id)
        .ok_or("scenario object was deleted during the behavior tick")?;
    let thread = state
        .threads
        .get(&actor.object_id)
        .ok_or("scenario thread is absent after the behavior tick")?;
    #[derive(Serialize)]
    struct Query<'a> {
        stop: &'a VmStop,
        temps: &'a [i16; 20],
        temp_xl: &'a [i32; 2],
        instructions: u32,
        diagnostics: &'a [VmDiagnostic],
        snapshot_unchanged: bool,
    }
    #[derive(Serialize)]
    struct Replay<'a> {
        snapshot_tick: u64,
        tick: u64,
        instructions: u32,
        stop: &'a VmStop,
        attributes: &'a [i16],
        effect_dispatches: usize,
        snapshots_match: bool,
        state_hash: String,
        snapshot_sha256: Digest,
    }
    #[derive(Serialize)]
    struct Report<'a> {
        schema: &'static str,
        sim_revision: &'static str,
        release: &'a CookedReleaseReport,
        query: Query<'a>,
        replay: Replay<'a>,
    }
    let state_hash = replayed
        .state_hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    json_report(&Report {
        schema: "wonderland.cooked-replay.v1",
        sim_revision: SIM_CORE_REVISION,
        release: &report,
        query: Query {
            stop: &query.stop,
            temps: &query.temps,
            temp_xl: &query.temp_xl,
            instructions: query.instructions,
            diagnostics: &query.diagnostics,
            snapshot_unchanged: true,
        },
        replay: Replay {
            snapshot_tick,
            tick: replayed.tick,
            instructions: replayed.instructions,
            stop: &thread.stop,
            attributes: &object.attributes,
            effect_dispatches: replayed.effects.len(),
            snapshots_match: true,
            state_hash,
            snapshot_sha256: Digest::of(&snapshot_after),
        },
    })
}
