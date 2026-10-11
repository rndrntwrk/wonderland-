#!/usr/bin/env bash
set -euo pipefail
native_cpal_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export ALSA_CONFIG_PATH="$native_cpal_dir/alsa-null.conf"
export ALSA_CONFIG_DIR=/usr/share/alsa
cargo run --locked --manifest-path "$native_cpal_dir/Cargo.toml" --example native_smoke
