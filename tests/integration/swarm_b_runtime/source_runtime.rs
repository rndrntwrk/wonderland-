//! Real checked-in BHAV bytes, not a substitute interpreter or provider.
use sha2::{Digest, Sha256};
use wonderland_content_runtime_bridge::import_bhav;
use wonderland_content_runtime_bridge::{
    isolated::{IsolatedRuntime, RoutineQuery, StateWatch, WatchField, WatchValue},
    sim_core::{
        ids::{EntityRef, ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        snapshot::SnapshotExpectation,
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{FrameContext, PrimitiveExit, RoutineKey, RoutineScope, RoutineStore, VmMode, VmStop},
        world::{Facing, LotModel, TilePos},
    },
};
use wonderland_legacy_formats::{iff, Limits};

const BAR_PATH: &str = "TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff";

fn source_bar() -> iff::IffFile {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join(BAR_PATH)).unwrap();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        hash,
        "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865"
    );
    iff::decode(&bytes, &Limits::default()).unwrap()
}

const OWNER: u32 = 0x0478_6aed;

fn fixture(budget: u32) -> (SimRuntime, EntityRef) {
    let source = source_bar();
    let chunk = source
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BHAV" && c.key.id == 4110)
        .unwrap();
    let mut routines = RoutineStore::new();
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            import_bhav(chunk, &Limits::default()).unwrap(),
        )
        .unwrap();
    // The four-attribute owner and empty lot are a declared test harness. The
    // BHAV bytes and interpreter are real; no complete bar behavior is claimed.
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap();
    let mut config = RuntimeConfig::new(VmMode::Ts1, 11, 7, 123);
    config.limits.instruction_budget_per_entity = budget;
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(8, 8, 1).unwrap(),
        config,
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: OWNER,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let entity = runtime.state().entities[&ObjectId(1)].info.reference;
    (runtime, entity)
}

#[test]
fn real_source_query_discards_attribute_writes_and_rejects_stale_generations() {
    let (runtime, actor) = fixture(10);
    let original = runtime.snapshot().unwrap();
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    let mut request = RoutineQuery {
        actor,
        target: actor,
        code_owner: OWNER,
        routine_id: 4110,
        args: vec![],
        instruction_budget: 10,
    };
    let result = isolated.query(&request).unwrap();
    assert_eq!(result.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(result.instructions, 3);
    assert_eq!(isolated.runtime().snapshot().unwrap(), original);
    assert_eq!(
        runtime.state().entities[&ObjectId(1)].attributes,
        [10, 20, 30, 40]
    );
    request.target.generation += 1;
    assert!(isolated.query(&request).is_err());
    assert_eq!(runtime.snapshot().unwrap(), original);
}

#[test]
fn restored_snapshot_replays_the_unmodified_source_behavior_in_the_accepted_tail() {
    let (mut runtime, actor) = fixture(10);
    let bytes = runtime.snapshot().unwrap();
    let mut isolated = IsolatedRuntime::from_snapshot(
        &bytes,
        runtime.content().clone(),
        SnapshotExpectation::new(11, 7),
    )
    .unwrap();
    assert_eq!(isolated.runtime().role(), RuntimeRole::Replica);
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            context: FrameContext::for_entity(actor, OWNER),
            args: vec![],
            replace: true,
        }])
        .unwrap();
    let authoritative = runtime.step(&input).unwrap();
    let copied = isolated.advance(&input).unwrap();
    assert_eq!(copied.state_hash, authoritative.state_hash);
    assert!(copied.effects.is_empty());
    assert_eq!(
        isolated.runtime().state().entities[&ObjectId(1)].attributes,
        [0, 0, 30, 0]
    );
    assert_eq!(
        isolated.runtime().state().threads[&ObjectId(1)].stop,
        VmStop::Completed(PrimitiveExit::ReturnTrue)
    );
    let before = isolated.runtime().snapshot().unwrap();
    let mut stale = isolated.runtime().next_tick(vec![]).unwrap();
    stale.epoch += 1;
    assert!(isolated.advance(&stale).is_err());
    assert_eq!(isolated.runtime().snapshot().unwrap(), before);
}

