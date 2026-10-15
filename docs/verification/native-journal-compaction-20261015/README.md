# Lossless checkpoint journal and receipt compaction

Source-only follow-on to the frozen playable vertical candidate `5dcfab147f50d37e113b184f15edd207867e7dec`. Includes the subsequently published strict-lint/wire-layout repair (`e7d5efc6`, cherry-picked here as `06160ef9`), plus separately committed native host custody/runner changes (`7c553747`, documentation `eb2ef583`). No deployment, infrastructure, grants, original assets or remote publication was performed.

See [CONTRACT.md](CONTRACT.md) for storage invariants, migration and honest limits.

## Verified in the existing cloud, serially

- `npm --prefix services/native-edge run e2e:compaction`: PASS. Actual workerd SQLite/R2, 4,224 accepted batches in each legacy/V2 lane, over 72 MiB cumulative retained payloads, bounded hot cache, original receipts/outboxes, archive corruption and failed writes/readback, atomic SQL rollback, current-state/control/recovery races, restart migration. [Storage report](storage-report.json).
- The same command's POSIX process-group SIGKILL checks: PASS after durable R2 write before pointer publication, and after SQLite pointer/chunk swap before caller response. Both preserve exact payload/receipt/outbox across restart and retry. [Crash report](crash-report.json).
- `npm --prefix services/native-edge run e2e:host-receipt-integration`: PASS. Production Rust Host through its existing authored owned-boundary fixture, real workerd SQLite/R2, and production private receipt helper. 64 controlled ticks cross two exact export/persist/ack boundaries. Rust/TypeScript hashes, nullable full replies, lost ack plus workerd restart, historical prepare/resolve/lookup, altered identity refusal and no reexecution are exercised together. [Paired report](host-integration-report.json).
- `node services/native-edge/tests/host-receipts.test.mjs` with `--test`: 51 PASS using Node's actual SQLite engine for bounded receipt schema, envelope/payload verification, exact historical identities, readback/SQL failure and corruption vectors. [Test log](host-receipts-unit.log).
- Existing `services/native-edge/tests/durability.mjs`: PASS with freshly built production Rust host/fixture binaries and actual workerd. Includes genuine multi-chunk native tail replay, checkpoint corruption, control guards, competing successors, pending real build/dialog effects, two owner replacements and exact receipts. The expected hot-journal count changes after compaction; retained receipt count is asserted separately. [Native durability report](native-durability-report.json).
- Existing `services/native-edge/tests/owned-runtime.mjs`: PASS for actual AccountAuthority/workerd/production Host owned admission, stale renewal rejection, SIGKILL/restart, replay/takeover, exact historical receipt and post-recovery fence. [Owned-runtime log](owned-runtime.log).
- `npm --prefix services/native-edge run check`: PASS. New worker tests run against the current source bundled by esbuild, not an earlier deployment artifact.
- Focused native host verification performed serially before workerd: 8 Rust host tests PASS, including 4,160 actual controlled tick commits beyond the old 4,096 limit with bounded acknowledged reply retirement. All-target host build, all-target strict Clippy (`-D warnings`), and Rustfmt PASS. Two focused existing NativePlayer `Unknown`/no-replay/recovery tests PASS. Production receipt/action supervisor Node tests: 10 PASS. These are reported by the host-change owner; exact commands are listed below.

Storage-envelope vectors are explicitly synthetic and do not certify native VM behavior. The bounded paired/native regressions exercise the production Host using existing authored content and do not certify original-game assets or complete gameplay.

## Native verification commands

- `cargo test -p wonderland-native-lot-host --bin wonderland-native-lot-host --offline --locked -j1`
- `cargo build -p wonderland-native-lot-host --all-targets --offline --locked -j1`
- `cargo clippy -p wonderland-native-lot-host --all-targets --offline --locked -j1 -- -D warnings`
- Focused game-runtime tests: `native_player::reconnect_retains_unknown_operation_and_never_replays_it` and `checkpoint_avatar_recovery::player_recovery_returns_poses_without_replaying_activity_or_resolving_unknown_action`
- `node --test tools/native-browser/owned-demo/receipts.test.mjs tools/native-browser/owned-demo/actions.test.mjs`

## Integration recommendation

Keep this as a dedicated follow-on source PR based on repaired PR77. It changes durable layout and private supervisor protocol in tandem; coordinator archival/host receipt storage and host/runner retirement must be reviewed and integrated together. Deploying only an old coordinator reader after compaction is unsupported unless archived payloads have first been verified and rehydrated. Original publication remains separate.

Retention remains lossless and storage grows. Provider capacity, full outbox settlement, original-content gameplay and a product retention/GC policy remain outside this increment. The pre-existing crash window before original action correlation has been exported remains `Unknown`, never inferred successful or automatically replayed; see the native host README. No unacknowledged outbox data or known terminal receipt is discarded by this compaction.
