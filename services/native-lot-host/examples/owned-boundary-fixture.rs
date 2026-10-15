//! AUTHORED TEST HARNESS ONLY. Not a sealed/original-content production loader,
//! not a playable demo, and never selected by the browser runner. It exercises
//! the unchanged production Host with real account exports over a private pipe.
#[path = "../src/bounded.rs"]
mod bounded;
#[path = "../src/events.rs"]
mod events;
#[path = "../src/player.rs"]
mod player;
#[path = "../src/runtime.rs"]
mod runtime;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Read, Write},
};
use wonderland_content_ir::manifest::Digest;
use wonderland_game_runtime::{
    AcceptedCommand, GameRuntime, LotModel, RuntimeConfig, RuntimeRole, VmMode,
    live_wire::{Packet, decode_packet},
    sim_core::{
        avatars::{motives::TsoMotiveTuning, state::TEMPLATE_PERSON},
        lot_permissions::LotPermissionState,
        owned_admission::SourceJoinMetadata,
        state::{ContentSet, ObjectDefinition, TuningSet},
        vm::RoutineStore,
    },
};
use wonderland_native_host_contracts::{
    DecimalId, HostBindingV1, RecoveryGrantV1, RequestV1, TakeoverMarkerV1,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn run() -> Result<()> {
    let content = ContentSet::new(
        RoutineStore::new(),
        vec![ObjectDefinition::new(TEMPLATE_PERSON, 0)],
        vec![],
        TuningSet {
            tso_motives: Some(TsoMotiveTuning {
                flat_sim: [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
                category_weights: [[1000; 7]; 11],
            }),
            ..Default::default()
        },
    )?;
    let mut config = RuntimeConfig::new(VmMode::Tso, 41, 7, 123);
    let mut game = GameRuntime::new(
        content,
        LotModel::new(8, 8, 1).map_err(|e| format!("test lot: {e:?}"))?,
        config.clone(),
        RuntimeRole::Authority,
    )?;
    let source = SourceJoinMetadata {
        source_import_sha256: [1; 32],
        content: game.sim().state().content,
        neighborhood_id: 8,
        lot_location: 16777472,
        avatar_guids: [TEMPLATE_PERSON].into(),
        april_fools_2019: false,
        object_flags: BTreeMap::new(),
        registered_groups: BTreeMap::new(),
    };
    game.advance(vec![
        AcceptedCommand::InitializeLotPermissions {
            lot: LotPermissionState::default(),
        },
        AcceptedCommand::InitializeOwnedAdmission { source },
    ])?;
    let mut binding = HostBindingV1 {
        protocol: 1,
        source_pin: "4c6b3e8f5835b228723caea3c9f683c62f244f73".into(),
        binding_sha256: "a".repeat(64),
        lot_incarnation: DecimalId(1),
    };
    let mut marker = None;
    if let Some(path) = std::env::args_os().nth(1) {
        let bytes = std::fs::read(path)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("test recovery input cap".into());
        }
        let value: Value = serde_json::from_slice(&bytes)?;
        let checkpoint = value["checkpoint_hex"]
            .as_str()
            .ok_or("test checkpoint missing")?;
        // Snapshot can exceed command cap; retain a separate bounded test decoder.
        fn decode(s: &str) -> Result<Vec<u8>> {
            if s.len() > 16 * 1024 * 1024
                || !s.len().is_multiple_of(2)
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("invalid test recovery hex".into());
            }
            s.as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
                .collect()
        }
        game.restore(&decode(checkpoint)?)?;
        if let Some(tails) = value["ticks_hex"].as_array() {
            if tails.len() > 64 {
                return Err("test tail bound".into());
            }
            for tail in tails {
                let packet = decode(tail.as_str().ok_or("test tail string")?)?;
                let Packet::Ticks(frames) = decode_packet(&packet, Default::default())? else {
                    return Err("test tail kind".into());
                };
                for frame in frames {
                    let outcome = game.apply_accepted(&frame.accepted)?;
                    if outcome.state_hash != frame.state_hash {
                        return Err("test tail hash".into());
                    }
                }
            }
        }
        let grant: RecoveryGrantV1 = serde_json::from_value(value["grant"].clone())?;
        let old = runtime::cursor(&game)?;
        if grant.schema_version != 1
            || grant.old_binding != binding
            || grant.old_cursor != old
            || grant.old_checkpoint_sha256 != Digest::of(&game.snapshot()?).as_str()
            || grant.new_binding.binding_sha256 != binding.binding_sha256
            || grant.new_binding.source_pin != binding.source_pin
            || grant.new_binding.protocol != binding.protocol
            || grant.new_binding.lot_incarnation.0
                != binding
                    .lot_incarnation
                    .0
                    .checked_add(1)
                    .ok_or("incarnation overflow")?
            || grant.new_authority_epoch.0
                != old
                    .authority_epoch
                    .0
                    .checked_add(1)
                    .ok_or("epoch overflow")?
        {
            return Err("test recovery grant mismatch".into());
        }
        game.adopt_authority_epoch(grant.new_authority_epoch.0)?;
        config.authority_epoch = grant.new_authority_epoch.0;
        binding = grant.new_binding.clone();
        marker = Some(TakeoverMarkerV1 {
            schema_version: 1,
            grant,
            new_cursor: runtime::cursor(&game)?,
            checkpoint_sha256: Digest::of(&game.snapshot()?).as_str().into(),
        });
    }
    let mut host = runtime::Host::new(
        game,
        config,
        binding,
        json!({"qualification":"authored-account-host-contract-harness-only","original_gameplay_qualified":false}),
        marker,
    )?;
    let mut output = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut output, &host.handle(RequestV1::Inspect)?)?;
    writeln!(output)?;
    output.flush()?;
    let mut input = io::stdin().lock();
    loop {
        let mut line = String::new();
        let read = input.by_ref().take(1024 * 1024 + 1).read_line(&mut line)?;
        if read == 0 {
            break;
        }
        if read > 1024 * 1024 {
            return Err("test request bound".into());
        }
        let response = serde_json::from_str::<RequestV1>(&line)
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
            .and_then(|r| host.handle(r))
            .unwrap_or_else(|e| json!({"kind":"denied","reason":e.to_string()}));
        serde_json::to_writer(&mut output, &response)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("authored owned boundary fixture stopped: {error}");
        std::process::exit(1)
    }
}
