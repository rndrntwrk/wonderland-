# Actual simulation replay: native Rust and browser WebAssembly

This acceptance harness runs `sim_core::runtime::SimRuntime` on native Rust and
`wasm32-unknown-unknown`. It compares **every accepted tick's state hash**, the
accepted-input hash, RNG state and VM instruction count. It also compares the
complete bytes of post-tick snapshots, checks their lengths and independently
recomputes their SHA-256 hashes in Node.

The fixture has no graphics, browser rendering callbacks, OS clock, host random
source, network service or WASI dependency. Node refuses any WASM host import.
The only host-to-module inputs are a scenario number and two halves of a u64
seed. Each successful call creates new simulation runtimes.

These are synthetic, explicitly supplied BHAV instruction/resource fixtures
executed by the actual interpreter and runtime. They establish cross-target
consistency for the cases below. They do not establish parity with the complete
legacy C# engine, completeness of the FreeSO resource corpus, a supported object
cohort, browser responsiveness, or correctness of a deployed durable-service
adapter. The separate source-extracted Mono references and content-cohort tests
remain necessary.

## Standard reviewer / CI commands

Requirements: Rust 1.75.0 with the browser WASM standard-library target, and a
current Node release supporting the standard `WebAssembly` API. No npm install,
WASM binding generator or WASI runtime is required. Run from the repository root:

```sh
rustup toolchain install 1.75.0 --profile minimal --component rustfmt
rustup target add wasm32-unknown-unknown --toolchain 1.75.0
cd tools/swarm-a/replay
cargo +1.75.0 test --locked --target-dir target-replay
node check.mjs --source-digest > target-replay/source-before.txt
cargo +1.75.0 run --release --locked --target-dir target-replay --bin sim-replay -- --all > target-replay/native-replays.txt
cargo +1.75.0 build --release --locked --lib --target wasm32-unknown-unknown --target-dir target-replay
RUSTUP_TOOLCHAIN=1.75.0 node check.mjs target-replay/wasm32-unknown-unknown/release/swarm_a_replay.wasm target-replay/native-replays.txt --source-digest-file target-replay/source-before.txt --write-evidence target-replay/results.json
```

`cargo test` runs the four semantic replay fixtures and the safe export/bounds
test. The release native CLI then runs all four cases for four seeds:
`0`, `1`, `123` and `18364758544493064720` (`0xfedcba9876543210`). The high-word
seed tests the two-u32 ABI without a JavaScript floating-point conversion of the
combined u64. The fixtures' semantic assertions run in release and WASM too.

The pre-build source digest includes the two manifests/lockfiles, both crates'
Rust source trees and the checker. The checker rejects a source change between
that digest and execution or while replay runs, so concurrent edits cannot be
mistaken for a result from one stable source revision.

The Node checker instantiates two fresh modules per scenario/seed combination
and executes each twice. It replaces the saved result with an invalid-scenario
error between calls, then verifies that the next valid call reproduces the exact
report. The default matrix is 64 WASM scenario executions, 5,840 per-tick
comparisons and 320 complete snapshot comparisons. Native runs contribute
1,460 distinct replay tick records and 80 snapshots across the sixteen cases.

All build products and the potentially large full native record file stay under
the ignored `target-replay/` directory. The nested crate is independent of the
sim-core manifest and uses the portable relative dependency path
`../../../crates/sim-core`. Its checked-in lockfile pins the same dependency
versions as sim-core. The standard commands above use ordinary crates.io
packages and do not depend on the task-local Ubuntu source replacement.

To inspect one native case separately:

```sh
cargo +1.75.0 run --release --locked --target-dir target-replay --bin sim-replay -- --scenario 1 --seed 123
```

Each output line is `SCENARIO DECIMAL_U64_SEED REPORT_HEX`. `check.mjs` deliberately
requires the complete matrix produced by `--all`, so a truncated native run
cannot silently reduce the acceptance coverage.

## Scenarios and asserted behavior

