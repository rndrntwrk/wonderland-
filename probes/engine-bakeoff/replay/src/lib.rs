//! Safe Rust exports for executing the same presentation code in ordinary WASM.
// The three globally named FFI symbols below are the only manual ABI surface.
// They contain no unsafe blocks or raw-pointer dereferences.
#![deny(unsafe_op_in_unsafe_fn)]
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use wonderland_engine_fixture::{audio_reference, hash_hex, reference_frame, representative_scene};
use wonderland_render_core::ViewMode;

static OUTPUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

pub fn observe(mode: u32, avatars: u32, tick: u32) -> Result<String, String> {
    let view = match mode {
        0 => ViewMode::Full2D,
        1 => ViewMode::Hybrid2D,
        2 => ViewMode::Full3D,
        _ => return Err("mode".into()),
    };
    let avatars = u16::try_from(avatars).map_err(|_| "avatars")?;
    let scene = representative_scene(view, avatars, u64::from(tick)).map_err(|e| e.to_string())?;
    let frame = reference_frame(&scene).map_err(|e| e.to_string())?;
    let vertices: usize = scene
        .draws
        .iter()
        .map(|draw| draw.mesh.vertices.len())
        .sum();
    let triangles: usize = scene
        .draws
        .iter()
        .map(|draw| draw.mesh.indices.len() / 3)
        .sum();
    let picks = frame.ids.iter().filter(|id| id.is_some()).count();
    let hz = [30, 60, 120][mode as usize];
    let (pcm, starts) = audio_reference(hz).map_err(|e| e.to_string())?;
    let mut audio_hash = Sha256::new();
    for sample in pcm {
        audio_hash.update(sample.to_le_bytes());
    }
    let audio_hash: [u8; 32] = audio_hash.finalize().into();
    Ok(format!(
        r#"{{"mode":{mode},"avatars":{avatars},"tick":{tick},"entities":{},"vertices":{vertices},"triangles":{triangles},"pickablePixels":{picks},"fixtureHash":"{}","referenceHash":"{}","audioStarts":{starts},"audioHash":"{}"}}"#,
        scene.frame.entities.len(),
        hash_hex(scene.hash),
        hash_hex(frame.digest),
        hash_hex(audio_hash)
    ))
}

#[no_mangle]
pub extern "C" fn c_probe_run(mode: u32, avatars: u32, tick: u32) -> u32 {
    let Ok(mut output) = OUTPUT.try_lock() else {
        return 2;
    };
    output.clear();
    match observe(mode, avatars, tick) {
        Ok(record) => {
            output.extend_from_slice(record.as_bytes());
            0
        }
        Err(error) => {
            output.extend_from_slice(error.as_bytes());
            1
        }
    }
}

/// Read-only bytes remain valid until the next c_probe_run. The single-threaded
/// JS harness copies them immediately; no shared memory or host imports are used.
#[no_mangle]
pub extern "C" fn c_probe_ptr() -> usize {
    OUTPUT
        .try_lock()
        .map_or(0, |output| output.as_ptr() as usize)
}
#[no_mangle]
pub extern "C" fn c_probe_len() -> usize {
    OUTPUT.try_lock().map_or(0, |output| output.len())
}
