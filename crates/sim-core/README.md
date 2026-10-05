# Wonderland simulation core

`sim-core` is Swarm A's presentation-independent Rust simulation package for
Wonderland's FreeSO rewrite. The same state transition runs natively and on
`wasm32-unknown-unknown`. It uses a 30 Hz simulation clock, explicit seeded RNG,
ordered state, generation-checked entity references, resumable VM/world work,
and bounded post-tick snapshots.

The package is standalone while Swarm F integrates the shared workspace and
contracts. The source baseline is FreeSO commit
`4c6b3e8f5835b228723caea3c9f683c62f244f73` in `rndrntwrk/wonderland-`.

## Run and verify

Use Rust 1.75.0 or the toolchain subsequently adopted by Swarm F:

```sh
cargo test --manifest-path crates/sim-core/Cargo.toml --locked
cargo test --manifest-path crates/sim-core/Cargo.toml --locked --release
cargo run --manifest-path crates/sim-core/Cargo.toml --locked --example headless
```

The example supplies a small immutable BHAV, creates an object, executes thirty
accepted ticks, takes a snapshot, restores it, and prints the final state hash.
It requires no graphics, assets, service, or network connection. The separate
[replay harness](../../tools/swarm-a/replay/README.md) compares the actual runtime
against its WebAssembly build, including every tick and complete snapshot bytes.

## Core interfaces

| Interface | Purpose |
|---|---|
| `state::ContentSet` | Validated immutable routines, object definitions, strings, SLOT metadata, animation metadata, outfits, and tuning. Its content/tuning hashes bind input and snapshots to these exact definitions. |
| `runtime::SimRuntime` | Own a `SimState`, immutable content, and an authority/replica role. `step` validates an accepted input before publishing one complete tick. |
| `runtime::AcceptedTick` | Ordered, admitted commands with lot, epoch, tick, content, and pre-tick RNG bindings. The application must authenticate and authorize their producer. |
| `runtime::TickOutcome` | Post-tick hash, instruction count, deterministic events, and newly issued public effect requests. A replica never dispatches effects. |
| `vm::VmThread`, `vm::VmHost` | Serializable BHAV execution and explicit provider interfaces. Unsupported providers fail explicitly. |
| `world::WorldState` | Semantic lot/object/portal state, slot reservations, and staged build operations. |
| `world::{RouteContinuation, PlacementContinuation}` | Resumable routing and scripted placement. Runtime routes are retained in `SimState.continuations`. |
| `avatars::AvatarState` | Person data, motives, needs/skills, outfits, head seek, animation timing, autonomy inputs, and lifecycle/social state. |
| `snapshot::{encode,decode,validate_state}` | Canonical bounded snapshot encoding, corruption/graph validation, and atomic restore. |
| `effects::EffectBook` | Stable effect IDs, retries, fencing metadata, typed responses, cancellation, and bounded duplicate retention. |

`next_tick` is a convenience for constructing a correctly bound input. It does
not authenticate a client or grant permission to edit a lot or spend money.
`query_behavior` runs a bounded probe on cloned state/RNG and returns no
executable external requests. Mutation and effect resolution enter through
accepted ticks; rendering and UI callbacks have no authority over VM progress.

## Evidence and integration status

Read the [Swarm A handoff](../../docs/swarm-a/README.md),
[work-package coverage](../../docs/swarm-a/COVERAGE.md), and
[verification record](../../docs/swarm-a/VERIFICATION.md) before integrating.
The primitive registry distinguishes implemented behavior, host adapters,
partial implementations, request boundaries, source no-ops, and unsupported
entries. There is no claim of complete FreeSO content or object-family parity.

The integration gates include normalized legacy resources and FSOv import,
interaction offers/queues and EOD providers, final shared protocol adoption,
authenticated durable services, coordinated build/VM commits, and the full
reference-content/physical-browser qualification. Source deviations, including
the deterministic routing replacement and single-wake scheduler policy, are
listed explicitly in the coverage and source notes.

## Shared-workspace lint maintenance

The Rust 1.99 integration pass applies equivalent optional-value predicates,
iterator/map operations, integer ceiling division, and stable descending-key
sorting to the imported runtime. Test setup uses struct initializers and retains
the declared Rust 1.75 API baseline. Action-string handling matches the optional
buffer directly instead of checking and unwrapping it. These changes preserve
source execution order, snapshot fields, accepted commands, and runtime results.
Narrow documented lint allowances keep the existing inline public enums and
explicit VM-host function parameters; no serialized fields or enum variants
were changed to satisfy a size or argument-count suggestion.

## License

MPL-2.0, consistent with the source repository. This package contains no game
asset pack or licensed-content distribution.
