//! A small, fully supplied behavior running in the same runtime used by replicas.
use sim_core::{
    ids::{ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{
        RoutineKey, RoutineScope, RoutineStore, Scope, Variable, VmInstruction, VmMode, VmRoutine,
    },
    world::{Facing, LotModel, TilePos},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let guid = 100;
    let main = RoutineKey {
        scope: RoutineScope::Private(guid),
        id: 4096,
    };
    let expression = |lhs, rhs, operator, next| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs,
                rhs,
                is_signed: 0,
                operator,
            }
            .encode(),
        )
    };
    let mut routines = RoutineStore::new();
    routines.insert(
        main,
        VmRoutine::new(
            4096,
            0,
            4,
            vec![
                expression(
                    Variable::new(Scope::MyObjectAttributes, 0),
                    Variable::new(Scope::Literal, 1),
                    3,
                    1,
                ),
                expression(
                    Variable::new(Scope::Parameters, 0),
                    Variable::new(Scope::Literal, 2),
                    5,
                    2,
                ),
                VmInstruction::new(0, 254, 255, [0; 8]),
            ],
        )?,
    )?;
    let mut object = ObjectDefinition::new(guid, 1);
    object.entry_points.insert(1, main);
    let content = ContentSet::new(routines, vec![object], vec![], TuningSet::default())
        .map_err(std::io::Error::other)?;
    let mut runtime = SimRuntime::new(
        content,
        LotModel::new(10, 10, 1).map_err(|error| std::io::Error::other(format!("{error:?}")))?,
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        RuntimeRole::Authority,
    )?;
    for tick in 1..=30 {
        let commands = if tick == 1 {
            vec![AcceptedCommand::Spawn(SpawnSpec {
                guid,
                position: TilePos::new(3, 3, 1).center(),
                facing: Facing::NORTH,
                persistent_id: PersistentId(0),
                avatar: false,
            })]
        } else {
            Vec::new()
        };
        let input = runtime.next_tick(commands)?;
        runtime.step(&input)?;
    }
    let snapshot = runtime.snapshot()?;
    let expected = runtime.state_hash()?;
    runtime.restore(&snapshot)?;
    assert_eq!(runtime.state_hash()?, expected);
    let hash: String = expected.iter().map(|byte| format!("{byte:02x}")).collect();
    println!(
        "tick={} attribute0={} snapshot_bytes={} state_hash={}",
        runtime.state().completed_tick,
        runtime.state().entities[&ObjectId(1)].attributes[0],
        snapshot.len(),
        hash
    );
    Ok(())
}
