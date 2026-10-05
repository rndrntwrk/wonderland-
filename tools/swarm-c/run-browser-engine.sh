#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
if [[ -f tools/swarm-c/package-lock.json ]]; then
  npm ci --prefix tools/swarm-c --ignore-scripts
else
  npm install --prefix tools/swarm-c --ignore-scripts
fi
tools/swarm-c/node_modules/.bin/playwright install --with-deps --no-shell chromium
mkdir -p tools/swarm-c/output/browser
cp tools/swarm-c/package-lock.json tools/swarm-c/output/browser/package-lock.json
cargo +1.95.0 run --manifest-path probes/engine-bakeoff/fixture/Cargo.toml --locked --bin reference -- tools/swarm-c/output/reference
node tools/swarm-c/browser-gate.mjs
