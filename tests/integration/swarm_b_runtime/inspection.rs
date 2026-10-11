//! Actual stored frames and source-computed registers in the real runtime.
use wonderland_content_runtime_bridge::{
    isolated::{IsolatedRuntime, StateWatch, WatchField, WatchValue},
    sim_core::{
        ids::{EntityRef, ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{
            FrameContext, RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmMode, VmRoutine,
        },
        world::{Facing, LotModel, TilePos},
    },
};

const OWNER: u32 = 0x1234;

fn fixture() -> (IsolatedRuntime, EntityRef) {
    fixture_with_identity(17, 3)
}

fn fixture_with_identity(lot: u64, epoch: u64) -> (IsolatedRuntime, EntityRef) {
    let key = RoutineKey {
        scope: RoutineScope::Private(OWNER),
        id: 4096,
    };
    let mut routines = RoutineStore::new();
    routines
        .insert(
            key,
            VmRoutine::new(
                4096,
                2,
                1,
                vec![VmInstruction {
                    opcode: 2,
                    true_pointer: 254,
                    false_pointer: 255,
                    operand: [0; 8],
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(OWNER, 2)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, lot, epoch, 42),
        RuntimeRole::Authority,
    )
    .unwrap();
    let spawn = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(50),
            avatar: true,
        })])
        .unwrap();
    runtime.step(&spawn).unwrap();
    let actor = runtime.state().entities[&ObjectId(1)].info.reference;
    let mut state = runtime.state().clone();
    let thread = state.threads.get_mut(&actor.object_id).unwrap();
    thread
        .push_entry(
            runtime.content().routines(),
            key,
            FrameContext::for_entity(actor, OWNER),
            vec![31],
        )
        .unwrap();
    thread.frames[0].locals[0] = 11;
    thread
        .push_entry(
            runtime.content().routines(),
            key,
            FrameContext::for_entity(actor, OWNER),
            vec![42],
        )
        .unwrap();
    thread.frames[1].locals[0] = 22;
    state
        .entities
        .get_mut(&actor.object_id)
        .unwrap()
        .avatar
        .as_mut()
        .unwrap()
        .person_data
        .values[58] = 25;
    // Global hour is derived from SimClock rather than this raw short.
    state.globals[0] = -123;
    let runtime =
        SimRuntime::from_state(state, runtime.content().clone(), RuntimeRole::Authority).unwrap();
    (IsolatedRuntime::capture(&runtime).unwrap(), actor)
}

#[test]
fn scoped_frame_watches_distinguish_stack_depth_and_argument_banks() {
    let (isolated, actor) = fixture();
    let before = isolated.runtime().snapshot().unwrap();
    for (field, expected) in [
        (WatchField::Local { depth: 0, index: 0 }, 11),
        (WatchField::Local { depth: 1, index: 0 }, 22),
        (WatchField::Argument { depth: 0, index: 0 }, 31),
        (WatchField::Argument { depth: 1, index: 0 }, 42),
        (WatchField::PersonData(58), 25),
        (WatchField::Global(0), 0),
    ] {
        assert_eq!(
            isolated
                .watch(StateWatch {
                    entity: actor,
                    field
                })
                .unwrap(),
            WatchValue::Scalar(expected)
        );
    }
    assert_eq!(isolated.runtime().snapshot().unwrap(), before);
}

#[test]
fn scoped_watches_reject_stale_generations_and_missing_banks() {
    let (isolated, actor) = fixture();
    for field in [
        WatchField::Local { depth: 2, index: 0 },
        WatchField::Argument { depth: 0, index: 1 },
        WatchField::PersonData(101),
        WatchField::Motive(16),
        WatchField::Global(38),
    ] {
        assert!(isolated
            .watch(StateWatch {
                entity: actor,
                field
            })
            .is_err());
    }
    assert!(isolated
        .watch(StateWatch {
            entity: EntityRef {
                generation: actor.generation + 1,
                ..actor
            },
            field: WatchField::Local { depth: 0, index: 0 }
        })
        .is_err());
}

#[test]
fn bounded_entity_inspection_contains_actual_frames_and_rejects_oversized_reports() {
    use wonderland_content_runtime_bridge::inspection::{
        inspect_entity_json, MAX_INSPECTION_JSON_BYTES,
    };
    let (isolated, actor) = fixture();
    let before = isolated.runtime().snapshot().unwrap();
    let bytes = inspect_entity_json(isolated.runtime(), actor, MAX_INSPECTION_JSON_BYTES).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["schema"], "wonderland.runtime-entity-inspection.v1");
    assert_eq!(value["completed_tick"], "1");
    assert_eq!(value["thread"]["frames"][0]["locals"][0], 11);
    assert_eq!(value["thread"]["frames"][1]["args"][0], 42);
    assert_eq!(value["thread"]["frames"][1]["context"]["code_owner"], OWNER);
    assert_eq!(value["active_queue_users"], serde_json::json!([]));
    assert_eq!(value["routes"], serde_json::json!([]));
    assert_eq!(
        inspect_entity_json(isolated.runtime(), actor, bytes.len()).unwrap(),
        bytes
    );
    assert!(inspect_entity_json(isolated.runtime(), actor, bytes.len() - 1).is_err());
    assert!(inspect_entity_json(isolated.runtime(), actor, 0).is_err());
    assert!(inspect_entity_json(
        isolated.runtime(),
        EntityRef {
            generation: actor.generation + 1,
            ..actor
        },
        MAX_INSPECTION_JSON_BYTES
    )
    .is_err());
    assert_eq!(isolated.runtime().snapshot().unwrap(), before);
}

