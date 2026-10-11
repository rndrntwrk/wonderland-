#!/usr/bin/env bash
set -euo pipefail
B_VERIFY_SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$B_VERIFY_SCRIPT_DIR/verify.py" "$@"
