#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
task_output="$task_root/tools/swarm-c/output/native-wasm"
mkdir -p "$task_output"
exec > >(tee "$task_output/verification.log") 2>&1
if command -v rustup >/dev/null 2>&1; then
  task_cargo=(cargo +1.75.0)
else
  task_cargo=(cargo)
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$task_root/probes/engine-bakeoff/replay/target}"
task_manifest=probes/engine-bakeoff/replay/Cargo.toml
"${task_cargo[@]}" run --release --locked --manifest-path "$task_manifest" > "$task_output/native.jsonl"
"${task_cargo[@]}" build --release --locked --lib --target wasm32-unknown-unknown --manifest-path "$task_manifest"
node tools/swarm-c/compare-wasm.mjs "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/wonderland_presentation_replay.wasm" "$task_output/native.jsonl" "$task_output/report.json"
