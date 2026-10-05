#!/usr/bin/env bash
# Package a previously built WASM binary. Never installs tooling or changes locks.
set -euo pipefail
task_here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
task_variant="${1:?Usage: package.sh variant wasm-path output-directory}"
task_wasm="${2:?Supply the compiled WASM path}"
task_output="${3:?Supply an output directory}"
case "$task_variant" in
  bevy-webgpu|bevy-webgl2) task_engine=bevy ;;
  fyrox-webgl2) task_engine=fyrox ;;
  *) printf '%s\n' 'Unsupported engine/backend variant' >&2; exit 2 ;;
esac
task_lock="$task_here/../$task_engine/Cargo.lock"
task_version="$(python3 - "$task_lock" <<'PY'
import sys,tomllib
with open(sys.argv[1],'rb') as source: lock=tomllib.load(source)
versions={p['version'] for p in lock['package'] if p['name']=='wasm-bindgen'}
if len(versions)!=1: raise SystemExit('Expected one wasm-bindgen crate version in the committed engine lock')
print(next(iter(versions)))
PY
)"
task_actual="$(wasm-bindgen --version)"
if [[ "$task_actual" != "wasm-bindgen $task_version" ]]; then
  printf 'Expected wasm-bindgen %s from %s; observed %s\n' "$task_version" "$task_lock" "$task_actual" >&2
  exit 2
fi
mkdir -p "$task_output/pkg/$task_variant" "$task_output/audio" "$task_output/data"
cp "$task_here/index.html" "$task_here/style.css" "$task_here/host.mjs" "$task_here/host-core.mjs" "$task_output/"
cp "$task_here/../../../crates/audio-runtime/browser/browser-audio.mjs" "$task_output/audio/"
cp "$task_here/data/resources.registry" "$task_output/data/"
wasm-bindgen "$task_wasm" --target web --out-dir "$task_output/pkg/$task_variant" --out-name engine
cp "$task_lock" "$task_output/pkg/$task_variant/Cargo.lock"
python3 - "$task_output/pkg/$task_variant" "$task_variant" "$task_version" <<'PY'
import hashlib,json,pathlib,sys
out=pathlib.Path(sys.argv[1])
manifest={'variant':sys.argv[2],'wasmBindgen':sys.argv[3],'files':{}}
for path in sorted(out.rglob('*')):
    if path.is_file() and path.name!='artifact.json':manifest['files'][str(path.relative_to(out))]=hashlib.sha256(path.read_bytes()).hexdigest()
(out/'artifact.json').write_text(json.dumps(manifest,indent=2)+'\n')
PY
printf 'Packaged %s in %s\n' "$task_variant" "$task_output"
