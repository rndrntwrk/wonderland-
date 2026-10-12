//! Explicit process-test input, never loaded by host startup unless supplied.
#[allow(dead_code)]
#[path = "../src/events.rs"]
mod events;
#[path = "../src/loader.rs"]
mod loader;
use bincode::Options;
use serde_json::json;
use std::{fs, path::Path};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    AcceptedCommand, EntityRef, Facing, LotModel, PersistentId, SpawnSpec, TilePos,
    sim_core::{
        ids::ObjectId,
        vm::{FrameContext, RoutineKey, RoutineScope},
    },
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(&args[2]);
    if args[1] == "prepare" {
        let lot = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&LotModel::new(8, 8, 1).expect("valid fixture lot"))?;
        fs::write(root.join("lot.bin"), &lot)?;
        fs::write(
            root.join("host.json"),
            serde_json::to_vec(
                &json!({"schema_version":1,"release_dir":root,"binding_sha256":Digest::of(&fs::read(root.join("binding.json"))?),"lot_file":root.join("lot.bin"),"lot_sha256":Digest::of(&lot),"mode":"Ts1","lot_id":"11","authority_epoch":"9007199254740993","lot_incarnation":"1","seed":"123","utc_start_dotnet_ticks":"0","effect_namespace":"9007199254740993"}),
            )?,
        )?;
        let actor = EntityRef {
            object_id: ObjectId(1),
            generation: 1,
        };
        let commands = vec![
            vec![AcceptedCommand::Spawn(SpawnSpec {
                guid: 123,
                position: TilePos::new(3, 3, 1).center(),
                facing: Facing::NORTH,
                persistent_id: PersistentId(1),
                avatar: false,
            })],
            vec![AcceptedCommand::StartBehavior {
                entity: actor,
                routine: RoutineKey {
                    scope: RoutineScope::Private(123),
                    id: 4096,
                },
                context: FrameContext::for_entity(actor, 123),
                args: vec![],
                replace: true,
            }],
        ];
        let encoded = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&commands)?;
        fs::write(root.join("commands.txt"), hex(&encoded))?;
        let external = vec![vec![AcceptedCommand::StartBehavior {
            entity: actor,
            routine: RoutineKey {
                scope: RoutineScope::Private(123),
                id: 4097,
            },
            context: FrameContext::for_entity(actor, 123),
            args: vec![],
            replace: true,
        }]];
        fs::write(
            root.join("effect-commands.txt"),
            hex(&bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .serialize(&external)?),
        )?;

        // A genuine accepted stream larger than one durable SQL chunk.
        let large_commands = vec![
            vec![
                AcceptedCommand::StartBehavior {
                    entity: actor,
                    routine: RoutineKey {
                        scope: RoutineScope::Private(123),
                        id: 4096
                    },
                    context: FrameContext::for_entity(actor, 123),
                    args: vec![0; 255],
                    replace: true,
                };
                100
            ];
            3
        ];
        fs::write(
            root.join("large-commands.txt"),
            hex(&bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .serialize(&large_commands)?),
        )?;

        let (mut runtime, _, _, _, _) = loader::load(&root.join("host.json"))?;
        for commands in commands {
            runtime.advance(commands)?;
        }
        use wonderland_game_runtime::sim_core::world::build::{
            BuildAuthority, BuildEdit, BuildIntent,
        };
        let mut authority = BuildAuthority::new(actor, PersistentId(1), 1000);
        authority.can_build = true;
        authority.prices.floor = 7;
        let intent = BuildIntent::new(
            9007199254740993,
            actor,
            runtime.sim().state().world.lot.revision().architecture,
            vec![BuildEdit::SetFloor {
                tile: TilePos::new(2, 2, 1),
                pattern: 1,
            }],
        );
        let preview = runtime.preview_build(&intent, &authority)?;
        fs::write(
            root.join("build-commands.txt"),
            hex(&bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .serialize(&vec![vec![AcceptedCommand::BeginBuild {
                    preview,
                    authority,
                }]])?),
        )?;
    } else if args[1] == "replay" {
        let (mut runtime, _, _, _, _) = loader::load(&root.join("host.json"))?;
        let initial: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("initial.json"))?)?;
        runtime.restore(&bytes(initial["checkpoint_hex"].as_str().unwrap()))?;
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("receipt.json"))?)?;
        let wire = bytes(receipt["ticks_hex"].as_str().unwrap());
        let packet = wonderland_game_runtime::live_wire::decode_packet(&wire, Default::default())?;
        if let wonderland_game_runtime::live_wire::Packet::Ticks(frames) = packet {
            for frame in frames {
                let result = runtime.apply_accepted(&frame.accepted)?;
                assert_eq!(result.state_hash, frame.state_hash);
            }
        } else {
            panic!("expected accepted tick packet");
        }
        assert_eq!(
            hex(&runtime.sim().state_hash()?),
            receipt["cursor"]["state_hash"]
        );
        assert_eq!(
            runtime.sim().state().entities[&ObjectId(1)].attributes,
            [41]
        );
        println!("real cooked behavior and accepted-wire replay matched: attribute 41");
    } else if args[1] == "join-tail" {
        let parts: Vec<String> =
            serde_json::from_slice(&fs::read(root.join("recovery-batches.json"))?)?;
        let mut frames = Vec::new();
        for part in parts {
            match wonderland_game_runtime::live_wire::decode_packet(
                &bytes(&part),
                Default::default(),
            )? {
                wonderland_game_runtime::live_wire::Packet::Ticks(batch) => frames.extend(batch),
                _ => return Err("expected tick batch".into()),
            }
        }
        fs::write(
            root.join("tail.bin"),
            wonderland_game_runtime::live_wire::encode_ticks(&frames, Default::default())?,
        )?;
    } else if args[1] == "takeover" {
        let (before, _, _, _, _) = loader::load(&root.join("recovery-original.json"))?;
        let (after, _, _, _, marker) = loader::load(&root.join("takeover.json"))?;
        let old = before.sim().pending_effects()?;
        let new = after.sim().pending_effects()?;
        assert_eq!(old.len(), 1);
        assert_eq!(new.len(), 1);
        assert_eq!(old[0].request, new[0].request);
        assert_eq!(new[0].request.issued_epoch, 9007199254740993);
        assert_eq!(new[0].dispatch_epoch, 9007199254740994);
        assert_eq!(
            marker.unwrap().grant.old_cursor.authority_epoch.0,
            9007199254740993
        );
        println!("logical effect preserved and dispatched under successor epoch");
    } else if args[1] == "effects" {
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("effect-receipt.json"))?)?;
        let effects: Vec<wonderland_game_runtime::sim_core::effects::EffectDispatch> =
            bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .reject_trailing_bytes()
                .deserialize(&bytes(receipt["effects_hex"].as_str().unwrap()))?;
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].request.issued_epoch, 9007199254740993);
        assert_eq!(effects[0].dispatch_epoch, 9007199254740993);
        println!("actual pending effect keeps exact epochs");
    } else if args[1] == "events" {
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("build-receipt.json"))?)?;
        let events: Vec<events::EventEnvelope> = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .reject_trailing_bytes()
            .deserialize(&bytes(receipt["events_hex"].as_str().unwrap()))?;
        assert_eq!(events.len(), 1);
        if let events::SupervisorEvent::BuildRequested(request) = &events[0].event {
            assert_eq!(request.operation, 9007199254740993);
            assert_eq!(request.cost, 7);
        } else {
            panic!("missing durable build request");
        }
        println!("exact u64 build request retained after commit");
    } else {
        return Err("unknown fixture operation".into());
    }
    Ok(())
}