| ID / fixture | Accepted ticks | Actual simulation exercised | Post-tick snapshots |
|---|---:|---|---|
| 0: `bhav-stack-sleep-rng` | 96 | Two scheduled objects execute private main BHAVs, private/semiglobal/global subcalls, short attributes, parameter arguments and three explicit RNG bounds plus Sleep's completion draw. Nested sleeps retain their instruction pointers, arguments and idle-start ticks. Main completion counts must equal 13 per object; eight check-tree queries must leave authoritative state and RNG unchanged. | Tick 2 with three call frames suspended in the semiglobal sleep; tick 5 inside the private caller sleep; tick 47 during repeated main execution; tick 96 final. |
| 1: `avatar-animation-motives` | 181 | Two TS1 avatars run actual AnimateSim and SetMotiveChange primitives. The second has ObjectData WalkStyle 17 set to one. The first normal/hurried frames must have exactly the f32 bits of 1.2/2.4. Multiple events queue in a tick, unsorted source record order is preserved, reverse clips run, missing expected events are synthesized, VM event branches consume them, and Hunger restoration retains its fractional state while six game minutes pass. | Tick 4 with event queues and f32 animation frames; tick 8 with distinct normal/hurried progress; ticks 61 and 121 across natural-decay boundaries; tick 181 final. |
| 2: `effect-fenced-retry` | 24 | A TSO non-avatar BHAV issues the typed TransferFunds request. No transfer executes here. Pending state survives snapshots and two authority takeovers; retries retain the original operation ID, issuing epoch, target and payload. Explicit accepted typed resolutions write TempXL and resume the primitive once. Same-tick and later-tick duplicates are idempotent. Deletion cancels the second request; the replacement local object ID has a new generation and remains unchanged by late delivery. | Tick 2 pending; tick 3 after epoch takeover; tick 8 after resolution while sleeping; tick 15 with another pending request; tick 24 final. |
| 3: `portal-route-callback` | 64, ticks 2–65 | A TS1 avatar's real BHAV opcode 45 selects an exact routing SLOT on level 2. The route walks to an entry 15 portal, waits three accepted ticks for its callback, crosses to the recorded upper-floor exit, reaches the SLOT and resumes its VM success branch once. Wrong route/token, duplicate completion and late callback deliveries reject atomically. | Normalized initial world at tick 1; VM route pending at tick 2; first portal callback at tick 13; callback held through tick 16; accepted portal crossing at tick 17; tick 65 final. |

Each accepted tick is also executed in a live replica and, once a checkpoint
exists, a fresh authority restored from that snapshot. Full state equality,
state hashes, VM instruction counts and runtime events must agree. Replicas
must never dispatch an external effect. Restored authorities must produce the
same new-effect dispatches as the uninterrupted authority. Every thirteenth
tick is delivered twice and must produce an exact no-op on the duplicate.

The forward animation's real records deliberately contain `xevt`s in this
encounter order: `7@0ms`, `101@0ms`, `-3@80ms`, `2@40ms`. Source's unused
`OrderBy` does not reorder them. The normal first frame queues `[7,101]`; the
hurried first frame queues `[7,101,-3,2]`. After completion, the expected-event
count produces codes `3,4`. Each finished clip must therefore have consumed
exactly six codes with sum 114. Reverse clips consume `[7,6,5]`, three codes
with sum 18. A record beyond the end of the clip must never fire.

The external-effect fixture rejects four typed deliveries atomically: a stale
delivery epoch, a cancelled operation after local-ID reuse, a conflicting
duplicate value, and an impossible future committed epoch. It separately
rejects a stale accepted-tick epoch. The cancellation case is specifically
`OperationCancelled`: a terminal cancellation cannot resume any target and
does not need a live-generation lookup. Exactly two operations complete, one
is cancelled, and the surviving object's final attributes must be
`[2, 2, 24000, 2]`. The adapter's authentication, durable commit and ledger
implementation are outside this harness.

The portal fixture starts from a trusted, fully validated normalized world,
using the same initial-state seam as `runtime_routes_source`. Initialization
spawns three entities and installs the two-level portal topology before the
recorded replay begins. Its complete initial snapshot at tick 1 is compared
across targets; every accepted replay tick from 2 through 65 is then recorded.
The Node checker enforces that distinct start boundary and verifies the initial
snapshot's canonical payload hash independently.