#[test]
fn browser_inspection_preserves_full_width_u64_identity_as_decimal_strings() {
    use wonderland_content_runtime_bridge::inspection::{
        inspect_entity_json, MAX_INSPECTION_JSON_BYTES,
    };
    let (isolated, actor) = fixture_with_identity(9_007_199_254_740_993, u64::MAX);
    let bytes = inspect_entity_json(isolated.runtime(), actor, MAX_INSPECTION_JSON_BYTES).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["lot_id"], "9007199254740993");
    assert_eq!(value["authority_epoch"], "18446744073709551615");
    assert!(value["revision"].is_string());
    assert_eq!(value["entity"]["object_id"], 1);
    assert_eq!(value["entity"]["generation"], 1);
    assert_eq!(value["thread"]["frames"][0]["args"][0], 31);
    assert_eq!(
        inspect_entity_json(isolated.runtime(), actor, bytes.len()).unwrap(),
        bytes
    );
    assert!(inspect_entity_json(isolated.runtime(), actor, bytes.len() - 1).is_err());
}

#[test]
fn inspection_reports_actual_routes_reservations_and_advertisement_keys() {
    use wonderland_content_runtime_bridge::{
        inspection::{inspect_entity_json, MAX_INSPECTION_JSON_BYTES},
        sim_core::{
            vm::{EntityField, MemoryAddress, VmStop},
            world::{ReservationRequest, RouteGoal, SlotDefinition, SlotKey},
        },
    };
    let (isolated, actor) = fixture();
    let mut state = isolated.runtime().state().clone();
    let thread = state.threads.get_mut(&actor.object_id).unwrap();
    thread.frames.clear();
    thread.stop = VmStop::Ready;
    let mut runtime = SimRuntime::from_state(
        state,
        isolated.runtime().content().clone(),
        RuntimeRole::Authority,
    )
    .unwrap();
    let slot = SlotKey {
        owner: actor,
        index: 0,
    };
    let input = runtime
        .next_tick(vec![
            // The authored avatar has no init BHAV: explicitly allow ground/floor
            // movement through the real source memory projection before routing.
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: actor,
                    field: EntityField::ObjectData,
                    index: 4,
                },
                value: 1,
            },
            AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: actor,
                    field: EntityField::ObjectData,
                    index: 42,
                },
                value: 3,
            },
            AcceptedCommand::DefineSlot(SlotDefinition {
                key: slot,
                capacity: 1,
                height: 1,
                support_strength: 100,
                max_size: 100,
                revision: 0,
            }),
            AcceptedCommand::ReserveSlot(ReservationRequest {
                slot,
                actor,
                operation: u64::MAX,
                expected_revision: 1,
                tick: 2,
                duration_ticks: 30,
            }),
            AcceptedCommand::SetInteractionProjection {
                entity: actor,
                queued_users: [actor].into(),
                active_advertisements: Some([((0, 7), -16)].into()),
            },
            AcceptedCommand::BeginRoute {
                entity: actor,
                target: None,
                goals: vec![RouteGoal::point(TilePos::new(7, 7, 1).center())],
            },
        ])
        .unwrap();
    let outcome = runtime.step(&input).unwrap();
    let before = runtime.snapshot().unwrap();
    assert_eq!(runtime.state().continuations.len(), 1, "{outcome:?}");
    let value: serde_json::Value = serde_json::from_slice(
        &inspect_entity_json(&runtime, actor, MAX_INSPECTION_JSON_BYTES).unwrap(),
    )
    .unwrap();
    assert_eq!(value["routes"].as_array().unwrap().len(), 1);
    assert!(value["routes"][0]["continuation_id"].is_string());
    assert_eq!(
        value["active_advertisements"],
        serde_json::json!([{"kind":0,"index":7,"value":-16}])
    );
    assert_eq!(
        value["slot_state"][0]["reservations"][0]["token"]["operation"],
        "18446744073709551615"
    );
    assert_eq!(
        value["slot_state"][0]["reservations"][0]["expires_at"],
        "32"
    );
    assert_eq!(runtime.snapshot().unwrap(), before);
}

#[cfg(feature = "creator-debug")]
#[test]
fn creator_scoped_watch_grammar_is_canonical_and_matches_real_state() {
    use wonderland_content_runtime_bridge::creator_debug::{creator_entity, SimDebugProvider};
    use wonderland_content_runtime_bridge::sim_core::snapshot::SnapshotExpectation;
    use wonderland_creator::debug::{DebugSnapshot, IsolatedDebugProvider, Watch};
    let (isolated, actor) = fixture();
    let snapshot = DebugSnapshot {
        tick: 1,
        bytes: isolated.runtime().snapshot().unwrap(),
    };
    let provider = SimDebugProvider::new(
        isolated.runtime().content().clone(),
        SnapshotExpectation::new(17, 3),
    )
    .unwrap();
    let watch = |field: &str| Watch {
        entity: creator_entity(actor),
        field: field.to_string(),
    };
    for (field, expected) in [
        ("local/1/0", "22"),
        ("arg/0/0", "31"),
        ("person-data/58", "25"),
        ("global/0", "0"),
    ] {
        assert_eq!(
            provider.inspect(&snapshot, &[watch(field)]).unwrap()[0].value,
            expected
        );
    }
    for invalid in [
        "local/01/0",
        "local/1/00",
        "local/1",
        "arg/0/0/0",
        "local/-1/0",
        "person-data/+1",
        "global/038",
    ] {
        assert!(
            provider.inspect(&snapshot, &[watch(invalid)]).is_err(),
            "{invalid}"
        );
    }
}
