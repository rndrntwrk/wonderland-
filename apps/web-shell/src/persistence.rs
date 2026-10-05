//! Only validated acknowledged snapshots cross the local preview storage boundary.
use serde::{Deserialize, Serialize};
use wonderland_contracts::authoring::*;
pub const STORAGE_KEY: &str = "wonderland.authoring.v1";
#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    snapshot: AuthoringProjection,
}
pub fn encode_snapshot(snapshot: &AuthoringProjection) -> Result<String, String> {
    snapshot.validate().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&Envelope {
        version: 1,
        snapshot: snapshot.clone(),
    })
    .map_err(|e| e.to_string())?;
    if json.len() > MAX_AUTHORING_JSON_BYTES {
        return Err("Local preview exceeds the save limit.".into());
    }
    Ok(json)
}
pub fn decode_snapshot(json: &str) -> Result<AuthoringProjection, String> {
    if json.len() > MAX_AUTHORING_JSON_BYTES {
        return Err("Saved preview exceeds the size limit.".into());
    }
    let envelope: Envelope =
        serde_json::from_str(json).map_err(|_| "Saved preview could not be read.".to_string())?;
    if envelope.version != 1 {
        return Err("Saved preview uses an unsupported version.".into());
    }
    envelope.snapshot.validate().map_err(|e| e.to_string())?;
    Ok(envelope.snapshot)
}
#[cfg(target_arch = "wasm32")]
pub fn load() -> Result<Option<AuthoringProjection>, String> {
    let storage = web_sys::window()
        .ok_or("Local saving is unavailable.")?
        .local_storage()
        .map_err(|_| "Local saving is unavailable.")?
        .ok_or("Local saving is unavailable.")?;
    storage
        .get_item(STORAGE_KEY)
        .map_err(|_| "Local saving is unavailable.".into())
        .and_then(|v| v.map(|v| decode_snapshot(&v)).transpose())
}
#[cfg(target_arch = "wasm32")]
pub fn save(snapshot: &AuthoringProjection) -> Result<(), String> {
    let json = encode_snapshot(snapshot)?;
    let storage = web_sys::window()
        .ok_or("Changes cannot be saved on this device.")?
        .local_storage()
        .map_err(|_| "Changes cannot be saved on this device.")?
        .ok_or("Changes cannot be saved on this device.")?;
    storage
        .set_item(STORAGE_KEY, &json)
        .map_err(|_| "Changes cannot be saved on this device.".into())
}
