//! Request bounds must be checked before cloning or restoring runtime state.
use wonderland_content_runtime_bridge::{
    import_bhav,
    isolated::{IsolatedRuntime, RoutineQuery},
    sim_core::{
        ids::{EntityRef, ObjectId, PersistentId},
        runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::{PrimitiveExit, RoutineKey, RoutineScope, RoutineStore, VmMode, VmRoutine, VmStop},
        world::{Facing, LotModel, TilePos},
    },
};
use wonderland_legacy_formats::{iff, Limits};

const OWNER: u32 = 0x0478_6aed;

fn fixture() -> (SimRuntime, EntityRef) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff");
    let source = iff::decode(&std::fs::read(path).unwrap(), &Limits::default()).unwrap();
    let chunk = source
        .chunks
        .iter()
        .find(|chunk| chunk.key.kind == *b"BHAV" && chunk.key.id == 4110)
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
    // An explicitly authored empty routine exercises the interpreter's empty
    // entry short circuit. The executable routine above uses real source bytes.
    routines
        .insert(
            RoutineKey {
                scope: RoutineScope::Global,
                id: 1,
            },
            VmRoutine::new(1, 0, 0, vec![]).unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap();
    let mut config = RuntimeConfig::new(VmMode::Ts1, 11, 7, 123);
    config.limits.instruction_budget_per_entity = 10;
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

fn query(actor: EntityRef) -> RoutineQuery {
    RoutineQuery {
        actor,
        target: actor,
        code_owner: OWNER,
        routine_id: 4110,
        args: vec![],
        instruction_budget: 10,
    }
}

#[test]
fn query_argument_limit_applies_even_to_an_empty_routine() {
    let (runtime, actor) = fixture();
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    let mut request = query(actor);
    request.routine_id = 1;
    request.args = vec![0; 256];
    let error = isolated.query(&request).unwrap_err();
    assert!(error.contains("argument"), "unexpected error: {error}");
}

#[test]
fn query_budget_is_checked_before_entity_or_routine_lookup() {
    let (runtime, actor) = fixture();
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    let mut request = query(actor);
    request.actor.generation += 1;
    request.routine_id = 4097;
    for budget in [0, 11, u32::MAX] {
        request.instruction_budget = budget;
        let error = isolated.query(&request).unwrap_err();
        assert!(
            error.contains("query instruction budget"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn query_accepts_the_argument_and_runtime_budget_boundaries() {
    let (runtime, actor) = fixture();
    let original = runtime.snapshot().unwrap();
    let isolated = IsolatedRuntime::capture(&runtime).unwrap();
    let mut request = query(actor);
    request.args = vec![0; 255];
    let result = isolated.query(&request).unwrap();
    assert_eq!(result.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(result.instructions, 3);
    request.instruction_budget = 1;
    assert!(isolated.query(&request).is_ok());
    assert_eq!(isolated.runtime().snapshot().unwrap(), original);
}

#[cfg(feature = "creator-debug")]
mod creator {
    use super::*;
    use wonderland_content_runtime_bridge::{
        creator_debug::{creator_entity, SimDebugProvider},
        sim_core::snapshot::SnapshotExpectation,
    };
    use wonderland_creator::debug::{DebugSnapshot, IsolatedDebugProvider, Watch};

    fn invalid_snapshot() -> DebugSnapshot {
        DebugSnapshot {
            tick: 1,
            bytes: b"invalid snapshot sentinel".to_vec(),
        }
    }

    #[test]
    fn oversized_watch_field_is_rejected_before_snapshot_restoration() {
        let (runtime, actor) = fixture();
        let provider =
            SimDebugProvider::new(runtime.content().clone(), SnapshotExpectation::new(11, 7))
                .unwrap();
        // Integer parsing accepts arbitrarily many leading zeroes unless the
        // request grammar applies its own bound before parsing or cloning.
        let watch = Watch {
            entity: creator_entity(actor),
            field: format!("temp/{}", "0".repeat(1 << 20)),
        };
        let error = provider.inspect(&invalid_snapshot(), &[watch]).unwrap_err();
        assert!(
            error.contains("watch field length"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn every_watch_is_parsed_before_snapshot_restoration() {
        let (runtime, actor) = fixture();
        let provider =
            SimDebugProvider::new(runtime.content().clone(), SnapshotExpectation::new(11, 7))
                .unwrap();
        for field in [
            "temp/+1",
            "temp/00",
            "temp/-1",
            "temp/",
            "temp/65536",
            "temp/1/2",
            "attribute/５",
            "temp/000001",
        ] {
            let watches = [
                Watch {
                    entity: creator_entity(actor),
                    field: "attribute/3".into(),
                },
                Watch {
                    entity: creator_entity(actor),
                    field: field.into(),
                },
            ];
            let error = provider.inspect(&invalid_snapshot(), &watches).unwrap_err();
            assert!(
                error.contains("watch index"),
                "field {field:?}: unexpected error: {error}"
            );
        }
    }

    #[test]
    fn every_watch_identity_is_validated_before_snapshot_restoration() {
        let (runtime, actor) = fixture();
        let provider =
            SimDebugProvider::new(runtime.content().clone(), SnapshotExpectation::new(11, 7))
                .unwrap();
        for entity in [0, 1, (1_u64 << 16) | 32768, 1_u64 << 48] {
            let watches = [
                Watch {
                    entity: creator_entity(actor),
                    field: "attribute/3".into(),
                },
                Watch {
                    entity,
                    field: "stop".into(),
                },
            ];
            let error = provider.inspect(&invalid_snapshot(), &watches).unwrap_err();
            assert!(
                error.contains("creator entity"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn unknown_watch_kind_is_rejected_before_snapshot_restoration() {
        let (runtime, actor) = fixture();
        let provider =
            SimDebugProvider::new(runtime.content().clone(), SnapshotExpectation::new(11, 7))
                .unwrap();
        let watches = [Watch {
            entity: creator_entity(actor),
            field: "execute/1".into(),
        }];
        let error = provider.inspect(&invalid_snapshot(), &watches).unwrap_err();
        assert!(
            error.contains("unknown watch field"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn bounded_watch_grammar_preserves_all_supported_fields() {
        let (runtime, actor) = fixture();
        let provider =
            SimDebugProvider::new(runtime.content().clone(), SnapshotExpectation::new(11, 7))
                .unwrap();
        let snapshot = DebugSnapshot {
            tick: 1,
            bytes: runtime.snapshot().unwrap(),
        };
        let watches: Vec<_> = [
            "attribute/3",
            "object-data/0",
            "temp/19",
            "temp-xl/1",
            "stop",
        ]
        .into_iter()
        .map(|field| Watch {
            entity: creator_entity(actor),
            field: field.into(),
        })
        .collect();
        let values = provider.inspect(&snapshot, &watches).unwrap();
        assert_eq!(
            values
                .iter()
                .map(|value| value.value.as_str())
                .collect::<Vec<_>>(),
            ["40", "0", "0", "0", "Completed(GotoFalse)"]
        );
        // Seventeen ASCII bytes is the maximum field grammar. Its u16 index
        // parses successfully, then the actual register bank rejects it.
        let error = provider
            .inspect(
                &snapshot,
                &[Watch {
                    entity: creator_entity(actor),
                    field: "object-data/65535".into(),
                }],
            )
            .unwrap_err();
        assert!(
            error.contains("stored register bank"),
            "unexpected error: {error}"
        );
    }
}
