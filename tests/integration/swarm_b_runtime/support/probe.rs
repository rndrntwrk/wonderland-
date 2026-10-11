//! Reproducible probe of an unmodified source BHAV in an explicitly authored
//! four-attribute harness. No original asset bytes are emitted.
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use wonderland_content_runtime_bridge::{
    import_bhav,
    isolated::{IsolatedRuntime, RoutineQuery, StateWatch, WatchField, WatchValue},
    sim_core::{
        ids::{ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        snapshot::SnapshotExpectation,
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{FrameContext, PrimitiveExit, RoutineKey, RoutineScope, RoutineStore, VmMode, VmStop},
        world::{Facing, LotModel, TilePos},
    },
    SIM_CORE_REVISION,
};
use wonderland_legacy_formats::{iff, Limits};

pub const SOURCE_SHA256: &str = "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865";

pub fn run(source: &Path) -> Result<serde_json::Value, String> {
    let mut bytes = Vec::new();
    File::open(source)
        .map_err(|e| e.to_string())?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("source probe input limit".into());
    }
    let source_hash = format!("{:x}", Sha256::digest(&bytes));
    if source_hash != SOURCE_SHA256 {
        return Err("source probe requires the exact pinned casino bar IFF".into());
    }
    let limits = Limits::default();
    let file = iff::decode(&bytes, &limits).map_err(|e| e.to_string())?;
    let chunk = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BHAV" && c.key.id == 4110)
        .ok_or("source BHAV 4110 missing")?;
    const OWNER: u32 = 0x0478_6aed;
    let key = RoutineKey {
        scope: RoutineScope::Private(OWNER),
        id: 4110,
    };
    let mut routines = RoutineStore::new();
    routines
        .insert(key, import_bhav(chunk, &limits)?)
        .map_err(|e| e.to_string())?;
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default())?;
    let mut live = SimRuntime::new(
        content.clone(),
        LotModel::new(8, 8, 1).map_err(|e| format!("{e:?}"))?,
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .map_err(|e| e.to_string())?;
    let spawn = live
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .map_err(|e| e.to_string())?;
    live.step(&spawn).map_err(|e| e.to_string())?;
    let actor = live.state().entities[&ObjectId(1)].info.reference;
    let snapshot = live.snapshot().map_err(|e| e.to_string())?;
    let isolated = IsolatedRuntime::capture(&live)?;
    let query = isolated.query(&RoutineQuery {
        actor,
        target: actor,
        code_owner: OWNER,
        routine_id: 4110,
        args: vec![],
        instruction_budget: 10,
    })?;
    if query.stop != VmStop::Completed(PrimitiveExit::ReturnTrue) || query.instructions != 3 {
        return Err("source query diverged from the three original assignment instructions".into());
    }
    if isolated.runtime().snapshot().map_err(|e| e.to_string())? != snapshot {
        return Err("source query changed the captured snapshot".into());
    }
    let mut replay =
        IsolatedRuntime::from_snapshot(&snapshot, content, SnapshotExpectation::new(11, 7))?;
    let accepted = live
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: key,
            context: FrameContext::for_entity(actor, OWNER),
            args: vec![],
            replace: true,
        }])
        .map_err(|e| e.to_string())?;
    let live_output = live.step(&accepted).map_err(|e| e.to_string())?;
    let replay_output = replay.advance(&accepted)?;
    let watched = (0..4)
        .map(|index| {
            replay.watch(StateWatch {
                entity: actor,
                field: WatchField::Attribute(index),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let wanted = [0, 0, 30, 0].map(WatchValue::Scalar);
    if watched != wanted
        || replay_output.state_hash != live_output.state_hash
        || !replay_output.effects.is_empty()
    {
        return Err("source accepted-tail replay diverged".into());
    }
    if replay.runtime().state().completed_tick != 2 || replay_output.instructions != 3 {
        return Err("source replay tick or instruction count diverged".into());
    }
    let final_snapshot = replay.runtime().snapshot().map_err(|e| e.to_string())?;
    let final_hash = format!("{:x}", Sha256::digest(&final_snapshot));
    let restored = IsolatedRuntime::from_snapshot(
        &final_snapshot,
        replay.runtime().content().clone(),
        SnapshotExpectation::new(11, 7),
    )?;
    if restored.runtime().snapshot().map_err(|e| e.to_string())? != final_snapshot {
        return Err("completed source snapshot did not round trip".into());
    }
    Ok(serde_json::json!({
        "schema": "wonderland.runtime-source-probe.v1",
        "sim_revision": SIM_CORE_REVISION,
        "source_sha256": source_hash,
        "source_bhav": 4110,
        "harness": "four authored attributes; unmodified source BHAV; actual sim-core interpreter",
        "query": {"stop":"ReturnTrue", "instructions": query.instructions,
            "temps": query.temps, "temp_xl": query.temp_xl, "snapshot_unchanged": true},
        "replay": {"snapshot_tick": 1, "completed_tick": replay_output.tick,
            "instructions": replay_output.instructions, "attributes": [0,0,30,0],
            "snapshot_sha256": final_hash,
            "state_hash": replay_output.state_hash.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "authority_matches": true, "durable_dispatches": replay_output.effects.len(),
            "snapshot_roundtrip": true},
        "scope": "No full casino bar, queue, dialog, route, animation, or EOD completion claim"
    }))
}