The route is started by GoToRoutingSlot in an actual VM frame. Portal entry 15
is supplied as explicit content, and the fixture supplies its result through
`AcceptedCommand::RouteCallback`. The saved route ID, callback token, actor,
portal target, direction and entry/exit coordinates must survive all restores.
Five malformed or stale callback deliveries are rejected without changing any
authority, replica or restored snapshot: wrong token, wrong route ID, two
completions in one tick, a consumed callback and an already finished route.
Exactly one route-finished event and one VM success branch must occur; the
avatar's final position is `(104, 40, level 2)`, with failure count zero. The bounded
grid route's continuation is exercised here; original door/stair BHAV behavior,
resource-cohort coverage and Bézier/frame-trace parity remain separate gates.

## Safe WASM ABI

| Export | Result |
|---|---|
| `replay_run(scenario: u32, seed_lo: u32, seed_hi: u32) -> u32` | Runs a fresh scenario. Zero means success; one means the result contains a UTF-8 error. The seed is `seed_lo \| (seed_hi << 32)` in Rust u64 arithmetic. |
| `replay_len() -> u32` | Number of saved output bytes, initially zero. |
| `replay_byte(index: u32) -> u32` | One byte in 0–255, or 256 for any out-of-range index. |

Result storage is a safe `thread_local! RefCell<Vec<u8>>` in the wrapper. The
ABI returns no raw pointer and reads no caller memory. There are no unsafe
blocks. The crate denies unsafe code except for the three `no_mangle`
attributes, which Rust 1.75 includes in that lint. The library's private fixture
state has no persistent RNG or mutable simulation singleton between calls.

The outer report uses explicit little-endian fixed-width integers and u32 byte
lengths. It starts with `SWARMRP1`, scenario and seed, then includes the full
ordered tick records, snapshots and asserted semantic observations. Snapshots
are the unmodified runtime `WLDSNAP\0` format. Node checks each snapshot's
declared payload length, lot/epoch/tick header, embedded checksum, full-file
SHA-256 and the SHA-256 of its canonical state payload. The latter must equal
the reported state hash. Final snapshot hashes must match the last tick hash.
The report bytes themselves must match native output byte for byte.

Node's evidence JSON includes compiler/runtime versions, module imports, module
size/digest, full native-record-file digest, per-case final state/RNG, a digest
of every ordered tick hash and every snapshot's size/digest. Host timestamps
appear only in this evidence document and never in simulation state or reports.

## Recorded execution and provenance

[results.json](results.json) records the successful run on 2026-10-05. All five
native fixture tests passed, followed by all sixteen release native cases and
the complete browser-target matrix:

```text
PASS: 64 WASM scenario executions, 5840 exact per-tick comparisons, 320 raw snapshot comparisons; no host imports.
```

The recorded build used Linux x86-64, Rust/Cargo 1.75.0 (rustc commit
`82e1608dfa6e0b5569232559e3d385fea5a93112`, LLVM 17.0.6) and Node v24.19.0.
Browser-target std was prebuilt from matching signed Ubuntu Rust source package
`1.75.0+dfsg0ubuntu1-0ubuntu7.4` and linked with LLD 17.0.6 through an isolated
sysroot. Application builds reused that completed std and required no
`RUSTC_BOOTSTRAP` or `build-std`. The source archive's SHA-256 was
`a33de6e0dcaf237f00ee12b1664f61ae5f319431bbff3f7965dc1c404d633687`.
This is provenance for the recorded environment; the portable pinned rustup
commands above are the public reproduction path and require no custom sysroot.

Mono JIT and C# compiler 6.8.0.105 were available for the separate source-extracted
reference fixtures. This replay harness invokes Rust and Node; its evidence
explicitly sets `legacyEngineOracle` to false.

| Recorded artifact | Identity |
|---|---|
| Application source | SHA-256 `9cd1618636c91b28db0b8a8cfeb8a7ab701122a74e4d6ccb47d22df96fde13e8`; unchanged before both builds and after replay. |
| WASM module | 2,969,275 bytes; SHA-256 `4d0a0da1c130062d937243342207ad5665951c23af7970d7c712a2e743cb8e75`; no imports. |
| Full native record file | 2,260,356 bytes; SHA-256 `beb1d7cedc3ee66151ef584726b0f18a624169e65ca71e38351a9fede382fab4`. |

Rebuild both targets after changes to sim-core or fixture content. The recorded
evidence identifies this verified revision; future schema or implementation
changes need their own source digest and replay result.
