//! Real accepted-command journey for a synchronous source controller callback.
//! This fixture proves the callback seam, not player admission or authentication.
use bincode::Options;
use sim_core::{
    ids::{EntityRef, ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{RoutineKey, RoutineScope, RoutineStore, Scope, Variable, VmInstruction, VmMode},
    world::{Facing, LotModel, LotPosition},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = RoutineKey {
        scope: RoutineScope::Private(100),
        id: 4096,
    };
    let assign = |lhs, rhs, next| {
        VmInstruction::new(
            2,
            next,
            255,
            ExpressionOperand {
                lhs,
                rhs,
                is_signed: 0,
                operator: 5,
            }
            .encode(),
        )
    };
    let mut routines = RoutineStore::new();
    routines.insert(
        key,
        sim_core::vm::VmRoutine::new(
            4096,
            0,
            4,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Parameters, 0),
                    1,
                ),
                assign(
                    Variable::new(Scope::Global, 21),
                    Variable::new(Scope::Temps, 0),
                    2,
                ),
                assign(
                    Variable::new(Scope::Global, 22),
                    Variable::new(Scope::TempXl, 0),
                    3,
                ),
                assign(
                    Variable::new(Scope::Temps, 0),
                    Variable::new(Scope::Literal, 99),
                    4,
                ),
                assign(
                    Variable::new(Scope::TempXl, 0),
                    Variable::new(Scope::Literal, 444),
                    255,
                ),
            ],
        )?,
    )?;
    let other_key = RoutineKey {
        scope: RoutineScope::Private(200),
        id: 4096,
    };
    routines.insert(
        other_key,
        sim_core::vm::VmRoutine::new(
            4096,
            0,
            4,
            vec![assign(
                Variable::new(Scope::Global, 20),
                Variable::new(Scope::Literal, 666),
                254,
            )],
        )?,
    )?;
    let empty_key = RoutineKey {
        scope: RoutineScope::Private(100),
        id: 4097,
    };
    routines.insert(empty_key, sim_core::vm::VmRoutine::new(4097, 0, 4, vec![])?)?;
    let low_key = RoutineKey {
        scope: RoutineScope::Global,
        id: 255,
    };
    routines.insert(
        low_key,
        sim_core::vm::VmRoutine::new(
            255,
            0,
            4,
            vec![assign(
                Variable::new(Scope::Global, 20),
                Variable::new(Scope::Literal, 667),
                254,
            )],
        )?,
    )?;
    let main_key = RoutineKey {
        scope: RoutineScope::Private(100),
        id: 4100,
    };
    routines.insert(
        main_key,
        sim_core::vm::VmRoutine::new(
            4100,
            0,
            4,
            vec![
                assign(
                    Variable::new(Scope::TempXl, 0),
                    Variable::new(Scope::Literal, 321),
                    1,
                ),
                assign(
                    Variable::new(Scope::Temps, 0),
                    Variable::new(Scope::Literal, 73),
                    2,
                ),
                VmInstruction::new(0, 254, 255, [0; 8]),
            ],
        )?,
    )?;
    let fault_key = RoutineKey {
        scope: RoutineScope::Private(100),
        id: 4101,
    };
    routines.insert(
        fault_key,
        sim_core::vm::VmRoutine::new(
            4101,
            0,
            4,
            vec![
                assign(
                    Variable::new(Scope::Global, 20),
                    Variable::new(Scope::Literal, 888),
                    1,
                ),
                assign(
                    Variable::new(Scope::Global, 21),
                    Variable::new(Scope::Global, 32767),
                    254,
                ),
            ],
        )?,
    )?;
    let content = ContentSet::new(
        routines,
        vec![ObjectDefinition::new(100, 1), ObjectDefinition::new(200, 1)],
        vec![],
        TuningSet::default(),
    )
    .map_err(std::io::Error::other)?
    .with_named_trees(vec![
        ((100, "CT - FSO Player Joined".into()), other_key),
        ((100, "Empty callback".into()), empty_key),
        ((100, "Low callback".into()), low_key),
        ((100, "Fault after mutation".into()), fault_key),
    ])
    .map_err(std::io::Error::other)?;
    let make = |role| {
        SimRuntime::new(
            content.clone(),
            LotModel::new(10, 10, 1).unwrap(),
            RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
            role,
        )
    };
    let mut authority = make(RuntimeRole::Authority)?;
    let mut replica = make(RuntimeRole::Replica)?;
    let initial = authority.next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
        guid: 100,
        position: LotPosition::OUT_OF_WORLD,
        facing: Facing::NORTH,
        persistent_id: PersistentId(0),
        avatar: false,
    })])?;
    authority.step(&initial)?;
    replica.step(&initial)?;
    let entity = EntityRef {
        object_id: ObjectId(1),
        generation: 1,
    };
    assert!(authority.state().threads[&entity.object_id]
        .frames
        .is_empty());
    let banks = authority.next_tick(vec![AcceptedCommand::WriteMemory {
        address: sim_core::vm::MemoryAddress::Entity {
            entity,
            field: sim_core::vm::EntityField::Temp,
            index: 0,
        },
        value: 73,
    }])?;
    authority.step(&banks)?;
    replica.step(&banks)?;
    let original_temps = authority.state().threads[&entity.object_id].temps;
    let command = AcceptedCommand::ExecuteNamedEntryPoint {
        entity,
        name: "CT - FSO Player Joined".into(),
        args: vec![41, 0, 0, 0],
    };
    let encoded = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .serialize(&command)?;
    assert_eq!(&encoded[..4], &25_u32.to_le_bytes());
    let tick = authority.next_tick(vec![command])?;
    let outcome = authority.step(&tick)?;
    replica.step(&tick)?;
    assert!(!outcome
        .events
        .iter()
        .any(|e| matches!(e, RuntimeEvent::ThreadFault { .. })));
    assert_eq!(authority.state().globals[20], 41);
    assert_eq!(authority.state().globals[21], 73);
    assert_eq!(
        authority.state().threads[&entity.object_id].temps,
        original_temps
    );
    assert!(authority.state().threads[&entity.object_id]
        .frames
        .is_empty());
    assert_eq!(authority.state_hash()?, replica.state_hash()?);
    let snapshot = authority.snapshot()?;
    let mut restored = make(RuntimeRole::Replica)?;
    restored.restore(&snapshot)?;
    assert_eq!(authority.state_hash()?, restored.state_hash()?);
    assert!(restored.step(&tick)?.duplicate);
    let invalid = authority.next_tick(vec![AcceptedCommand::ExecuteNamedEntryPoint {
        entity,
        name: "x".repeat(257),
        args: vec![],
    }])?;
    let before = authority.state_hash()?;
    assert!(authority.step(&invalid).is_err());
    assert_eq!(before, authority.state_hash()?);
    for name in ["Empty callback", "Low callback"] {
        let inert = authority.next_tick(vec![AcceptedCommand::ExecuteNamedEntryPoint {
            entity,
            name: name.into(),
            args: vec![],
        }])?;
        authority.step(&inert)?;
        restored.step(&inert)?;
        assert_eq!(authority.state().globals[20], 41);
        assert_eq!(authority.state_hash()?, restored.state_hash()?);
    }
    let missing = authority.next_tick(vec![AcceptedCommand::ExecuteNamedEntryPoint {
        entity,
        name: "Missing callback".into(),
        args: vec![],
    }])?;
    authority.step(&missing)?;
    restored.step(&missing)?;
    assert_eq!(authority.state().globals[20], 41);
    assert_eq!(authority.state_hash()?, restored.state_hash()?);
    let main = authority.next_tick(vec![AcceptedCommand::StartBehavior {
        entity,
        routine: main_key,
        context: sim_core::vm::FrameContext::for_entity(entity, 100),
        args: vec![100, 0, 0, 0],
        replace: true,
    }])?;
    authority.step(&main)?;
    restored.step(&main)?;
    let frames = authority.state().threads[&entity.object_id].frames.clone();
    assert!(!frames.is_empty());
    let callback = authority.next_tick(vec![AcceptedCommand::ExecuteNamedEntryPoint {
        entity,
        name: "CT - FSO Player Joined".into(),
        args: vec![42, 0, 0, 0],
    }])?;
    authority.step(&callback)?;
    restored.step(&callback)?;
    assert_eq!(authority.state().globals[22], 321);
    assert_eq!(authority.state().threads[&entity.object_id].temp_xl[0], 321);
    assert_eq!(authority.state().threads[&entity.object_id].temps[0], 73);
    assert_eq!(authority.state().threads[&entity.object_id].frames, frames);
    assert_eq!(authority.state_hash()?, restored.state_hash()?);
    // Explicitly preserve the runtime's existing fail-closed policy. FreeSO's
    // ordinary check-thread exception handling retains preceding mutations;
    // this is a documented parity gap, not proof of identical fault semantics.
    let fault = authority.next_tick(vec![AcceptedCommand::ExecuteNamedEntryPoint {
        entity,
        name: "Fault after mutation".into(),
        args: vec![],
    }])?;
    let before_fault = authority.state_hash()?;
    assert!(authority.step(&fault).is_err());
    assert!(restored.step(&fault).is_err());
    assert_eq!(before_fault, authority.state_hash()?);
    assert_eq!(before_fault, restored.state_hash()?);
    assert_eq!(authority.state().globals[20], 42);
    let durable = authority.snapshot()?;
    restored.restore(&durable)?;
    assert_eq!(authority.state_hash()?, restored.state_hash()?);
    println!("named callback: empty main stack executes; ReturnFalse retains mutation; temp banks/main stack isolated; replica, snapshot, duplicate and failed-command rollback passed");
    Ok(())
}
