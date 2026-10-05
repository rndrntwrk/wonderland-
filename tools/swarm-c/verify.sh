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
task_failures=0
task_check() {
  if "$@"; then
    return 0
  else
    task_failures=$((task_failures + 1))
  fi
}
for task_crate in render-core render-iso render-3d avatar-view audio-runtime; do
  task_check "${task_cargo[@]}" fmt --manifest-path "crates/$task_crate/Cargo.toml" --check
  task_check "${task_cargo[@]}" test --manifest-path "crates/$task_crate/Cargo.toml" --locked
done
for task_crate in fixture replay; do
  task_check "${task_cargo[@]}" fmt --manifest-path "probes/engine-bakeoff/$task_crate/Cargo.toml" --check
  task_check "${task_cargo[@]}" test --manifest-path "probes/engine-bakeoff/$task_crate/Cargo.toml" --locked
done
task_check "${task_cargo[@]}" run --manifest-path probes/engine-bakeoff/fixture/Cargo.toml --locked --bin reference -- tools/swarm-c/output/reference
task_check node --test crates/audio-runtime/browser/*.test.mjs
task_check node --test probes/engine-bakeoff/web/host-core.test.mjs
task_check node tools/swarm-c/browser-gate.mjs --self-test
if ((task_failures)); then
  printf '%s checks failed; all independent checks were attempted.\n' "$task_failures"
  exit 1
fi
