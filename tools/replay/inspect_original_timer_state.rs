//! Read-only private evidence from a trusted native snapshot; no state mutation.
use bincode::Options;
use sha2::{Digest, Sha256};
use std::io::Read;
use wonderland_game_runtime::sim_core::state::SimState;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let input = input.trim();
    if input.len() > 16 * 1024 * 1024 || input.len() % 2 != 0 {
        return Err("checkpoint bound".into());
    }
    let bytes: Vec<u8> = input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    if bytes.len() < 140 || &bytes[..8] != b"WLDSNAP\0" {
        return Err("snapshot envelope".into());
    }
    let end = bytes.len() - 32;
    if &Sha256::digest(&bytes[..end])[..] != &bytes[end..] {
        return Err("snapshot integrity".into());
    }
    let state: SimState = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .with_limit(8 * 1024 * 1024)
        .reject_trailing_bytes()
        .deserialize(&bytes[108..end])?;
    let objects:Vec<_>=state.entities.values().filter(|entity|matches!(entity.info.guid,0x39D04C9C|0x1d95c9b0)).map(|entity|serde_json::json!({"entity":entity.info.reference,"guid":entity.info.guid,"attributes":entity.attributes,"graphic":entity.object_data[0],"object_flags":entity.object_data[8],"disabled_flags":entity.disabled_flags,"position":entity.info.position,"persistent_id":entity.info.persistent_id,"owner":state.world.object(entity.info.reference).and_then(|object|object.owner).map(|owner|owner.0)})).collect();
    println!(
        "{}",
        serde_json::json!({"schema":state.schema,"tick":state.completed_tick,"objects":objects,"lot_permissions":state.lot_permissions,"owned_actors":state.owned_admission.actors(),"eod_known_clear":state.owned_departures.eod_known_clear,"eod_next_request_id":state.eod.next_request_id,"eod_source_threads":state.eod.connections.keys().filter_map(|entity|state.threads.get(&entity.object_id)).collect::<Vec<_>>(),"eod_sessions":state.eod.sessions.values().collect::<Vec<_>>(),"eod_delivered":state.eod.delivered.values().collect::<Vec<_>>()})
    );
    Ok(())
}
