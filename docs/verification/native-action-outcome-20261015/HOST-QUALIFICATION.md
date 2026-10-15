# Host and durable action outcome qualification

Verified 2026-10-15 in the action-outcome worktree based on PR78 (`39983bf3`), using the existing shared Rust target. No original-game content or playable-demo qualification is claimed.

## Production change

The typed player-action proposal contains `action_v1` before its canonical digest is computed: authenticated five-field control identity, exact original WLC1 request and SHA-256, canonical original prepare input hash, sequence, and the two exact WLA1 receipt variants. The coordinator selects and stores the terminal outcome in the same SQLite transaction as the batch/cursor/outbox or controlled denial. Independently retained decisions pin the complete action outcome hash; missing or modified outcome records fail closed. Raw/tick/admission proposal shapes remain compatible and old actions without evidence remain Unknown.

Outcome identity includes authority, lot, original incarnation, exact control, and exact request hash. Rejected command sequences are not a durable VM high water, so sequence alone is never used to reject a changed legitimate future request. Restored accepted VM sequence state still rejects a replay. An accepted WLA1 reports the acceptance tick, not interaction completion.

## Focused native verification

- `cargo test --locked --offline -j1 -p wonderland-native-lot-host --bin wonderland-native-lot-host -- --nocapture`: 11 passed, including 4160 committed ticks with checkpoint receipt retirement and the three new action-evidence tests.
- `cargo build --locked --offline -j1 -p wonderland-native-lot-host --all-targets`: passed.
- `cargo clippy --locked --offline -j1 -p wonderland-native-lot-host --all-targets -- -D warnings`: passed with the matching Clippy/toolchain.
- Targeted rustfmt and `git diff --check`: passed.

The fixture uses a clearly authored two-instruction waiting action and authored placement flags. Normal production admission, offers, action validation, tick execution, recovery, and receipt encoding remain in use. Accepted snapshots are not edited to fabricate action admission.

## Actual process witness

`npm run e2e:action-outcome-runtime` passed using the built Rust `owned-boundary-fixture` plus actual workerd AccountAuthority/LotCoordinator SQLite and R2 persistence.

1. Register one synthetic account, reserve/export its avatar, and commit normal typed admission.
2. Derive and execute a valid authored Invoke, then derive a visible-queue Cancel from the resulting native state.
3. Commit Cancel to the coordinator but do not resolve or export the host reply. Confirm the host still exposes the predecessor and pending proposal; SIGKILL the native host.
4. Restart the persisted workerd harness. Retrieve the exact original WLC1/WLA1 and durable decision; retry the same batch without cursor, journal, or outbox advance.
5. Recover from the checkpoint and one accepted Cancel tail into a successor incarnation. Historical lookup remains exact; a query pretending that old action belongs to the new incarnation remains Unknown.
6. Invoke the actual authenticated runner reconciliation dispatcher with real account revalidation and coordinator state. It emits the exact WLA1 after fencing/revalidation/lookup, without renewing controls or changing VM/cursor/outbox state.
7. Explicitly submit the old WLC1 to the restored native host as an adversarial test. The accepted-sequence high water rejects it without execution. The production reconnect path does not submit it.
8. Prepare a fresh action, revoke the actual account session, drain real fences, and obtain an atomic durable rejection. The real authenticated dispatcher refuses lookup for that revoked session. SIGKILL before rejected host resolve/export and restart; exact committed and rejected evidence remain available through the private coordinator API, with no cursor/outbox change.

See `action-outcome-runtime.json` for exact synthetic request/receipt evidence and `host-source-sha256.txt` for source/binary hashes. Logs contain no session tokens or passwords. Temporary workerd storage is not a deliverable. Security checks can persist required expiry/fence housekeeping; this qualification claims no game execution/replay or lease/activity renewal by reconciliation, not globally write-free authorization checks.
