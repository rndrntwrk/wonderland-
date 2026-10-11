//! Source query/register alias regressions through the integrated runtime.
//! VMThread.EvaluateCheck clones owner banks outside Scheduler.RunningNow;
//! VMMemory scope13 continues to address the real Entity.Thread bank.
use sim_core::{
    ids::{EntityRef, ObjectId, PersistentId},
    primitives::arithmetic::ExpressionOperand,
    runtime::{AcceptedCommand, QueryOutcome, RuntimeConfig, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{
        FrameContext, PrimitiveExit, RoutineKey, RoutineScope, RoutineStore, Scope, Variable,
        VmInstruction, VmMode, VmRoutine, VmStop,
    },
    world::{Facing, LotModel, TilePos},
};

const GUID: u32 = 100;
const QUERY_NESTED: u16 = 4100;
const CONDITION: u16 = 4101;
const ACTION: u16 = 4102;
const QUERY_SCOPE13: u16 = 4103;
const QUERY_SCOPE13_YIELD: u16 = 4104;
const QUERY_NAMED_CURRENT: u16 = 4105;
const QUERY_NAMED_ENTITY: u16 = 4106;
const NAMED_CHILD: u16 = 4107;
const PARKED_REAL_THREAD: u16 = 4108;

fn key(id: u16) -> RoutineKey {
    RoutineKey {
        scope: RoutineScope::Private(GUID),
        id,
    }
}

fn expression(
    lhs: Scope,
    lhs_index: i16,
    rhs: Scope,
    rhs_index: i16,
    operator: u8,
    next: u8,
) -> VmInstruction {
    VmInstruction::new(
        2,
        next,
        255,
        ExpressionOperand {
            lhs: Variable::new(lhs, lhs_index),
            rhs: Variable::new(rhs, rhs_index),
            is_signed: 0,
            operator,
        }
        .encode(),
    )
}

fn assign(lhs: Scope, index: i16, rhs: Scope, data: i16, next: u8) -> VmInstruction {
    expression(lhs, index, rhs, data, 5, next)
}

fn add_xl(value: i16, next: u8) -> VmInstruction {
    expression(Scope::TempXl, 0, Scope::Literal, value, 3, next)
}

fn insert(store: &mut RoutineStore, id: u16, instructions: Vec<VmInstruction>) {
    store
        .insert(key(id), VmRoutine::new(id, 0, 4, instructions).unwrap())
        .unwrap();
}

fn scope13_program(yield_once: bool) -> Vec<VmInstruction> {
    let mut instructions = vec![
        assign(Scope::Temps, 0, Scope::Literal, 90, 1),
        assign(Scope::Temps, 1, Scope::StackObjectTemp, 0, 2),
        assign(Scope::StackObjectTemp, 0, Scope::Literal, 88, 3),
        assign(Scope::Temps, 2, Scope::Temps, 0, 4),
        assign(
            Scope::Temps,
            3,
            Scope::StackObjectTemp,
            0,
            if yield_once { 5 } else { 254 },
        ),
    ];
    if yield_once {
        instructions.extend([
            assign(Scope::Parameters, 0, Scope::Literal, 1, 6),
            VmInstruction::new(0, 7, 255, [0; 8]),
            assign(Scope::Temps, 4, Scope::StackObjectTemp, 0, 254),
        ]);
    }
    instructions
}

fn named_program(destination: u8) -> Vec<VmInstruction> {
    vec![
        assign(Scope::Temps, 0, Scope::Literal, 90, 1),
        add_xl(20_000, 2),
        // STR300, current code-owner scope, one-based string1, destination0/1.
        VmInstruction::new(28, 3, 255, [44, 1, 0, 0, 1, destination, 0, 0]),
        assign(Scope::Temps, 1, Scope::StackObjectTemp, 0, 4),
        assign(Scope::Temps, 2, Scope::MyObjectAttributes, 2, 254),
    ]
}

fn content() -> ContentSet {
    let mut routines = RoutineStore::new();
    insert(
        &mut routines,
        QUERY_NESTED,
        vec![
            assign(Scope::Temps, 10, Scope::Temps, 0, 1),
            assign(Scope::Temps, 0, Scope::Literal, 90, 2),
            add_xl(20_000, 3),
            VmInstruction::new(20, 4, 255, [0; 8]),
            assign(Scope::Temps, 1, Scope::MyObjectAttributes, 0, 5),
            assign(Scope::Temps, 2, Scope::MyObjectAttributes, 1, 254),
        ],
    );
    insert(
        &mut routines,
        CONDITION,
        vec![
            assign(Scope::MyObjectAttributes, 0, Scope::Temps, 0, 1),
            assign(Scope::MyObjectAttributes, 1, Scope::TempXl, 0, 2),
            assign(Scope::Temps, 0, Scope::Literal, 100, 3),
            add_xl(30_000, 254),
        ],
    );
    insert(
        &mut routines,
        ACTION,
        vec![VmInstruction::new(15, 254, 255, [0; 8])],
    );
    insert(&mut routines, QUERY_SCOPE13, scope13_program(false));
    insert(&mut routines, QUERY_SCOPE13_YIELD, scope13_program(true));
    insert(&mut routines, QUERY_NAMED_CURRENT, named_program(0));
    insert(&mut routines, QUERY_NAMED_ENTITY, named_program(1));
    insert(
        &mut routines,
        NAMED_CHILD,
        vec![
            assign(Scope::Temps, 0, Scope::Literal, 120, 1),
            add_xl(1_000, 2),
            assign(Scope::MyObjectAttributes, 2, Scope::TempXl, 0, 254),
        ],
    );
    insert(
        &mut routines,
        PARKED_REAL_THREAD,
        vec![VmInstruction::new(0, 254, 255, [0; 8])],
    );
    let mut object = ObjectDefinition::new(GUID, 3);
    object.entry_points.insert(18, key(ACTION));
    object.entry_conditions.insert(18, key(CONDITION));
    ContentSet::new(routines, vec![object], vec![], TuningSet::default())
        .unwrap()
        .with_strings(vec![((GUID, 300), vec!["named child".into()])])
        .unwrap()
        .with_named_trees(vec![((GUID, "named child".into()), key(NAMED_CHILD))])
        .unwrap()
}

fn fixture(real_frame: bool) -> (SimRuntime, EntityRef) {
    let content = content();
    let mut runtime = SimRuntime::new(
        content.clone(),
        LotModel::new(8, 8, 1).unwrap(),
        RuntimeConfig::new(VmMode::Ts1, 41, 7, 123),
        RuntimeRole::Authority,
    )
    .unwrap();
    let tick = runtime
        .next_tick(vec![AcceptedCommand::Spawn(SpawnSpec {
            guid: GUID,
            position: TilePos::new(3, 3, 1).center(),
            facing: Facing::NORTH,
            persistent_id: PersistentId(0),
            avatar: false,
        })])
        .unwrap();
    runtime.step(&tick).unwrap();
    let mut state = runtime.state().clone();
    let owner = state.entities[&ObjectId(1)].info.reference;
    let thread = state.threads.get_mut(&owner.object_id).unwrap();
    thread.temps[0] = 77;
    thread.temp_xl[0] = 70_000;
    if real_frame {
        thread
            .push_entry(
                content.routines(),
                key(PARKED_REAL_THREAD),
                FrameContext::for_entity(owner, GUID),
                vec![4, 0, 0, 0],
            )
            .unwrap();
        thread.schedule_idle_start = 1;
        thread.stop = VmStop::Sleeping { until_tick: 6 };
        state.scheduler.schedule(owner, 6).unwrap();
    }
    (
        SimRuntime::from_state(state, content, RuntimeRole::Authority).unwrap(),
        owner,
    )
}

fn query(runtime: &SimRuntime, owner: EntityRef, routine: u16) -> QueryOutcome {
    let result = runtime
        .query_behavior(
            owner,
            key(routine),
            FrameContext::for_entity(owner, GUID),
            vec![0; 4],
            64,
        )
        .unwrap();
    assert_eq!(result.stop, VmStop::Completed(PrimitiveExit::ReturnTrue));
    result
}

#[test]
fn nested_outside_tick_check_clones_real_banks_without_overwriting_query_banks() {
    // Catches zero query seeding, cloning the current query instead of Entity.Thread,
    // and same-owner copyback that incorrectly treats distinct check banks as aliases.
    let (runtime, owner) = fixture(false);
    let before = runtime.state().clone();
    let result = query(&runtime, owner, QUERY_NESTED);
    assert_eq!(
        result.temps[10], 77,
        "the query initially copies real Temp0"
    );
    assert_eq!(
        result.temps[0], 90,
        "the nested check owns a separate Temp0"
    );
    assert_eq!(
        result.temp_xl[0], 90_000,
        "nested XL changes must stay separate"
    );
    assert_eq!(
        result.temps[1], 77,
        "the nested check starts from real Temp0"
    );
    assert_eq!(
        result.temps[2], 4_464,
        "real XL70000 narrows to source i16 4464"
    );
    assert_eq!(runtime.state(), &before);
}

#[test]
fn outside_tick_scope13_reads_and_writes_real_projection_without_aliasing_query() {
    // Catches the unconditional self-owner shortcut or a writeback hook that
    // wrongly applies real entity-bank writes to the force-cloned query bank.
    let (runtime, owner) = fixture(false);
    let before = runtime.state().clone();
    let result = query(&runtime, owner, QUERY_SCOPE13);
    assert_eq!(&result.temps[..4], &[90, 77, 90, 88]);
    assert_eq!(result.temp_xl[0], 70_000);
    assert_eq!(runtime.state(), &before);
}

#[test]
fn outside_tick_scope13_entity_bank_writes_survive_timer_yield_host_recreation() {
    // Catches rebuilding a timer-yield host from the pre-write entity overlay.
    let (runtime, owner) = fixture(false);
    let before = runtime.state().clone();
    let result = query(&runtime, owner, QUERY_SCOPE13_YIELD);
    assert_eq!(&result.temps[..5], &[90, 77, 90, 88, 88]);
    assert_eq!(
        runtime.state(),
        &before,
        "query schedules and RNG stay isolated"
    );
}

#[test]
fn named_query_destination_distinguishes_current_check_from_same_owner_real_thread() {
    // Catches selecting a thread by EntityRef alone, losing inherited clone
    // identity on destination0, or copying destination1 banks into the query.
    for (routine, temps, xl) in [
        (QUERY_NAMED_CURRENT, [120, 77, 25_464], 91_000),
        (QUERY_NAMED_ENTITY, [90, 120, 5_464], 90_000),
    ] {
        let (runtime, owner) = fixture(true);
        let before = runtime.state().clone();
        let result = query(&runtime, owner, routine);
        assert_eq!(&result.temps[..3], &temps, "routine {routine}");
        assert_eq!(result.temp_xl[0], xl, "routine {routine}");
        assert_eq!(runtime.state(), &before);
    }
}
