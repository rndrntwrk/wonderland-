# Controlled-mode gate verification

This is the first narrow implementation slice of the reviewed controlled-native
admission contract, not the admission implementation. Base source includes the
separately reviewed native-account interruption fix and the contract's explicit
bounded cleanup-progress/expiry-denial corrections.

Implemented:
- Private new-lot controlled-mode initializer with atomic immutable mode selection.
- Exact decimal control revision, transactional tombstone increments and duplicate
  idempotency. Exhaustion rolls back the tombstone and revision together.
- No V1 commit/checkpoint/recovery mutation for controlled lots. Async checkpoint
  initialization race is checked after hashing and again at durable publication.
- Strict V2 guard-state Rust/TypeScript data contract and 12 shared golden cases.
- Existing read-only CI adds the contract execution; no deployment or write grant.

Qualification:
- Actual workerd SQLite and real native host proposal tests: initialization race,
  no upgrade/downgrade, V1 mutation/recovery denial, concurrent exact tombstones,
  overflow atomicity and persisted process restart.
- Existing real native-host durability/R2 replacement and abrupt SIGKILL suites.
- Existing account/HTTP/Argon2/SQLite/crash suite and four provider interruption tests.
- TypeScript check, Worker build, two Rust contract tests, native host/all-targets
  and runtime-bridge builds, workspace formatting and whitespace checks.
- Twelve Rust/TypeScript accepted/refused cases and exact canonical JSON agreement.

The initial suite failed because initializeControlledV2 did not exist. A separate
adversarial checkpoint race then failed with published=true before the final mode
rechecks were added. Counter-exhaustion testing initially expected an “exhausted”
message; the existing canonical decimal validator correctly reports “invalid decimal
identity”, so the assertion was corrected without changing the guard.

Local strict Clippy could not execute: the installed toolchain reports no clippy
command. The existing hosted strict all-targets Clippy gate remains required.
No fresh full-workspace Rust suite, release/browser build or rendered browser
acceptance is claimed for this guard-only change. Prior account Chromium startup
remains blocked by the executor sandbox. The new mode has no actor admission,
lease installation/expiry behavior, V2 runtime mutation/recovery, gameplay grant,
source detach, public route, deployment or migration. Those cases are not silently
counted passed. The tests never insert a fabricated admitted actor to claim success.
