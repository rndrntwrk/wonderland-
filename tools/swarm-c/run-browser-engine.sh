#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
node --test probes/engine-bakeoff/web/fixture-server.test.mjs
if [[ -f tools/swarm-c/package-lock.json ]]; then
  npm ci --prefix tools/swarm-c --ignore-scripts
else
  npm install --prefix tools/swarm-c --ignore-scripts
fi
tools/swarm-c/node_modules/.bin/playwright install --with-deps --no-shell chromium
mkdir -p tools/swarm-c/output/browser
cp tools/swarm-c/package-lock.json tools/swarm-c/output/browser/package-lock.json
cargo +1.95.0 run --manifest-path probes/engine-bakeoff/fixture/Cargo.toml --locked --bin reference -- tools/swarm-c/output/reference
WONDERLAND_ENGINE_VARIANT="${WONDERLAND_ENGINE}-${WONDERLAND_BACKEND}" \
  WONDERLAND_REFERENCE_DIR="$task_root/tools/swarm-c/output/reference" \
  WONDERLAND_GPU_PICK_REPORT="$task_root/tools/swarm-c/output/browser/gpu-picker-${WONDERLAND_ENGINE}-${WONDERLAND_BACKEND}.json" \
  WONDERLAND_BUILD_PROFILE="dev,opt${CARGO_PROFILE_DEV_OPT_LEVEL:-0},debug${CARGO_PROFILE_DEV_DEBUG:-0}" \
  node --test probes/engine-bakeoff/web/gpu-picker.browser-test.mjs
node tools/swarm-c/browser-gate.mjs
