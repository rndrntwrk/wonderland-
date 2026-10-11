//! F-owned scenario driver; all simulation, decoding and scheduling use shipping crates.
//! Authored metadata is not original installed game content.
use sim_core::{
    ids::{EntityRef, PersistentId},
    runtime::{AcceptedCommand, RuntimeConfig, RuntimeEvent, RuntimeRole, SimRuntime, SpawnSpec},
    state::{ContentSet, ObjectDefinition, TuningSet},
    vm::{EntityField, MemoryAddress, RoutineKey, RoutineScope, RoutineStore, VmMode, VmStop},
    world::{Facing, LotModel, LotPosition},
};
use std::fmt::Write;
use std::sync::OnceLock;
use wonderland_content_runtime_bridge::import_bhav;
use wonderland_legacy_formats::{iff, Limits};

const OWNER: u32 = 0xf00d0044;
const SOURCE: &[u8] = include_bytes!(
    "../../TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"
);
const AUTHORED: &str = include_str!("../../fixtures/reference/interpreter-authored.iff.hex");
const HEADER: &str = "case\tts1\ttick\tclock_ticks\trng\tentities\tobject_id\tattribute_0\tattribute_1\tattribute_2\tattribute_3\tstack_count\troutine\tpc\targ0\tlocal0\n";
static OUTPUT: OnceLock<(String, String)> = OnceLock::new();

fn content(authored: bool) -> ContentSet {
    let bytes = if authored {
        let text = AUTHORED.strip_suffix('\n').expect("one LF in hex fixture");
        assert_eq!(text.len(), 744);
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex fixture"))
            .collect::<Vec<_>>()
    } else {
        SOURCE.to_vec()
    };
    let file = iff::decode(&bytes, &Limits::default()).expect("real IFF decoder");
    let ids: &[u16] = if authored { &[4096, 4097] } else { &[4110] };
    let mut store = RoutineStore::new();
    for &id in ids {
        let chunk = file.chunks.iter()
            .find(|chunk| chunk.key.kind == *b"BHAV" && chunk.key.id == id)
            .expect("declared routine");
        store.insert(
            RoutineKey { scope: RoutineScope::Private(OWNER), id },
            import_bhav(chunk, &Limits::default()).expect("shipping BHAV importer"),
        ).expect("validated routine");
    }
    let mut object = ObjectDefinition::new(OWNER, 4);
    object.attributes = vec![10, 20, 30, 40];
    let main = RoutineKey {
        scope: RoutineScope::Private(OWNER),
        id: if authored { 4096 } else { 4110 },
    };
    object.entry_points.insert(1, main);
    if !authored {
        object.entry_points.insert(0, main);
    }
    ContentSet::new(store, vec![object], vec![], TuningSet::default())
        .expect("declared immutable content")
}

fn write(entity: EntityRef, index: u16, value: i16) -> AcceptedCommand {
    AcceptedCommand::WriteMemory {
        address: MemoryAddress::Entity { entity, field: EntityField::Attribute, index },
        value,
    }
}

fn run() -> (String, String) {
    let mut output = String::from(HEADER);
    let mut hashes = String::from("case\tts1\ttick\tstate_sha256\n");
    for authored in [false, true] {
        let name = if authored { "authored" } else { "source4110" };
        for ts1 in [true, false] {
            let mut config = RuntimeConfig::new(
                if ts1 { VmMode::Ts1 } else { VmMode::Tso },
                44, 1, 0x123456789abcdef0,
            );
            config.utc_start_dotnet_ticks = 630822816000000000;
            let mut runtime = SimRuntime::new(
                content(authored), LotModel::new(8, 8, 1).unwrap(),
                config, RuntimeRole::Authority,
            ).expect("shipping simulation initialization");
            let mut actor = None;
            for tick in 1..=32u32 {
                let mut commands = if tick == 1 {
                    vec![AcceptedCommand::Spawn(SpawnSpec {
                        guid: OWNER, position: LotPosition::OUT_OF_WORLD,
                        facing: Facing::NORTH, persistent_id: PersistentId(0), avatar: false,
                    })]
                } else {
                    let actor = actor.expect("spawn receipt");
                    if authored {
                        vec![write(actor, 0, if tick % 8 < 4 { tick as i16 } else { -(tick as i16) })]
                    } else {
                        vec![write(actor, 0, -(tick as i16)),
                             write(actor, 1, 100 + tick as i16),
                             write(actor, 3, 200 + tick as i16)]
                    }
                };
                if cfg!(interpreter_fault_attribute) && !authored && ts1 && tick == 7 {
                    // Change a real admitted input, not the returned trace.
                    commands.push(write(actor.unwrap(), 2, 31));
                }
                if !(cfg!(interpreter_fault_tick) && !authored && ts1 && tick == 7) {
                    let input = runtime.next_tick(commands).expect("admitted tick");
                    let outcome = runtime.step(&input).expect("real simulation tick");
                    assert!(outcome.effects.is_empty(), "no external effects in this cohort");
                    for event in outcome.events {
                        match event {
                            RuntimeEvent::Spawned(entity) => actor = Some(entity),
                            RuntimeEvent::ThreadFault { fault, .. } => panic!("{fault:?}"),
                            _ => panic!("unexpected runtime event"),
                        }
                    }
                }
                let actor = actor.expect("one real entity");
                let state = runtime.state();
                let entity = &state.entities[&actor.object_id];
                let thread = &state.threads[&actor.object_id];
                assert!(thread.diagnostics.is_empty());
                assert!(!matches!(thread.stop, VmStop::Faulted(_) | VmStop::BudgetExhausted));
                let top = thread.frames.last();
                writeln!(
                    output, "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    name, u8::from(ts1), tick, state.clock.ticks, state.rng.state(),
                    state.entities.len(), actor.object_id.0,
                    entity.attributes[0], entity.attributes[1],
                    entity.attributes[2], entity.attributes[3], thread.frames.len(),
                    top.map_or(-1, |f| i32::from(f.routine.id)),
                    top.map_or(-1, |f| i32::from(f.instruction_pointer)),
                    top.and_then(|f| f.args.first()).copied().unwrap_or(-1),
                    top.and_then(|f| f.locals.first()).copied().unwrap_or(-1),
                ).unwrap();
                let digest = runtime.state_hash().expect("shipping canonical native state");
                let digest: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
                writeln!(hashes, "{}\t{}\t{}\t{}", name, u8::from(ts1), tick, digest).unwrap();
            }
        }
    }
    (output, hashes)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::io::Write;
    assert_eq!(std::env::var("WONDERLAND_INTERPRETER_TEST_ONLY").as_deref(), Ok("1"));
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 2, "supply a new native-state hash output path");
    let (trace, hashes) = OUTPUT.get_or_init(run);
    let mut file = std::fs::OpenOptions::new()
        .write(true).create_new(true).open(&args[1]).expect("new hash evidence");
    file.write_all(hashes.as_bytes()).unwrap();
    print!("{trace}");
}

// SAFETY: these probe-only exports expose immutable process-lifetime buffers.
// No caller pointer is accepted; lengths are bounds checked by the host.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn interpreter_ptr() -> *const u8 { OUTPUT.get_or_init(run).0.as_ptr() }
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn interpreter_len() -> usize { OUTPUT.get_or_init(run).0.len() }
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn interpreter_hash_ptr() -> *const u8 { OUTPUT.get_or_init(run).1.as_ptr() }
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn interpreter_hash_len() -> usize { OUTPUT.get_or_init(run).1.len() }
