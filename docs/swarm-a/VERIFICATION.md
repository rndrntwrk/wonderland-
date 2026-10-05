# Swarm A verification record

Verified on **2026-10-05** against the unchanged FreeSO source baseline
`4c6b3e8f5835b228723caea3c9f683c62f244f73`.

The complete simulation package has **351 ordinary tests** plus one explicitly
selected Mono source-reference test. The standalone replay package has five
additional tests. Native/WASM comparisons run the actual Rust runtime and
complete snapshot codec on both targets. They do not execute the full original
C# engine or a deployed durable service.

## Final results

| Gate | Result |
|---|---|
| Complete native package, debug profile | **351 passed, 0 failed**; the one Mono test is deliberately ignored by the ordinary command and was run separately below. |
| Complete native package, optimized profile | **351 passed, 0 failed**; the same one Mono test is deliberately ignored by this ordinary command. |
| Explicit avatar source-reference gate | **1 passed, 0 failed**; original source classes/fragments compiled and executed with Mono. |
| Extracted numeric/runtime C# probe | Completed successfully; **93 observation lines** covering numeric conversion, rounding, division/modulo, square roots, and RNG. |
| Hash-pinned lifecycle C# reference | **All 49 expected TSV lines matched**. |
| Native replay package | **5 passed, 0 failed**, including safe ABI bounds/error replacement and all four semantic scenarios. |
| Native/WASM replay matrix | **16 native cases, 64 WASM executions, 5,840 exact per-tick comparisons, 320 complete snapshot comparisons**. |
| WASM dependencies on its host | **No imports**. No WASI, JS random/clock, rendering callback, or service was supplied to the module. |
| Source stability during replay | The pre-build application digest matched before and after execution. Root independently repeated the complete checker and obtained the same result. |
| Headless example | Tick 30, attribute 0 = 10; 5,194-byte snapshot restored with the identical state hash. |
| Rust formatting, staged whitespace, and documentation links | **Passed** for both Rust packages; shell syntax passed; staged whitespace was clean; **all 38 local document links resolved**. Staged files matched the verified worktree. |

The debug and release figures describe the same 351 tests in two build
profiles, not 702 different tests. Focused reviewer counts also overlap with
the complete package. Their scope and corrections are recorded in
[REVIEW.md](REVIEW.md). The machine-readable summary is
[verification-results.json](verification-results.json).

## Reproduce the gates

The portable entrypoint is:

```sh
tools/swarm-a/verify.sh
```

It runs formatting checks, both package profiles, the headless example, the
native replay tests, and the three source-reference gates. Install Python 3,
Mono's `mcs` and `mono`, and Rust 1.75.0 first. The expanded core commands are:

```sh
cargo fmt --manifest-path crates/sim-core/Cargo.toml --all -- --check
cargo test --manifest-path crates/sim-core/Cargo.toml --locked
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --release
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --test avatar_source_reference -- --ignored
python3 tools/swarm-a/reference-lifecycle.py
cargo run --manifest-path crates/sim-core/Cargo.toml --locked --example headless
cargo test --manifest-path tools/swarm-a/replay/Cargo.toml --locked --target-dir tools/swarm-a/replay/target-replay
```

To add the browser-target comparison, install the matching target and Node:

```sh
rustup target add wasm32-unknown-unknown --toolchain 1.75.0
RUSTUP_TOOLCHAIN=1.75.0 tools/swarm-a/verify.sh --wasm
```

The [replay README](../../tools/swarm-a/replay/README.md) provides separate
build/check commands, the safe exported ABI, and each scenario's assertions.
The checker refuses missing/duplicate native cases, unexpected module imports,
source drift, malformed report lengths, unequal tick records, and unequal raw
snapshot bytes/checksums. Its evidence output includes complete per-case
snapshot identities and final state/RNG values.

## Actual environment and provenance

