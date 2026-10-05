#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
task_engine="${WONDERLAND_ENGINE:?Set WONDERLAND_ENGINE to bevy or fyrox}"
task_backend="${WONDERLAND_BACKEND:?Set WONDERLAND_BACKEND to native, webgpu or webgl2}"
case "$task_engine:$task_backend" in
  bevy:native|bevy:webgpu|bevy:webgl2|fyrox:native|fyrox:webgl2) ;;
  *) exit 2 ;;
esac
task_manifest="probes/engine-bakeoff/$task_engine/Cargo.toml"
task_output="tools/swarm-c/output/$task_engine-$task_backend"
mkdir -p "$task_output"
exec > >(tee "$task_output/build.log") 2>&1
rustc +1.95.0 -vV > "$task_output/compiler.txt"
git rev-parse HEAD > "$task_output/source-commit.txt"
# Bootstrap resolution is preserved as an artifact. Once committed, this command
# requires the checked lock and never silently changes the dependency graph.
if [[ -f "${task_manifest%Cargo.toml}Cargo.lock" ]]; then
  task_lock=(--locked)
else
  cargo +1.95.0 generate-lockfile --manifest-path "$task_manifest"
  task_lock=(--locked)
fi
cp "${task_manifest%Cargo.toml}Cargo.lock" "$task_output/Cargo.lock"
task_flags=(--manifest-path "$task_manifest" "${task_lock[@]}" --no-default-features)
if [[ "$task_engine" == bevy ]]; then
  if [[ "$task_backend" == native ]]; then
    task_flags+=(--features x11)
  else
    task_flags+=(--features "$task_backend")
  fi
fi
if [[ "$task_backend" == native ]]; then
  cargo +1.95.0 test "${task_flags[@]}" --lib
  cargo +1.95.0 build "${task_flags[@]}"
else
  cargo +1.95.0 build "${task_flags[@]}" --lib --target wasm32-unknown-unknown
  task_bindgen="$(python3 - "$task_output/Cargo.lock" <<'PY'
import sys, tomllib
with open(sys.argv[1], 'rb') as lock_file:
    lock = tomllib.load(lock_file)
versions = [p['version'] for p in lock['package'] if p['name'] == 'wasm-bindgen']
if len(set(versions)) != 1:
    raise SystemExit('Expected exactly one wasm-bindgen version in engine lock')
print(versions[0])
PY
)"
  cargo +1.95.0 install wasm-bindgen-cli --version "=$task_bindgen" --locked
  task_pkg="probes/engine-bakeoff/web/pkg/$task_engine-$task_backend"
  mkdir -p "$task_pkg"
  wasm-bindgen "${CARGO_TARGET_DIR:-$task_root/engine-target}/wasm32-unknown-unknown/debug/${task_engine}_gate.wasm" --target web --out-dir "$task_pkg" --out-name engine
  mkdir -p probes/engine-bakeoff/web/audio
  cp crates/audio-runtime/browser/browser-audio.mjs probes/engine-bakeoff/web/audio/browser-audio.mjs
  sha256sum "$task_pkg"/* > "$task_output/artifact-sha256.txt"
fi
cargo +1.95.0 tree --manifest-path "$task_manifest" --locked > "$task_output/dependencies.txt"
