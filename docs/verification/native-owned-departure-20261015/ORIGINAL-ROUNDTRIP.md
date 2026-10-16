# Original source owned departure and re-entry

The fresh private v5 witness passed against the shipping sealed loader, native host and actual account/coordinator HTTP path. The sanitized identities and evidence digests are in `ORIGINAL-ROUNDTRIP.json`; original content, raw checkpoints, account tokens and full private receipts are not included here.

## Observed and asserted

- A source-initialized original mailbox placed two genuine account avatars at `(24,24,1)` and `(24,56,1)`. Both exact actor/control placements were in world; enter and reconnect returned ready with distinct ownership.
- Authenticated leave fenced the current control. The original leave action captured portable state at tick7 and detached through source mode25 at tick33. The removal reason was `source_leave`, not forced cleanup.
- The coordinator committed the immutable first-leave save and the account authority applied all 24 whitelisted columns. A real host takeover occurred after that save and before removal. The subsequent detach receipt and account release matched the same exact identity and save digest.
- The remaining peer's installed identity stayed unchanged. An exact leave retry returned the retained outcome without changing the immutable receipts.
- The same account/avatar re-entered under a new admission operation, principal3 and control generation3. Local object2 was reused only with generation2 and was again in world at `(24,24,1)`.
- The actual accepted re-entry command's portable data matched all 24 saved columns. Native post-startup state matched all 23 non-motive columns. Motives also matched in this run, but the harness does not assume time-varying motives must stay frozen. Saved changed relationships are checked when present.

The departure loop progressed without live player sockets. This does not by itself qualify automatic idle-expiry timing or a rendered browser disconnect/reconnect journey.

## Retained notifications and support boundaries

The account save/release lane was fully acknowledged and the separate departure outbox was empty. Two ordinary supervisor-event rows remained retained: the tick7 EOD-disconnect-request/capture notifications and the tick33 detach notification. This proof does not claim those generic notifications were delivered. Fresh known-clear EOD state was retained; no actual plugin/session teardown acknowledgement was invented.

Legacy unknown EOD history, real plugin associations, unsupported asynchronous effects and source faults retain their documented fail-closed boundaries. The earlier private mode40-blocked snapshot was not rewritten or synthetically reconciled. The exact source synchronous `TestStackObject` implementation was qualified in a fresh run.

The current runner also contains a separately reviewed optional authored-object initialization hook. This manifest omitted that hook, and this witness did not execute it. Shared banner interaction, complete avatar rendering and the full rendered two-account journey remain separate gates.

## Reproduction inputs

The generic authored-lot exporter is retained as `tools/replay/native_authored_lot_genesis.rs` (SHA256 `f6f488ce7f26b104f6f6ffda39b75d0081f4e20fb593665e8eeef5587ebbace1`), copied byte-for-byte from the already reviewed source used for schema6 genesis. `native_authored_join_anchor.rs` then installs the independently pinned original mailbox through its source lifecycle. Operator settings, complete original release and raw witness remain in the approved private workspace; they are not public fixture material.
