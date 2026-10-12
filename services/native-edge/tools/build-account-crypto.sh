#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$ROOT"
cargo build -p wonderland-account-crypto --target wasm32-unknown-unknown --release --locked
BINDGEN="${WASM_BINDGEN:-wasm-bindgen}"
[[ "$("$BINDGEN" --version)" == "wasm-bindgen 0.2.129" ]] || { echo 'wasm-bindgen 0.2.129 is required' >&2; exit 1; }
"$BINDGEN" target/wasm32-unknown-unknown/release/wonderland_account_crypto.wasm --target web --out-dir services/native-edge/generated --out-name account_crypto
