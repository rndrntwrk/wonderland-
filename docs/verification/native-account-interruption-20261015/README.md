# Native account interruption hardening

Continuation of staged account entry, based on local `d7a229c021a3225be53746f7568d2348edebb405`, tree `b43102f0ecbf5c5f4732fb78c48539ccdd5857f8`. That tree matches published PR75 commit `6dc9dbe1a67e0d0aede1ad77a54fa307660fcfa3`; ancestry differs. Apply this delta to that published tree rather than replacing its ancestry.

## Changes

- A pending roster refresh keeps the authenticated account panel and last roster visible. Refresh is disabled; Sign out remains available.
- Sign-out immediately erases the roster and cancels the active refresh. Late data cannot restore account state. Repeated sign-out does not cancel the sole server revocation request or incorrectly report it confirmed. Login stays disabled until sign-out settles.
- Cached presentation is cleared when authentication expires or another login starts, preventing prior-account roster reuse.
- Cancellation cleanup handles a rejected response-body cancellation without an unhandled promise rejection.

No new route, login cancellation control, registration, character creation, admission, custody release, gameplay readiness or deployment is introduced.

## Verification

- Four provider regression cases were observed failing before their corresponding fixes, then passed: pending refresh/sign-out, repeated sign-out, cross-account cached roster isolation, aborted response cleanup. These use controlled transport replies and assert actual provider state; they are not rendered browser evidence.
- `npm --prefix services/native-edge run check` and `run build` passed.
- `npm --prefix services/native-edge run e2e:accounts` passed: 13 actual account groups, five denied public routes, abrupt workerd process-loss recovery, HTTP/provider journey, and four interruption regression cases. The added HTTP scenario holds an already received real service response to prove confirmed SQLite session revocation and refusal to restore late roster state.
- `npm --prefix services/native-edge run e2e` passed after building the required native host/fixture/runtime-bridge binaries. The initial attempt failed solely because those binaries were absent from this new worktree. Actual native processes, persisted SQLite/R2, replacement, fencing, durable outboxes and abrupt process-loss checks passed.
- `cargo test -p wonderland-web-shell --lib --locked --offline`: all 41 tests passed.
- `cargo check -p wonderland-web-shell --target wasm32-unknown-unknown --locked --offline` passed.
- Workspace `cargo fmt --all -- --check` and `git diff --check` passed.
- Trunk development build completed; final packaged native-account module is checked byte-for-byte against source. Release-optimized packaging is not claimed.

Tooling required the existing pinned Rust 1.99, wasm-bindgen 0.2.129, and Trunk 0.21.14. Known nonfatal diagnostics: npm's inherited http-proxy configuration warning and upstream proc-macro-error2 future-incompatibility warning.

Rendered Chromium was attempted on the base before implementation but stopped before page assertions: `socket() failed: Operation not permitted`. Supported escalation then failed preparing the sandbox mount (`/root/.codex: Not a directory`). The runner now also asserts pending-refresh account controls at desktop and narrow viewports, but these new rendered assertions have **not run**. No screenshot, visual acceptance, two-browser gameplay or hosted-game completion is claimed.
