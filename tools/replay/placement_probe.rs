//! F-only placement witness; calls the shipping world implementation.
//! No placement algorithm is copied. Metadata/geometry and admission policy are
//! authored test inputs, not an installed object or a production build receipt.
use sim_core::ids::{EntityRef, ObjectId};
use sim_core::world::{
    Facing, Footprint, IntersectionCall, LotModel, LotPosition, PlacementRequest, PlacementResult,
    PlacementRules, TilePos, WorldObject, WorldState, validate_placement,
};
use std::fmt::Write;
use std::sync::OnceLock;

const CASES: &str = include_str!("../../fixtures/reference/placement-cases.tsv");
const HEADER: &str = "case\tts1\tfacing\tquery_status\tquery_blocker\tcommit_status\tcommit_blocker\tquery_unchanged\tafter_x\tafter_y\tafter_level\tafter_facing\tblocker_x\tblocker_y\tblocker_level\tblocker_facing\tentities\trecover_status\trecover_x\trecover_y\tretry_status\tretry_blocker\tfinal_x\tfinal_y\tfinal_facing\n";
const A: EntityRef = EntityRef {
    object_id: ObjectId(1),
    generation: 1,
};
const B: EntityRef = EntityRef {
    object_id: ObjectId(2),
    generation: 1,
};
static OUTPUT: OnceLock<String> = OnceLock::new();

fn setup() -> WorldState {
    let mut world = WorldState::new(LotModel::new(8, 8, 1).expect("declared lot"));
    for (x, pattern) in [(3, 1), (5, 65534), (6, 65535)] {
        world.lot.set_floor(TilePos::new(x, 5, 1), pattern).unwrap();
    }
    for (entity, x, y) in [(A, 40, 40), (B, 88, 56)] {
        let mut object = WorldObject::new(entity, LotPosition::new(x, y, 1));
        object.footprint = Footprint::rectangle(-8, -8, 8, 8);
        world.insert_object(object).unwrap();
    }
    world
}
fn query(world: &WorldState, target: LotPosition, facing: Facing, allow: bool) -> PlacementResult {
    let mut request = PlacementRequest::new(world.object(A).unwrap().clone(), target, facing);
    request.flags.allow_intersection = allow;
    validate_placement(request, world, &mut |_: &IntersectionCall| -> bool {
        panic!("this metadata cohort has no intersection script provider")
    })
    .expect("well-formed placement request")
}
fn place(
    world: &mut WorldState,
    target: LotPosition,
    facing: Facing,
    allow: bool,
) -> PlacementResult {
    let result = query(world, target, facing, allow);
    // This is the explicit test-host admission policy; move_object is a trusted
    // projection update, not itself a permission/placement validator.
    if result.is_success() {
        world.move_object(A, target, facing).unwrap();
    }
    result
}
fn blocker(result: &PlacementResult) -> i16 {
    result.blocker.map_or(0, |value| value.object_id.0)
}
fn pose(world: &WorldState, entity: EntityRef) -> [i32; 4] {
    world
        .object(entity)
        .map_or([-32768, -32768, 1, 0], |value| {
            [
                value.position.x,
                value.position.y,
                i32::from(value.position.level),
                i32::from(value.facing.0),
            ]
        })
}
fn run() -> String {
    let mut output = String::from(HEADER);
    let mut lines = CASES.lines();
    assert_eq!(
        lines.next(),
        Some("case\tx\ty\tflags\twall_flags\theights\tallow_intersection\texpected_status")
    );
    let mut count = 0;
    for line in lines {
        let values: Vec<&str> = line.split('\t').collect();
        assert_eq!(values.len(), 8);
        count += 1;
        assert!(count <= 24);
        let integer = |index: usize| {
            values[index]
                .parse::<i32>()
                .expect("pinned fixture integer")
        };
        for ts1 in [1, 0] {
            for notch in [0, 2, 4, 6] {
                let mut world = setup();
                let mut mover = world.object(A).unwrap().clone();
                mover.rules.flags = u16::try_from(integer(3)).unwrap();
                mover.rules.wall_flags = u16::try_from(integer(4)).unwrap();
                mover.rules.allowed_heights = u16::try_from(integer(5)).unwrap();
                world.replace_object(mover).unwrap();
                let inject = values[0] == "collision" && ts1 == 1 && notch == 0;
                let saved_blocker = world.object(B).unwrap().clone();
                if cfg!(placement_fault_blocker) && inject {
                    world.remove_object(B).unwrap();
                }
                let before_query = world.clone();
                let target = LotPosition::new(integer(1), integer(2), 1);
                let facing = Facing(notch);
                let allow = values[6] == "1";
                let tested = query(&world, target, facing, allow);
                let pure = i32::from(before_query == world);
                let committed = place(&mut world, target, facing, allow);
                if cfg!(placement_fault_rollback) && inject {
                    // Negative control: perform an actual forbidden projection
                    // update after rejection, rather than changing a trace cell.
                    assert!(!committed.is_success());
                    world.move_object(A, target, facing).unwrap();
                }
                let after = pose(&world, A);
                let other = pose(&world, B);
                let entities = world.objects().len();
                if cfg!(placement_fault_blocker) && inject {
                    world.insert_object(saved_blocker).unwrap();
                }
                let mut mover = world.object(A).unwrap().clone();
                mover.rules = PlacementRules::default();
                world.replace_object(mover).unwrap();
                let recovered = place(
                    &mut world,
                    LotPosition::new(40, 72, 1),
                    Facing::NORTH,
                    false,
                );
                let recovery_pose = pose(&world, A);
                let retried = place(
                    &mut world,
                    LotPosition::new(88, 56, 1),
                    Facing::NORTH,
                    false,
                );
                let final_pose = pose(&world, A);
                writeln!(output,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    values[0], ts1, notch, tested.status.code(), blocker(&tested),
                    committed.status.code(), blocker(&committed), pure,
                    after[0],after[1],after[2],after[3],other[0],other[1],other[2],other[3],entities,
                    recovered.status.code(),recovery_pose[0],recovery_pose[1],retried.status.code(),
                    blocker(&retried),final_pose[0],final_pose[1],final_pose[3]
                ).unwrap();
            }
        }
    }
    assert_eq!(count, 24);
    output
}
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    print!("{}", OUTPUT.get_or_init(run));
}
// SAFETY: uniquely named test-only exports expose immutable process-lifetime
// data. No caller-provided pointer is accepted, dereferenced or freed.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn placement_ptr() -> *const u8 {
    OUTPUT.get_or_init(run).as_ptr()
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn placement_len() -> usize {
    OUTPUT.get_or_init(run).len()
}
