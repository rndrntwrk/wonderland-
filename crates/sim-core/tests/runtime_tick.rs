use sim_core::{
    ids::{ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::*,
    state::*,
    vm::*,
    world::{LotModel, TilePos},
};

fn content() -> ContentSet {
    let key = RoutineKey {
        scope: RoutineScope::Private(100),
        id: 4096,
    };
    let mut routines = RoutineStore::new();
    let add = ExpressionOperand {
        lhs: Variable { scope: 0, data: 0 },
        rhs: Variable::new(Scope::Literal, 1),
        is_signed: 0,
        operator: 3,
    }
    .encode();
    routines
        .insert(
            key,
            VmRoutine::new(4096, 0, 4, vec![VmInstruction::new(2, 254, 255, add)]).unwrap(),
        )
        .unwrap();
    let mut object = ObjectDefinition::new(100, 1);
    object.entry_points.insert(1, key);
    ContentSet::new(routines, vec![object], vec![], TuningSet::default()).unwrap()
}
fn runtime(role: RuntimeRole) -> SimRuntime {
    SimRuntime::new(
        content(),
        LotModel::new(12, 12, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 7, 1, 123),
        role,
    )
    .unwrap()
}
fn spawn() -> AcceptedCommand {
    AcceptedCommand::Spawn(SpawnSpec {
        guid: 100,
        position: TilePos::new(3, 3, 1).center(),
        facing: sim_core::world::Facing::NORTH,
        persistent_id: PersistentId(0),
        avatar: false,
    })
}

#[test]
fn authority_and_replica_replay_the_same_sorted_simulation() {
    let mut a = runtime(RuntimeRole::Authority);
    let mut b = runtime(RuntimeRole::Replica);
    for tick in 1..=12 {
        let input = a
            .next_tick(if tick == 1 {
                vec![spawn(), spawn()]
            } else {
                vec![]
            })
            .unwrap();
        let ao = a.step(&input).unwrap();
        let bo = b.step(&input).unwrap();
        assert_eq!(ao.state_hash, bo.state_hash);
        assert_eq!(ao.events, bo.events);
    }
    assert_eq!(a.state().entities[&ObjectId(1)].attributes[0], 12);
    assert_eq!(a.state().entities[&ObjectId(2)].attributes[0], 12);
    assert_eq!(a.state().clock.ticks, 12);
}

#[test]
fn exact_duplicate_is_noop_but_conflicting_tick_is_rejected() {
    let mut a = runtime(RuntimeRole::Authority);
    let input = a.next_tick(vec![spawn()]).unwrap();
    a.step(&input).unwrap();
    let before = a.state().clone();
    let duplicate = a.step(&input).unwrap();
    assert!(duplicate.duplicate);
    assert!(duplicate.events.is_empty());
    assert_eq!(a.state(), &before);
    let mut conflict = input.clone();
    conflict.commands.clear();
    assert!(matches!(
        a.step(&conflict),
        Err(RuntimeError::ConflictingTick)
    ));
    assert_eq!(a.state(), &before);
}

#[test]
fn command_failure_and_tick_gap_leave_no_partial_state() {
    let mut a = runtime(RuntimeRole::Authority);
    let before = a.state().clone();
    let mut bad = match spawn() {
        AcceptedCommand::Spawn(s) => s,
        _ => unreachable!(),
    };
    bad.guid = 999;
    let input = a
        .next_tick(vec![spawn(), AcceptedCommand::Spawn(bad)])
        .unwrap();
    assert!(a.step(&input).is_err());
    assert_eq!(a.state(), &before);
    let mut gap = a.next_tick(vec![]).unwrap();
    gap.tick = 2;
    assert!(matches!(a.step(&gap), Err(RuntimeError::TickGap { .. })));
    assert_eq!(a.state(), &before);
}

#[test]
fn stale_epoch_content_and_rng_are_rejected_atomically() {
    let mut a = runtime(RuntimeRole::Authority);
    let before = a.state().clone();
    for variant in 0..3 {
        let mut input = a.next_tick(vec![spawn()]).unwrap();
        match variant {
            0 => input.epoch = 9,
            1 => input.content.tuning_hash[0] ^= 1,
            _ => input.rng_before ^= 1,
        };
        assert!(a.step(&input).is_err());
        assert_eq!(a.state(), &before);
    }
}

#[test]
fn deletion_mixes_rng_before_removal_and_reuse_increments_generation() {
    let mut a = runtime(RuntimeRole::Authority);
    let input = a.next_tick(vec![spawn()]).unwrap();
    a.step(&input).unwrap();
    let old = a.state().entities[&ObjectId(1)].info.reference;
    let seed = a.state().rng.state();
    let delete = a
        .next_tick(vec![AcceptedCommand::Delete { entity: old }])
        .unwrap();
    a.step(&delete).unwrap();
    assert!(a.state().entities.is_empty());
    assert_eq!(a.state().rng.state(), seed.wrapping_add(1));
    let input = a.next_tick(vec![spawn()]).unwrap();
    a.step(&input).unwrap();
    let new = a.state().entities[&ObjectId(1)].info.reference;
    assert_eq!(new.generation, old.generation + 1);
    let before = a.state().clone();
    let stale = a
        .next_tick(vec![AcceptedCommand::Delete { entity: old }])
        .unwrap();
    assert!(a.step(&stale).is_err());
    assert_eq!(a.state(), &before);
}

#[test]
fn post_tick_snapshot_resumes_at_next_tick() {
    let mut a = runtime(RuntimeRole::Authority);
    let input = a.next_tick(vec![spawn()]).unwrap();
    a.step(&input).unwrap();
    let bytes = a.snapshot().unwrap();
    let mut b = runtime(RuntimeRole::Replica);
    b.restore(&bytes).unwrap();
    assert_eq!(a.state(), b.state());
    for _ in 0..20 {
        let tick = a.next_tick(vec![]).unwrap();
        assert_eq!(
            a.step(&tick).unwrap().state_hash,
            b.step(&tick).unwrap().state_hash
        );
    }
}

#[test]
fn behavior_query_cannot_change_tick_rng_or_entity_state() {
    let mut a = runtime(RuntimeRole::Authority);
    let input = a.next_tick(vec![spawn()]).unwrap();
    a.step(&input).unwrap();
    let before = a.state().clone();
    let entity = a.state().entities[&ObjectId(1)].info.reference;
    let query = a
        .query_behavior(
            entity,
            RoutineKey {
                scope: RoutineScope::Private(100),
                id: 4096,
            },
            FrameContext::for_entity(entity, 100),
            vec![0; 4],
            32,
        )
        .unwrap();
    assert_eq!(query.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    assert_eq!(a.state(), &before);
}
