//! TEST ONLY: line-delimited controller for the actual A/B runtime.
//! Never installed by production startup. Original BHAV bytes are unchanged;
//! object/lot/grant metadata and the account identity are declared fixtures.
#[allow(dead_code)]
#[path = "../tests/live_session/support.rs"]
pub(crate) mod support;
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
use wonderland_game_runtime::live_session::{Checkpoint, TickFrame};
use wonderland_game_runtime::live_wire::player::admission::revalidate_action;
use wonderland_game_runtime::live_wire::player::{
    ActionReceipt, Bootstrap, PlayerAction, PlayerBinding, decode_action, encode_bootstrap,
    encode_receipt,
};
use wonderland_game_runtime::live_wire::{
    WireLimits, decode_checkpoint_request, encode_checkpoint, encode_ticks,
};
use wonderland_game_runtime::sim_core::vm::{EntityField, MemoryAddress};
use wonderland_game_runtime::world_view::WorldDocument;
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, Facing, GameRuntime, InteractionAccess, LotModel, PersistentId,
    RuntimeConfig, RuntimeEvent, RuntimeRole, SpawnSpec, TilePos, VmMode,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").unwrap();
    }
    out
}
fn unhex(input: &str) -> Result<Vec<u8>> {
    if input.len() > 131072
        || !input.len().is_multiple_of(2)
        || !input.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid controlled packet".into());
    }
    input
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| Ok(u8::from_str_radix(std::str::from_utf8(s)?, 16)?))
        .collect()
}
// Explicitly authored waiting harness for cancellation QA. The needs action still
// uses its unchanged checked-in BHAV; these two routines are NOT original assets.
fn fixture_content() -> wonderland_game_runtime::sim_core::state::ContentSet {
    use wonderland_game_runtime::sim_core::{
        interactions::{
            ActionFlags, InteractionDefinition, InteractionKey, InteractionScope, PermissionFlags,
            RoutineBinding,
        },
        state::ContentSet,
        vm::{RoutineKey, RoutineScope, VmInstruction, VmRoutine},
    };
    let original = support::content("cursebook_set_permission.iff", 4107, false);
    let mut routines = original.routines().clone();
    for id in [4998, 4999] {
        let finish = if id == 4998 { 254 } else { 0 };
        routines
            .insert(
                RoutineKey {
                    scope: RoutineScope::Private(support::OWNER),
                    id,
                },
                VmRoutine::new(
                    id,
                    0,
                    4,
                    vec![
                        VmInstruction {
                            opcode: 2,
                            true_pointer: 1,
                            false_pointer: 1,
                            operand: [0, 0, 0x28, 0x23, 0, 5, 9, 7],
                        }, // Parameter0 = 9000 ticks.
                        VmInstruction {
                            opcode: 17,
                            true_pointer: finish,
                            false_pointer: finish,
                            operand: [0, 0, u8::from(id == 4999), 0, 0, 0, 0, 0],
                        },
                    ],
                )
                .unwrap(),
            )
            .unwrap();
    }
    let mut object = original.object(support::OWNER).unwrap().clone();
    object.entry_points.insert(
        1,
        RoutineKey {
            scope: RoutineScope::Private(support::OWNER),
            id: 4999,
        },
    );
    let mut table = original.interaction_table(support::OWNER).unwrap().clone();
    table.definitions.push(InteractionDefinition {
        key: InteractionKey {
            tta_index: 8,
            scope: InteractionScope::Local,
        },
        action: RoutineBinding {
            routine_id: 4998,
            code_owner_guid: support::OWNER,
        },
        check: None,
        flags: ActionFlags(ActionFlags::ALLOW_VISITORS),
        permissions: PermissionFlags::default(),
        label: Some("Wait (test harness)".into()),
    });
    ContentSet::new(routines, vec![object], vec![], original.tuning().clone())
        .unwrap()
        .with_interaction_tables(vec![(support::OWNER, table)])
        .unwrap()
}
pub(crate) fn source() -> Result<(GameRuntime, EntityRef)> {
    let content = fixture_content();
    let mut runtime = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).map_err(|_| "Invalid controlled lot geometry")?,
        RuntimeConfig::new(VmMode::Ts1, 11, 7, 123),
        RuntimeRole::Authority,
    )?;
    let result = runtime.advance(vec![AcceptedCommand::Spawn(SpawnSpec {
        guid: support::OWNER,
        position: TilePos::new(3, 3, 1).center(),
        facing: Facing::NORTH,
        persistent_id: PersistentId(42),
        avatar: true,
    })])?;
    let actor = result
        .events
        .iter()
        .find_map(|e| match e {
            RuntimeEvent::Spawned(actor) => Some(*actor),
            _ => None,
        })
        .ok_or("No source actor")?;
    runtime.advance(vec![AcceptedCommand::SetInteractionAuthority {
        actor,
        access: Some(InteractionAccess {
            principal: support::PRINCIPAL,
            allow_hidden: false,
        }),
    }])?;
    reset_needs(&mut runtime, actor)?;
    Ok((runtime, actor))
}
pub(crate) fn reset_needs(runtime: &mut GameRuntime, actor: EntityRef) -> Result<Value> {
    tick(
        runtime,
        // Original BHAV4107 randomly fills E/H/H or Bladder/Social/Fun.
        // Reset both groups so either unmodified source branch is observable.
        [5, 7, 8, 9, 14, 15]
            .into_iter()
            .map(|index| AcceptedCommand::WriteMemory {
                address: MemoryAddress::Entity {
                    entity: actor,
                    field: EntityField::Motive,
                    index,
                },
                value: 0,
            })
            .collect(),
    )
}
fn tick(runtime: &mut GameRuntime, commands: Vec<AcceptedCommand>) -> Result<Value> {
    let accepted = runtime.sim().next_tick(commands)?;
    let result = runtime.apply_accepted(&accepted)?;
    let bytes = encode_ticks(
        &[TickFrame {
            accepted,
            state_hash: result.state_hash,
        }],
        WireLimits::default(),
    )?;
    Ok(
        json!({"packet":hex(&bytes),"tick":result.tick.to_string(),"hash":hex(&result.state_hash),"events":format!("{:?}",result.events)}),
    )
}
fn process(runtime: &mut GameRuntime, actor: EntityRef, input: Value) -> Result<Value> {
    match input["op"].as_str().ok_or("Missing controlled operation")? {
        "bootstrap" => {
            let binding = PlayerBinding {
                source_epoch: input["source_epoch"]
                    .as_str()
                    .ok_or("Missing source epoch")?
                    .parse()?,
                lot_incarnation: input["lot_incarnation"]
                    .as_str()
                    .ok_or("Missing lot incarnation")?
                    .parse()?,
                lot_location: 16777472,
                avatar_id: 42,
            };
            let state = runtime.sim().state();
            let appearance = WorldDocument::from_blueprint_xml(
                "<house><size>8</size><world><floors/><walls/></world><objects/></house>",
                "test:source-runtime-fixture",
                "declared-browser-fixture",
            )?;
            let value = Bootstrap {
                binding,
                principal: support::PRINCIPAL,
                actor,
                content: runtime.sim().content().clone(),
                appearance,
                lot: state.world.lot.clone(),
                mode: state.mode,
                lot_id: state.lot_id,
                authority_epoch: state.authority_epoch,
                effect_namespace: state.effects.namespace(),
                limits: state.limits.clone(),
                effect_limits: state.effects.limits(),
            };
            Ok(json!({"packet":hex(&encode_bootstrap(&value)?)}))
        }
        "checkpoint" => {
            let request = unhex(input["request"].as_str().ok_or("Missing request")?)?;
            let request = decode_checkpoint_request(&request)?;
            let bytes = runtime.snapshot()?;
            let state = runtime.sim().state();
            let packet = encode_checkpoint(
                request.id,
                Checkpoint {
                    completed_tick: state.completed_tick,
                    state_hash: runtime.sim().state_hash()?,
                    bytes: &bytes,
                },
                &[],
                WireLimits::default(),
            )?;
            Ok(
                json!({"packet":hex(&packet),"tick":state.completed_tick.to_string(),"hash":hex(&runtime.sim().state_hash()?)}),
            )
        }
        "tick" => tick(runtime, vec![]),
        "reset_needs" => reset_needs(runtime, actor),
        "action" => {
            let bytes = unhex(input["request"].as_str().ok_or("Missing action")?)?;
            let action = decode_action(&bytes)?;
            let sequence = action.sequence();
            let result = revalidate_action(runtime, support::PRINCIPAL, actor, &bytes)
                .ok()
                .and_then(|action| {
                    tick(
                        runtime,
                        vec![match action {
                            PlayerAction::Invoke(intent) => {
                                AcceptedCommand::QueueInteraction(intent)
                            }
                            PlayerAction::Cancel(intent) => {
                                AcceptedCommand::CancelInteraction(intent)
                            }
                        }],
                    )
                    .ok()
                });
            let accepted_tick = result
                .as_ref()
                .map(|_| runtime.sim().state().completed_tick);
            let receipt = encode_receipt(&ActionReceipt {
                request: bytes,
                accepted_tick,
            })?;
            Ok(
                json!({"transition":result,"receipt":hex(&receipt),"accepted":accepted_tick.is_some(),"sequence":sequence.to_string()}),
            )
        }
        "state" => Ok(
            json!({"tick":runtime.sim().state().completed_tick.to_string(),"hash":hex(&runtime.sim().state_hash()?),"projection":runtime.projection()}),
        ),
        _ => Err("Unknown controlled operation".into()),
    }
}
fn main() -> Result<()> {
    if std::env::var("WONDERLAND_NATIVE_PEER_TEST_ONLY").as_deref() != Ok("1") {
        return Err("This is a test-only peer; explicit opt-in is required".into());
    }
    let (mut runtime, actor) = source()?;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    loop {
        let mut line = String::new();
        let count = reader.by_ref().take(262145).read_line(&mut line)?;
        if count == 0 {
            break;
        }
        if count > 262144 || !line.ends_with('\n') {
            return Err("Controlled command limit".into());
        }
        let input: Value = serde_json::from_str(&line)?;
        let result = match process(&mut runtime, actor, input) {
            Ok(value) => value,
            Err(error) => json!({"error":error.to_string()}),
        };
        serde_json::to_writer(&mut stdout, &result)?;
        writeln!(&mut stdout)?;
        stdout.flush()?;
    }
    Ok(())
}
