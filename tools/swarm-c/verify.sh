#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
mkdir -p tools/swarm-c/output
exec > >(tee tools/swarm-c/output/reference.log) 2>&1
if command -v rustup >/dev/null 2>&1; then
  task_cargo=(cargo +1.75.0)
else
  task_cargo=(cargo)
fi
for task_crate in render-core render-iso render-3d avatar-view audio-runtime; do
  "${task_cargo[@]}" fmt --manifest-path "crates/$task_crate/Cargo.toml" --check
  "${task_cargo[@]}" test --manifest-path "crates/$task_crate/Cargo.toml" --locked
done
"${task_cargo[@]}" fmt --manifest-path probes/engine-bakeoff/fixture/Cargo.toml --check
"${task_cargo[@]}" test --manifest-path probes/engine-bakeoff/fixture/Cargo.toml --locked
"${task_cargo[@]}" run --manifest-path probes/engine-bakeoff/fixture/Cargo.toml --locked --bin reference -- tools/swarm-c/output/reference
node --test crates/audio-runtime/browser/*.test.mjs
