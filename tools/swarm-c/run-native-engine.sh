#!/usr/bin/env bash
set -euo pipefail
task_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$task_root"
task_engine="${WONDERLAND_ENGINE:?}"
case "$task_engine" in bevy|fyrox) ;; *) exit 2 ;; esac
task_output="tools/swarm-c/output/$task_engine-native"
mkdir -p "$task_output"
export LIBGL_ALWAYS_SOFTWARE=true
export WGPU_BACKEND=vulkan
export WONDERLAND_PROBE_FRAMES=16
task_binary="${CARGO_TARGET_DIR:-$task_root/engine-target}/debug/$task_engine-gate"
timeout 240s xvfb-run -a "$task_binary" --mode hybrid2d --avatars 64 --tick 30 --frames 16 > "$task_output/native.log" 2>&1
python3 - "$task_output" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
text = (directory / 'native.log').read_text()
records = [json.loads(line.split('WONDERLAND_PROBE ', 1)[1]) for line in text.splitlines() if 'WONDERLAND_PROBE ' in line]
if not records or 'WONDERLAND_PROBE_ERROR' in text:
    raise SystemExit('Native process did not report a successful scene')
last = records[-1]
if last.get('engineErrors') or last.get('authoritativeTicksAdvanced') != 0:
    raise SystemExit('Native adapter reported errors or advanced authoritative time')
report = {'evidence': 'hosted-native-software-runtime', 'processExitedSuccessfully': True,
          'hardwareQualified': False, 'imageParityQualified': False, 'lastState': last}
(directory / 'native-report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
PY
