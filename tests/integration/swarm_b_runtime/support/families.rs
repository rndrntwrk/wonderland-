//! Generic source execution evidence, shared by native tests and the WASI
//! example. No source behavior is rewritten and no missing routine is filled.
//! Only source selection belongs to the family; the execution algorithm is
//! identical for every private BHAV and both caller contexts in both dialects.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};
use wonderland_content_ir::strings::{lookup_string, LocaleSelection};
use wonderland_content_runtime_bridge::{
    import_bhav,
    interactions::certify_read_only,
    isolated::{IsolatedRuntime, RoutineQuery},
    sim_core::{
        avatars::motives::TsoMotiveTuning,
        ids::{EntityRef, ObjectId, PersistentId},
        primitives::primitive_info,
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        snapshot::SnapshotExpectation,
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{FrameContext, RoutineKey, RoutineScope, RoutineStore, VmMode},
        world::{Facing, LotModel, TilePos},
    },
    SIM_CORE_REVISION,
};
use wonderland_legacy_formats::{
    iff::{self, IffChunk, IffFile},
    semantic, Limits,
};

pub struct Source {
    pub name: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub object_id: u16,
    pub guid: u32,
}
pub const SOURCES: [Source; 3] = [
    Source {
        name: "chair",
        file: "TSOClient/FSO.Content.TSO/Content/Objects/Chair_fso_Bouncy_Beach_Ball.iff",
        sha256: "ab9975d947e64ea19ac194679dee6080f99a5b4c0a4c89f03f719d75113d57a7",
        object_id: 16807,
        guid: 0x0462_db31,
    },
    Source {
        name: "bed",
        file: "TSOClient/FSO.Content.TSO/Content/Objects/k8capbedts.iff",
        sha256: "c8289640be973e53a79a7a0d4430809f8e306cc7e6b38bec0c7eaa187a5808bb",
        object_id: 16831,
        guid: 0xf8ea_4345,
    },
    Source {
        name: "appliance",
        file: "TSOClient/FSO.Content.TSO/Content/Objects/k8oblfridgehd.iff",
        sha256: "7a2eb22e4958dc12bce6b470a263e87ecfa0cd57019deafd3efb24270a41a145",
        object_id: 16809,
        guid: 0x46ea_4345,
    },
];

const AVATAR_GUID: u32 = 0xffff_fffe;
const LOT: u64 = 31;
const EPOCH: u64 = 7;
const BUDGET: u32 = 4096;
const INPUT_LIMIT: usize = 16 * 1024 * 1024;

pub struct FamilyHarness {
    baseline: SimRuntime,
    actor: EntityRef,
    object: EntityRef,
    owner: u32,
    routines: BTreeMap<u16, Value>,
    metadata: Value,
    unresolved: Vec<Value>,
}

