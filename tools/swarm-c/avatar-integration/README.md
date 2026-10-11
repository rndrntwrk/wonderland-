# Read-only A/C avatar ownership probe

This probe executes real A simulation transitions alongside C's pure pose and
mesh pipeline. A is pinned to published commit
`8a0e251d19e222a0a6833d7408ca629f674e1729`. The run script requires the checkout to
be exactly that commit and never changes it. No unresolved A branch path
appears in a shipping Cargo manifest.

```sh
export SWARM_A_PATH=/path/to/read-only/pinned-A
export CARGO_HOME=/path/to/offline/cargo
export CARGO_TARGET_DIR=/path/to/reusable/target
python3 tools/swarm-c/avatar-integration/run.py /tmp/avatar-a-probe
```

The output directory receives a temporary independent Cargo manifest with
explicit local path dependencies and a prefix extracted from A's existing
`runtime_avatar_integration.rs` fixture helpers. Test content and pose assets
are synthetic; no licensed assets are fetched. The simulation has two ordered
looping layers (including reverse and hurry speed) and frozen carry. Its genuine
A markers execute while C only reads the immutable post-tick fields.

The test compares every state_hash and ordered RuntimeEvent vector over 60
actual A ticks for C absent, 30 Hz, 60 Hz and 120 Hz sampling. It checks A state
hash immediately after each draw and replays every accepted tick to assert zero
additional events/cues and no additional C commit. Each cadence executes two
real A marker cues in this fixture. The pose resource association checks resolved
resource string, num_frames and a labeled synthetic digest; it does not claim
that B's real pose corpus is integrated.

For CI, check out C and a separate authorized A repository at the exact commit
above, set SWARM_A_PATH to the pinned A checkout, then run this script using the
configured offline registry. The script fails if the A commit differs. Checkout
and authentication remain the integration owner's responsibility; this probe
does not install dependencies or change repository state.

Root's engine-bakeoff replay owns native/WASM execution evidence. This probe is
native Rust 1.75 A/C boundary evidence, not a browser or GPU benchmark.
