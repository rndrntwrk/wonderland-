# C avatar cooker and source probes

This independent Rust 1.75 package consumes C's normalized avatar sidecar. It
contains no original game assets, byte decoder, provider precedence or gameplay
marker executor. The fixture command generates a small labeled synthetic input.

```sh
cargo run --offline --release --manifest-path tools/swarm-c/avatar-cooker/Cargo.toml -- fixture /tmp/synthetic.wcav
cargo run --offline --release --manifest-path tools/swarm-c/avatar-cooker/Cargo.toml -- validate /tmp/synthetic.wcav
cargo run --offline --release --manifest-path tools/swarm-c/avatar-cooker/Cargo.toml -- crowd
```

Use a configured offline CARGO_HOME and CARGO_TARGET_DIR appropriate to the
checkout. `validate` checks header/digest/limits, normalized coordinate policy,
rig admission, dual-local mesh binding, and active animation channels. It
separately reports the signed-time-property bridge: negative source IDs survive
in the sidecar but cannot be cast into A's unsigned milliseconds.

The crowd command reports a measured CPU workload: 32 and 64 instances, 60
presentation ticks, 14 bones/312 vertices/156 triangles, bounded pose cadence,
shared prepared memory and required endpoint checks. GPU pass time is explicitly
unmeasured. These are synthetic reference instances, not production cohorts.

The Python source probe accepts a **read-only** original source checkout and a
scratch output directory:

```sh
python3 tools/swarm-c/avatar-cooker/source_probe.py /path/to/A /tmp/avatar-source-probe
```

It checks Skeleton.cs/Animator.cs against original baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`, extracts original methods, and compiles
with local Mono/mcs and the checkout's existing Mac MonoGame assembly. Source
excerpts and executable stay in the output directory. The checked result is for
the assembly hash in `source-probe-results.txt`; its version is 0.0.0.0, so it
does not verify the project's MonoGame 3.6.0.1625 NuGet pin.

See `docs/swarm-c/avatar-source-notes.md` for source policy, admission deviations,
source normal math, zero-prefix handling, adapter ownership and open gates.