| Component | Recorded environment |
|---|---|
| Rust/Cargo | 1.75.0, Rust commit `82e1608dfa6e0b5569232559e3d385fea5a93112`; x86_64 Linux; LLVM 17.0.6. |
| Node | v24.19.0, Linux x64. |
| C# reference runtime | Mono compiler/JIT 6.8.0.105, amd64. |
| Dependencies | Pinned lockfiles; local execution used a distro-provided offline source registry. The checked-in lockfiles contain ordinary crates.io source identities/checksums and no local paths. |
| Browser standard library | Matching signed Ubuntu Rust source, compiled into a separate prebuilt `wasm32-unknown-unknown` sysroot. Application builds used that completed sysroot, with no application `build-std` or bootstrap step. |

The public reproduction commands use the ordinary Rust target distribution.
Machine-specific source-replacement configuration, compiler caches, sysroot
files, and test executables are not part of the branch.

Two initial full debug attempts stopped before executing a generated test
binary because its executable mode was missing in the task environment. No
test assertion failed in those attempts. The generated ELF permissions were
restored to ordinary executable mode and the entire suite was rerun to exit 0.
For the final optimized gate, compilation and execution were separated so the
generated artifacts could be checked before the complete test run. This did
not change source, test cases, assertions, dependency versions, or outcomes.

## Replay artifact identities

| Artifact | Identity |
|---|---|
| Application source SHA-256 | `9cd1618636c91b28db0b8a8cfeb8a7ab701122a74e4d6ccb47d22df96fde13e8` |
| WASM bytes | 2,969,275 |
| WASM SHA-256 | `4d0a0da1c130062d937243342207ad5665951c23af7970d7c712a2e743cb8e75` |
| Native record bytes | 2,260,356 |
| Native record SHA-256 | `beb1d7cedc3ee66151ef584726b0f18a624169e65ca71e38351a9fede382fab4` |

The application digest covers both manifests/lockfiles, all Rust source used by
the simulation/replay applications, and the Node checker. Documentation,
evidence timestamps, and the standalone example are excluded because those
files do not participate in either replay application. See
[results.json](../../tools/swarm-a/replay/results.json) for the complete evidence.

### Scenarios

| Scenario | Recorded ticks per seed | Snapshots per seed | Main assertions |
|---|---:|---:|---|
| BHAV stack, Sleep, RNG | 96 | 4 | Nested private/semiglobal/global calls, short attributes and arguments, scheduled sleeps, repeated Main, isolated queries, restored/live replica equivalence. |
| Avatar animation and motives | 181 | 5 | Source f32 frames, normal/hurried/reverse playback, encountered xevt order, synthesized completion events, VM event branches, fractional motive changes and decay. |
| Fenced effect retry | 24 | 5 | Typed pending TransferFunds request, stable operation ID, two authority takeovers, accepted resolution, duplicate suppression, cancellation, recycled local ID and stale-generation protection. No transfer executes in this fixture. |
| Portal route callback | 64, following the complete normalized tick-1 snapshot | 6 | Actual opcode 45, a level-2 route via entry-15 portal, a callback held for three ticks, snapshot/tail replay, five rejected malformed/stale deliveries, and one VM success resume at the exact destination. |

Each case runs with seeds 0, 1, 123, and `0xfedcba9876543210`. The checker uses
two fresh WASM instances per case and two executions per instance. Every
scenario checks live authority, replica, and restored-authority behavior.
Snapshots are compared as complete bytes, including independently recomputed
SHA-256 values; matching one final hash is not the only comparison.

## Limits of this evidence

The C# references compile identified source classes/methods or extracted
expressions with disclosed content/interpreter/storage shims. The numeric
probe's observation count is not a test-count substitute. The lifecycle harness
checks the original source hashes and the complete expected output. The avatar
gate explicitly runs the otherwise ignored source comparison.

The verified WASM module is the headless simulation acceptance harness, not a
complete browser client or a production download-size benchmark. No physical
browser, renderer, account service, transaction ledger, real object-family
corpus, 32-client load test, or complete original-engine differential replay was
qualified by these commands. There is no legacy FSOv decoder in this branch.
Those gates and intentional numeric/scheduler/initialization/routing choices
are enumerated in [COVERAGE.md](COVERAGE.md) and [HANDOFF.md](HANDOFF.md).
