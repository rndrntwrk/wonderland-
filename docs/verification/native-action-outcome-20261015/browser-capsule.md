# Owned browser action-outcome continuity

The browser keeps a memory-only recovery capsule across a verified replacement
native host incarnation. This contains exact pending WLC1 bytes, the original
incarnation, pre-action accepted tick, sequence high-water, prior accepted tick,
lot binding and the full committed OwnedControl. It contains no replica snapshot
or bearer credential. A fresh bootstrap and checkpoint are required before
import. The committed control must still match the same account, session,
admission operation/digests, principal, actor generation and control generation;
source epoch, avatar, packed source location and database lot also stay fixed.
Incarnation must increase and accepted state cannot roll back.

The account provider keeps a separate local entry serial stable only while the
same authenticated entry continues. Logout, leaving, another selection or
provider cleanup discards capsule continuity. A second takeover before the first
checkpoint retains the earlier capsule rather than replacing it with an empty
uninitialized player. Page reload is outside this memory-only continuity scope.

After checkpoint installation, one separate text lookup is sent per connection:
`{type:"native_action_result",lot_incarnation:"<original>",action_hex:"<exact lowercase bytes>"}`.
The authenticated server binds ownership. No WLC1 action is resubmitted. The
lookup does not renew activity or start/refresh a receipt deadline. Only a
correlated WLA1 receipt can settle the pending status; an accepted receipt must
be after the original pre-action tick and covered by accepted recovered state.
No evidence leaves the action Unknown. Explicit dismissal retains sequence
high-water and permits only a newly selected action.

## Executed checks

- `node --test apps/web-shell/scripts/native-owned-account.test.mjs apps/web-shell/scripts/native-socket.test.mjs apps/web-shell/scripts/native-receipt-deadline.test.mjs`: 31 passed.
- `cargo test --locked -p wonderland-game-runtime --test native_owned_recovery --test native_player`: 7 new capsule tests and 20 existing player tests passed.
- `cargo check --locked -p wonderland-web-shell --target wasm32-unknown-unknown`: exit 0, 44.15 seconds. Existing proc-macro-error2 future-incompatibility notice only.
- Rustfmt and `git diff --check`: passed.

Terminal outputs are adjacent `browser-js-tests.log`, `browser-rust-tests.log`
and `browser-wasm-check.log`. The Rust protocol harness uses authored executable
content and checkpoint geometry; it does not qualify original avatar admission,
movement, rendering, leave/save or source-object assets. No complete rendered
original-content two-account journey is claimed by these checks.