#[test]
fn typed_watches_are_generation_and_index_checked_on_the_restored_state() {
    let (runtime, actor) = fixture(10);
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    assert_eq!(
        isolated
            .watch(StateWatch {
                entity: actor,
                field: WatchField::Attribute(3)
            })
            .unwrap(),
        WatchValue::Scalar(40)
    );
    assert!(isolated
        .watch(StateWatch {
            entity: actor,
            field: WatchField::Temp(20)
        })
        .is_err());
    let stale = EntityRef {
        generation: actor.generation + 1,
        ..actor
    };
    assert!(isolated
        .watch(StateWatch {
            entity: stale,
            field: WatchField::Attribute(0)
        })
        .is_err());
}

#[test]
fn dispatch_limit_remains_a_real_fault_and_frame_positions_are_not_a_trace() {
    let (mut runtime, actor) = fixture(1);
    let input = runtime
        .next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: RoutineKey {
                scope: RoutineScope::Private(OWNER),
                id: 4110,
            },
            context: FrameContext::for_entity(actor, OWNER),
            args: vec![],
            replace: true,
        }])
        .unwrap();
    runtime.step(&input).unwrap();
    let mut isolated = IsolatedRuntime::capture(&runtime).unwrap();
    assert!(matches!(
        isolated
            .watch(StateWatch {
                entity: actor,
                field: WatchField::Stop
            })
            .unwrap(),
        WatchValue::Stop(VmStop::Faulted(_))
    ));
    let positions = isolated.frame_positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].routine.id, 4110);
    assert_eq!(positions[0].instruction, 1);
    assert_eq!(positions[0].opcode, Some(2));
    let next = isolated.runtime().next_tick(vec![]).unwrap();
    isolated.advance(&next).unwrap();
    assert_eq!(
        isolated
            .watch(StateWatch {
                entity: actor,
                field: WatchField::Attribute(0)
            })
            .unwrap(),
        WatchValue::Scalar(10)
    );
}

#[cfg(feature = "creator-debug")]
#[test]
fn creator_provider_reads_real_snapshots_and_labels_tick_stepping_honestly() {
    use wonderland_content_runtime_bridge::creator_debug::{creator_entity, SimDebugProvider};
    use wonderland_creator::debug::{DebugSnapshot, IsolatedDebugProvider, Watch};
    let (runtime, actor) = fixture(10);
    let provider =
        SimDebugProvider::new(runtime.content().clone(), SnapshotExpectation::new(11, 7)).unwrap();
    let snapshot = DebugSnapshot {
        tick: runtime.state().completed_tick,
        bytes: runtime.snapshot().unwrap(),
    };
    let values = provider
        .inspect(
            &snapshot,
            &[Watch {
                entity: creator_entity(actor),
                field: "attribute/3".into(),
            }],
        )
        .unwrap();
    assert_eq!(values[0].value, "40");
    assert!(provider
        .inspect(
            &snapshot,
            &[Watch {
                entity: creator_entity(actor),
                field: "anything-executable()".into()
            }]
        )
        .is_err());
    assert!(provider
        .trace(&snapshot)
        .unwrap_err()
        .contains("unsupported"));
    let next = provider.step_isolated(&snapshot).unwrap();
    assert_eq!(next.tick, snapshot.tick + 1);
    assert_eq!(runtime.snapshot().unwrap(), snapshot.bytes);
    let mismatched = DebugSnapshot {
        tick: snapshot.tick + 1,
        bytes: snapshot.bytes,
    };
    assert!(provider.inspect(&mismatched, &[]).is_err());
}

#[test]
fn imported_source_bhav_retains_the_three_attribute_writes() {
    let file = source_bar();
    let source = file
        .chunks
        .iter()
        .find(|c| c.key.kind == *b"BHAV" && c.key.id == 4110)
        .unwrap();
    let routine = import_bhav(source, &Limits::default()).unwrap();
    assert_eq!(routine.id(), 4110);
    assert_eq!(routine.instructions().len(), 3);
    // Source BHAV 4110: attributes 3, 0, 1 become literal zero, in this order.
    assert_eq!(routine.instructions()[0].operand, [3, 0, 0, 0, 0, 5, 0, 7]);
    assert_eq!(routine.instructions()[1].operand, [0, 0, 0, 0, 0, 5, 0, 7]);
    assert_eq!(routine.instructions()[2].operand, [1, 0, 0, 0, 0, 5, 0, 7]);
    assert_eq!(routine.instructions()[2].true_pointer, 254);
}
