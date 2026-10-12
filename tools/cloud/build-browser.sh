#!/usr/bin/env bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
export WONDERLAND_TOOLS_ROOT=${WONDERLAND_TOOLS_ROOT:-"$repo/.cloud-tools"}
mkdir -p "$WONDERLAND_TOOLS_ROOT"
WONDERLAND_TOOLS_ROOT=$(cd "$WONDERLAND_TOOLS_ROOT" && pwd)
export WONDERLAND_TOOLS_ROOT
export CARGO_HOME="$WONDERLAND_TOOLS_ROOT/cargo"
export RUSTUP_HOME="$WONDERLAND_TOOLS_ROOT/rustup"
export XDG_CACHE_HOME="$WONDERLAND_TOOLS_ROOT/cache"
mkdir -p "$XDG_CACHE_HOME"
export PATH="$CARGO_HOME/bin:$PATH"
# Trunk parses NO_COLOR as a boolean, unlike common shell conventions.
if [[ -n "${NO_COLOR:-}" && "$NO_COLOR" != false ]]; then export NO_COLOR=true; fi
cd "$repo/apps/web-shell"
trunk build --release --locked
