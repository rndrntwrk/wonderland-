# Native action receipt deadlines

Continuation of PR #30 at `0df5bf4c10272e2dc9fee976cf4d2b744e9d7e16`.

## Failure and correction

The browser already bounded connection inactivity. However, every accepted incoming tick restarted that watchdog. If an action decision never arrived while the world kept ticking, the native player remained `Pending` indefinitely and its source action/cancellation controls stayed unavailable.

The socket now owns **two independent deadlines**: its existing connection/handshake/liveness timer and a 15-second deadline for the single successfully sent unresolved action. Ordinary frames, action-completion events, checkpoint recovery, readiness notifications and control requests cannot extend or settle an action's receipt deadline.

A receipt settles only after Rust's actual `NativePlayer::receive_update` returns a validated, correlated `PlayerUpdate::Receipt`. The callback completes native validation and releases its mutable runtime borrow before notifying the JavaScript socket owner. A protocol header, a successful socket write or an observed world change is not an acknowledgement.

On receipt expiry, the socket is disposed and emits `receipt-timeout`. The mounted player reports that the result is unknown and calls the existing native disconnect path. It preserves the pending operation and sequence high-water for reconnect. It does not resend the command, mark it rejected, or mistake elapsed time for action completion. Normal source outcome/history feedback remains distinct from receipt acceptance.

Same-player reconnect requires a fresh admission ticket and checkpoint. The user may explicitly dismiss an unknown decision without retrying, then separately choose a new action. This correction does not add a durable server operation journal or guarantee recovery after closing the browser.

## Ownership and bounds

Only one unresolved `send()` may own a receipt timer. The host rejects an additional send rather than replacing the first timer. Failed, empty, oversized and backpressured writes do not allocate an action deadline. Protocol-control writes neither start nor clear it.

Both deadline families have independent opaque owners. Clearing or rearming invalidates callbacks already queued for an older timer, including an older action's deadline after another action begins. Disposal invalidates both owners, clears both timer handles, removes socket callbacks and closes once. No new unbounded queue, saved payload, external dependency or game-state mutation is introduced.

Browser timers can be delayed while a tab is suspended or throttled. Fifteen seconds is the scheduling deadline, not a promise of real-time execution in a frozen browser. Expiry is uncertainty, not proof the server failed to execute the command. Existing Origin/ticket checks and frame/write/backpressure limits remain in force; browser receive checks still do not replace server pre-buffer limits.

## Regression evidence

Tests were committed before implementation. The initial `Native socket` run **37575726865**, job **112644077579**, failed eight of twenty tests. Its primary witness kept receiving ticks every second but did not close at the action deadline (`actual 0`, `expected 1`). Another witness admitted a second pending send. The original ten socket tests still passed. Those historical failures are not relabelled as successful checks.

After implementation, run **37575979800** passed all twenty socket tests. An intermediate browser run subsequently failed on Rust formatting, before its application execution; the canonical one-line correction was produced by the pinned formatter. The temporary formatting transfer workflow is removed from the final tree. Permanent verification is read-only and tests tracked source without generating call sites.

### Deterministic socket tests

`apps/web-shell/scripts/native-receipt-deadline.test.mjs` adds ten tests for continuing traffic, repeated readiness, one unresolved write, validated settlement, obsolete timer callbacks, disposal, backpressure, control writes and thrown sends. Its virtual clock is only in Node unit tests; no test override changes the production 15-second constant.

```sh
node --test --test-reporter=tap apps/web-shell/scripts/native-*.test.mjs
```

### Actual built-browser witness

`tools/native-browser/verify-receipt-deadline.mjs` drives the compiled Rust/WASM application in a mobile-sized Chromium page through the existing controlled original gateway and real native Rust authority. `peer.mjs` can omit one receipt without disconnecting the socket or omitting its accepted state/events. The runner checks:

1. A normally acknowledged action allows another action.
2. A second action completes and later accepted ticks continue while its receipt remains pending.
3. The unchanged 15-second deadline closes the client socket despite continuing traffic, displays explicit uncertainty and sends no retry.
4. Reconnect retains the unknown decision and terminal history without replaying either.
5. Explicit dismissal sends nothing; a separately selected action is accepted and remains live beyond its deadline.
6. Leaving during another pending action disposes its timers; no delayed notice or socket mutation appears on the city screen.

It checks that the served JavaScript host is byte-identical to tracked source, uses actual browser timers, and captures four unedited images alongside console/error records. Full run results and hashes are emitted by the `Native browser` workflow; the existence of this runner alone is not proof its checks passed.

Run from the repository root after the pinned release build and existing test tools are installed:

```sh
cargo build -p wonderland-browser-gateway --example controlled_replay --locked
cargo build -p wonderland-game-runtime --example native_browser_peer --locked
node tools/native-browser/verify-receipt-deadline.mjs
```

The runner uses loopback ports 19187/19188 and writes under `/tmp/wonderland-native-browser-evidence/receipt-deadline`. It is sequential with, not a replacement for, the existing action/cancellation/reconnect/four-viewport suite. `WONDERLAND_NATIVE_QA_OUTPUT` and `WONDERLAND_NATIVE_QA_DIST` override its evidence and built-application paths.

## Verification boundary

Hosted results must be read for the exact published revision. `verification-summary.json` records the tested Git commit/tree, source and compiled-WASM hashes, native count extraction and both browser reports. Native summaries are not a pass for skipped or failed later gates. The normal workflows retain the complete logs/reports/screenshots.

This fixes a native browser action-liveness defect; it does not complete the game. The test service uses declared 8×8 fixture content around unchanged source routines, not production services or distinct real players. Original avatar/animation/contact integration, complete content/providers, durable economy, full Buy/Build/social/object screens, and original FSOv/VMNet continuation remain separate work. No original source/assets, dependency versions, parent branches, deployment or saved game state are changed.
