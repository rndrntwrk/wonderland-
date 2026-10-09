//! Dedicated browser worker binary. Never mounts Leptos or opens a live session.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = executeFacadeRequest)]
pub fn execute_facade_request(bytes: &[u8]) -> Result<js_sys::Array, JsValue> {
    let output = wonderland_world_view::execute_facade_worker_request(bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let result = js_sys::Array::new();
    // Owned JS allocation, not a view into memory released when Rust returns.
    result.push(&js_sys::Uint8Array::from(output.bytes.as_slice()));
    result.push(&JsValue::from_str(&output.metadata_json));
    result.push(&JsValue::from_str(&output.source_hash));
    Ok(result)
}
fn main() {}
