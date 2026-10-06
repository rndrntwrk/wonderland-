#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
task_output="$task_root/tools/swarm-c/output/audio"
mkdir -p "$task_output"
exec > >(tee "$task_output/verification.log") 2>&1
if command -v rustup >/dev/null 2>&1; then
  task_cargo=(cargo +1.75.0)
else
  task_cargo=(cargo)
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$task_root/crates/audio-runtime/target}"
"${task_cargo[@]}" build --locked --manifest-path crates/audio-runtime/Cargo.toml --bin audio-decode
export AUDIO_DECODER="$CARGO_TARGET_DIR/debug/audio-decode"
command -v mcs
command -v mono
command -v ffplay
python3 -m unittest discover -s tools/swarm-c/audio-cooker/tests -v
python3 tools/swarm-c/audio-cooker/reference_probe.py --source-root "$task_root" --decoder "$AUDIO_DECODER" --report "$task_output/source-differential.json"
