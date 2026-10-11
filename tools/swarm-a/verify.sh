#!/usr/bin/env bash
# Reproduce Swarm A's native gates; --wasm also compares actual runtime replays.
set -euo pipefail

swarm_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
swarm_mode=${1:-native}
if [[ "$swarm_mode" != native && "$swarm_mode" != --wasm ]]; then
    printf '%s\n' 'Usage: tools/swarm-a/verify.sh [--wasm]' >&2
    exit 2
fi
cd -- "$swarm_root"

cargo fmt --manifest-path crates/sim-core/Cargo.toml --all -- --check
cargo fmt --manifest-path tools/swarm-a/replay/Cargo.toml --all -- --check
cargo test --manifest-path crates/sim-core/Cargo.toml --locked
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --release
cargo run --manifest-path crates/sim-core/Cargo.toml --locked --example headless
cargo test --manifest-path tools/swarm-a/replay/Cargo.toml --locked --target-dir tools/swarm-a/replay/target-replay

if ! command -v mcs >/dev/null || ! command -v mono >/dev/null; then
    printf '%s\n' 'The source-reference gate requires mcs and mono.' >&2
    exit 1
fi
swarm_reference_dir=$(mktemp -d)
trap 'rm -rf -- "$swarm_reference_dir"' EXIT
mcs -checked- -out:"$swarm_reference_dir/reference-numeric.exe" tools/swarm-a/reference-numeric.cs
mono "$swarm_reference_dir/reference-numeric.exe"
python3 tools/swarm-a/reference-lifecycle.py
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --test avatar_source_reference -- --ignored

if [[ "$swarm_mode" == --wasm ]]; then
    # Install the matching wasm32-unknown-unknown target before invoking this gate.
    cd tools/swarm-a/replay
    mkdir -p target-replay
    node check.mjs --source-digest > target-replay/source-before.txt
    cargo run --release --locked --target-dir target-replay --bin sim-replay -- --all > target-replay/native-replays.txt
    cargo build --release --locked --lib --target wasm32-unknown-unknown --target-dir target-replay
    node check.mjs target-replay/wasm32-unknown-unknown/release/swarm_a_replay.wasm \
        target-replay/native-replays.txt \
        --source-digest-file target-replay/source-before.txt \
        --write-evidence target-replay/results.json
fi