fn chunk(file: &IffFile, kind: [u8; 4], id: u16) -> Option<&IffChunk> {
    file.chunks
        .iter()
        .find(|chunk| chunk.key.kind == kind && chunk.key.id == id)
}
fn label(chunk: &IffChunk) -> String {
    chunk
        .label
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(char::from)
        .collect()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl FamilyHarness {
    pub fn load(root: &Path, source: &Source, mode: VmMode) -> Result<Self, String> {
        let path = root.join(source.file);
        let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() > INPUT_LIMIT as u64 {
            return Err("source probe needs a bounded regular file".into());
        }
        let mut bytes = Vec::new();
        File::open(&path)
            .map_err(|error| error.to_string())?
            .take(INPUT_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > INPUT_LIMIT || digest(&bytes) != source.sha256 {
            return Err("source family requires its exact pinned IFF".into());
        }
        let limits = Limits {
            max_input_bytes: INPUT_LIMIT,
            max_total_decoded_bytes: 32 * 1024 * 1024,
            max_entries: 8192,
            ..Limits::default()
        };
        let file = iff::decode(&bytes, &limits).map_err(|error| error.to_string())?;
        let raw = chunk(&file, *b"OBJD", source.object_id).ok_or("selected OBJD missing")?;
        let version = u32::from_le_bytes(
            raw.data
                .get(..4)
                .ok_or("OBJD version missing")?
                .try_into()
                .unwrap(),
        );
        // Original OBJD.RawData fixed prefix; extra edit-preserved words are not
        // runtime fields. See the bridge's strict source importer for this map.
        let words = match version {
            136 => 78,
            138 => 93,
            139 => 94,
            140 | 141 => 95,
            142 => 103,
            _ => return Err("OBJD version cannot be represented".into()),
        };
        let objd =
            semantic::decode_objd(raw.data.get(..4 + words * 2).ok_or("short OBJD")?, &limits)
                .map_err(|error| error.to_string())?;
        if objd.guid() != source.guid || objd.guid() == AVATAR_GUID {
            return Err("selected source GUID changed".into());
        }
        let owner = objd.guid();
        let mut routines = BTreeMap::new();
        let mut store = RoutineStore::new();
        let mut rejected = Vec::new();
        let mut rejected_resources = Vec::new();
        let mut names = BTreeMap::new();
        let mut strings = Vec::new();
        let mut tuning = TuningSet::default();
        let mut primitive_requirements = BTreeMap::new();
        let mut attrs = usize::from(objd.field("NumAttributes").unwrap_or(0));
        // Neutral motive inputs are an explicit harness choice. They are not a
        // substitute for missing TSO global tuning in a gameplay content set.
        if mode == VmMode::Tso {
            let mut flat = [0; 12];
            flat[8] = 16;
            tuning.tso_motives = Some(TsoMotiveTuning {
                flat_sim: flat,
                category_weights: [[1000; 7]; 11],
            });
        }
        for resource in &file.chunks {
            match &resource.key.kind {
                b"BHAV" => {
                    if routines.len() + rejected.len() >= 512 {
                        return Err("source routine count limit".into());
                    }
                    if !(4096..8192).contains(&resource.key.id) {
                        rejected.push(json!({"id": resource.key.id, "reason": "private IFF contains a nonprivate routine ID"}));
                        continue;
                    }
                    match import_bhav(resource, &limits) {
                        Ok(routine) => {
                            let name: String = resource
                                .label
                                .iter()
                                .copied()
                                .take_while(|byte| *byte != 0)
                                .map(|byte| {
                                    if byte.is_ascii() {
                                        char::from(byte)
                                    } else {
                                        '?'
                                    }
                                })
                                .collect();
                            names.entry((owner, name)).or_insert(RoutineKey {
                                scope: RoutineScope::Private(owner),
                                id: resource.key.id,
                            });
                            for instruction in routine.instructions() {
                                if instruction.opcode < 256 {
                                    let info = primitive_info(mode, instruction.opcode);
                                    primitive_requirements.entry(instruction.opcode).or_insert_with(|| json!({"opcode": instruction.opcode, "handler": info.map(|value| value.handler), "registry_status": info.map(|value| format!("{:?}", value.status))}));
                                }
                            }
                            routines.insert(resource.key.id, json!({"id": resource.key.id, "label": label(resource), "sha256": digest(&resource.data), "instructions": routine.instructions().len(), "arguments": routine.arguments(), "locals": routine.locals()}));
                            store
                                .insert(
                                    RoutineKey {
                                        scope: RoutineScope::Private(owner),
                                        id: resource.key.id,
                                    },
                                    routine,
                                )
                                .map_err(|error| error.to_string())?;
                        }
                        Err(error) => {
                            rejected.push(json!({"id": resource.key.id, "reason": error}))
                        }
                    }
                }
                b"STR#" => {
                    let table = match semantic::decode_strings(&resource.data, &limits) {
                        Ok(table) => table,
                        Err(error) => {
                            // A diagnostic import keeps the unsupported source
                            // record visible and leaves its runtime table absent.
                            // It does not silently manufacture an empty table.
                            rejected_resources.push(json!({"kind": "STR#", "id": resource.key.id, "sha256": digest(&resource.data), "error": error.to_string()}));
                            continue;
                        }
                    };
                    let values: Vec<_> = (0..table.sets.first().map_or(0, Vec::len))
                        .filter_map(|index| {
                            lookup_string(&table, index, LocaleSelection::default())
                                .map(|selected| selected.item.value.text())
                        })
                        .collect();
                    if resource.key.id == 256 {
                        attrs = attrs.max(values.len());
                    }
                    strings.push(((owner, resource.key.id), values));
                }
                b"BCON" => {
                    let table = semantic::decode_bcon(&resource.data, &limits)
                        .map_err(|error| error.to_string())?;
                    for (index, value) in table.constants.into_iter().enumerate() {
                        tuning
                            .values
                            .insert((owner, resource.key.id, index as u16), value as i16);
                    }
                }
                _ => {}
            }
        }
        if attrs > 4096 {
            return Err("source attribute count cannot be represented".into());
        }
        let mut definition = ObjectDefinition::new(owner, attrs as u16);
        definition.definition = [((version % 0xffff) as u16) as i16, (version >> 16) as i16]
            .into_iter()
            .chain(objd.fields.iter().map(|word| *word as i16))
            .collect();
        definition.object_data[8] = 1 << 8;
        definition.animation_table_id = objd.field("AnimationTableID").unwrap_or(0);
        definition.body_string_id = objd.field("BodyStringID").unwrap_or(0);
        let slots = chunk(&file, *b"SLOT", objd.slot_id())
            .map(|raw| semantic::decode_slot(&raw.data, &limits).map_err(|error| error.to_string()))
            .transpose()?;
        if let Some(slot) = &slots {
            definition.slot_count = slot
                .slots
                .iter()
                .filter(|slot| slot.type_id == 0)
                .count()
                .try_into()
                .map_err(|_| "slot count")?;
        }
        let functions = if objd.field("UsesFnTable").unwrap_or(0) != 0 {
            let table = chunk(&file, *b"OBJf", source.object_id).ok_or("source OBJf missing")?;
            serde_json::to_value(
                semantic::decode_objf(&table.data, &limits)
                    .map_err(|error| error.to_string())?
                    .functions,
            )
            .map_err(|error| error.to_string())?
        } else {
            // Keep the exact named OBJD BHAV fields; do not install lifecycle
            // entries in this harness, because their dependencies are absent.
            Value::Array(
                semantic::OBJD_FIELDS
                    .iter()
                    .filter(|field| field.starts_with("BHAV_"))
                    .map(|field| json!({"field": field, "routine": objd.field(field).unwrap_or(0)}))
                    .collect(),
            )
        };
        let interactions = chunk(&file, *b"TTAB", objd.tree_table_id())
            .map(|raw| semantic::decode_ttab(&raw.data, &limits).map_err(|error| error.to_string()))
            .transpose()?;
        let global_names = file
            .chunks
            .iter()
            .filter(|chunk| chunk.key.kind == *b"GLOB")
            .map(|chunk| {
                semantic::decode_glob(&chunk.data, &limits).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let metadata = json!({
            "name": source.name, "source": source.file, "source_sha256": source.sha256,
            "guid": owner, "object_chunk_id": source.object_id, "object_version": version,
            "source_multitile_master": objd.master_id(), "source_subindex": objd.sub_index(),
            "mode": mode, "routine_count": routines.len(), "rejected_routines": rejected,
            "rejected_resources": rejected_resources,
            "source_entrypoints": functions, "source_interactions": interactions.map(|table| table.interactions),
            "source_globals": global_names, "source_slots": slots,
            "primitive_requirements": primitive_requirements.into_values().collect::<Vec<_>>(),
            "harness": "One isolated source object and one authored avatar; zeroed source-sized attributes and private BCON/STR resources; default rectangular footprint; lifecycle/autonomy disabled; TSO uses explicit neutral harness motive tuning; no global/semi routines, animation metadata, normalized routing slots, or multipart topology supplied",
            "complete_gameplay": false,
        });
        let mut avatar_definition = ObjectDefinition::new(AVATAR_GUID, 0);
        avatar_definition.object_data[4] = 1; // Explicit authored ground height.
        avatar_definition.object_data[42] = 3; // ON_FLOOR | ON_TERRAIN.
        let content = ContentSet::new(store, vec![definition, avatar_definition], vec![], tuning)?
            .with_strings(strings)?
            .with_named_trees(names.into_iter().collect())?;
        let mut unresolved = Vec::new();
        for &id in routines.keys() {
            let key = RoutineKey {
                scope: RoutineScope::Private(owner),
                id,
            };
            for (index, instruction) in content
                .routines()
                .get(key)
                .unwrap()
                .instructions()
                .iter()
                .enumerate()
            {
                if instruction.opcode >= 256
                    && content
                        .routines()
                        .resolve(owner, instruction.opcode)
                        .is_none()
                {
                    unresolved.push(json!({"routine": id, "instruction": index, "call": instruction.opcode, "namespace": if instruction.opcode < 4096 { "Global" } else if instruction.opcode < 8192 { "Private" } else { "SemiGlobal" }}));
                }
            }
        }
        let mut config = RuntimeConfig::new(mode, LOT, EPOCH, 123);
        config.limits.instruction_budget_per_entity = BUDGET;
        let mut baseline = SimRuntime::new(
            content,
            LotModel::new(8, 8, 1).map_err(|error| format!("{error:?}"))?,
            config,
            RuntimeRole::Authority,
        )
        .map_err(|error| error.to_string())?;
        let input = baseline
            .next_tick(vec![
                AcceptedCommand::Spawn(SpawnSpec {
                    guid: AVATAR_GUID,
                    position: TilePos::new(2, 3, 1).center(),
                    facing: Facing::NORTH,
                    persistent_id: PersistentId(1),
                    avatar: true,
                }),
                AcceptedCommand::Spawn(SpawnSpec {
                    guid: owner,
                    position: TilePos::new(4, 3, 1).center(),
                    facing: Facing::NORTH,
                    persistent_id: PersistentId(0),
                    avatar: false,
                }),
            ])
            .map_err(|error| error.to_string())?;
        baseline.step(&input).map_err(|error| error.to_string())?;
        let actor = baseline.state().entities[&ObjectId(1)].info.reference;
        let object = baseline.state().entities[&ObjectId(2)].info.reference;
        Ok(Self {
            baseline,
            actor,
            object,
            owner,
            routines,
            metadata,
            unresolved,
        })
    }

    pub fn unresolved_calls(&self) -> &[Value] {
        &self.unresolved
    }

    fn start(&self, routine_id: u16, avatar: bool) -> Result<AcceptedCommand, String> {
        let key = self
            .baseline
            .content()
            .routines()
            .resolve(self.owner, routine_id)
            .ok_or("source routine unavailable")?;
        let routine = self.baseline.content().routines().get(key).unwrap();
        let actor = if avatar { self.actor } else { self.object };
        Ok(AcceptedCommand::StartBehavior {
            entity: actor,
            routine: key,
            context: FrameContext {
                caller: actor,
                callee: self.object,
                stack_object: self.object.object_id,
                stack_object_ref: Some(self.object),
                code_owner: self.owner,
            },
            args: vec![0; usize::from(routine.arguments()).max(4)],
            replace: true,
        })
    }

    pub fn execute(&self, routine_id: u16, avatar: bool) -> Result<Value, String> {
        let isolated = IsolatedRuntime::capture(&self.baseline)?;
        let snapshot = isolated
            .runtime()
            .snapshot()
            .map_err(|error| error.to_string())?;
        let actor = if avatar { self.actor } else { self.object };
        let key = isolated
            .runtime()
            .content()
            .routines()
            .resolve(self.owner, routine_id)
            .ok_or("source routine unavailable")?;
        let routine = isolated.runtime().content().routines().get(key).unwrap();
        let result = isolated.query(&RoutineQuery {
            actor,
            target: self.object,
            code_owner: self.owner,
            routine_id,
            args: vec![0; usize::from(routine.arguments()).max(4)],
            instruction_budget: BUDGET,
        });
        if isolated
            .runtime()
            .snapshot()
            .map_err(|error| error.to_string())?
            != snapshot
        {
            return Err("real source query mutated its captured runtime".into());
        }
        let query = match result {
            Ok(result) => {
                json!({"stop": result.stop, "temps": result.temps, "temp_xl": result.temp_xl, "instructions": result.instructions, "diagnostics": result.diagnostics, "snapshot_unchanged": true})
            }
            Err(error) => json!({"error": error, "snapshot_unchanged": true}),
        };
        Ok(json!({
            "routine": self.routines[&routine_id], "context": if avatar { "avatar_to_object" } else { "object_self" },
            "read_only_proof": certify_read_only(self.baseline.content(), self.owner, routine_id),
            "query": query, "accepted": self.execute_tail(&[(routine_id, avatar)])?,
        }))
    }

    pub fn execute_tail(&self, entries: &[(u16, bool)]) -> Result<Value, String> {
        if entries.is_empty() || entries.len() > 32 {
            return Err("source tail must have 1..32 entries".into());
        }
        let mut live = SimRuntime::from_state(
            self.baseline.state().clone(),
            self.baseline.content().clone(),
            RuntimeRole::Authority,
        )
        .map_err(|error| error.to_string())?;
        let bytes = live.snapshot().map_err(|error| error.to_string())?;
        let mut replica = IsolatedRuntime::from_snapshot(
            &bytes,
            live.content().clone(),
            SnapshotExpectation::new(LOT, EPOCH),
        )?;
        let mut ticks = Vec::new();
        let mut accepted_error = None;
        for &(id, avatar) in entries {
            let input = live
                .next_tick(vec![self.start(id, avatar)?])
                .map_err(|error| error.to_string())?;
            let previous = live.snapshot().map_err(|error| error.to_string())?;
            match (live.step(&input), replica.advance(&input)) {
                (Ok(output), Ok(copied)) => {
                    if output.state_hash != copied.state_hash
                        || output.events != copied.events
                        || !copied.effects.is_empty()
                    {
                        return Err(
                            "source accepted tick differs between real authority and replica"
                                .into(),
                        );
                    }
                    ticks.push(json!({"tick": output.tick, "instructions": output.instructions, "state_hash": hex(&output.state_hash), "events": output.events.iter().map(|event| format!("{event:?}")).collect::<Vec<_>>(), "authority_external_requests": output.effects.len()}));
                }
                (Err(error), Err(copied)) => {
                    if error.to_string() != copied
                        || live.snapshot().map_err(|error| error.to_string())? != previous
                        || replica
                            .runtime()
                            .snapshot()
                            .map_err(|error| error.to_string())?
                            != previous
                    {
                        return Err("source rejected tick differs or mutates a runtime".into());
                    }
                    accepted_error = Some(error.to_string());
                    break;
                }
                _ => return Err("source authority/replica acceptance differs".into()),
            }
        }
        let final_snapshot = live.snapshot().map_err(|error| error.to_string())?;
        let restored = IsolatedRuntime::from_snapshot(
            &final_snapshot,
            live.content().clone(),
            SnapshotExpectation::new(LOT, EPOCH),
        )?;
        if restored
            .runtime()
            .snapshot()
            .map_err(|error| error.to_string())?
            != final_snapshot
        {
            return Err("source final snapshot fails round trip".into());
        }
        let (_, avatar) = entries.last().copied().unwrap();
        let runner = if avatar { self.actor } else { self.object };
        let thread = live.state().threads.get(&runner.object_id);
        Ok(json!({
            "ticks": ticks, "error": accepted_error, "stop": thread.map(|thread| &thread.stop),
            "diagnostics": thread.map(|thread| &thread.diagnostics), "frames": thread.map(|thread| &thread.frames),
            "actor": observe(&live, self.actor), "object": observe(&live, self.object),
            "rng_before": self.baseline.state().rng.state(), "rng_after": live.state().rng.state(),
            "authority_matches": true, "replica_external_dispatches": 0, "snapshot_roundtrip": true,
            "state_hash": hex(&live.state_hash().map_err(|error| error.to_string())?), "snapshot_sha256": digest(&final_snapshot),
        }))
    }
}

fn observe(runtime: &SimRuntime, entity: EntityRef) -> Value {
    match runtime.state().entities.get(&entity.object_id) {
        None => Value::Null,
        Some(item) => json!({"entity": item.info.reference, "position": item.info.position,
            "attributes": item.attributes, "object_data": item.object_data, "slots": item.slots,
            "motives": item.avatar.as_ref().map(|avatar| avatar.motives.values),
            "person_data": item.avatar.as_ref().map(|avatar| &avatar.person_data.values),
        }),
    }
}

pub fn run(root: &Path) -> Result<Value, String> {
    let mut families = Vec::new();
    for source in &SOURCES {
        for mode in [VmMode::Ts1, VmMode::Tso] {
            let harness = FamilyHarness::load(root, source, mode)?;
            let mut result = harness.metadata.clone();
            let mut executions = Vec::new();
            for id in harness.routines.keys() {
                for avatar in [false, true] {
                    executions.push(harness.execute(*id, avatar)?);
                }
            }
            result["unresolved_calls"] = json!(harness.unresolved_calls());
            result["executions"] = json!(executions);
            families.push(result);
        }
    }
    Ok(
        json!({"schema": "wonderland.runtime-source-families.v1", "sim_revision": SIM_CORE_REVISION,
        "source_revision": "4c6b3e8f5835b228723caea3c9f683c62f244f73", "families": families,
        "scope": "One real accepted tick per private routine/context and isolated synchronous queries. Stored frames and tick outcomes are not instruction traces. Full select/walk/reserve/use/animate/cancel gameplay remains unqualified."}),
    )
}
