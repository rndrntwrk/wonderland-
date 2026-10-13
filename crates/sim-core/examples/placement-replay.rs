//! Executable runtime journey: accepted placement, replica replay and restore.
//! Fixture content is explicit; this does not claim native player admission.
use sim_core::{
    ids::{EntityRef, ObjectId, PersistentId},
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{FrameContext, RoutineKey, RoutineScope, RoutineStore, VmInstruction, VmMode, VmRoutine},
    world::{Facing, LotModel, TilePos},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for (flags, expected) in [(0, TilePos::new(2, 2, 1)), (2, TilePos::new(3, 3, 1))] {
        let key = RoutineKey {
            scope: RoutineScope::Private(100),
            id: 4096,
        };
        let mut routines = RoutineStore::new();
        routines.insert(
            key,
            VmRoutine::new(
                4096,
                0,
                4,
                vec![VmInstruction::new(
                    16,
                    254,
                    255,
                    [0, 0, flags, 0, 0, 0, 0, 0],
                )],
            )?,
        )?;
        let mut reference = ObjectDefinition::new(100, 2);
        reference.object_data[8] = 4;
        reference.object_data[4] = 1;
        reference.object_data[42] = 3;
        let mut target_definition = ObjectDefinition::new(200, 2);
        target_definition.object_data[4] = 1;
        target_definition.object_data[42] = 3;
        let content = ContentSet::new(
            routines,
            vec![reference, target_definition],
            vec![],
            TuningSet::default(),
        )
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
        let spawn = |guid, tile: TilePos| {
            AcceptedCommand::Spawn(SpawnSpec {
                guid,
                position: tile.center(),
                facing: Facing::NORTH,
                persistent_id: PersistentId(0),
                avatar: guid == 100,
            })
        };
        let initial = authority.next_tick(vec![
            spawn(100, TilePos::new(3, 3, 1)),
            spawn(200, TilePos::new(7, 7, 1)),
        ])?;
        authority.step(&initial)?;
        replica.step(&initial)?;
        let actor = EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        };
        let target = EntityRef {
            object_id: ObjectId(2),
            generation: 1,
        };
        let mut context = FrameContext::for_entity(actor, 100);
        context.stack_object = target.object_id;
        context.stack_object_ref = Some(target);
        let tick = authority.next_tick(vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: key,
            context,
            args: vec![],
            replace: true,
        }])?;
        let outcome = authority.step(&tick)?;
        replica.step(&tick)?;
        assert!(
            !outcome
                .events
                .iter()
                .any(|e| matches!(e, RuntimeEvent::ThreadFault { .. })),
            "{:?}",
            outcome.events
        );
        println!(
            "stop={:?} events={:?}",
            authority.state().threads[&ObjectId(1)].stop,
            outcome.events
        );
        assert_eq!(
            authority.state().world.object(target).unwrap().position,
            expected.center()
        );
        assert_eq!(authority.state_hash()?, replica.state_hash()?);
        let snapshot = authority.snapshot()?;
        let mut restored = make(RuntimeRole::Replica)?;
        restored.restore(&snapshot)?;
        assert_eq!(authority.state_hash()?, restored.state_hash()?);
        assert!(restored.step(&tick)?.duplicate);
        let next = authority.next_tick(vec![])?;
        authority.step(&next)?;
        restored.step(&next)?;
        assert_eq!(authority.state_hash()?, restored.state_hash()?);
        println!("placement flags={flags}: accepted placement, replica equality, snapshot recovery, duplicate receipt and continuation passed");
    }
    Ok(())
}
