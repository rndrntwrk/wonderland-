//! A valid snapshot must not amplify a large stored fault into an unbounded report.
use wonderland_content_runtime_bridge::{
    creator_debug::{creator_entity, SimDebugProvider},
    sim_core::{
        ids::{EntityRef, ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        snapshot::SnapshotExpectation,
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{RoutineStore, VmFault, VmMode, VmStop},
        world::{Facing, LotModel, TilePos},
    },
};
use wonderland_creator::debug::{DebugSnapshot, IsolatedDebugProvider, Watch, WatchValue};

const OUTPUT_BUDGET: usize = 1024 * 1024;

fn fixture(fault: VmFault) -> (SimDebugProvider, DebugSnapshot, EntityRef) {
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(123, 1)],
        vec![],
        TuningSet::default(),
    )
    .unwrap();
    let mut runtime = SimRuntime::new(
        content.clone(),
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: 123,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let entity = runtime.state().entities[&ObjectId(1)].info.reference;
    let mut state = runtime.state().clone();
    state.threads.get_mut(&entity.object_id).unwrap().stop = VmStop::Faulted(fault);
    let runtime = SimRuntime::from_state(state, content.clone(), RuntimeRole::Replica).unwrap();
    let snapshot = DebugSnapshot {
        tick: 1,
        bytes: runtime.snapshot().unwrap(),
    };
    let provider = SimDebugProvider::new(content, SnapshotExpectation::new(11, 7)).unwrap();
    (provider, snapshot, entity)
}

fn watch(entity: EntityRef, field: &str) -> Watch {
    Watch {
        entity: creator_entity(entity),
        field: field.into(),
    }
}

fn retained_bytes(values: &Vec<WatchValue>) -> usize {
    values.capacity() * std::mem::size_of::<WatchValue>()
        + values
            .iter()
            .map(|value| value.watch.field.capacity() + value.value.capacity())
            .sum::<usize>()
}

#[test]
fn repeated_large_stop_watches_reject_a_valid_snapshot_without_a_partial_report() {
    let (provider, snapshot, entity) = fixture(VmFault::HostUnsupported("x".repeat(64 * 1024)));
    assert!(snapshot.bytes.len() < 72 * 1024);
    let original = snapshot.clone();
    let watches = vec![watch(entity, "stop"); 128];
    match provider.inspect(&snapshot, &watches) {
        Err(error) => assert!(error.contains("output byte limit"), "{error}"),
        Ok(_) => panic!("an amplified report must fail before output materialization"),
    }
    assert_eq!(snapshot, original);
}

#[test]
fn bounded_reports_preserve_exact_escaped_stop_and_scalar_values() {
    let fault = VmFault::InvalidOperand {
        opcode: u16::MAX,
        detail: "\0\n\"\\€😀".repeat(128),
    };
    let expected = format!("{:?}", VmStop::Faulted(fault.clone()));
    let (provider, snapshot, entity) = fixture(fault);
    let watches = [watch(entity, "stop"), watch(entity, "attribute/0")];
    let values = provider.inspect(&snapshot, &watches).unwrap();
    assert_eq!(values[0].watch, watches[0]);
    assert_eq!(values[0].value, expected);
    assert_eq!(values[1].value, "0");
    assert_eq!(values[0].value.capacity(), expected.len());
    assert!(retained_bytes(&values) <= OUTPUT_BUDGET);
}

#[test]
fn exact_output_budget_accepts_its_boundary_and_rejects_one_additional_byte() {
    let fixed = std::mem::size_of::<WatchValue>()
        + "stop".len()
        + format!(
            "{:?}",
            VmStop::Faulted(VmFault::HostUnsupported(String::new()))
        )
        .len();
    let (provider, snapshot, entity) =
        fixture(VmFault::HostUnsupported("a".repeat(OUTPUT_BUDGET - fixed)));
    let values = provider
        .inspect(&snapshot, &[watch(entity, "stop")])
        .unwrap();
    assert_eq!(retained_bytes(&values), OUTPUT_BUDGET);
    let (provider, snapshot, entity) = fixture(VmFault::HostUnsupported(
        "a".repeat(OUTPUT_BUDGET - fixed + 1),
    ));
    match provider.inspect(&snapshot, &[watch(entity, "stop")]) {
        Err(error) => assert!(error.contains("output byte limit"), "{error}"),
        Ok(_) => panic!("one byte beyond the report budget must fail"),
    }
}
