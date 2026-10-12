#![forbid(unsafe_code)]
//! Versioned server-only password kernel. No client-supplied cost parameters.
use argon2::{Algorithm, Argon2, Params, Version};
use subtle::ConstantTimeEq;
use wasm_bindgen::prelude::*;

fn derive(password: &[u8], salt: &[u8]) -> Result<[u8; 32], &'static str> {
    if password.is_empty() || password.len() > 1024 || salt.len() != 16 {
        return Err("invalid password input bounds");
    }
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|_| "invalid cost profile")?;
    let mut hash = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password, salt, &mut hash)
        .map_err(|_| "password derivation failed")?;
    Ok(hash)
}

#[wasm_bindgen]
pub fn password_hash(password: &mut [u8], salt: &[u8]) -> Result<Vec<u8>, JsValue> {
    let result = derive(password, salt);
    password.fill(0);
    result.map(|v| v.to_vec()).map_err(JsValue::from_str)
}

#[wasm_bindgen]
pub fn password_matches(
    password: &mut [u8],
    salt: &[u8],
    expected: &[u8],
) -> Result<bool, JsValue> {
    let result = if expected.len() != 32 {
        Err("invalid stored verifier length")
    } else {
        derive(password, salt).map(|actual| bool::from(actual.as_slice().ct_eq(expected)))
    };
    password.fill(0);
    result.map_err(JsValue::from_str)
}
