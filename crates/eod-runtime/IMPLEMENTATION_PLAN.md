# Source Handler Integration Implementation Plan

> **For agentic workers:** Use the approved shared-branch execution and source-handler delegation; root stages and commits. Steps use checkbox tracking.

**Goal:** Wire DanceFloor, Signs, Scoreboard and PermissionDoor through the authenticated native EOD host while preserving timer behavior and private recovery.

**Architecture:** Preserve timer entry points and the scoped inbound protocol. Generalize private instance state, add native controller bindings for DanceFloor and checkpointed typed plugin-data intents for the persisted handlers. Every plugin remains unverified against the original runtime and every production provider remains unimplemented.

**Tech Stack:** Dependency-free Rust 1.90, source C# contracts, Python census generation.

**Spec:** Parent task and approved design messages; original handlers, clients, `VMEODServer`, `EODPersist` and existing host/checkpoint/adversarial tests.

## Global Constraints

- Modify only `crates/eod-runtime/**`, the census generator/outputs and `docs/swarm-b/eod-coverage.md`.
- Root owns branch, staging and commits; no staging or branch changes in this task.
- Cargo target: `/workspace/scratch/378e4c36af7b/swarm-b-eod-next-target`, debug and incremental disabled.
- Preserve nonforgeable authority, scoped sessions, private UI separation, bounds and checkpoint barriers.

## Review Focus

- Controller events must target the native bound DanceFloor invoker and derive the avatar ObjectID from invocation, including after controller teardown/rebind.
- Source numeric/string edge cases must preserve UTF-16 truncation and i16 arithmetic, while malformed lengths are bounded before allocation.
- Persistence retries must retain operation identity after lost responses and restart; conflict must not silently overwrite provider data.
- Restored persisted state must not expose UI or accept writes until provider reconciliation succeeds.
- Queue and record rejection must not consume session sequence or mutate source state.

### Task 1: Source state machines

**Files:** `src/source_plugins.rs` and inline tests, delegated to source audit agent.

**Interfaces:** `Signs`, `Scoreboard`, `PermissionDoor` return typed private UI/object events and optional persistence bytes; opaque bounded private codecs retain loading/initialization/state.

- [x] Write literal source-derived tables and observe failing tests.
- [x] Implement source event/payload, parse, permission and string/arithmetic rules.
- [x] Run standalone source tests and integrate them into crate validation.

### Task 2: Host dispatch and typed persistence

**Files:** `src/host.rs`, `src/persistence.rs`, `src/protocol.rs`, `src/lib.rs`, `tests/source_handlers.rs`.

**Interfaces:** Add `PluginConnectRequest`, `PluginInput`, `connect_plugin`, native DanceFloor controller attach/rebind, `drive_persistence`, explicit conflict abort. Keep timer APIs stable.

- [x] Write actual-host tests and observe missing-feature failures.
- [x] Implement handler enum, controller routing, source event allowlists and typed persistence records.
- [x] Enforce finite controller/participant/timer, payload, total byte and pending reconciliation bounds; preserve atomic queue admission.
- [x] Pass actual-host lifecycle, authority, malformed, backpressure and provider retry tests.

### Task 3: Private recovery

**Files:** `src/checkpoint.rs`, `tests/source_handlers.rs`, existing adversarial tests.

- [x] Add failed-first tests for queued intent dispatch barriers, mixed restore, provider divergence, fresh scoped bindings and source private redaction.
- [x] Retain format-1 timer compatibility; use explicit format-2 handler schemas and bounded record parsing for generalized state.
- [x] Checkpoint immutable write intents before provider dispatch; replay the same key under fresh host fencing after restore and reconcile stored bytes/revisions before rebind.
- [x] Run the full EOD crate suite with all prior timer/security coverage.

### Task 4: Honest generated coverage and handoff

**Files:** `tools/swarm-b/eod-census.py`, generated registry/census/coverage, crate README.

- [x] Mark five source-translated native handlers, preserving separate original-runtime/UI/production-provider unverified status and open leaves.
- [x] Run census generation/check, format/check, full tests and clippy.
- [x] Report exact command outcomes, source quirks, remaining integration gates and changed files for independent root review.

## Execution ledger

- Design approved by root before host changes.
- Ruling: persistence writes need a checkpointed intent before dispatch, and restored sessions need provider reconciliation. A provider with non-durable deduplication cannot satisfy this boundary; no such provider is supplied.
- Ruling: preserve timer-only format 1 and add format 2 for other handlers to retain the existing compatibility/security vectors without encoding new state in timer fields.
- Ruling: Signs pre-initialization writes are explicitly rejected until permission calculation has run; this closes a source initialization race and is documented as a native safety boundary.

- Task 1: complete — source state machines integrated; 26 pure source unit tests pass in the crate. Initial missing-feature tests and the four persistence-correlation helper tests were observed failing before implementation.
- Task 2: complete — all four requested handlers dispatch through NativeHost; 18 actual-host tests cover source behavior, authority, bounds, provider handoff and recovery.
- Task 3: complete — format 1 timer compatibility retained; mixed format 2 includes checkpoint-before-dispatch intents and detached reconciliation. Four host review regressions were observed failing and then passing: handler/intent disagreement, handler/binding disagreement, orphan intent tails, and normal Signs writer permission changes against the provider baseline.
- Task 4: complete — generated census reports five source translations, zero original-runtime verified handlers and zero production-provider verified handlers. The remaining 25 registrations stay rejected.
- Ruling: source BinaryWriter throws on a split UTF-16 surrogate; reject that mutation atomically instead of normalizing to replacement text or preserving a partially changed source state.
- Ruling: persistent handlers allow one native session/write stream per scoped persistent object key. This makes patch order explicit; original-runtime joinable behavior remains an evidence gate.
- Review: focused source review found handler/persistence correlation and canonical-intent gaps; both fixed with failing-first host regressions. Root independent review found normal Signs Write flags must also match the prior provider binding; fixed for both non-owner and owner actors in ordinary Write mode.
- Verification: full crate test suite passed 81 tests (32 unit, 24 adversarial, 6 scope, 18 source-host, 1 compile-fail doc test). Clippy all targets with -D warnings passed after style fixes. Census regeneration/check and cargo fmt --check passed. Final command outcomes are reported to root for staging/commit; no branch, staging or commit operation was performed by this agent.
