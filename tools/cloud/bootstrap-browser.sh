#!/usr/bin/env bash
set -euo pipefail
# Rebuildable cloud setup; all installed tools remain inside the chosen directory.
repo=$(cd "$(dirname "$0")/../.." && pwd)
export WONDERLAND_TOOLS_ROOT=${WONDERLAND_TOOLS_ROOT:-"$repo/.cloud-tools"}
mkdir -p "$WONDERLAND_TOOLS_ROOT"
WONDERLAND_TOOLS_ROOT=$(cd "$WONDERLAND_TOOLS_ROOT" && pwd)
export WONDERLAND_TOOLS_ROOT
export CARGO_HOME="$WONDERLAND_TOOLS_ROOT/cargo"
export RUSTUP_HOME="$WONDERLAND_TOOLS_ROOT/rustup"
export npm_config_cache="$WONDERLAND_TOOLS_ROOT/npm-cache"
export PLAYWRIGHT_BROWSERS_PATH="$WONDERLAND_TOOLS_ROOT/playwright-browsers"
export PATH="$CARGO_HOME/bin:$PATH"
mkdir -p "$WONDERLAND_TOOLS_ROOT"
if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
  curl --fail --location --proto '=https' --tlsv1.2 https://sh.rustup.rs -o "$WONDERLAND_TOOLS_ROOT/rustup-init.sh"
  sh "$WONDERLAND_TOOLS_ROOT/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain none
fi
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy --target wasm32-unknown-unknown
if ! trunk --version 2>/dev/null | grep -qx 'trunk 0.21.14'; then
  cargo +1.99.0 install trunk --version 0.21.14 --locked
fi
npm install --prefix "$WONDERLAND_TOOLS_ROOT/browser" --ignore-scripts --no-audit --no-fund --save-exact playwright@1.62.1 ws@8.18.3
if [[ "${WONDERLAND_INSTALL_BROWSER:-1}" == 1 ]]; then
  node "$WONDERLAND_TOOLS_ROOT/browser/node_modules/playwright/cli.js" install chromium
fi
rustc +1.99.0 --version
trunk --version
printf 'Tool root: %s\n' "$WONDERLAND_TOOLS_ROOT"
